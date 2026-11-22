//! Lexing and matching shared by the rule-based steps.

use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Word,
    Space,
    Punct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token<'a> {
    pub kind: Kind,
    pub text: Cow<'a, str>,
}

impl<'a> Token<'a> {
    pub fn new(kind: Kind, text: impl Into<Cow<'a, str>>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }

    pub fn is_word(&self) -> bool {
        self.kind == Kind::Word
    }

    pub fn is_space(&self) -> bool {
        self.kind == Kind::Space
    }

    pub fn is_newline(&self) -> bool {
        self.kind == Kind::Space && self.text.contains('\n')
    }

    /// A space or tab run without a line break.
    pub fn is_inline_space(&self) -> bool {
        self.kind == Kind::Space && !self.text.contains('\n')
    }

    pub fn is_punct(&self, set: &[char]) -> bool {
        self.kind == Kind::Punct && self.text.chars().next().is_some_and(|c| set.contains(&c))
    }

    pub fn lower(&self) -> String {
        self.text.to_lowercase()
    }
}

pub(crate) const SENTENCE_END: &[char] = &['.', '?', '!'];
pub(crate) const CLAUSE_MARKS: &[char] = &['.', ',', '?', '!', ':', ';'];

/// Letters, digits, `_` and combining marks (decomposed accents).
pub(crate) fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
        || c == '_'
        || matches!(c, '\u{0300}'..='\u{036F}' | '\u{1AB0}'..='\u{1AFF}' | '\u{1DC0}'..='\u{1DFF}' | '\u{20D0}'..='\u{20FF}' | '\u{FE20}'..='\u{FE2F}')
}

/// Han, kana and fullwidth letters: scripts written without spaces, where a
/// phrase can sit right next to other letters ("住所を教えて").
fn is_unspaced(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{30FF}' | '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{FF66}'..='\u{FF9F}')
}

/// `a` followed by `b` belong to the same word.
fn glued(a: char, b: char) -> bool {
    is_word_char(a) && is_word_char(b) && !is_unspaced(a) && !is_unspaced(b)
}

pub(crate) fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}')
}

fn is_joiner(c: char) -> bool {
    is_apostrophe(c) || matches!(c, '-' | '\u{2010}')
}

/// Splits text into words (with inner apostrophes and hyphens: "don't",
/// "mm-hmm"), whitespace runs and single punctuation characters. Joining the
/// tokens gives back the input.
pub(crate) fn tokenize(text: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        let mut end = start + c.len_utf8();
        let kind = if c.is_whitespace() {
            while let Some(&(i, c)) = chars.peek() {
                if !c.is_whitespace() {
                    break;
                }
                end = i + c.len_utf8();
                chars.next();
            }
            Kind::Space
        } else if is_word_char(c) {
            loop {
                match chars.peek().copied() {
                    Some((i, c)) if is_word_char(c) => {
                        end = i + c.len_utf8();
                        chars.next();
                    }
                    Some((i, c)) if is_joiner(c) => {
                        let next = text[i + c.len_utf8()..].chars().next();
                        if !next.is_some_and(is_word_char) {
                            break;
                        }
                        end = i + c.len_utf8();
                        chars.next();
                    }
                    _ => break,
                }
            }
            Kind::Word
        } else {
            Kind::Punct
        };
        tokens.push(Token::new(kind, &text[start..end]));
    }
    tokens
}

/// Words in a dictation, for stats. Each Han or kana character counts as a
/// word, since those scripts don't separate words with spaces.
pub fn count_words(text: &str) -> u32 {
    let count: usize = tokenize(text)
        .iter()
        .filter(|t| t.is_word())
        .map(|t| {
            let unspaced = t.text.chars().filter(|&c| is_unspaced(c)).count();
            unspaced + usize::from(t.text.chars().any(|c| !is_unspaced(c)))
        })
        .sum();
    u32::try_from(count).unwrap_or(u32::MAX)
}

pub(crate) fn join(tokens: &[Token<'_>]) -> String {
    tokens.iter().map(|t| t.text.as_ref()).collect()
}

/// "hello" → "Hello". Words that already contain a capital ("iPhone") are
/// left alone.
pub(crate) fn capitalize(word: &str) -> Cow<'_, str> {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) if first.is_lowercase() && !word.chars().any(char::is_uppercase) => {
            Cow::Owned(first.to_uppercase().chain(chars).collect())
        }
        _ => Cow::Borrowed(word),
    }
}

