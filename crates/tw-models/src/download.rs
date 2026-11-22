use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use tw_core::Millis;
use tw_core::models::{DownloadState, ModelInfo, ModelKind, ModelProgress};
use ureq::Agent;
use ureq::http::HeaderMap;

use crate::{Inventory, ModelsError, Result};

const CHUNK: usize = 256 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
/// Left free on the disk after a download, so finishing one never fills it.
const SPACE_MARGIN: u64 = 100 * 1024 * 1024;

/// One file to fetch into the inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRequest {
    /// Reported back in every `ModelProgress`.
    pub id: String,
    pub url: String,
    pub file: String,
    pub kind: ModelKind,
    pub size_bytes: Option<u64>,
    /// Lowercase hex. Checked before the file is moved into place.
    pub sha256: Option<String>,
}

impl DownloadRequest {
    /// The request for a catalog model; `None` if it has no URL.
    pub fn for_model(model: &ModelInfo) -> Option<Self> {
        Some(Self {
            id: model.id.clone(),
            url: model.url.clone()?,
            file: model.file.clone(),
            kind: model.kind,
            size_bytes: Some(model.size_bytes).filter(|size| *size > 0),
            sha256: model.sha256.clone(),
        })
    }
}

/// One HTTP request the app made, for showing the user exactly what was
/// downloaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestRecord {
    pub url: String,
    /// Body bytes received by this request.
    pub bytes: u64,
    pub started: Millis,
    pub finished: Millis,
    pub outcome: RequestOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestOutcome {
    Completed,
    Cancelled,
    /// The server couldn't continue the earlier partial download, so it is
    /// being fetched again from the start.
    Restarted,
    Failed {
        message: String,
    },
}

type RequestLog = Box<dyn Fn(&RequestRecord) + Send + Sync>;

/// Blocking HTTPS downloader. Holds a connection pool, so keep one around.
pub struct Downloader {
    agent: Agent,
    offline: bool,
    log: Option<RequestLog>,
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new()
    }
}

impl Downloader {
    pub fn new() -> Self {
        let agent = Agent::config_builder()
            .http_status_as_error(false)
            .user_agent(concat!("tiny-wispr/", env!("CARGO_PKG_VERSION")))
            .timeout_resolve(Some(Duration::from_secs(15)))
            .timeout_connect(Some(Duration::from_secs(15)))
            // A stalled connection, not a slow one: the timer restarts on every read.
            .timeout_per_read(Some(Duration::from_secs(30)))
            .build()
            .into();
        Self {
            agent,
            offline: false,
            log: None,
        }
    }

    /// When set, every download fails with `ModelsError::Offline` without
    /// touching the network.
    #[must_use]
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    /// Called once for every HTTP request, after it ends.
    #[must_use]
    pub fn with_request_log(
        mut self,
        log: impl Fn(&RequestRecord) + Send + Sync + 'static,
    ) -> Self {
        self.log = Some(Box::new(log));
        self
    }

    /// Downloads `request` into `inventory` and returns the final path.
    ///
    /// Data goes to `<file>.partial` first, which is resumed with a Range
    /// request next time. When the hash is known it is checked before the
    /// file is moved into place; a mismatch deletes the partial file.
    /// Cancelling keeps the partial file.
    ///
    /// `on_progress` gets `Downloading` at most every 100 ms, then exactly one
    /// of `Done`, `Paused` (cancelled, resumable) or `Failed`.
    pub fn download(
        &self,
        inventory: &Inventory,
        request: &DownloadRequest,
        on_progress: &mut dyn FnMut(ModelProgress),
        cancel: &AtomicBool,
    ) -> Result<PathBuf> {
        let mut progress = Progress {
            id: &request.id,
            sink: on_progress,
            received: 0,
            total: request.size_bytes.unwrap_or(0),
            last: None,
        };
        let result = self.fetch(inventory, request, &mut progress, cancel);
        if result.is_err()
            && let Ok(partial) = inventory.partial_path(request.kind, &request.file)
            && fs::metadata(&partial).is_ok_and(|meta| meta.len() == 0)
        {
            let _ = fs::remove_file(partial);
        }
        progress.finish(match &result {
            Ok(_) => DownloadState::Done,
            Err(ModelsError::Cancelled) => DownloadState::Paused,
            Err(_) => DownloadState::Failed,
        });
        result
    }

