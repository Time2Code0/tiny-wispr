use tauri::State;
use tw_core::text::pipeline::{self, PipelineContext};
use tw_core::text::{
    DictionaryEntryInput, MAX_EXPANSION_CHARS, MAX_PHRASE_CHARS, SnippetInput, StyleCategory,
};
use tw_core::{DictionaryEntry, Millis, Snippet};

use crate::error::{CommandError, CommandResult, ErrorKind};
use crate::state::AppState;
use crate::store::new_id;
use crate::windows::now_ms;

#[tauri::command]
pub fn list_dictionary(state: State<'_, AppState>) -> Vec<DictionaryEntry> {
    state.data.dictionary.get()
}

#[tauri::command]
pub fn upsert_dictionary_entry(
    state: State<'_, AppState>,
    entry: DictionaryEntryInput,
) -> CommandResult<DictionaryEntry> {
    state
        .data
        .dictionary
        .try_update(|entries| upsert_word(entries, entry, now_ms()))
}

#[tauri::command]
pub fn delete_dictionary_entry(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    state
        .data
        .dictionary
        .try_update(|entries| remove_by(entries, |e| e.id == id))
}

#[tauri::command]
pub fn list_snippets(state: State<'_, AppState>) -> Vec<Snippet> {
    state.data.snippets.get()
}

#[tauri::command]
pub fn upsert_snippet(state: State<'_, AppState>, snippet: SnippetInput) -> CommandResult<Snippet> {
    state
        .data
        .snippets
        .try_update(|snippets| upsert_snippet_in(snippets, snippet, now_ms()))
}

#[tauri::command]
pub fn delete_snippet(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    state
        .data
        .snippets
        .try_update(|snippets| remove_by(snippets, |s| s.id == id))
}

/// Runs the rule-based pipeline on typed text for the Style page's live
/// example. `category` defaults to Other.
#[tauri::command]
pub fn test_polish(
    state: State<'_, AppState>,
    text: String,
    category: Option<StyleCategory>,
) -> String {
    let settings = state.data.settings.get();
    let category = category.unwrap_or(StyleCategory::Other);
    let dictionary = state.data.dictionary.get();
    let snippets = state.data.snippets.get();
    let ctx = PipelineContext {
        level: settings.cleanup_level,
        style: settings.styles.get(category),
        category,
        dictionary: &dictionary,
        snippets: &snippets,
    };
    pipeline::run(&text, &ctx)
}

fn remove_by<T>(items: &mut Vec<T>, matches: impl Fn(&T) -> bool) -> CommandResult<()> {
    let before = items.len();
    items.retain(|item| !matches(item));
    if items.len() == before {
        return Err(CommandError::new(ErrorKind::NotFound, "It's already gone"));
    }
    Ok(())
}

fn phrase(text: &str, what: &str) -> CommandResult<String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(CommandError::invalid(format!("Enter a {what}")));
    }
    if text.chars().count() > MAX_PHRASE_CHARS {
        return Err(CommandError::invalid(format!(
            "Keep the {what} under {MAX_PHRASE_CHARS} characters"
        )));
    }
    Ok(text.to_owned())
}

fn same(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Adds a new entry (no id) or edits one, after the checks of spec §2.4.
fn upsert_word(
    entries: &mut Vec<DictionaryEntry>,
    input: DictionaryEntryInput,
    now: Millis,
) -> CommandResult<DictionaryEntry> {
    let word = phrase(&input.word, "word")?;
    let misspelling = match input.misspelling.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(wrong) => Some(phrase(wrong, "misspelling")?),
    };
    if misspelling
        .as_deref()
        .is_some_and(|wrong| same(wrong, &word))
    {
        return Err(CommandError::invalid(
            "The misspelling is the same as the word",
        ));
    }
    let duplicate = entries.iter().any(|e| {
        Some(&e.id) != input.id.as_ref()
            && same(&e.word, &word)
            && e.misspelling.as_deref().map(str::to_lowercase)
                == misspelling.as_deref().map(str::to_lowercase)
    });
    if duplicate {
        return Err(CommandError::invalid("That's already in your dictionary"));
    }
    let entry = match &input.id {
        Some(id) => {
            let existing = entries
                .iter_mut()
                .find(|e| &e.id == id)
                .ok_or_else(|| CommandError::new(ErrorKind::NotFound, "That word is gone"))?;
            existing.word = word;
            existing.misspelling = misspelling;
            existing.clone()
        }
        None => {
            let entry = DictionaryEntry {
                id: new_id(),
                word,
                misspelling,
                created_at: now,
            };
            entries.push(entry.clone());
            entry
        }
    };
    Ok(entry)
}

