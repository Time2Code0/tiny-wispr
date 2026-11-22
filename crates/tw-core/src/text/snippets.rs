//! Snippets: a spoken trigger replaced by saved text.

use super::dictionary::newest_first;
use super::words::{Phrase, is_word_char, replace_phrases};
use super::{
    DictionaryEntry, InputError, MAX_EXPANSION_CHARS, Snippet, SnippetInput, check_phrase,
};

/// Expansions stand in the text as private-use characters while the later
/// steps run, so style and whitespace rules never touch them.
const PLACEHOLDER_BASE: u32 = 0xF_0000;
const PLACEHOLDER_LIMIT: usize = 0xFFFD;

/// A snippet expansion standing in the text: the final period after it can
/// go like after a word.
pub(crate) fn is_placeholder(c: char) -> bool {
    (PLACEHOLDER_BASE..PLACEHOLDER_BASE + PLACEHOLDER_LIMIT as u32).contains(&(c as u32))
}

pub(crate) struct Marked<'a> {
    pub text: String,
    pub expansions: Vec<&'a str>,
    pub count: u32,
}

impl Marked<'_> {
    /// Puts the expansions back. Whisper's period after a trigger goes when
    /// the expansion already ends a sentence ("Thanks!" not "Thanks!.").
    pub fn restore(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            let index = (c as u32).checked_sub(PLACEHOLDER_BASE).map(|i| i as usize);
            match index.and_then(|i| self.expansions.get(i)) {
                Some(expansion) => {
                    out.push_str(expansion);
                    if expansion.ends_with(['.', '!', '?']) && chars.peek() == Some(&'.') {
                        chars.next();
                    }
                }
                None => out.push(c),
            }
        }
        out
    }
}

fn usable(snippets: &[Snippet]) -> Vec<&Snippet> {
    newest_first(snippets, |s| s.created_at)
        .into_iter()
        .filter(|s| !s.trigger.trim().is_empty())
        .take(PLACEHOLDER_LIMIT)
        .collect()
}

/// The expansion when the whole dictation is just a trigger, ignoring the
/// whitespace and punctuation around it ("Calendar link.").
pub(crate) fn alone<'a>(text: &str, snippets: &'a [Snippet]) -> Option<&'a str> {
    let edge = |c: char| !is_word_char(c);
    let spoken = text.trim_matches(edge);
    if spoken.is_empty() {
        return None;
    }
    usable(snippets).into_iter().find_map(|s| {
        let phrase = Phrase::new(s.trigger.trim_matches(edge), "")?;
        phrase.matches_whole(spoken).then_some(s.expansion.as_str())
    })
}

/// Replaces triggers inside the text with placeholders.
pub(crate) fn mark<'a>(text: &str, snippets: &'a [Snippet]) -> Marked<'a> {
    let snippets = usable(snippets);
    let placeholders: Vec<String> = (0..snippets.len())
        .map(|i| {
            char::from_u32(PLACEHOLDER_BASE + i as u32)
                .map(String::from)
                .unwrap_or_default()
        })
        .collect();
    let phrases: Vec<Phrase<'_>> = snippets
        .iter()
        .zip(&placeholders)
        .filter_map(|(s, p)| Phrase::new(&s.trigger, p))
        .collect();
    let (text, count) = replace_phrases(text, &phrases);
    Marked {
        text,
        expansions: snippets.iter().map(|s| s.expansion.as_str()).collect(),
        count,
    }
}

/// Expands every trigger; a dictation that is only a trigger becomes exactly
/// the expansion.
pub fn expand_snippets(text: &str, snippets: &[Snippet]) -> String {
    if let Some(expansion) = alone(text, snippets) {
        return expansion.to_owned();
    }
    let marked = mark(text, snippets);
    marked.restore(&marked.text)
}

/// A snippet that passed the checks, and another trigger close enough to
/// confuse (one contains the other, or they differ only in punctuation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedSnippet {
    pub snippet: SnippetInput,
    pub similar_trigger: Option<String>,
}

