//! Text: cleanup levels, writing styles, dictionary, snippets, app categories,
//! and the transcript → final text pipeline.

mod dictionary;
mod fillers;
pub mod pipeline;
pub mod polish;
mod snippets;
mod spoken;
mod style;
mod words;

use serde::{Deserialize, Serialize};

pub use dictionary::{WHISPER_PROMPT_BYTES, validate_dictionary_entry, vocabulary_prompt};
pub use snippets::{CheckedSnippet, validate_snippet};
pub use words::count_words;

use crate::Millis;

/// Max length of a dictionary word or a snippet trigger.
pub const MAX_PHRASE_CHARS: usize = 60;
/// Max length of a snippet expansion.
pub const MAX_EXPANSION_CHARS: usize = 4_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CleanupLevel {
    /// The raw transcript.
    None,
    /// Removes fillers, fixes grammar and punctuation.
    #[default]
    Light,
    /// Also rewords for clarity and concision.
    Medium,
}

/// The four style tabs on the Style page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StyleCategory {
    Personal,
    Work,
    Email,
    Other,
}

impl StyleCategory {
    pub const ALL: [Self; 4] = [Self::Personal, Self::Work, Self::Email, Self::Other];

    /// Messaging categories drop the final period in Casual and very casual.
    pub fn is_messaging(self) -> bool {
        matches!(self, Self::Personal | Self::Work)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WritingStyle {
    /// Full capitalisation and punctuation.
    #[default]
    Formal,
    /// Capitalisation kept, lighter punctuation.
    Casual,
    /// No capitalisation, lighter punctuation. Personal messages only.
    VeryCasual,
    /// More exclamation marks. Work, Email and Other only.
    Excited,
}

impl WritingStyle {
    pub fn available_in(self, category: StyleCategory) -> bool {
        match self {
            Self::Formal | Self::Casual => true,
            Self::VeryCasual => category == StyleCategory::Personal,
            Self::Excited => category != StyleCategory::Personal,
        }
    }
}

/// The style chosen for each category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Styles {
    pub personal: WritingStyle,
    pub work: WritingStyle,
    pub email: WritingStyle,
    pub other: WritingStyle,
}

impl Styles {
    pub fn get(&self, category: StyleCategory) -> WritingStyle {
        match category {
            StyleCategory::Personal => self.personal,
            StyleCategory::Work => self.work,
            StyleCategory::Email => self.email,
            StyleCategory::Other => self.other,
        }
    }
}

/// What kind of app has focus. Finer than [`StyleCategory`] so the mapping can
/// change without re-detecting apps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppCategory {
    PersonalMessaging,
    WorkMessaging,
    Email,
    Code,
    Documents,
    Browser,
    #[default]
    Other,
}

impl AppCategory {
    pub fn style_category(self) -> StyleCategory {
        match self {
            Self::PersonalMessaging => StyleCategory::Personal,
            Self::WorkMessaging => StyleCategory::Work,
            Self::Email => StyleCategory::Email,
            Self::Code | Self::Documents | Self::Browser | Self::Other => StyleCategory::Other,
        }
    }
}

/// The app that received (or was meant to receive) a dictation.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    /// macOS bundle id, Windows exe name, or Linux WM_CLASS.
    pub id: Option<String>,
    pub category: AppCategory,
}

/// A word the model should know, optionally replacing a misspelling of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryEntry {
    pub id: String,
    /// The correct spelling, kept as typed in the output.
    pub word: String,
    /// What Whisper tends to write instead (`wrong → word`).
    pub misspelling: Option<String>,
    pub created_at: Millis,
}

/// Argument of `upsert_dictionary_entry`; no `id` means a new entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryEntryInput {
    pub id: Option<String>,
    pub word: String,
    pub misspelling: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: String,
    pub trigger: String,
    pub expansion: String,
    pub created_at: Millis,
}

/// Argument of `upsert_snippet`; no `id` means a new snippet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetInput {
    pub id: Option<String>,
    pub trigger: String,
    pub expansion: String,
}

/// Why a dictionary word or snippet can't be saved. The messages are
/// interface copy.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InputError {
    #[error("Enter a {0}")]
    Empty(&'static str),
    #[error("Keep the {0} under 60 characters")]
    TooLong(&'static str),
    #[error("Use at least one letter or number in the {0}")]
    NoLetters(&'static str),
    #[error("The misspelling is the same as the word")]
    MisspellingIsWord,
    #[error("That's already in your dictionary")]
    DuplicateWord,
    #[error("That misspelling already has a correction")]
    DuplicateMisspelling,
    #[error("Enter the text to insert")]
    EmptyExpansion,
    #[error("Keep the text under 4,000 characters")]
    ExpansionTooLong,
    #[error("A snippet with that trigger already exists")]
    DuplicateTrigger,
    #[error("That trigger is already a dictionary correction")]
    TriggerIsCorrection,
}

/// A trimmed word or trigger: not empty, at most [`MAX_PHRASE_CHARS`], and
/// not only punctuation or symbols.
pub fn check_phrase(text: &str, what: &'static str) -> Result<String, InputError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(InputError::Empty(what));
    }
    if text.chars().count() > MAX_PHRASE_CHARS {
        return Err(InputError::TooLong(what));
    }
    if !text.chars().any(char::is_alphanumeric) {
        return Err(InputError::NoLetters(what));
    }
    Ok(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_copy_matches_limits() {
        assert!(
            InputError::TooLong("word")
                .to_string()
                .contains(&MAX_PHRASE_CHARS.to_string())
        );
        assert!(InputError::ExpansionTooLong.to_string().contains("4,000"));
        assert_eq!(MAX_EXPANSION_CHARS, 4_000);
    }

    #[test]
    fn defaults() {
        assert_eq!(CleanupLevel::default(), CleanupLevel::Light);
        let styles = Styles::default();
        for c in StyleCategory::ALL {
            assert_eq!(styles.get(c), WritingStyle::Formal);
        }
    }

    #[test]
    fn style_availability() {
        assert!(WritingStyle::VeryCasual.available_in(StyleCategory::Personal));
        assert!(!WritingStyle::VeryCasual.available_in(StyleCategory::Email));
        assert!(!WritingStyle::Excited.available_in(StyleCategory::Personal));
        assert!(WritingStyle::Excited.available_in(StyleCategory::Work));
    }

    #[test]
    fn serde_names() {
        assert_eq!(
            serde_json::to_string(&WritingStyle::VeryCasual).unwrap(),
            "\"veryCasual\""
        );
        assert_eq!(
            serde_json::to_string(&AppCategory::WorkMessaging).unwrap(),
            "\"workMessaging\""
        );
    }
}
