//! `tw-llm-worker`: reads [`tw_llm::Request`] JSON lines on stdin and answers
//! with [`tw_llm::Response`] JSON lines on stdout. Exits when stdin closes.
//!
//! A reader thread owns stdin so `cancel` and end of input are seen while the
//! main thread is generating. llama.cpp's own logging is silenced: stdout
//! carries the protocol only.

mod stream;

use std::io::{self, BufRead, Write};
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Instant;

use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use tw_llm::{Completion, Done, ErrorKind, Request, Response, WorkerError, WorkerInfo};

use crate::stream::{StopScanner, Utf8Decoder};

const ALL_LAYERS: u32 = 999;
const MIN_CONTEXT: u32 = 512;
const BATCH: u32 = 512;
const PENALTY_WINDOW: i32 = 64;
const THINK_OFF: &str = "<think>\n\n</think>\n\n";

enum Incoming {
    Request(Request),
    Invalid(String),
}

#[derive(Default)]
struct Control {
    /// The id of the last `cancel` received.
    cancel: AtomicU64,
    /// Stdin closed: the app is gone.
    closed: AtomicBool,
}

impl Control {
    fn stop_requested(&self, id: u64) -> bool {
        self.closed.load(Ordering::SeqCst) || self.cancel.load(Ordering::SeqCst) == id
    }
}

fn spawn_reader(control: Arc<Control>) -> io::Result<mpsc::Receiver<Incoming>> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new().name("stdin".into()).spawn(move || {
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            let incoming = match serde_json::from_str::<Request>(&line) {
                Ok(Request::Cancel { id }) => {
                    control.cancel.store(id, Ordering::SeqCst);
                    continue;
                }
                Ok(request) => Incoming::Request(request),
                Err(err) => Incoming::Invalid(format!("invalid request: {err}")),
            };
            if tx.send(incoming).is_err() {
                break;
            }
        }
        control.closed.store(true, Ordering::SeqCst);
    })?;
    Ok(rx)
}

struct Out<W: Write>(W);

impl<W: Write> Out<W> {
    fn send(&mut self, response: &Response) -> io::Result<()> {
        serde_json::to_writer(&mut self.0, response)?;
        self.0.write_all(b"\n")?;
        self.0.flush()
    }
}

fn main() -> anyhow::Result<()> {
    // Before init, so ggml's device setup is silenced too.
    llama_cpp_2::send_logs_to_tracing(llama_cpp_2::LogOptions::default().with_logs_enabled(false));
    let backend = LlamaBackend::init()?;
    let control = Arc::new(Control::default());
    let rx = spawn_reader(Arc::clone(&control))?;
    let mut out = Out(io::stdout().lock());

    let mut pending = None;
    while let Some(incoming) = pending.take().or_else(|| rx.recv().ok()) {
        let request = match incoming {
            Incoming::Request(request) => request,
            Incoming::Invalid(message) => {
                out.send(&WorkerError::new(ErrorKind::InvalidRequest, message).into())?;
                continue;
            }
        };
        match request {
            Request::Load {
                model_path,
                context_length,
                gpu_layers,
            } => {
                let started = Instant::now();
                match load_model(&backend, &model_path, gpu_layers, &control) {
                    Ok((model, layers)) => {
                        let loaded = Loaded {
                            model: &model,
                            gpu_layers: layers,
                            started,
                        };
                        pending = serve(
                            &backend,
                            &loaded,
                            &model_path,
                            context_length,
                            &rx,
                            &control,
                            &mut out,
                        )?;
                    }
                    Err(err) => out.send(&err.into())?,
                }
            }
            Request::Complete(completion) => out.send(
                &WorkerError::new(ErrorKind::NotLoaded, "no model loaded")
                    .for_request(completion.id)
                    .into(),
            )?,
            Request::Unload => out.send(&Response::Unloaded)?,
            Request::Info => out.send(&Response::Info(WorkerInfo {
                backend: backend_name(&backend, ALL_LAYERS).to_owned(),
                ..WorkerInfo::default()
            }))?,
            Request::Cancel { .. } => {}
        }
    }
    Ok(())
}

fn backend_name(backend: &LlamaBackend, gpu_layers: u32) -> &'static str {
    if gpu_layers == 0 || !backend.supports_gpu_offload() {
        "cpu"
    } else if cfg!(feature = "cuda") {
        "cuda"
    } else if cfg!(feature = "vulkan") {
        "vulkan"
    } else if cfg!(target_os = "macos") {
        "metal"
    } else {
        "gpu"
    }
}

