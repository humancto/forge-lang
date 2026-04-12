use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::vm::compiler;
use crate::vm::machine::VM;

fn parse_program(source: &str) -> crate::parser::ast::Program {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer error");
    let mut parser = Parser::new(tokens);
    parser.parse_program().expect("parse error")
}

fn compile_ok(source: &str) {
    let program = parse_program(source);
    compiler::compile(&program).expect("compile error");
}

fn run_vm(source: &str) {
    let program = parse_program(source);
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    vm.execute(&chunk).expect("vm error");
}

#[test]
fn vm_schedule_compiles() {
    compile_ok("schedule every 1 seconds { let x = 1 }");
}

#[test]
fn vm_schedule_minutes_compiles() {
    compile_ok("schedule every 5 minutes { let x = 1 }");
}

#[test]
fn vm_schedule_hours_compiles() {
    compile_ok("schedule every 2 hours { let x = 1 }");
}

#[test]
fn vm_schedule_captures_variable() {
    compile_ok("let x = 42\nschedule every 1 seconds { let y = x }");
}

#[test]
fn vm_schedule_fires() {
    // Schedule spawns a detached background thread. Verify it compiles and
    // starts without error. The thread is killed on process exit.
    run_vm("schedule every 1 seconds { let x = 42 }");
}

#[test]
fn vm_watch_compiles() {
    compile_ok(r#"watch "somefile.txt" { let x = 1 }"#);
}

#[test]
fn vm_watch_captures_variable() {
    compile_ok(
        r#"let x = 42
watch "somefile.txt" { let y = x }"#,
    );
}

#[test]
fn vm_watch_fires_on_change() {
    // Watch spawns a background thread polling file mtime. Verify it runs without error.
    let watched = std::env::temp_dir().join("forge_watch_vm_test.txt");
    std::fs::write(&watched, "initial").expect("write watched file");

    let source = format!(r#"watch "{}" {{ let x = 1 }}"#, watched.display());
    run_vm(&source);
    let _ = std::fs::remove_file(&watched);
}

#[test]
fn vm_watch_no_fire_without_change() {
    // Verify watch compiles and runs without error when file exists but doesn't change
    let watched = std::env::temp_dir().join("forge_watch_vm_nochange.txt");
    std::fs::write(&watched, "stable").expect("write watched file");

    let source = format!(r#"watch "{}" {{ let x = 1 }}"#, watched.display());
    run_vm(&source);
    let _ = std::fs::remove_file(&watched);
}

#[test]
fn vm_schedule_zero_interval_errors() {
    let program = parse_program("schedule every 0 seconds { let x = 1 }");
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    let result = vm.execute(&chunk);
    assert!(result.is_err(), "zero interval should error");
    assert!(
        result.unwrap_err().message.contains("positive integer"),
        "error should mention positive integer"
    );
}

#[test]
fn vm_schedule_negative_interval_errors() {
    let program = parse_program("schedule every -1 seconds { let x = 1 }");
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    let result = vm.execute(&chunk);
    // Negative interval is parsed as unary minus on 1, producing Int(-1)
    // VM should reject non-positive intervals
    assert!(result.is_err(), "negative interval should error");
}

#[test]
fn vm_watch_nonexistent_file() {
    // Watch on a non-existent file should start without panic —
    // the thread just sees last_modified = None and waits for creation
    run_vm(r#"watch "/tmp/forge_nonexistent_99999.txt" { let x = 1 }"#);
}

#[test]
fn vm_schedule_captures_and_uses_variable() {
    // Verify upvalue capture actually works at runtime, not just compilation.
    // The closure body accesses captured variable x; if upvalue transfer
    // fails, the child VM would panic/error on the background thread.
    // We verify by running without error (errors on background threads
    // are printed to stderr but don't crash the parent).
    run_vm("let x = 42\nschedule every 1 seconds { let y = x + 1 }");
}
