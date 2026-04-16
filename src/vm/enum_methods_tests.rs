// VM enum-method tests (M9.5 impl blocks on algebraic types)
//
// Mirrors `interpreter/tests.rs::enum_*` to keep the VM backend in
// parity. Exercises the full surface: basic dispatch, multi-arg
// methods, returning ADT instances, chained calls, destructuring in
// match bodies, predicate methods, recursive ADTs, collection builtins
// dispatching via lambda, static methods, error paths, and parser pins.
//
// The VM's method_tables/static_methods live on `VM` and are populated
// by `__forge_register_method` / `__forge_call_method` emitted by the
// compiler from `impl Type { ... }`. None of these tests construct
// method tables manually — they all flow through the normal compiler
// path. Variant names avoid Option/Result collisions (Some/None/Ok/Err).

use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::vm::compiler;
use crate::vm::machine::{VMError, VM};

fn parse_program(source: &str) -> crate::parser::ast::Program {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer error");
    let mut parser = Parser::new(tokens);
    parser.parse_program().expect("parse error")
}

fn vm_output(source: &str) -> Vec<String> {
    let program = parse_program(source);
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    vm.execute(&chunk).expect("execution error");
    vm.output.clone()
}

fn vm_run(source: &str) -> Result<(), VMError> {
    let program = parse_program(source);
    let chunk = compiler::compile(&program).expect("compile error");
    let mut vm = VM::new();
    vm.execute(&chunk)?;
    Ok(())
}

// ----- Core dispatch ------------------------------------------------------

#[test]
fn vm_enum_method_single_variant_returns_primitive() {
    assert_eq!(
        vm_output(
            r#"
            type Box = Wrap(int)
            impl Box {
                fn peek(it) { return it._0 }
            }
            say Wrap(42).peek()
            "#
        ),
        vec!["42"]
    );
}

#[test]
fn vm_enum_method_two_variant_dispatch() {
    assert_eq!(
        vm_output(
            r#"
            type Shape = Circle(float) | Square(float)
            impl Shape {
                fn area(it) {
                    match it {
                        Circle(r) => return 3.14 * r * r
                        Square(s) => return s * s
                    }
                }
            }
            say Circle(5.0).area()
            "#
        ),
        vec!["78.5"]
    );
}

#[test]
fn vm_enum_method_with_one_extra_arg() {
    assert_eq!(
        vm_output(
            r#"
            type Box = Wrap(int)
            impl Box {
                fn add(it, n) { return it._0 + n }
            }
            say Wrap(10).add(5)
            "#
        ),
        vec!["15"]
    );
}

#[test]
fn vm_enum_method_with_multiple_extra_args() {
    assert_eq!(
        vm_output(
            r#"
            type Box = Wrap(int)
            impl Box {
                fn compute(it, a, b, c) { return it._0 + a * b - c }
            }
            say Wrap(100).compute(2, 3, 4)
            "#
        ),
        vec!["102"]
    );
}

#[test]
fn vm_enum_method_returns_same_type_instance() {
    assert_eq!(
        vm_output(
            r#"
            type Shape = Circle(float) | Square(float)
            impl Shape {
                fn scale(it, f) {
                    match it {
                        Circle(radius) => return Circle(radius * f)
                        Square(side) => return Square(side * f)
                    }
                }
                fn area_squared(it) {
                    match it {
                        Circle(radius) => return radius * radius
                        Square(side) => return side * side
                    }
                }
            }
            say Circle(2.0).scale(3.0).area_squared()
            "#
        ),
        vec!["36"]
    );
}

#[test]
fn vm_enum_method_variant_converting() {
    // Reads Circle's field and constructs a Square in the same body.
    assert_eq!(
        vm_output(
            r#"
            type Shape = Circle(float) | Square(float)
            impl Shape {
                fn to_square(it) {
                    match it {
                        Circle(r) => return Square(r * 2.0)
                        Square(s) => return Square(s)
                    }
                }
                fn side(it) {
                    match it {
                        Circle(r) => return r
                        Square(s) => return s
                    }
                }
            }
            say Circle(3.0).to_square().side()
            "#
        ),
        vec!["6"]
    );
}

