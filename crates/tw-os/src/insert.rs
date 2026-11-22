//! Text insertion into the focused app: clipboard-preserving paste, or typed
//! keystrokes for apps that block paste.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub use tw_core::settings::InsertionMethod;

use crate::{Result, platform};

/// The clipboard is restored this long after the paste keystroke.
pub const CLIPBOARD_RESTORE_MS: u64 = 500;
/// How long insertion waits for the hotkey's modifiers to be let go.
pub const MODIFIER_RELEASE_TIMEOUT_MS: u64 = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertOptions {
    pub method: InsertionMethod,
    /// Put the previous clipboard back after pasting. Off leaves the
    /// transcript on the clipboard as an ordinary entry.
    pub restore_clipboard: bool,
    pub restore_after: Duration,
    /// Synthesising while the user still holds the hotkey's modifiers would
    /// turn ⌘V into Fn⌘V. Insertion goes ahead anyway after this long.
    pub modifier_timeout: Duration,
}

impl Default for InsertOptions {
    fn default() -> Self {
        Self {
            method: InsertionMethod::Paste,
            restore_clipboard: true,
            restore_after: Duration::from_millis(CLIPBOARD_RESTORE_MS),
            modifier_timeout: Duration::from_millis(MODIFIER_RELEASE_TIMEOUT_MS),
        }
    }
}

/// Inserts `text` into the focused field. Blocks until the keystrokes are
/// sent (the modifier wait plus a few tens of milliseconds); the clipboard
/// restore runs on a background thread. Call it off the main thread.
///
/// Fails with `PermissionMissing(Accessibility)` on macOS without the
/// permission, and `Unsupported` on Wayland. Never prompts.
pub fn insert_text(text: &str, options: &InsertOptions) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    platform::insert::insert_text(text, options)
}

/// Puts `text` on the clipboard as an ordinary entry, for "Press ⌃⌘V to
/// paste" when insertion failed.
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    platform::insert::copy_to_clipboard(text)
}

/// The text selected in the focused app, read by sending the copy shortcut
/// and restoring the clipboard afterwards. `None` when nothing is selected.
pub fn read_selection() -> Result<Option<String>> {
    platform::insert::read_selection()
}

/// Reads the active keyboard layout so the paste and copy shortcuts use the
/// right physical keys (Dvorak, AZERTY, non-Latin layouts). macOS: call on
/// the main thread at launch and when the input source changes; until then
/// the ANSI positions are used. No-op elsewhere.
pub fn refresh_keyboard_layout() -> Result<()> {
    platform::insert::refresh_keyboard_layout()
}

/// The platform clipboard, behind a trait so the restore logic is tested
/// against a fake.
pub(crate) trait Clipboard: Send + Sync + 'static {
    type Snapshot: Send + 'static;

    /// Changes whenever anyone writes the clipboard.
    fn change_count(&self) -> i64;
    fn snapshot(&self) -> Result<Self::Snapshot>;
    /// Writes `text`, marked so clipboard managers skip it when `transient`.
    fn write_text(&self, text: &str, transient: bool) -> Result<()>;
    fn restore(&self, snapshot: &Self::Snapshot) -> Result<()>;
}

/// The clipboard contents to put back once the latest paste is done.
pub(crate) struct PendingRestore<S> {
    generation: u64,
    original: Option<S>,
    /// The change count right after our write: if it still reads this, the
    /// clipboard holds our transcript.
    ours: Option<i64>,
}

impl<S> PendingRestore<S> {
    pub(crate) const fn new() -> Self {
        Self {
            generation: 0,
            original: None,
            ours: None,
        }
    }

    /// Records a paste. When the clipboard still held the previous paste's
    /// transcript, the snapshot is that transcript, so the older original is
    /// kept instead: two quick dictations restore what the user had copied.
    fn register(&mut self, snapshot: S, before: i64, after: i64) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        if self.ours != Some(before) {
            self.original = Some(snapshot);
        }
        self.ours = Some(after);
        self.generation
    }

    /// A paste that keeps its transcript: nothing earlier comes back.
    fn keep(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.original = None;
        self.ours = None;
    }

    /// What to restore for the paste `generation`, given the current change
    /// count. `None` when a newer paste took over or the user copied
    /// something in the meantime.
    fn take(&mut self, generation: u64, now: i64) -> Option<S> {
        if self.generation != generation {
            return None;
        }
        let ours = self.ours.take();
        let original = self.original.take();
        (ours == Some(now)).then_some(original).flatten()
    }
}

