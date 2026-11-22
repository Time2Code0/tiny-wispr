//! Hesitation sounds and stutters.

use super::words::{Kind, SENTENCE_END, Token, capitalize, join, tokenize};

/// Function words whose immediate repetition is always a stutter. "that
/// that", "had had", "is is" and "you you" can be correct English.
const STUTTER_WORDS: &[&str] = &[
    "the", "a", "an", "i", "and", "but", "or", "of", "we", "they",
];

fn is_filler(word: &str) -> bool {
    // "ER", "HM", "UH" are acronyms.
    if word.chars().count() > 1 && word.chars().all(char::is_uppercase) {
        return false;
    }
    let w = word.to_lowercase();
    let run = |s: &str, c: char| s.len() - s.trim_start_matches(c).len();
    let u = run(&w, 'u');
    let after_u = &w[u..];
    let shape = |rest: &str, a: char, b: Option<char>| {
        let n = run(rest, a);
        let rest = &rest[n..];
        n > 0
            && match b {
                None => rest.is_empty(),
                Some(b) => !rest.is_empty() && rest.chars().all(|c| c == b),
            }
    };
    (u > 0
        && (shape(after_u, 'm', None)
            || shape(after_u, 'h', None)
            || shape(after_u, 'h', Some('m'))))
        || w == "erm"
        || (w.starts_with('h') && shape(&w[1..], 'm', None))
}

/// "er" is German for "he", so it only counts as a filler where Whisper
/// marked a pause after it ("So, er, I left").
fn is_paused_er(tokens: &[Token<'_>], i: usize) -> bool {
    tokens[i].lower() == "er"
        && tokens[i].text != "ER"
        && tokens.get(i + 1).is_some_and(|t| {
            t.is_punct(&[',', '\u{2026}'])
                || t.text == "." && tokens.get(i + 2).is_some_and(|t| t.text == ".")
        })
}

/// "uh huh" and "mm hmm" written with a space are answers, not fillers.
fn is_answer(prev: Option<&Token<'_>>, word: &str, next: Option<&Token<'_>>) -> bool {
    let w = word.to_lowercase();
    let next = next.map(Token::lower);
    let prev = prev.map(Token::lower);
    (w == "uh" && next.as_deref() == Some("huh"))
        || (w.starts_with('h') && prev.as_deref() == Some("mm"))
}

/// Removes fillers and collapses stutters. Returns the text and how many
/// words were removed.
pub(crate) fn remove(text: &str) -> (String, u32) {
    let tokens = tokenize(text);
    let mut out: Vec<Token<'_>> = Vec::with_capacity(tokens.len());
    let mut removed = 0;
    let mut capitalize_next = false;
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        if token.is_word() && (is_filler(&token.text) || is_paused_er(&tokens, i)) {
            let prev_word = tokens[..i].iter().rev().find(|t| !t.is_space());
            let next_word = tokens[i + 1..].iter().find(|t| !t.is_space());
            if !is_answer(prev_word, &token.text, next_word) {
                i = remove_filler(&tokens, i, &mut out, &mut capitalize_next);
                removed += 1;
                continue;
            }
        }
        if token.is_word() && is_stutter(&tokens, i, &out) {
            while out.last().is_some_and(|t| !t.is_word()) {
                out.pop();
            }
            removed += 1;
            i += 1;
            continue;
        }
        if token.is_space() && removed > 0 && out.last().is_none_or(Token::is_space) {
            if !(token.is_newline() && out.last().is_some_and(Token::is_inline_space)) {
                i += 1;
                continue;
            }
            out.pop();
        }
        if token.is_word() && capitalize_next {
            out.push(Token::new(Kind::Word, capitalize(&token.text).into_owned()));
            capitalize_next = false;
        } else {
            out.push(token.clone());
        }
        i += 1;
    }
    if removed == 0 {
        return (text.to_owned(), 0);
    }
    while out.last().is_some_and(Token::is_inline_space) {
        out.pop();
    }
    (join(&out), removed)
}

pub fn remove_fillers(text: &str) -> String {
    remove(text).0
}

/// Drops the filler at `i` with the punctuation Whisper put around it and
/// returns the index to continue from.
fn remove_filler<'a>(
    tokens: &[Token<'a>],
    i: usize,
    out: &mut Vec<Token<'a>>,
    capitalize_next: &mut bool,
) -> usize {
    let at_sentence_start = out
        .iter()
        .rev()
        .find(|t| !t.is_inline_space())
        .is_none_or(|t| t.is_newline() || t.is_punct(SENTENCE_END));
    let mut j = i + 1;
    let mut pause_after = tokens.get(j).is_some_and(|t| t.is_punct(&[',']));
    if pause_after {
        j += 1;
    } else {
        while tokens.get(j).is_some_and(|t| t.is_punct(&['\u{2026}']))
            || (tokens.get(j).is_some_and(|t| t.is_punct(&['.']))
                && tokens.get(j + 1).is_some_and(|t| t.is_punct(&['.'])))
        {
            pause_after = true;
            j += 1;
            while tokens.get(j).is_some_and(|t| t.is_punct(&['.'])) {
                j += 1;
            }
        }
    }
    let sentence_end_follows =
        j == tokens.len() || tokens.get(j).is_some_and(|t| t.is_punct(SENTENCE_END));
    if at_sentence_start {
        if sentence_end_follows && !pause_after {
            // The filler was the whole sentence ("Okay. Um. Next").
            j += 1;
        }
        *capitalize_next = true;
    } else if sentence_end_follows || pause_after {
        let comma_before = out
            .iter()
            .rev()
            .find(|t| !t.is_inline_space())
            .is_some_and(|t| t.is_punct(&[',']));
        if sentence_end_follows || comma_before {
            while out.last().is_some_and(Token::is_inline_space) {
                out.pop();
            }
            if comma_before {
                out.pop();
            }
        }
    }
    j.min(tokens.len())
}

