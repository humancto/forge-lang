//! Where program output goes.
//!
//! Every byte a Forge program prints on the engines' own paths (`say`,
//! `print`, `yell`, `whisper`, `io.print`, the GenZ debug kit, `term.*` and
//! `log.*` chrome) is written through [`out`] / [`err`]. By default they are
//! exactly `print!` / `eprint!`. A host that cannot use the process streams
//! installs a capture and reads the text back, in order, with the stream
//! each piece was written to:
//!
//! * the browser playground (`wasm32-unknown-unknown` has no stdout) uses
//!   [`capture`];
//! * the sandbox (`crate::sandbox`) installs a [`Sink::bounded`] with
//!   [`scope`] on its worker thread, so `forge mcp` and other embedders get
//!   a script's stdout and stderr (`eprintln`, `log.*`, `sus`, `term.*`
//!   chrome), bounded by one output budget, instead of the host's streams.
//!
//! While a capture takes stderr, `log.*` writes a plain `[level] message`
//! line into it instead of emitting a `tracing` event (the process's
//! subscriber writes to the host's stderr, outside the capture and its
//! budget), and color chrome is off for every captured stream: a capture
//! is not a terminal.
//!
//! # Invariant: forks inherit the capture
//!
//! A capture is a shared [`Sink`] installed per thread.
//! `crate::permissions::inherit` (and therefore `permissions::spawn` and
//! `recursion::spawn_worker`) carries the current sink into every thread an
//! engine starts, together with the permission policy and the resource
//! budget, so output from `spawn`ed tasks, `timeout` bodies and squads on
//! either engine lands in the same capture.
//!
//! Invariant: with no capture installed, behaviour is identical to
//! `print!`/`eprint!` (including panicking on a closed stdout).

// The capture API is for library embedders (bindings/wasm, the sandbox);
// the `forge` binary, which compiles this module too, only writes.
#![allow(dead_code)]

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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

    /// The first `limit` bytes written, in write order, cut on a character
    /// boundary, split into `(stdout, stderr)`.
    pub fn split_within(&self, limit: usize) -> (String, String) {
        let (mut stdout, mut stderr) = (String::new(), String::new());
        let mut room = limit;
        for (stream, text) in &self.chunks {
            if room == 0 {
                break;
            }
            let mut take = text.len().min(room);
            while !text.is_char_boundary(take) {
                take -= 1;
            }
            room -= take;
            match stream {
                Stream::Stdout => stdout.push_str(&text[..take]),
                Stream::Stderr => stderr.push_str(&text[..take]),
            }
            if take < text.len() {
                break;
            }
        }
        (stdout, stderr)
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

/// One capture's shared state. Every thread that inherits the capture
/// ([`current_sink`] / [`scope`]) writes into the same sink.
pub struct Sink {
    limit: usize,
    /// Capture stderr too; otherwise stderr goes to the host's stderr
    /// exactly as without a capture.
    stderr: bool,
    /// Stored `true` the first time output is dropped for the limit (the
    /// sandbox points it at the run's cancellation flag, so a runaway
    /// print loop stops at its next safe point).
    on_overflow: Option<Arc<AtomicBool>>,
    captured: Mutex<Captured>,
}

impl std::fmt::Debug for Sink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sink")
            .field("limit", &self.limit)
            .field("stderr", &self.stderr)
            .finish()
    }
}

impl Sink {
    /// A sink that keeps at most `limit` bytes of stdout and stderr.
    pub fn new(limit: usize) -> Arc<Sink> {
        Arc::new(Sink {
            limit,
            stderr: true,
            on_overflow: None,
            captured: Mutex::new(Captured::default()),
        })
    }

    /// A sink that keeps at most `limit` bytes of stdout and stderr
    /// together (one budget for both streams) and stores `true` in
    /// `on_overflow` once output past `limit` is dropped.
    pub fn bounded(limit: usize, on_overflow: Option<Arc<AtomicBool>>) -> Arc<Sink> {
        Arc::new(Sink {
            limit,
            stderr: true,
            on_overflow,
            captured: Mutex::new(Captured::default()),
        })
    }