/// Writes `text` to the clipboard, runs `paste` and schedules the restore.
/// Returns the restore thread, if one was started.
pub(crate) fn paste_with<C: Clipboard>(
    clipboard: &Arc<C>,
    pending: &'static Mutex<PendingRestore<C::Snapshot>>,
    text: &str,
    restore: bool,
    restore_after: Duration,
    paste: impl FnOnce() -> Result<()>,
) -> Result<Option<JoinHandle<()>>> {
    let lock = move || pending.lock().unwrap_or_else(|error| error.into_inner());
    let before = clipboard.change_count();
    let snapshot = if restore {
        Some(clipboard.snapshot()?)
    } else {
        None
    };
    if let Err(error) = clipboard.write_text(text, restore) {
        if let Some(snapshot) = &snapshot
            && let Err(restore_error) = clipboard.restore(snapshot)
        {
            tracing::error!(%restore_error, "could not put the clipboard back after a failed write");
        }
        return Err(error);
    }
    let after = clipboard.change_count();
    let Some(snapshot) = snapshot else {
        lock().keep();
        paste()?;
        return Ok(None);
    };
    let generation = lock().register(snapshot, before, after);
    // The restore is scheduled even when the paste fails, so a failed paste
    // never leaves the transcript in place of the user's clipboard.
    let pasted = paste();
    let clipboard = Arc::clone(clipboard);
    let thread = std::thread::Builder::new()
        .name("tw-clipboard-restore".into())
        .spawn(move || {
            std::thread::sleep(restore_after);
            let original = lock().take(generation, clipboard.change_count());
            if let Some(original) = original
                && let Err(error) = clipboard.restore(&original)
            {
                tracing::warn!(%error, "could not restore the clipboard after pasting");
            }
        })?;
    pasted?;
    Ok(Some(thread))
}

/// Polls `released` until it holds or `timeout` passes. Returns whether it
/// held.
pub(crate) fn wait_for(timeout: Duration, released: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if released() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// One synthesised typing step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Stroke {
    /// At most `max_units` UTF-16 units, never splitting a surrogate pair.
    Text(Vec<u16>),
    /// `\n`, `\r\n` or `\r`; sent as Shift+Return on macOS so chat apps
    /// insert a line break instead of sending.
    LineBreak,
}

