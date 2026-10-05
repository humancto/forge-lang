//! Boot a Forge `@server` program in-process on either execution engine.
//!
//! Shared by `tests/server_concurrency.rs` and
//! `tests/server_engine_parity.rs`. Each server runs on its own thread
//! with its own multi-threaded tokio runtime and stays up until the test
//! process exits.

#![allow(dead_code)] // each test binary uses a different subset

use std::net::TcpListener;
use std::time::Duration;

use forge_lang::interpreter::Interpreter;
use forge_lang::lexer::Lexer;
use forge_lang::parser::Parser;
use forge_lang::runtime::metadata::{extract_runtime_plan, top_level_fn_params};

/// The engine that runs the handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    /// The bytecode VM (`forge run`, the default).
    Vm,
    /// The tree-walking interpreter (`forge run --interp`).
    Interpreter,
}

/// Pick an unused TCP port by binding 0 and letting the kernel choose.
pub fn pick_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    port
}

/// Boot `source` (with `__PORT__` replaced) on `engine` and wait until
/// `GET /ping` succeeds. Returns the port.
pub fn spawn_server(source: &str, engine: Engine) -> u16 {
    spawn_server_on(source, engine, |builder| builder)
}

/// [`spawn_server`] with a hook to configure the runtime (e.g. with
/// `forge_lang::runtime::recursion::configure_runtime`, as the CLI does).
pub fn spawn_server_on(
    source: &str,
    engine: Engine,
    configure: fn(&mut tokio::runtime::Builder) -> &mut tokio::runtime::Builder,
) -> u16 {
    let port = pick_port();
    let src = source.replace("__PORT__", &port.to_string());

    std::thread::spawn(move || {
        let mut builder = tokio::runtime::Builder::new_multi_thread();
        let rt = configure(&mut builder)
            .worker_threads(2)
            .max_blocking_threads(64)
            .enable_all()
            .build()
            .expect("build tokio runtime");
        rt.block_on(async move {
            let tokens = Lexer::new(&src).tokenize().expect("lex");
            let program = Parser::new(tokens).parse_program().expect("parse");
            let plan = extract_runtime_plan(&program);
            assert!(plan.server.is_some(), "program has no @server decorator");

            match engine {
                Engine::Interpreter => {
                    // Schedules start during `run` here (the interpreter's
                    // deferred mode is CLI-internal); the serving-fork
                    // isolation contract is the same either way.
                    let mut interp = Interpreter::new();
                    interp.run(&program).expect("run");
                    let server = plan.server.as_ref().expect("server plan");
                    forge_lang::runtime::server::start_server(interp, server)
                        .await
                        .expect("server start");
                }
                Engine::Vm => {
                    // Exactly what `forge run` does for an @server program.
                    let chunk = forge_lang::vm::compiler::compile(&program).expect("compile");
                    let mut vm = forge_lang::vm::machine::VM::new();
                    vm.defer_host_runtime();
                    vm.execute(&chunk).expect("run");
                    forge_lang::runtime::host::launch_vm(vm, &plan, top_level_fn_params(&program))
                        .await
                        .expect("server start");
                }
            }
        });
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .expect("client");
    let url = format!("http://127.0.0.1:{}/ping", port);
    for _ in 0..100 {
        if client
            .get(&url)
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return port;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!(
        "{:?} server failed to start on port {} within 10s",
        engine, port
    );
}
