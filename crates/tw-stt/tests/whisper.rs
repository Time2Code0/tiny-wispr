//! Transcription with real whisper.cpp models. Set `TINY_WISPR_TEST_MODELS`
//! to a directory holding `whisper/ggml-tiny.en.bin` and
//! `whisper/ggml-base.bin`. Speech fixtures are made on demand with
//! `scripts/make-speech-fixture.sh` (macOS `say` and `ffmpeg`). Without the
//! models or the tools the tests are skipped. Run with `--nocapture` to see the
//! measured real-time factors.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use tw_audio::{SAMPLE_RATE, wav};
use tw_stt::{
    SttError, TranscribeOptions, Transcriber, Transcript, WhisperEngine, vocabulary_prompt,
};

const TINY_EN: &str = "ggml-tiny.en.bin";
const BASE: &str = "ggml-base.bin";

const LONG_TEXT: &str = "The history of the written word is a history of tools. People \
    carved marks into clay, painted letters onto paper, and later set metal type by hand \
    to print books for thousands of readers. Each new tool made writing faster, and each \
    one changed who could take part. When the typewriter arrived, offices filled with the \
    sound of keys, and a page that once took an hour could be finished in minutes. The \
    personal computer went further still. It let anyone fix a mistake without starting \
    over, move a paragraph with a few keystrokes, and send a letter across the world in \
    seconds. Yet for all that progress, most of us still type with our fingers, one letter \
    at a time, even though we speak three or four times faster than we type. Dictation \
    software has promised to close that gap for decades. Early systems needed long \
    training sessions, a quiet room, and a patient speaker who paused between every word. \
    Modern speech models are different. They are trained on huge amounts of recorded \
    speech, they cope with accents and background noise, and they add punctuation on \
    their own. Best of all, the smaller ones now run on an ordinary laptop, so your voice \
    never has to leave the machine in front of you. That matters for privacy, and it \
    matters for speed, because there is no server to wait for. You hold a key, say what \
    you mean, let go, and the words appear where your cursor was. It feels less like \
    using a program and more like thinking out loud, which is exactly the point.";

fn model(file: &str) -> Option<PathBuf> {
    let Some(dir) = std::env::var_os("TINY_WISPR_TEST_MODELS") else {
        eprintln!("skipping: TINY_WISPR_TEST_MODELS is not set");
        return None;
    };
    let path = Path::new(&dir).join("whisper").join(file);
    if path.is_file() {
        Some(path)
    } else {
        eprintln!("skipping: {} is missing", path.display());
        None
    }
}

fn load(file: &str) -> Option<WhisperEngine> {
    Some(WhisperEngine::load(&model(file)?).expect("model loads"))
}

fn fixture(name: &str, text: &str, voice: &str) -> Option<Vec<f32>> {
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("stt-{name}.wav"));
    if !out.exists() {
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/make-speech-fixture.sh");
        let made = Command::new(script)
            .args([text, out.to_str()?, voice])
            .output()
            .ok()?;
        if !made.status.success() {
            eprintln!(
                "skipping: could not make a speech fixture: {}",
                String::from_utf8_lossy(&made.stderr)
            );
            return None;
        }
    }
    Some(wav::read(&out).expect("fixture reads").samples)
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Share of the expected words that appear in `actual`.
fn recall(expected: &str, actual: &str) -> f32 {
    let expected = words(expected);
    let actual = words(actual);
    let found = expected.iter().filter(|w| actual.contains(w)).count();
    found as f32 / expected.len() as f32
}

fn transcribe(engine: &WhisperEngine, samples: &[f32], options: &TranscribeOptions) -> Transcript {
    engine
        .transcribe(samples, options, &AtomicBool::new(false))
        .expect("transcription succeeds")
}

fn report(label: &str, transcript: &Transcript) {
    eprintln!(
        "{label}: {} ms audio in {} ms, real-time factor {:.3}",
        transcript.audio_ms,
        transcript.processing_ms,
        transcript.processing_ms as f64 / transcript.audio_ms as f64
    );
}

/// Deterministic white noise at roughly `dbfs` RMS, like a quiet room.
fn room(seconds: f32, dbfs: f32) -> Vec<f32> {
    let amplitude = 10_f32.powf(dbfs / 20.0) * 3_f32.sqrt();
    let mut state = 0x9e37_79b9_u32;
    (0..(seconds * SAMPLE_RATE as f32) as usize)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32 * 2.0 - 1.0) * amplitude
        })
        .collect()
}

#[test]
fn english_sentence_comes_back_with_its_words() {
    const TEXT: &str = "Please send the quarterly report to the finance team before Friday.";
    let Some(speech) = fixture("english", TEXT, "Samantha") else {
        return;
    };
    for file in [TINY_EN, BASE] {
        let Some(engine) = load(file) else {
            return;
        };
        let first = transcribe(&engine, &speech, &TranscribeOptions::default());
        report(file, &first);
        assert!(recall(TEXT, &first.text) >= 0.8, "{file}: {:?}", first.text);
        assert_eq!(first.language.as_deref(), Some("en"), "{file}");

        let again = transcribe(&engine, &speech, &TranscribeOptions::default());
        assert_eq!(again.text, first.text, "{file}: a second call differs");
    }
}

