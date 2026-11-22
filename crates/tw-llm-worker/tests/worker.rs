//! Spawns the real worker. Tests that need a model read it from the directory
//! in `TINY_WISPR_TEST_MODELS` and are skipped when it is unset or the file
//! is missing.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tw_core::text::polish::PolishRequest;
use tw_core::text::{CleanupLevel, StyleCategory, WritingStyle};
use tw_llm::{
    Completion, ErrorKind, LlmClient, LlmError, LlmPolisher, Message, ModelSpec, Response, Role,
};

const WORKER: &str = env!("CARGO_BIN_EXE_tw-llm-worker");
const SMALL_MODEL: &str = "Qwen3.5-2B-Q4_K_M.gguf";
const LONG: Duration = Duration::from_secs(120);

fn model(file: &str) -> Option<PathBuf> {
    let path = Path::new(&std::env::var_os("TINY_WISPR_TEST_MODELS")?).join(file);
    if path.is_file() {
        Some(path)
    } else {
        eprintln!("skipped: {} is missing", path.display());
        None
    }
}

fn chat(user: &str, max_tokens: u32) -> Completion {
    Completion {
        messages: vec![
            Message::new(Role::System, "You are a helpful assistant."),
            Message::new(Role::User, user),
        ],
        max_tokens,
        disable_thinking: true,
        ..Completion::default()
    }
}

