use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::protocol::{Completion, Done, ErrorKind, Request, Response, WorkerError, WorkerInfo};

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("worker binary not found at {0}")]
    WorkerNotFound(PathBuf),
    #[error("worker exited unexpectedly")]
    WorkerExited,
    #[error("no model loaded")]
    NotLoaded,
    #[error("the worker did not answer in time")]
    Timeout,
    #[error(transparent)]
    Worker(#[from] WorkerError),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl LlmError {
    /// The worker's typed error kind, when the worker reported one.
    pub fn kind(&self) -> Option<ErrorKind> {
        match self {
            Self::Worker(err) => Some(err.kind),
            _ => None,
        }
    }
}

pub type Result<T, E = LlmError> = std::result::Result<T, E>;

/// What [`LlmClient::load`] reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub load_ms: u64,
    pub info: WorkerInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LoadSpec {
    model_path: PathBuf,
    context_length: u32,
}

/// Shared with [`CancelHandle`]s, so a cancel can be written while
/// [`LlmClient::complete`] blocks on the response.
#[derive(Default)]
struct Shared {
    stdin: Mutex<Option<ChildStdin>>,
    /// The id of the running `complete` request, 0 when idle.
    current: AtomicU64,
    /// The last id a cancel was asked for, so a restart doesn't resend it.
    cancelled: AtomicU64,
}

impl Shared {
    fn send(&self, request: &Request) -> Result<()> {
        let mut line =
            serde_json::to_vec(request).map_err(|e| LlmError::Protocol(e.to_string()))?;
        line.push(b'\n');
        let mut stdin = self.stdin.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(stdin) = stdin.as_mut() else {
            return Err(LlmError::WorkerExited);
        };
        stdin.write_all(&line)?;
        stdin.flush()?;
        Ok(())
    }
}

/// Cancels the request currently running on an [`LlmClient`], from any thread.
#[derive(Clone)]
pub struct CancelHandle(Arc<Shared>);

impl CancelHandle {
    /// The running request ends promptly with [`ErrorKind::Cancelled`]. Does
    /// nothing when no request is running.
    pub fn cancel(&self) {
        let id = self.0.current.load(Ordering::SeqCst);
        if id != 0 {
            self.0.cancelled.store(id, Ordering::SeqCst);
            // The worker may be gone; the request then ends with WorkerExited.
            let _ = self.0.send(&Request::Cancel { id });
        }
    }
}

struct Worker {
    child: Child,
    responses: Receiver<Result<Response, String>>,
}

/// Spawns and supervises one `tw-llm-worker` process.
///
/// Blocking: call it from a background thread. A worker that dies is started
/// again on the next request, with the last loaded model; a request it dies
/// in the middle of is retried once.
pub struct LlmClient {
    worker_path: PathBuf,
    worker: Option<Worker>,
    loaded: Option<LoadSpec>,
    shared: Arc<Shared>,
    next_id: u64,
    load_timeout: Duration,
}

const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const CANCEL_GRACE: Duration = Duration::from_secs(2);

impl LlmClient {
    pub fn new(worker_path: impl Into<PathBuf>) -> Self {
        Self {
            worker_path: worker_path.into(),
            worker: None,
            loaded: None,
            shared: Arc::default(),
            next_id: 1,
            load_timeout: Duration::from_secs(120),
        }
    }

    pub fn worker_path(&self) -> &Path {
        &self.worker_path
    }

    pub fn set_load_timeout(&mut self, timeout: Duration) {
        self.load_timeout = timeout;
    }

    pub fn cancel_handle(&self) -> CancelHandle {
        CancelHandle(Arc::clone(&self.shared))
    }

    /// The worker's process id, for diagnostics.
    pub fn worker_pid(&self) -> Option<u32> {
        self.worker.as_ref().map(|w| w.child.id())
    }

    pub fn is_running(&mut self) -> bool {
        match self.worker.as_mut().map(|w| w.child.try_wait()) {
            Some(Ok(None)) => true,
            Some(_) => {
                self.drop_worker();
                false
            }
            None => false,
        }
    }

    /// The model the next request runs on, if any. It stays set across
    /// [`Self::shutdown`], which only ends the process.
    pub fn model_path(&self) -> Option<&Path> {
        self.loaded.as_ref().map(|spec| spec.model_path.as_path())
    }

    pub fn context_length(&self) -> Option<u32> {
        self.loaded.as_ref().map(|spec| spec.context_length)
    }

    /// Starts the worker if needed and loads `model_path`, replacing any
    /// model it had.
    pub fn load(&mut self, model_path: &Path, context_length: u32) -> Result<Loaded> {
        let spec = LoadSpec {
            model_path: model_path.to_path_buf(),
            context_length,
        };
        let loaded = self.load_spec(&spec);
        self.loaded = loaded.as_ref().ok().map(|_| spec);
        loaded
    }

    fn load_spec(&mut self, spec: &LoadSpec) -> Result<Loaded> {
        self.ensure_spawned()?;
        self.shared.send(&Request::Load {
            model_path: spec.model_path.clone(),
            context_length: spec.context_length,
            gpu_layers: None,
        })?;
        match self.recv_until(Instant::now() + self.load_timeout)? {
            Response::Loaded { load_ms, info, .. } => Ok(Loaded { load_ms, info }),
            Response::Error(err) => Err(err.into()),
            other => Err(unexpected(&other)),
        }
    }

