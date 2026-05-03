use crate::interpreter::Interpreter;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::permissions;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct EmbeddedSourceConfig {
    pub source_label: String,
    pub allow_run: bool,
}

impl EmbeddedSourceConfig {
    pub fn new(source_label: impl Into<String>, allow_run: bool) -> Self {
        Self {
            source_label: source_label.into(),
            allow_run,
        }
    }
}

pub fn execute_source_standalone(source: &str, config: EmbeddedSourceConfig) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("failed to create Tokio runtime: {err}"))?;

    runtime.block_on(execute_source_on_current_runtime(source, config))
}

pub async fn execute_source_on_current_runtime(
    source: &str,
    config: EmbeddedSourceConfig,
) -> Result<(), String> {
    permissions::set_allow_run(config.allow_run);

    let mut lexer = Lexer::new(source);
    let tokens = lexer
        .tokenize()
        .map_err(|err| format!("{}: {}", config.source_label, err))?;
    let mut parser = Parser::new(tokens);
    let program = parser
        .parse_program()
        .map_err(|err| format!("{}: {}", config.source_label, err))?;

    let mut interpreter = Interpreter::new();
    interpreter.source = Some(source.to_string());
    interpreter.source_file = source_file_label(&config.source_label);
    interpreter.set_defer_host_runtime(true);

    interpreter
        .run(&program)
        .map_err(|err| format_runtime_error(source, &config.source_label, &err))?;

    let runtime_plan = super::metadata::extract_runtime_plan(&program);
    super::host::launch(interpreter, &runtime_plan)
        .await
        .map_err(|err| err.message)
}

fn source_file_label(label: &str) -> Option<PathBuf> {
    if label.is_empty() {
        return None;
    }

    Some(Path::new(label).to_path_buf())
}

fn format_runtime_error(
    source: &str,
    label: &str,
    err: &crate::interpreter::RuntimeError,
) -> String {
    if err.line > 0 {
        crate::errors::format_error(
            source,
            err.line,
            if err.col > 0 { err.col } else { 1 },
            &format!("[{}] {}", label, err.message),
        )
    } else {
        crate::errors::format_simple_error(&format!("[{}] {}", label, err.message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_source_runs_simple_program() {
        execute_source_standalone(
            "let x = 41\nlet y = x + 1",
            EmbeddedSourceConfig::new("inline.fg", false),
        )
        .expect("source should run");
    }

    #[test]
    fn standalone_source_rejects_shell_without_permission() {
        let err = execute_source_standalone(
            r#"sh("echo nope")"#,
            EmbeddedSourceConfig::new("shell.fg", false),
        )
        .expect_err("shell should be denied");

        assert!(
            err.contains("Shell execution denied"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn standalone_source_rejects_server_without_routes() {
        let err = execute_source_standalone(
            "@server(port: 3000)",
            EmbeddedSourceConfig::new("server.fg", false),
        )
        .expect_err("server without routes should fail");

        assert!(
            err.contains("@server defined but no route handlers found"),
            "unexpected error: {err}"
        );
    }
}
