use crate::interpreter::{DebugAction, DebugState, Interpreter};
use crate::lexer::Lexer;
use crate::parser::Parser;
use serde_json::{json, Value as JsonValue};
use std::collections::HashSet;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

mod transport;

/// A `launch` request whose program has not started yet. The program starts
/// on `configurationDone`, so breakpoints sent during configuration are in
/// place before the first statement runs.
struct PendingLaunch {
    program: String,
    stop_on_entry: bool,
}

/// Run the DAP server over stdin/stdout.
pub fn run_dap() {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let stdout = Arc::new(Mutex::new(io::stdout()));

    eprintln!("Forge DAP server started");

    let seq = Arc::new(Mutex::new(1i64));
    let mut pending_breakpoints: std::collections::HashMap<String, HashSet<usize>> =
        std::collections::HashMap::new();
    let mut interpreter_handle: Option<InterpreterSession> = None;
    let mut pending_launch: Option<PendingLaunch> = None;
    let mut configuration_done = false;

    loop {
        let content = match transport::read_message(&mut reader) {
            Ok(Some(body)) => body,
            Ok(None) => break,
            Err(e) => {
                eprintln!("forge dap: {}", e);
                break;
            }
        };
        {
            let msg: JsonValue = match serde_json::from_slice(&content) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("forge dap: ignoring malformed message: {}", e);
                    continue;
                }
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

                    let launch = PendingLaunch {
                        program,
                        stop_on_entry,
                    };
                    if configuration_done {
                        interpreter_handle =
                            start_program(launch, &mut pending_breakpoints, &stdout, &seq);
                    } else {
                        pending_launch = Some(launch);
                    }
                }

                "configurationDone" => {
                    configuration_done = true;
                    let resp = make_response(request_seq, command, &seq, json!(null));
                    send_message(&stdout, &resp);
                    if let Some(launch) = pending_launch.take() {
                        interpreter_handle =
                            start_program(launch, &mut pending_breakpoints, &stdout, &seq);
                    }
                }

                "setBreakpoints" => {
                    let source_path = args["source"]["path"].as_str().unwrap_or("").to_string();
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
                        let entry = bps.entry(source_path.clone()).or_default();
                        entry.clear();
                        for &l in &lines {
                            entry.insert(l);
                        }
                    } else {
                        let entry = pending_breakpoints.entry(source_path.clone()).or_default();
                        entry.clear();
                        for &l in &lines {
                            entry.insert(l);
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
        }
    }
}

/// Start a launched program and apply breakpoints collected before launch.
/// On failure, reports the error to the client and ends the session.
fn start_program(
    launch: PendingLaunch,
    pending_breakpoints: &mut std::collections::HashMap<String, HashSet<usize>>,
    stdout: &Arc<Mutex<io::Stdout>>,
    seq: &Arc<Mutex<i64>>,
) -> Option<InterpreterSession> {
    let initial_breakpoints = std::mem::take(pending_breakpoints);
    match launch_interpreter(
        &launch.program,
        launch.stop_on_entry,
        initial_breakpoints,
        stdout.clone(),
        seq,
    ) {
        Ok(session) => Some(session),
        Err(e) => {
            let event = make_event(
                "output",
                seq,
                json!({
                    "category": "stderr",
                    "output": format!("Launch failed: {}\n", e),
                }),
            );
            send_message(stdout, &event);
            let event = make_event("terminated", seq, json!({}));
            send_message(stdout, &event);
            None
        }
    }
}

struct InterpreterSession {
    debug_state: Arc<DebugState>,
    current_line: Arc<Mutex<usize>>,
    source_name: String,
    source_path: String,
    _thread: std::thread::JoinHandle<()>,
    _forwarder: std::thread::JoinHandle<()>,
}