#[test]
fn german_is_detected_with_the_multilingual_model() {
    const TEXT: &str =
        "Guten Morgen, ich möchte heute Nachmittag einen Termin beim Arzt vereinbaren.";
    let Some(speech) = fixture("german", TEXT, "Anna") else {
        return;
    };
    let Some(engine) = load(BASE) else {
        return;
    };
    let auto = transcribe(&engine, &speech, &TranscribeOptions::default());
    assert_eq!(auto.language.as_deref(), Some("de"), "{:?}", auto.text);
    assert!(recall(TEXT, &auto.text) >= 0.6, "{:?}", auto.text);

    let among = TranscribeOptions {
        detect_among: vec!["en".to_owned(), "de".to_owned()],
        ..TranscribeOptions::default()
    };
    let constrained = transcribe(&engine, &speech, &among);
    assert_eq!(constrained.language.as_deref(), Some("de"));

    let translate = TranscribeOptions {
        translate: true,
        ..TranscribeOptions::default()
    };
    let english = transcribe(&engine, &speech, &translate);
    assert!(
        recall("good morning appointment doctor", &english.text) >= 0.5,
        "{:?}",
        english.text
    );
}

#[test]
fn silence_returns_empty_text() {
    for file in [TINY_EN, BASE] {
        let Some(engine) = load(file) else {
            return;
        };
        let zeros = vec![0.0; SAMPLE_RATE as usize];
        assert_eq!(
            transcribe(&engine, &zeros, &TranscribeOptions::default()).text,
            ""
        );
        for dbfs in [-70.0, -55.0] {
            let quiet = room(1.0, dbfs);
            let transcript = transcribe(&engine, &quiet, &TranscribeOptions::default());
            assert_eq!(transcript.text, "", "{file} at {dbfs} dBFS");
        }
    }
}

#[test]
fn initial_prompt_changes_the_spelling_of_names() {
    let Some(speech) = fixture(
        "names",
        "Amelia Lindqvist is building Tiny Wispr this week.",
        "Samantha",
    ) else {
        return;
    };
    let Some(engine) = load(TINY_EN) else {
        return;
    };
    let plain = transcribe(&engine, &speech, &TranscribeOptions::default());
    let prompted = transcribe(
        &engine,
        &speech,
        &TranscribeOptions {
            initial_prompt: vocabulary_prompt(&["Amelia Lindqvist", "Tiny Wispr"]),
            ..TranscribeOptions::default()
        },
    );
    assert!(!plain.text.contains("Wispr"), "{:?}", plain.text);
    assert!(prompted.text.contains("Wispr"), "{:?}", prompted.text);
    assert!(prompted.text.contains("Lindqvist"), "{:?}", prompted.text);
    // A bare list of names as the prompt made tiny.en skip the speech that
    // repeated it.
    assert!(
        recall("is building this week", &prompted.text) == 1.0,
        "{:?}",
        prompted.text
    );
}

#[test]
fn cancelling_returns_promptly() {
    let Some(speech) = fixture("long-cancel", LONG_TEXT, "Samantha") else {
        return;
    };
    let Some(engine) = load(BASE) else {
        return;
    };
    let engine = Arc::new(engine);
    let cancel = Arc::new(AtomicBool::new(false));
    let worker = {
        let engine = Arc::clone(&engine);
        let cancel = Arc::clone(&cancel);
        thread::spawn(move || engine.transcribe(&speech, &TranscribeOptions::default(), &cancel))
    };
    thread::sleep(Duration::from_millis(300));
    let cancelled_at = Instant::now();
    cancel.store(true, Ordering::Relaxed);
    let result = worker.join().expect("worker finishes");
    let latency = cancelled_at.elapsed();
    eprintln!("cancelled in {} ms", latency.as_millis());
    assert!(matches!(result, Err(SttError::Cancelled)), "{result:?}");
    assert!(latency < Duration::from_millis(500), "{latency:?}");

    let already = engine.transcribe(&[0.1; 16_000], &TranscribeOptions::default(), &cancel);
    assert!(matches!(already, Err(SttError::Cancelled)));
}

#[test]
fn a_ninety_second_clip_transcribes() {
    let Some(speech) = fixture("long", LONG_TEXT, "Samantha") else {
        return;
    };
    let seconds = speech.len() / SAMPLE_RATE as usize;
    assert!((80..=110).contains(&seconds), "fixture is {seconds} s");
    for file in [TINY_EN, BASE] {
        let Some(engine) = load(file) else {
            return;
        };
        let transcript = transcribe(&engine, &speech, &TranscribeOptions::default());
        report(file, &transcript);
        let recall = recall(LONG_TEXT, &transcript.text);
        assert!(
            recall >= 0.9,
            "{file}: recall {recall}: {:?}",
            transcript.text
        );
    }
}
