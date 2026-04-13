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

fn run_on_vm(source: &str) -> Vec<String> {
    let program = parse_program(source);
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    vm.execute(&chunk).expect("vm error");
    vm.output.clone()
}

fn run_on_vm_value(source: &str) -> String {
    let program = parse_program(source);
    let chunk = compiler::compile_repl(&program).expect("compile error");
    let mut vm = VM::new();
    let value = vm.execute(&chunk).expect("vm error");
    value.display(&vm.gc)
}

#[test]
fn vm_spawn_returns_task_handle() {
    let out = run_on_vm_value("typeof(spawn { 42 })");
    assert_eq!(out, "TaskHandle");
}

#[test]
fn vm_spawn_display() {
    let out = run_on_vm_value("let h = spawn { 1 }\nh");
    assert_eq!(out, "<task>");
}

#[test]
fn vm_await_spawn_gets_value() {
    let out = run_on_vm_value("await spawn { return 42 }");
    assert_eq!(out, "42");
}

#[test]
fn vm_await_non_task_passthrough() {
    let out = run_on_vm_value("await 99");
    assert_eq!(out, "99");
}

#[test]
fn vm_spawn_stmt_fire_and_forget() {
    // Stmt::Spawn discards the handle — just verify it doesn't crash
    run_on_vm(
        r#"
        spawn { let x = 1 + 2 }
        wait 0.1 seconds
    "#,
    );
}

#[test]
fn vm_multiple_spawns_await() {
    let out = run_on_vm(
        r#"
        let a = spawn { return 10 }
        let b = spawn { return 20 }
        let c = spawn { return 30 }
        println(await a + await b + await c)
    "#,
    );
    assert_eq!(out, vec!["60"]);
}

#[test]
fn vm_spawn_with_computation() {
    let out = run_on_vm_value(
        r#"
        let h = spawn {
            let mut sum = 0
            let mut i = 0
            while i < 100 {
                sum = sum + i
                i = i + 1
            }
            return sum
        }
        await h
    "#,
    );
    assert_eq!(out, "4950");
}

#[test]
fn vm_await_string_result() {
    let out = run_on_vm_value(
        r#"
        let h = spawn { return "hello from spawn" }
        await h
    "#,
    );
    assert_eq!(out, "hello from spawn");
}

#[test]
fn vm_spawn_captures_variable() {
    let out = run_on_vm_value(
        r#"
        let x = 42
        let h = spawn { return x }
        await h
    "#,
    );
    assert_eq!(out, "42");
}

#[test]
fn vm_nested_spawn() {
    let out = run_on_vm_value(
        r#"
        let h = spawn {
            let inner = spawn { return 7 }
            return await inner
        }
        await h
    "#,
    );
    assert_eq!(out, "7");
}

#[test]
fn vm_spawn_error_no_crash() {
    // Spawn block that errors — parent awaits and gets null
    let out = run_on_vm_value(
        r#"
        let h = spawn {
            let x = null
            return x.field
        }
        await h
    "#,
    );
    assert_eq!(out, "null");
}

#[test]
fn vm_spawn_returns_object() {
    let out = run_on_vm(
        r#"
        let h = spawn { return { a: 1, b: "hi" } }
        let result = await h
        println(result.a)
        println(result.b)
    "#,
    );
    assert_eq!(out, vec!["1", "hi"]);
}

#[test]
fn vm_double_await_returns_same_value() {
    let out = run_on_vm(
        r#"
        let h = spawn { return 99 }
        let first = await h
        let second = await h
        println(first)
        println(second)
    "#,
    );
    assert_eq!(out, vec!["99", "99"]);
}

// ----- Channel builtins -----

#[test]
fn vm_channel_create_bounded() {
    let out = run_on_vm(
        r#"
        let ch = channel(1)
        println(type(ch))
    "#,
    );
    assert_eq!(out, vec!["channel"]);
}

#[test]
fn vm_channel_create_unbounded() {
    let out = run_on_vm(
        r#"
        let ch = channel()
        println(type(ch))
    "#,
    );
    assert_eq!(out, vec!["channel"]);
}

#[test]
fn vm_channel_send_receive() {
    let out = run_on_vm(
        r#"
        let ch = channel(1)
        send(ch, 42)
        let val = receive(ch)
        println(val)
    "#,
    );
    assert_eq!(out, vec!["42"]);
}

#[test]
fn vm_channel_send_receive_string() {
    let out = run_on_vm(
        r#"
        let ch = channel(1)
        send(ch, "hello")
        let val = receive(ch)
        println(val)
    "#,
    );
    assert_eq!(out, vec!["hello"]);
}

#[test]
fn vm_channel_close() {
    let out = run_on_vm(
        r#"
        let ch = channel(2)
        send(ch, 1)
        send(ch, 2)
        close(ch)
        let a = receive(ch)
        let b = receive(ch)
        let c = receive(ch)
        println(a)
        println(b)
        println(c)
    "#,
    );
    // After close, buffered values are still receivable; then null
    assert_eq!(out, vec!["1", "2", "null"]);
}

#[test]
fn vm_channel_cross_spawn() {
    let out = run_on_vm(
        r#"
        let ch = channel(1)
        spawn {
            send(ch, 99)
        }
        let val = receive(ch)
        println(val)
    "#,
    );
    assert_eq!(out, vec!["99"]);
}
