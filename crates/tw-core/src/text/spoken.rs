//! Spoken punctuation and line breaks ("comma", "new paragraph").

use super::words::{CLAUSE_MARKS, Kind, SENTENCE_END, Token, capitalize, join, tokenize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Mark(char),
    Dash,
    Break(&'static str),
}

const COMMANDS: &[(&str, Command)] = &[
    ("period", Command::Mark('.')),
    ("full stop", Command::Mark('.')),
    ("comma", Command::Mark(',')),
    ("question mark", Command::Mark('?')),
    ("exclamation point", Command::Mark('!')),
    ("exclamation mark", Command::Mark('!')),
    ("colon", Command::Mark(':')),
    ("semicolon", Command::Mark(';')),
    ("semi colon", Command::Mark(';')),
    ("em dash", Command::Dash),
    ("emdash", Command::Dash),
    ("new line", Command::Break("\n")),
    ("newline", Command::Break("\n")),
    ("next line", Command::Break("\n")),
    ("line break", Command::Break("\n")),
    ("skip a line", Command::Break("\n\n")),
    ("new paragraph", Command::Break("\n\n")),
    ("start a new paragraph", Command::Break("\n\n")),
];
const MAX_COMMAND_WORDS: usize = 4;

/// A command word right after one of these is talked about, not dictated
/// ("remove the comma", "a period of time", "c'est un colon").
const MENTION_BEFORE: &[&str] = &[
    "the", "a", "an", "this", "that", "these", "those", "of", "no", "every", "each", "any",
    "another", "my", "your", "his", "its", "our", "their", "extra", "missing", "stray", "double",
    "single", "final", "last", "trailing",
    // French, Spanish, Italian and German articles: "colon", "comma" and
    // "period" are words there too.
    "le", "la", "les", "un", "une", "des", "du", "el", "los", "las", "una", "il", "lo", "gli",
    "del", "al", "nel", "della", "der", "die", "das", "den", "dem", "ein", "eine", "einen",
];
/// Words that can end a question ("who are you going with question mark"),
/// so they only make a mention when prose continues after the command word
/// ("talk about comma placement").
const MENTION_BEFORE_IF_CONTINUED: &[&str] = &["with", "without", "about", "which", "what", "per"];
const MENTION_AFTER: &[&str] = &[
    "key",
    "keys",
    "character",
    "characters",
    "sign",
    "symbol",
    "button",
    "splice",
    "separated",
    "delimited",
];
/// Nouns that make "period" part of a compound ("trial period").
const PERIOD_COMPOUNDS: &[&str] = &[
    "trial",
    "grace",
    "waiting",
    "notice",
    "cooling",
    "probation",
    "probationary",
    "transition",
    "transitional",
    "billing",
    "holding",
    "honeymoon",
    "review",
    "warranty",
    "reporting",
    "study",
    "interim",
    "introductory",
    "return",
    "lunch",
    "class",
    "school",
    "time",
    "fiscal",
    "accounting",
];
const COMMA_COMPOUNDS: &[&str] = &["oxford", "serial", "inverted"];
/// Nouns after "colon" that make it the organ ("colon cancer").
const COLON_COMPOUNDS: &[&str] = &[
    "cancer",
    "health",
    "cleanse",
    "cleansing",
    "polyp",
    "polyps",
    "surgery",
    "screening",
    "exam",
    "disease",
    "tumor",
    "tumour",
    "biopsy",
];
/// Words that make "new line" an adjective and a noun ("new line items").
const NEW_LINE_COMPOUNDS: &[&str] = &["of", "item", "items", "manager", "managers"];

/// Converts dictated punctuation words to symbols, absorbing the punctuation
/// Whisper already put around them, and fixes spacing and capitals next to
/// them. Words that are probably mentions stay words. Returns the text and
/// how many commands were applied.
pub(crate) fn format(text: &str) -> (String, u32) {
    let tokens = tokenize(text);
    let mut items: Vec<Item<'_>> = Vec::with_capacity(tokens.len());
    let mut applied = 0;
    let mut i = 0;
    while i < tokens.len() {
        if let Some((command, end)) = command_at(&tokens, i) {
            items.push(Item::Command(command));
            applied += 1;
            i = end;
        } else {
            items.push(Item::Token(tokens[i].clone()));
            i += 1;
        }
    }
    if applied == 0 {
        return (tidy(tokens), 0);
    }
    (tidy(assemble(items)), applied)
}

pub fn format_spoken(text: &str) -> String {
    format(text).0
}

enum Item<'a> {
    Token(Token<'a>),
    Command(Command),
}

/// The command starting at word `i` and the token index after it.
fn command_at(tokens: &[Token<'_>], i: usize) -> Option<(Command, usize)> {
    named_commands(tokens, i)
        .into_iter()
        .find(|&(name, command, end)| !is_mention(tokens, i, end, name, command))
        .map(|(_, command, end)| (command, end))
}

/// Command names spelled by the words at `i`, longest first.
fn named_commands(tokens: &[Token<'_>], i: usize) -> Vec<(&'static str, Command, usize)> {
    if !tokens.get(i).is_some_and(Token::is_word) {
        return Vec::new();
    }
    let mut words = Vec::with_capacity(MAX_COMMAND_WORDS);
    let mut ends = Vec::with_capacity(MAX_COMMAND_WORDS);
    let mut j = i;
    loop {
        words.push(tokens[j].lower().replace('-', " "));
        ends.push(j + 1);
        let next_word = tokens.get(j + 1).is_some_and(Token::is_inline_space)
            && tokens.get(j + 2).is_some_and(Token::is_word);
        if words.len() == MAX_COMMAND_WORDS || !next_word {
            break;
        }
        j += 2;
    }
    (1..=words.len())
        .rev()
        .filter_map(|n| {
            let phrase = words[..n].join(" ");
            let &(name, command) = COMMANDS.iter().find(|(name, _)| *name == phrase)?;
            Some((name, command, ends[n - 1]))
        })
        .collect()
}

fn is_mention(
    tokens: &[Token<'_>],
    start: usize,
    end: usize,
    name: &str,
    command: Command,
) -> bool {
    let before = start
        .checked_sub(1)
        .filter(|&b| tokens[b].is_inline_space())
        .and_then(|b| b.checked_sub(1))
        .map(|b| &tokens[b]);
    let before_word = before.filter(|t| t.is_word()).map(Token::lower);
    let after = tokens
        .get(end)
        .filter(|t| t.is_inline_space())
        .and_then(|_| tokens.get(end + 1));
    let after_word = after.filter(|t| t.is_word()).map(Token::lower);
    let after = after.or_else(|| tokens.get(end));

    let continues = after_word.is_some()
        && after.is_some_and(|t| t.text.chars().next().is_some_and(char::is_lowercase))
        && named_commands(tokens, end + 1).is_empty();
    if before_word.as_deref().is_some_and(|w| {
        MENTION_BEFORE.contains(&w) || (continues && MENTION_BEFORE_IF_CONTINUED.contains(&w))
    }) || after_word
        .as_deref()
        .is_some_and(|w| MENTION_AFTER.contains(&w))
    {
        return true;
    }
    let compound = match name {
        "period" => PERIOD_COMPOUNDS,
        "comma" => COMMA_COMPOUNDS,
        _ => &[],
    };
    if before_word
        .as_deref()
        .is_some_and(|w| compound.contains(&w))
    {
        return true;
    }
    // "her" is possessive before these nouns but an object before a comma
    // ("ask her comma why").
    if matches!(name, "period" | "colon") && before_word.as_deref() == Some("her") {
        return true;
    }
    if name == "period" && after_word.as_deref() == Some("of") {
        return true;
    }
    if name == "new line"
        && after_word
            .as_deref()
            .is_some_and(|w| NEW_LINE_COMPOUNDS.contains(&w))
    {
        return true;
    }
    if name == "colon"
        && after_word
            .as_deref()
            .is_some_and(|w| COLON_COMPOUNDS.contains(&w))
    {
        return true;
    }
    // "period" is also an ordinary noun; as a command it ends a sentence, so
    // a lowercase word straight after means prose ("period cramps").
    if name == "period" {
        let at_boundary = match after {
            None => true,
            Some(t) if t.is_word() => {
                t.text.chars().next().is_some_and(char::is_uppercase)
                    || !named_commands(tokens, if after_word.is_some() { end + 1 } else { end })
                        .is_empty()
            }
            Some(_) => true,
        };
        if !at_boundary {
            return true;
        }
    }
    // A command word that starts a sentence of prose is its subject ("Comma
    // is my favourite", "Next line is blank"). A mark also needs something
    // to attach to, so it can't start the text or a line at all.
    let is_mark = matches!(command, Command::Mark(_) | Command::Dash);
    let mut k = start;
    let mut sentence_start = false;
    loop {
        let Some(prev) = k.checked_sub(1) else {
            sentence_start = true;
            break;
        };
        let t = &tokens[prev];
        if t.is_newline() {
            sentence_start = true;
            break;
        }
        if t.is_word() || (t.kind == Kind::Punct && !t.is_punct(CLAUSE_MARKS)) {
            break;
        }
        if t.is_punct(SENTENCE_END) {
            sentence_start = true;
        }
        k = prev;
    }
    let lowercase_after = after_word.is_some()
        && after.is_some_and(|t| t.text.chars().next().is_some_and(char::is_lowercase));
    if sentence_start && lowercase_after {
        return true;
    }
    is_mark && (k == 0 || tokens[k - 1].is_newline())
}

/// Replaces commands with their symbols and settles the punctuation, spaces
/// and capitals around them.
fn assemble(items: Vec<Item<'_>>) -> Vec<Token<'_>> {
    let mut out: Vec<Token<'_>> = Vec::with_capacity(items.len());
    // Commands' own output, so Whisper's punctuation can be told apart.
    let mut ours: Vec<bool> = Vec::with_capacity(items.len());
    let mut pending: Option<Command> = None;
    for item in items {
        match item {
            Item::Command(command) => {
                let keep_marks = matches!(command, Command::Break(_));
                while let Some(last) = out.last() {
                    let ours_last = ours.last().copied().unwrap_or(false);
                    let drop = last.is_inline_space()
                        || (!keep_marks && !ours_last && last.is_punct(CLAUSE_MARKS));
                    if !drop {
                        break;
                    }
                    out.pop();
                    ours.pop();
                }
                let token = match command {
                    Command::Mark(c) => Token::new(Kind::Punct, c.to_string()),
                    Command::Dash => Token::new(Kind::Punct, "\u{2014}"),
                    Command::Break(s) => Token::new(Kind::Space, s),
                };
                out.push(token);
                ours.push(true);
                pending = Some(command);
            }
            Item::Token(token) => {
                if let Some(command) = pending {
                    if token.is_inline_space() || token.is_punct(CLAUSE_MARKS) {
                        continue;
                    }
                    if token.kind != Kind::Space {
                        let space_before = matches!(command, Command::Mark(_))
                            && (token.is_word()
                                || token
                                    .is_punct(&['"', '(', '[', '\u{201C}', '\u{2018}', '¿', '¡']));
                        if space_before {
                            out.push(Token::new(Kind::Space, " "));
                            ours.push(true);
                        }
                        let starts_sentence = match command {
                            Command::Mark(c) => SENTENCE_END.contains(&c),
                            Command::Dash => false,
                            Command::Break(_) => true,
                        };
                        let token = if starts_sentence && token.is_word() {
                            Token::new(Kind::Word, capitalize(&token.text).into_owned())
                        } else {
                            token
                        };
                        out.push(token);
                        ours.push(false);
                        pending = None;
                        continue;
                    }
                }
                out.push(token);
                ours.push(false);
            }
        }
    }
    out
}

/// Collapses doubled spaces and drops a space before a comma or period that
/// ends a word ("Hello , there"). Other marks keep their spacing, which is
/// correct before `?`, `!`, `:` and `;` in French.
fn tidy(tokens: Vec<Token<'_>>) -> String {
    let mut out: Vec<Token<'_>> = Vec::with_capacity(tokens.len());
    for (i, token) in tokens.iter().enumerate() {
        if token.is_inline_space() {
            let prev_word = out.last().is_some_and(Token::is_word);
            let next_mark = tokens.get(i + 1).is_some_and(|t| t.is_punct(&['.', ',']))
                && tokens.get(i + 2).is_none_or(|t| t.is_space());
            if prev_word && next_mark {
                continue;
            }
            if token.text.len() > 1 && token.text.chars().all(|c| c == ' ') {
                out.push(Token::new(Kind::Space, " "));
                continue;
            }
        }
        out.push(token.clone());
    }
    join(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_become_punctuation() {
        let cases = [
            (
                "Hello comma how are you question mark",
                "Hello, how are you?",
            ),
            ("Hello, comma, how are you period.", "Hello, how are you."),
            ("Hello comma how are you. Period.", "Hello, how are you."),
            (
                "I'm done period See you tomorrow",
                "I'm done. See you tomorrow",
            ),
            (
                "I'm done, period, see you tomorrow.",
                "I'm done. See you tomorrow.",
            ),
            ("That's amazing exclamation point", "That's amazing!"),
            ("That's amazing exclamation mark!", "That's amazing!"),
            ("Wait, question mark?", "Wait?"),
            ("Dear Anna colon", "Dear Anna:"),
            (
                "We have three options colon. Red, green and blue.",
                "We have three options: Red, green and blue.",
            ),
            ("I came semicolon I saw", "I came; I saw"),
            ("It's ready semi-colon ship it", "It's ready; ship it"),
            (
                "It was perfect em dash almost",
                "It was perfect\u{2014}almost",
            ),
            (
                "It was perfect, em-dash, almost.",
                "It was perfect\u{2014}almost.",
            ),
            (
                "It was perfect emdash almost",
                "It was perfect\u{2014}almost",
            ),
            ("Thanks full stop", "Thanks."),
            (
                "Hi Sam, new line, thanks for the notes.",
                "Hi Sam,\nThanks for the notes.",
            ),
            ("Hi Sam next line thanks", "Hi Sam\nThanks"),
            (
                "First point. Line break. Second point.",
                "First point.\nSecond point.",
            ),
            (
                "End of intro. New paragraph. Next part.",
                "End of intro.\n\nNext part.",
            ),
            (
                "End of intro new paragraph next part",
                "End of intro\n\nNext part",
            ),
            ("Intro. Start a new paragraph. Body.", "Intro.\n\nBody."),
            ("Intro, skip a line, body", "Intro,\n\nBody"),
            ("Bullet one new line Bullet two", "Bullet one\nBullet two"),
            (
                "Is it done question mark new line Yes period",
                "Is it done?\nYes.",
            ),
            ("PERIOD", "PERIOD"),
            ("Okay Period.", "Okay."),
            ("Yes period no", "Yes period no"),
            ("Ask \"why\" question mark", "Ask \"why\"?"),
            ("Ask her comma \"why\"", "Ask her, \"why\""),
            (
                "Who are you going with question mark",
                "Who are you going with?",
            ),
            ("What is it about question mark", "What is it about?"),
            (
                "Send me the report by Friday period",
                "Send me the report by Friday.",
            ),
            (
                "I'll be there in a minute comma Anna",
                "I'll be there in a minute, Anna",
            ),
            (
                "Hey Sam comma can you send the file question mark",
                "Hey Sam, can you send the file?",
            ),
            (
                "Dear Sir or Madam colon new paragraph I am writing to apply",
                "Dear Sir or Madam:\n\nI am writing to apply",
            ),
            ("Item one semicolon item two", "Item one; item two"),
            (
                "Thanks so much exclamation point exclamation point",
                "Thanks so much!!",
            ),
            ("It's 5 p.m. period", "It's 5 p.m."),
            ("Go to example.com period", "Go to example.com."),
            (
                "The plan is colon, first we test.",
                "The plan is: first we test.",
            ),
            (
                "Groceries colon milk comma eggs comma bread",
                "Groceries: milk, eggs, bread",
            ),
            (
                "Can you call me back question mark Thanks period",
                "Can you call me back? Thanks.",
            ),
            (
                "I'm free on Monday comma Tuesday comma and Friday",
                "I'm free on Monday, Tuesday, and Friday",
            ),
            ("Well comma I think so", "Well, I think so"),
            ("Great job em dash really", "Great job\u{2014}really"),
            (
                "Title colon Q3 plan new line Owner colon Anna",
                "Title: Q3 plan\nOwner: Anna",
            ),
            (
                "Thanks period new paragraph Best comma new line Amelia",
                "Thanks.\n\nBest,\nAmelia",
            ),
            ("Um comma I think", "Um, I think"),
            ("New paragraph.", "\n\n"),
        ];
        for (input, want) in cases {
            assert_eq!(format_spoken(input), want, "{input}");
        }
    }

    #[test]
    fn mentions_stay_words() {
        let cases = [
            "Remove the comma after hello.",
            "Can you add a comma here?",
            "It lasted a period of time.",
            "The trial period ends on Friday.",
            "Is it still in the trial period?",
            "This period was the hardest.",
            "Press the period key twice.",
            "Type the colon character.",
            "Use a semicolon instead.",
            "Put a question mark at the end.",
            "There's a question mark over the plan.",
            "Every comma counts.",
            "No comma needed.",
            "I love the Oxford comma.",
            "That's a comma splice.",
            "Upload a comma separated file.",
            "The grace period is over.",
            "The waiting period is long.",
            "Period cramps are the worst.",
            "I have period cramps.",
            "Colon cancer screening saves lives.",
            "We need new line items in the budget.",
            "Start on a new line.",
            "The next line is blank.",
            "Comma is my favourite mark.",
            "Hello. Comma is my favourite mark.",
            "Hit the exclamation mark key.",
            "He talked about the em dash.",
            "Use the dollar sign.",
            "Without a full stop it reads oddly.",
            "She's on her period.",
            "I have colon cancer.",
            "Talk about comma placement with the team.",
            "Next line is blank.",
            "New paragraph starts here.",
            "Remove that comma.",
            "Delete the last period.",
            "There's an extra comma in the title.",
            "There's a missing period at the end.",
            "How many exclamation points is too many?",
            "The question marks are wrong.",
            "We're in a transition period now.",
            "Over the holiday period we rest.",
            "Mark the end with a full stop.",
            "What's the semicolon for?",
            "His colon was inflamed.",
            "The em dash and the en dash differ.",
            "Write a comma, then a space.",
            "Each period lasts fifty minutes.",
            "A period drama.",
            "Fourth period is math.",
            "Which comma do you mean?",
        ];
        for input in cases {
            assert_eq!(format_spoken(input), input);
        }
    }

    #[test]
    fn other_languages_are_untouched() {
        let cases = [
            "Setz hier ein Komma, dann einen Punkt.",
            "Die Straße ist schön, oder?",
            "Bonjour, comment ça va ?",
            "Il faut une virgule ici : voilà.",
            "¿Dónde está el niño? ¡Aquí!",
            "Mañana vamos a la playa.",
            "Ça coûte 3,50 € et 2.5 %.",
            "C'est un colon.",
            "Le colon est arrivé hier.",
            "Il comma 3 dice che non si può.",
            "Es un período largo, ¿verdad?",
            "La coma va aquí.",
            "Der Punkt ist wichtig: wirklich.",
            "Er sagte: „Ich komme morgen.“",
            "東京は晴れです。",
        ];
        for input in cases {
            assert_eq!(format_spoken(input), input);
        }
    }

    #[test]
    fn spacing_is_tidied() {
        let cases = [
            ("Hello ,  world .", "Hello, world."),
            ("Version 2 .5 works", "Version 2 .5 works"),
            ("Visit example.com today", "Visit example.com today"),
            ("Line one\n\n  indented", "Line one\n\n  indented"),
        ];
        for (input, want) in cases {
            assert_eq!(format_spoken(input), want, "{input}");
        }
    }

    #[test]
    fn counts_commands() {
        assert_eq!(format("Yes comma b period").1, 2);
        assert_eq!(format("the comma").1, 0);
    }
}
