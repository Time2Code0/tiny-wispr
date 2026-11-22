//! Writing styles: capitals and final punctuation per app category.

use super::dictionary::proper_nouns;
use super::snippets::is_placeholder;
use super::words::{Kind, SENTENCE_END, Token, decapitalize, join, tokenize};
use super::{DictionaryEntry, StyleCategory, WritingStyle};

/// Casual styles drop the final period only in short messages.
const MAX_CASUAL_SENTENCES: usize = 10;
/// A final period after these belongs to the word.
const ABBREVIATIONS: &[&str] = &[
    "etc", "inc", "ltd", "corp", "jr", "sr", "dr", "mr", "mrs", "ms", "vs", "approx",
];

pub fn apply_style(text: &str, style: WritingStyle, category: StyleCategory) -> String {
    apply_style_with(text, style, category, &[])
}

/// [`apply_style`] that also keeps the capitals of dictionary words in very
/// casual.
pub fn apply_style_with(
    text: &str,
    style: WritingStyle,
    category: StyleCategory,
    dictionary: &[DictionaryEntry],
) -> String {
    let style = if style.available_in(category) {
        style
    } else {
        WritingStyle::Formal
    };
    let body = text.trim_end();
    let tail = &text[body.len()..];
    if body.is_empty() {
        return text.to_owned();
    }
    let body = match style {
        WritingStyle::Formal => ensure_terminal(body),
        WritingStyle::Casual => {
            if category.is_messaging() && sentence_count(body) <= MAX_CASUAL_SENTENCES {
                drop_final_period(body).to_owned()
            } else {
                body.to_owned()
            }
        }
        WritingStyle::VeryCasual => {
            let lowered = lowercase_sentence_starts(body, &proper_nouns(dictionary));
            drop_final_period(&lowered).to_owned()
        }
        WritingStyle::Excited => {
            let body = ensure_terminal(body);
            match removable_final_period(&body) {
                // A snippet's expansion is kept exactly, nothing glued to it.
                Some(stem) if !stem.ends_with(is_placeholder) => format!("{stem}!"),
                _ => body,
            }
        }
    };
    body + tail
}

/// URLs, emails, paths and code keep their punctuation and case.
fn is_codeish(word: &str) -> bool {
    word.contains("://")
        || word.chars().any(|c| {
            matches!(
                c,
                '/' | '\\' | '@' | '_' | '{' | '}' | '<' | '>' | '=' | '`' | '|' | '~' | '#'
            )
        })
}

fn last_word(body: &str) -> &str {
    body.rsplit(char::is_whitespace).next().unwrap_or(body)
}

/// The text without its final period, if the period ends a sentence rather
/// than an abbreviation ("p.m.", "etc."), a URL or an ellipsis.
fn removable_final_period(body: &str) -> Option<&str> {
    let stem = body.strip_suffix('.')?;
    let word = last_word(stem);
    let bare = word.trim_start_matches(|c: char| !c.is_alphanumeric());
    let ends_in_letter = stem
        .chars()
        .last()
        .is_some_and(|c| c.is_alphanumeric() || is_placeholder(c))
        || stem.ends_with(['"', '\u{201D}', ')', '\'']);
    let abbreviation = bare.contains('.') || ABBREVIATIONS.contains(&bare.to_lowercase().as_str());
    (ends_in_letter && !abbreviation && !is_codeish(word)).then_some(stem)
}

fn drop_final_period(body: &str) -> &str {
    removable_final_period(body).unwrap_or(body)
}

/// Adds a period to a final sentence that has none, when it looks like a
/// full sentence: capitalised, at least three words, plain prose.
fn ensure_terminal(body: &str) -> String {
    let line = body.rsplit('\n').next().unwrap_or(body);
    let sentence = line
        .rsplit_once(['.', '?', '!'])
        .map_or(line, |(_, rest)| rest)
        .trim();
    let words: Vec<&str> = sentence.split_whitespace().collect();
    let looks_like_sentence = words.len() >= 3
        && sentence.chars().next().is_some_and(char::is_uppercase)
        && body.chars().last().is_some_and(char::is_alphanumeric)
        && !words.iter().any(|w| is_codeish(w));
    if looks_like_sentence {
        format!("{body}.")
    } else {
        body.to_owned()
    }
}

