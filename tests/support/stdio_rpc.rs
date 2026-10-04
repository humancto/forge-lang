//! Shared harness for end-to-end tests that drive `forge lsp` / `forge dap`
//! over real stdio pipes using `Content-Length` framing.
//!
//! Every blocking operation has a timeout and the child process is killed on
//! drop, so a server regression (e.g. a stdin deadlock) fails the test
//! instead of hanging CI.

#![allow(dead_code)]

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

pub const TIMEOUT: Duration = Duration::from_secs(20);

pub struct StdioServer {
    child: Child,
    stdin: Option<ChildStdin>,
    messages: Receiver<Value>,
}

/// Read one framed message. Returns `None` on EOF or malformed framing.
fn read_framed(reader: &mut impl BufRead) -> Option<Value> {
    let mut length: Option<usize> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().ok();
            }
        }
    }
    let mut body = vec![0u8; length?];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

pub fn frame(msg: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(msg).expect("serialize message");
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(&body);
    out
}

impl StdioServer {
    /// Spawn `forge <subcommand>` with piped stdio.
    pub fn spawn(subcommand: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_forge"))
            .arg(subcommand)
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn forge");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("child stdout");
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Some(msg) = read_framed(&mut reader) {
                if tx.send(msg).is_err() {
                    break;
                }
            }
        });
        StdioServer {
            child,
            stdin,
            messages: rx,
        }
    }

    pub fn write_raw(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("stdin already closed");
        stdin.write_all(bytes).expect("write to server stdin");
        stdin.flush().expect("flush server stdin");
    }

    pub fn send(&mut self, msg: Value) {
        let bytes = frame(&msg);
        self.write_raw(&bytes);
    }

    pub fn close_stdin(&mut self) {
        self.stdin.take();
    }

    /// Next message from the server, panicking after [`TIMEOUT`].
    pub fn recv(&self) -> Value {
        match self.messages.recv_timeout(TIMEOUT) {
            Ok(msg) => msg,
            Err(RecvTimeoutError::Timeout) => {
                panic!("server sent nothing within {:?} (deadlock?)", TIMEOUT)
            }
            Err(RecvTimeoutError::Disconnected) => panic!("server closed stdout unexpectedly"),
        }
    }

    /// Receive messages until one satisfies `pred`; returns it.
    pub fn recv_until(&self, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.messages.recv_timeout(left) {
                Ok(msg) if pred(&msg) => return msg,
                Ok(_) => continue,
                Err(_) => panic!("timed out after {:?} waiting for {}", TIMEOUT, what),
            }
        }
    }

    /// Wait for the process to exit, panicking after [`TIMEOUT`].
    pub fn wait_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                return status;
            }
            if Instant::now() > deadline {
                panic!("server did not exit within {:?}", TIMEOUT);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for StdioServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
