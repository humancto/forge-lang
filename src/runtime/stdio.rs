//! Where program output goes.
//!
//! Every byte a Forge program prints on the engines' own paths (`say`,
//! `print`, `yell`, `whisper`, `io.print`, the GenZ debug kit, `term.*` and
//! `log.*` chrome) is written through [`out`] / [`err`]. By default they are
//! exactly `print!` / `eprint!`. A host that cannot use the process streams
//! (the browser playground: `wasm32-unknown-unknown` has no stdout) installs
//! a per-thread [`capture`] and reads the text back, in order, with the
//! stream each piece was written to.
//!
//! Invariant: with no capture installed, behaviour is identical to
//! `print!`/`eprint!` (including panicking on a closed stdout).

// The capture API is for library embedders (bindings/wasm); the `forge`
// binary, which compiles this module too, only writes.
#![allow(dead_code)]

use std::cell::RefCell;

/// The stream a piece of output was written to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// Output collected by a [`capture`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Captured {
    /// Consecutive writes to the same stream are merged.
    pub chunks: Vec<(Stream, String)>,
    /// Bytes accepted (at most the capture's limit).
    pub bytes: usize,
    /// True when output past the limit was dropped.
    pub truncated: bool,
}

impl Captured {
    /// Everything written to `stream`, concatenated.
    pub fn text(&self, stream: Stream) -> String {
        self.chunks
            .iter()
            .filter(|(s, _)| *s == stream)
            .map(|(_, t)| t.as_str())
            .collect()
    }

    fn push(&mut self, stream: Stream, text: &str, limit: usize) {
        if self.truncated || text.is_empty() {
            return;
        }
        let room = limit.saturating_sub(self.bytes);
        let mut take = text.len().min(room);
        if take < text.len() {
            self.truncated = true;
            while !text.is_char_boundary(take) {
                take -= 1;
            }
        }
        if take == 0 {
            return;
        }
        let piece = &text[..take];
        self.bytes += take;
        match self.chunks.last_mut() {
            Some((s, buf)) if *s == stream => buf.push_str(piece),
            _ => self.chunks.push((stream, piece.to_string())),
        }
    }
}

struct Active {
    limit: usize,
    captured: Captured,
}

thread_local! {
    static CAPTURE: RefCell<Option<Active>> = const { RefCell::new(None) };
}

/// Captures this thread's output until [`CaptureGuard::finish`] (or drop).
#[must_use = "output is only captured while the guard lives"]
pub struct CaptureGuard {
    /// The capture this one replaced (restored when it ends).
    previous: Option<Active>,
    /// Set once the capture has ended (by `finish` or drop).
    ended: bool,
}

impl CaptureGuard {
    /// Stop capturing and return what was written.
    pub fn finish(mut self) -> Captured {
        self.take()
    }

    /// Whether the limit has been reached (output is being dropped).
    pub fn truncated(&self) -> bool {
        CAPTURE.with(|c| c.borrow().as_ref().is_some_and(|a| a.captured.truncated))
    }

    fn take(&mut self) -> Captured {
        if std::mem::replace(&mut self.ended, true) {
            return Captured::default();
        }
        let previous = self.previous.take();
        CAPTURE
            .with(|c| std::mem::replace(&mut *c.borrow_mut(), previous))
            .map(|a| a.captured)
            .unwrap_or_default()
    }
}

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        let _ = self.take();
    }
}

/// Capture this thread's output, keeping at most `limit` bytes. Nested
/// captures restore the outer one when they finish.
pub fn capture(limit: usize) -> CaptureGuard {
    let previous = CAPTURE.with(|c| {
        c.borrow_mut().replace(Active {
            limit,
            captured: Captured::default(),
        })
    });
    CaptureGuard {
        previous,
        ended: false,
    }
}

fn write(stream: Stream, text: &str) -> bool {
    CAPTURE.with(|c| match c.borrow_mut().as_mut() {
        Some(active) => {
            let limit = active.limit;
            active.captured.push(stream, text, limit);
            true
        }
        None => false,
    })
}

/// Write `text` to the program's stdout.
pub fn out(text: &str) {
    if !write(Stream::Stdout, text) {
        print!("{}", text);
    }
}

/// Write `text` and a newline to the program's stdout.
pub fn out_line(text: &str) {
    if !write(Stream::Stdout, text) {
        println!("{}", text);
    } else {
        write(Stream::Stdout, "\n");
    }
}

/// Write `text` to the program's stderr.
pub fn err(text: &str) {
    if !write(Stream::Stderr, text) {
        eprint!("{}", text);
    }
}

/// Write `text` and a newline to the program's stderr.
pub fn err_line(text: &str) {
    if !write(Stream::Stderr, text) {
        eprintln!("{}", text);
    } else {
        write(Stream::Stderr, "\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_both_streams_in_order() {
        let guard = capture(1024);
        out_line("a");
        err("warn");
        err_line("!");
        out("b");
        let got = guard.finish();
        assert_eq!(
            got.chunks,
            vec![
                (Stream::Stdout, "a\n".to_string()),
                (Stream::Stderr, "warn!\n".to_string()),
                (Stream::Stdout, "b".to_string()),
            ]
        );
        assert_eq!(got.text(Stream::Stdout), "a\nb");
        assert!(!got.truncated);
    }

    #[test]
    fn limit_truncates_on_a_char_boundary() {
        let guard = capture(5);
        out("abcd");
        out("é!"); // 'é' is two bytes and does not fit
        out("more");
        assert!(guard.truncated());
        let got = guard.finish();
        assert_eq!(got.text(Stream::Stdout), "abcd");
        assert!(got.truncated);
    }

    #[test]
    fn nested_captures_restore_the_outer_one() {
        let outer = capture(100);
        out("1");
        let inner = capture(100);
        out("2");
        assert_eq!(inner.finish().text(Stream::Stdout), "2");
        out("3");
        assert_eq!(outer.finish().text(Stream::Stdout), "13");
    }
}