fn sentence_count(body: &str) -> usize {
    let tokens = tokenize(body);
    let ends = tokens
        .iter()
        .enumerate()
        .filter(|(i, t)| {
            t.is_punct(SENTENCE_END)
                && i.checked_sub(1).is_some_and(|p| tokens[p].is_word())
                && tokens.get(i + 1).is_none_or(Token::is_space)
        })
        .count();
    let open_end = !body.ends_with(SENTENCE_END);
    (ends + usize::from(open_end)).max(1)
}

/// "Hey. How are you?" → "hey. how are you?" Keeps "I", acronyms, inner
/// capitals, dictionary proper nouns and anything that looks like a URL.
fn lowercase_sentence_starts(body: &str, proper: &[String]) -> String {
    let mut tokens = tokenize(body);
    let mut at_start = true;
    for i in 0..tokens.len() {
        let token = &tokens[i];
        match token.kind {
            Kind::Word if at_start => {
                at_start = false;
                let glued = tokens
                    .get(i + 1)
                    .is_some_and(|t| t.is_punct(&['.', '/', '@', ':', '_']))
                    && tokens.get(i + 2).is_some_and(Token::is_word);
                if glued || proper.iter().any(|p| token.text == p.as_str()) {
                    continue;
                }
                let lowered = decapitalize(&token.text).into_owned();
                tokens[i] = Token::new(Kind::Word, lowered);
            }
            Kind::Word => {}
            Kind::Space => at_start |= token.is_newline(),
            Kind::Punct => {
                if token.is_punct(SENTENCE_END) {
                    at_start = true;
                } else if !token.is_punct(&['"', '\'', '(', '[', '\u{201C}', '\u{2018}', '¿', '¡'])
                {
                    at_start = false;
                }
            }
        }
    }
    join(&tokens)
}

#[cfg(test)]
mod tests {
    use super::super::dictionary::tests::entry;
    use super::*;
    use StyleCategory::{Email, Other, Personal, Work};
    use WritingStyle::{Casual, Excited, Formal, VeryCasual};