/// Trims and checks a snippet before it is saved. The expansion is kept
/// exactly as typed.
pub fn validate_snippet(
    input: &SnippetInput,
    snippets: &[Snippet],
    dictionary: &[DictionaryEntry],
) -> Result<CheckedSnippet, InputError> {
    let trigger = check_phrase(&input.trigger, "trigger")?;
    if input.expansion.trim().is_empty() {
        return Err(InputError::EmptyExpansion);
    }
    if input.expansion.chars().count() > MAX_EXPANSION_CHARS {
        return Err(InputError::ExpansionTooLong);
    }
    let key = Phrase::new(&trigger, "");
    let same = |other: &str| key.as_ref().is_some_and(|k| k.matches_whole(other.trim()));
    let others: Vec<&Snippet> = snippets
        .iter()
        .filter(|s| Some(&s.id) != input.id.as_ref())
        .collect();
    if others.iter().any(|s| same(&s.trigger)) {
        return Err(InputError::DuplicateTrigger);
    }
    if dictionary
        .iter()
        .any(|e| e.misspelling.as_deref().is_some_and(same))
    {
        return Err(InputError::TriggerIsCorrection);
    }
    let letters = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let contains = |outer: &str, inner: &str| {
        Phrase::new(inner, "").is_some_and(|p| replace_phrases(outer, &[p]).1 > 0)
    };
    let similar_trigger = others
        .iter()
        .find(|s| {
            letters(&s.trigger) == letters(&trigger)
                || contains(&s.trigger, &trigger)
                || contains(&trigger, &s.trigger)
        })
        .map(|s| s.trigger.clone());
    Ok(CheckedSnippet {
        snippet: SnippetInput {
            id: input.id.clone(),
            trigger,
            expansion: input.expansion.clone(),
        },
        similar_trigger,
    })
}

#[cfg(test)]
mod tests {
    use super::super::dictionary::tests::entry;
    use super::*;

    fn snippet(trigger: &str, expansion: &str, created_at: i64) -> Snippet {
        Snippet {
            id: format!("{trigger}-{created_at}"),
            trigger: trigger.into(),
            expansion: expansion.into(),
            created_at,
        }
    }

    fn library() -> Vec<Snippet> {
        vec![
            snippet("my address", "221B Baker Street, London", 1),
            snippet("my address at work", "1 Main St, Springfield", 2),
            snippet("calendar link", "https://cal.example.com/amelia", 3),
            snippet("sig", "Best,\n  Amelia  ", 4),
            snippet("", "never", 5),
            snippet("mi dirección", "Calle Mayor 1, Madrid", 6),
            snippet("住所", "東京都千代田区", 7),
            snippet("brb", "be right back", 8),
        ]
    }

    #[test]
    fn trigger_alone_is_exactly_the_expansion() {
        let cases = [
            ("Calendar link.", "https://cal.example.com/amelia"),
            ("  calendar link  ", "https://cal.example.com/amelia"),
            ("\"Calendar link!\"", "https://cal.example.com/amelia"),
            ("Sig.", "Best,\n  Amelia  "),
            ("Mi dirección.", "Calle Mayor 1, Madrid"),
            ("住所。", "東京都千代田区"),
            ("My address at work.", "1 Main St, Springfield"),
        ];
        for (input, want) in cases {
            assert_eq!(expand_snippets(input, &library()), want, "{input}");
        }
    }