/// "Hello" → "hello". Keeps "I" and its contractions, acronyms and words
/// with inner capitals ("NASA", "McDonald").
pub(crate) fn decapitalize(word: &str) -> Cow<'_, str> {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return Cow::Borrowed(word);
    };
    let rest = chars.as_str();
    let is_i = first == 'I' && (rest.is_empty() || rest.starts_with(is_apostrophe));
    if !first.is_uppercase() || is_i || rest.chars().any(char::is_uppercase) {
        return Cow::Borrowed(word);
    }
    Cow::Owned(first.to_lowercase().chain(rest.chars()).collect())
}

/// A phrase matched whole-word and case-insensitively anywhere in a text.
/// Spaces and hyphens in the phrase match either in the text.
#[derive(Debug, Clone)]
pub(crate) struct Phrase<'a> {
    folded: Vec<char>,
    pub replacement: &'a str,
}

const SEPARATOR: char = ' ';

impl<'a> Phrase<'a> {
    /// `None` for a phrase with nothing to match.
    pub fn new(phrase: &str, replacement: &'a str) -> Option<Self> {
        let mut folded = Vec::new();
        for c in phrase.trim().chars() {
            if c.is_whitespace() || matches!(c, '-' | '\u{2010}') {
                if folded.last() != Some(&SEPARATOR) {
                    folded.push(SEPARATOR);
                }
            } else {
                folded.extend(c.to_lowercase());
            }
        }
        while folded.last() == Some(&SEPARATOR) {
            folded.pop();
        }
        (!folded.is_empty() && folded[0] != SEPARATOR).then_some(Self {
            folded,
            replacement,
        })
    }

    /// The phrase is all of `text`.
    pub fn matches_whole(&self, text: &str) -> bool {
        self.match_at(text, 0) == Some(text.len())
    }

    /// The end of a match starting at `start`, if the phrase matches there.
    fn match_at(&self, text: &str, start: usize) -> Option<usize> {
        let mut pattern = self.folded.iter().peekable();
        let mut chars = text[start..].char_indices().peekable();
        while let Some(&expected) = pattern.next() {
            if expected == SEPARATOR {
                let mut any = false;
                while let Some(&(_, c)) = chars.peek() {
                    if c != '\n' && (c.is_whitespace() || matches!(c, '-' | '\u{2010}')) {
                        any = true;
                        chars.next();
                    } else {
                        break;
                    }
                }
                if !any {
                    return None;
                }
                continue;
            }
            let (_, c) = chars.next()?;
            let mut lower = c.to_lowercase();
            if lower.next() != Some(expected) {
                return None;
            }
            for extra in lower {
                if pattern.next() != Some(&extra) {
                    return None;
                }
            }
        }
        let end = chars.peek().map_or(text.len(), |&(i, _)| start + i);
        let last = text[..end].chars().next_back()?;
        ends_word(last, &text[end..]).then_some(end)
    }
}

/// Whether a match may end right before `rest`: not attached to a word, but a
/// possessive "'s" may follow ("John's"); "n't" and other contractions may not.
fn ends_word(last: char, rest: &str) -> bool {
    let mut chars = rest.chars();
    match chars.next() {
        None => true,
        Some(c) if is_apostrophe(c) => match chars.next() {
            Some('s' | 'S') => !chars.next().is_some_and(is_word_char),
            Some(c) => !is_word_char(c),
            None => true,
        },
        Some(c) => !glued(last, c),
    }
}

/// Characters that glue words into a URL, email address or path.
fn in_address(c: char) -> bool {
    matches!(c, '@' | '/' | '\\' | '.' | '#')
}

/// `rest` carries on an address: "john@…", "john.com", "john/…", "host:8080".
fn continues_address(rest: &str) -> bool {
    let mut chars = rest.chars();
    match chars.next() {
        Some('@' | '/' | '\\') => true,
        Some('.' | ':') => chars.next().is_some_and(is_word_char),
        _ => false,
    }
}