    /// Whether this sink takes output written to `stream`.
    pub fn takes(&self, stream: Stream) -> bool {
        stream == Stream::Stdout || self.stderr
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Captured> {
        // A poisoned sink still captures: output must never fall back to
        // the host's streams once a capture is installed.
        self.captured.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Returns false when this sink does not take `stream`.
    fn write(&self, stream: Stream, text: &str) -> bool {
        if stream == Stream::Stderr && !self.stderr {
            return false;
        }
        let overflowed = {
            let mut captured = self.lock();
            let before = captured.truncated;
            captured.push(stream, text, self.limit);
            !before && captured.truncated
        };
        if overflowed {
            if let Some(flag) = &self.on_overflow {
                flag.store(true, Ordering::Release);
            }
        }
        true
    }

    /// Whether output past the limit has been dropped.
    pub fn truncated(&self) -> bool {
        self.lock().truncated
    }

    /// Bytes captured so far (at most the limit).
    pub fn bytes(&self) -> usize {
        self.lock().bytes
    }

    /// Everything captured so far.
    pub fn snapshot(&self) -> Captured {
        self.lock().clone()
    }

    /// Take everything captured so far, leaving the sink empty (its byte
    /// count and truncation state are kept, so the limit still holds).
    pub fn take(&self) -> Captured {
        let mut captured = self.lock();
        let kept = Captured {
            chunks: Vec::new(),
            bytes: captured.bytes,
            truncated: captured.truncated,
        };
        std::mem::replace(&mut *captured, kept)
    }
}

thread_local! {
    static CAPTURE: RefCell<Option<Arc<Sink>>> = const { RefCell::new(None) };
}

/// The capture installed on this thread, if any.
pub fn current_sink() -> Option<Arc<Sink>> {
    CAPTURE.try_with(|c| c.borrow().clone()).ok().flatten()
}

/// Whether output written to `stream` on this thread goes to a capture
/// (rather than the process's stream).
pub fn captures(stream: Stream) -> bool {
    current_sink().is_some_and(|sink| sink.takes(stream))
}

/// Restores the previous capture of this thread when dropped.
#[must_use = "output is only captured while the guard lives"]
pub struct ScopeGuard {
    previous: Option<Arc<Sink>>,
    ended: bool,
}

impl ScopeGuard {
    fn end(&mut self) {
        if !std::mem::replace(&mut self.ended, true) {
            let previous = self.previous.take();
            let _ = CAPTURE.try_with(|c| *c.borrow_mut() = previous);
        }
    }
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        self.end();
    }
}

/// Send this thread's output to `sink` (`None`: to the process streams)
/// until the guard is dropped.
pub fn scope(sink: Option<Arc<Sink>>) -> ScopeGuard {
    let previous = CAPTURE
        .try_with(|c| std::mem::replace(&mut *c.borrow_mut(), sink))
        .ok()
        .flatten();
    ScopeGuard {
        previous,
        ended: false,
    }
}

/// Captures this thread's output (and that of the threads it starts) until
/// [`CaptureGuard::finish`] (or drop).
#[must_use = "output is only captured while the guard lives"]
pub struct CaptureGuard {
    sink: Arc<Sink>,
    scope: ScopeGuard,
}

impl CaptureGuard {
    /// Stop capturing and return what was written.
    pub fn finish(mut self) -> Captured {
        self.scope.end();
        self.sink.take()
    }

    /// Whether the limit has been reached (output is being dropped).
    pub fn truncated(&self) -> bool {
        self.sink.truncated()
    }
}

/// Capture this thread's output, keeping at most `limit` bytes. Nested
/// captures restore the outer one when they finish.
pub fn capture(limit: usize) -> CaptureGuard {
    let sink = Sink::new(limit);
    CaptureGuard {
        scope: scope(Some(sink.clone())),
        sink,
    }
}

fn write(stream: Stream, text: &str) -> bool {
    match current_sink() {
        Some(sink) => sink.write(stream, text),
        None => false,
    }
}

/// Write to the process's stdout. When the reader has gone away
/// (`forge run app.fg | head`), exit quietly like other command-line
/// tools instead of panicking with a backtrace (`print!` panics).
fn process_stdout(text: &str, newline: bool) {
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    let result = stdout.write_all(text.as_bytes()).and_then(|()| {
        if newline {
            stdout.write_all(b"\n")?;
        }
        if newline || text.contains('\n') {
            stdout.flush()?;
        }
        Ok(())
    });
    if let Err(e) = result {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            #[cfg(not(target_arch = "wasm32"))]
            std::process::exit(0);
        }
    }
}

/// Write `text` to the program's stdout.
pub fn out(text: &str) {
    if !write(Stream::Stdout, text) {
        process_stdout(text, false);
    }
}

/// Write `text` and a newline to the program's stdout.
pub fn out_line(text: &str) {
    match current_sink() {
        // One write, so another thread's output cannot land between the
        // text and its newline.
        Some(sink) if sink.write(Stream::Stdout, &format!("{}\n", text)) => {}
        _ => process_stdout(text, true),
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
    match current_sink() {
        Some(sink) if sink.write(Stream::Stderr, &format!("{}\n", text)) => {}
        _ => eprintln!("{}", text),
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

    #[test]
    fn threads_started_with_inherit_share_the_capture() {
        let guard = capture(1024);
        out_line("parent");
        crate::permissions::spawn(|| out_line("child"))
            .join()
            .expect("child thread");
        // A bare thread does not inherit it (and writes to the real stdout).
        assert!(std::thread::spawn(current_sink)
            .join()
            .expect("bare thread")
            .is_none());
        assert_eq!(guard.finish().text(Stream::Stdout), "parent\nchild\n");
    }

    #[test]
    fn bounded_sinks_share_one_budget_and_flag_overflow() {
        let flag = Arc::new(AtomicBool::new(false));
        let sink = Sink::bounded(6, Some(flag.clone()));
        let _scope = scope(Some(sink.clone()));
        assert!(captures(Stream::Stdout) && captures(Stream::Stderr));
        out("abc");
        err("de");
        assert!(!flag.load(Ordering::Acquire));
        err("fg");
        assert!(flag.load(Ordering::Acquire));
        assert!(sink.truncated());
        let got = sink.take();
        assert_eq!(got.text(Stream::Stdout), "abc");
        assert_eq!(got.text(Stream::Stderr), "def");
    }

    #[test]
    fn split_within_keeps_write_order_and_char_boundaries() {
        let got = Captured {
            chunks: vec![
                (Stream::Stdout, "ab".to_string()),
                (Stream::Stderr, "cé".to_string()),
                (Stream::Stdout, "z".to_string()),
            ],
            bytes: 6,
            truncated: false,
        };
        assert_eq!(got.split_within(3), ("ab".to_string(), "c".to_string()));
        assert_eq!(got.split_within(4), ("ab".to_string(), "c".to_string()));
        assert_eq!(got.split_within(5), ("ab".to_string(), "cé".to_string()));
        assert_eq!(got.split_within(99), ("abz".to_string(), "cé".to_string()));
        assert_eq!(got.split_within(0), (String::new(), String::new()));
    }

    #[test]
    fn no_capture_takes_nothing() {
        let _scope = scope(None);
        assert!(!captures(Stream::Stdout) && !captures(Stream::Stderr));
    }
}
