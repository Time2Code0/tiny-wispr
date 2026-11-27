//! Downloads against a local HTTP server. Nothing here touches the network.

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tw_models::{
    DownloadRequest, DownloadState, Downloader, Inventory, ModelKind, ModelProgress, ModelsError,
    RequestOutcome, RequestRecord,
};

#[derive(Clone)]
enum Route {
    File {
        body: Arc<Vec<u8>>,
        ranges: bool,
        chunk_delay: Duration,
    },
    Redirect(&'static str),
}

/// What the server was asked for: path and `Range` header.
type Seen = (String, Option<String>);

struct Server {
    addr: SocketAddr,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Server {
    fn start(routes: Vec<(&'static str, Route)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let routes: Arc<HashMap<_, _>> = Arc::new(routes.into_iter().collect());
        let log = Arc::clone(&seen);
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let routes = Arc::clone(&routes);
                let log = Arc::clone(&log);
                thread::spawn(move || handle(stream, &routes, &log));
            }
        });
        Self { addr, seen }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

fn handle(mut stream: TcpStream, routes: &HashMap<&'static str, Route>, log: &Mutex<Vec<Seen>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let path = line.split_whitespace().nth(1).unwrap_or("").to_owned();
    let mut range = None;
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("range")
        {
            range = Some(value.trim().to_owned());
        }
    }
    log.lock().unwrap().push((path.clone(), range.clone()));

    let respond = |stream: &mut TcpStream, head: String| {
        let _ = stream.write_all(format!("{head}Connection: close\r\n\r\n").as_bytes());
    };
    match routes.get(path.as_str()) {
        None => respond(
            &mut stream,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n".into(),
        ),
        Some(Route::Redirect(to)) => respond(
            &mut stream,
            format!("HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\n"),
        ),
        Some(Route::File {
            body,
            ranges,
            chunk_delay,
        }) => {
            let len = body.len();
            let start = range.as_deref().filter(|_| *ranges).and_then(|r| {
                r.strip_prefix("bytes=")?
                    .strip_suffix('-')?
                    .parse::<usize>()
                    .ok()
            });
            let start = match start {
                Some(start) if start >= len => {
                    let head = format!(
                        "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{len}\r\nContent-Length: 0\r\n"
                    );
                    return respond(&mut stream, head);
                }
                Some(start) => {
                    respond(
                        &mut stream,
                        format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{}/{len}\r\nContent-Length: {}\r\n",
                            len - 1,
                            len - start
                        ),
                    );
                    start
                }
                None => {
                    respond(
                        &mut stream,
                        format!("HTTP/1.1 200 OK\r\nContent-Length: {len}\r\n"),
                    );
                    0
                }
            };
            for chunk in body[start..].chunks(16 * 1024) {
                if stream.write_all(chunk).is_err() {
                    return;
                }
                thread::sleep(*chunk_delay);
            }
        }
    }
}

fn data(len: usize) -> Arc<Vec<u8>> {
    Arc::new(
        (0..len)
            .map(|i| u8::try_from(i * 31 % 251).unwrap())
            .collect(),
    )
}

fn file(body: &Arc<Vec<u8>>, ranges: bool) -> Route {
    Route::File {
        body: Arc::clone(body),
        ranges,
        chunk_delay: Duration::ZERO,
    }
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "tw-models-download-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const FILE: &str = "ggml-test.bin";

fn request(url: String, body: &[u8]) -> DownloadRequest {
    DownloadRequest {
        id: "test".to_owned(),
        url,
        file: FILE.to_owned(),
        kind: ModelKind::Voice,
        size_bytes: Some(body.len() as u64),
        sha256: Some(sha256(body)),
    }
}

/// A downloader whose request log is collected into the returned vector.
fn logged() -> (Downloader, Arc<Mutex<Vec<RequestRecord>>>) {
    let records = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&records);
    let downloader =
        Downloader::new().with_request_log(move |r| sink.lock().unwrap().push(r.clone()));
    (downloader, records)
}

struct Run {
    result: Result<PathBuf, ModelsError>,
    progress: Vec<ModelProgress>,
}

fn run(downloader: &Downloader, inventory: &Inventory, request: &DownloadRequest) -> Run {
    let mut progress = Vec::new();
    let result = downloader.download(
        inventory,
        request,
        &mut |p| progress.push(p),
        &AtomicBool::new(false),
    );
    Run { result, progress }
}