#[test]
fn vm_enum_method_chained_calls() {
    assert_eq!(
        vm_output(
            r#"
            type Box = Wrap(int)
            impl Box {
                fn inc(it) { return Wrap(it._0 + 1) }
                fn get(it) { return it._0 }
            }
            say Wrap(10).inc().inc().inc().get()
            "#
        ),
        vec!["13"]
    );
}

#[test]
fn vm_enum_method_field_destructuring_in_match() {
    assert_eq!(
        vm_output(
            r#"
            type Pair = Tuple2(int, int)
            impl Pair {
                fn sum(it) {
                    match it {
                        Tuple2(a, b) => return a + b
                    }
                }
            }
            say Tuple2(7, 35).sum()
            "#
        ),
        vec!["42"]
    );
}

#[test]
fn vm_enum_method_calls_another_method_on_it() {
    assert_eq!(
        vm_output(
            r#"
            type Box = Wrap(int)
            impl Box {
                fn raw(it) { return it._0 }
                fn doubled(it) { return it.raw() * 2 }
            }
            say Wrap(21).doubled()
            "#
        ),
        vec!["42"]
    );
}

#[test]
fn vm_enum_method_calls_free_function() {
    assert_eq!(
        vm_output(
            r#"
            fn double(n) { return n * 2 }
            type Box = Wrap(int)
            impl Box {
                fn via_free(it) { return double(it._0) }
            }
            say Wrap(21).via_free()
            "#
        ),
        vec!["42"]
    );
}

#[test]
fn vm_enum_method_let_bindings_and_early_return() {
    assert_eq!(
        vm_output(
            r#"
            type Box = Wrap(int)
            impl Box {
                fn clamp_pos(it) {
                    let n = it._0
                    if n < 0 { return 0 }
                    return n
                }
            }
            say Wrap(-5).clamp_pos()
            say Wrap(7).clamp_pos()
            "#
        ),
        vec!["0", "7"]
    );
}

#[test]
fn vm_enum_method_zero_field_plus_data_variant() {
    assert_eq!(
        vm_output(
            r#"
            type Status = Idle | Active(int)
            impl Status {
                fn level(it) {
                    match it {
                        Idle => return 0
                        Active(n) => return n
                    }
                }
            }
            say Idle.level()
            say Active(9).level()
            "#
        ),
        vec!["0", "9"]
    );
}

#[test]
fn vm_enum_method_predicate_returns_bool() {
    assert_eq!(
        vm_output(
            r#"
            type Shape = Circle(float) | Square(float)
            impl Shape {
                fn is_circle(it) {
                    match it {
                        Circle(_) => return true
                        _ => return false
                    }
                }
            }
            say Circle(1.0).is_circle()
            say Square(1.0).is_circle()
            "#
        ),
        vec!["true", "false"]
    );
}

#[test]
fn vm_enum_method_reads_field_via_underscore_zero() {
    assert_eq!(
        vm_output(
            r#"
            type Box = Wrap(int)
            impl Box {
                fn peek(it) { return it._0 }
            }
            say Wrap(123).peek()
            "#
        ),
        vec!["123"]
    );
}

#[test]
fn vm_enum_method_match_wildcard_fallthrough() {
    assert_eq!(
        vm_output(
            r#"
            type Kind = One | Two | Three
            impl Kind {
                fn tag(it) {
                    match it {
                        One => return "one"
                        _ => return "other"
                    }
                }
            }
            say One.tag()
            say Two.tag()
            say Three.tag()
            "#
        ),
        vec!["one", "other", "other"]
    );
}

#[test]
fn vm_enum_method_three_variant_type() {
    assert_eq!(
        vm_output(
            r##"
            type Color = Red | Green | Blue
            impl Color {
                fn hex(it) {
                    match it {
                        Red => return "#f00"
                        Green => return "#0f0"
                        Blue => return "#00f"
                    }
                }
            }
            say Green.hex()
            "##
        ),
        vec!["#0f0"]
    );
}

