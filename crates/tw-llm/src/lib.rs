//! Talks to `tw-llm-worker`, a separate process that runs llama.cpp.
//!
//! The worker is separate because whisper.cpp and llama.cpp both embed ggml
//! and clash when linked into one binary; ending the process also frees the
//! model's memory.
//!
//! [`LlmPolisher`] is what the app uses: one call per dictation. [`LlmClient`]
//! is the lower-level process supervisor, and [`protocol`] the wire format.

mod client;
mod polisher;
pub mod protocol;

pub use client::{CancelHandle, LlmClient, LlmError, Loaded, Result};
pub use polisher::{DEFAULT_CONTEXT, LlmPolisher, ModelSpec, PolishError, Polished};
pub use protocol::{
    Completion, Done, ErrorKind, Message, Request, Response, Role, WorkerError, WorkerInfo,
};
