use crate::interpreter::{DebugAction, DebugFrame, DebugState, Interpreter, Value};
use crate::lexer::Lexer;
use crate::parser::Parser;
use serde_json::{json, Value as JsonValue};
use std::collections::HashSet;
use std::io::{self, BufRead, Read as _, Write};
use std::sync::{Arc, Mutex};

/// Run the DAP server over stdin/stdout.
pub fn run_dap() {
    let stdin = io::stdin();
    let stdout = Arc::new(Mutex::new(io::stdout()));

    eprintln!("Forge DAP server started");

    let seq = Arc::new(Mutex::new(1i64));
    let mut pending_breakpoints: HashSet<usize> = HashSet::new();
    let mut interpreter_handle: Option<InterpreterSession> = None;

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.starts_with("Content-Length:") {
            let len: usize = line
                .trim_start_matches("Content-Length:")
                .trim()
                .parse()
                .unwrap_or(0);

            // Read empty line separator
            let mut empty = String::new();
            io::stdin().read_line(&mut empty).ok();

            // Read content body
            let mut content = vec![0u8; len];
            io::stdin().lock().read_exact(&mut content).ok();
            let body = String::from_utf8_lossy(&content).to_string();

            let msg: JsonValue = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let command = msg["command"].as_str().unwrap_or("");
            let request_seq = msg["seq"].as_i64().unwrap_or(0);
            let args = &msg["arguments"];

            match command {
                "initialize" => {
                    let resp = make_response(
                        request_seq,
                        command,
                        &seq,
                        json!({
                            "supportsConfigurationDoneRequest": true,
                            "supportsFunctionBreakpoints": false,
                            "supportsConditionalBreakpoints": false,
                            "supportsEvaluateForHovers": false,
                            "supportsStepBack": false,
                            "supportsSetVariable": false,
                            "supportsRestartFrame": false,
                            "supportsGotoTargetsRequest": false,
                            "supportsCompletionsRequest": false,
                            "supportsModulesRequest": false,
                            "supportsExceptionOptions": false,
                            "supportsTerminateRequest": true,
                        }),
                    );
                    send_message(&stdout, &resp);

                    // Send initialized event
                    let event = make_event("initialized", &seq, json!({}));
                    send_message(&stdout, &event);
                }

                "launch" => {
                    let program = args["program"].as_str().unwrap_or("").to_string();
                    let stop_on_entry = args["stopOnEntry"].as_bool().unwrap_or(false);

                    let resp = make_response(request_seq, command, &seq, json!(null));
                    send_message(&stdout, &resp);

                    // Launch the interpreter in a background thread
                    let session = launch_interpreter(&program, stop_on_entry, stdout.clone(), &seq);

                    match session {
                        Ok(s) => {
                            // Apply any pre-launch breakpoints
                            if !pending_breakpoints.is_empty() {
                                let mut bps = s
                                    .debug_state
                                    .breakpoints
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner());
                                for &l in &pending_breakpoints {
                                    bps.insert(l);
                                }
                                pending_breakpoints.clear();
                            }
                            interpreter_handle = Some(s);
                        }
                        Err(e) => {
                            let event = make_event(
                                "output",
                                &seq,
                                json!({
                                    "category": "stderr",
                                    "output": format!("Launch failed: {}\n", e),
                                }),
                            );
                            send_message(&stdout, &event);
                            let event = make_event("terminated", &seq, json!({}));
                            send_message(&stdout, &event);
                        }
                    }
                }

                "configurationDone" => {
                    let resp = make_response(request_seq, command, &seq, json!(null));
                    send_message(&stdout, &resp);
                }

                "setBreakpoints" => {
                    let lines: Vec<usize> = args["breakpoints"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|bp| bp["line"].as_u64().map(|l| l as usize))
                                .collect()
                        })
                        .unwrap_or_default();

                    if let Some(ref session) = interpreter_handle {
                        let mut bps = session
                            .debug_state
                            .breakpoints
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        bps.clear();
                        for &l in &lines {
                            bps.insert(l);
                        }
                    } else {
                        // Buffer breakpoints before launch
                        pending_breakpoints.clear();
                        for &l in &lines {
                            pending_breakpoints.insert(l);
                        }
                    }

                    let verified: Vec<JsonValue> = lines
                        .iter()
                        .map(|l| json!({"verified": true, "line": l}))
                        .collect();

                    let resp = make_response(
                        request_seq,
                        command,
                        &seq,
                        json!({
                            "breakpoints": verified,
                        }),
                    );
                    send_message(&stdout, &resp);
                }

                "threads" => {
                    let resp = make_response(
                        request_seq,
                        command,
                        &seq,
                        json!({
                            "threads": [{"id": 1, "name": "main"}],
                        }),
                    );
                    send_message(&stdout, &resp);
                }

                "stackTrace" => {
                    let frames = if let Some(ref session) = interpreter_handle {
                        let stack = session
                            .debug_state
                            .call_frames
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        let current_line = session
                            .current_line
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());

                        let mut frames = Vec::new();
                        // Current position as top frame
                        frames.push(json!({
                            "id": 0,
                            "name": stack.last().map(|f| f.name.as_str()).unwrap_or("<main>"),
                            "line": *current_line,
                            "column": 1,
                            "source": {
                                "name": &session.source_name,
                                "path": &session.source_path,
                            }
                        }));

                        // Call stack frames (reversed, most recent first)
                        for (i, frame) in stack.iter().rev().skip(1).enumerate() {
                            frames.push(json!({
                                "id": i + 1,
                                "name": &frame.name,
                                "line": frame.line,
                                "column": 1,
                                "source": {
                                    "name": &session.source_name,
                                    "path": &session.source_path,
                                }
                            }));
                        }
                        frames
                    } else {
                        vec![]
                    };

                    let resp = make_response(
                        request_seq,
                        command,
                        &seq,
                        json!({
                            "stackFrames": frames,
                            "totalFrames": frames.len(),
                        }),
                    );
                    send_message(&stdout, &resp);
                }

                "scopes" => {
                    let resp = make_response(
                        request_seq,
                        command,
                        &seq,
                        json!({
                            "scopes": [{
                                "name": "Locals",
                                "variablesReference": 1,
                                "expensive": false,
                            }],
                        }),
                    );
                    send_message(&stdout, &resp);
                }

                "variables" => {
                    let variables = if let Some(ref session) = interpreter_handle {
                        let vars = session
                            .debug_state
                            .variables
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        vars.iter()
                            .map(|(name, value)| {
                                json!({
                                    "name": name,
                                    "value": value,
                                    "variablesReference": 0,
                                })
                            })
                            .collect::<Vec<_>>()
                    } else {
                        vec![]
                    };

                    let resp = make_response(
                        request_seq,
                        command,
                        &seq,
                        json!({
                            "variables": variables,
                        }),
                    );
                    send_message(&stdout, &resp);
                }

                "continue" => {
                    if let Some(ref session) = interpreter_handle {
                        resume_interpreter(&session.debug_state, DebugAction::Continue, 0);
                    }
                    let resp = make_response(
                        request_seq,
                        command,
                        &seq,
                        json!({
                            "allThreadsContinued": true,
                        }),
                    );
                    send_message(&stdout, &resp);
                }

                "next" => {
                    if let Some(ref session) = interpreter_handle {
                        let depth = *session
                            .debug_state
                            .paused_depth
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        resume_interpreter(&session.debug_state, DebugAction::StepOver, depth);
                    }
                    let resp = make_response(request_seq, command, &seq, json!(null));
                    send_message(&stdout, &resp);
                }

                "stepIn" => {
                    if let Some(ref session) = interpreter_handle {
                        resume_interpreter(&session.debug_state, DebugAction::StepIn, 0);
                    }
                    let resp = make_response(request_seq, command, &seq, json!(null));
                    send_message(&stdout, &resp);
                }

                "stepOut" => {
                    if let Some(ref session) = interpreter_handle {
                        let depth = *session
                            .debug_state
                            .paused_depth
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        resume_interpreter(&session.debug_state, DebugAction::StepOut, depth);
                    }
                    let resp = make_response(request_seq, command, &seq, json!(null));
                    send_message(&stdout, &resp);
                }

                "pause" => {
                    if let Some(ref session) = interpreter_handle {
                        *session
                            .debug_state
                            .action
                            .lock()
                            .unwrap_or_else(|e| e.into_inner()) = DebugAction::Pause;
                    }
                    let resp = make_response(request_seq, command, &seq, json!(null));
                    send_message(&stdout, &resp);
                }

                "disconnect" | "terminate" => {
                    let resp = make_response(request_seq, command, &seq, json!(null));
                    send_message(&stdout, &resp);
                    break;
                }

                _ => {
                    // Unknown command — send empty success response
                    let resp = make_response(request_seq, command, &seq, json!(null));
                    send_message(&stdout, &resp);
                }
            }

            // Drain output and paused events from the interpreter
            if let Some(ref session) = interpreter_handle {
                drain_output(&session.output_sink, &stdout, &seq);
                drain_paused_events(session, &stdout, &seq);
            }
        }
    }
}