fn wait_for_exit(child: &mut std::process::Child, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn missing_model_is_a_typed_error() {
    let mut client = LlmClient::new(WORKER);
    let err = client
        .load(Path::new("/nonexistent/model.gguf"), 2048)
        .unwrap_err();
    assert_eq!(err.kind(), Some(ErrorKind::ModelNotFound), "{err:?}");
    assert!(client.is_running(), "the worker survives a failed load");
    assert!(matches!(
        client.complete(chat("hi", 8), LONG, &mut |_| {}),
        Err(LlmError::NotLoaded)
    ));
}

#[test]
fn corrupt_model_is_a_typed_error() {
    let dir = std::env::temp_dir().join(format!("tw-llm-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("broken.gguf");
    std::fs::write(&path, b"GGUF this is not really a model").unwrap();
    let mut client = LlmClient::new(WORKER);
    let err = client.load(&path, 2048).unwrap_err();
    assert_eq!(err.kind(), Some(ErrorKind::LoadFailed), "{err:?}");
    assert!(client.is_running());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn missing_worker_is_reported() {
    let mut client = LlmClient::new("/nonexistent/tw-llm-worker");
    assert!(matches!(
        client.load(Path::new("/m.gguf"), 2048),
        Err(LlmError::WorkerNotFound(_))
    ));
}

#[test]
fn answers_raw_protocol_and_exits_when_stdin_closes() {
    let mut child = Command::new(WORKER)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    stdin.write_all(b"nonsense\n\"info\"\n").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let Response::Error(err) = serde_json::from_str(&line).unwrap() else {
        panic!("expected an error for {line}");
    };
    assert_eq!(err.kind, ErrorKind::InvalidRequest);
    line.clear();
    stdout.read_line(&mut line).unwrap();
    let Response::Info(info) = serde_json::from_str(&line).unwrap() else {
        panic!("expected info, got {line}");
    };
    assert_eq!(info.model_name, None);

    drop(stdin);
    assert!(wait_for_exit(&mut child, Duration::from_secs(5)));
}

#[test]
fn loads_completes_and_streams() {
    let Some(path) = model(SMALL_MODEL) else {
        return;
    };
    let mut client = LlmClient::new(WORKER);
    let loaded = client.load(&path, 2048).unwrap();
    assert!(loaded.info.template_found);
    assert!(loaded.info.model_name.is_some());
    eprintln!("load {} ms, {:?}", loaded.load_ms, loaded.info);

    let done = client
        .complete(
            chat("Reply with the single word: banana", 16),
            LONG,
            &mut |_| {},
        )
        .unwrap();
    assert!(done.text.to_lowercase().contains("banana"), "{done:?}");
    assert!(!done.text.contains("<think>"), "{done:?}");

    let mut streamed = String::new();
    let mut pieces = 0;
    let done = client
        .complete(
            Completion {
                stream: true,
                ..chat("Count from one to ten in words, separated by spaces.", 48)
            },
            LONG,
            &mut |piece| {
                pieces += 1;
                streamed.push_str(piece);
            },
        )
        .unwrap();
    assert!(pieces > 3, "{pieces}");
    assert_eq!(streamed, done.text);
    assert!(done.text.to_lowercase().contains("five"), "{done:?}");
    eprintln!(
        "first token {} ms, {} tokens in {} ms",
        done.first_token_ms, done.tokens, done.total_ms
    );
}

#[test]
fn stop_strings_end_generation() {
    let Some(path) = model(SMALL_MODEL) else {
        return;
    };
    let mut client = LlmClient::new(WORKER);
    client.load(&path, 2048).unwrap();
    let done = client
        .complete(
            Completion {
                stop: vec!["four".into()],
                temperature: 0.0,
                ..chat("Count from one to ten in words, separated by spaces.", 48)
            },
            LONG,
            &mut |_| {},
        )
        .unwrap();
    let text = done.text.to_lowercase();
    assert!(text.contains("three") && !text.contains("four"), "{done:?}");
}

#[test]
fn cancel_mid_generation_returns_fast() {
    let Some(path) = model(SMALL_MODEL) else {
        return;
    };
    let mut client = LlmClient::new(WORKER);
    client.load(&path, 4096).unwrap();
    let cancel = client.cancel_handle();
    let started = Arc::new(Mutex::new(None::<Instant>));
    let mut first = true;
    let mark = Arc::clone(&started);
    let err = client
        .complete(
            Completion {
                stream: true,
                ..chat("Write a very long story about a lighthouse keeper.", 2000)
            },
            LONG,
            &mut |_| {
                if first {
                    first = false;
                    *mark.lock().unwrap() = Some(Instant::now());
                    cancel.cancel();
                }
            },
        )
        .unwrap_err();
    let since_cancel = started.lock().unwrap().unwrap().elapsed();
    assert_eq!(err.kind(), Some(ErrorKind::Cancelled), "{err:?}");
    assert!(
        since_cancel < Duration::from_millis(500),
        "{since_cancel:?}"
    );
    // The worker is still usable.
    let done = client
        .complete(
            chat("Reply with the single word: yes", 8),
            LONG,
            &mut |_| {},
        )
        .unwrap();
    assert!(!done.text.is_empty());
}

#[test]
fn killed_worker_is_restarted_and_the_request_retried() {
    let Some(path) = model(SMALL_MODEL) else {
        return;
    };
    let mut client = LlmClient::new(WORKER);
    client.load(&path, 2048).unwrap();
    let pid = client.worker_pid().unwrap();
    let killed = AtomicBool::new(false);
    let done = client
        .complete(
            Completion {
                stream: true,
                ..chat("Reply with the single word: banana", 16)
            },
            LONG,
            &mut |_| {
                if !killed.swap(true, Ordering::SeqCst) {
                    Command::new("kill")
                        .args(["-9", &pid.to_string()])
                        .status()
                        .unwrap();
                }
            },
        )
        .unwrap();
    assert!(killed.load(Ordering::SeqCst));
    assert_ne!(client.worker_pid(), Some(pid));
    assert!(done.text.to_lowercase().contains("banana"), "{done:?}");
}

#[test]
fn prompt_longer_than_the_context_is_too_long() {
    let Some(path) = model(SMALL_MODEL) else {
        return;
    };
    let mut client = LlmClient::new(WORKER);
    client.load(&path, 512).unwrap();
    let err = client
        .complete(
            chat(&"lorem ipsum dolor ".repeat(300), 64),
            LONG,
            &mut |_| {},
        )
        .unwrap_err();
    assert_eq!(err.kind(), Some(ErrorKind::TooLong), "{err:?}");
}

#[test]
fn stdin_close_mid_generation_ends_the_worker() {
    let Some(path) = model(SMALL_MODEL) else {
        return;
    };
    let mut child = Command::new(WORKER)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let load = tw_llm::Request::Load {
        model_path: path,
        context_length: 4096,
        gpu_layers: None,
    };
    let complete = tw_llm::Request::Complete(Completion {
        id: 1,
        stream: true,
        ..chat("Write a very long story about a lighthouse keeper.", 2000)
    });
    for request in [load, complete] {
        serde_json::to_writer(&mut stdin, &request).unwrap();
        stdin.write_all(b"\n").unwrap();
    }
    stdin.flush().unwrap();
    let mut line = String::new();
    while !line.contains("\"token\"") {
        line.clear();
        assert!(
            stdout.read_line(&mut line).unwrap() > 0,
            "worker ended early"
        );
    }
    drop(stdin);
    assert!(wait_for_exit(&mut child, Duration::from_secs(2)));
}

#[test]
fn polisher_cleans_text_and_unloads_when_idle() {
    let Some(path) = model(SMALL_MODEL) else {
        return;
    };
    let polisher = LlmPolisher::new(WORKER, Some(Duration::from_millis(400)));
    polisher.set_model(Some(ModelSpec::new(&path, false)));
    let request = PolishRequest {
        text: "So we should meet at 2, actually let's say 3, at the office.".into(),
        level: CleanupLevel::Light,
        style: WritingStyle::Formal,
        category: StyleCategory::Work,
        vocabulary: vec![],
        reasoning_model: false,
    };
    let first = polisher.polish_timed(&request).unwrap();
    assert!(first.load_ms.is_some());
    assert!(
        first.text.contains('3') && !first.text.contains('2'),
        "{first:?}"
    );
    assert!(polisher.is_loaded());

    thread::sleep(Duration::from_millis(1500));
    assert!(!polisher.is_loaded(), "the worker ends after the idle time");

    let again = polisher.polish_timed(&request).unwrap();
    assert!(again.load_ms.is_some(), "reloaded on demand");
    eprintln!("reload {:?} ms", again.load_ms);
}
