//! Rule-based steps of the dictation pipeline (spec §2.5 and §3).
//!
//! Order: `format_spoken` → `remove_fillers` → (LLM polish, outside this
//! module) → `apply_dictionary` → `expand_snippets` → `apply_style` →
//! whitespace. At [`CleanupLevel::None`] only the dictionary and snippets run.

use serde::{Deserialize, Serialize};

use super::{CleanupLevel, DictionaryEntry, Snippet, StyleCategory, WritingStyle};
use super::{dictionary, fillers, snippets, spoken, style};

pub use super::dictionary::apply_dictionary;
pub use super::fillers::remove_fillers;
pub use super::snippets::expand_snippets;
pub use super::spoken::format_spoken;
pub use super::style::{apply_style, apply_style_with};

/// Everything the rule-based pipeline needs for one dictation.
#[derive(Debug, Clone, Copy)]
pub struct PipelineContext<'a> {
    pub level: CleanupLevel,
    pub style: WritingStyle,
    pub category: StyleCategory,
    pub dictionary: &'a [DictionaryEntry],
    pub snippets: &'a [Snippet],
}

/// What the rule-based steps changed in one dictation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fixes {
    /// Spoken punctuation and line breaks turned into symbols.
    pub spoken_commands: u32,
    /// Fillers and stuttered words removed.
    pub fillers_removed: u32,
    /// Misspellings corrected and casing fixed from the dictionary.
    pub dictionary_fixes: u32,
    pub snippets_expanded: u32,
}

impl Fixes {
    /// The "fixes made" stat: words cleaned up and dictionary corrections.
    /// Commands and snippets are what the user asked for, not fixes.
    pub fn total(&self) -> u32 {
        self.fillers_removed + self.dictionary_fixes
    }

    fn and(self, other: Self) -> Self {
        Self {
            spoken_commands: self.spoken_commands + other.spoken_commands,
            fillers_removed: self.fillers_removed + other.fillers_removed,
            dictionary_fixes: self.dictionary_fixes + other.dictionary_fixes,
            snippets_expanded: self.snippets_expanded + other.snippets_expanded,
        }
    }
}

/// Text after some pipeline steps, with what they changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Processed {
    pub text: String,
    pub fixes: Fixes,
}

/// Steps that run before the LLM. Skipped entirely at [`CleanupLevel::None`].
pub fn before_polish_with_fixes(raw: &str, ctx: &PipelineContext<'_>) -> Processed {
    if ctx.level == CleanupLevel::None {
        return Processed {
            text: raw.to_owned(),
            fixes: Fixes::default(),
        };
    }
    let (text, spoken_commands) = spoken::format(raw);
    let (text, fillers_removed) = fillers::remove(&text);
    Processed {
        text,
        fixes: Fixes {
            spoken_commands,
            fillers_removed,
            ..Fixes::default()
        },
    }
}

/// Steps that run after the LLM (or straight after [`before_polish`]).
pub fn after_polish_with_fixes(text: &str, ctx: &PipelineContext<'_>) -> Processed {
    let (text, dictionary_fixes) = dictionary::apply(text, ctx.dictionary);
    if let Some(expansion) = snippets::alone(&text, ctx.snippets) {
        return Processed {
            text: expansion.to_owned(),
            fixes: Fixes {
                dictionary_fixes,
                snippets_expanded: 1,
                ..Fixes::default()
            },
        };
    }
    let marked = snippets::mark(&text, ctx.snippets);
    let text = if ctx.level == CleanupLevel::None {
        marked.text.clone()
    } else {
        let styled = style::apply_style_with(&marked.text, ctx.style, ctx.category, ctx.dictionary);
        normalize_whitespace(&styled)
    };
    Processed {
        text: marked.restore(&text),
        fixes: Fixes {
            dictionary_fixes,
            snippets_expanded: marked.count,
            ..Fixes::default()
        },
    }
}

/// The whole rule-based pipeline without an LLM.
pub fn run_with_fixes(raw: &str, ctx: &PipelineContext<'_>) -> Processed {
    let before = before_polish_with_fixes(raw, ctx);
    let after = after_polish_with_fixes(&before.text, ctx);
    Processed {
        text: after.text,
        fixes: before.fixes.and(after.fixes),
    }
}

pub fn before_polish(raw: &str, ctx: &PipelineContext<'_>) -> String {
    before_polish_with_fixes(raw, ctx).text
}