#[test]
fn vm_enum_method_taking_closure() {
    assert_eq!(
        vm_output(
            r#"
            type Box = Wrap(int)
            impl Box {
                fn map_val(it, f) { return Wrap(f(it._0)) }
                fn get(it) { return it._0 }
            }
            say Wrap(10).map_val(fn(x) { return x + 5 }).get()
            "#
        ),
        vec!["15"]
    );
}

#[test]
fn vm_enum_method_closure_captures_it() {
    // Per R1: lambda captures `it` from the method body.
    assert_eq!(
        vm_output(
            r#"
            type Box = Wrap(int)
            impl Box {
                fn scale_each(it) {
                    return [1, 2, 3].map(fn(x) { return x * it._0 })
                }
            }
            say Wrap(10).scale_each()
            "#
        ),
        vec!["[10, 20, 30]"]
    );
}

#[test]
fn vm_enum_method_recursive_on_nested_adt() {
    // Per M1: constructor-recursive fields, not flat self-recursion.
    // VM does NOT have the interpreter's match_pattern Binding-smart
    // peek bug, so we can exercise deep recursion here.
    assert_eq!(
        vm_output(
            r#"
            type Tree = Leaf(int) | Node(Tree, Tree)
            impl Tree {
                fn sum(it) {
                    match it {
                        Leaf(n) => return n
                        Node(l, r) => return l.sum() + r.sum()
                    }
                }
            }
            say Node(Leaf(1), Node(Leaf(2), Leaf(3))).sum()
            "#
        ),
        vec!["6"]
    );
}

#[test]
fn vm_enum_method_throws_via_must_err() {
    let res = vm_run(
        r#"
        type Box = Wrap(int)
        impl Box {
            fn bomb(it) { must err("kaboom") }
        }
        Wrap(0).bomb()
        "#,
    );
    assert!(
        res.is_err(),
        "expected must-err to propagate, got {:?}",
        res
    );
}

#[test]
fn vm_enum_method_nested_dispatch_across_methods() {
    assert_eq!(
        vm_output(
            r#"
            type Box = Wrap(int)
            impl Box {
                fn a(it) { return it.b() + 1 }
                fn b(it) { return it.c() + 2 }
                fn c(it) { return it._0 + 4 }
            }
            say Wrap(10).a()
            "#
        ),
        vec!["17"]
    );
}

#[test]
fn vm_enum_method_type_annotation_on_arg() {
    assert_eq!(
        vm_output(
            r#"
            type Box = Wrap(int)
            impl Box {
                fn scaled(it, multiplier: float) { return it._0 * multiplier }
            }
            say Wrap(10).scaled(2.5)
            "#
        ),
        vec!["25"]
    );
}

// ----- Collection-builtin integration (per M2) ----------------------------

#[test]
fn vm_enum_method_via_map() {
    assert_eq!(
        vm_output(
            r#"
            type Shape = Circle(float) | Square(float)
            impl Shape {
                fn area_squared(it) {
                    match it {
                        Circle(radius) => return radius * radius
                        Square(side) => return side * side
                    }
                }
            }
            let shapes = [Circle(1.0), Square(2.0), Circle(3.0)]
            say shapes.map(fn(shape) { return shape.area_squared() })
            "#
        ),
        vec!["[1, 4, 9]"]
    );
}

#[test]
fn vm_enum_method_via_filter() {
    assert_eq!(
        vm_output(
            r#"
            type Shape = Circle(float) | Square(float)
            impl Shape {
                fn is_circle(it) {
                    match it {
                        Circle(_) => return true
                        _ => return false
                    }
                }
            }
            let shapes = [Circle(1.0), Square(2.0), Circle(3.0)]
            say shapes.filter(fn(s) { return s.is_circle() }).len()
            "#
        ),
        vec!["2"]
    );
}

#[test]
fn vm_enum_method_via_reduce() {
    assert_eq!(
        vm_output(
            r#"
            type Shape = Circle(float) | Square(float)
            impl Shape {
                fn area_squared(it) {
                    match it {
                        Circle(radius) => return radius * radius
                        Square(side) => return side * side
                    }
                }
            }
            let shapes = [Square(2.0), Square(3.0), Square(4.0)]
            say shapes.reduce(0.0, fn(acc, shape) { return acc + shape.area_squared() })
            "#
        ),
        vec!["29"]
    );
}