    /// Runs one completion; `completion.id` is assigned here. `on_token`
    /// receives text as it is generated when `completion.stream` is set.
    pub fn complete(
        &mut self,
        mut completion: Completion,
        timeout: Duration,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<Done> {
        let spec = self.loaded.clone().ok_or(LlmError::NotLoaded)?;
        if !self.is_running() {
            self.load_spec(&spec)?;
        }
        completion.id = self.next_id;
        self.next_id += 1;
        let id = completion.id;
        self.shared.current.store(id, Ordering::SeqCst);
        let request = Request::Complete(completion);
        let mut result = self.run(&request, id, timeout, on_token);
        if matches!(result, Err(LlmError::WorkerExited))
            && self.shared.cancelled.load(Ordering::SeqCst) != id
        {
            result = self
                .load_spec(&spec)
                .and_then(|_| self.run(&request, id, timeout, on_token));
        }
        self.shared.current.store(0, Ordering::SeqCst);
        result
    }

    fn run(
        &mut self,
        request: &Request,
        id: u64,
        timeout: Duration,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<Done> {
        self.shared.send(request)?;
        let deadline = Instant::now() + timeout;
        loop {
            match self.recv_until(deadline) {
                Ok(Response::Token { id: got, text }) if got == id => on_token(&text),
                Ok(Response::Done(done)) if done.id == id => return Ok(done),
                Ok(Response::Error(err)) if err.id.is_none_or(|got| got == id) => {
                    return Err(err.into());
                }
                // Leftovers from an earlier request that timed out.
                Ok(_) => {}
                Err(LlmError::Timeout) => {
                    self.abandon(id);
                    return Err(LlmError::Timeout);
                }
                Err(err) => return Err(err),
            }
        }
    }

    /// Cancels a request that ran out of time and waits briefly for it to
    /// end; a worker that doesn't respond is killed.
    fn abandon(&mut self, id: u64) {
        let _ = self.shared.send(&Request::Cancel { id });
        let deadline = Instant::now() + CANCEL_GRACE;
        loop {
            match self.recv_until(deadline) {
                Ok(Response::Done(done)) if done.id == id => return,
                Ok(Response::Error(err)) if err.id == Some(id) => return,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        self.kill();
    }

    /// Frees the model but keeps the process.
    pub fn unload(&mut self) -> Result<()> {
        self.loaded = None;
        if !self.is_running() {
            return Ok(());
        }
        self.shared.send(&Request::Unload)?;
        match self.recv_until(Instant::now() + SHUTDOWN_GRACE)? {
            Response::Unloaded => Ok(()),
            Response::Error(err) => Err(err.into()),
            other => Err(unexpected(&other)),
        }
    }

    pub fn info(&mut self) -> Result<WorkerInfo> {
        self.ensure_spawned()?;
        self.shared.send(&Request::Info)?;
        match self.recv_until(Instant::now() + SHUTDOWN_GRACE)? {
            Response::Info(info) => Ok(info),
            other => Err(unexpected(&other)),
        }
    }

    /// Ends the worker process, freeing all of its memory. The model stays
    /// selected: the next [`Self::complete`] starts the worker and loads it
    /// again.
    pub fn shutdown(&mut self) {
        let Some(mut worker) = self.worker.take() else {
            return;
        };
        // Closing stdin asks the worker to exit on its own.
        self.shared
            .stdin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let deadline = Instant::now() + SHUTDOWN_GRACE;
        while Instant::now() < deadline {
            if !matches!(worker.child.try_wait(), Ok(None)) {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = worker.child.kill();
        let _ = worker.child.wait();
    }

    fn kill(&mut self) {
        if let Some(mut worker) = self.worker.take() {
            let _ = worker.child.kill();
            let _ = worker.child.wait();
        }
        self.drop_worker();
    }

    fn drop_worker(&mut self) {
        self.worker = None;
        self.shared
            .stdin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }

    fn ensure_spawned(&mut self) -> Result<()> {
        if self.is_running() {
            return Ok(());
        }
        if !self.worker_path.is_file() {
            return Err(LlmError::WorkerNotFound(self.worker_path.clone()));
        }
        let mut child = Command::new(&self.worker_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(LlmError::Protocol("worker pipes missing".into()));
        };
        let (tx, responses) = mpsc::channel();
        thread::Builder::new()
            .name("tw-llm-reader".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    let parsed = serde_json::from_str(&line).map_err(|e| format!("{e}: {line}"));
                    if tx.send(parsed).is_err() {
                        break;
                    }
                }
            })?;
        *self
            .shared
            .stdin
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(stdin);
        self.worker = Some(Worker { child, responses });
        Ok(())
    }

    fn recv_until(&mut self, deadline: Instant) -> Result<Response> {
        let Some(worker) = self.worker.as_ref() else {
            return Err(LlmError::WorkerExited);
        };
        let wait = deadline.saturating_duration_since(Instant::now());
        match worker.responses.recv_timeout(wait) {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(line)) => Err(LlmError::Protocol(line)),
            Err(RecvTimeoutError::Timeout) => Err(LlmError::Timeout),
            Err(RecvTimeoutError::Disconnected) => {
                self.kill();
                Err(LlmError::WorkerExited)
            }
        }
    }
}

fn unexpected(response: &Response) -> LlmError {
    LlmError::Protocol(format!("unexpected response {response:?}"))
}

impl Drop for LlmClient {
    fn drop(&mut self) {
        self.kill();
    }
}
