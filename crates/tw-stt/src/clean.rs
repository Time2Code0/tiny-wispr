//! Removes what Whisper writes that nobody said.

/// Whisper's stock output for silence or noise. Only dropped when the audio
/// held no speech, because each of these can also be dictated.
const SILENCE_HALLUCINATIONS: &[&str] = &[
    "you",
    "thank you",
    "thanks",
    "thanks for watching",
    "thank you for watching",
    "thank you so much for watching",
    "thank you very much",
    "please subscribe",
    "like and subscribe",
    "bye",
    "so",
    "okay",
    "subtitles by the amaraorg community",
    "transcription by castingwords",
];

/// Words that mark a parenthesised span as a sound description, such as
/// "(music)" or "(people laughing)", rather than dictated text.
const SOUND_WORDS: &[&str] = &[
    "music",
    "applause",
    "laugh",
    "laughs",
    "laughing",
    "laughter",
    "silence",
    "noise",
    "inaudible",
    "cough",
    "coughs",
    "coughing",
    "sigh",
    "sighs",
    "clapping",
    "static",
    "beep",
    "beeping",
    "breathing",
    "blank_audio",
    "no audio",
    "foreign language",
];

/// Strips sound annotations (`[BLANK_AUDIO]`, `(music)`, `♪`), collapses
/// whitespace and, when `has_speech` is false, drops the phrases Whisper
/// invents for silence.
pub fn clean_transcript(text: &str, has_speech: bool) -> String {
    let without_brackets = remove_spans(text, '[', ']', |_| true);
    let without_sounds = remove_spans(&without_brackets, '(', ')', is_sound_description);
    let without_notes: String = without_sounds
        .chars()
        .map(|c| if matches!(c, '♪' | '♫') { ' ' } else { c })
        .collect();
    let cleaned = without_notes
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if !has_speech && is_silence_hallucination(&cleaned) {
        return String::new();
    }
    if cleaned.chars().all(|c| !c.is_alphanumeric()) {
        return String::new();
    }
    cleaned
}

fn remove_spans(text: &str, open: char, close: char, remove: impl Fn(&str) -> bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        let Some(len) = rest[start + open.len_utf8()..].find(close) else {
            break;
        };
        let inner = &rest[start + open.len_utf8()..start + open.len_utf8() + len];
        let end = start + open.len_utf8() + len + close.len_utf8();
        out.push_str(&rest[..start]);
        if remove(inner) {
            out.push(' ');
        } else {
            out.push_str(&rest[start..end]);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn is_sound_description(inner: &str) -> bool {
    let lower = inner.trim().to_lowercase();
    lower.split_whitespace().count() <= 4
        && SOUND_WORDS.iter().any(|word| {
            if word.contains(' ') {
                lower.contains(word)
            } else {
                lower
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|w| w == *word)
            }
        })
}

fn is_silence_hallucination(text: &str) -> bool {
    let normalised: String = text
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .to_lowercase();
    let normalised = normalised.split_whitespace().collect::<Vec<_>>().join(" ");
    SILENCE_HALLUCINATIONS.contains(&normalised.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bracketed_annotations_go() {
        assert_eq!(clean_transcript("[BLANK_AUDIO]", true), "");
        assert_eq!(
            clean_transcript(" [Music] Hello there. [MUSIC]", true),
            "Hello there."
        );
        assert_eq!(clean_transcript("Hi [ Silence ] you", true), "Hi you");
    }

    #[test]
    fn sound_descriptions_go_but_real_parentheses_stay() {
        assert_eq!(clean_transcript("(music)", true), "");
        assert_eq!(
            clean_transcript("(upbeat music) Let's go.", true),
            "Let's go."
        );
        assert_eq!(
            clean_transcript("Sure (laughs) why not", true),
            "Sure why not"
        );
        assert_eq!(clean_transcript("(speaking in foreign language)", true), "");
        assert_eq!(
            clean_transcript("The deadline (Friday) holds.", true),
            "The deadline (Friday) holds."
        );
        assert_eq!(
            clean_transcript(
                "Play the song (it has great music in the chorus) again",
                true
            ),
            "Play the song (it has great music in the chorus) again"
        );
    }

    #[test]
    fn music_notes_go() {
        assert_eq!(clean_transcript("♪ ♪", true), "");
        assert_eq!(clean_transcript("♪ la la ♪", true), "la la");
    }

    #[test]
    fn silence_hallucinations_go_only_without_speech() {
        for text in [
            " you",
            "Thank you.",
            "Thanks for watching!",
            " Bye.",
            "Subtitles by the Amara.org community",
        ] {
            assert_eq!(clean_transcript(text, false), "", "{text}");
        }
        assert_eq!(clean_transcript("Thank you.", true), "Thank you.");
        assert_eq!(
            clean_transcript("Thanks for watching the kids today.", false),
            "Thanks for watching the kids today."
        );
    }

    #[test]
    fn punctuation_only_is_empty() {
        assert_eq!(clean_transcript(" . ", true), "");
        assert_eq!(clean_transcript("...", true), "");
    }

    #[test]
    fn whitespace_is_collapsed_and_trimmed() {
        assert_eq!(
            clean_transcript("  Hello   world. \n Next ", true),
            "Hello world. Next"
        );
    }

    #[test]
    fn unclosed_brackets_are_left_alone() {
        assert_eq!(clean_transcript("a [b", true), "a [b");
    }
}