    /// Fetches `url` into memory, one request recorded in the request log.
    /// For small files that aren't models: the update manifest and the
    /// update archive. Fails with `TooLarge` once more than `limit` bytes
    /// arrive, or before reading when the server announces more.
    ///
    /// `on_progress` gets the bytes received so far and the announced total.
    pub fn get(
        &self,
        url: &str,
        limit: u64,
        on_progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<Vec<u8>> {
        if self.offline {
            return Err(ModelsError::Offline);
        }
        let started = now();
        let mut body = Vec::new();
        let result = self.get_into(url, limit, &mut body, on_progress);
        if let Some(log) = &self.log {
            log(&RequestRecord {
                url: url.to_owned(),
                bytes: body.len() as u64,
                started,
                finished: now(),
                outcome: match &result {
                    Ok(()) => RequestOutcome::Completed,
                    Err(e) => RequestOutcome::Failed {
                        message: e.to_string(),
                    },
                },
            });
        }
        result.map(|()| body)
    }

    fn get_into(
        &self,
        url: &str,
        limit: u64,
        body: &mut Vec<u8>,
        on_progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<()> {
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| ModelsError::Network(Box::new(e)))?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(ModelsError::Http { status });
        }
        let total = content_length(response.headers());
        if total.is_some_and(|total| total > limit) {
            return Err(ModelsError::TooLarge { limit });
        }
        let mut reader = response.into_body().into_reader();
        let mut buf = vec![0; CHUNK];
        let mut last: Option<Instant> = None;
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(ModelsError::Network(Box::new(e))),
            };
            if body.len() as u64 + n as u64 > limit {
                return Err(ModelsError::TooLarge { limit });
            }
            body.extend_from_slice(&buf[..n]);
            if last.is_none_or(|last| last.elapsed() >= PROGRESS_INTERVAL) {
                last = Some(Instant::now());
                on_progress(body.len() as u64, total);
            }
        }
        if total.is_some_and(|total| total != body.len() as u64) {
            return Err(ModelsError::Network("the connection closed early".into()));
        }
        on_progress(body.len() as u64, total);
        Ok(())
    }

    fn fetch(
        &self,
        inventory: &Inventory,
        request: &DownloadRequest,
        progress: &mut Progress<'_>,
        cancel: &AtomicBool,
    ) -> Result<PathBuf> {
        if self.offline {
            return Err(ModelsError::Offline);
        }
        let destination = inventory.path(request.kind, &request.file)?;
        let partial_path = inventory.partial_path(request.kind, &request.file)?;
        let dir = inventory.dir(request.kind);
        fs::create_dir_all(&dir)?;

        let mut partial = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&partial_path)?;
        let mut received = partial.metadata()?.len();
        if request.size_bytes.is_some_and(|size| received > size) {
            partial.set_len(0)?;
            received = 0;
        }
        if let Some(size) = request.size_bytes {
            check_space(&dir, size - received)?;
        }

        let mut hasher = Sha256::new();
        if received > 0 && request.sha256.is_some() {
            hash_prefix(&mut partial, &mut hasher, cancel)?;
        }
        progress.update(received);

        let mut transfer = Transfer {
            file: &mut partial,
            hasher: &mut hasher,
            received,
            fetched: 0,
        };
        if request.size_bytes != Some(received) {
            while self.request(request, &dir, &mut transfer, progress, cancel)? == Step::Restart {
                transfer.reset()?;
                progress.update(0);
            }
        }
        let received = transfer.received;

        let damaged = match &request.sha256 {
            Some(expected) => !to_hex(&hasher.finalize()).eq_ignore_ascii_case(expected),
            None => request.size_bytes.is_some_and(|size| size != received),
        };
        if damaged {
            drop(partial);
            fs::remove_file(&partial_path)?;
            progress.received = 0;
            return Err(ModelsError::ChecksumMismatch {
                file: request.file.clone(),
            });
        }
        partial.sync_all()?;
        drop(partial);
        fs::rename(&partial_path, &destination)?;
        sync_dir(&dir)?;
        Ok(destination)
    }

    /// One HTTP request, recorded in the request log.
    fn request(
        &self,
        request: &DownloadRequest,
        dir: &Path,
        transfer: &mut Transfer<'_>,
        progress: &mut Progress<'_>,
        cancel: &AtomicBool,
    ) -> Result<Step> {
        let started = now();
        let before = transfer.fetched;
        let result = self.stream(request, dir, transfer, progress, cancel);
        if let Some(log) = &self.log {
            log(&RequestRecord {
                url: request.url.clone(),
                bytes: transfer.fetched - before,
                started,
                finished: now(),
                outcome: match &result {
                    Ok(Step::Done) => RequestOutcome::Completed,
                    Ok(Step::Restart) => RequestOutcome::Restarted,
                    Err(ModelsError::Cancelled) => RequestOutcome::Cancelled,
                    Err(e) => RequestOutcome::Failed {
                        message: e.to_string(),
                    },
                },
            });
        }
        result
    }

    fn stream(
        &self,
        request: &DownloadRequest,
        dir: &Path,
        transfer: &mut Transfer<'_>,
        progress: &mut Progress<'_>,
        cancel: &AtomicBool,
    ) -> Result<Step> {
        let resuming = transfer.received > 0;
        let mut call = self.agent.get(&request.url);
        if resuming {
            call = call.header("Range", format!("bytes={}-", transfer.received));
        }
        let response = call.call().map_err(|e| ModelsError::Network(Box::new(e)))?;
        let status = response.status().as_u16();

        let total = match status {
            206 => match content_range(response.headers()) {
                Some((start, total))
                    if start == transfer.received
                        && (total.is_none()
                            || request.size_bytes.is_none()
                            || total == request.size_bytes) =>
                {
                    total.or(request.size_bytes)
                }
                _ if resuming => return Ok(Step::Restart),
                _ => return Err(ModelsError::Http { status }),
            },
            200 => {
                if resuming {
                    transfer.reset()?;
                    progress.update(0);
                }
                content_length(response.headers()).or(request.size_bytes)
            }
            416 if resuming => return Ok(Step::Restart),
            _ => return Err(ModelsError::Http { status }),
        };
        if let (Some(total), Some(expected)) = (total, request.size_bytes)
            && total != expected
        {
            return Err(ModelsError::UnexpectedFile {
                file: request.file.clone(),
            });
        }
        if let Some(total) = total
            && request.size_bytes.is_none()
        {
            check_space(dir, total.saturating_sub(transfer.received))?;
        }
        progress.total = total.unwrap_or(0);

        let mut body = response.into_body().into_reader();
        let mut buf = vec![0; CHUNK];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(ModelsError::Cancelled);
            }
            let n = match body.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(ModelsError::Network(Box::new(e))),
            };
            transfer.write(&buf[..n])?;
            progress.update(transfer.received);
        }
        if total.is_some_and(|total| total != transfer.received) {
            return Err(ModelsError::Network("the connection closed early".into()));
        }
        Ok(Step::Done)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Step {
    Done,
    Restart,
}

