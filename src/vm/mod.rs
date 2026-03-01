pub mod bytecode;
pub mod compiler;
pub mod frame;
pub mod gc;
pub mod green;
pub mod jit;
pub mod machine;
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
    let chunk = compiler::compile(program).map_err(|e| VMError::new(&e.message))?;
    vm.execute(&chunk)
}
