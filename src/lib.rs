// Forge language library — exposes the runtime for AOT-compiled binaries
// and the embedding API.
//
// AOT binaries link against libforge.a and call forge_execute_bytecode()
// to run embedded bytecode without needing the `forge` CLI.
//
// Hosts that embed Forge to run untrusted scripts (AI agents, automation)
// use `forge_lang::Sandbox`: default-deny capabilities, explicit grants,
// a wall-clock limit, deterministic fuel/memory/handle limits and captured
// output. See `src/sandbox.rs` and `src/runtime/limits.rs`.
//
// Without the default `host` feature (`--no-default-features`, e.g. the
// browser playground in bindings/wasm) only the portable core is built:
// lexer, parser, type checker, formatter (`tooling`), interpreter, VM and
// the pure stdlib. Host-only helpers the core still links are then unused.

#![cfg_attr(not(feature = "host"), allow(dead_code))]

mod builtins_registry;
pub mod clock;
mod color;
mod errors;
// The binary's `forge fmt` driver; the library uses `format_source`
// (through `tooling`).
#[allow(dead_code)]
mod formatter;
pub mod interpreter;
pub mod lexer;
// `manifest`, `package` and `registry` are the CLI's package manager; the
// library uses only import resolution from them (`package::resolve_import*`),
// so the rest is dead code from the library's point of view (the binary
// still reports genuinely unused items).
#[allow(dead_code)]
mod manifest;
#[cfg(feature = "host")]
pub mod mcp;
#[allow(dead_code)]
mod package;
pub mod parser;
pub mod permissions;
mod plugins;
#[cfg(feature = "host")]
#[allow(dead_code)]
mod registry;
pub mod runtime;
#[cfg(feature = "host")]
mod sandbox;
mod semantics;
mod stdlib;
// The binary uses the whole checker (CLI, LSP, `--strict`); the library only
// `analyze` (for `forge_lang::mcp`).
pub mod tooling;
#[allow(dead_code)]
mod typechecker;
pub mod vm;

pub use permissions::{Capabilities, Capability, PermissionError};
pub use runtime::limits::{CountingAllocator, Limits};
#[cfg(feature = "host")]
pub use sandbox::{CancelHandle, Engine, Output, Sandbox, SandboxError};
pub use semantics::edition::Edition;

// The library's own tests exercise `Sandbox::max_memory`, which measures
// the interpreter through the counting allocator.
#[cfg(test)]
#[global_allocator]
static TEST_ALLOCATOR: CountingAllocator = CountingAllocator;

use std::panic;

/// Execute serialized bytecode. Returns 0 on success, 1 on error.
///
/// # Safety
/// `bytecode_ptr` must point to `bytecode_len` valid bytes of serialized
/// Forge bytecode (produced by `vm::serialize::serialize_chunk`).
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn forge_execute_bytecode(bytecode_ptr: *const u8, bytecode_len: usize) -> i32 {
    if bytecode_ptr.is_null() || bytecode_len == 0 {
        eprintln!("forge: null or empty bytecode");
        return 1;
    }

    let bytecode = unsafe { std::slice::from_raw_parts(bytecode_ptr, bytecode_len) };

    // Catch panics so we don't abort the process
    let result = panic::catch_unwind(|| {
        let chunk = match vm::serialize::deserialize_chunk(bytecode) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("forge: bytecode deserialization failed: {}", e.message);
                return 1;
            }
        };

        let mut machine = vm::machine::VM::new();
        match machine.execute(&chunk) {
            Ok(_) => 0,
            Err(e) => {
                eprintln!("forge: runtime error: {}", e);
                1
            }
        }
    });

    match result {
        Ok(code) => code,
        Err(_) => {
            eprintln!("forge: internal panic during execution");
            1
        }
    }
}

/// Execute embedded Forge source. Returns 0 on success, 1 on error.
///
/// This is the source-runtime standalone entrypoint used by generated native
/// wrappers for programs that need interpreter-only features such as
/// decorator-driven HTTP servers.
///
/// # Safety
/// `source_ptr` must point to `source_len` valid bytes of UTF-8 Forge source
/// for the duration of this call. When `path_len > 0`, `path_ptr` must point to
/// `path_len` valid bytes of UTF-8 diagnostic label data.
#[cfg(feature = "host")]
#[no_mangle]
pub unsafe extern "C" fn forge_execute_source(
    source_ptr: *const u8,
    source_len: usize,
    path_ptr: *const u8,
    path_len: usize,
    allow_run: i32,
) -> i32 {
    if source_ptr.is_null() || source_len == 0 {
        eprintln!("forge: null or empty source");
        return 1;
    }
    if path_ptr.is_null() && path_len > 0 {
        eprintln!("forge: null source path with nonzero length");
        return 1;
    }

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let source_bytes = unsafe { std::slice::from_raw_parts(source_ptr, source_len) };
        let source = match std::str::from_utf8(source_bytes) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("forge: source is not valid UTF-8: {err}");
                return 1;
            }
        };

        let source_label = if path_len == 0 {
            "<embedded>".to_string()
        } else {
            let path_bytes = unsafe { std::slice::from_raw_parts(path_ptr, path_len) };
            match std::str::from_utf8(path_bytes) {
                Ok(path) => path.to_string(),
                Err(err) => {
                    eprintln!("forge: source path is not valid UTF-8: {err}");
                    return 1;
                }
            }
        };

        let config = runtime::embedded::EmbeddedSourceConfig::new(source_label, allow_run != 0);
        match runtime::embedded::execute_source_standalone(source, config) {
            Ok(()) => 0,
            Err(err) => {
                eprintln!("{err}");
                1
            }
        }
    }));

    match result {
        Ok(code) => code,
        Err(_) => {
            eprintln!("forge: internal panic during source execution");
            1
        }
    }
}

/// C-ABI entry points, exercised the way a native launcher calls them. Named
/// `miri_*` so the Miri CI job (`cargo +nightly miri test --lib -- miri_`)
/// checks the raw-pointer handling for undefined behaviour.
#[cfg(all(test, feature = "host"))]
mod ffi_miri_tests;
