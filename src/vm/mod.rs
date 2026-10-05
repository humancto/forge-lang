mod builtins; // VM builtin dispatch — extracted from machine.rs
pub mod bytecode;
pub mod compiler;
pub mod frame;
pub mod gc;
pub mod green;
#[cfg(feature = "jit")]
pub mod jit;
mod local_ops; // local-variable access and in-place update opcodes
pub mod machine;
pub mod nanbox;
pub mod profiler;
mod range_loop; // counting `for v in range(..)` / `repeat n times` loops
pub mod serialize;
pub mod serve;
pub mod value;

use crate::parser::ast::Program;
use machine::{VMError, VM};

/// Execute an already-compiled program, optionally printing a profile.
pub fn run_chunk(chunk: &bytecode::Chunk, profile: bool) -> Result<(), VMError> {
    let mut vm = if profile {
        VM::with_profiling()
    } else {
        VM::new()
    };
    vm.execute(chunk)?;
    if profile {
        vm.profiler.print_report();
    }
    Ok(())
}

/// Compile and execute in REPL mode (returns the last value).
#[allow(dead_code)]
pub fn run_repl(vm: &mut VM, program: &Program) -> Result<value::Value, VMError> {
    let chunk = compiler::compile_repl(program).map_err(|e| VMError::new(&e.message))?;
    vm.execute(&chunk)
}

#[cfg(test)]
mod async_tests;
#[cfg(test)]
mod enum_methods_tests;
#[cfg(all(test, feature = "jit"))]
mod jit_tests;
#[cfg(test)]
mod map_tests;
#[cfg(test)]
mod must_ask_freeze_tests;
#[cfg(test)]
mod parity_tests;
#[cfg(test)]
mod perf_tests;
#[cfg(test)]
mod runtime_safety_tests;
#[cfg(test)]
mod schedule_watch_tests;
#[cfg(test)]
mod serve_tests;
#[cfg(test)]
mod set_tests;
#[cfg(test)]
mod squad_tests;
#[cfg(test)]
mod stream_tests;
#[cfg(test)]
mod tuple_tests;