fn launch_interpreter(
    program_path: &str,
    stop_on_entry: bool,
    initial_breakpoints: std::collections::HashMap<String, HashSet<usize>>,
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
        breakpoints: Mutex::new(initial_breakpoints),
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
    // Set by the interpreter thread when the program ends: `Some(error)`.
    let finished: Arc<Mutex<Option<Option<String>>>> = Arc::new(Mutex::new(None));

    let source_name = std::path::Path::new(program_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| program_path.to_string());
    let source_path = program_path.to_string();

    let thread = {
        let ds = debug_state.clone();
        let sink = output_sink.clone();
        let finished = finished.clone();
        std::thread::spawn(move || {
            let mut interp = Interpreter::new();
            interp.debug_state = Some(ds);
            interp.output_sink = Some(sink);
            interp.source = Some(source);

            let error = interp
                .run(&program)
                .err()
                .map(|e| format!("Runtime error (line {}): {}\n", e.line, e.message));
            *finished.lock().unwrap_or_else(|e| e.into_inner()) = Some(error);
        })
    };

    // The forwarder is the only thread that emits program events, so
    // output, `stopped`, `exited` and `terminated` reach the client promptly
    // (without waiting for the next client request) and in order.
    let forwarder = {
        let forwarder = EventForwarder {
            paused_receiver,
            debug_state: debug_state.clone(),
            output_sink,
            current_line: current_line.clone(),
            finished,
            stdout,
            seq: seq.clone(),
            first_stop_is_entry: AtomicBool::new(stop_on_entry),
        };
        std::thread::spawn(move || forwarder.run())
    };

    Ok(InterpreterSession {
        debug_state,
        current_line,
        source_name,
        source_path,
        _thread: thread,
        _forwarder: forwarder,
    })
}

struct EventForwarder {
    paused_receiver: std::sync::mpsc::Receiver<usize>,
    debug_state: Arc<DebugState>,
    output_sink: Arc<Mutex<Vec<String>>>,
    current_line: Arc<Mutex<usize>>,
    finished: Arc<Mutex<Option<Option<String>>>>,
    stdout: Arc<Mutex<io::Stdout>>,
    seq: Arc<Mutex<i64>>,
    first_stop_is_entry: AtomicBool,
}

impl EventForwarder {
    fn run(self) {
        loop {
            match self.paused_receiver.recv_timeout(Duration::from_millis(25)) {
                Ok(line) => {
                    drain_output(&self.output_sink, &self.stdout, &self.seq);
                    self.send_stopped(line);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    drain_output(&self.output_sink, &self.stdout, &self.seq);
                    let done = self
                        .finished
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .take();
                    if let Some(error) = done {
                        self.send_end(error);
                        return;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn send_stopped(&self, line: usize) {
        *self.current_line.lock().unwrap_or_else(|e| e.into_inner()) = line;
        let reason = if self.first_stop_is_entry.swap(false, Ordering::SeqCst) {
            "entry"
        } else {
            let action = *self
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
            &self.seq,
            json!({
                "reason": reason,
                "threadId": 1,
                "allThreadsStopped": true,
            }),
        );
        send_message(&self.stdout, &event);
    }

    fn send_end(&self, error: Option<String>) {
        drain_output(&self.output_sink, &self.stdout, &self.seq);
        if let Some(ref message) = error {
            let event = make_event(
                "output",
                &self.seq,
                json!({ "category": "stderr", "output": message }),
            );
            send_message(&self.stdout, &event);
        }
        let exit_code = if error.is_some() { 1 } else { 0 };
        let event = make_event("exited", &self.seq, json!({ "exitCode": exit_code }));
        send_message(&self.stdout, &event);
        let event = make_event("terminated", &self.seq, json!({}));
        send_message(&self.stdout, &event);
    }
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
    // Recover from poisoning: a panicked writer must not silence the adapter.
    let mut out = stdout.lock().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = transport::write_message(&mut *out, msg) {
        eprintln!("forge dap: failed to write message: {}", e);
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
            breakpoints: Mutex::new({
                let mut m = std::collections::HashMap::new();
                m.insert("test.fg".to_string(), HashSet::from([5, 10, 15]));
                m
            }),
            action: Mutex::new(DebugAction::Continue),
            step_depth: Mutex::new(0),
            paused_sender: tx,
            resume: (Mutex::new(false), std::sync::Condvar::new()),
            variables: Mutex::new(Vec::new()),
            call_frames: Mutex::new(Vec::new()),
            paused_depth: Mutex::new(0),
        };

        let bps = state.breakpoints.lock().unwrap();
        let file_bps = bps.get("test.fg").unwrap();
        assert!(file_bps.contains(&5));
        assert!(file_bps.contains(&10));
        assert!(!file_bps.contains(&7));
    }

    #[test]
    fn resume_interpreter_sets_action_and_signals() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let state = Arc::new(DebugState {
            breakpoints: Mutex::new(std::collections::HashMap::new()),
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
