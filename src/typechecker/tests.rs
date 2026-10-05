//! Unit tests of the checker (the original gradual checker's tests, plus
//! inference, resolution and diagnostic-code tests).

use super::*;

fn check_source(source: &str, strict: bool) -> Vec<Diagnostic> {
    analyze(source, &CheckOptions { strict, file: None })
        .expect("test source parses")
        .diagnostics
}

fn warnings_for(source: &str) -> Vec<Diagnostic> {
    check_source(source, false)
}

fn errors_for(source: &str) -> Vec<Diagnostic> {
    check_source(source, true)
}

#[test]
fn no_warnings_for_unannotated_code() {
    let w = warnings_for("let x = 42\nlet y = x + 1\nprintln(y)");
    assert!(w.is_empty());
}

#[test]
fn no_warnings_for_correct_annotations() {
    let w = warnings_for("let x: Int = 42");
    assert!(w.is_empty());
}

#[test]
fn warns_on_let_type_mismatch() {
    let w = warnings_for("let x: Int = \"hello\"");
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("type mismatch"));
    assert!(w[0].message.contains("Int"));
    assert!(w[0].message.contains("String"));
    assert!(!w[0].is_error());
}

#[test]
fn strict_mode_produces_errors() {
    let w = errors_for("let x: Int = \"hello\"");
    assert_eq!(w.len(), 1);
    assert!(w[0].is_error());
}

#[test]
fn warns_on_return_type_mismatch() {
    let w = warnings_for("fn add(a: Int, b: Int) -> Int { return \"oops\" }");
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("return type mismatch"));
}

#[test]
fn no_warning_for_correct_return() {
    let w = warnings_for("fn add(a: Int, b: Int) -> Int { return a + b }");
    assert!(w.is_empty());
}

#[test]
fn warns_on_arity_mismatch() {
    let w = warnings_for("fn add(a, b) { return a + b }\nadd(1, 2, 3)");
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("expects 2"));
}

#[test]
fn warns_on_argument_type_mismatch() {
    let w = warnings_for("fn double(x: Int) -> Int { return x * 2 }\ndouble(\"hello\")");
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("argument 1"));
    assert!(w[0].message.contains("expected Int"));
    assert!(w[0].message.contains("got String"));
}

#[test]
fn no_warning_for_correct_args() {
    let w = warnings_for("fn double(x: Int) -> Int { return x * 2 }\ndouble(5)");
    assert!(w.is_empty());
}

#[test]
fn infers_string_concatenation() {
    let w = warnings_for("let x: Int = \"a\" + \"b\"");
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("String"));
}

#[test]
fn infers_float_promotion() {
    // 1 + 2.5 is a Float. Int widens to Float, but a Float is not an Int.
    assert!(warnings_for("let x: Float = 1 + 2").is_empty());
    let w = warnings_for("let x: Int = 1 + 2.5");
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].code, Code::TypeMismatch);
}

#[test]
fn infers_comparison_as_bool() {
    let w = warnings_for("let x: Int = 5 > 3");
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("Bool"));
}

#[test]
fn infers_array_type() {
    let w = warnings_for("let x: Int = [1, 2, 3]");
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("[Int]"));
}

#[test]
fn infers_object_type() {
    let w = warnings_for("let x: Int = { name: \"Odin\" }");
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("Object"));
}

#[test]
fn assignment_type_check() {
    let w = warnings_for("let mut x: Int = 5\nx = \"hello\"");
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("type mismatch"));
}

#[test]
fn unannotated_code_no_errors_strict() {
    let w = errors_for("let x = 42\nlet y = \"hello\"");
    assert!(w.is_empty());
}

#[test]
fn builtin_return_types_known() {
    let w = warnings_for("let x: String = len([1,2,3])");
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("Int"));
}

#[test]
fn string_interp_inferred_as_string() {
    let w = warnings_for("let name = \"world\"\nlet x: Int = \"hello {name}\"");
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("String"));
}

#[test]
fn negation_preserves_type() {
    let w = warnings_for("let x: String = -5");
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("Int"));
}

#[test]
fn not_always_bool() {
    let w = warnings_for("let x: Int = !true");
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("Bool"));
}

#[test]
fn lambda_body_checked() {
    let w = warnings_for(
        "fn takes_int(x: Int) -> Int { return x }\nlet f = fn() { takes_int(\"bad\") }",
    );
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("argument 1"));
}

#[test]
fn multiple_errors() {
    let w =
        warnings_for("let x: Int = \"hello\"\nlet y: String = 42\nfn f(a, b) { return a }\nf(1)");
    assert_eq!(w.len(), 3);
}

#[test]
fn interface_satisfaction_pass() {
    let w = warnings_for(
        "interface Printable { fn display() -> String }\nstruct User { name: String, display: String }\nfn show(p: Printable) { println(p) }\nlet u = User { name: \"Alice\", display: \"Alice\" }\nshow(u)",
    );
    // User has 'display' field, satisfies Printable — no warning
    let interface_warnings: Vec<_> = w.iter().filter(|w| w.message.contains("satisfy")).collect();
    assert!(interface_warnings.is_empty());
}