pub fn after_polish(text: &str, ctx: &PipelineContext<'_>) -> String {
    after_polish_with_fixes(text, ctx).text
}

/// Used by `test_polish`.
pub fn run(raw: &str, ctx: &PipelineContext<'_>) -> String {
    run_with_fixes(raw, ctx).text
}

/// Collapses runs of spaces inside a line and drops spaces at the start of
/// the text and at line ends. Line breaks and indentation stay.
fn normalize_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (n, line) in text.split('\n').enumerate() {
        if n > 0 {
            out.push('\n');
        }
        let body = line.trim_end_matches([' ', '\t']);
        let indent_len = body.len() - body.trim_start_matches([' ', '\t']).len();
        let (indent, rest) = body.split_at(if n == 0 { 0 } else { indent_len });
        out.push_str(indent);
        let mut prev_space = n == 0;
        for c in rest.chars() {
            let space = c == ' ' || c == '\t';
            if !(space && prev_space) {
                out.push(if space { ' ' } else { c });
            }
            prev_space = space;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::super::dictionary::tests::entry;
    use super::*;

    fn snippet(trigger: &str, expansion: &str) -> Snippet {
        Snippet {
            id: trigger.into(),
            trigger: trigger.into(),
            expansion: expansion.into(),
            created_at: 1,
        }
    }

    fn ctx<'a>(
        level: CleanupLevel,
        style: WritingStyle,
        category: StyleCategory,
        dictionary: &'a [DictionaryEntry],
        snippets: &'a [Snippet],
    ) -> PipelineContext<'a> {
        PipelineContext {
            level,
            style,
            category,
            dictionary,
            snippets,
        }
    }

    #[test]
    fn end_to_end() {
        use CleanupLevel::{Light, Medium, None as Raw};
        use StyleCategory::{Email, Other, Personal, Work};
        use WritingStyle::{Casual, Excited, Formal, VeryCasual};
        let dictionary = [
            entry("Kubernetes", Some("cooper netties"), 1),
            entry("Anna", None, 2),
        ];
        let snippets = [
            snippet("my sig", "Best,\nAmelia"),
            snippet("zoom link", "https://zoom.us/j/1  "),
            snippet("thx", "Thank you!"),
        ];
        let cases: &[(CleanupLevel, WritingStyle, StyleCategory, &str, &str)] = &[
            (Raw, Formal, Other, "um hello there", "um hello there"),
            (
                Raw,
                Casual,
                Personal,
                "Um, ask anna comma okay.",
                "Um, ask Anna comma okay.",
            ),
            (Raw, Formal, Other, "Zoom link.", "https://zoom.us/j/1  "),
            (
                Light,
                Formal,
                Other,
                "Um, hello comma how are you question mark",
                "Hello, how are you?",
            ),
            (
                Light,
                Casual,
                Personal,
                "Um, ask anna if she's free period",
                "Ask Anna if she's free",
            ),
            (
                Light,
                VeryCasual,
                Personal,
                "Hey anna. Are we still on for tonight?",
                "hey Anna. are we still on for tonight?",
            ),
            (
                Light,
                Excited,
                Work,
                "We, uh, deployed cooper netties period",
                "We deployed Kubernetes!",
            ),
            (
                Medium,
                Formal,
                Email,
                "Hi Anna comma new paragraph the the report is ready period new line my sig",
                "Hi Anna,\n\nThe report is ready.\nBest,\nAmelia",
            ),
            (
                Light,
                Casual,
                Work,
                "Here's the zoom link.",
                "Here's the https://zoom.us/j/1  ",
            ),
            (Light, Formal, Other, "My sig.", "Best,\nAmelia"),
            (
                Light,
                Formal,
                Other,
                "Fine  then , I   guess",
                "Fine then, I guess.",
            ),
            (Light, Formal, Other, "Thanks new line", "Thanks\n"),
            (Light, Formal, Other, "Okay, thx.", "Okay, Thank you!"),
            (
                Light,
                Excited,
                Email,
                "Thanks period new line my sig",
                "Thanks.\nBest,\nAmelia",
            ),
            (
                Light,
                Formal,
                Other,
                "Er ist nach Hause gegangen",
                "Er ist nach Hause gegangen.",
            ),
            (
                Light,
                Casual,
                Personal,
                "So, uh, the the cooper netties cluster is down period",
                "So the Kubernetes cluster is down",
            ),
            (
                Light,
                VeryCasual,
                Personal,
                "Um, I'll call anna later period",
                "I'll call Anna later",
            ),
            (
                Raw,
                Formal,
                Other,
                "Ask anna about cooper netties",
                "Ask Anna about Kubernetes",
            ),
            (
                Light,
                Formal,
                Email,
                "Hi Anna comma new paragraph thanks for the update period new paragraph Best comma new line Amelia",
                "Hi Anna,\n\nThanks for the update.\n\nBest,\nAmelia",
            ),
            (
                Medium,
                Casual,
                Work,
                "Can we, um, ship it today question mark",
                "Can we ship it today?",
            ),
            (
                Light,
                Excited,
                Work,
                "Um. We hit our target period",
                "We hit our target!",
            ),
            (Light, Formal, Other, "  hello   world  ", "hello world"),
            (
                Light,
                Formal,
                Other,
                "Remove the comma after hello",
                "Remove the comma after hello.",
            ),
        ];
        for &(level, style, category, input, want) in cases {
            let c = ctx(level, style, category, &dictionary, &snippets);
            assert_eq!(
                run(input, &c),
                want,
                "{level:?} {style:?} {category:?} {input:?}"
            );
        }
    }

    #[test]
    fn reports_fixes() {
        let dictionary = [entry("Kubernetes", Some("cooper netties"), 1)];
        let snippets = [snippet("my sig", "Best")];
        let c = ctx(
            CleanupLevel::Light,
            WritingStyle::Formal,
            StyleCategory::Other,
            &dictionary,
            &snippets,
        );
        let out = run_with_fixes("Um, the the cooper netties cluster comma my sig", &c);
        assert_eq!(out.text, "The Kubernetes cluster, Best");
        assert_eq!(
            out.fixes,
            Fixes {
                spoken_commands: 1,
                fillers_removed: 2,
                dictionary_fixes: 1,
                snippets_expanded: 1,
            }
        );
        assert_eq!(out.fixes.total(), 3);
    }

    #[test]
    fn idempotent() {
        let dictionary = [entry("Kubernetes", Some("cooper netties"), 1)];
        let inputs = [
            "Um, hello comma how are you question mark",
            "The trial period ends on Friday",
            "So, uh, we deployed cooper netties period new paragraph next steps colon testing",
            "I I think the the plan is fine. Hmm.",
            "Setz hier ein Komma, dann einen Punkt.",
            "Hi Anna comma new paragraph thanks for the update period",
            "Wait... what question mark",
            "Email me at john@example.com period",
            "Hey. How are you? Um, see you soon.",
            "Ça va. Très bien, merci.",
        ];
        for style in [
            WritingStyle::Formal,
            WritingStyle::Casual,
            WritingStyle::VeryCasual,
            WritingStyle::Excited,
        ] {
            for category in StyleCategory::ALL {
                let c = ctx(CleanupLevel::Light, style, category, &dictionary, &[]);
                for input in inputs {
                    let once = run(input, &c);
                    assert_eq!(run(&once, &c), once, "{style:?} {category:?} {input:?}");
                }
            }
        }
    }

    #[test]
    fn never_adds_trailing_whitespace() {
        let c = ctx(
            CleanupLevel::Light,
            WritingStyle::Casual,
            StyleCategory::Personal,
            &[],
            &[],
        );
        for input in ["Hello there.", "Hello um", "Hello comma", "Um."] {
            let out = run(input, &c);
            assert_eq!(out, out.trim_end(), "{input:?}");
        }
    }

    #[test]
    fn a_thousand_words_is_fast() {
        let dictionary: Vec<_> = (0..200)
            .map(|i| entry(&format!("Name{i}"), Some(&format!("nayme{i}")), i))
            .collect();
        let snippets: Vec<_> = (0..50)
            .map(|i| snippet(&format!("snip {i}"), "text"))
            .collect();
        let c = ctx(
            CleanupLevel::Light,
            WritingStyle::Casual,
            StyleCategory::Work,
            &dictionary,
            &snippets,
        );
        let sentence = "So, um, I think the the nayme7 cluster comma which runs fine period ";
        let text = sentence.repeat(1_000 / 12 + 1);
        let start = Instant::now();
        let out = run(&text, &c);
        assert!(
            start.elapsed() < Duration::from_millis(200),
            "{:?}",
            start.elapsed()
        );
        assert!(out.contains("Name7 cluster, which runs fine. So I think"));
    }
}
