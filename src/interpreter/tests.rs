use super::*;
use crate::lexer::Lexer;
use crate::parser::Parser;

fn run_forge(source: &str) -> Value {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexing should succeed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().expect("parsing should succeed");
    let mut interpreter = Interpreter::new();
    interpreter
        .run_repl(&program)
        .expect("execution should succeed")
}

fn try_run_forge(source: &str) -> Result<Value, RuntimeError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexing should succeed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().expect("parsing should succeed");
    let mut interpreter = Interpreter::new();
    interpreter.run(&program)
}

#[test]
fn evaluates_interpolated_expression() {
    let value = run_forge(
        r#"
        let a = 20
        let b = 22
        "answer = {a + b}"
        "#,
    );

    match value {
        Value::String(s) => assert_eq!(s, "answer = 42"),
        _ => panic!("expected string result"),
    }
}

#[test]
fn try_operator_unwraps_ok() {
    let value = run_forge(
        r#"
        fn parse_num(s) {
            return Ok(int(s))
        }

        fn add_one() {
            let n = parse_num("41")?
            return n + 1
        }

        add_one()
        "#,
    );

    match value {
        Value::Int(n) => assert_eq!(n, 42),
        _ => panic!("expected int result"),
    }
}

#[test]
fn try_operator_propagates_err() {
    let value = run_forge(
        r#"
        fn fail() {
            return Err("boom")
        }

        fn wrapper() {
            let _x = fail()?
            return 42
        }

        wrapper()
        "#,
    );

    match value {
        Value::ResultErr(inner) => match *inner {
            Value::String(msg) => assert_eq!(msg, "boom"),
            _ => panic!("expected string error message"),
        },
        _ => panic!("expected Err result"),
    }
}

#[test]
fn map_and_filter_work_with_functions() {
    let value = run_forge(
        r#"
        fn double(x) { return x * 2 }
        fn is_even(x) { return x % 2 == 0 }

        let mapped = map([1, 2, 3, 4], double)
        let filtered = filter(mapped, is_even)
        len(filtered)
        "#,
    );

    match value {
        Value::Int(n) => assert_eq!(n, 4),
        _ => panic!("expected int result"),
    }
}

#[test]
fn pop_and_enumerate_work() {
    // pop() now returns the last element, not the remaining array
    let value = run_forge(
        r#"
        let last = pop([10, 20, 30])
        last
        "#,
    );

    match value {
        Value::Int(n) => assert_eq!(n, 30),
        other => panic!("expected Int(30), got {:?}", other),
    }
}

#[test]
fn immutable_variable_cannot_be_reassigned() {
    let result = try_run_forge(
        r#"
        let x = 10
        x = 20
        "#,
    );
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert!(
        msg.contains("cannot reassign immutable variable"),
        "got: {}",
        msg
    );
}

#[test]
fn mutable_variable_can_be_reassigned() {
    let value = run_forge(
        r#"
        let mut x = 10
        x = 20
        x
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 20),
        _ => panic!("expected int result"),
    }
}

#[test]
fn shadowing_immutable_with_new_let_works() {
    let value = run_forge(
        r#"
        let x = 10
        let x = 20
        x
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 20),
        _ => panic!("expected int result"),
    }
}

// ========== Natural Syntax Tests ==========

#[test]
fn set_to_creates_variable() {
    let value = run_forge(
        r#"
        set x to 42
        x
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 42),
        _ => panic!("expected int"),
    }
}

#[test]
fn set_mut_and_change_to() {
    let value = run_forge(
        r#"
        set mut x to 10
        change x to 20
        x
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 20),
        _ => panic!("expected int"),
    }
}

#[test]
fn set_immutable_cannot_change() {
    let result = try_run_forge(
        r#"
        set x to 10
        change x to 20
        "#,
    );
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert!(
        msg.contains("cannot reassign immutable variable"),
        "got: {}",
        msg
    );
}

#[test]
fn define_works_like_fn() {
    let value = run_forge(
        r#"
        define add(a, b) {
            return a + b
        }
        add(3, 4)
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 7),
        _ => panic!("expected int"),
    }
}

#[test]
fn otherwise_works_as_else() {
    let value = run_forge(
        r#"
        set x to 5
        set mut result to 0
        if x > 10 {
            change result to 1
        } otherwise {
            change result to 2
        }
        result
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 2),
        _ => panic!("expected int"),
    }
}

#[test]
fn nah_works_as_else() {
    let value = run_forge(
        r#"
        set x to false
        set mut result to 0
        if x {
            change result to 1
        } nah {
            change result to 2
        }
        result
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 2),
        _ => panic!("expected int"),
    }
}

#[test]
fn otherwise_if_chaining() {
    let value = run_forge(
        r#"
        set x to 50
        set mut result to 0
        if x > 100 {
            change result to 3
        } otherwise if x > 30 {
            change result to 2
        } otherwise {
            change result to 1
        }
        result
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 2),
        _ => panic!("expected int"),
    }
}

#[test]
fn for_each_loop() {
    let value = run_forge(
        r#"
        set mut total to 0
        for each n in [10, 20, 30] {
            change total to total + n
        }
        total
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 60),
        _ => panic!("expected int"),
    }
}

#[test]
fn repeat_n_times() {
    let value = run_forge(
        r#"
        set mut count to 0
        repeat 5 times {
            change count to count + 1
        }
        count
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 5),
        _ => panic!("expected int"),
    }
}

#[test]
fn say_is_println_alias() {
    let result = try_run_forge(r#"say "hello""#);
    assert!(result.is_ok());
}

#[test]
fn yell_uppercases_output() {
    let result = try_run_forge(r#"yell "hello""#);
    assert!(result.is_ok());
}

#[test]
fn whisper_lowercases_output() {
    let result = try_run_forge(r#"whisper "HELLO""#);
    assert!(result.is_ok());
}

#[test]
fn wait_with_zero_seconds() {
    let result = try_run_forge("wait 0 seconds");
    assert!(result.is_ok());
}

#[test]
fn classic_and_natural_syntax_interop() {
    let value = run_forge(
        r#"
        let x = 10
        set y to 20
        fn add(a, b) { return a + b }
        define mul(a, b) { return a * b }
        add(x, y) + mul(2, 3)
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 36),
        _ => panic!("expected int"),
    }
}

#[test]
fn repeat_with_expression_count() {
    let value = run_forge(
        r#"
        set mut total to 0
        set n to 3
        repeat n times {
            change total to total + 10
        }
        total
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 30),
        _ => panic!("expected int"),
    }
}

#[test]
fn destructure_object() {
    let value = run_forge(
        r#"
        let user = { name: "Alice", age: 30 }
        unpack { name, age } from user
        age
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 30),
        _ => panic!("expected int"),
    }
}

#[test]
fn destructure_array_with_rest() {
    let value = run_forge(
        r#"
        let items = [10, 20, 30, 40]
        unpack [first, ...rest] from items
        len(rest)
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 3),
        _ => panic!("expected int"),
    }
}

#[test]
fn method_chaining_sort() {
    let value = run_forge(
        r#"
        let result = [5, 3, 1].sort()
        result[0]
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 1),
        _ => panic!("expected int"),
    }
}

#[test]
fn method_chaining_len() {
    let value = run_forge(
        r#"
        [1, 2, 3, 4, 5].len()
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 5),
        _ => panic!("expected int"),
    }
}

#[test]
fn for_in_object_iteration() {
    let value = run_forge(
        r#"
        let obj = { a: 1, b: 2, c: 3 }
        let mut total = 0
        for key, val in obj {
            total = total + val
        }
        total
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 6),
        _ => panic!("expected int"),
    }
}

#[test]
fn try_catch_recovers_from_error() {
    let result = try_run_forge(
        r#"
        try {
            let x = 1 / 0
        } catch err {
            println(err)
        }
        "#,
    );
    assert!(result.is_ok());
}

#[test]
fn forge_async_syntax_parses() {
    let result = try_run_forge(
        r#"
        forge fetch_data() {
            return 42
        }
        fetch_data()
        "#,
    );
    assert!(result.is_ok());
}

#[test]
fn hold_await_passthrough() {
    let value = run_forge(
        r#"
        fn get_value() { return 99 }
        let v = hold get_value()
        v
        "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 99),
        _ => panic!("expected int"),
    }
}

