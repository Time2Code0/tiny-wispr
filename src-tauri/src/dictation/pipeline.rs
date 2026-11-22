//! One dictation's audio to the text to insert (spec §3): trim silence →
//! speech? → Whisper → rule-based formatting → polish → dictionary, snippets
//! and style.

use std::sync::atomic::{AtomicBool, Ordering};

use tw_core::Settings;
use tw_core::dictation::machine::{Outcome, Problem, Transcript};
use tw_core::settings::Languages;
use tw_core::text::pipeline::{self, PipelineContext};
use tw_core::text::polish::PolishRequest;
use tw_core::text::{CleanupLevel, StyleCategory};
use tw_stt::{SttError, TranscribeOptions};

use super::{Deps, Polisher};

/// `None` when cancelled: the machine has moved on and wants no answer.
pub fn process(
    deps: &Deps,
    samples: &[f32],
    cancel: &AtomicBool,
    polish_started: &dyn Fn(),
) -> Option<Outcome> {
    if !samples.is_empty() && samples.iter().all(|&s| s == 0.0) && deps.desktop.microphone_denied()
    {
        return Some(Outcome::Failed(Problem::MicrophoneDenied));
    }
    if !tw_audio::vad::has_speech(samples) {
        return Some(Outcome::NoSpeech);
    }
    let settings = deps.data.settings.get();
    let engine = match deps.transcribers.load(&settings) {
        Ok(engine) => engine,
        Err(problem) => return Some(Outcome::Failed(problem)),
    };
    let dictionary = deps.data.dictionary.get();
    let snippets = deps.data.snippets.get();
    let vocabulary: Vec<String> = dictionary.iter().map(|e| e.word.clone()).collect();
    let options = transcribe_options(&settings, &vocabulary);
    let audio = tw_audio::vad::trim_silence(samples);
    let transcript = match engine.transcribe(audio, &options, cancel) {
        Ok(transcript) => transcript,
        Err(SttError::Cancelled) => return None,
        Err(err) => {
            tracing::warn!(%err, "transcription failed");
            return Some(Outcome::Failed(Problem::Failed(err.to_string())));
        }
    };
    tracing::info!(
        audio_ms = transcript.audio_ms,
        processing_ms = transcript.processing_ms,
        language = ?transcript.language,
        "transcribed"
    );
    let raw = tw_stt::clean_transcript(&transcript.text, true);
    if raw.is_empty() {
        return Some(Outcome::NoSpeech);
    }

    let app = deps.desktop.frontmost_app();
    let category = app
        .as_ref()
        .map_or(StyleCategory::Other, |app| app.category.style_category());
    let ctx = PipelineContext {
        level: settings.cleanup_level,
        style: settings.styles.get(category),
        category,
        dictionary: &dictionary,
        snippets: &snippets,
    };
    let (text, fixes) = format(
        deps.polisher.as_ref(),
        &settings,
        &ctx,
        &raw,
        vocabulary,
        polish_started,
    );
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    Some(Outcome::Transcript(Transcript {
        raw,
        text,
        app,
        fixes,
    }))
}

/// Whisper options for the user's languages, biased towards the dictionary.
pub fn transcribe_options(settings: &Settings, vocabulary: &[String]) -> TranscribeOptions {
    TranscribeOptions {
        initial_prompt: tw_stt::vocabulary_prompt(vocabulary),
        ..language_options(&settings.languages)
    }
}

/// Whisper's cleaned text to the text to insert: rule-based formatting,
/// polish when a language model is selected (keeping the rule-based text if
/// it fails), then dictionary, snippets and style. Returns the text and the
/// number of fixes made.
pub fn format(
    polisher: &dyn Polisher,
    settings: &Settings,
    ctx: &PipelineContext,
    raw: &str,
    vocabulary: Vec<String>,
    polish_started: &dyn Fn(),
) -> (String, u32) {
    let before = pipeline::before_polish_with_fixes(raw, ctx);
    let mut text = before.text;
    if settings.cleanup_level != CleanupLevel::None && polisher.active(settings) {
        polish_started();
        let request = PolishRequest {
            text: text.clone(),
            level: ctx.level,
            style: ctx.style,
            category: ctx.category,
            vocabulary,
            reasoning_model: false,
        };
        if let Some(polished) = polisher.polish(&request) {
            text = polished;
        }
    }
    let after = pipeline::after_polish_with_fixes(&text, ctx);
    (after.text, before.fixes.total() + after.fixes.total())
}

/// Auto-detect, one forced language, or detection among the chosen ones.
fn language_options(languages: &Languages) -> TranscribeOptions {
    match (languages.auto_detect, languages.selected.as_slice()) {
        (false, [only]) => TranscribeOptions {
            language: Some(only.clone()),
            ..TranscribeOptions::default()
        },
        (false, selected) => TranscribeOptions {
            detect_among: selected.to_vec(),
            ..TranscribeOptions::default()
        },
        (true, _) => TranscribeOptions::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_map_to_whisper_options() {
        let auto = Languages {
            auto_detect: true,
            selected: vec!["de".into()],
        };
        assert_eq!(language_options(&auto), TranscribeOptions::default());
        let one = Languages {
            auto_detect: false,
            selected: vec!["de".into()],
        };
        assert_eq!(language_options(&one).language.as_deref(), Some("de"));
        let two = Languages {
            auto_detect: false,
            selected: vec!["de".into(), "en".into()],
        };
        let options = language_options(&two);
        assert_eq!(options.language, None);
        assert_eq!(options.detect_among, vec!["de", "en"]);
    }
}