struct Transfer<'a> {
    file: &'a mut File,
    hasher: &'a mut Sha256,
    /// Bytes in the file.
    received: u64,
    /// Bytes from the network, across restarts.
    fetched: u64,
}

impl Transfer<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.file.write_all(bytes)?;
        self.hasher.update(bytes);
        self.received += bytes.len() as u64;
        self.fetched += bytes.len() as u64;
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        *self.hasher = Sha256::new();
        self.received = 0;
        Ok(())
    }
}

struct Progress<'a> {
    id: &'a str,
    sink: &'a mut dyn FnMut(ModelProgress),
    received: u64,
    total: u64,
    last: Option<Instant>,
}

impl Progress<'_> {
    fn update(&mut self, received: u64) {
        self.received = received;
        let due = self
            .last
            .is_none_or(|last| last.elapsed() >= PROGRESS_INTERVAL);
        if due || (self.total > 0 && received == self.total) {
            self.last = Some(Instant::now());
            self.emit(DownloadState::Downloading);
        }
    }

    fn finish(&mut self, state: DownloadState) {
        self.emit(state);
    }

    fn emit(&mut self, state: DownloadState) {
        (self.sink)(ModelProgress {
            id: self.id.to_owned(),
            received: self.received,
            total: self.total,
            state,
        });
    }
}

