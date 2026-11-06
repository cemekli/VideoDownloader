//! Bounded line splitting for child-process output (DL-05: stdout/stderr are
//! never accumulated without limit).
//!
//! Bytes arrive in arbitrary chunks. [`LineSplitter`] yields complete lines,
//! cuts any line longer than its limit (keeping the head and dropping the rest
//! up to the next newline) and decodes invalid UTF-8 lossily. `\r` also ends a
//! line, so a progress bar without `--newline` cannot grow one line forever.

/// Default limit for one progress/stderr line. Our template lines are ~300 B.
pub const MAX_LINE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    /// The line was longer than the limit and was cut.
    pub truncated: bool,
}

#[derive(Debug)]
pub struct LineSplitter {
    max: usize,
    buf: Vec<u8>,
    /// Inside an over-long line: drop bytes until the next line end.
    dropping: bool,
}

impl Default for LineSplitter {
    fn default() -> Self {
        Self::new(MAX_LINE_BYTES)
    }
}

impl LineSplitter {
    pub fn new(max_line_bytes: usize) -> Self {
        assert!(max_line_bytes > 0);
        LineSplitter {
            max: max_line_bytes,
            buf: Vec::new(),
            dropping: false,
        }
    }

    /// Feeds one chunk; returns the lines it completed. Empty lines are skipped.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Line> {
        let mut out = Vec::new();
        for &b in chunk {
            if b == b'\n' || b == b'\r' {
                let truncated = std::mem::take(&mut self.dropping);
                self.emit(&mut out, truncated);
                continue;
            }
            if self.dropping {
                continue;
            }
            if self.buf.len() == self.max {
                self.dropping = true;
                continue;
            }
            self.buf.push(b);
        }
        out
    }

    /// End of stream: returns the unterminated last line, if any. A process
    /// killed mid-write leaves such a partial line.
    pub fn finish(&mut self) -> Option<Line> {
        let truncated = std::mem::take(&mut self.dropping);
        let mut out = Vec::new();
        self.emit(&mut out, truncated);
        out.pop()
    }

    /// Bytes currently held for an incomplete line (never more than the limit).
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    fn emit(&mut self, out: &mut Vec<Line>, truncated: bool) {
        if self.buf.is_empty() && !truncated {
            return;
        }
        let text = String::from_utf8_lossy(&self.buf).into_owned();
        self.buf.clear();
        out.push(Line { text, truncated });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: Vec<Line>) -> Vec<String> {
        lines.into_iter().map(|l| l.text).collect()
    }

    #[test]
    fn joins_chunks_and_splits_on_any_line_end() {
        let mut s = LineSplitter::new(64);
        assert!(s.push(b"ab").is_empty());
        assert_eq!(texts(s.push(b"c\r\nde\rf\n\n")), ["abc", "de", "f"]);
        assert_eq!(s.finish(), None);
    }

    #[test]
    fn cuts_long_lines_and_keeps_memory_bounded() {
        let mut s = LineSplitter::new(4);
        let mut lines = Vec::new();
        for _ in 0..10_000 {
            lines.extend(s.push(b"xxxxxxxx"));
            assert!(s.buffered() <= 4);
        }
        lines.extend(s.push(b"\nok\n"));
        assert_eq!(
            lines,
            [
                Line {
                    text: "xxxx".into(),
                    truncated: true
                },
                Line {
                    text: "ok".into(),
                    truncated: false
                },
            ]
        );
    }

    #[test]
    fn partial_last_line_and_invalid_utf8() {
        let mut s = LineSplitter::new(64);
        assert_eq!(texts(s.push("İş\n".as_bytes())), ["İş"]);
        s.push(&[b'a', 0xff, b'b']);
        let last = s.finish().unwrap();
        assert_eq!(last.text, "a\u{fffd}b");
        assert!(!last.truncated);
    }
}