    #[test]
    fn triggers_inside_sentences() {
        let cases = [
            (
                "Send it to my address please.",
                "Send it to 221B Baker Street, London please.",
            ),
            (
                "Send it to My Address.",
                "Send it to 221B Baker Street, London.",
            ),
            (
                "Use my address at work, thanks.",
                "Use 1 Main St, Springfield, thanks.",
            ),
            (
                "Here's the calendar link: ok",
                "Here's the https://cal.example.com/amelia: ok",
            ),
            ("Ok brb, brb.", "Ok be right back, be right back."),
            ("The brb's meaning", "The be right back's meaning"),
            ("brbs and xbrb and brb_x", "brbs and xbrb and brb_x"),
            ("brb2 stays", "brb2 stays"),
            (
                "Envíalo a mi dirección hoy.",
                "Envíalo a Calle Mayor 1, Madrid hoy.",
            ),
            ("Hallo, wie geht's?", "Hallo, wie geht's?"),
            ("Email me at brb@x.com", "Email me at brb@x.com"),
            ("Open brb.html now", "Open brb.html now"),
            (
                "Calendar link, calendar link.",
                "https://cal.example.com/amelia, https://cal.example.com/amelia.",
            ),
            (
                "Thanks for the calendar link!",
                "Thanks for the https://cal.example.com/amelia!",
            ),
            ("住所を教えて", "東京都千代田区を教えて"),
            ("My sig is below.", "My Best,\n  Amelia   is below."),
            ("Signing off. Sig", "Signing off. Best,\n  Amelia  "),
            ("Brb\nbrb", "be right back\nbe right back"),
            ("\"brb\" she said", "\"be right back\" she said"),
            (
                "BRB, my address.",
                "be right back, 221B Baker Street, London.",
            ),
        ];
        for (input, want) in cases {
            assert_eq!(expand_snippets(input, &library()), want, "{input}");
        }
    }

    #[test]
    fn expansions_are_not_rescanned() {
        let loop_ = [snippet("ping", "pong ping", 1), snippet("pong", "ping", 2)];
        assert_eq!(
            expand_snippets("ping and pong", &loop_),
            "pong ping and ping"
        );
        assert_eq!(mark("ping pong", &loop_).count, 2);
        let ends = [snippet("thx", "Thank you!", 1)];
        assert_eq!(expand_snippets("Okay, thx.", &ends), "Okay, Thank you!");
        assert_eq!(expand_snippets("thx...", &ends), "Thank you!");
    }

    #[test]
    fn validation() {
        let snippets = library();
        let dictionary = [entry("Kubernetes", Some("cooper netties"), 1)];
        let input = |trigger: &str, expansion: &str| SnippetInput {
            id: None,
            trigger: trigger.into(),
            expansion: expansion.into(),
        };
        let check = |trigger: &str, expansion: &str| {
            validate_snippet(&input(trigger, expansion), &snippets, &dictionary)
        };
        assert_eq!(check("", "x").unwrap_err(), InputError::Empty("trigger"));
        assert_eq!(
            check("...", "x").unwrap_err(),
            InputError::NoLetters("trigger")
        );
        assert_eq!(check("hi", "  ").unwrap_err(), InputError::EmptyExpansion);
        let long = "x".repeat(MAX_EXPANSION_CHARS + 1);
        assert_eq!(
            check("hi", &long).unwrap_err(),
            InputError::ExpansionTooLong
        );
        assert!(check("hi", &"x".repeat(MAX_EXPANSION_CHARS)).is_ok());
        assert_eq!(
            check(" BRB ", "x").unwrap_err(),
            InputError::DuplicateTrigger
        );
        assert_eq!(
            check("my-address", "x").unwrap_err(),
            InputError::DuplicateTrigger
        );
        assert_eq!(
            check("Cooper Netties", "x").unwrap_err(),
            InputError::TriggerIsCorrection
        );
        let ok = check("  thanks ", " Thank you! ").unwrap();
        assert_eq!(ok.snippet.trigger, "thanks");
        assert_eq!(ok.snippet.expansion, " Thank you! ");
        assert_eq!(ok.similar_trigger, None);
        assert_eq!(
            check("b.r.b", "x").unwrap().similar_trigger.as_deref(),
            Some("brb")
        );
        assert_eq!(
            check("address", "x").unwrap().similar_trigger.as_deref(),
            Some("my address")
        );
        assert_eq!(
            check("my address at home", "x")
                .unwrap()
                .similar_trigger
                .as_deref(),
            Some("my address")
        );
        let editing = SnippetInput {
            id: Some("brb-8".into()),
            ..input("brb", "be back soon")
        };
        assert!(validate_snippet(&editing, &snippets, &dictionary).is_ok());
    }
}
