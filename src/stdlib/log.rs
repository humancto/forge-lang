//! Forge `log` stdlib module — user-facing logging surface.
//!
//! Each call emits a structured `tracing` event with stable
//! `target = "forge.user"` so users can filter their own log volume
//! independently from runtime noise:
//!
//! ```bash
//! # quiet runtime, keep user logs at info
//! FORGE_LOG=forge_lang=warn,tower_http=warn,forge.user=info forge run app.fg
//! ```
//!
//! Additionally, when stderr is a terminal, the original colored
//! `eprintln!` output is preserved so interactive `forge run` keeps
//! its scannable green/yellow/red output. When stderr is piped
//! (CI, log capture), only the structured event fires — no ANSI
//! escape codes leak into log files.
//!
//! This dual-output pattern matches the server's startup banner
//! decision: structured event always, human-friendly extras on TTY.
//!
//! Inside an output capture that takes stderr (a `Sandbox` run, `forge
//! mcp`), neither happens: the call writes one plain `[level] message`
//! line into the capture, so the host (an agent) sees its program's logs
//! and they count against the run's output budget. The process's tracing
//! subscriber writes to the host's stderr, outside both. The browser
//! playground (no `host` feature, no subscriber) keeps its timestamped
//! line.
use crate::interpreter::Value;
use indexmap::IndexMap;
use std::io::IsTerminal;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("info".to_string(), Value::BuiltIn("log.info".to_string()));
    m.insert("warn".to_string(), Value::BuiltIn("log.warn".to_string()));
    m.insert("error".to_string(), Value::BuiltIn("log.error".to_string()));
    m.insert("debug".to_string(), Value::BuiltIn("log.debug".to_string()));
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    let level = match name {
        "log.info" => "info",
        "log.warn" => "warn",
        "log.error" => "error",
        "log.debug" => "debug",
        _ => return Err(format!("unknown log function: {}", name)),
    };
    let text: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
    let message = text.join(" ");

    // A capture that takes stderr gets the line, and nothing reaches the
    // tracing subscriber (the host's stderr, unbounded by the run's
    // output budget).
    #[cfg(feature = "host")]
    if crate::runtime::stdio::captures(crate::runtime::stdio::Stream::Stderr) {
        crate::runtime::stdio::err_line(&captured_line(level, &message));
        return Ok(Value::Null);
    }

    // Ensure a tracing subscriber is installed. Idempotent -- the server
    // path also calls this on boot. Without this call, log.info from a
    // CLI-invoked script (where start_server never ran) would silently
    // drop because no subscriber is registered.
    #[cfg(feature = "host")]
    crate::runtime::tracing_init::init_subscriber();

    // 1. Always emit the structured tracing event. This is what log
    //    aggregators consume. If called from inside an HTTP handler,
    //    the per-request span context (method, uri, handler) is
    //    inherited automatically because the server propagates
    //    Span::current() across spawn_blocking.
    match name {
        "log.info" => tracing::info!(target: "forge.user", message = %message),
        "log.warn" => tracing::warn!(target: "forge.user", message = %message),
        "log.error" => tracing::error!(target: "forge.user", message = %message),
        _ => tracing::debug!(target: "forge.user", message = %message),
    }

    // 2. On a TTY only, also print the original colored line. Keeps
    //    `forge run script.fg` scannable interactively. Skipped when
    //    piped so escape codes never reach a log file. Without the host
    //    runtime (browser playground) no subscriber exists, so this line
    //    is the only output and is always written.
    if std::io::stderr().is_terminal() || !cfg!(feature = "host") {
        let now = chrono::Local::now().format("%H:%M:%S");
        let (code, label, pad) = match level {
            "info" => ("32", "INFO", "  "),
            "warn" => ("33", "WARN", "  "),
            "error" => ("31", "ERROR", " "),
            _ => ("90", "DEBUG", " "),
        };
        // Only the prefix is chrome; the message is the program's data and
        // is printed untouched.
        let prefix = crate::color::paint(
            crate::color::Stream::Stderr,
            code,
            &format!("[{} {}]", now, label),
        );
        crate::runtime::stdio::err_line(&format!("{}{}{}", prefix, pad, message));
    }

    Ok(Value::Null)
}

/// The line `log.*` writes into an output capture: `[info] message`.
pub(crate) fn captured_line(level: &str, message: &str) -> String {
    format!("[{}] {}", level, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::stdio::{self, Stream};

    #[test]
    fn logs_go_to_a_stderr_capture_as_plain_lines() {
        let guard = stdio::capture(1024);
        call(
            "log.info",
            vec![Value::String("ready".into()), Value::Int(3)],
        )
        .expect("info");
        call("log.warn", vec![Value::String("careful".into())]).expect("warn");
        call("log.error", vec![Value::String("bad".into())]).expect("error");
        call("log.debug", vec![Value::String("detail".into())]).expect("debug");
        assert!(call("log.nope", vec![]).is_err());
        let got = guard.finish();
        assert_eq!(got.text(Stream::Stdout), "");
        let stderr = got.text(Stream::Stderr);
        if cfg!(feature = "host") {
            assert_eq!(
                stderr,
                "[info] ready 3\n[warn] careful\n[error] bad\n[debug] detail\n"
            );
        } else {
            assert!(stderr.contains("ready 3"), "{stderr}");
        }
    }
}