#[test]
fn vm_enum_method_via_sort_by_area() {
    assert_eq!(
        vm_output(
            r#"
            type Shape = Circle(float) | Square(float)
            impl Shape {
                fn area_squared(it) {
                    match it {
                        Circle(radius) => return radius * radius
                        Square(side) => return side * side
                    }
                }
                fn extent(it) {
                    match it {
                        Circle(radius) => return radius
                        Square(side) => return side
                    }
                }
            }
            let shapes = [Square(5.0), Square(2.0), Square(4.0)]
            let sorted = shapes.sort(fn(x, y) {
                if x.area_squared() < y.area_squared() { return -1 }
                if x.area_squared() > y.area_squared() { return 1 }
                return 0
            })
            say sorted.map(fn(shape) { return shape.extent() })
            "#
        ),
        vec!["[2, 4, 5]"]
    );
}

// ----- Static methods on algebraic types (per B3) ------------------------
// Both interpreter and VM currently can't resolve `TypeName.method()` on
// algebraic `type` definitions — the type name is never bound as a
// callable receiver. Pinned here so a future fix is a visible diff.

#[test]
fn vm_enum_pin_static_method_zero_arg_not_resolvable_on_algebraic() {
    let res = vm_run(
        r#"
        type Shape = Circle(float) | Square(float)
        impl Shape {
            fn unit_circle() { return Circle(1.0) }
        }
        Shape.unit_circle()
        "#,
    );
    assert!(
        res.is_err(),
        "pin: expected undefined-Shape error today, got {:?}",
        res
    );
}

#[test]
fn vm_enum_pin_static_method_with_arg_not_resolvable_on_algebraic() {
    let res = vm_run(
        r#"
        type Shape = Circle(float) | Square(float)
        impl Shape {
            fn from_radius(r) { return Circle(r) }
        }
        Shape.from_radius(7.5)
        "#,
    );
    assert!(
        res.is_err(),
        "pin: expected undefined-Shape error today, got {:?}",
        res
    );
}

// ----- Error-path tests (per M3) ------------------------------------------

#[test]
fn vm_enum_method_error_no_such_method() {
    let res = vm_run(
        r#"
        type Box = Wrap(int)
        impl Box {
            fn peek(it) { return it._0 }
        }
        Wrap(1).nonexistent()
        "#,
    );
    assert!(
        res.is_err(),
        "expected error for missing method, got {:?}",
        res
    );
}

#[test]
fn vm_enum_method_pin_missing_self_return_type() {
    // Per B2: `Self` is not implemented as a real type; parses and is
    // erased. Pin current behavior so a future `Self` implementation
    // shows up as a visible diff.
    assert_eq!(
        vm_output(
            r#"
            type Box = Wrap(int)
            impl Box {
                fn inc(it) -> Self { return Wrap(it._0 + 1) }
                fn get(it) { return it._0 }
            }
            say Wrap(5).inc().get()
            "#
        ),
        vec!["6"]
    );
}

// ----- Spawn parity (per Risks) -------------------------------------------

#[test]
fn vm_enum_pin_spawn_child_vm_missing_type_registry() {
    // Per Risks: `fork_for_spawn` does NOT propagate type definitions
    // or method_tables to the child VM. The spawned closure can't
    // resolve the `Wrap` constructor (undefined variable), and even
    // if the ADT value is captured as an upvalue, `.peek()` fails
    // with "no method 'peek' on Object" because the child has no
    // method table. Pinned here so a future fix is a visible diff.
    let res = vm_run(
        r#"
        type Box = Wrap(int)
        impl Box {
            fn peek(it) { return it._0 }
        }
        let h = spawn { say Wrap(99).peek() }
        h.join()
        "#,
    );
    assert!(
        res.is_err(),
        "pin: expected spawn/join to error today, got {:?}",
        res
    );
}
