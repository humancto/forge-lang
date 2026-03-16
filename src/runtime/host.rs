use crate::interpreter::{Interpreter, RuntimeError, Value};

use super::metadata::{RuntimePlan, SchedulePlan, WatchPlan};

pub async fn launch(interpreter: Interpreter, plan: &RuntimePlan) -> Result<(), RuntimeError> {
    for schedule in &plan.schedules {
        spawn_schedule(&interpreter, schedule)?;
    }

    for watch in &plan.watches {
        spawn_watch(&interpreter, watch)?;
    }

    if let Some(server) = &plan.server {
        if server.routes.is_empty() {
            return Err(RuntimeError::new(
                "@server defined but no route handlers found. Add @get/@post functions.",
            ));
        }
        super::server::start_server(interpreter, server).await
    } else {
        Ok(())
    }
}

pub(crate) fn spawn_schedule(
    interpreter: &Interpreter,
    schedule: &SchedulePlan,
) -> Result<(), RuntimeError> {
    let secs = schedule_interval_seconds(interpreter, schedule)?;
    let body = schedule.body.clone();
    let mut sched_interp = interpreter.fork_for_background_runtime();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(secs));
        let _ = sched_interp.exec_background_block(&body);
    });
    Ok(())
}

pub(crate) fn spawn_watch(
    interpreter: &Interpreter,
    watch: &WatchPlan,
) -> Result<(), RuntimeError> {
    let path = watch_path(interpreter, watch)?;
    let body = watch.body.clone();
    let mut watch_interp = interpreter.fork_for_background_runtime();
    std::thread::spawn(move || {
        let mut last_modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let current = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            if current != last_modified {
                last_modified = current;
                let _ = watch_interp.exec_background_block(&body);
            }
        }
    });
    Ok(())
}

fn schedule_interval_seconds(
    interpreter: &Interpreter,
    schedule: &SchedulePlan,
) -> Result<u64, RuntimeError> {
    let mut eval_interp = interpreter.fork_for_background_runtime();
    match eval_interp.eval_expr(&schedule.interval)? {
        Value::Int(n) => Ok(match schedule.unit.as_str() {
            "minutes" => n as u64 * 60,
            "hours" => n as u64 * 3600,
            _ => n as u64,
        }),
        _ => Ok(60),
    }
}

fn watch_path(interpreter: &Interpreter, watch: &WatchPlan) -> Result<String, RuntimeError> {
    let mut eval_interp = interpreter.fork_for_background_runtime();
    match eval_interp.eval_expr(&watch.path)? {
        Value::String(path) => Ok(path),
        _ => Err(RuntimeError::new("watch requires a string path")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::runtime::metadata::extract_runtime_plan;

    fn parse_runtime_plan(src: &str) -> RuntimePlan {
        let tokens = Lexer::new(src).tokenize().expect("lex failed");
        let program = Parser::new(tokens).parse_program().expect("parse failed");
        extract_runtime_plan(&program)
    }

    #[test]
    fn schedule_interval_uses_runtime_bindings() {
        let mut interpreter = Interpreter::new();
        interpreter.env.define("delay".to_string(), Value::Int(5));
        let plan = parse_runtime_plan("schedule every delay minutes { let tick = 1 }\n");

        let secs = schedule_interval_seconds(&interpreter, &plan.schedules[0]).expect("seconds");
        assert_eq!(secs, 300);
    }

    #[test]
    fn watch_path_requires_string() {
        let mut interpreter = Interpreter::new();
        interpreter.env.define("path".to_string(), Value::Int(42));
        let plan = parse_runtime_plan("watch path { let changed = true }\n");

        let err = watch_path(&interpreter, &plan.watches[0]).expect_err("watch path error");
        assert_eq!(err.message, "watch requires a string path");
    }

    #[tokio::test]
    async fn launch_rejects_server_without_routes() {
        let interpreter = Interpreter::new();
        let plan = parse_runtime_plan("@server(port: 3000)\n");

        let err = launch(interpreter, &plan).await.expect_err("launch error");
        assert_eq!(
            err.message,
            "@server defined but no route handlers found. Add @get/@post functions."
        );
    }
}