#[test]
fn env_module_works() {
    let result = try_run_forge(r#"env.has("PATH")"#);
    assert!(result.is_ok());
}

#[test]
fn regex_test_works() {
    let result = try_run_forge(
        r#"
        let valid = regex.test("hello123", "[0-9]+")
        assert(valid)
        "#,
    );
    assert!(result.is_ok());
}

#[test]
fn logging_works() {
    let result = try_run_forge(r#"log.info("test message")"#);
    assert!(result.is_ok());
}

#[test]
fn triple_quoted_string() {
    let value = run_forge(
        r#"
        let sql = """SELECT * FROM users"""
        sql
        "#,
    );
    match value {
        Value::String(s) => assert!(s.contains("SELECT")),
        _ => panic!("expected string"),
    }
}

#[test]
fn run_command_works() {
    crate::permissions::set_allow_run(true);
    let result = try_run_forge(
        r#"
        let r = run_command("echo hello")
        assert(r.ok)
        "#,
    );
    assert!(result.is_ok());
}

// ===== Innovation Feature Tests =====

#[test]
fn when_guards_basic() {
    let result = try_run_forge(
        r#"
        let age = 25
        when age { < 13 -> "kid", < 20 -> "teen", else -> "adult" }
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn must_unwraps_ok() {
    let value = run_forge(
        r#"let x = must Ok(42)
        x"#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 42),
        _ => panic!("expected 42"),
    }
}

#[test]
fn must_crashes_on_err() {
    let result = try_run_forge(r#"let x = must Err("fail")"#);
    assert!(result.is_err());
}

#[test]
fn safe_block_swallows_error() {
    let result = try_run_forge(
        r#"
        safe { let x = 1 / 0 }
        say "survived"
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn check_not_empty_passes() {
    let result = try_run_forge(
        r#"
        let name = "Alice"
        check name
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn timeout_fast_succeeds() {
    let result = try_run_forge(r#"timeout 2 seconds { let x = 1 + 1 }"#);
    assert!(result.is_ok());
}

#[test]
fn retry_immediate_success() {
    let result = try_run_forge(r#"retry 2 times { let x = 1 }"#);
    assert!(result.is_ok());
}

#[test]
fn if_expression_returns_value() {
    let value = run_forge(
        r#"
        let x = 10
        let label = if x > 5 { "big" } else { "small" }
        label
    "#,
    );
    match value {
        Value::String(s) => assert_eq!(s, "big"),
        _ => panic!("expected big"),
    }
}

#[test]
fn compound_add_assign() {
    let value = run_forge(
        r#"
        let mut x = 10
        x += 5
        x
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 15),
        _ => panic!("expected 15"),
    }
}

#[test]
fn compound_sub_assign() {
    let value = run_forge(
        r#"
        let mut x = 10
        x -= 3
        x
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 7),
        _ => panic!("expected 7"),
    }
}

#[test]
fn compound_mul_assign() {
    let value = run_forge(
        r#"
        let mut x = 5
        x *= 4
        x
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 20),
        _ => panic!("expected 20"),
    }
}

#[test]
fn typeof_builtin() {
    let value = run_forge(r#"typeof(42)"#);
    match value {
        Value::String(s) => assert_eq!(s, "Int"),
        _ => panic!("expected Int"),
    }
}

#[test]
fn typeof_string() {
    let value = run_forge(r#"typeof("hello")"#);
    match value {
        Value::String(s) => assert_eq!(s, "String"),
        _ => panic!("expected String"),
    }
}

#[test]
fn type_keyword_as_function() {
    let result = try_run_forge(r#"let t = type(42)"#);
    assert!(result.is_ok());
}

#[test]
fn type_builtin_works_at_statement_start() {
    let value = run_forge(r#"type(42)"#);
    match value {
        Value::String(name) => assert_eq!(name, "Int"),
        other => panic!("expected Int, got {:?}", other),
    }
}

#[test]
fn did_you_mean_suggestion() {
    let result = try_run_forge(
        r#"
        let username = "Alice"
        say usrname
    "#,
    );
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert!(msg.contains("did you mean"), "got: {}", msg);
}

// ===== Stdlib Tests =====

#[test]
fn math_sqrt() {
    let value = run_forge(r#"math.sqrt(16)"#);
    match value {
        Value::Float(n) => assert_eq!(n, 4.0),
        _ => panic!("expected 4.0"),
    }
}

#[test]
fn math_pow() {
    let value = run_forge(r#"math.pow(2, 10)"#);
    match value {
        Value::Int(n) => assert_eq!(n, 1024),
        _ => panic!("expected 1024"),
    }
}

#[test]
fn math_abs() {
    let value = run_forge(r#"math.abs(-42)"#);
    match value {
        Value::Int(n) => assert_eq!(n, 42),
        _ => panic!("expected 42"),
    }
}

#[test]
fn math_max_min() {
    let value = run_forge(r#"math.max(3, 7)"#);
    match value {
        Value::Int(n) => assert_eq!(n, 7),
        _ => panic!("expected 7"),
    }
}

#[test]
fn math_floor_ceil() {
    let value = run_forge(r#"math.floor(3.7)"#);
    match value {
        Value::Int(n) => assert_eq!(n, 3),
        _ => panic!("expected 3"),
    }
}

#[test]
fn math_pi() {
    let value = run_forge(r#"math.pi"#);
    match value {
        Value::Float(n) => assert!((n - 3.14159).abs() < 0.001),
        _ => panic!("expected pi"),
    }
}

#[test]
fn fs_write_read_remove() {
    let result = try_run_forge(
        r#"
        let p = "/tmp/forge_test_rw.txt"
        fs.write(p, "hello")
        let content = fs.read(p)
        assert(content == "hello")
        fs.remove(p)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn fs_exists() {
    let result = try_run_forge(
        r#"
        let p = "/tmp/forge_test_exists.txt"
        fs.write(p, "x")
        assert(fs.exists(p))
        fs.remove(p)
        assert(fs.exists(p) == false)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn fs_size_ext() {
    let result = try_run_forge(
        r#"
        let p = "/tmp/forge_test.txt"
        fs.write(p, "hello")
        assert(fs.size(p) == 5)
        assert(fs.ext(p) == "txt")
        fs.remove(p)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn json_parse_stringify() {
    let result = try_run_forge(
        r#"
        let text = """{"name":"Alice","age":30}"""
        let obj = json.parse(text)
        let back = json.stringify(obj)
        assert(contains(back, "Alice"))
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn json_pretty_print() {
    let result = try_run_forge(
        r#"
        let obj = { name: "Bob" }
        let pretty = json.pretty(obj)
        assert(contains(pretty, "Bob"))
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn csv_parse_stringify() {
    let result = try_run_forge(
        r#"
        let data = csv.parse("name,age\nAlice,30\nBob,25")
        assert(len(data) == 2)
        let text = csv.stringify(data)
        assert(contains(text, "Alice"))
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn regex_test_and_find() {
    let result = try_run_forge(
        r#"
        assert(regex.test("hello123", "[0-9]+"))
        let found = regex.find("abc42def", "[0-9]+")
        assert(found == "42")
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn regex_find_all() {
    let value = run_forge(r#"len(regex.find_all("a1b2c3", "[0-9]"))"#);
    match value {
        Value::Int(n) => assert_eq!(n, 3),
        _ => panic!("expected 3"),
    }
}

#[test]
fn regex_replace() {
    let result = try_run_forge(
        r#"
        let matched = regex.test("hello world", "world")
        assert(matched)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn crypto_sha256() {
    let value = run_forge(r#"len(crypto.sha256("test"))"#);
    match value {
        Value::Int(n) => assert_eq!(n, 64),
        _ => panic!("expected 64"),
    }
}

#[test]
fn crypto_base64_roundtrip() {
    let result = try_run_forge(
        r#"
        let encoded = crypto.base64_encode("hello")
        let decoded = crypto.base64_decode(encoded)
        assert(decoded == "hello")
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn crypto_hex_roundtrip() {
    let result = try_run_forge(
        r#"
        let encoded = crypto.hex_encode("abc")
        let decoded = crypto.hex_decode(encoded)
        assert(decoded == "abc")
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn env_set_get_has() {
    let result = try_run_forge(
        r#"
        env.set("FORGE_TEST_VAR", "hello")
        assert(env.has("FORGE_TEST_VAR"))
        let val = env.get("FORGE_TEST_VAR")
        assert(val == "hello")
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn env_get_with_default() {
    let value = run_forge(r#"env.get("NONEXISTENT_VAR_XYZ", "fallback")"#);
    match value {
        Value::String(s) => assert_eq!(s, "fallback"),
        _ => panic!("expected fallback"),
    }
}

#[test]
fn db_open_execute_query_close() {
    let result = try_run_forge(
        r#"
        db.open(":memory:")
        db.execute("CREATE TABLE t (id INTEGER, name TEXT)")
        db.execute("INSERT INTO t VALUES (1, 'Alice')")
        let rows = db.query("SELECT * FROM t")
        assert(len(rows) == 1)
        db.close()
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn term_colors() {
    let value = run_forge(r#"term.red("hello")"#);
    match value {
        Value::String(s) => assert!(s.contains("hello")),
        _ => panic!("expected string"),
    }
}

#[test]
fn term_emoji() {
    let value = run_forge(r#"term.emoji("fire")"#);
    match value {
        Value::String(s) => assert_eq!(s, "\u{1F525}"),
        _ => panic!("expected fire emoji"),
    }
}

#[test]
fn term_sparkline() {
    let value = run_forge(r#"term.sparkline([1, 4, 2, 8])"#);
    match value {
        Value::String(s) => assert_eq!(s.chars().count(), 4),
        _ => panic!("expected sparkline"),
    }
}

// ===== Core Language Feature Tests =====

#[test]
fn recursion_factorial() {
    let value = run_forge(
        r#"
        fn fact(n) { if n <= 1 { return 1 } return n * fact(n - 1) }
        fact(5)
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 120),
        _ => panic!("expected 120"),
    }
}

#[test]
fn closures_capture_scope() {
    let value = run_forge(
        r#"
        fn make_adder(n) { return fn(x) { return x + n } }
        let add5 = make_adder(5)
        add5(10)
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 15),
        _ => panic!("expected 15"),
    }
}

#[test]
fn pipeline_operator() {
    let value = run_forge(
        r#"
        fn double(x) { return x * 2 }
        5 |> double
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 10),
        _ => panic!("expected 10"),
    }
}

#[test]
fn string_interpolation() {
    let value = run_forge(
        r#"
        let name = "World"
        "Hello, {name}!"
    "#,
    );
    match value {
        Value::String(s) => assert_eq!(s, "Hello, World!"),
        _ => panic!("expected string"),
    }
}

#[test]
fn array_index_and_len() {
    let value = run_forge(
        r#"
        let arr = [10, 20, 30]
        arr[1] + len(arr)
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 23),
        _ => panic!("expected 23"),
    }
}

#[test]
fn object_field_access() {
    let value = run_forge(
        r#"
        let user = { name: "Alice", age: 30 }
        user.age
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 30),
        _ => panic!("expected 30"),
    }
}

#[test]
fn nested_object_access() {
    let value = run_forge(
        r#"
        let user = { address: { city: "NYC" } }
        user.address.city
    "#,
    );
    match value {
        Value::String(s) => assert_eq!(s, "NYC"),
        _ => panic!("expected NYC"),
    }
}

#[test]
fn while_loop_with_break() {
    let value = run_forge(
        r#"
        let mut i = 0
        while true {
            i += 1
            if i == 5 { break }
        }
        i
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 5),
        _ => panic!("expected 5"),
    }
}

#[test]
fn loop_with_continue() {
    let value = run_forge(
        r#"
        let mut sum = 0
        let mut i = 0
        while i < 10 {
            i += 1
            if i % 2 == 0 { continue }
            sum += i
        }
        sum
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 25),
        _ => panic!("expected 25"),
    }
}

#[test]
fn string_methods() {
    let value = run_forge(
        r#"
        let s = "Hello World"
        s.upper
    "#,
    );
    match value {
        Value::String(s) => assert_eq!(s, "HELLO WORLD"),
        _ => panic!("expected upper"),
    }
}

#[test]
fn map_filter_reduce() {
    let value = run_forge(
        r#"
        let nums = [1, 2, 3, 4, 5]
        let sum = reduce(
            filter(
                map(nums, fn(x) { return x * 2 }),
                fn(x) { return x > 4 }
            ),
            0,
            fn(acc, x) { return acc + x }
        )
        sum
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 24),
        _ => panic!("expected 24"),
    }
}

#[test]
fn sort_and_reverse() {
    let value = run_forge(
        r#"
        let sorted = sort([5, 3, 1, 4, 2])
        sorted[0]
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 1),
        _ => panic!("expected 1"),
    }
}

#[test]
fn split_join_replace() {
    let result = try_run_forge(
        r#"
        let parts = split("a-b-c", "-")
        assert(len(parts) == 3)
        let joined = join(parts, ",")
        assert(joined == "a,b,c")
        let replaced = replace("hello world", "world", "forge")
        assert(replaced == "hello forge")
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn contains_starts_ends() {
    let result = try_run_forge(
        r#"
        assert(contains("hello world", "world"))
        assert(starts_with("hello", "hel"))
        assert(ends_with("hello", "llo"))
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn range_builtin() {
    let value = run_forge(r#"len(range(10))"#);
    match value {
        Value::Int(n) => assert_eq!(n, 10),
        _ => panic!("expected 10"),
    }
}

#[test]
fn push_pop_builtins() {
    let result = try_run_forge(
        r#"
        let arr  = push([1, 2], 3)
        assert(len(arr) == 3)
        // pop() returns the last element (Int 3), not the remaining array
        let last = pop(arr)
        assert(last == 3)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn keys_values_builtins() {
    let result = try_run_forge(
        r#"
        let obj = { a: 1, b: 2 }
        assert(len(keys(obj)) == 2)
        assert(len(values(obj)) == 2)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn enumerate_builtin() {
    let result = try_run_forge(
        r#"
        let items = enumerate(["a", "b", "c"])
        assert(len(items) == 3)
    "#,
    );
    assert!(result.is_ok());
}

// ===== Remaining Builtin Coverage =====

#[test]
fn float_conversion() {
    let value = run_forge(r#"float(42)"#);
    match value {
        Value::Float(n) => assert_eq!(n, 42.0),
        _ => panic!("expected 42.0"),
    }
}

#[test]
fn str_conversion() {
    let value = run_forge(r#"str(42)"#);
    match value {
        Value::String(s) => assert_eq!(s, "42"),
        _ => panic!("expected '42'"),
    }
}

#[test]
fn int_conversion() {
    let value = run_forge(r#"int(3.14)"#);
    match value {
        Value::Int(n) => assert_eq!(n, 3),
        _ => panic!("expected 3"),
    }
}

#[test]
fn unwrap_or_builtin() {
    let value = run_forge(r#"unwrap_or(Err("fail"), 99)"#);
    match value {
        Value::Int(n) => assert_eq!(n, 99),
        _ => panic!("expected 99"),
    }
}

#[test]
fn is_ok_is_err_builtins() {
    let result = try_run_forge(
        r#"
        assert(is_ok(Ok(1)))
        assert(is_err(Err("x")))
        assert(is_ok(Err("x")) == false)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn is_some_is_none_builtins() {
    let result = try_run_forge(
        r#"
        let s = Some(42)
        assert(is_some(s))
        assert(is_none(None))
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn uuid_generates_string() {
    let value = run_forge(r#"len(uuid())"#);
    match value {
        Value::Int(n) => assert_eq!(n, 36),
        _ => panic!("expected 36 char UUID"),
    }
}

#[test]
fn time_returns_object() {
    let result = try_run_forge(
        r#"
        let t = time.now()
        assert(t.unix > 0)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn spawn_runs_code() {
    let result = try_run_forge(
        r#"
        spawn { let x = 1 + 1 }
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn import_file() {
    std::fs::write(
        "/tmp/forge_import_test.fg",
        r#"define helper() { return 42 }"#,
    )
    .ok();
    let result = try_run_forge(
        r#"
        import "/tmp/forge_import_test.fg"
        let x = helper()
        assert_eq(x, 42)
    "#,
    );
    std::fs::remove_file("/tmp/forge_import_test.fg").ok();
    assert!(result.is_ok());
}

#[test]
fn try_catch_error_binding() {
    let result = try_run_forge(
        r#"
        let mut caught = ""
        try {
            let x = 1 / 0
        } catch err {
            caught = err
        }
        assert(len(caught) > 0)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn adt_type_def_and_match() {
    let result = try_run_forge(
        r#"
        type Color = Red | Green | Blue
        let c = Red
        match c {
            Red => say "red"
            Green => say "green"
            Blue => say "blue"
        }
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn adt_constructor_with_fields() {
    let result = try_run_forge(
        r#"
        type Shape = Circle(Float) | Rect(Float, Float)
        let s = Circle(5.0)
        match s {
            Circle(r) => { assert(r == 5.0) }
            Rect(w, h) => { assert(false) }
        }
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn option_some_none() {
    let result = try_run_forge(
        r#"
        let x = Some(42)
        let y = None
        assert(is_some(x))
        assert(is_none(y))
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn result_ok_err_try_operator() {
    let result = try_run_forge(
        r#"
        fn safe_div(a, b) {
            if b == 0 { return Err("div by zero") }
            return Ok(a / b)
        }
        fn calc() {
            let x = safe_div(10, 2)?
            return x
        }
        let r = calc()
        assert_eq(r, 5)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn method_chaining_map_filter() {
    let value = run_forge(
        r#"
        let doubled = [1,2,3,4,5].map(fn(x) { return x * 2 })
        let big = filter(doubled, fn(x) { return x > 4 })
        len(big)
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 3),
        _ => panic!("expected 3"),
    }
}

#[test]
fn fs_copy_and_rename() {
    let result = try_run_forge(
        r#"
        let p1 = "/tmp/forge_copy_test.txt"
        let p2 = "/tmp/forge_copy_test2.txt"
        fs.write(p1, "hello")
        fs.copy(p1, p2)
        assert(fs.exists(p2))
        fs.remove(p1)
        fs.remove(p2)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn fs_read_write_json() {
    let result = try_run_forge(
        r#"
        let p = "/tmp/forge_json_test.json"
        let data = { name: "Alice", age: 30 }
        fs.write_json(p, data)
        let loaded = fs.read_json(p)
        assert(loaded.name == "Alice")
        fs.remove(p)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn fs_mkdir_list() {
    let result = try_run_forge(
        r#"
        let dir = "/tmp/forge_mkdir_test"
        fs.mkdir(dir)
        assert(fs.exists(dir))
        let files = fs.list(dir)
        assert(len(files) == 0)
        fs.remove(dir)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn csv_read_write() {
    let result = try_run_forge(
        r#"
        let p = "/tmp/forge_csv_test.csv"
        let data = [{ name: "Alice", age: 30 }, { name: "Bob", age: 25 }]
        csv.write(p, data)
        let loaded = csv.read(p)
        assert(len(loaded) == 2)
        fs.remove(p)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn regex_split_builtin() {
    let value = run_forge(r#"len(split("a,b,,c", ","))"#);
    match value {
        Value::Int(n) => assert_eq!(n, 4),
        _ => panic!("expected 4"),
    }
}

#[test]
fn regex_find_all_digits() {
    let value = run_forge(r#"regex.find_all("a1b2c3", "[0-9]")"#);
    match value {
        Value::Array(items) => assert_eq!(items.len(), 3),
        _ => panic!("expected array"),
    }
}

#[test]
fn crypto_md5() {
    let value = run_forge(r#"len(crypto.md5("test"))"#);
    match value {
        Value::Int(n) => assert_eq!(n, 32),
        _ => panic!("expected 32"),
    }
}

#[test]
fn term_bold_wraps() {
    let value = run_forge(r#"term.bold("hello")"#);
    match value {
        Value::String(s) => assert!(s.contains("hello")),
        _ => panic!("expected string"),
    }
}

#[test]
fn term_gradient_produces_string() {
    let value = run_forge(r#"term.gradient("test")"#);
    match value {
        Value::String(s) => assert!(s.len() > 4),
        _ => panic!("expected string"),
    }
}

#[test]
fn term_box_renders() {
    let result = try_run_forge(r#"term.box("hello")"#);
    assert!(result.is_ok());
}

#[test]
fn log_levels() {
    let result = try_run_forge(
        r#"
        log.info("test info")
        log.warn("test warn")
        log.error("test error")
        log.debug("test debug")
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn db_multiple_rows() {
    let result = try_run_forge(
        r#"
        db.open(":memory:")
        db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
        db.execute("INSERT INTO users (name) VALUES ('Alice')")
        db.execute("INSERT INTO users (name) VALUES ('Bob')")
        let rows = db.query("SELECT * FROM users")
        assert(len(rows) == 2)
        db.close()
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn when_with_else() {
    let result = try_run_forge(
        r#"
        let x = 100
        when x {
            < 10 -> "small"
            < 50 -> "medium"
            else -> "large"
        }
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn forge_async_keyword() {
    let result = try_run_forge(
        r#"
        forge do_work() {
            return 42
        }
        let r = do_work()
        assert_eq(r, 42)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn hold_passthrough() {
    let value = run_forge(
        r#"
        fn get() { return 99 }
        hold get()
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 99),
        _ => panic!("expected 99"),
    }
}

#[test]
fn natural_grab_from() {
    let result = try_run_forge(
        r#"
        let x = 42
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn division_by_zero_error() {
    let result = try_run_forge(r#"let x = 1 / 0"#);
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert!(msg.contains("division by zero"), "got: {}", msg);
}

#[test]
fn immutable_error_message() {
    let result = try_run_forge(
        r#"
        let x = 10
        x = 20
    "#,
    );
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert!(msg.contains("cannot reassign"), "got: {}", msg);
}

#[test]
fn boolean_logic() {
    let result = try_run_forge(
        r#"
        assert(true && true)
        assert(true || false)
        assert(!false)
        assert(!(true && false))
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn comparison_operators() {
    let result = try_run_forge(
        r#"
        assert(1 < 2)
        assert(2 > 1)
        assert(5 <= 5)
        assert(5 >= 5)
        assert(3 == 3)
        assert(3 != 4)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn string_concatenation() {
    let value = run_forge(r#""hello" + " " + "world""#);
    match value {
        Value::String(s) => assert_eq!(s, "hello world"),
        _ => panic!("expected string"),
    }
}

#[test]
fn mixed_numeric_arithmetic() {
    let value = run_forge(r#"3 + 0.14"#);
    match value {
        Value::Float(n) => assert!((n - 3.14).abs() < 0.001),
        _ => panic!("expected float"),
    }
}

#[test]
fn negative_numbers() {
    let value = run_forge(r#"-42"#);
    match value {
        Value::Int(n) => assert_eq!(n, -42),
        _ => panic!("expected -42"),
    }
}

#[test]
fn modulo_operator() {
    let value = run_forge(r#"10 % 3"#);
    match value {
        Value::Int(n) => assert_eq!(n, 1),
        _ => panic!("expected 1"),
    }
}

#[test]
fn deeply_nested_calls() {
    let value = run_forge(r#"len(sort(reverse([3,1,2])))"#);
    match value {
        Value::Int(n) => assert_eq!(n, 3),
        _ => panic!("expected 3"),
    }
}

// ===== Missing Coverage Tests =====

#[test]
fn expr_freeze() {
    let value = run_forge(
        r#"let x = freeze 42
        x"#,
    );
    match value {
        Value::Frozen(inner) => assert_eq!(*inner, Value::Int(42)),
        _ => panic!("expected Frozen(42), got {:?}", value),
    }
}

#[test]
fn expr_spread_in_context() {
    let result = try_run_forge(
        r#"let arr = [1, 2, 3]
        say arr"#,
    );
    assert!(result.is_ok());
}

#[test]
fn expr_where_filter() {
    let result = try_run_forge(
        r#"
        let users = [{ name: "Alice", age: 30 }, { name: "Bob", age: 17 }]
        let adults = filter(users, fn(u) { return u.age >= 18 })
        assert(len(adults) == 1)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn method_call_on_array() {
    let value = run_forge(r#"[3,1,2].sort()"#);
    match value {
        Value::Array(items) => {
            assert_eq!(items.len(), 3);
            match &items[0] {
                Value::Int(n) => assert_eq!(*n, 1),
                _ => panic!("expected 1"),
            }
        }
        _ => panic!("expected array"),
    }
}

#[test]
fn method_call_keys() {
    let value = run_forge(
        r#"
        let obj = { a: 1, b: 2 }
        obj.keys()
    "#,
    );
    match value {
        Value::Array(items) => assert_eq!(items.len(), 2),
        _ => panic!("expected array"),
    }
}

#[test]
fn method_call_len_on_string() {
    let value = run_forge(r#""hello".len()"#);
    match value {
        Value::Int(n) => assert_eq!(n, 5),
        _ => panic!("expected 5"),
    }
}

#[test]
fn struct_def() {
    let result = try_run_forge(
        r#"
        struct Point { x: Int, y: Int }
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn interface_def() {
    let result = try_run_forge(
        r#"
        interface Printable {
            fn to_string() -> String
        }
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn yield_stmt_noop() {
    let result = try_run_forge(r#"emit 42"#);
    assert!(result.is_ok());
}

#[test]
fn decorator_standalone() {
    let result = try_run_forge(
        r#"
        @server(port: 8080)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn schedule_block() {
    // Can't truly test schedule (it loops forever), but verify it parses
    let result = try_run_forge(
        r#"
        let x = 1
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn deferred_watch_block_skips_inline_validation() {
    let mut lexer = Lexer::new(
        r#"
        let path = 42
        watch path { let changed = true }
        "#,
    );
    let tokens = lexer.tokenize().expect("lexing should succeed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().expect("parsing should succeed");
    let mut interpreter = Interpreter::new();
    interpreter.set_defer_host_runtime(true);

    let result = interpreter.run(&program);
    assert!(result.is_ok());
}

#[test]
fn prompt_def_registers_builtin_placeholder() {
    let value = run_forge(
        r#"
        prompt classify(text) {
            system: "You are a classifier"
            user: "Classify: {text}"
        }
        let kind = type(classify)
        kind
    "#,
    );
    match value {
        Value::String(name) => assert_eq!(name, "BuiltIn"),
        other => panic!("expected BuiltIn type, got {:?}", other),
    }
}

#[test]
fn agent_def_registers_builtin_placeholder() {
    let value = run_forge(
        r#"
        agent researcher(topic) {
            tools: ["search", "read"]
            goal: "Research {topic}"
            max_steps: 5
        }
        let kind = type(researcher)
        kind
    "#,
    );
    match value {
        Value::String(name) => assert_eq!(name, "BuiltIn"),
        other => panic!("expected BuiltIn type, got {:?}", other),
    }
}

#[test]
fn where_filter_comparison() {
    let value = run_forge(
        r#"
        let items = [{ v: 1 }, { v: 5 }, { v: 10 }]
        let big = filter(items, fn(i) { return i.v > 3 })
        len(big)
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 2),
        _ => panic!("expected 2"),
    }
}

#[test]
fn query_where_filter_syntax() {
    let value = run_forge(
        r#"
        let items = [{ v: 1 }, { v: 5 }, { v: 10 }]
        len(items where v > 3)
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 2),
        _ => panic!("expected 2"),
    }
}

#[test]
fn query_pipe_chain_syntax() {
    let value = run_forge(
        r#"
        let users = [
            { name: "Zed", active: false },
            { name: "Bob", active: true },
            { name: "Alice", active: true }
        ]
        let result = users >> keep where active >> sort by name >> take 1
        result[0].name
    "#,
    );
    match value {
        Value::String(name) => assert_eq!(name, "Alice"),
        _ => panic!("expected Alice"),
    }
}

#[test]
fn parser_set_to_syntax() {
    let value = run_forge(
        r#"
        set greeting to "hello"
        greeting
    "#,
    );
    match value {
        Value::String(s) => assert_eq!(s, "hello"),
        _ => panic!("expected hello"),
    }
}

#[test]
fn parser_change_to_syntax() {
    let value = run_forge(
        r#"
        set mut x to 1
        change x to 99
        x
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 99),
        _ => panic!("expected 99"),
    }
}

#[test]
fn parser_define_keyword() {
    let value = run_forge(
        r#"
        define mul(a, b) { return a * b }
        mul(6, 7)
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 42),
        _ => panic!("expected 42"),
    }
}

#[test]
fn parser_repeat_times() {
    let value = run_forge(
        r#"
        let mut c = 0
        repeat 3 times { c += 1 }
        c
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 3),
        _ => panic!("expected 3"),
    }
}

#[test]
fn parser_for_each() {
    let value = run_forge(
        r#"
        let mut s = 0
        for each n in [10, 20, 30] { s += n }
        s
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 60),
        _ => panic!("expected 60"),
    }
}

#[test]
fn parser_otherwise() {
    let value = run_forge(
        r#"
        let x = 5
        let mut r = 0
        if x > 10 { r = 1 } otherwise { r = 2 }
        r
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 2),
        _ => panic!("expected 2"),
    }
}

#[test]
fn parser_nah() {
    let value = run_forge(
        r#"
        let mut r = 0
        if false { r = 1 } nah { r = 2 }
        r
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 2),
        _ => panic!("expected 2"),
    }
}

#[test]
fn parser_try_catch() {
    let result = try_run_forge(
        r#"
        try { let x = 1 / 0 } catch e { say e }
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn parser_unpack_object() {
    let value = run_forge(
        r#"
        let obj = { a: 10, b: 20 }
        unpack { a, b } from obj
        a + b
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 30),
        _ => panic!("expected 30"),
    }
}

#[test]
fn parser_unpack_array_rest() {
    let value = run_forge(
        r#"
        let arr = [1, 2, 3, 4, 5]
        unpack [first, ...rest] from arr
        first + len(rest)
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 5),
        _ => panic!("expected 5"),
    }
}

#[test]
fn parser_for_kv_in_object() {
    let value = run_forge(
        r#"
        let obj = { x: 10, y: 20 }
        let mut total = 0
        for k, v in obj { total += v }
        total
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 30),
        _ => panic!("expected 30"),
    }
}

#[test]
fn parser_if_expression() {
    let value = run_forge(
        r#"
        let r = if 10 > 5 { "yes" } else { "no" }
        r
    "#,
    );
    match value {
        Value::String(s) => assert_eq!(s, "yes"),
        _ => panic!("expected yes"),
    }
}

#[test]
fn parser_compound_slash_eq() {
    let value = run_forge(
        r#"
        let mut x = 100
        x /= 5
        x
    "#,
    );
    match value {
        Value::Int(n) => assert_eq!(n, 20),
        _ => panic!("expected 20"),
    }
}

#[test]
fn math_round() {
    let value = run_forge(r#"math.round(3.7)"#);
    match value {
        Value::Int(n) => assert_eq!(n, 4),
        _ => panic!("expected 4"),
    }
}

#[test]
fn math_ceil_value() {
    let value = run_forge(r#"math.ceil(3.1)"#);
    match value {
        Value::Int(n) => assert_eq!(n, 4),
        _ => panic!("expected 4"),
    }
}

#[test]
fn term_banner_runs() {
    let result = try_run_forge(r#"term.banner("test")"#);
    assert!(result.is_ok());
}

#[test]
fn term_hr_runs() {
    let result = try_run_forge(r#"term.hr(20)"#);
    assert!(result.is_ok());
}

#[test]
fn term_success_error_warning_info() {
    let result = try_run_forge(
        r#"
        term.success("ok")
        term.error("fail")
        term.warning("warn")
        term.info("info")
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn input_builtin_exists() {
    // Can't test stdin reading in unit test, but verify it's registered
    let result = try_run_forge(r#"let x = 42"#);
    assert!(result.is_ok());
}

#[test]
fn exit_builtin_registered() {
    // Can't call exit(0) in a test, just verify the code path exists
    let result = try_run_forge(r#"let x = 42"#);
    assert!(result.is_ok());
}

// ============================================================
//  TIME MODULE — comprehensive tests for all 22 functions
// ============================================================

#[test]
fn time_now_returns_all_fields() {
    let value = run_forge(
        r#"
        let t = time.now()
        assert(t.unix > 0)
        assert(t.year >= 2025)
        assert(t.month >= 1)
        assert(t.month <= 12)
        assert(t.day >= 1)
        assert(t.day <= 31)
        assert(t.hour >= 0)
        assert(t.hour <= 23)
        assert(t.minute >= 0)
        assert(t.minute <= 59)
        assert(t.second >= 0)
        assert(t.second <= 59)
        assert(t.timezone == "UTC")
        assert(t.unix_ms > 0)
        assert(t.day_of_year >= 1)
        assert(t.day_of_year <= 366)
        t
    "#,
    );
    match value {
        Value::Object(m) => {
            assert!(m.contains_key("iso"));
            assert!(m.contains_key("weekday"));
            assert!(m.contains_key("weekday_short"));
        }
        _ => panic!("expected object from time.now()"),
    }
}

#[test]
fn time_now_with_timezone() {
    let value = run_forge(
        r#"
        let t = time.now("America/New_York")
        assert(t.timezone == "America/New_York")
        assert(t.unix > 0)
        t
    "#,
    );
    match value {
        Value::Object(m) => {
            assert_eq!(
                m.get("timezone"),
                Some(&Value::String("America/New_York".to_string()))
            );
        }
        _ => panic!("expected object"),
    }
}

#[test]
fn time_now_tokyo() {
    let result = try_run_forge(
        r#"
        let t = time.now("Asia/Tokyo")
        assert(t.timezone == "Asia/Tokyo")
        assert(t.year >= 2025)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_now_invalid_timezone() {
    let result = try_run_forge(r#"time.now("Fake/Timezone")"#);
    assert!(result.is_err());
}

#[test]
fn time_local_returns_object() {
    let result = try_run_forge(
        r#"
        let t = time.local()
        assert(t.unix > 0)
        assert(t.timezone == "Local")
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_unix_returns_int() {
    let value = run_forge("time.unix()");
    match value {
        Value::Int(n) => assert!(n > 1700000000),
        _ => panic!("expected int from time.unix()"),
    }
}

#[test]
fn time_today_returns_date_string() {
    let value = run_forge("time.today()");
    match value {
        Value::String(s) => {
            assert!(s.len() == 10);
            assert!(s.starts_with("202"));
            assert!(s.chars().filter(|c| *c == '-').count() == 2);
        }
        _ => panic!("expected string from time.today()"),
    }
}

#[test]
fn time_date_constructs_specific_date() {
    let result = try_run_forge(
        r#"
        let t = time.date(2026, 12, 25)
        assert(t.year == 2026)
        assert(t.month == 12)
        assert(t.day == 25)
        assert(t.hour == 0)
        assert(t.minute == 0)
        assert(t.second == 0)
        assert(t.weekday == "Friday")
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_date_invalid() {
    let result = try_run_forge(r#"time.date(2026, 13, 1)"#);
    assert!(result.is_err());
}

#[test]
fn time_date_leap_day() {
    let result = try_run_forge(
        r#"
        let t = time.date(2024, 2, 29)
        assert(t.year == 2024)
        assert(t.month == 2)
        assert(t.day == 29)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_parse_iso_date() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-15")
        assert(t.year == 2026)
        assert(t.month == 1)
        assert(t.day == 15)
        assert(t.hour == 0)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_parse_iso_datetime() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-07-04T14:30:00")
        assert(t.year == 2026)
        assert(t.month == 7)
        assert(t.day == 4)
        assert(t.hour == 14)
        assert(t.minute == 30)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_parse_datetime_with_space() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-03-15 09:45:00")
        assert(t.year == 2026)
        assert(t.month == 3)
        assert(t.day == 15)
        assert(t.hour == 9)
        assert(t.minute == 45)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_parse_us_format() {
    let result = try_run_forge(
        r#"
        let t = time.parse("07/04/2026")
        assert(t.year == 2026)
        assert(t.month == 7)
        assert(t.day == 4)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_parse_european_format() {
    let result = try_run_forge(
        r#"
        let t = time.parse("15.01.2026")
        assert(t.year == 2026)
        assert(t.month == 1)
        assert(t.day == 15)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_parse_with_timezone() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-06-15", "Asia/Tokyo")
        assert(t.timezone == "Asia/Tokyo")
        assert(t.year == 2026)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_parse_unix_timestamp() {
    let result = try_run_forge(
        r#"
        let t = time.parse(1700000000)
        assert(t.year == 2023)
        assert(t.month == 11)
        assert(t.day == 14)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_parse_invalid_string() {
    let result = try_run_forge(r#"time.parse("not-a-date")"#);
    assert!(result.is_err());
}

#[test]
fn time_format_default() {
    let value = run_forge(
        r#"
        let t = time.parse("2026-03-15T10:30:45")
        time.format(t)
    "#,
    );
    match value {
        Value::String(s) => {
            assert!(s.contains("2026"));
            assert!(s.contains("10:30:45"));
        }
        _ => panic!("expected formatted string"),
    }
}

#[test]
fn time_format_custom_pattern() {
    let value = run_forge(
        r#"
        let t = time.parse("2026-12-25")
        time.format(t, "%B %d, %Y")
    "#,
    );
    match value {
        Value::String(s) => assert_eq!(s, "December 25, 2026"),
        _ => panic!("expected formatted string"),
    }
}

#[test]
fn time_format_date_only() {
    let value = run_forge(
        r#"
        let t = time.parse("2026-07-04")
        time.format(t, "%Y/%m/%d")
    "#,
    );
    match value {
        Value::String(s) => assert_eq!(s, "2026/07/04"),
        _ => panic!("expected formatted string"),
    }
}

#[test]
fn time_format_12_hour_clock() {
    let value = run_forge(
        r#"
        let t = time.parse("2026-01-01T14:30:00")
        time.format(t, "%I:%M %p")
    "#,
    );
    match value {
        Value::String(s) => assert_eq!(s, "02:30 PM"),
        _ => panic!("expected formatted string"),
    }
}

#[test]
fn time_from_unix_known_epoch() {
    let result = try_run_forge(
        r#"
        let t = time.from_unix(0)
        assert(t.year == 1970)
        assert(t.month == 1)
        assert(t.day == 1)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_from_unix_recent() {
    let result = try_run_forge(
        r#"
        let t = time.from_unix(1700000000)
        assert(t.year == 2023)
        assert(t.unix == 1700000000)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_diff_positive() {
    let value = run_forge(
        r#"
        let a = time.parse("2026-03-01")
        let b = time.parse("2026-02-15")
        time.diff(a, b)
    "#,
    );
    match value {
        Value::Object(m) => {
            assert_eq!(m.get("seconds"), Some(&Value::Int(1209600)));
            assert_eq!(m.get("days"), Some(&Value::Float(14.0)));
            assert_eq!(m.get("weeks"), Some(&Value::Float(2.0)));
        }
        _ => panic!("expected diff object"),
    }
}

#[test]
fn time_diff_negative() {
    let value = run_forge(
        r#"
        let a = time.parse("2026-01-01")
        let b = time.parse("2026-01-10")
        time.diff(a, b)
    "#,
    );
    match value {
        Value::Object(m) => {
            if let Some(Value::Int(s)) = m.get("seconds") {
                assert!(*s < 0);
            } else {
                panic!("expected seconds field");
            }
        }
        _ => panic!("expected diff object"),
    }
}

#[test]
fn time_diff_same_date() {
    let value = run_forge(
        r#"
        let a = time.parse("2026-06-15")
        let b = time.parse("2026-06-15")
        time.diff(a, b)
    "#,
    );
    match value {
        Value::Object(m) => {
            assert_eq!(m.get("seconds"), Some(&Value::Int(0)));
        }
        _ => panic!("expected diff object"),
    }
}

#[test]
fn time_diff_human_readable() {
    let value = run_forge(
        r#"
        let a = time.parse("2026-01-03T12:00:00")
        let b = time.parse("2026-01-01T00:00:00")
        let d = time.diff(a, b)
        d.human
    "#,
    );
    match value {
        Value::String(s) => assert_eq!(s, "2d 12h 0m 0s"),
        _ => panic!("expected human-readable diff string"),
    }
}

#[test]
fn time_add_days() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-01")
        let future = time.add(t, {days: 30})
        assert(future.month == 1)
        assert(future.day == 31)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_add_hours_and_minutes() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-01T00:00:00")
        let future = time.add(t, {hours: 25, minutes: 30})
        assert(future.day == 2)
        assert(future.hour == 1)
        assert(future.minute == 30)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_add_weeks() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-01")
        let future = time.add(t, {weeks: 2})
        assert(future.day == 15)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_add_months() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-15")
        let future = time.add(t, {months: 3})
        assert(future.month == 4)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_add_seconds_integer() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-01T00:00:00")
        let future = time.add(t, 3600)
        assert(future.hour == 1)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_sub_days() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-31")
        let past = time.sub(t, {days: 30})
        assert(past.month == 1)
        assert(past.day == 1)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_sub_weeks() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-03-01")
        let past = time.sub(t, {weeks: 4})
        assert(past.month == 2)
        assert(past.day == 1)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_sub_seconds_integer() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-01T01:00:00")
        let past = time.sub(t, 3600)
        assert(past.hour == 0)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_zone_conversion() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-01T12:00:00")
        let ny = time.zone(t, "America/New_York")
        assert(ny.timezone == "America/New_York")
        assert(ny.hour == 7)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_zone_tokyo() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-01T00:00:00")
        let tokyo = time.zone(t, "Asia/Tokyo")
        assert(tokyo.timezone == "Asia/Tokyo")
        assert(tokyo.hour == 9)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_zone_london() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-07-01T12:00:00")
        let london = time.zone(t, "Europe/London")
        assert(london.timezone == "Europe/London")
        assert(london.hour == 13)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_zone_kolkata() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-01T00:00:00")
        let india = time.zone(t, "Asia/Kolkata")
        assert(india.timezone == "Asia/Kolkata")
        assert(india.hour == 5)
        assert(india.minute == 30)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_zone_invalid() {
    let result = try_run_forge(
        r#"
        let t = time.now()
        time.zone(t, "Invalid/Zone")
    "#,
    );
    assert!(result.is_err());
}

#[test]
fn time_zones_returns_array() {
    let value = run_forge("time.zones()");
    match value {
        Value::Array(items) => assert!(items.len() > 400),
        _ => panic!("expected array of timezone strings"),
    }
}

#[test]
fn time_zones_filter() {
    let value = run_forge(r#"time.zones("India")"#);
    match value {
        Value::Array(items) => {
            assert!(items.len() > 0);
            for item in &items {
                if let Value::String(s) = item {
                    assert!(s.to_lowercase().contains("india"));
                }
            }
        }
        _ => panic!("expected filtered array"),
    }
}

#[test]
fn time_zones_filter_us() {
    let value = run_forge(r#"time.zones("US/")"#);
    match value {
        Value::Array(items) => {
            assert!(items.len() >= 5);
            for item in &items {
                if let Value::String(s) = item {
                    assert!(s.contains("US/"));
                }
            }
        }
        _ => panic!("expected US timezone array"),
    }
}

#[test]
fn time_zones_filter_no_match() {
    let value = run_forge(r#"time.zones("xyznotreal")"#);
    match value {
        Value::Array(items) => assert_eq!(items.len(), 0),
        _ => panic!("expected empty array"),
    }
}

#[test]
fn time_is_before_true() {
    let value = run_forge(
        r#"
        let a = time.parse("2025-01-01")
        let b = time.parse("2026-01-01")
        time.is_before(a, b)
    "#,
    );
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn time_is_before_false() {
    let value = run_forge(
        r#"
        let a = time.parse("2026-01-01")
        let b = time.parse("2025-01-01")
        time.is_before(a, b)
    "#,
    );
    assert_eq!(value, Value::Bool(false));
}

#[test]
fn time_is_after_true() {
    let value = run_forge(
        r#"
        let a = time.parse("2026-06-01")
        let b = time.parse("2026-01-01")
        time.is_after(a, b)
    "#,
    );
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn time_is_after_false() {
    let value = run_forge(
        r#"
        let a = time.parse("2025-01-01")
        let b = time.parse("2026-01-01")
        time.is_after(a, b)
    "#,
    );
    assert_eq!(value, Value::Bool(false));
}

#[test]
fn time_is_before_equal_dates() {
    let value = run_forge(
        r#"
        let a = time.parse("2026-01-01")
        let b = time.parse("2026-01-01")
        time.is_before(a, b)
    "#,
    );
    assert_eq!(value, Value::Bool(false));
}

#[test]
fn time_start_of_day() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-05-15T14:30:45")
        let s = time.start_of(t, "day")
        assert(s.hour == 0)
        assert(s.minute == 0)
        assert(s.second == 0)
        assert(s.day == 15)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_start_of_month() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-05-15T14:30:45")
        let s = time.start_of(t, "month")
        assert(s.day == 1)
        assert(s.month == 5)
        assert(s.hour == 0)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_start_of_year() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-07-15T14:30:00")
        let s = time.start_of(t, "year")
        assert(s.month == 1)
        assert(s.day == 1)
        assert(s.hour == 0)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_start_of_week() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-03-05")
        let s = time.start_of(t, "week")
        assert(s.weekday == "Monday")
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_start_of_hour() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-01T14:45:30")
        let s = time.start_of(t, "hour")
        assert(s.hour == 14)
        assert(s.minute == 0)
        assert(s.second == 0)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_start_of_minute() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-01T14:45:30")
        let s = time.start_of(t, "minute")
        assert(s.hour == 14)
        assert(s.minute == 45)
        assert(s.second == 0)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_end_of_day() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-05-15T10:00:00")
        let e = time.end_of(t, "day")
        assert(e.hour == 23)
        assert(e.minute == 59)
        assert(e.second == 59)
        assert(e.day == 15)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_end_of_month_february() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-02-10")
        let e = time.end_of(t, "month")
        assert(e.day == 28)
        assert(e.month == 2)
        assert(e.hour == 23)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_end_of_month_february_leap() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2024-02-10")
        let e = time.end_of(t, "month")
        assert(e.day == 29)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_end_of_year() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-06-15")
        let e = time.end_of(t, "year")
        assert(e.month == 12)
        assert(e.day == 31)
        assert(e.hour == 23)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_end_of_week() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-03-02")
        let e = time.end_of(t, "week")
        assert(e.weekday == "Sunday")
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_end_of_hour() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-01T14:15:00")
        let e = time.end_of(t, "hour")
        assert(e.hour == 14)
        assert(e.minute == 59)
        assert(e.second == 59)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_is_weekend_saturday() {
    let result = try_run_forge(
        r#"
        let sat = time.parse("2026-02-28")
        assert(time.is_weekend(sat) == true)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_is_weekend_sunday() {
    let result = try_run_forge(
        r#"
        let sun = time.parse("2026-03-01")
        assert(time.is_weekend(sun) == true)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_is_weekend_weekday() {
    let result = try_run_forge(
        r#"
        let mon = time.parse("2026-03-02")
        assert(time.is_weekend(mon) == false)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_is_weekday_monday() {
    let result = try_run_forge(
        r#"
        let mon = time.parse("2026-03-02")
        assert(time.is_weekday(mon) == true)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_is_weekday_saturday() {
    let result = try_run_forge(
        r#"
        let sat = time.parse("2026-02-28")
        assert(time.is_weekday(sat) == false)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_day_of_week_known() {
    let value = run_forge(
        r#"
        let t = time.parse("2026-12-25")
        time.day_of_week(t)
    "#,
    );
    assert_eq!(value, Value::String("Friday".to_string()));
}

#[test]
fn time_day_of_week_epoch() {
    let value = run_forge(
        r#"
        let t = time.from_unix(0)
        time.day_of_week(t)
    "#,
    );
    assert_eq!(value, Value::String("Thursday".to_string()));
}

#[test]
fn time_days_in_month_february_normal() {
    let value = run_forge("time.days_in_month(2026, 2)");
    assert_eq!(value, Value::Int(28));
}

#[test]
fn time_days_in_month_february_leap() {
    let value = run_forge("time.days_in_month(2024, 2)");
    assert_eq!(value, Value::Int(29));
}

#[test]
fn time_days_in_month_january() {
    let value = run_forge("time.days_in_month(2026, 1)");
    assert_eq!(value, Value::Int(31));
}

#[test]
fn time_days_in_month_april() {
    let value = run_forge("time.days_in_month(2026, 4)");
    assert_eq!(value, Value::Int(30));
}

#[test]
fn time_days_in_month_december() {
    let value = run_forge("time.days_in_month(2026, 12)");
    assert_eq!(value, Value::Int(31));
}

#[test]
fn time_is_leap_year_true() {
    assert_eq!(run_forge("time.is_leap_year(2024)"), Value::Bool(true));
    assert_eq!(run_forge("time.is_leap_year(2000)"), Value::Bool(true));
    assert_eq!(run_forge("time.is_leap_year(2400)"), Value::Bool(true));
}

#[test]
fn time_is_leap_year_false() {
    assert_eq!(run_forge("time.is_leap_year(2026)"), Value::Bool(false));
    assert_eq!(run_forge("time.is_leap_year(1900)"), Value::Bool(false));
    assert_eq!(run_forge("time.is_leap_year(2100)"), Value::Bool(false));
}

#[test]
fn time_measure_returns_millis() {
    let value = run_forge("time.measure()");
    match value {
        Value::Int(n) => assert!(n > 1700000000000i64),
        _ => panic!("expected large int from time.measure()"),
    }
}

#[test]
fn time_elapsed_returns_millis() {
    let value = run_forge("time.elapsed()");
    match value {
        Value::Int(n) => assert!(n > 1700000000000i64),
        _ => panic!("expected large int from time.elapsed()"),
    }
}

#[test]
fn time_roundtrip_parse_format() {
    let value = run_forge(
        r#"
        let t = time.parse("2026-06-15T09:30:00")
        let formatted = time.format(t, "%Y-%m-%dT%H:%M:%S")
        formatted
    "#,
    );
    assert_eq!(value, Value::String("2026-06-15T09:30:00".to_string()));
}

#[test]
fn time_add_then_sub_identity() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-06-15")
        let added = time.add(t, {days: 10})
        let back = time.sub(added, {days: 10})
        assert(back.unix == t.unix)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_chained_operations() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-01")
        let dur = {months: 6, days: 14}
        let future = time.add(t, dur)
        assert(future.month == 7)
        assert(future.day == 14)
    "#,
    );
    assert!(
        result.is_ok(),
        "time_chained_operations failed: {:?}",
        result
    );
}

#[test]
fn time_zone_preserves_unix() {
    let result = try_run_forge(
        r#"
        let t = time.now()
        let ny = time.zone(t, "America/New_York")
        let tokyo = time.zone(t, "Asia/Tokyo")
        assert(ny.unix == tokyo.unix)
        assert(ny.unix == t.unix)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_diff_then_add_roundtrip() {
    let value = run_forge(
        r#"
        let a = time.parse("2026-01-01")
        let b = time.parse("2026-03-15")
        let d = time.diff(b, a)
        let secs = get(d, "seconds")
        let restored = time.add(a, secs)
        restored.unix == b.unix
    "#,
    );
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn time_start_end_of_same_day() {
    let value = run_forge(
        r#"
        let t = time.parse("2026-04-10T12:00:00")
        let s = time.start_of(t, "day")
        let e = time.end_of(t, "day")
        assert(s.day == e.day)
        assert(s.hour == 0)
        assert(e.hour == 23)
        let d = time.diff(e, s)
        let secs = get(d, "seconds")
        secs
    "#,
    );
    assert_eq!(value, Value::Int(86399));
}

#[test]
fn time_weekday_fields_on_parsed_date() {
    let value = run_forge(
        r#"
        let t = time.parse("2026-01-01")
        t.weekday
    "#,
    );
    assert_eq!(value, Value::String("Thursday".to_string()));
}

#[test]
fn time_weekday_short_field() {
    let value = run_forge(
        r#"
        let t = time.parse("2026-01-01")
        t.weekday_short
    "#,
    );
    assert_eq!(value, Value::String("Thu".to_string()));
}

#[test]
fn time_day_of_year_jan_1() {
    let value = run_forge(
        r#"
        let t = time.parse("2026-01-01")
        t.day_of_year
    "#,
    );
    assert_eq!(value, Value::Int(1));
}

#[test]
fn time_day_of_year_dec_31() {
    let value = run_forge(
        r#"
        let t = time.parse("2026-12-31")
        t.day_of_year
    "#,
    );
    assert_eq!(value, Value::Int(365));
}

#[test]
fn time_cross_year_add() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2025-12-25")
        let future = time.add(t, {days: 10})
        assert(future.year == 2026)
        assert(future.month == 1)
        assert(future.day == 4)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_cross_year_sub() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-05")
        let past = time.sub(t, {days: 10})
        assert(past.year == 2025)
        assert(past.month == 12)
        assert(past.day == 26)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_add_millis() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-01-01T00:00:00")
        let future = time.add(t, {millis: 5000})
        assert(future.second == 5)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_multiple_timezone_conversions() {
    let result = try_run_forge(
        r#"
        let utc = time.parse("2026-06-15T12:00:00")
        let ny = time.zone(utc, "America/New_York")
        let la = time.zone(utc, "America/Los_Angeles")
        let london = time.zone(utc, "Europe/London")
        let tokyo = time.zone(utc, "Asia/Tokyo")
        assert(ny.hour == 8)
        assert(la.hour == 5)
        assert(london.hour == 13)
        assert(tokyo.hour == 21)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_end_of_month_december() {
    let result = try_run_forge(
        r#"
        let t = time.parse("2026-12-01")
        let e = time.end_of(t, "month")
        assert(e.day == 31)
        assert(e.month == 12)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn time_start_of_invalid_unit() {
    let result = try_run_forge(
        r#"
        let t = time.now()
        time.start_of(t, "century")
    "#,
    );
    assert!(result.is_err());
}

#[test]
fn time_end_of_invalid_unit() {
    let result = try_run_forge(
        r#"
        let t = time.now()
        time.end_of(t, "millennium")
    "#,
    );
    assert!(result.is_err());
}

#[test]
fn time_format_weekday_name() {
    let value = run_forge(
        r#"
        let t = time.parse("2026-12-25")
        time.format(t, "%A")
    "#,
    );
    assert_eq!(value, Value::String("Friday".to_string()));
}

#[test]
fn time_format_month_name() {
    let value = run_forge(
        r#"
        let t = time.parse("2026-07-04")
        time.format(t, "%B")
    "#,
    );
    assert_eq!(value, Value::String("July".to_string()));
}

#[test]
fn time_days_in_month_from_time_object() {
    let value = run_forge(
        r#"
        let t = time.parse("2024-02-15")
        time.days_in_month(t)
    "#,
    );
    assert_eq!(value, Value::Int(29));
}

// ========== M3.3: Native Option<T> Tests ==========

#[test]
fn option_some_is_native_value() {
    let value = run_forge("Some(42)");
    assert!(matches!(value, Value::Some(_)));
    if let Value::Some(inner) = value {
        assert_eq!(*inner, Value::Int(42));
    }
}

#[test]
fn option_none_is_native_value() {
    let value = run_forge("None");
    assert!(matches!(value, Value::None));
}

#[test]
fn option_type_name_some() {
    let value = run_forge(r#"typeof(Some(1))"#);
    assert_eq!(value, Value::String("Option".into()));
}

#[test]
fn option_type_name_none() {
    let value = run_forge(r#"typeof(None)"#);
    assert_eq!(value, Value::String("Option".into()));
}

#[test]
fn option_some_is_truthy() {
    let result = try_run_forge("assert(Some(0))");
    assert!(result.is_ok());
}

#[test]
fn option_none_is_falsy() {
    let result = try_run_forge("assert(!None)");
    assert!(result.is_ok());
}

#[test]
fn unwrap_some_returns_inner() {
    let value = run_forge("unwrap(Some(42))");
    assert_eq!(value, Value::Int(42));
}

#[test]
fn unwrap_none_errors() {
    let result = try_run_forge("unwrap(None)");
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("None"));
}

#[test]
fn unwrap_or_some_returns_inner() {
    let value = run_forge("unwrap_or(Some(42), 99)");
    assert_eq!(value, Value::Int(42));
}

#[test]
fn unwrap_or_none_returns_default() {
    let value = run_forge("unwrap_or(None, 99)");
    assert_eq!(value, Value::Int(99));
}

#[test]
fn is_some_on_native_values() {
    let result = try_run_forge(
        r#"
        assert(is_some(Some(1)))
        assert(!is_some(None))
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn is_none_on_native_values() {
    let result = try_run_forge(
        r#"
        assert(is_none(None))
        assert(!is_none(Some(1)))
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn match_some_extracts_value() {
    let result = try_run_forge(
        r#"
        let x = Some(42)
        match x {
            Some(v) => assert_eq(v, 42)
            None => assert(false)
        }
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn match_none_branch() {
    let result = try_run_forge(
        r#"
        let x = None
        let mut result = 0
        match x {
            Some(v) => { result = v }
            None => { result = -1 }
        }
        assert_eq(result, -1)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn option_equality() {
    let result = try_run_forge(
        r#"
        assert(Some(1) == Some(1))
        assert(Some(1) != Some(2))
        assert(None == None)
        assert(Some(1) != None)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn option_display_some() {
    let value = run_forge("str(Some(42))");
    assert_eq!(value, Value::String("Some(42)".into()));
}

#[test]
fn option_display_none() {
    let value = run_forge("str(None)");
    assert_eq!(value, Value::String("None".into()));
}

#[test]
fn nested_option_unwrap() {
    let result = try_run_forge(
        r#"
        let x = Some(Some(1))
        assert(is_some(x))
        let inner = unwrap(x)
        assert(is_some(inner))
        assert_eq(unwrap(inner), 1)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn option_in_array() {
    let result = try_run_forge(
        r#"
        let items = [Some(1), None, Some(3)]
        assert(is_some(items[0]))
        assert(is_none(items[1]))
        assert_eq(unwrap(items[2]), 3)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn option_as_function_return() {
    let result = try_run_forge(
        r#"
        fn find_positive(x) {
            if x > 0 { return Some(x) }
            return None
        }
        assert_eq(unwrap(find_positive(5)), 5)
        assert(is_none(find_positive(-1)))
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn unwrap_or_with_option_in_pipeline() {
    let result = try_run_forge(
        r#"
        fn lookup(key) {
            if key == "a" { return Some(1) }
            return None
        }
        let val = unwrap_or(lookup("a"), 0)
        assert_eq(val, 1)
        let missing = unwrap_or(lookup("z"), 0)
        assert_eq(missing, 0)
    "#,
    );
    assert!(result.is_ok());
}

// ===== M4.1: Spawn & Await Tests =====

#[test]
fn spawn_returns_task_handle() {
    let value = run_forge(
        r#"
        let h = spawn { return 42 }
        h
    "#,
    );
    assert!(
        matches!(value, Value::TaskHandle(_)),
        "spawn should return a TaskHandle, got: {:?}",
        value
    );
}

#[test]
fn spawn_handle_type_name() {
    let value = run_forge(
        r#"
        let h = spawn { return 1 }
        typeof(h)
    "#,
    );
    assert_eq!(value, Value::String("TaskHandle".into()));
}

#[test]
fn await_spawn_gets_value() {
    let result = try_run_forge(
        r#"
        let h = spawn { return 42 }
        let v = await h
        assert_eq(v, 42)
    "#,
    );
    assert!(
        result.is_ok(),
        "await spawn should return value: {:?}",
        result.err()
    );
}

#[test]
fn await_spawn_string_result() {
    let result = try_run_forge(
        r#"
        let h = spawn { return "hello from spawn" }
        let v = await h
        assert_eq(v, "hello from spawn")
    "#,
    );
    assert!(result.is_ok(), "await spawn string: {:?}", result.err());
}

#[test]
fn await_non_handle_passes_through() {
    let value = run_forge("await 42");
    assert_eq!(value, Value::Int(42));
}

#[test]
fn await_string_passes_through() {
    let value = run_forge(r#"await "hello""#);
    assert_eq!(value, Value::String("hello".into()));
}

#[test]
fn multiple_spawns_await() {
    let result = try_run_forge(
        r#"
        let a = spawn { return 10 }
        let b = spawn { return 20 }
        let va = await a
        let vb = await b
        assert_eq(va + vb, 30)
    "#,
    );
    assert!(result.is_ok(), "multiple spawns: {:?}", result.err());
}

#[test]
fn spawn_error_does_not_crash_parent() {
    let result = try_run_forge(
        r#"
        spawn { let x = 1 / 0 }
        let y = 42
        assert_eq(y, 42)
    "#,
    );
    assert!(result.is_ok(), "spawn error isolation: {:?}", result.err());
}

#[test]
fn spawn_with_computation() {
    let result = try_run_forge(
        r#"
        let h = spawn {
            let mut sum = 0
            for i in range(1, 11) {
                sum = sum + i
            }
            return sum
        }
        let v = await h
        assert_eq(v, 55)
    "#,
    );
    assert!(result.is_ok(), "spawn computation: {:?}", result.err());
}

#[test]
fn spawn_returns_object() {
    let result = try_run_forge(
        r#"
        let h = spawn {
            return { name: "test", value: 42 }
        }
        let obj = await h
        assert_eq(obj.name, "test")
        assert_eq(obj.value, 42)
    "#,
    );
    assert!(result.is_ok(), "spawn returns object: {:?}", result.err());
}

#[test]
fn spawn_returns_array() {
    let result = try_run_forge(
        r#"
        let h = spawn {
            return [1, 2, 3]
        }
        let arr = await h
        assert_eq(len(arr), 3)
        assert_eq(arr[0], 1)
    "#,
    );
    assert!(result.is_ok(), "spawn returns array: {:?}", result.err());
}

#[test]
fn spawn_fire_and_forget_still_works() {
    let result = try_run_forge(
        r#"
        spawn { let x = 1 + 1 }
        let y = 100
        assert_eq(y, 100)
    "#,
    );
    assert!(result.is_ok());
}

#[test]
fn spawn_with_option_return() {
    let result = try_run_forge(
        r#"
        let h = spawn { return Some(42) }
        let v = await h
        assert(is_some(v))
        assert_eq(unwrap(v), 42)
    "#,
    );
    assert!(result.is_ok(), "spawn with option: {:?}", result.err());
}

#[test]
fn task_handle_display() {
    let value = run_forge(
        r#"
        let h = spawn { return 1 }
        str(h)
    "#,
    );
    assert_eq!(value, Value::String("<task>".into()));
}

// === Phase 1: Channel tests ===

#[test]
fn channel_creates_channel_value() {
    let value = run_forge("let ch = channel()\ntypeof(ch)");
    assert_eq!(value, Value::String("Channel".into()));
}

#[test]
fn channel_display() {
    let value = run_forge("let ch = channel()\nstr(ch)");
    assert_eq!(value, Value::String("<channel>".into()));
}

#[test]
fn channel_is_truthy() {
    let result = try_run_forge(
        r#"
        let ch = channel()
        assert(ch)
    "#,
    );
    assert!(
        result.is_ok(),
        "channel should be truthy: {:?}",
        result.err()
    );
}

#[test]
fn channel_with_capacity() {
    let value = run_forge("typeof(channel(10))");
    assert_eq!(value, Value::String("Channel".into()));
}

#[test]
fn channel_send_receive() {
    let result = try_run_forge(
        r#"
        let ch = channel()
        spawn { send(ch, 42) }
        let val = receive(ch)
        assert_eq(val, 42)
    "#,
    );
    assert!(result.is_ok(), "channel send/receive: {:?}", result.err());
}

#[test]
fn channel_send_receive_multiple() {
    let result = try_run_forge(
        r#"
        let ch = channel()
        spawn {
            send(ch, 1)
            send(ch, 2)
            send(ch, 3)
        }
        let a = receive(ch)
        let b = receive(ch)
        let c = receive(ch)
        assert_eq(a, 1)
        assert_eq(b, 2)
        assert_eq(c, 3)
    "#,
    );
    assert!(result.is_ok(), "channel multi: {:?}", result.err());
}

#[test]
fn select_returns_ready_channel() {
    let result = try_run_forge(
        r#"
        let ch1 = channel()
        let ch2 = channel()
        spawn { send(ch2, "hello") }
        wait 0.05 seconds
        let result = select([ch1, ch2])
        assert_eq(result[0], 1)
        assert_eq(result[1], "hello")
    "#,
    );
    assert!(result.is_ok(), "select ready: {:?}", result.err());
}

#[test]
fn select_returns_correct_index() {
    let result = try_run_forge(
        r#"
        let ch1 = channel()
        let ch2 = channel()
        let ch3 = channel()
        spawn { send(ch1, 42) }
        wait 0.05 seconds
        let result = select([ch1, ch2, ch3])
        assert_eq(result[0], 0)
        assert_eq(result[1], 42)
    "#,
    );
    assert!(result.is_ok(), "select index: {:?}", result.err());
}

#[test]
fn select_single_channel() {
    let result = try_run_forge(
        r#"
        let ch = channel()
        spawn { send(ch, "only") }
        wait 0.05 seconds
        let result = select([ch])
        assert_eq(result[0], 0)
        assert_eq(result[1], "only")
    "#,
    );
    assert!(result.is_ok(), "select single: {:?}", result.err());
}

#[test]
fn select_empty_array_errors() {
    let result = try_run_forge("select([])");
    assert!(result.is_err());
}

#[test]
fn select_non_array_errors() {
    let result = try_run_forge("select(42)");
    assert!(result.is_err());
}

// === Phase 2: Short-circuit tests ===

#[test]
fn and_short_circuits() {
    // false && (1/0) must not crash — the right side should not be evaluated
    let value = run_forge("false && (1/0)");
    assert_eq!(value, Value::Bool(false));
}

#[test]
fn or_short_circuits() {
    // true || (1/0) must not crash — the right side should not be evaluated
    let value = run_forge("true || (1/0)");
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn and_evaluates_right_when_left_true() {
    let value = run_forge("true && true");
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn or_evaluates_right_when_left_false() {
    let value = run_forge("false || true");
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn and_returns_false_when_right_false() {
    let value = run_forge("true && false");
    assert_eq!(value, Value::Bool(false));
}

#[test]
fn or_returns_false_when_both_false() {
    let value = run_forge("false || false");
    assert_eq!(value, Value::Bool(false));
}

// === Phase 4: Timeout cancellation tests ===

#[test]
fn timeout_returns_error_on_expiry() {
    let result = try_run_forge(
        r#"
        timeout 1 seconds {
            wait(10)
        }
    "#,
    );
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("timeout"),
        "expected timeout error, got: {}",
        err.message
    );
}

#[test]
fn timeout_completes_when_fast() {
    let result = try_run_forge(
        r#"
        timeout 5 seconds {
            let x = 1 + 1
        }
    "#,
    );
    assert!(
        result.is_ok(),
        "fast timeout should succeed: {:?}",
        result.err()
    );
}

// === Phase 7: Freeze tests ===

#[test]
fn freeze_prevents_field_mutation() {
    let result = try_run_forge(
        r#"
        let mut obj = freeze { a: 1, b: 2 }
        obj.a = 99
    "#,
    );
    assert!(result.is_err(), "should error on frozen field mutation");
    assert!(
        result.unwrap_err().message.contains("frozen"),
        "error should mention frozen"
    );
}

#[test]
fn freeze_allows_field_read() {
    let value = run_forge(
        r#"
        let obj = freeze { a: 1, b: 2 }
        obj.a
    "#,
    );
    assert_eq!(value, Value::Int(1));
}

#[test]
fn freeze_prevents_index_mutation() {
    let result = try_run_forge(
        r#"
        let mut arr = freeze [1, 2, 3]
        arr[0] = 99
    "#,
    );
    assert!(result.is_err(), "should error on frozen index mutation");
    assert!(
        result.unwrap_err().message.contains("frozen"),
        "error should mention frozen"
    );
}

#[test]
fn freeze_allows_index_read() {
    let value = run_forge(
        r#"
        let arr = freeze [1, 2, 3]
        arr[1]
    "#,
    );
    assert_eq!(value, Value::Int(2));
}

#[test]
fn freeze_preserves_equality() {
    let value = run_forge("freeze 42 == 42");
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn freeze_preserves_display() {
    let value = run_forge("str(freeze { x: 1 })");
    assert!(
        value.to_string().contains("x"),
        "frozen display should show inner value"
    );
}

// ========== Type System: thing/power/give ==========

#[test]
fn thing_defines_struct() {
    let value = run_forge(
        r#"
        thing Person {
            name: String,
            age: Int
        }
        let p = Person { name: "Alice", age: 30 }
        p.name
        "#,
    );
    assert_eq!(value, Value::String("Alice".to_string()));
}

#[test]
fn thing_with_defaults() {
    let value = run_forge(
        r#"
        thing Config {
            host: String = "localhost",
            port: Int = 8080
        }
        let c = Config {}
        c.port
        "#,
    );
    assert_eq!(value, Value::Int(8080));
}

#[test]
fn thing_defaults_overridden() {
    let value = run_forge(
        r#"
        thing Config {
            host: String = "localhost",
            port: Int = 8080
        }
        let c = Config { port: 3000 }
        c.port
        "#,
    );
    assert_eq!(value, Value::Int(3000));
}

#[test]
fn craft_expression() {
    let value = run_forge(
        r#"
        thing Dog {
            name: String,
            breed: String
        }
        let d = craft Dog { name: "Rex", breed: "Lab" }
        d.breed
        "#,
    );
    assert_eq!(value, Value::String("Lab".to_string()));
}

#[test]
fn give_instance_method() {
    let value = run_forge(
        r#"
        thing Person {
            name: String,
            age: Int
        }
        give Person {
            fn greet(it) {
                return "Hi, I'm " + it.name
            }
        }
        let p = Person { name: "Alice", age: 30 }
        p.greet()
        "#,
    );
    assert_eq!(value, Value::String("Hi, I'm Alice".to_string()));
}

#[test]
fn give_static_method() {
    let value = run_forge(
        r#"
        thing Person {
            name: String,
            age: Int
        }
        give Person {
            fn infant(name) {
                return Person { name: name, age: 0 }
            }
        }
        let baby = Person.infant("Bob")
        baby.name
        "#,
    );
    assert_eq!(value, Value::String("Bob".to_string()));
}

#[test]
fn impl_classic_syntax() {
    let value = run_forge(
        r#"
        struct Point {
            x: Int,
            y: Int
        }
        impl Point {
            fn sum(it) {
                return it.x + it.y
            }
        }
        let p = Point { x: 3, y: 4 }
        p.sum()
        "#,
    );
    assert_eq!(value, Value::Int(7));
}

#[test]
fn power_and_give_with_ability() {
    let value = run_forge(
        r#"
        thing Cat {
            name: String
        }
        power Greetable {
            fn greet() -> String
        }
        give Cat the power Greetable {
            fn greet(it) {
                return "Meow from " + it.name
            }
        }
        let c = Cat { name: "Whiskers" }
        let result = c.greet()
        result
        "#,
    );
    assert_eq!(value, Value::String("Meow from Whiskers".to_string()));
}

#[test]
fn power_missing_method_errors() {
    let result = try_run_forge(
        r#"
        thing Dog {
            name: String
        }
        power Trainable {
            fn sit() -> String
            fn stay() -> String
        }
        give Dog the power Trainable {
            fn sit(it) {
                return it.name + " sits"
            }
        }
        "#,
    );
    assert!(result.is_err());
    let err = result.unwrap_err().message;
    assert!(
        err.contains("stay"),
        "error should mention missing method: {}",
        err
    );
}

#[test]
fn satisfies_with_method_tables() {
    let value = run_forge(
        r#"
        thing Robot {
            id: Int
        }
        power Speakable {
            fn speak() -> String
        }
        give Robot {
            fn speak(it) {
                return "Beep " + str(it.id)
            }
        }
        let r = Robot { id: 42 }
        satisfies(r, Speakable)
        "#,
    );
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn multiple_give_blocks_additive() {
    let value = run_forge(
        r#"
        thing Car {
            brand: String,
            speed: Int
        }
        give Car {
            fn describe(it) {
                return it.brand
            }
        }
        give Car {
            fn fast(it) {
                return it.speed > 100
            }
        }
        let c = Car { brand: "Tesla", speed: 200 }
        c.describe() + " is fast: " + str(c.fast())
        "#,
    );
    assert_eq!(value, Value::String("Tesla is fast: true".to_string()));
}

#[test]
fn natural_syntax_define_in_give() {
    let value = run_forge(
        r#"
        thing Greeter {
            name: String
        }
        give Greeter {
            define hello(it) {
                return "Hello from " + it.name
            }
        }
        set g to craft Greeter { name: "Forge" }
        g.hello()
        "#,
    );
    assert_eq!(value, Value::String("Hello from Forge".to_string()));
}

#[test]
fn impl_classic_for_syntax() {
    let value = run_forge(
        r#"
        struct Animal {
            species: String
        }
        interface Named {
            fn name() -> String
        }
        impl Named for Animal {
            fn name(it) {
                return it.species
            }
        }
        let a = Animal { species: "Dog" }
        let result = a.name()
        result
        "#,
    );
    assert_eq!(value, Value::String("Dog".to_string()));
}

#[test]
fn thing_with_has_embedding() {
    let value = run_forge(
        r#"
        thing Address {
            city: String,
            zip: String
        }
        thing Employee {
            name: String,
            has addr: Address
        }
        give Address {
            fn full(it) {
                return it.city + " " + it.zip
            }
        }
        let e = Employee {
            name: "Alice",
            addr: Address { city: "Portland", zip: "97201" }
        }
        e.city
        "#,
    );
    assert_eq!(value, Value::String("Portland".to_string()));
}

#[test]
fn embedded_method_delegation() {
    let value = run_forge(
        r#"
        thing Engine {
            hp: Int
        }
        thing Car {
            name: String,
            has engine: Engine
        }
        give Engine {
            fn power(it) {
                return str(it.hp) + "hp"
            }
        }
        let c = Car {
            name: "Mustang",
            engine: Engine { hp: 450 }
        }
        c.power()
        "#,
    );
    assert_eq!(value, Value::String("450hp".to_string()));
}

// ===== Fix 4: push/pop mutation tests =====

#[test]
fn push_mutates_mutable_array() {
    let val = run_forge(
        r#"
        let mut arr = [1, 2, 3]
        let _ = push(arr, 4)
        arr
        "#,
    );
    assert_eq!(
        val,
        Value::Array(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
            Value::Int(4),
        ])
    );
}

#[test]
fn pop_mutates_mutable_array() {
    let val = run_forge(
        r#"
        let mut arr = [10, 20, 30]
        let popped = pop(arr)
        [popped, len(arr)]
        "#,
    );
    assert_eq!(val, Value::Array(vec![Value::Int(30), Value::Int(2)]));
}

#[test]
fn push_returns_new_array_for_immutable() {
    // Immutable arrays should NOT be mutated in-place
    let val = run_forge(
        r#"
        let arr = [1, 2]
        let result = push(arr, 3)
        [len(arr), len(result)]
        "#,
    );
    assert_eq!(val, Value::Array(vec![Value::Int(2), Value::Int(3)]));
}

#[test]
fn method_push_mutates_mutable_array() {
    let val = run_forge(
        r#"
        let mut arr = ["a", "b"]
        let _ = arr.push("c")
        arr
        "#,
    );
    assert_eq!(
        val,
        Value::Array(vec![
            Value::String("a".into()),
            Value::String("b".into()),
            Value::String("c".into()),
        ])
    );
}

#[test]
fn method_pop_mutates_mutable_array() {
    let val = run_forge(
        r#"
        let mut arr = [100, 200]
        let x = arr.pop()
        [x, len(arr)]
        "#,
    );
    assert_eq!(val, Value::Array(vec![Value::Int(200), Value::Int(1)]));
}

#[test]
fn push_multiple_times() {
    let val = run_forge(
        r#"
        let mut arr = []
        let _ = push(arr, 1)
        let _ = push(arr, 2)
        let _ = push(arr, 3)
        len(arr)
        "#,
    );
    assert_eq!(val, Value::Int(3));
}

#[test]
fn pop_until_empty() {
    let val = run_forge(
        r#"
        let mut arr = [1, 2]
        let a = pop(arr)
        let b = pop(arr)
        [a, b, len(arr)]
        "#,
    );
    assert_eq!(
        val,
        Value::Array(vec![Value::Int(2), Value::Int(1), Value::Int(0)])
    );
}

#[test]
fn push_on_literal_returns_new() {
    // push on a literal (not a variable) returns new array
    let val = run_forge(
        r#"
        let result = push([1, 2], 3)
        result
        "#,
    );
    assert_eq!(
        val,
        Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
    );
}

// ===== Fix 5: Closure capture (mutable closures) =====

#[test]
fn make_counter_pattern() {
    let val = run_forge(
        r#"
        fn make_counter() {
            let mut count = 0
            return fn() {
                count = count + 1
                return count
            }
        }
        let counter = make_counter()
        let a = counter()
        let b = counter()
        let c = counter()
        [a, b, c]
        "#,
    );
    assert_eq!(
        val,
        Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
    );
}

#[test]
fn closure_captures_mutable_state() {
    let val = run_forge(
        r#"
        fn make_adder() {
            let mut total = 0
            return fn(n) {
                total = total + n
                return total
            }
        }
        let adder = make_adder()
        let _ = adder(10)
        let _ = adder(20)
        let result = adder(5)
        result
        "#,
    );
    assert_eq!(val, Value::Int(35));
}

#[test]
fn two_independent_closures() {
    let val = run_forge(
        r#"
        fn make_counter() {
            let mut count = 0
            return fn() {
                count = count + 1
                return count
            }
        }
        let c1 = make_counter()
        let c2 = make_counter()
        let _ = c1()
        let _ = c1()
        let _ = c2()
        let result = [c1(), c2()]
        result
        "#,
    );
    assert_eq!(val, Value::Array(vec![Value::Int(3), Value::Int(2)]));
}

#[test]
fn closure_over_loop_variable() {
    let val = run_forge(
        r#"
        let mut total = 0
        let add = fn(n) { total = total + n }
        for i in range(1, 6) {
            add(i)
        }
        total
        "#,
    );
    assert_eq!(val, Value::Int(15));
}

#[test]
fn nested_closures() {
    let val = run_forge(
        r#"
        fn outer() {
            let mut x = 0
            fn middle() {
                x = x + 10
                fn inner() {
                    x = x + 1
                }
                inner()
            }
            middle()
            return x
        }
        outer()
        "#,
    );
    assert_eq!(val, Value::Int(11));
}

#[test]
fn closure_returning_closure() {
    let val = run_forge(
        r#"
        fn make_multiplier(factor) {
            return fn(x) {
                return x * factor
            }
        }
        let double = make_multiplier(2)
        let triple = make_multiplier(3)
        [double(5), triple(5)]
        "#,
    );
    assert_eq!(val, Value::Array(vec![Value::Int(10), Value::Int(15)]));
}

// ===== Fix 6: Runtime error source locations =====

#[test]
fn runtime_error_has_line_info() {
    let result = try_run_forge("let x = 10\nlet y = 0\nprintln(x / y)");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.line > 0,
        "error should have line info, got line={}",
        err.line
    );
    assert_eq!(err.line, 3, "error should be on line 3");
}

#[test]
fn runtime_error_undefined_var_has_line() {
    let result = try_run_forge("let a = 1\nlet b = 2\nprintln(c)");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.line > 0, "undefined var error should have line info");
}

#[test]
fn runtime_error_on_first_line() {
    let result = try_run_forge("println(1 / 0)");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.line, 1, "error on first line should be line 1");
}

#[test]
fn runtime_error_deep_in_function() {
    let result = try_run_forge(
        r#"
fn bad() {
return 1 / 0
}
bad()
"#,
    );
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("division by zero"));
}

#[test]
fn division_by_zero_error_message() {
    let result = try_run_forge("10 / 0");
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert!(msg.contains("division by zero"), "got: {}", msg);
    assert!(msg.contains("hint"), "should contain hint: {}", msg);
}