/// Feeds the bytes already on disk to the hasher, leaving the file positioned
/// at its end for appending.
fn hash_prefix(file: &mut File, hasher: &mut Sha256, cancel: &AtomicBool) -> Result<()> {
    file.seek(SeekFrom::Start(0))?;
    let mut buf = vec![0; CHUNK];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(ModelsError::Cancelled);
        }
        match file.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => hasher.update(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
}

/// Fails with `NotEnoughSpace` if `needed` bytes plus a margin won't fit.
/// Passes when the free space can't be read.
pub(crate) fn check_space(dir: &Path, needed: u64) -> Result<()> {
    if needed == 0 {
        return Ok(());
    }
    match fs4::available_space(dir) {
        Ok(available) if available < needed.saturating_add(SPACE_MARGIN) => {
            Err(ModelsError::NotEnoughSpace { needed, available })
        }
        _ => Ok(()),
    }
}

/// `bytes <start>-<end>/<total or *>` → `(start, total)`.
fn content_range(headers: &HeaderMap) -> Option<(u64, Option<u64>)> {
    let value = headers.get("content-range")?.to_str().ok()?;
    let (range, total) = value.strip_prefix("bytes ")?.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let start: u64 = start.trim().parse().ok()?;
    let end: u64 = end.trim().parse().ok()?;
    let total = match total.trim() {
        "*" => None,
        total => Some(total.parse().ok()?),
    };
    (start <= end && total.is_none_or(|t| end < t)).then_some((start, total))
}

fn content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get("content-length")?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        })
}

fn now() -> Millis {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| {
        Millis::try_from(d.as_millis()).unwrap_or(Millis::MAX)
    })
}

/// Makes the rename durable. Windows has no directory handles to sync.
fn sync_dir(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

#[cfg(test)]
mod tests {
    use ureq::http::HeaderValue;

    use super::*;

    fn headers(name: &'static str, value: &'static str) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert(name, HeaderValue::from_static(value));
        map
    }

    #[test]
    fn parses_content_range() {
        let parse = |v| content_range(&headers("content-range", v));
        assert_eq!(parse("bytes 100-199/200"), Some((100, Some(200))));
        assert_eq!(parse("bytes 0-0/*"), Some((0, None)));
        assert_eq!(parse("bytes 100-199/150"), None);
        assert_eq!(parse("bytes */200"), None);
        assert_eq!(parse("items 0-1/2"), None);
        assert_eq!(content_range(&HeaderMap::new()), None);
    }

    #[test]
    fn hashes_to_lowercase_hex() {
        assert_eq!(
            to_hex(&Sha256::digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn progress_is_throttled_but_reaches_the_end() {
        let mut events = Vec::new();
        let mut sink = |p: ModelProgress| events.push((p.received, p.state));
        let mut progress = Progress {
            id: "x",
            sink: &mut sink,
            received: 0,
            total: 1000,
            last: None,
        };
        for received in (0..=1000).step_by(10) {
            progress.update(received);
        }
        progress.finish(DownloadState::Done);
        assert_eq!(
            events,
            [
                (0, DownloadState::Downloading),
                (1000, DownloadState::Downloading),
                (1000, DownloadState::Done)
            ]
        );
    }

    #[test]
    fn space_check_refuses_impossible_downloads() {
        let dir = std::env::temp_dir();
        assert!(check_space(&dir, 0).is_ok());
        assert!(check_space(&dir, 1024).is_ok());
        assert!(matches!(
            check_space(&dir, u64::MAX / 2),
            Err(ModelsError::NotEnoughSpace { .. })
        ));
    }
}
