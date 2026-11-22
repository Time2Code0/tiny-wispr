//! Dictionary corrections, casing of known words, the Whisper vocabulary
//! prompt and input checks for the Dictionary page.

use super::words::{Phrase, Token, replace_phrases, tokenize};
use super::{DictionaryEntry, DictionaryEntryInput, InputError, check_phrase};

/// A conservative budget for Whisper's initial prompt, which holds at most
/// 224 tokens; names and jargon average under three bytes per token.
pub const WHISPER_PROMPT_BYTES: usize = 600;

/// Entries newest first: the most recently saved wins ties.
pub(crate) fn newest_first<T>(items: &[T], created_at: impl Fn(&T) -> i64) -> Vec<&T> {
    let mut sorted: Vec<(usize, &T)> = items.iter().enumerate().collect();
    sorted.sort_by(|(ia, a), (ib, b)| created_at(b).cmp(&created_at(a)).then(ib.cmp(ia)));
    sorted.into_iter().map(|(_, item)| item).collect()
}

/// Replaces misspellings with the saved word and fixes the casing of every
/// saved word. An explicit correction beats another entry's casing. Returns the text and how many matches changed it.
pub(crate) fn apply(text: &str, entries: &[DictionaryEntry]) -> (String, u32) {
    let entries = newest_first(entries, |e| e.created_at);
    let corrections = entries
        .iter()
        .filter_map(|e| Phrase::new(e.misspelling.as_deref()?, e.word.trim()));
    let casing = entries
        .iter()
        .filter_map(|e| Phrase::new(&e.word, e.word.trim()));
    let phrases: Vec<Phrase<'_>> = corrections.chain(casing).collect();
    replace_phrases(text, &phrases)
}

pub fn apply_dictionary(text: &str, entries: &[DictionaryEntry]) -> String {
    apply(text, entries).0
}

/// An initial prompt that teaches Whisper the saved words, newest first,
/// within `max_bytes`. Framed as a labelled sentence: a bare list makes
/// small models treat speech that repeats it as already transcribed.
pub fn vocabulary_prompt(entries: &[DictionaryEntry], max_bytes: usize) -> Option<String> {
    const PREFIX: &str = "Vocabulary: ";
    let mut prompt = String::from(PREFIX);
    let mut seen: Vec<String> = Vec::new();
    for entry in newest_first(entries, |e| e.created_at) {
        let word = entry.word.trim();
        let key = word.to_lowercase();
        if word.is_empty() || seen.contains(&key) {
            continue;
        }
        let separator = if seen.is_empty() { "" } else { ", " };
        if prompt.len() + separator.len() + word.len() + 1 > max_bytes {
            continue;
        }
        prompt.push_str(separator);
        prompt.push_str(word);
        seen.push(key);
    }
    if seen.is_empty() {
        return None;
    }
    prompt.push('.');
    Some(prompt)
}

/// Words of saved entries that carry capitals ("Tiny", "Wispr", "NASA"):
/// proper nouns that keep their capital in every style.
pub(crate) fn proper_nouns(entries: &[DictionaryEntry]) -> Vec<String> {
    entries
        .iter()
        .flat_map(|e| {
            tokenize(&e.word)
                .into_iter()
                .filter(Token::is_word)
                .map(|t| t.text.into_owned())
        })
        .filter(|w| w.chars().any(char::is_uppercase))
        .collect()
}

