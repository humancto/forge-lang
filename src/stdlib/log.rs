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
    let text: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
    let message = text.join(" ");

    // Ensure a tracing subscriber is installed. Idempotent -- the server
    // path also calls this on boot. Without this call, log.info from a
    // CLI-invoked script (where start_server never ran) would silently
    // drop because no subscriber is registered.
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
        "log.debug" => tracing::debug!(target: "forge.user", message = %message),
        _ => return Err(format!("unknown log function: {}", name)),
    }

    // 2. On a TTY only, also print the original colored line. Keeps
    //    `forge run script.fg` scannable interactively. Skipped when
    //    piped so escape codes never reach a log file.
    if std::io::stderr().is_terminal() {
        let now = chrono::Local::now().format("%H:%M:%S");
        match name {
            "log.info" => eprintln!("\x1B[32m[{} INFO]\x1B[0m  {}", now, message),
            "log.warn" => eprintln!("\x1B[33m[{} WARN]\x1B[0m  {}", now, message),
            "log.error" => eprintln!("\x1B[31m[{} ERROR]\x1B[0m {}", now, message),
            "log.debug" => eprintln!("\x1B[90m[{} DEBUG]\x1B[0m {}", now, message),
            _ => {}
        }
    }

    Ok(Value::Null)
}