/// Loads with every layer on the GPU and falls back to the CPU.
fn load_model(
    backend: &LlamaBackend,
    path: &Path,
    gpu_layers: Option<u32>,
    control: &Arc<Control>,
) -> Result<(LlamaModel, u32), WorkerError> {
    if !path.is_file() {
        return Err(WorkerError::new(
            ErrorKind::ModelNotFound,
            format!("model file not found: {}", path.display()),
        ));
    }
    let wanted = gpu_layers.unwrap_or(ALL_LAYERS);
    let attempts: &[u32] = if wanted == 0 { &[0] } else { &[wanted, 0] };
    for &layers in attempts {
        let control = Arc::clone(control);
        let params = LlamaModelParams::default()
            .with_n_gpu_layers(layers)
            .with_progress_callback(move |_| !control.closed.load(Ordering::SeqCst));
        if let Ok(model) = LlamaModel::load_from_file(backend, path, &params) {
            return Ok((model, layers));
        }
    }
    Err(WorkerError::new(
        ErrorKind::LoadFailed,
        format!(
            "could not load {}: the file is damaged or not a supported GGUF model",
            path.display()
        ),
    ))
}

struct Loaded<'m> {
    model: &'m LlamaModel,
    gpu_layers: u32,
    started: Instant,
}

