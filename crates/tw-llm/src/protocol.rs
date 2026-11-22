//! The JSON-lines protocol between [`crate::LlmClient`] and `tw-llm-worker`.
//!
//! The client writes one [`Request`] per line to the worker's stdin; the
//! worker answers each with one [`Response`] line (`complete` with `stream`
//! set sends `token` lines first). `cancel` gets no answer of its own: the
//! request it targets ends with a `cancelled` error.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Request {
    Load {
        model_path: PathBuf,
        context_length: u32,
        /// `None` offloads every layer to the GPU, falling back to the CPU
        /// when that fails.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gpu_layers: Option<u32>,
    },
    Complete(Completion),
    Cancel {
        id: u64,
    },
    Unload,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Completion {
    pub id: u64,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    /// 0 samples greedily.
    pub temperature: f32,
    pub top_p: f32,
    pub repeat_penalty: f32,
    pub seed: u32,
    /// Generation ends before the first of these; it is not part of the text.
    pub stop: Vec<String>,
    pub stream: bool,
    /// Pre-fills an empty `<think></think>` block when the model's template
    /// knows about thinking, so hybrid and reasoning models answer directly.
    pub disable_thinking: bool,
}

impl Default for Completion {
    fn default() -> Self {
        Self {
            id: 0,
            messages: Vec::new(),
            max_tokens: 256,
            temperature: 0.2,
            top_p: 0.9,
            repeat_penalty: 1.05,
            seed: 42,
            stop: Vec::new(),
            stream: false,
            disable_thinking: false,
        }
    }
}

/// Externally tagged, so `unloaded` is `"unloaded"` and an error is
/// `{"error":{"kind":"tooLong",…}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Response {
    Loaded {
        model_path: PathBuf,
        load_ms: u64,
        info: WorkerInfo,
    },
    Token {
        id: u64,
        text: String,
    },
    Done(Done),
    Unloaded,
    Info(WorkerInfo),
    Error(WorkerError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Done {
    pub id: u64,
    pub text: String,
    pub prompt_tokens: u32,
    pub tokens: u32,
    /// From the start of the request to the first generated token, prompt
    /// processing included.
    pub first_token_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerInfo {
    /// `None` when no model is loaded.
    pub model_name: Option<String>,
    pub context_length: u32,
    /// The GGUF carries a chat template; otherwise ChatML is used.
    pub template_found: bool,
    /// `"metal"`, `"cuda"`, `"vulkan"` or `"cpu"`.
    pub backend: String,
    pub gpu_layers: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    InvalidRequest,
    ModelNotFound,
    LoadFailed,
    NotLoaded,
    /// The prompt plus `maxTokens` does not fit the context. Nothing is
    /// truncated; the caller falls back.
    TooLong,
    Cancelled,
    Generation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct WorkerError {
    pub kind: ErrorKind,
    pub message: String,
    /// The `complete` request this ends, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
}

impl WorkerError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            id: None,
        }
    }

    pub fn for_request(mut self, id: u64) -> Self {
        self.id = Some(id);
        self
    }
}

impl From<WorkerError> for Response {
    fn from(err: WorkerError) -> Self {
        Self::Error(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip_request(req: &Request) {
        let line = serde_json::to_string(req).unwrap();
        assert!(!line.contains('\n'));
        assert_eq!(&serde_json::from_str::<Request>(&line).unwrap(), req);
    }

    fn round_trip_response(res: &Response) {
        let line = serde_json::to_string(res).unwrap();
        assert!(!line.contains('\n'));
        assert_eq!(&serde_json::from_str::<Response>(&line).unwrap(), res);
    }

    #[test]
    fn requests_round_trip() {
        round_trip_request(&Request::Load {
            model_path: "/m/a.gguf".into(),
            context_length: 4096,
            gpu_layers: None,
        });
        round_trip_request(&Request::Complete(Completion {
            id: 7,
            messages: vec![
                Message::new(Role::System, "s"),
                Message::new(Role::User, "line one\nline two"),
            ],
            stop: vec!["</text>".into()],
            stream: true,
            disable_thinking: true,
            ..Completion::default()
        }));
        round_trip_request(&Request::Cancel { id: 7 });
        round_trip_request(&Request::Unload);
        round_trip_request(&Request::Info);
    }

    #[test]
    fn responses_round_trip() {
        let info = WorkerInfo {
            model_name: Some("Qwen".into()),
            context_length: 4096,
            template_found: true,
            backend: "metal".into(),
            gpu_layers: 99,
        };
        round_trip_response(&Response::Loaded {
            model_path: "/m/a.gguf".into(),
            load_ms: 300,
            info: info.clone(),
        });
        round_trip_response(&Response::Token {
            id: 1,
            text: "Hé".into(),
        });
        round_trip_response(&Response::Done(Done {
            id: 1,
            text: "Hello.".into(),
            prompt_tokens: 10,
            tokens: 3,
            first_token_ms: 40,
            total_ms: 90,
        }));
        round_trip_response(&Response::Unloaded);
        round_trip_response(&Response::Info(info));
        round_trip_response(
            &WorkerError::new(ErrorKind::TooLong, "too long")
                .for_request(3)
                .into(),
        );
    }

    #[test]
    fn wire_format_is_camel_case_and_tagged() {
        assert_eq!(
            serde_json::to_string(&Response::Unloaded).unwrap(),
            r#""unloaded""#
        );
        assert_eq!(
            serde_json::to_string(&Response::from(WorkerError::new(
                ErrorKind::ModelNotFound,
                "missing"
            )))
            .unwrap(),
            r#"{"error":{"kind":"modelNotFound","message":"missing"}}"#
        );
        let req: Request = serde_json::from_str(
            r#"{"complete":{"id":3,"messages":[{"role":"user","content":"hi"}],"maxTokens":64}}"#,
        )
        .unwrap();
        let Request::Complete(c) = req else {
            panic!("expected complete");
        };
        assert_eq!((c.id, c.max_tokens), (3, 64));
        assert!((c.temperature - 0.2).abs() < f32::EPSILON);
        assert_eq!(c.messages[0].role, Role::User);
    }
}