struct InterpreterSession {
    debug_state: Arc<DebugState>,
    output_sink: Arc<Mutex<Vec<String>>>,
    current_line: Arc<Mutex<usize>>,
    paused_receiver: std::sync::mpsc::Receiver<usize>,
    source_name: String,
    source_path: String,
    _thread: std::thread::JoinHandle<()>,
}

fn launch_interpreter(
    program_path: &str,
    stop_on_entry: bool,
    stdout: Arc<Mutex<io::Stdout>>,
    seq: &Arc<Mutex<i64>>,
) -> Result<InterpreterSession, String> {
    let source = std::fs::read_to_string(program_path)
        .map_err(|e| format!("could not read '{}': {}", program_path, e))?;

    let mut lexer = Lexer::new(&source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("lexer error: {}", e))?;

    let mut parser = Parser::new(tokens);
    let program = parser
        .parse_program()
        .map_err(|e| format!("parse error: {}", e))?;

    let (paused_sender, paused_receiver) = std::sync::mpsc::channel::<usize>();

    let debug_state = Arc::new(DebugState {
        breakpoints: Mutex::new(HashSet::new()),
        action: Mutex::new(if stop_on_entry {
            DebugAction::Pause
        } else {
            DebugAction::Continue
        }),
        step_depth: Mutex::new(0),
        paused_sender,
        resume: (Mutex::new(false), std::sync::Condvar::new()),
        variables: Mutex::new(Vec::new()),
        call_frames: Mutex::new(Vec::new()),
        paused_depth: Mutex::new(0),
    });

    let output_sink: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let current_line: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

    let ds = debug_state.clone();
    let sink = output_sink.clone();
    let stdout_clone = stdout.clone();
    let seq_clone = seq.clone();

    let source_name = std::path::Path::new(program_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| program_path.to_string());
    let source_path = program_path.to_string();

    let thread = std::thread::spawn(move || {
        let mut interp = Interpreter::new();
        interp.debug_state = Some(ds);
        interp.output_sink = Some(sink.clone());
        interp.source = Some(source.clone());

        let result = interp.run(&program);

        // Send output events for any remaining output
        drain_output(&sink, &stdout_clone, &seq_clone);

        // Send terminated event
        match result {
            Ok(_) => {}
            Err(e) => {
                let event = make_event(
                    "output",
                    &seq_clone,
                    json!({
                        "category": "stderr",
                        "output": format!("Runtime error (line {}): {}\n", e.line, e.message),
                    }),
                );
                send_message(&stdout_clone, &event);
            }
        }

        let event = make_event("terminated", &seq_clone, json!({}));
        send_message(&stdout_clone, &event);
    });

    // If stop_on_entry, wait for the first pause
    if stop_on_entry {
        if let Ok(line_num) = paused_receiver.recv_timeout(std::time::Duration::from_secs(5)) {
            *current_line.lock().unwrap_or_else(|e| e.into_inner()) = line_num;

            let event = make_event(
                "stopped",
                seq,
                json!({
                    "reason": "entry",
                    "threadId": 1,
                    "allThreadsStopped": true,
                }),
            );
            send_message(&stdout, &event);
        }
    }

    Ok(InterpreterSession {
        debug_state,
        output_sink,
        current_line,
        paused_receiver,
        source_name,
        source_path,
        _thread: thread,
    })
}