/// Replaces every match of `phrases` in one left-to-right pass. At each
/// position the longest match wins, ties go to the earlier phrase, and
/// replaced text is never scanned again. Returns the text and how many
/// matches changed it.
pub(crate) fn replace_phrases(text: &str, phrases: &[Phrase<'_>]) -> (String, u32) {
    if phrases.is_empty() {
        return (text.to_owned(), 0);
    }
    let mut out = String::with_capacity(text.len());
    let mut changed = 0;
    let mut copied = 0;
    let mut prev: Option<char> = None;
    for (i, c) in text.char_indices() {
        if i < copied {
            prev = Some(c);
            continue;
        }
        if !prev.is_some_and(|p| glued(p, c) || in_address(p)) {
            let best = phrases
                .iter()
                .filter_map(|p| p.match_at(text, i).map(|end| (end, p)))
                .filter(|&(end, _)| !continues_address(&text[end..]))
                .fold(
                    None,
                    |best: Option<(usize, &Phrase<'_>)>, (end, p)| match best {
                        Some((best_end, _)) if best_end >= end => best,
                        _ => Some((end, p)),
                    },
                );
            if let Some((end, phrase)) = best {
                out.push_str(&text[copied..i]);
                out.push_str(phrase.replacement);
                if text[i..end] != *phrase.replacement {
                    changed += 1;
                }
                copied = end;
            }
        }
        prev = Some(c);
    }
    out.push_str(&text[copied..]);
    (out, changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_round_trips_and_keeps_inner_joiners() {
        for text in [
            "Hello, comma, how are you?",
            "Don't stop – mm-hmm… straße, café, niño!",
            "  two  spaces\n\nand a URL: https://example.com/a_b.",
            "",
        ] {
            assert_eq!(join(&tokenize(text)), text);
        }
        let words: Vec<String> = tokenize("I don't—uh-huh 'quote' rock-")
            .into_iter()
            .filter(Token::is_word)
            .map(|t| t.text.into_owned())
            .collect();
        assert_eq!(words, ["I", "don't", "uh-huh", "quote", "rock"]);
    }

    #[test]
    fn counts_words() {
        let cases = [
            ("", 0),
            ("  \n ", 0),
            ("Hello, world.", 2),
            ("I don't know — mm-hmm.", 4),
            ("Die Straße ist schön.", 4),
            ("¿Dónde está el niño?", 4),
            ("東京に行きます", 7),
            ("Use 住所 please", 4),
            ("Call me at 5 p.m.", 6),
        ];
        for (text, want) in cases {
            assert_eq!(count_words(text), want, "{text:?}");
        }
    }

    #[test]
    fn casing_helpers() {
        let cases = [
            ("hello", "Hello", "hello"),
            ("Hello", "Hello", "hello"),
            ("iPhone", "iPhone", "iPhone"),
            ("NASA", "NASA", "NASA"),
            ("I", "I", "I"),
            ("I'm", "I'm", "I'm"),
            ("Ich", "Ich", "ich"),
            ("élan", "Élan", "élan"),
            ("Ñandú", "Ñandú", "ñandú"),
        ];
        for (word, up, down) in cases {
            assert_eq!(capitalize(word), up, "{word}");
            assert_eq!(decapitalize(word), down, "{word}");
        }
    }

    #[test]
    fn phrase_matching() {
        let p = |s| Phrase::new(s, "X").unwrap();
        let cases: &[(&str, &str, &str)] = &[
            ("tiny wispr", "I use tiny-wispr daily", "I use X daily"),
            ("tiny wispr", "I use TINY  WISPR.", "I use X."),
            ("john", "John's car", "X's car"),
            ("john", "John’s car", "X’s car"),
            ("john", "Johnny and John", "Johnny and X"),
            ("don", "I don't know Don", "I don't know X"),
            ("straße", "Die STRAßE ist lang", "Die X ist lang"),
            ("café", "un Café, s'il vous plaît", "un X, s'il vous plaît"),
            ("x", "x_y x", "x_y X"),
        ];
        for &(phrase, text, want) in cases {
            assert_eq!(
                replace_phrases(text, &[p(phrase)]).0,
                want,
                "{phrase} in {text}"
            );
        }
        assert!(Phrase::new("  ", "X").is_none());
    }
}