fn outcomes(records: &Mutex<Vec<RequestRecord>>) -> Vec<(RequestOutcome, u64)> {
    records
        .lock()
        .unwrap()
        .iter()
        .map(|r| (r.outcome.clone(), r.bytes))
        .collect()
}

fn write_partial(inventory: &Inventory, bytes: &[u8]) -> PathBuf {
    let path = inventory.partial_path(ModelKind::Voice, FILE).unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn downloads_verifies_and_moves_into_place() {
    let body = data(300_000);
    let server = Server::start(vec![("/m", file(&body, true))]);
    let tmp = TempDir::new();
    let inventory = Inventory::new(&tmp.0);
    let (downloader, records) = logged();

    let run = run(&downloader, &inventory, &request(server.url("/m"), &body));

    let path = run.result.unwrap();
    assert_eq!(path, inventory.path(ModelKind::Voice, FILE).unwrap());
    assert_eq!(fs::read(&path).unwrap(), *body);
    assert!(
        !inventory
            .partial_path(ModelKind::Voice, FILE)
            .unwrap()
            .exists()
    );
    let last = run.progress.last().unwrap();
    assert_eq!(
        (last.received, last.total, last.state),
        (300_000, 300_000, DownloadState::Done)
    );
    assert!(
        run.progress[..run.progress.len() - 1]
            .iter()
            .all(|p| p.state == DownloadState::Downloading && p.id == "test")
    );
    assert_eq!(outcomes(&records), [(RequestOutcome::Completed, 300_000)]);
    let record = &records.lock().unwrap()[0];
    assert_eq!(record.url, server.url("/m"));
    assert!(record.started <= record.finished && record.started > 0);
    assert_eq!(server.seen(), [("/m".to_owned(), None)]);
}

#[test]
fn resumes_a_partial_download_with_range() {
    let body = data(300_000);
    let server = Server::start(vec![("/m", file(&body, true))]);
    let tmp = TempDir::new();
    let inventory = Inventory::new(&tmp.0);
    write_partial(&inventory, &body[..120_000]);
    let (downloader, records) = logged();

    let run = run(&downloader, &inventory, &request(server.url("/m"), &body));

    assert_eq!(fs::read(run.result.unwrap()).unwrap(), *body);
    assert_eq!(
        server.seen(),
        [("/m".to_owned(), Some("bytes=120000-".to_owned()))]
    );
    assert_eq!(outcomes(&records), [(RequestOutcome::Completed, 180_000)]);
    assert_eq!(run.progress[0].received, 120_000);
}

#[test]
fn restarts_when_the_server_ignores_range() {
    let body = data(300_000);
    let server = Server::start(vec![("/m", file(&body, false))]);
    let tmp = TempDir::new();
    let inventory = Inventory::new(&tmp.0);
    // Wrong bytes: only a restart from zero produces the right hash.
    write_partial(&inventory, &[0xAA; 100_000]);
    let (downloader, records) = logged();

    let run = run(&downloader, &inventory, &request(server.url("/m"), &body));

    assert_eq!(fs::read(run.result.unwrap()).unwrap(), *body);
    assert_eq!(server.seen()[0].1.as_deref(), Some("bytes=100000-"));
    assert_eq!(outcomes(&records), [(RequestOutcome::Completed, 300_000)]);
}

#[test]
fn a_finished_partial_is_moved_into_place_without_a_request() {
    let body = data(50_000);
    let server = Server::start(vec![("/m", file(&body, true))]);
    let tmp = TempDir::new();
    let inventory = Inventory::new(&tmp.0);
    write_partial(&inventory, &body);
    let (downloader, records) = logged();

    let run = run(&downloader, &inventory, &request(server.url("/m"), &body));

    assert_eq!(fs::read(run.result.unwrap()).unwrap(), *body);
    assert!(server.seen().is_empty());
    assert!(records.lock().unwrap().is_empty());
}

#[test]
fn a_changed_remote_file_restarts_then_fails() {
    let old = data(300_000);
    let new = data(310_000);
    let server = Server::start(vec![("/m", file(&new, true))]);
    let tmp = TempDir::new();
    let inventory = Inventory::new(&tmp.0);
    let partial = write_partial(&inventory, &old[..100_000]);
    let (downloader, records) = logged();

    let run = run(&downloader, &inventory, &request(server.url("/m"), &old));

    assert!(
        matches!(run.result, Err(ModelsError::UnexpectedFile { .. })),
        "{:?}",
        run.result
    );
    assert_eq!(
        server.seen(),
        [
            ("/m".to_owned(), Some("bytes=100000-".to_owned())),
            ("/m".to_owned(), None)
        ]
    );
    let outcomes = outcomes(&records);
    assert_eq!(outcomes[0], (RequestOutcome::Restarted, 0));
    assert!(matches!(outcomes[1].0, RequestOutcome::Failed { .. }));
    assert!(!partial.exists(), "the emptied partial is removed");
    assert_eq!(run.progress.last().unwrap().state, DownloadState::Failed);
}

#[test]
fn checksum_mismatch_deletes_the_partial() {
    let body = data(100_000);
    let server = Server::start(vec![("/m", file(&body, true))]);
    let tmp = TempDir::new();
    let inventory = Inventory::new(&tmp.0);
    let mut request = request(server.url("/m"), &body);
    request.sha256 = Some(sha256(b"something else"));

    let run = run(&Downloader::new(), &inventory, &request);

    let error = run.result.unwrap_err();
    assert!(
        matches!(error, ModelsError::ChecksumMismatch { .. }),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "The download of ggml-test.bin was damaged, so it was deleted. Try again."
    );
    assert!(
        !inventory
            .partial_path(ModelKind::Voice, FILE)
            .unwrap()
            .exists()
    );
    assert!(!inventory.path(ModelKind::Voice, FILE).unwrap().exists());
    let last = run.progress.last().unwrap();
    assert_eq!((last.received, last.state), (0, DownloadState::Failed));
}

#[test]
fn without_a_hash_the_size_is_checked() {
    let body = data(100_000);
    let server = Server::start(vec![("/m", file(&body, true))]);
    let tmp = TempDir::new();
    let inventory = Inventory::new(&tmp.0);
    let mut request = request(server.url("/m"), &body);
    request.sha256 = None;
    request.size_bytes = None;

    let run = run(&Downloader::new(), &inventory, &request);

    assert_eq!(fs::read(run.result.unwrap()).unwrap(), *body);
    assert_eq!(
        run.progress.last().unwrap().total,
        100_000,
        "total comes from Content-Length"
    );
}

#[test]
fn cancelling_keeps_the_partial_for_a_resume() {
    let body = data(2_000_000);
    let server = Server::start(vec![(
        "/m",
        Route::File {
            body: Arc::clone(&body),
            ranges: true,
            chunk_delay: Duration::from_millis(5),
        },
    )]);
    let tmp = TempDir::new();
    let inventory = Inventory::new(&tmp.0);
    let request = request(server.url("/m"), &body);
    let (downloader, records) = logged();

    let cancel = AtomicBool::new(false);
    let mut last = None;
    let result = downloader.download(
        &inventory,
        &request,
        &mut |p| {
            if p.received > 0 {
                cancel.store(true, Ordering::Relaxed);
            }
            last = Some(p);
        },
        &cancel,
    );
    assert!(matches!(result, Err(ModelsError::Cancelled)), "{result:?}");
    let last = last.unwrap();
    assert_eq!(last.state, DownloadState::Paused);
    let partial = inventory.partial_path(ModelKind::Voice, FILE).unwrap();
    let kept = fs::metadata(&partial).unwrap().len();
    assert!(kept > 0 && kept < 2_000_000, "{kept}");
    assert_eq!(last.received, kept);
    assert!(!inventory.path(ModelKind::Voice, FILE).unwrap().exists());

    let run = run(&downloader, &inventory, &request);

    assert_eq!(fs::read(run.result.unwrap()).unwrap(), *body);
    assert_eq!(server.seen()[1].1, Some(format!("bytes={kept}-")));
    assert_eq!(
        outcomes(&records),
        [
            (RequestOutcome::Cancelled, kept),
            (RequestOutcome::Completed, 2_000_000 - kept)
        ]
    );
}

#[test]
fn http_errors_are_reported_and_leave_nothing_behind() {
    let server = Server::start(vec![]);
    let tmp = TempDir::new();
    let inventory = Inventory::new(&tmp.0);
    let (downloader, records) = logged();

    let run = run(
        &downloader,
        &inventory,
        &request(server.url("/missing"), b"x"),
    );

    assert!(
        matches!(run.result, Err(ModelsError::Http { status: 404 })),
        "{:?}",
        run.result
    );
    assert!(
        !inventory
            .partial_path(ModelKind::Voice, FILE)
            .unwrap()
            .exists()
    );
    assert_eq!(
        outcomes(&records),
        [(
            RequestOutcome::Failed {
                message: "The model file is no longer at its address (404)".to_owned()
            },
            0
        )]
    );
    assert_eq!(run.progress.last().unwrap().state, DownloadState::Failed);
}

#[test]
fn follows_redirects_and_resumes_through_them() {
    let body = data(200_000);
    let server = Server::start(vec![
        ("/m", Route::Redirect("/cdn/m")),
        ("/cdn/m", file(&body, true)),
    ]);
    let tmp = TempDir::new();
    let inventory = Inventory::new(&tmp.0);
    write_partial(&inventory, &body[..50_000]);
    let (downloader, records) = logged();

    let run = run(&downloader, &inventory, &request(server.url("/m"), &body));

    assert_eq!(fs::read(run.result.unwrap()).unwrap(), *body);
    let range = Some("bytes=50000-".to_owned());
    assert_eq!(
        server.seen(),
        [
            ("/m".to_owned(), range.clone()),
            ("/cdn/m".to_owned(), range)
        ]
    );
    assert_eq!(outcomes(&records), [(RequestOutcome::Completed, 150_000)]);
}

#[test]
fn unreachable_server_is_a_network_error() {
    let addr = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let tmp = TempDir::new();
    let inventory = Inventory::new(&tmp.0);

    let run = run(
        &Downloader::new(),
        &inventory,
        &request(format!("http://{addr}/m"), b"x"),
    );

    let error = run.result.unwrap_err();
    assert!(matches!(error, ModelsError::Network(_)), "{error:?}");
    assert_eq!(
        error.to_string(),
        "Couldn't reach the download server. Check your internet connection."
    );
}

#[test]
fn refuses_a_download_that_cannot_fit_before_connecting() {
    let server = Server::start(vec![]);
    let tmp = TempDir::new();
    let inventory = Inventory::new(&tmp.0);
    let mut request = request(server.url("/m"), b"x");
    request.size_bytes = Some(u64::MAX / 2);

    let run = run(&Downloader::new(), &inventory, &request);

    assert!(
        matches!(run.result, Err(ModelsError::NotEnoughSpace { .. })),
        "{:?}",
        run.result
    );
    assert!(server.seen().is_empty());
    assert!(
        !inventory
            .partial_path(ModelKind::Voice, FILE)
            .unwrap()
            .exists()
    );
}

#[test]
fn get_fetches_into_memory_and_logs_one_request() {
    let body = data(3_000);
    let server = Server::start(vec![
        ("/latest.json", Route::Redirect("/asset")),
        ("/asset", file(&body, false)),
    ]);
    let (downloader, records) = logged();
    let mut progress = Vec::new();

    let fetched = downloader
        .get(
            &server.url("/latest.json"),
            10_000,
            &mut |received, total| {
                progress.push((received, total));
            },
        )
        .unwrap();

    assert_eq!(fetched, *body);
    assert_eq!(progress.last(), Some(&(3_000, Some(3_000))));
    assert_eq!(outcomes(&records), vec![(RequestOutcome::Completed, 3_000)]);
    assert_eq!(records.lock().unwrap()[0].url, server.url("/latest.json"));
}

#[test]
fn get_refuses_a_body_over_the_limit() {
    let body = data(3_000);
    let server = Server::start(vec![("/big", file(&body, false))]);
    let (downloader, records) = logged();

    let error = downloader
        .get(&server.url("/big"), 1_000, &mut |_, _| ())
        .unwrap_err();

    assert!(
        matches!(error, ModelsError::TooLarge { limit: 1_000 }),
        "{error:?}"
    );
    let logged = outcomes(&records);
    assert!(
        matches!(logged[..], [(RequestOutcome::Failed { .. }, 0)]),
        "{logged:?}"
    );
}

#[test]
fn get_reports_http_errors() {
    let server = Server::start(vec![]);
    let (downloader, records) = logged();

    let error = downloader
        .get(&server.url("/missing"), 1_000, &mut |_, _| ())
        .unwrap_err();
    assert!(
        matches!(error, ModelsError::Http { status: 404 }),
        "{error:?}"
    );
    assert_eq!(records.lock().unwrap().len(), 1);
}