#[test]
fn interface_satisfaction_fail() {
    let w = warnings_for(
        "interface Serializable { fn serialize() -> String }\nstruct Point { x: Int, y: Int }\nfn save(s: Serializable) { println(s) }\nlet p = Point { x: 1, y: 2 }\nsave(p)",
    );
    let interface_warnings: Vec<_> = w.iter().filter(|w| w.message.contains("satisfy")).collect();
    assert!(!interface_warnings.is_empty());
    assert!(interface_warnings[0].message.contains("serialize"));
}

#[test]
fn interface_impl_block_satisfaction() {
    let w = warnings_for(
        "interface Printable { fn display() -> String }\nstruct User { name: String }\nimpl User { fn display() -> String { return \"User\" } }\nfn show(p: Printable) { println(p) }\nlet u = User { name: \"Alice\" }\nshow(u)",
    );
    let interface_warnings: Vec<_> = w
        .iter()
        .filter(|w| {
            w.message.contains("satisfy")
                || w.message.contains("parameter")
                || w.message.contains("returns")
        })
        .collect();
    assert!(
        interface_warnings.is_empty(),
        "impl block method should satisfy interface: {:?}",
        interface_warnings
    );
}

#[test]
fn interface_wrong_param_count() {
    let w = warnings_for(
        "interface Hasher { fn hash(data: String) -> Int }\nstruct MyHash { x: Int }\nimpl MyHash { fn hash() -> Int { return 0 } }\nfn do_hash(h: Hasher) { println(h) }\nlet m = MyHash { x: 1 }\ndo_hash(m)",
    );
    let param_warnings: Vec<_> = w
        .iter()
        .filter(|w| w.message.contains("parameter"))
        .collect();
    assert!(
        !param_warnings.is_empty(),
        "wrong param count should warn: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn interface_wrong_return_type() {
    let w = warnings_for(
        "interface Stringer { fn to_str() -> String }\nstruct Num { val: Int }\nimpl Num { fn to_str() -> Int { return 0 } }\nfn stringify(s: Stringer) { println(s) }\nlet n = Num { val: 1 }\nstringify(n)",
    );
    let ret_warnings: Vec<_> = w.iter().filter(|w| w.message.contains("returns")).collect();
    assert!(
        !ret_warnings.is_empty(),
        "wrong return type should warn: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn interface_multi_method() {
    let w = warnings_for(
        "interface ReadWrite { fn read() -> String\nfn write(data: String) }\nstruct File { path: String }\nfn process(rw: ReadWrite) { println(rw) }\nlet f = File { path: \"test\" }\nprocess(f)",
    );
    let missing_warnings: Vec<_> = w.iter().filter(|w| w.message.contains("missing")).collect();
    assert!(
        missing_warnings.len() >= 2,
        "should warn about both missing methods: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

// ========== M3.3: Option<T> Type Checking ==========

#[test]
fn option_type_annotation_accepts_none() {
    let w = warnings_for("let x: ?Int = None");
    assert!(w.is_empty(), "None should be valid for ?Int");
}

#[test]
fn option_type_annotation_accepts_some() {
    let w = warnings_for("let x: ?Int = Some(42)");
    assert!(w.is_empty(), "Some(42) should be valid for ?Int");
}

#[test]
fn non_optional_rejects_none() {
    let w = warnings_for("let x: Int = None");
    assert!(!w.is_empty(), "None should not be valid for bare Int");
}

#[test]
fn some_inferred_as_option_type() {
    let w = warnings_for("let x: Int = Some(42)");
    assert!(!w.is_empty(), "Some(42) is Option, not Int");
    assert!(w[0].message.contains("Option") || w[0].message.contains("?"));
}

// ========== 8A.1: Return Type Inference ==========

#[test]
fn infers_return_type_from_explicit_return() {
    // add() returns Int (inferred), so assigning to String should warn
    let w = warnings_for("fn add(a: Int, b: Int) { return a + b }\nlet x: String = add(1, 2)");
    assert_eq!(w.len(), 1);
    assert!(
        w[0].message.contains("Int"),
        "expected Int mismatch, got: {}",
        w[0].message
    );
}

#[test]
fn inferred_return_type_no_false_positive() {
    // add() returns Int (inferred), assigned to Int — no warning
    let w = warnings_for("fn add(a: Int, b: Int) { return a + b }\nlet x: Int = add(1, 2)");
    assert!(
        w.is_empty(),
        "should not warn: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn infers_string_return_type() {
    let w = warnings_for(
        "fn greet(name: String) { return \"hello \" + name }\nlet x: Int = greet(\"world\")",
    );
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("String"));
}

#[test]
fn infers_null_for_no_return() {
    // Function with no return statements returns Null
    let w = warnings_for("fn noop() { let x = 1 }\nlet y: Int = noop()");
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("Null"));
}

#[test]
fn infers_from_multiple_consistent_returns() {
    let w = warnings_for(
        "fn abs_val(x: Int) {\n  if x > 0 { return x }\n  return 0 - x\n}\nlet y: String = abs_val(5)",
    );
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("Int"));
}

#[test]
fn mixed_int_float_promotes_to_float() {
    let w = warnings_for(
        "fn mixed(x: Int) {\n  if x > 0 { return x }\n  return 1.5\n}\nlet y: String = mixed(5)",
    );
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("Float"));
}

#[test]
fn incompatible_returns_stay_unknown() {
    // Int and String returns → Unknown, so no warning on caller
    let w = warnings_for(
        "fn weird(x: Int) {\n  if x > 0 { return x }\n  return \"negative\"\n}\nlet y: String = weird(5)",
    );
    // Unknown return type — no mismatch warning for caller
    assert!(w.is_empty());
}

#[test]
fn implicit_last_expression_inferred() {
    // Last expression is the return value
    let w = warnings_for("fn double(x: Int) { x * 2 }\nlet y: String = double(5)");
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("Int"));
}