fn upsert_snippet_in(
    snippets: &mut Vec<Snippet>,
    input: SnippetInput,
    now: Millis,
) -> CommandResult<Snippet> {
    let trigger = phrase(&input.trigger, "trigger")?;
    let expansion = input.expansion.trim().to_owned();
    if expansion.is_empty() {
        return Err(CommandError::invalid("Enter the text to insert"));
    }
    if expansion.chars().count() > MAX_EXPANSION_CHARS {
        return Err(CommandError::invalid(format!(
            "Keep the text under {MAX_EXPANSION_CHARS} characters"
        )));
    }
    if snippets
        .iter()
        .any(|s| Some(&s.id) != input.id.as_ref() && same(&s.trigger, &trigger))
    {
        return Err(CommandError::invalid(
            "A snippet with that trigger already exists",
        ));
    }
    let snippet = match &input.id {
        Some(id) => {
            let existing = snippets
                .iter_mut()
                .find(|s| &s.id == id)
                .ok_or_else(|| CommandError::new(ErrorKind::NotFound, "That snippet is gone"))?;
            existing.trigger = trigger;
            existing.expansion = expansion;
            existing.clone()
        }
        None => {
            let snippet = Snippet {
                id: new_id(),
                trigger,
                expansion,
                created_at: now,
            };
            snippets.push(snippet.clone());
            snippet
        }
    };
    Ok(snippet)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(id: Option<&str>, word: &str, wrong: Option<&str>) -> DictionaryEntryInput {
        DictionaryEntryInput {
            id: id.map(Into::into),
            word: word.into(),
            misspelling: wrong.map(Into::into),
        }
    }

    fn snippet(id: Option<&str>, trigger: &str, expansion: &str) -> SnippetInput {
        SnippetInput {
            id: id.map(Into::into),
            trigger: trigger.into(),
            expansion: expansion.into(),
        }
    }

    #[test]
    fn dictionary_entries_are_checked_trimmed_and_unique() {
        let mut entries = Vec::new();
        let added = upsert_word(&mut entries, word(None, "  Kubernetes ", Some(" ")), 1).unwrap();
        assert_eq!(added.word, "Kubernetes");
        assert_eq!(added.misspelling, None);
        assert!(upsert_word(&mut entries, word(None, "kubernetes", None), 2).is_err());
        assert!(
            upsert_word(
                &mut entries,
                word(None, "Kubernetes", Some("cooper netties")),
                2
            )
            .is_ok()
        );
        assert!(upsert_word(&mut entries, word(None, "", None), 2).is_err());
        assert!(upsert_word(&mut entries, word(None, &"a".repeat(61), None), 2).is_err());
        assert!(upsert_word(&mut entries, word(None, "Bob", Some("bob")), 2).is_err());

        let edited = upsert_word(&mut entries, word(Some(&added.id), "K8s", None), 3).unwrap();
        assert_eq!(
            (edited.id.as_str(), edited.created_at),
            (added.id.as_str(), 1)
        );
        assert_eq!(entries.len(), 2);
        assert!(upsert_word(&mut entries, word(Some("nope"), "x", None), 3).is_err());
    }

    #[test]
    fn snippet_triggers_are_unique_and_expansions_bounded() {
        let mut snippets = Vec::new();
        let added =
            upsert_snippet_in(&mut snippets, snippet(None, "my email", "a@b.c"), 1).unwrap();
        assert!(upsert_snippet_in(&mut snippets, snippet(None, "My Email", "x"), 2).is_err());
        assert!(upsert_snippet_in(&mut snippets, snippet(None, "sig", "  "), 2).is_err());
        assert!(
            upsert_snippet_in(&mut snippets, snippet(None, "sig", &"x".repeat(4_001)), 2).is_err()
        );
        let edited = upsert_snippet_in(
            &mut snippets,
            snippet(Some(&added.id), "My email", "d@e.f"),
            3,
        )
        .unwrap();
        assert_eq!(edited.expansion, "d@e.f");
        assert_eq!(snippets.len(), 1);
        assert!(remove_by(&mut snippets, |s| s.id == added.id).is_ok());
        assert!(remove_by(&mut snippets, |s| s.id == added.id).is_err());
    }
}
