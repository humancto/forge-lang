// Forge language library — exposes the runtime for AOT-compiled binaries.
//
// AOT binaries link against libforge.a and call forge_execute_bytecode()
// to run embedded bytecode without needing the `forge` CLI.

mod errors;
pub mod interpreter;
pub mod lexer;
mod manifest;
mod package;
pub mod parser;
mod permissions;
mod registry;
pub mod runtime;
mod stdlib;
mod typechecker;
pub mod vm;

use std::panic::{self, AssertUnwindSafe};

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

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
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