#[test]
fn forward_call_inference_works() {
    // caller defined before callee — pass 1.5 runs on all functions
    let w = warnings_for("fn caller() { return callee(5) }\nfn callee(x: Int) { return x * 2 }");
    // callee's return type is inferred as Int, so caller's return is also Int
    // No warnings expected (no type annotations to conflict)
    assert!(w.is_empty());
}

// ========== 8A.2: Flow-Sensitive Type Narrowing ==========

#[test]
fn narrowing_not_null_unwraps_option() {
    // x is ?String, but after `x != null` check it should be String
    let w = warnings_for("fn f(x: ?String) {\n  if x != null {\n    let y: String = x\n  }\n}");
    assert!(
        w.is_empty(),
        "should not warn when Option narrowed to inner type: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn narrowing_eq_null_narrows_to_null() {
    // In the then-branch of `x == null`, x is Null
    // In the else-branch, x should be non-null (String)
    let w = warnings_for(
        "fn f(x: ?String) {\n  if x == null {\n    let y: Null = x\n  } else {\n    let z: String = x\n  }\n}",
    );
    assert!(
        w.is_empty(),
        "should narrow to Null in then, String in else: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn narrowing_does_not_leak_scope() {
    // After the if-block (no early return), narrowing should not persist
    let w = warnings_for(
        "fn f(x: ?String) {\n  if x != null {\n    let y: String = x\n  }\n  let z: String = x\n}",
    );
    // The `let z: String = x` should warn because x is still ?String outside the if
    assert_eq!(
        w.len(),
        1,
        "narrowing should not leak: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn narrowing_negation() {
    // !(x == null) is the same as x != null
    let w = warnings_for("fn f(x: ?String) {\n  if !(x == null) {\n    let y: String = x\n  }\n}");
    assert!(
        w.is_empty(),
        "negation should invert narrowing: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn narrowing_and_chain() {
    // x != null && y != null should narrow both
    let w = warnings_for(
        "fn f(x: ?String, y: ?Int) {\n  if x != null && y != null {\n    let a: String = x\n    let b: Int = y\n  }\n}",
    );
    assert!(
        w.is_empty(),
        "AND chain should narrow both vars: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn narrowing_is_some() {
    let w = warnings_for("fn f(x: ?String) {\n  if is_some(x) {\n    let y: String = x\n  }\n}");
    assert!(
        w.is_empty(),
        "is_some should narrow Option: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn narrowing_is_ok() {
    let w =
        warnings_for("fn f(x: Result<Int, String>) {\n  if is_ok(x) {\n    let y: Int = x\n  }\n}");
    assert!(
        w.is_empty(),
        "is_ok should narrow Result to Ok type: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn narrowing_early_return() {
    // if x == null { return } → x is non-null after the if
    let w = warnings_for("fn f(x: ?String) {\n  if x == null { return }\n  let y: String = x\n}");
    assert!(
        w.is_empty(),
        "early return should narrow after if: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn narrowing_unknown_not_narrowed() {
    // Unknown types should stay Unknown (no narrowing)
    let w = warnings_for("fn f(x) {\n  if x != null {\n    let y: String = x\n  }\n}");
    // x is Unknown (no annotation), narrowing Unknown stays Unknown,
    // and Unknown is compatible with anything — no warning
    assert!(w.is_empty());
}

#[test]
fn match_some_constructor_narrows() {
    // Match with Some(...) constructor pattern should narrow Option to inner type
    let w = warnings_for(
        "fn f(x: ?String) {\n  match x {\n    Some(v) => { let y: String = x }\n    _ => {}\n  }\n}",
    );
    assert!(
        w.is_empty(),
        "Some constructor should narrow Option: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn match_ok_constructor_narrows() {
    let w = warnings_for(
        "fn f(x: Result<Int, String>) {\n  match x {\n    Ok(v) => { let y: Int = x }\n    Err(e) => { let z: String = x }\n  }\n}",
    );
    assert!(
        w.is_empty(),
        "Ok/Err constructors should narrow Result: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

// ========== 8A.3: Exhaustive Match Checking ==========

#[test]
fn exhaustive_option_missing_none() {
    let w = warnings_for("fn f(x: ?String) {\n  match x {\n    Some(v) => { say v }\n  }\n}");
    assert_eq!(
        w.len(),
        1,
        "should warn about missing None: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(
        w[0].message.contains("None"),
        "warning should mention None: {}",
        w[0].message
    );
}

#[test]
fn exhaustive_option_complete() {
    let w = warnings_for(
        "fn f(x: ?String) {\n  match x {\n    Some(v) => { say v }\n    _ => { say \"none\" }\n  }\n}",
    );
    assert!(
        w.is_empty(),
        "complete Option match should not warn: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn exhaustive_result_missing_err() {
    let w =
        warnings_for("fn f(x: Result<Int, String>) {\n  match x {\n    Ok(v) => { say v }\n  }\n}");
    assert_eq!(
        w.len(),
        1,
        "should warn about missing Err: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(
        w[0].message.contains("Err"),
        "warning should mention Err: {}",
        w[0].message
    );
}

#[test]
fn exhaustive_result_complete() {
    let w = warnings_for(
        "fn f(x: Result<Int, String>) {\n  match x {\n    Ok(v) => { say v }\n    Err(e) => { say e }\n  }\n}",
    );
    assert!(
        w.is_empty(),
        "complete Result match should not warn: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn exhaustive_bool_missing_false() {
    let w = warnings_for("fn f(x: Bool) {\n  match x {\n    true => { say \"yes\" }\n  }\n}");
    assert_eq!(
        w.len(),
        1,
        "should warn about missing false: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(
        w[0].message.contains("false"),
        "warning should mention false: {}",
        w[0].message
    );
}

#[test]
fn exhaustive_wildcard_covers_all() {
    let w = warnings_for(
        "fn f(x: Result<Int, String>) {\n  match x {\n    _ => { say \"catch all\" }\n  }\n}",
    );
    assert!(
        w.is_empty(),
        "wildcard should make match exhaustive: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn exhaustive_binding_covers_all() {
    let w = warnings_for("fn f(x: ?String) {\n  match x {\n    v => { say v }\n  }\n}");
    assert!(
        w.is_empty(),
        "binding should make match exhaustive: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn exhaustive_unknown_no_warning() {
    // Unknown type — can't check exhaustiveness
    let w = warnings_for("fn f(x) {\n  match x {\n    1 => { say \"one\" }\n  }\n}");
    assert!(
        w.is_empty(),
        "unknown type should not trigger exhaustiveness: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn exhaustive_int_no_warning() {
    // Int — can't check exhaustiveness
    let w = warnings_for("fn f(x: Int) {\n  match x {\n    1 => { say \"one\" }\n  }\n}");
    assert!(
        w.is_empty(),
        "Int should not trigger exhaustiveness: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

// ========== 8B.2: Generic Type Resolution ==========

#[test]
fn generic_identity_resolves_return_type() {
    // fn identity<T>(x: T) -> T: called with Int → return Int
    let w = warnings_for("fn identity<T>(x: T) -> T { return x }\nlet y: String = identity(42)");
    assert_eq!(
        w.len(),
        1,
        "should warn: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(
        w[0].message.contains("Int"),
        "return type should resolve to Int: {}",
        w[0].message
    );
}

#[test]
fn generic_identity_no_false_positive() {
    let w = warnings_for("fn identity<T>(x: T) -> T { return x }\nlet y: Int = identity(42)");
    assert!(
        w.is_empty(),
        "should not warn: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn generic_two_params_resolves() {
    // fn first<T, U>(a: T, b: U) -> T: called with (Int, String) → return Int
    let w = warnings_for(
        "fn first<T, U>(a: T, b: U) -> T { return a }\nlet y: String = first(42, \"hi\")",
    );
    assert_eq!(w.len(), 1);
    assert!(
        w[0].message.contains("Int"),
        "T should resolve to Int: {}",
        w[0].message
    );
}

#[test]
fn generic_array_return_resolves() {
    // fn wrap<T>(x: T) -> [T]: called with Int → return [Int]
    let w = warnings_for("fn wrap<T>(x: T) -> [T] { return [x] }\nlet y: String = wrap(42)");
    assert_eq!(w.len(), 1);
    assert!(
        w[0].message.contains("[Int]"),
        "return should be [Int]: {}",
        w[0].message
    );
}

#[test]
fn non_generic_unchanged() {
    // Non-generic function behavior unchanged
    let w = warnings_for("fn add(a: Int, b: Int) -> Int { return a + b }\nlet y: Int = add(1, 2)");
    assert!(w.is_empty());
}

// ========== 8B.3: Generic Struct Definitions ==========

#[test]
fn generic_struct_stores_type_params() {
    // Generic struct should parse and type-check without errors
    let w = warnings_for("struct Pair<T> {\n  first: T\n  second: T\n}");
    assert!(
        w.is_empty(),
        "generic struct def should not warn: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn non_generic_struct_still_works() {
    let w = warnings_for("struct Point {\n  x: Int\n  y: Int\n}");
    assert!(w.is_empty());
}

#[test]
fn generic_struct_with_multiple_type_params() {
    let w = warnings_for("struct Either<L, R> {\n  left: L\n  right: R\n}");
    assert!(w.is_empty());
}

// ========== 8C.1: Union Types ==========

#[test]
fn union_type_accepts_member() {
    let w = warnings_for("type StringOrInt = String | Int\nlet x: StringOrInt = 42");
    assert!(
        w.is_empty(),
        "Int should be assignable to String|Int: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn union_type_accepts_other_member() {
    let w = warnings_for("type StringOrInt = String | Int\nlet x: StringOrInt = \"hello\"");
    assert!(
        w.is_empty(),
        "String should be assignable to String|Int: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn union_type_rejects_non_member() {
    let w = warnings_for("type StringOrInt = String | Int\nlet x: StringOrInt = true");
    assert_eq!(
        w.len(),
        1,
        "Bool should not be assignable to String|Int: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn union_type_nullable() {
    let w = warnings_for("type Nullable = String | Null\nlet x: Nullable = null");
    assert!(
        w.is_empty(),
        "null should be assignable to String|Null: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn single_variant_alias() {
    // type ID = Int — simple alias
    let w = warnings_for("type ID = Int\nlet x: ID = 42");
    assert!(
        w.is_empty(),
        "Int should be assignable to ID alias: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn single_variant_alias_rejects_wrong_type() {
    let w = warnings_for("type ID = Int\nlet x: ID = \"hello\"");
    assert_eq!(
        w.len(),
        1,
        "String should not be assignable to Int alias: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

// ========== 8C.3: Typed Collection Literals ==========

#[test]
fn typed_array_correct_elements() {
    let w = warnings_for("let xs: [Int] = [1, 2, 3]");
    assert!(
        w.is_empty(),
        "should not warn for correct array type: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn typed_array_wrong_elements() {
    let w = warnings_for("let xs: [Int] = [\"a\", \"b\"]");
    assert_eq!(
        w.len(),
        1,
        "should warn for wrong element type: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn typed_array_string_elements() {
    let w = warnings_for("let xs: [String] = [\"a\", \"b\"]");
    assert!(w.is_empty());
}

#[test]
fn typed_array_empty_compatible() {
    let w = warnings_for("let xs: [Int] = []");
    assert!(
        w.is_empty(),
        "empty array should be compatible with any typed array: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

// ========== Option<T> Enforcement ==========

#[test]
fn unwrap_returns_inner_type() {
    let w = warnings_for("let x: ?Int = Some(42)\nlet y: Int = unwrap(x)");
    assert!(
        w.is_empty(),
        "unwrap(?Int) should return Int: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn unwrap_mismatch_warns() {
    let w = warnings_for("let x: ?Int = Some(42)\nlet y: String = unwrap(x)");
    assert!(!w.is_empty(), "unwrap(?Int) assigned to String should warn");
}

#[test]
fn unwrap_or_returns_inner_type() {
    let w = warnings_for("let x: ?Int = Some(42)\nlet y: Int = unwrap_or(x, 0)");
    assert!(
        w.is_empty(),
        "unwrap_or(?Int, 0) should return Int: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn unwrap_or_fallback_type_mismatch_warns() {
    let w = warnings_for("let x: ?Int = Some(42)\nlet y = unwrap_or(x, \"hello\")");
    assert!(
        !w.is_empty(),
        "unwrap_or(?Int, String) should warn about incompatible fallback"
    );
    assert!(w[0].message.contains("incompatible"));
}

#[test]
fn option_in_arithmetic_warns() {
    let w = warnings_for("let x: ?Int = Some(42)\nlet y = x + 1");
    assert!(!w.is_empty(), "Option in arithmetic should warn");
    assert!(w[0].message.contains("Option"));
}

#[test]
fn option_in_equality_no_warn() {
    let w = warnings_for("let x: ?Int = Some(42)\nlet y = x == null");
    assert!(
        w.is_empty(),
        "Option in == should not warn: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn option_in_not_equal_no_warn() {
    let w = warnings_for("let x: ?Int = Some(42)\nlet y = x != None");
    assert!(
        w.is_empty(),
        "Option in != should not warn: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn reassign_none_to_non_option_warns() {
    let w = warnings_for("let mut x: Int = 5\nx = None");
    assert!(!w.is_empty(), "assigning None to Int variable should warn");
}

#[test]
fn reassign_none_to_option_ok() {
    let w = warnings_for("let mut x: ?Int = Some(5)\nx = None");
    assert!(
        w.is_empty(),
        "assigning None to ?Int should be fine: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn option_generic_syntax_parses() {
    let w = warnings_for("let x: Option<Int> = Some(42)");
    assert!(
        w.is_empty(),
        "Option<Int> syntax should work: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn narrowed_option_no_arithmetic_warn() {
    let w = warnings_for("fn f(x: ?Int) {\n  if is_some(x) {\n    let y = x + 1\n  }\n}");
    assert!(
        w.is_empty(),
        "narrowed Option should not warn in arithmetic: {:?}",
        w.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

// ========== Diagnostic codes, spans and fixes ==========

fn codes(source: &str) -> Vec<Code> {
    warnings_for(source).iter().map(|d| d.code).collect()
}

fn analysis(source: &str) -> Analysis {
    analyze(source, &CheckOptions::default()).expect("test source parses")
}

#[test]
fn unknown_name_has_precise_span_and_fix() {
    let w = warnings_for("let count = 1\nsay cuont + 1");
    assert_eq!(w.len(), 1, "{:?}", w);
    let d = &w[0];
    assert_eq!(d.code, Code::UnknownName);
    assert_eq!((d.span.start.line, d.span.start.col), (2, 5));
    assert_eq!((d.span.end.line, d.span.end.col), (2, 10));
    assert_eq!(d.help.as_deref(), Some("did you mean 'count'?"));
    assert_eq!(d.fixes.len(), 1);
    assert_eq!(d.fixes[0].replacement, "count");
    assert_eq!(d.fixes[0].span, d.span);
}

#[test]
fn unknown_names_inside_interpolation_are_located() {
    let w = warnings_for("let name = \"x\"\nsay \"hi {nmae}!\"");
    assert_eq!(w.len(), 1, "{:?}", w);
    assert_eq!(w[0].code, Code::UnknownName);
    assert_eq!((w[0].span.start.line, w[0].span.start.col), (2, 10));
}

#[test]
fn builtins_modules_and_shadowing_are_known() {
    assert!(codes("say len([1])\nsay math.sqrt(4.0)\nlet len = 3\nsay len").is_empty());
    assert!(codes("say None\nsay null\nsay Some(1)\nsay Ok(1)").is_empty());
}

#[test]
fn use_before_definition_only_in_the_same_frame() {
    assert_eq!(
        codes("say later\nlet later = 1"),
        vec![Code::UseBeforeDefinition]
    );
    // A function body runs when called, after the global exists.
    assert!(codes("fn f() { return later }\nlet later = 1\nsay f()").is_empty());
    // `let x = x + 1` reads the outer x.
    assert!(codes("let x = 1\nfn g() {\n  let x = x + 1\n  return x\n}").is_empty());
}

#[test]
fn invalid_operators_follow_the_shared_rules() {
    assert_eq!(codes("say \"a\" - 1"), vec![Code::InvalidOperator]);
    assert_eq!(codes("say [1] + [2]"), vec![Code::InvalidOperator]);
    assert_eq!(codes("say 1 + true"), vec![Code::InvalidOperator]);
    // String + anything concatenates; numbers mix; strings compare.
    assert!(codes("say \"a\" + 1\nsay 1 + 2.5\nsay \"a\" < \"b\"\nsay 7 / 2").is_empty());
}

#[test]
fn struct_fields_are_typed_and_checked() {
    let src = "struct Point { x: Int, y: Int }\n";
    assert_eq!(
        codes(&format!("{}let p = Point {{ x: 1, y: \"2\" }}", src)),
        vec![Code::TypeMismatch]
    );
    assert_eq!(
        codes(&format!("{}let p = Point {{ x: 1, y: 2, z: 3 }}", src)),
        vec![Code::UnknownField]
    );
    assert_eq!(
        codes(&format!("{}let p = Point {{ x: 1 }}", src)),
        vec![Code::MissingField]
    );
    let w = warnings_for(&format!("{}let p = Point {{ x: 1, y: 2 }}\nsay p.yy", src));
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].code, Code::UnknownField);
    assert_eq!(w[0].fixes[0].replacement, "y");
    assert_eq!(
        codes(&format!(
            "{}let p = Point {{ x: 1, y: 2 }}\nlet s: String = p.x",
            src
        )),
        vec![Code::TypeMismatch]
    );
}

#[test]
fn struct_methods_embedding_and_statics() {
    let src = "struct A { street: String }\n\
               struct B { name: String, has addr: A }\n\
               impl B {\n  fn new(n) { return B { name: n, addr: A { street: \"s\" } } }\n  fn label(it) { return it.name + it.street }\n}\n\
               let b = B.new(\"x\")\n\
               say b.street\nsay b.label()\nsay b.len()";
    assert!(codes(src).is_empty(), "{:?}", warnings_for(src));
    let bad = format!("{}\nsay b.lable()", src);
    let w = warnings_for(&bad);
    assert_eq!(w.len(), 1, "{:?}", w);
    assert_eq!(w[0].code, Code::UnknownField);
    assert_eq!(w[0].help.as_deref(), Some("did you mean 'label'?"));
    assert_eq!(
        codes(&format!("{}\nsay b.label(1)", src)),
        vec![Code::Arity]
    );
}

#[test]
fn generic_struct_fields_are_instantiated() {
    let src = "struct Box<T> { value: T }\nlet b = Box { value: 42 }\nlet s: String = b.value";
    assert_eq!(codes(src), vec![Code::TypeMismatch]);
    assert!(
        codes("struct Box<T> { value: T }\nlet b = Box { value: 42 }\nlet n: Int = b.value")
            .is_empty()
    );
}

#[test]
fn unknown_module_member_suggests() {
    let w = warnings_for("say math.sqr(4)");
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].code, Code::UnknownMember);
    assert_eq!(w[0].fixes[0].replacement, "sqrt");
    // A user variable named like a module is not the module.
    assert!(codes("let math = { sqr: 1 }\nsay math.sqr").is_empty());
}

#[test]
fn element_types_of_arrays() {
    assert_eq!(
        codes("let xs: [Int] = [1, \"a\"]"),
        vec![Code::TypeMismatch]
    );
    assert!(codes("let xs: [Float] = [1, 2.5]").is_empty());
    assert_eq!(
        codes("let xs: [[Int]] = [[\"a\"]]"),
        vec![Code::TypeMismatch]
    );
    // Unannotated arrays may be heterogeneous and grow with anything.
    assert!(codes("let mut xs = [1]\nxs = push(xs, \"a\")").is_empty());
}

#[test]
fn function_types_in_annotations() {
    assert!(codes("let f: fn(Int) -> Int = fn(x) { x * 2 }\nsay f(2)").is_empty());
    assert_eq!(
        codes("let f: fn(Int) -> Int = fn(x) { x * 2 }\nsay f(\"a\")"),
        vec![Code::ArgumentType]
    );
    assert_eq!(
        codes("fn apply(f: fn(Int) -> Int, x: Int) -> Int { return f(x) }\nsay apply(fn(s: String) { s }, 1)"),
        vec![Code::ArgumentType]
    );
    assert_eq!(codes("let f: fn(Int) -> Int = 5"), vec![Code::TypeMismatch]);
    // The lambda parameter gets the annotated type, so misuse is caught.
    assert_eq!(
        codes("let f: fn(String) -> Int = fn(s) { s - 1 }"),
        vec![Code::InvalidOperator]
    );
}

#[test]
fn lambda_parameters_are_inferred_from_higher_order_builtins() {
    assert_eq!(
        codes("let xs = [\"a\", \"b\"]\nlet ys = map(xs, fn(s) { s - 1 })"),
        vec![Code::InvalidOperator]
    );
    let a = analysis("let xs = [1, 2]\nlet ys = map(xs, fn(n) { n * 2.0 })\nlet zs = filter(xs, fn(n) { n > 1 })");
    let ty_of = |name: &str| {
        a.index
            .occurrences
            .iter()
            .enumerate()
            .find(|(_, o)| o.name == name && o.is_def())
            .and_then(|(id, _)| a.facts.def_types.get(&id))
            .map(|t| t.to_string())
    };
    assert_eq!(ty_of("ys").as_deref(), Some("[Float]"));
    assert_eq!(ty_of("zs").as_deref(), Some("[Int]"));
    assert_eq!(ty_of("n").as_deref(), Some("Int"));
}

#[test]
fn local_inference_feeds_hover_types() {
    let a = analysis(
        "let n = 1\nlet s = \"x\" + str(n)\nlet o = Some(n)\nfn add(a: Int, b: Int) { return a + b }\nlet r = add(1, 2)",
    );
    let types: std::collections::HashMap<String, String> = a
        .facts
        .def_types
        .iter()
        .map(|(id, t)| (a.index.occurrences[*id].name.clone(), t.to_string()))
        .collect();
    assert_eq!(types["n"], "Int");
    assert_eq!(types["s"], "String");
    assert_eq!(types["o"], "?Int");
    assert_eq!(types["add"], "fn(Int, Int) -> Int");
    assert_eq!(types["r"], "Int");
}

#[test]
fn mutable_variables_are_widened_across_assignments() {
    // x is assigned a String later (even inside a loop): no stale Int.
    let src = "let mut x = 0\nfor i in [1] {\n  say x - 1\n  x = \"done\"\n}";
    assert!(codes(src).is_empty(), "{:?}", warnings_for(src));
    let a = analysis("let mut n = 0\nn = n + 1");
    let ty = a.facts.def_types.values().next().map(|t| t.to_string());
    assert_eq!(ty.as_deref(), Some("Int"));
}

#[test]
fn missing_return_and_unreachable_code() {
    assert_eq!(
        codes("fn f() -> Int { let y = 1 }"),
        vec![Code::MissingReturn]
    );
    assert_eq!(
        codes("fn f(x: Int) -> Int {\n  if x > 0 { return 1 }\n}"),
        vec![Code::MissingReturn]
    );
    assert!(codes("fn f(x: Int) -> Int {\n  if x > 0 { return 1 }\n  return 0\n}").is_empty());
    assert!(codes("fn f(x: Int) -> Int {\n  if x > 0 { 1 } else { 2 }\n}").is_empty());
    assert!(codes("fn f(x: Int) -> Int { x * 2 }").is_empty());
    assert!(codes("fn f() -> ?Int { let y = 1 }").is_empty());
    assert!(codes("fn f() -> Int { loop { return 1 } }").is_empty());
    assert_eq!(
        codes("fn f() -> Int {\n  return 1\n  say \"x\"\n}"),
        vec![Code::Unreachable]
    );
    assert_eq!(
        codes("for i in [1] {\n  break\n  say i\n}"),
        vec![Code::Unreachable]
    );
    assert_eq!(codes("fn f() -> Int { \"s\" }"), vec![Code::ReturnType]);
}

#[test]
fn adt_constructors_matches_and_exhaustiveness() {
    let src = "type Shape = Circle(Float) | Rect(Float, Float)\n";
    assert!(codes(&format!(
        "{}let s = Circle(1.0)\nmatch s {{\n  Circle(r) => say r\n  Rect(w, h) => say w * h\n}}",
        src
    ))
    .is_empty());
    assert_eq!(
        codes(&format!(
            "{}let s = Circle(1.0)\nmatch s {{\n  Circle(r) => say r\n}}",
            src
        )),
        vec![Code::NonExhaustiveMatch]
    );
    assert_eq!(
        codes(&format!("{}let s = Rect(1.0)", src)),
        vec![Code::Arity]
    );
    assert_eq!(
        codes(&format!("{}let s = Circle(\"r\")", src)),
        vec![Code::ArgumentType]
    );
    // Pattern fields are typed from the variant.
    assert_eq!(
        codes(&format!(
            "{}let s = Circle(1.0)\nmatch s {{\n  Circle(r) => say r - \"x\"\n  _ => say 0\n}}",
            src
        )),
        vec![Code::InvalidOperator]
    );
    // Unit variants are values and patterns, not bindings.
    assert!(codes(
        "type Color = Red | Green\nlet c = Red\nmatch c {\n  Red => say 1\n  Green => say 2\n}"
    )
    .is_empty());
}

#[test]
fn immutable_assignment_is_reported() {
    assert_eq!(codes("let x = 1\nx = 2"), vec![Code::ImmutableAssign]);
    assert_eq!(
        codes("let p = { a: 1 }\np.a = 2"),
        vec![Code::ImmutableAssign]
    );
    assert!(codes("let mut x = 1\nx = 2\nfn f(a) { a = 3 }").is_empty());
}

#[test]
fn not_callable() {
    assert_eq!(codes("let c = 5\nc()"), vec![Code::NotCallable]);
}

#[test]
fn expected_failures_are_not_reported() {
    assert!(codes("assert_throws(fn() { say undefined_thing - 1 })").is_empty());
}

#[test]
fn interfaces_are_values() {
    let src = "interface Shape { fn area() -> Float }\nstruct Sq { s: Float }\nimpl Sq { fn area(it) -> Float { return it.s * it.s } }\nsay satisfies(Sq { s: 1.0 }, Shape)";
    assert!(codes(src).is_empty(), "{:?}", warnings_for(src));
}

#[test]
fn nested_generic_annotations_and_tuples_parse() {
    assert!(
        codes("let x: Option<Option<Int>> = None\nlet t: (Int, String) = (1, \"a\")").is_empty()
    );
    assert_eq!(
        codes("let t: (Int, String) = (1, 2)"),
        vec![Code::TypeMismatch]
    );
    assert_eq!(codes("let k: Strng = \"x\""), vec![Code::UnknownType]);
}

#[test]
fn strict_makes_every_diagnostic_an_error() {
    let e = errors_for("say \"a\" - 1\nsay nope");
    assert_eq!(e.len(), 2);
    assert!(e.iter().all(|d| d.is_error()));
}

#[test]
fn native_imports_bind_their_names() {
    let src = "import native \"libs/libmath\" as m\nsay m.add(1, 2)\nimport { mul } from native \"libs/libmath\"\nsay mul(2, 3)";
    assert!(codes(src).is_empty(), "{:?}", warnings_for(src));
    assert_eq!(
        codes("import { mul } from native \"libs/libmath\"\nsay mull(1)"),
        vec![Code::UnknownName]
    );
}
