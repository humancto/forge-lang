// Forge language library — exposes the runtime for AOT-compiled binaries.
//
// AOT binaries link against libforge.a and call forge_execute_bytecode()
// to run embedded bytecode without needing the `forge` CLI.

mod errors;
mod interpreter;
mod lexer;
mod manifest;
mod package;
mod parser;
mod permissions;
mod registry;
mod runtime;
mod stdlib;
mod typechecker;
pub mod vm;

use std::panic;

/// Execute serialized bytecode. Returns 0 on success, 1 on error.
///
/// # Safety
/// `bytecode_ptr` must point to `bytecode_len` valid bytes of serialized
/// Forge bytecode (produced by `vm::serialize::serialize_chunk`).
#[no_mangle]
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