fn resume_interpreter(debug_state: &Arc<DebugState>, action: DebugAction, depth: usize) {
    *debug_state.action.lock().unwrap_or_else(|e| e.into_inner()) = action;
    *debug_state
        .step_depth
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = depth;
    let (lock, cvar) = &debug_state.resume;
    let mut resumed = lock.lock().unwrap_or_else(|e| e.into_inner());
    *resumed = true;
    cvar.notify_all();
}

fn drain_output(
    sink: &Arc<Mutex<Vec<String>>>,
    stdout: &Arc<Mutex<io::Stdout>>,
    seq: &Arc<Mutex<i64>>,
) {
    let messages: Vec<String> = {
        let mut buf = sink.lock().unwrap_or_else(|e| e.into_inner());
        buf.drain(..).collect()
    };
    for msg in messages {
        let event = make_event(
            "output",
            seq,
            json!({
                "category": "stdout",
                "output": msg,
            }),
        );
        send_message(stdout, &event);
    }
}

fn drain_paused_events(
    session: &InterpreterSession,
    stdout: &Arc<Mutex<io::Stdout>>,
    seq: &Arc<Mutex<i64>>,
) {
    // Non-blocking check for pause events from the interpreter
    while let Ok(line) = session.paused_receiver.try_recv() {
        *session
            .current_line
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = line;

        // Snapshot variables when paused
        // (variables are snapped by the interpreter thread before pause — not accessible here
        //  since the interpreter is on another thread. We use the shared variables Arc instead.)

        let reason = {
            let action = *session
                .debug_state
                .action
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            match action {
                DebugAction::Pause => "pause",
                DebugAction::StepOver | DebugAction::StepIn | DebugAction::StepOut => "step",
                DebugAction::Continue => "breakpoint",
            }
        };

        let event = make_event(
            "stopped",
            seq,
            json!({
                "reason": reason,
                "threadId": 1,
                "allThreadsStopped": true,
            }),
        );
        send_message(stdout, &event);
    }
}