/// Answers requests while a model is loaded. Returns the request that needs
/// the model gone (`load` of another file, `unload`), or `None` at the end of
/// input.
fn serve(
    backend: &LlamaBackend,
    loaded: &Loaded<'_>,
    model_path: &Path,
    context_length: u32,
    rx: &mpsc::Receiver<Incoming>,
    control: &Control,
    out: &mut Out<impl Write>,
) -> io::Result<Option<Incoming>> {
    let model = loaded.model;
    let n_ctx = context_length.min(model.n_ctx_train()).max(MIN_CONTEXT);
    let threads = thread::available_parallelism().map_or(4, |n| n.get().min(8)) as i32;
    let params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(n_ctx))
        .with_n_batch(BATCH)
        .with_n_threads(threads)
        .with_n_threads_batch(threads);
    let mut ctx = match model.new_context(backend, params) {
        Ok(ctx) => ctx,
        Err(err) => {
            out.send(
                &WorkerError::new(
                    ErrorKind::LoadFailed,
                    format!("could not create a context: {err}"),
                )
                .into(),
            )?;
            return Ok(None);
        }
    };
    let template = model.chat_template(None).ok();
    let template_text = template
        .as_ref()
        .and_then(|t| t.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let info = WorkerInfo {
        model_name: Some(
            model
                .meta_val_str("general.name")
                .unwrap_or_else(|_| file_stem(model_path)),
        ),
        context_length: ctx.n_ctx(),
        template_found: template.is_some(),
        backend: backend_name(backend, loaded.gpu_layers).to_owned(),
        gpu_layers: loaded.gpu_layers.min(model.n_layer() + 1),
    };
    out.send(&Response::Loaded {
        model_path: model_path.to_path_buf(),
        load_ms: millis(loaded.started),
        info: info.clone(),
    })?;

    while let Ok(incoming) = rx.recv() {
        let request = match incoming {
            Incoming::Request(request) => request,
            Incoming::Invalid(message) => {
                out.send(&WorkerError::new(ErrorKind::InvalidRequest, message).into())?;
                continue;
            }
        };
        match request {
            Request::Complete(completion) => {
                let id = completion.id;
                let prompt = render_prompt(model, template.as_ref(), &template_text, &completion);
                let response = match prompt
                    .and_then(|p| generate(model, &mut ctx, &p, &completion, control, out))
                {
                    Ok(done) => Response::Done(done),
                    Err(err) => err.for_request(id).into(),
                };
                out.send(&response)?;
            }
            Request::Info => out.send(&Response::Info(info.clone()))?,
            Request::Load {
                model_path: ref next,
                context_length: next_ctx,
                ..
            } if next == model_path && next_ctx == context_length => {
                out.send(&Response::Loaded {
                    model_path: model_path.to_path_buf(),
                    load_ms: 0,
                    info: info.clone(),
                })?;
            }
            Request::Cancel { .. } => {}
            other => return Ok(Some(Incoming::Request(other))),
        }
    }
    Ok(None)
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn millis(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The model's own chat template from the GGUF, ChatML when it has none or
/// llama.cpp can't apply it.
fn render_prompt(
    model: &LlamaModel,
    template: Option<&LlamaChatTemplate>,
    template_text: &str,
    completion: &Completion,
) -> Result<String, WorkerError> {
    let messages = completion
        .messages
        .iter()
        .map(|m| LlamaChatMessage::new(m.role.as_str().to_owned(), m.content.clone()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| WorkerError::new(ErrorKind::InvalidRequest, e.to_string()))?;
    let applied = template.and_then(|t| model.apply_chat_template(t, &messages, true).ok());
    let mut prompt = match applied {
        Some(prompt) => prompt,
        None => {
            let chatml = LlamaChatTemplate::new("chatml")
                .map_err(|e| WorkerError::new(ErrorKind::Generation, e.to_string()))?;
            model
                .apply_chat_template(&chatml, &messages, true)
                .map_err(|e| WorkerError::new(ErrorKind::Generation, e.to_string()))?
        }
    };
    if completion.disable_thinking
        && template_text.contains("<think>")
        && !prompt.ends_with(THINK_OFF)
    {
        prompt.push_str(THINK_OFF);
    }
    Ok(prompt)
}

fn sampler(model: &LlamaModel, completion: &Completion) -> LlamaSampler {
    let penalties = LlamaSampler::penalties(
        model.n_vocab(),
        PENALTY_WINDOW,
        completion.repeat_penalty.max(f32::MIN_POSITIVE),
        0.0,
        0.0,
    );
    if completion.temperature <= 0.0 {
        return LlamaSampler::chain_simple([penalties, LlamaSampler::greedy()]);
    }
    LlamaSampler::chain_simple([
        penalties,
        LlamaSampler::top_k(40),
        LlamaSampler::top_p(completion.top_p, 1),
        LlamaSampler::temp(completion.temperature),
        LlamaSampler::dist(completion.seed),
    ])
}

fn generation_error(err: impl std::fmt::Display) -> WorkerError {
    WorkerError::new(ErrorKind::Generation, err.to_string())
}

fn cancelled() -> WorkerError {
    WorkerError::new(ErrorKind::Cancelled, "cancelled")
}

fn generate(
    model: &LlamaModel,
    ctx: &mut LlamaContext<'_>,
    prompt: &str,
    completion: &Completion,
    control: &Control,
    out: &mut Out<impl Write>,
) -> Result<Done, WorkerError> {
    let started = Instant::now();
    let id = completion.id;
    let vocab = model.vocab();
    let tokens = vocab.tokenize(prompt.as_bytes(), true, true);
    let needed = tokens.len() + completion.max_tokens as usize;
    if needed > ctx.n_ctx() as usize {
        return Err(WorkerError::new(
            ErrorKind::TooLong,
            format!(
                "the prompt ({} tokens) plus {} tokens of output does not fit the context of {}",
                tokens.len(),
                completion.max_tokens,
                ctx.n_ctx()
            ),
        ));
    }

    ctx.clear_kv_cache();
    let mut batch = LlamaBatch::new(BATCH as usize, 1);
    let mut pos: i32 = 0;
    for chunk in tokens.chunks(BATCH as usize) {
        if control.stop_requested(id) {
            return Err(cancelled());
        }
        batch.clear();
        for (i, &token) in chunk.iter().enumerate() {
            let last = pos as usize + i + 1 == tokens.len();
            batch
                .add(token, pos + i as i32, &[0], last)
                .map_err(generation_error)?;
        }
        ctx.decode(&mut batch).map_err(generation_error)?;
        pos += chunk.len() as i32;
    }

    let mut sampler = sampler(model, completion);
    let mut decoder = Utf8Decoder::default();
    let mut stops = StopScanner::new(&completion.stop);
    let mut first_token_ms = 0;
    let mut generated: u32 = 0;
    while generated < completion.max_tokens {
        if control.stop_requested(id) {
            return Err(cancelled());
        }
        let token = sampler.sample(ctx, batch.n_tokens() - 1);
        if generated == 0 {
            first_token_ms = millis(started);
        }
        generated += 1;
        if vocab.is_eog(token) {
            break;
        }
        let piece = decoder.push(&vocab.token_to_piece(token, false, None));
        let (ready, stopped) = stops.push(&piece);
        if completion.stream && !ready.is_empty() {
            out.send(&Response::Token { id, text: ready })
                .map_err(generation_error)?;
        }
        if stopped {
            break;
        }
        batch.clear();
        batch
            .add(token, pos, &[0], true)
            .map_err(generation_error)?;
        ctx.decode(&mut batch).map_err(generation_error)?;
        pos += 1;
    }
    let (rest, _) = stops.push(&decoder.finish());
    let tail = stops.finish();
    if completion.stream {
        let text = rest + &tail;
        if !text.is_empty() {
            out.send(&Response::Token { id, text })
                .map_err(generation_error)?;
        }
    }
    Ok(Done {
        id,
        text: stops.into_text(),
        prompt_tokens: u32::try_from(tokens.len()).unwrap_or(u32::MAX),
        tokens: generated,
        first_token_ms,
        total_ms: millis(started),
    })
}
