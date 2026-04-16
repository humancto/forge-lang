mod builtins; // VM builtin dispatch — extracted from machine.rs
pub mod bytecode;
pub mod compiler;
pub mod frame;
pub mod gc;
pub mod green;
#[cfg(feature = "jit")]
pub mod jit;
pub mod machine;
pub mod nanbox;
pub mod profiler;
pub mod serialize;
pub mod value;

use crate::parser::ast::Program;
use machine::{VMError, VM};

/// Compile and execute a Forge program using the bytecode VM.
pub fn run(program: &Program) -> Result<(), VMError> {
    let chunk = compiler::compile(program).map_err(|e| VMError::new(&e.message))?;
    let mut vm = VM::new();
    vm.execute(&chunk)?;
    Ok(())
}

/// Compile and execute with profiling enabled. Prints a report after execution.
pub fn run_with_profiling(program: &Program) -> Result<(), VMError> {
    let chunk = compiler::compile(program).map_err(|e| VMError::new(&e.message))?;
    let mut vm = VM::with_profiling();
    vm.execute(&chunk)?;
    vm.profiler.print_report();
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
mod schedule_watch_tests;
#[cfg(test)]
mod set_tests;
#[cfg(test)]
mod stream_tests;
#[cfg(test)]
mod tuple_tests;