    #[test]
    fn styles() {
        let dictionary = [entry("Tiny Wispr", None, 1), entry("Anna", None, 2)];
        let cases: &[(WritingStyle, StyleCategory, &str, &str)] = &[
            (Formal, Personal, "See you at noon.", "See you at noon."),
            (Formal, Other, "See you at noon", "See you at noon."),
            (Formal, Other, "Thanks", "Thanks"),
            (
                Formal,
                Other,
                "Is it done? Yes it is",
                "Is it done? Yes it is.",
            ),
            (
                Formal,
                Other,
                "Open src/main.rs now",
                "Open src/main.rs now",
            ),
            (
                Formal,
                Email,
                "Hi Anna,\n\nSee you at noon",
                "Hi Anna,\n\nSee you at noon.",
            ),
            (Casual, Personal, "See you at noon.", "See you at noon"),
            (
                Casual,
                Work,
                "Done. Shipping it now.",
                "Done. Shipping it now",
            ),
            (Casual, Work, "Are you free?", "Are you free?"),
            (Casual, Work, "That's great!", "That's great!"),
            (Casual, Email, "See you at noon.", "See you at noon."),
            (Casual, Other, "See you at noon.", "See you at noon."),
            (Casual, Personal, "Meet at 5 p.m.", "Meet at 5 p.m."),
            (
                Casual,
                Personal,
                "Bring snacks, drinks, etc.",
                "Bring snacks, drinks, etc.",
            ),
            (Casual, Personal, "Wait for it...", "Wait for it..."),
            (Casual, Personal, "Visit example.com.", "Visit example.com."),
            (Casual, Personal, "Mail me at a@b.co.", "Mail me at a@b.co."),
            (Casual, Personal, "He said \"no.\"", "He said \"no.\""),
            (
                Casual,
                Personal,
                "First.\n\nSecond line here.\n",
                "First.\n\nSecond line here\n",
            ),
            (Casual, Personal, &"One. ".repeat(11), &"One. ".repeat(11)),
            (
                VeryCasual,
                Personal,
                "Hey. How are you?",
                "hey. how are you?",
            ),
            (
                VeryCasual,
                Personal,
                "Sure. I'll be there.",
                "sure. I'll be there",
            ),
            (VeryCasual, Personal, "I think so.", "I think so"),
            (
                VeryCasual,
                Personal,
                "NASA called. Anna too.",
                "NASA called. Anna too",
            ),
            (
                VeryCasual,
                Personal,
                "Tiny Wispr works. Great.",
                "Tiny Wispr works. great",
            ),
            (
                VeryCasual,
                Personal,
                "Example.com is down.",
                "Example.com is down",
            ),
            (
                VeryCasual,
                Personal,
                "Line one\nNext line",
                "line one\nnext line",
            ),
            (
                VeryCasual,
                Personal,
                "\"Hello,\" she said.",
                "\"hello,\" she said",
            ),
            (VeryCasual, Personal, "¿Qué tal? Bien.", "¿qué tal? bien"),
            (VeryCasual, Personal, "iPhone time.", "iPhone time"),
            (VeryCasual, Work, "Hey. How are you?", "Hey. How are you?"),
            (Excited, Work, "We shipped it.", "We shipped it!"),
            (
                Excited,
                Email,
                "Thanks. See you soon.",
                "Thanks. See you soon!",
            ),
            (Excited, Other, "Is it ready?", "Is it ready?"),
            (Excited, Other, "Wow!", "Wow!"),
            (Excited, Other, "Go to example.com.", "Go to example.com."),
            (Excited, Other, "See you at 9 a.m.", "See you at 9 a.m."),
            (Excited, Other, "Wait...", "Wait..."),
            (Excited, Personal, "We shipped it", "We shipped it."),
            (Excited, Personal, "We shipped it.", "We shipped it."),
            (Formal, Other, "", ""),
            (Casual, Personal, "Danke, bis später.", "Danke, bis später"),
            (
                Formal,
                Work,
                "Can you review the PR",
                "Can you review the PR.",
            ),
            (Formal, Other, "Sounds good", "Sounds good"),
            (Formal, Other, "The total is 42", "The total is 42."),
            (Formal, Other, "Dear Anna,", "Dear Anna,"),
            (
                Formal,
                Email,
                "See https://example.com for details",
                "See https://example.com for details",
            ),
            (Formal, Other, "東京に行きます", "東京に行きます"),
            (
                Casual,
                Work,
                "Done.\n\nThanks for waiting.",
                "Done.\n\nThanks for waiting",
            ),
            (Casual, Personal, "Call me at 5.", "Call me at 5"),
            (
                Casual,
                Personal,
                "It's at ~/notes.txt.",
                "It's at ~/notes.txt.",
            ),
            (
                Casual,
                Personal,
                "We moved it to Friday (finally).",
                "We moved it to Friday (finally)",
            ),
            (
                VeryCasual,
                Personal,
                "OK. I'm on my way.",
                "OK. I'm on my way",
            ),
            (
                VeryCasual,
                Personal,
                "Hey Anna. See you at 5.",
                "hey Anna. see you at 5",
            ),
            (
                VeryCasual,
                Personal,
                "Check https://Example.com now.",
                "check https://Example.com now",
            ),
            (
                VeryCasual,
                Personal,
                "Wait! Really? Yes.",
                "wait! really? yes",
            ),
            (VeryCasual, Email, "Hey there friend", "Hey there friend."),
            (
                Excited,
                Work,
                "We shipped the new release",
                "We shipped the new release!",
            ),
            (
                Excited,
                Email,
                "Great news. Did we win?",
                "Great news. Did we win?",
            ),
            (Excited, Work, "Thanks!", "Thanks!"),
            (Excited, Work, "Thanks", "Thanks"),
            (
                Excited,
                Other,
                "Hi team,\n\nThe launch went well.",
                "Hi team,\n\nThe launch went well!",
            ),
            (
                Excited,
                Other,
                "See the file at /tmp/out.log.",
                "See the file at /tmp/out.log.",
            ),
            (Excited, Email, "Wir sind fertig.", "Wir sind fertig!"),
            (
                VeryCasual,
                Personal,
                "Ça va. Très bien.",
                "ça va. très bien",
            ),
        ];
        for &(style, category, input, want) in cases {
            assert_eq!(
                apply_style_with(input, style, category, &dictionary),
                want,
                "{style:?} {category:?} {input:?}"
            );
        }
    }

    #[test]
    fn counts_sentences() {
        assert_eq!(sentence_count("One. Two? Three!"), 3);
        assert_eq!(sentence_count("One. Two"), 2);
        assert_eq!(sentence_count("Wait... what"), 1);
        assert_eq!(sentence_count(""), 1);
    }
}
