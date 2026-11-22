//! Turning token bytes into text: UTF-8 that a token split in two, and stop
//! strings that may arrive over several tokens.

/// Buffers bytes until they form complete UTF-8 characters.
#[derive(Default)]
pub struct Utf8Decoder {
    pending: Vec<u8>,
}

impl Utf8Decoder {
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let ready = match std::str::from_utf8(&self.pending) {
            Err(err) if err.error_len().is_none() => err.valid_up_to(),
            _ => self.pending.len(),
        };
        let text = String::from_utf8_lossy(&self.pending[..ready]).into_owned();
        self.pending.drain(..ready);
        text
    }

    pub fn finish(&mut self) -> String {
        let text = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        text
    }
}

/// Collects generated text, ends it before the first stop string, and holds
/// back what could still turn into one so streamed text never contains it.
pub struct StopScanner<'a> {
    stops: &'a [String],
    text: String,
    emitted: usize,
    stopped: bool,
}

impl<'a> StopScanner<'a> {
    pub fn new(stops: &'a [String]) -> Self {
        Self {
            stops,
            text: String::new(),
            emitted: 0,
            stopped: false,
        }
    }

    /// Returns the text that is safe to stream and whether a stop string
    /// was reached.
    pub fn push(&mut self, piece: &str) -> (String, bool) {
        if self.stopped {
            return (String::new(), true);
        }
        let scan_from = self.emitted;
        self.text.push_str(piece);
        let found = self
            .stops
            .iter()
            .filter(|s| !s.is_empty())
            .filter_map(|s| self.text[scan_from..].find(s.as_str()))
            .min();
        let safe = match found {
            Some(at) => {
                self.text.truncate(scan_from + at);
                self.stopped = true;
                self.text.len()
            }
            None => self.text.len() - self.held_back(),
        };
        let ready = self.text[self.emitted..safe].to_owned();
        self.emitted = safe;
        (ready, self.stopped)
    }

    /// The longest tail of the text that is the start of a stop string.
    fn held_back(&self) -> usize {
        self.stops
            .iter()
            .flat_map(|stop| {
                stop.char_indices()
                    .skip(1)
                    .map(|(i, _)| &stop[..i])
                    .filter(|prefix| self.text.ends_with(prefix))
                    .map(str::len)
            })
            .max()
            .unwrap_or(0)
    }

    /// The text not streamed yet.
    pub fn finish(&mut self) -> String {
        let rest = self.text[self.emitted..].to_owned();
        self.emitted = self.text.len();
        rest
    }

    pub fn into_text(self) -> String {
        self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_joins_split_characters() {
        let mut d = Utf8Decoder::default();
        let bytes = "é✓".as_bytes();
        assert_eq!(d.push(&bytes[..1]), "");
        assert_eq!(d.push(&bytes[1..3]), "é");
        assert_eq!(d.push(&bytes[3..]), "✓");
        assert_eq!(d.push(&[0xE2]), "");
        assert_eq!(d.finish(), "\u{FFFD}");
    }

    #[test]
    fn scanner_stops_across_pieces_and_never_streams_the_stop() {
        let stops = vec!["</text>".to_owned()];
        let mut s = StopScanner::new(&stops);
        let mut streamed = String::new();
        for piece in ["Hello", " world.</", "te", "xt> more"] {
            let (ready, stopped) = s.push(piece);
            streamed.push_str(&ready);
            if stopped {
                break;
            }
        }
        streamed.push_str(&s.finish());
        assert_eq!(streamed, "Hello world.");
        assert_eq!(s.into_text(), "Hello world.");
    }

    #[test]
    fn scanner_releases_a_false_alarm() {
        let stops = vec!["</text>".to_owned()];
        let mut s = StopScanner::new(&stops);
        assert_eq!(s.push("a </"), ("a ".to_owned(), false));
        assert_eq!(s.push("b>"), ("</b>".to_owned(), false));
        assert_eq!(s.push("<"), (String::new(), false));
        assert_eq!(s.finish(), "<");
        assert_eq!(s.into_text(), "a </b><");
    }

    #[test]
    fn scanner_without_stops_streams_everything() {
        let mut s = StopScanner::new(&[]);
        assert_eq!(s.push("abc"), ("abc".to_owned(), false));
        assert_eq!(s.finish(), "");
    }
}