/// Trims and checks a dictionary entry before it is saved. A blank
/// misspelling means vocabulary only.
pub fn validate_dictionary_entry(
    input: &DictionaryEntryInput,
    entries: &[DictionaryEntry],
) -> Result<DictionaryEntryInput, InputError> {
    let word = check_phrase(&input.word, "word")?;
    let misspelling = match input.misspelling.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(wrong) => Some(check_phrase(wrong, "misspelling")?),
    };
    let same = |a: &str, b: &str| a.to_lowercase() == b.to_lowercase();
    if misspelling
        .as_deref()
        .is_some_and(|wrong| same(wrong, &word))
    {
        return Err(InputError::MisspellingIsWord);
    }
    let others = entries.iter().filter(|e| Some(&e.id) != input.id.as_ref());
    for other in others {
        let other_wrong = other.misspelling.as_deref();
        if same(&other.word, &word)
            && other_wrong.map(str::to_lowercase) == misspelling.as_deref().map(str::to_lowercase)
        {
            return Err(InputError::DuplicateWord);
        }
        if let (Some(a), Some(b)) = (other_wrong, misspelling.as_deref())
            && same(a, b)
        {
            return Err(InputError::DuplicateMisspelling);
        }
    }
    Ok(DictionaryEntryInput {
        id: input.id.clone(),
        word,
        misspelling,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn entry(word: &str, wrong: Option<&str>, created_at: i64) -> DictionaryEntry {
        DictionaryEntry {
            id: format!("{word}-{created_at}"),
            word: word.into(),
            misspelling: wrong.map(Into::into),
            created_at,
        }
    }

    #[test]
    fn corrections_and_casing() {
        let entries = [
            entry("Bob", Some("John"), 1),
            entry("Tiny Wispr", None, 2),
            entry("Kubernetes", Some("cooper netties"), 3),
            entry("Kubernetes cluster", Some("cooper netties cluster"), 4),
            entry("NASA", None, 5),
            entry("Zoë", Some("Zoey"), 6),
            entry("Dawn", Some("Don"), 7),
            entry("e-mail", Some("electronic mail"), 8),
            entry("Müller", Some("Mueller"), 9),
            entry("José", Some("jose"), 10),
            entry("github", None, 11),
            entry("GitHub", None, 12),
        ];
        let cases = [
            ("John is here.", "Bob is here."),
            ("I saw john today", "I saw Bob today"),
            ("JOHN!", "Bob!"),
            ("That's John's car.", "That's Bob's car."),
            ("That's John’s car.", "That's Bob’s car."),
            ("Johnny and Johnson stay", "Johnny and Johnson stay"),
            ("(John), \"John\"", "(Bob), \"Bob\""),
            ("I love tiny wispr.", "I love Tiny Wispr."),
            ("I love tiny-wispr.", "I love Tiny Wispr."),
            ("tinywispr stays", "tinywispr stays"),
            ("Deploy cooper netties.", "Deploy Kubernetes."),
            ("A cooper netties cluster.", "A Kubernetes cluster."),
            ("Ask nasa about it", "Ask NASA about it"),
            ("Zoey says hi", "Zoë says hi"),
            ("I don't know, Don.", "I don't know, Dawn."),
            ("Send an electronic mail", "Send an e-mail"),
            ("Bob already right", "Bob already right"),
            (
                "Email john@example.com or visit john.com/john today",
                "Email john@example.com or visit john.com/john today",
            ),
            ("See ~/john/notes", "See ~/john/notes"),
            ("John: call me.", "Bob: call me."),
            ("The JOHN's list", "The Bob's list"),
            ("We run Cooper-Netties.", "We run Kubernetes."),
            ("cooper\nnetties", "cooper\nnetties"),
            ("Herr Mueller kommt.", "Herr Müller kommt."),
            ("Ask jose, then John.", "Ask José, then Bob."),
            ("Push it to github.", "Push it to GitHub."),
            ("Johnson's and John's", "Johnson's and Bob's"),
            ("John'll do it", "John'll do it"),
        ];
        for (input, want) in cases {
            assert_eq!(apply_dictionary(input, &entries), want, "{input}");
        }
        assert_eq!(apply("John and john", &entries).1, 2);
        assert_eq!(apply("Bob", &entries).1, 0);
    }

    #[test]
    fn no_chains_and_newest_wins() {
        let chain = [entry("B", Some("A"), 1), entry("C", Some("B"), 2)];
        assert_eq!(apply_dictionary("A B", &chain), "B C");
        let dupes = [
            entry("Alice", Some("alis"), 1),
            entry("Alyce", Some("ALIS"), 2),
        ];
        assert_eq!(apply_dictionary("alis", &dupes), "Alyce");
        let swap = [entry("y", Some("x"), 1), entry("x", Some("y"), 2)];
        assert_eq!(apply_dictionary("x y", &swap), "y x");
    }

    #[test]
    fn prompt_is_newest_first_and_bounded() {
        let entries = [
            entry("Amelia", None, 1),
            entry("Tiny Wispr", None, 3),
            entry("tiny wispr", None, 2),
            entry("Kubernetes", Some("cooper netties"), 4),
        ];
        assert_eq!(
            vocabulary_prompt(&entries, WHISPER_PROMPT_BYTES).as_deref(),
            Some("Vocabulary: Kubernetes, Tiny Wispr, Amelia.")
        );
        assert_eq!(
            vocabulary_prompt(&entries, 31).as_deref(),
            Some("Vocabulary: Kubernetes, Amelia.")
        );
        assert_eq!(vocabulary_prompt(&[], WHISPER_PROMPT_BYTES), None);
        let many: Vec<_> = (0..500)
            .map(|i| entry(&format!("Word{i}"), None, i))
            .collect();
        let prompt = vocabulary_prompt(&many, WHISPER_PROMPT_BYTES).unwrap_or_default();
        assert!(prompt.len() <= WHISPER_PROMPT_BYTES);
        assert!(prompt.starts_with("Vocabulary: Word499, Word498"));
    }

    #[test]
    fn validation() {
        let entries = [entry("Bob", Some("John"), 1), entry("Tiny Wispr", None, 2)];
        let input = |word: &str, wrong: Option<&str>| DictionaryEntryInput {
            id: None,
            word: word.into(),
            misspelling: wrong.map(Into::into),
        };
        type Case<'a> = (
            &'a str,
            Option<&'a str>,
            Result<(&'a str, Option<&'a str>), InputError>,
        );
        let cases: &[Case<'_>] = &[
            ("  Anna  ", Some("  "), Ok(("Anna", None))),
            ("Anna", Some(" Ana "), Ok(("Anna", Some("Ana")))),
            ("Bob", None, Ok(("Bob", None))),
            ("", None, Err(InputError::Empty("word"))),
            ("   ", None, Err(InputError::Empty("word"))),
            ("?!…", None, Err(InputError::NoLetters("word"))),
            (
                "Anna",
                Some("--"),
                Err(InputError::NoLetters("misspelling")),
            ),
            ("Anna", Some("anna"), Err(InputError::MisspellingIsWord)),
            ("bob", Some("john"), Err(InputError::DuplicateWord)),
            ("tiny wispr", None, Err(InputError::DuplicateWord)),
            ("Rob", Some("JOHN"), Err(InputError::DuplicateMisspelling)),
            ("Straße", None, Ok(("Straße", None))),
        ];
        for (word, wrong, want) in cases {
            let got = validate_dictionary_entry(&input(word, *wrong), &entries)
                .map(|e| (e.word, e.misspelling));
            let want = want
                .clone()
                .map(|(w, m)| (w.to_owned(), m.map(str::to_owned)));
            assert_eq!(got, want, "{word:?}");
        }
        let long = "x".repeat(super::super::MAX_PHRASE_CHARS + 1);
        let err = validate_dictionary_entry(&input(&long, None), &entries).unwrap_err();
        assert_eq!(err, InputError::TooLong("word"));
        assert_eq!(err.to_string(), "Keep the word under 60 characters");
        let editing = DictionaryEntryInput {
            id: Some("Bob-1".into()),
            word: "Bob".into(),
            misspelling: Some("Jon".into()),
        };
        assert!(validate_dictionary_entry(&editing, &entries).is_ok());
    }
}