fn next_seq(seq: &Arc<Mutex<i64>>) -> i64 {
    let mut s = seq.lock().unwrap_or_else(|e| e.into_inner());
    let val = *s;
    *s += 1;
    val
}

fn make_response(
    request_seq: i64,
    command: &str,
    seq: &Arc<Mutex<i64>>,
    body: JsonValue,
) -> String {
    json!({
        "seq": next_seq(seq),
        "type": "response",
        "request_seq": request_seq,
        "success": true,
        "command": command,
        "body": body,
    })
    .to_string()
}

fn make_event(event: &str, seq: &Arc<Mutex<i64>>, body: JsonValue) -> String {
    json!({
        "seq": next_seq(seq),
        "type": "event",
        "event": event,
        "body": body,
    })
    .to_string()
}

fn send_message(stdout: &Arc<Mutex<io::Stdout>>, msg: &str) {
    if let Ok(mut out) = stdout.lock() {
        let bytes = msg.as_bytes();
        let _ = write!(out, "Content-Length: {}\r\n\r\n{}", bytes.len(), msg);
        let _ = out.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_response_includes_required_fields() {
        let seq = Arc::new(Mutex::new(1i64));
        let resp = make_response(5, "initialize", &seq, json!({"foo": "bar"}));
        let parsed: JsonValue = serde_json::from_str(&resp).unwrap();
        assert_eq!(parsed["type"], "response");
        assert_eq!(parsed["request_seq"], 5);
        assert_eq!(parsed["command"], "initialize");
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["body"]["foo"], "bar");
        assert_eq!(*seq.lock().unwrap(), 2);
    }

    #[test]
    fn make_event_includes_required_fields() {
        let seq = Arc::new(Mutex::new(1i64));
        let event = make_event("stopped", &seq, json!({"reason": "breakpoint"}));
        let parsed: JsonValue = serde_json::from_str(&event).unwrap();
        assert_eq!(parsed["type"], "event");
        assert_eq!(parsed["event"], "stopped");
        assert_eq!(parsed["body"]["reason"], "breakpoint");
        assert_eq!(*seq.lock().unwrap(), 2);
    }

    #[test]
    fn send_message_uses_content_length_framing() {
        let stdout = Arc::new(Mutex::new(io::stdout()));
        let msg = r#"{"seq":1,"type":"event"}"#;
        // Just verify it doesn't panic — actual output goes to stdout
        send_message(&stdout, msg);
    }

    #[test]
    fn snapshot_variables_filters_builtins() {
        let interp = Interpreter::new();
        let vars = interp.snapshot_user_variables();
        // Should not include module objects or builtins
        for (name, _) in &vars {
            assert!(!name.starts_with("__"));
        }
    }

    #[test]
    fn debug_state_breakpoint_matching() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let state = DebugState {
            breakpoints: Mutex::new(HashSet::from([5, 10, 15])),
            action: Mutex::new(DebugAction::Continue),
            step_depth: Mutex::new(0),
            paused_sender: tx,
            resume: (Mutex::new(false), std::sync::Condvar::new()),
            variables: Mutex::new(Vec::new()),
            call_frames: Mutex::new(Vec::new()),
            paused_depth: Mutex::new(0),
        };

        let bps = state.breakpoints.lock().unwrap();
        assert!(bps.contains(&5));
        assert!(bps.contains(&10));
        assert!(!bps.contains(&7));
    }

    #[test]
    fn resume_interpreter_sets_action_and_signals() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let state = Arc::new(DebugState {
            breakpoints: Mutex::new(HashSet::new()),
            action: Mutex::new(DebugAction::Pause),
            step_depth: Mutex::new(0),
            paused_sender: tx,
            resume: (Mutex::new(false), std::sync::Condvar::new()),
            variables: Mutex::new(Vec::new()),
            call_frames: Mutex::new(Vec::new()),
            paused_depth: Mutex::new(0),
        });

        resume_interpreter(&state, DebugAction::StepOver, 3);

        assert_eq!(*state.action.lock().unwrap(), DebugAction::StepOver);
        assert_eq!(*state.step_depth.lock().unwrap(), 3);
        assert!(*state.resume.0.lock().unwrap());
    }
}