pub(crate) fn typing_strokes(text: &str, max_units: usize) -> Vec<Stroke> {
    let mut strokes = Vec::new();
    let mut units = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if matches!(c, '\r' | '\n') {
            if c == '\r' {
                chars.next_if_eq(&'\n');
            }
            if !units.is_empty() {
                strokes.push(Stroke::Text(std::mem::take(&mut units)));
            }
            strokes.push(Stroke::LineBreak);
            continue;
        }
        let mut buffer = [0; 2];
        let encoded = c.encode_utf16(&mut buffer);
        if units.len() + encoded.len() > max_units {
            strokes.push(Stroke::Text(std::mem::take(&mut units)));
        }
        units.extend_from_slice(encoded);
    }
    if !units.is_empty() {
        strokes.push(Stroke::Text(units));
    }
    strokes
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, Ordering};

    use super::*;
    use crate::OsError;

    #[derive(Default)]
    struct FakeClipboard {
        count: AtomicI64,
        content: Mutex<(String, bool)>,
    }

    impl FakeClipboard {
        fn holding(text: &str) -> Arc<Self> {
            let clipboard = Self::default();
            clipboard.set(text, false);
            Arc::new(clipboard)
        }

        fn set(&self, text: &str, transient: bool) {
            *self.content.lock().expect("lock") = (text.to_owned(), transient);
            self.count.fetch_add(1, Ordering::SeqCst);
        }

        fn text(&self) -> String {
            self.content.lock().expect("lock").0.clone()
        }

        fn transient(&self) -> bool {
            self.content.lock().expect("lock").1
        }
    }

    impl Clipboard for FakeClipboard {
        type Snapshot = String;

        fn change_count(&self) -> i64 {
            self.count.load(Ordering::SeqCst)
        }

        fn snapshot(&self) -> Result<String> {
            Ok(self.text())
        }

        fn write_text(&self, text: &str, transient: bool) -> Result<()> {
            self.set(text, transient);
            Ok(())
        }

        fn restore(&self, snapshot: &String) -> Result<()> {
            self.set(snapshot, false);
            Ok(())
        }
    }

    fn pending() -> &'static Mutex<PendingRestore<String>> {
        Box::leak(Box::new(Mutex::new(PendingRestore::new())))
    }

    fn join(thread: Option<JoinHandle<()>>) {
        thread
            .expect("restore scheduled")
            .join()
            .expect("restore thread");
    }

    #[test]
    fn pastes_marked_text_then_restores_the_clipboard() {
        let clipboard = FakeClipboard::holding("copied earlier");
        let seen = Mutex::new(None);
        let thread = paste_with(&clipboard, pending(), "hello", true, Duration::ZERO, || {
            *seen.lock().expect("lock") = Some((clipboard.text(), clipboard.transient()));
            Ok(())
        })
        .expect("paste");
        assert_eq!(
            seen.into_inner().expect("lock"),
            Some(("hello".into(), true))
        );
        join(thread);
        assert_eq!(clipboard.text(), "copied earlier");
    }

    #[test]
    fn a_copy_after_the_paste_is_not_clobbered() {
        let clipboard = FakeClipboard::holding("old");
        let thread = paste_with(
            &clipboard,
            pending(),
            "hello",
            true,
            Duration::from_millis(50),
            || Ok(()),
        )
        .expect("paste");
        clipboard.set("copied by the user", false);
        join(thread);
        assert_eq!(clipboard.text(), "copied by the user");
    }

    #[test]
    fn two_quick_pastes_restore_the_original() {
        let clipboard = FakeClipboard::holding("original");
        let state = pending();
        let first = paste_with(
            &clipboard,
            state,
            "one",
            true,
            Duration::from_millis(50),
            || Ok(()),
        )
        .expect("first");
        let second =
            paste_with(&clipboard, state, "two", true, Duration::ZERO, || Ok(())).expect("second");
        join(second);
        join(first);
        assert_eq!(clipboard.text(), "original");
    }

    #[test]
    fn without_restore_the_transcript_stays_as_an_ordinary_entry() {
        let clipboard = FakeClipboard::holding("old");
        let thread = paste_with(
            &clipboard,
            pending(),
            "keep me",
            false,
            Duration::ZERO,
            || Ok(()),
        )
        .expect("paste");
        assert!(thread.is_none());
        assert_eq!(clipboard.text(), "keep me");
        assert!(!clipboard.transient());
    }

    #[test]
    fn a_failed_paste_still_restores() {
        let clipboard = FakeClipboard::holding("old");
        let state = pending();
        let result = paste_with(&clipboard, state, "hello", true, Duration::ZERO, || {
            Err(OsError::Platform("no".into()))
        });
        assert!(result.is_err());
        // The restore thread was detached; wait for it to finish.
        assert!(wait_for(Duration::from_secs(2), || clipboard.text() == "old"));
    }

    #[test]
    fn typing_splits_into_chunks_without_breaking_surrogate_pairs() {
        let strokes = typing_strokes("abc😀d", 4);
        assert_eq!(
            strokes,
            vec![
                Stroke::Text("abc".encode_utf16().collect()),
                Stroke::Text("😀d".encode_utf16().collect()),
            ]
        );
        for stroke in typing_strokes(&"😀".repeat(30), 20) {
            let Stroke::Text(units) = stroke else {
                panic!("no line breaks here");
            };
            assert!(units.len() <= 20);
            assert!(String::from_utf16(&units).is_ok(), "pair kept together");
        }
    }

    #[test]
    fn typing_turns_every_newline_style_into_one_line_break() {
        let strokes = typing_strokes("a\r\nb\nc\rd\n", 20);
        let text = |s: &str| Stroke::Text(s.encode_utf16().collect());
        assert_eq!(
            strokes,
            vec![
                text("a"),
                Stroke::LineBreak,
                text("b"),
                Stroke::LineBreak,
                text("c"),
                Stroke::LineBreak,
                text("d"),
                Stroke::LineBreak,
            ]
        );
        assert!(typing_strokes("", 20).is_empty());
    }

    #[test]
    fn wait_for_reports_a_timeout() {
        assert!(wait_for(Duration::ZERO, || true));
        assert!(!wait_for(Duration::from_millis(20), || false));
    }
}