/// The word at `i` repeats the previous word ("the the", "I, I think").
fn is_stutter(tokens: &[Token<'_>], i: usize, out: &[Token<'_>]) -> bool {
    let word = tokens[i].lower();
    if !STUTTER_WORDS.contains(&word.as_str()) {
        return false;
    }
    let mut between = out.iter().rev().take_while(|t| !t.is_word());
    let separated = between.all(|t| t.is_inline_space() || t.is_punct(&[',']));
    let gap = out.iter().rev().take_while(|t| !t.is_word()).count();
    separated && gap > 0 && out.iter().rev().nth(gap).is_some_and(|t| t.lower() == word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_fillers_with_their_punctuation() {
        let cases = [
            ("Um, I think so.", "I think so."),
            ("so, uh, we could go.", "so we could go."),
            ("I was, um, thinking about it.", "I was thinking about it."),
            ("I was um thinking about it.", "I was thinking about it."),
            ("Uh, what time is it?", "What time is it?"),
            ("Let's meet at, uh, five.", "Let's meet at five."),
            ("And then, um.", "And then."),
            ("And then um.", "And then."),
            ("What do you think, hmm?", "What do you think?"),
            (
                "That's it. Um, and then we left.",
                "That's it. And then we left.",
            ),
            ("Okay. Um. Next item.", "Okay. Next item."),
            ("Um... I guess so.", "I guess so."),
            ("We could, uh… try again.", "We could try again."),
            ("Umm, uhh, maybe.", "Maybe."),
            ("Erm, sure.", "Sure."),
            ("So, er, I left.", "So I left."),
            ("It's, er... complicated.", "It's complicated."),
            ("Hmm, hmmm, let me see.", "Let me see."),
            ("Uhm, okay.", "Okay."),
            ("So um, yeah.", "So yeah."),
            ("First line.\nUm, second line.", "First line.\nSecond line."),
            ("Um", ""),
            ("Hmm?", ""),
            ("Well, um, iPhone sales grew.", "Well iPhone sales grew."),
            ("Also uh", "Also"),
            (
                "I think, uh, that we should, um, go.",
                "I think that we should go.",
            ),
            ("Uh. Okay.", "Okay."),
            ("Um, so, um, yeah.", "So yeah."),
            ("Hmm, I'm not sure, hmm.", "I'm not sure."),
            ("Ummm, uhhh, okay.", "Okay."),
            ("The, uh, the report is late.", "The report is late."),
            ("We, um, we should go.", "We should go."),
            ("Uh, I, I don't know.", "I don't know."),
            ("Can you, um, send it?", "Can you send it?"),
            ("Er, where was I?", "Where was I?"),
            ("Ja, ähm, uh, okay.", "Ja, ähm okay."),
        ];
        for (input, want) in cases {
            assert_eq!(remove_fillers(input), want, "{input}");
        }
    }

    #[test]
    fn collapses_stutters() {
        let cases = [
            ("I went to the the store.", "I went to the store."),
            ("I I think so.", "I think so."),
            ("I, I think so.", "I think so."),
            ("The the plan is fine.", "The plan is fine."),
            ("It was a a mistake.", "It was a mistake."),
            ("We we should go.", "We should go."),
            ("Bread and and butter.", "Bread and butter."),
            ("I I I don't know.", "I don't know."),
        ];
        for (input, want) in cases {
            assert_eq!(remove_fillers(input), want, "{input}");
        }
    }

    #[test]
    fn leaves_real_words_alone() {
        let cases = [
            "I like you.",
            "You know, it's fine.",
            "I mean it.",
            "It's sort of blue.",
            "Mm-hmm, that works.",
            "Uh-huh, sure.",
            "Uh huh, sure.",
            "Mm hmm, sure.",
            "She had had enough.",
            "I said that that was fine.",
            "What it is is a problem.",
            "I told you you were right.",
            "We need 2 2-inch screws.",
            "The umbrella is under the bed.",
            "Hummus and herbs.",
            "To err is human.",
            "Ähm, ich weiß nicht.",
            "Euh, je ne sais pas.",
            "Ich habe das das Buch gegeben.",
            "My my, what a surprise.",
            "Who did you talk to to get this?",
            "It was so so good.",
            "Uma and Umar met.",
            "Take him to the ER.",
            "HM Revenue and Customs.",
            "Er ist nach Hause gegangen.",
            "Gestern hat er gesagt, dass er kommt.",
            "So er I left.",
            "Ah, I see.",
            "Huh, interesting.",
            "Oh, okay.",
            "Mmm, delicious.",
            "Hum along with me.",
            "Do you like it? I like it like that.",
            "And I mean, sort of, you know.",
            "Sí, sí, claro.",
            "I. I think so.",
        ];
        for input in cases {
            assert_eq!(remove_fillers(input), input);
        }
    }

    #[test]
    fn counts_removals() {
        assert_eq!(remove("Um, the the plan").1, 2);
        assert_eq!(remove("The plan").1, 0);
    }
}
