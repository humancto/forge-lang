//! Shared language semantics for the tree-walking interpreter and the
//! bytecode VM.
//!
//! The two engines use different value representations (`interpreter::Value`
//! is a plain enum, the VM uses NaN-boxed values with GC handles), so they
//! cannot share evaluation code directly. Instead, each engine projects its
//! operands into the small borrowed views defined here ([`Operand`],
//! [`Shape`]) and calls the same rule functions. Every user-visible decision
//! that both engines must agree on — arithmetic and comparison rules, string
//! concatenation, truthiness, negative indexing, and the text of the
//! resulting error messages — lives in this module, so a fix lands in both
//! engines at once instead of drifting.
//!
//! When adding a rule here, add a row to the table tests at the bottom and
//! make both engines call it. Never re-implement one of these rules inline in
//! `interpreter/` or `vm/`.

/// Borrowed view of a binary-operator operand.
#[derive(Clone, Copy, Debug)]
pub enum Operand<'a> {
    Int(i64),
    Float(f64),
    Str(&'a str),
    Bool,
    Null,
    /// Any other value. Carries the user-facing type name
    /// (`"Array"`, `"Tuple"`, `"Set"`, `"Map"`, `"Option"`, `"Object"`, ...).
    Other(&'a str),
}

impl Operand<'_> {
    pub fn type_name(&self) -> &str {
        match self {
            Operand::Int(_) => "Int",
            Operand::Float(_) => "Float",
            Operand::Str(_) => "String",
            Operand::Bool => "Bool",
            Operand::Null => "Null",
            Operand::Other(name) => name,
        }
    }
}

/// Arithmetic and ordering operators whose semantics are shared.
/// Equality (`==` / `!=`) is structural and stays engine-specific.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
    Gt,
    LtEq,
    GtEq,
}

impl BinaryOp {
    fn is_comparison(self) -> bool {
        matches!(
            self,
            BinaryOp::Lt | BinaryOp::Gt | BinaryOp::LtEq | BinaryOp::GtEq
        )
    }
}

/// Result of a shared binary operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Int(i64),
    Float(f64),
    Bool(bool),
    /// String concatenation: the engine renders `display(left) + display(right)`
    /// with its own value formatter.
    Concat,
}

fn invalid(op: BinaryOp, l: &Operand<'_>, r: &Operand<'_>) -> String {
    format!(
        "cannot apply {:?} to {} and {}",
        op,
        l.type_name(),
        r.type_name()
    )
}

fn compare_partial<T: PartialOrd>(op: BinaryOp, a: T, b: T) -> bool {
    match op {
        BinaryOp::Lt => a < b,
        BinaryOp::Gt => a > b,
        BinaryOp::LtEq => a <= b,
        BinaryOp::GtEq => a >= b,
        _ => false,
    }
}

pub const DIVISION_BY_ZERO: &str =
    "division by zero\n  hint: check that the divisor is not zero before dividing";
pub const MODULO_BY_ZERO: &str =
    "modulo by zero\n  hint: check that the divisor is not zero before using %";

fn int_op(op: BinaryOp, a: i64, b: i64) -> Result<Outcome, String> {
    let fallback = |f: f64| Ok(Outcome::Float(f));
    match op {
        BinaryOp::Add => a
            .checked_add(b)
            .map_or_else(|| fallback(a as f64 + b as f64), |v| Ok(Outcome::Int(v))),
        BinaryOp::Sub => a
            .checked_sub(b)
            .map_or_else(|| fallback(a as f64 - b as f64), |v| Ok(Outcome::Int(v))),
        BinaryOp::Mul => a
            .checked_mul(b)
            .map_or_else(|| fallback(a as f64 * b as f64), |v| Ok(Outcome::Int(v))),
        BinaryOp::Div => {
            if b == 0 {
                return Err(DIVISION_BY_ZERO.to_string());
            }
            // i64::MIN / -1 overflows; promote like the other operators.
            a.checked_div(b)
                .map_or_else(|| fallback(a as f64 / b as f64), |v| Ok(Outcome::Int(v)))
        }
        BinaryOp::Mod => {
            if b == 0 {
                return Err(MODULO_BY_ZERO.to_string());
            }
            Ok(Outcome::Int(a.checked_rem(b).unwrap_or(0)))
        }
        _ => Ok(Outcome::Bool(compare_partial(op, a, b))),
    }
}

fn float_op(op: BinaryOp, a: f64, b: f64) -> Outcome {
    match op {
        BinaryOp::Add => Outcome::Float(a + b),
        BinaryOp::Sub => Outcome::Float(a - b),
        BinaryOp::Mul => Outcome::Float(a * b),
        BinaryOp::Div => Outcome::Float(a / b),
        BinaryOp::Mod => Outcome::Float(a % b),
        _ => Outcome::Bool(compare_partial(op, a, b)),
    }
}

/// Evaluate `l <op> r` for the shared operators.
///
/// Rules (in priority order):
/// * numbers: int/int stays int (overflow promotes to float), any float
///   operand makes the operation float;
/// * `String op String`: `+` concatenates, ordering is lexicographic;
/// * `String + anything` / `anything + String` concatenates the displayed
///   values; no other operator accepts a string next to a non-string;
/// * everything else (bools, null, collections, ...) is a type error.
pub fn binary(op: BinaryOp, l: Operand<'_>, r: Operand<'_>) -> Result<Outcome, String> {
    use Operand::*;
    match (l, r) {
        (Int(a), Int(b)) => int_op(op, a, b),
        (Float(a), Float(b)) => Ok(float_op(op, a, b)),
        (Int(a), Float(b)) => Ok(float_op(op, a as f64, b)),
        (Float(a), Int(b)) => Ok(float_op(op, a, b as f64)),
        (Str(a), Str(b)) => match op {
            BinaryOp::Add => Ok(Outcome::Concat),
            _ if op.is_comparison() => Ok(Outcome::Bool(compare_partial(op, a, b))),
            _ => Err("invalid operator for String".to_string()),
        },
        (Bool, Bool) => Err("invalid operator for Bool".to_string()),
        (Str(_), _) | (_, Str(_)) => match op {
            BinaryOp::Add => Ok(Outcome::Concat),
            _ => Err("invalid operator".to_string()),
        },
        (Null, _) | (_, Null) => Err("cannot perform arithmetic on null".to_string()),
        (Other(a), Other(b)) if a == b => Err(match a {
            "Option" => "invalid operator for Option".to_string(),
            "Tuple" => "tuples only support == and != operators".to_string(),
            "Set" => "sets only support == and != operators".to_string(),
            "Map" => "maps only support == and != operators".to_string(),
            _ => invalid(op, &l, &r),
        }),
        _ => Err(invalid(op, &l, &r)),
    }
}

/// Arm test of a `when` guard (`when x { < 13 -> ..., == 5 -> ... }`).
/// `==` / `!=` compare the displayed values; ordering arms only match
/// numbers (anything else simply does not match).
pub fn when_matches(
    op: &str,
    subject: Operand<'_>,
    value: Operand<'_>,
    displays_equal: bool,
) -> bool {
    let cmp = match op {
        "==" => return displays_equal,
        "!=" => return !displays_equal,
        "<" => BinaryOp::Lt,
        ">" => BinaryOp::Gt,
        "<=" => BinaryOp::LtEq,
        ">=" => BinaryOp::GtEq,
        _ => return false,
    };
    let numeric = |o: &Operand<'_>| matches!(o, Operand::Int(_) | Operand::Float(_));
    if !numeric(&subject) || !numeric(&value) {
        return false;
    }
    matches!(binary(cmp, subject, value), Ok(Outcome::Bool(true)))
}

/// Coarse shape of a value for truthiness.
#[derive(Clone, Copy, Debug)]
pub enum Shape {
    Bool(bool),
    Int(i64),
    Float(f64),
    Null,
    /// String / Array / Tuple / Set / Map / Object with its length.
    Sized(usize),
    ResultOk,
    ResultErr,
    OptionSome,
    OptionNone,
    /// Functions, tasks, channels, ...
    Other,
}

/// Truthiness rule shared by `if`, `while`, `!`, `&&`, `||`.
pub fn is_truthy(shape: Shape) -> bool {
    match shape {
        Shape::Bool(b) => b,
        Shape::Int(n) => n != 0,
        Shape::Float(f) => f != 0.0,
        Shape::Null => false,
        Shape::Sized(len) => len != 0,
        Shape::ResultOk | Shape::OptionSome => true,
        Shape::ResultErr | Shape::OptionNone => false,
        Shape::Other => true,
    }
}

/// Resolve a (possibly negative, Python-style) index against a sequence of
/// length `len`. Returns `None` when out of bounds.
pub fn normalize_index(index: i64, len: usize) -> Option<usize> {
    let len = len as i64;
    let actual = if index < 0 { len + index } else { index };
    if actual < 0 || actual >= len {
        None
    } else {
        Some(actual as usize)
    }
}

/// `container` is the lowercase container kind (`"array"`, `"tuple"`).
pub fn index_out_of_bounds(index: i64, container: &str, len: usize) -> String {
    format!(
        "index out of bounds: index {} on {} of length {}",
        index, container, len
    )
}

pub fn missing_key(key: &str) -> String {
    format!("key '{}' not found", key)
}

/// Error for `container[index]` when the pair of types is not indexable.
pub fn invalid_index(container_type: &str, index_type: &str) -> String {
    match container_type {
        "Set" => "cannot index a set; sets are unordered — use .has() or iteration".to_string(),
        _ => format!(
            "invalid index operation: cannot index {} with {}",
            container_type, index_type
        ),
    }
}

pub fn invalid_index_assign(container_type: &str) -> String {
    match container_type {
        "Tuple" => "cannot mutate a tuple".to_string(),
        "Set" => "cannot index-assign a set; use .add() and .remove()".to_string(),
        other => format!("cannot index-assign a value of type {}", other),
    }
}

/// Message raised when a program executes `yield` / `emit`. Generators are
/// not implemented in either engine; failing loudly beats silently dropping
/// the value.
pub const YIELD_UNSUPPORTED: &str =
    "yield/emit is not supported yet: generator functions are not implemented\n  hint: collect values into an array and return it instead";

/// Reading a name that is bound nowhere. `similar` is an optional
/// did-you-mean suggestion.
pub fn undefined_variable(name: &str, similar: Option<&str>) -> String {
    match similar {
        Some(similar) => format!(
            "undefined variable: '{}'\n  hint: did you mean '{}'?",
            name, similar
        ),
        None => format!(
            "undefined variable: '{}'\n  hint: make sure the variable is defined before use",
            name
        ),
    }
}

/// Assigning to a binding declared without `mut`. Raised at run time by both
/// engines (so `try`/`catch` can observe it).
pub fn immutable_reassign(name: &str) -> String {
    format!(
        "cannot reassign immutable variable '{}' (use 'let mut' to make it mutable)",
        name
    )
}

pub fn check_failed(displayed_value: &str) -> String {
    format!("check failed: {} did not pass validation", displayed_value)
}

/// `return` executed outside any function (top level of a program, inside
/// a block expression). The program ends with that value on both engines.
pub const RETURN_OUTSIDE_FUNCTION: &str = "return outside of a function";

/// `check value between lo and hi`: inclusive range check. Ints and floats
/// may be mixed (compared as floats); strings compare lexicographically.
/// Any other combination fails the check.
pub fn between(value: Operand<'_>, lo: Operand<'_>, hi: Operand<'_>) -> bool {
    fn num(o: &Operand<'_>) -> Option<f64> {
        match *o {
            Operand::Int(n) => Some(n as f64),
            Operand::Float(f) => Some(f),
            _ => None,
        }
    }
    match (value, lo, hi) {
        (Operand::Int(v), Operand::Int(l), Operand::Int(h)) => l <= v && v <= h,
        (Operand::Str(v), Operand::Str(l), Operand::Str(h)) => l <= v && v <= h,
        (v, l, h) => match (num(&v), num(&l), num(&h)) {
            (Some(v), Some(l), Some(h)) => l <= v && v <= h,
            _ => false,
        },
    }
}

/// Parameter count rule for calling a user-defined function or lambda
/// directly (`f(a, b)`).
///
/// * `params` - declared parameters;
/// * `required` - parameters a caller must pass: everything up to and
///   including the last parameter without a default value;
/// * `got` - arguments passed.
///
/// Passing fewer than `required` or more than `params` arguments is an
/// error. Functions invoked *by builtins* as callbacks (`map`, `filter`,
/// `sort`, `reduce`, ...) are not subject to this rule: missing parameters
/// are bound to `null` (or their default) and extra arguments are ignored,
/// so `map(xs, fn(x) { ... })` and `map(xs, fn(x, i) { ... })` both work.
pub fn check_call_arity(
    fn_name: &str,
    params: usize,
    required: usize,
    got: usize,
) -> Result<(), String> {
    if got >= required && got <= params {
        return Ok(());
    }
    let name = if fn_name.is_empty() || fn_name == "<lambda>" {
        "fn".to_string()
    } else {
        format!("fn {}", fn_name)
    };
    let plural = |n: usize| if n == 1 { "argument" } else { "arguments" };
    let expected = if required == params {
        format!("{} {}", params, plural(params))
    } else if got > params {
        format!("at most {} {}", params, plural(params))
    } else {
        format!("at least {} {}", required, plural(required))
    };
    Err(format!("{} expects {}, got {}", name, expected, got))
}

/// Implicit-return rule for the *last* statement of a function, lambda or
/// `spawn` body: an expression statement yields its value (handled by each
/// engine directly), and so do these block statements — the value of the
/// branch or arm that ran (null when none ran, e.g. `if` without `else`).
/// Every other statement (loops, `let`, assignments, ...) yields null.
///
/// So `fn sign(x) { if x < 0 { -1 } else { 1 } }` returns -1 or 1. Side
/// effects are unchanged: the taken branch runs exactly as before, only
/// its final expression's value is kept.
pub fn is_value_tail(stmt: &crate::parser::ast::Stmt) -> bool {
    use crate::parser::ast::Stmt;
    matches!(
        stmt,
        Stmt::If { .. } | Stmt::When { .. } | Stmt::Match { .. } | Stmt::SafeBlock { .. }
    )
}

/// Number of leading parameters a caller must pass, given which parameters
/// have default values (see [`check_call_arity`]).
pub fn required_params<I>(has_default: I) -> usize
where
    I: DoubleEndedIterator<Item = bool> + ExactSizeIterator,
{
    let total = has_default.len();
    let trailing_defaults = has_default.rev().take_while(|d| *d).count();
    total - trailing_defaults
}

/// Built-in module names that `import "<name>"` accepts as a no-op because
/// the module is always in scope.
pub const BUILTIN_MODULES: &[&str] = &[
    "math", "fs", "io", "crypto", "db", "pg", "env", "json", "regex", "log", "term", "http", "csv",
    "exec", "time", "url", "toml", "npc", "ws", "jwt", "mysql", "os", "path",
];

pub fn import_missing_name(path: &str, name: &str) -> String {
    format!(
        "import '{}' does not export '{}'\n  hint: only top-level `fn`, `let`, `type` and `struct` definitions can be imported",
        path, name
    )
}

pub fn import_not_found(path: &str) -> String {
    format!(
        "cannot import '{}': file not found (checked relative to the importing file, {0}, {0}.fg, forge_modules/{0}/main.fg)",
        path
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use Operand::*;

    #[test]
    fn between_table() {
        assert!(between(Int(5), Int(1), Int(10)));
        assert!(between(Int(1), Int(1), Int(10)));
        assert!(!between(Int(11), Int(1), Int(10)));
        assert!(between(Int(5), Float(1.0), Float(10.0)));
        assert!(between(Float(2.5), Int(1), Int(10)));
        assert!(between(Str("b"), Str("a"), Str("c")));
        assert!(!between(Str("b"), Int(1), Int(10)));
        assert!(!between(Null, Int(1), Int(10)));
    }

    #[test]
    fn call_arity_table() {
        assert_eq!(check_call_arity("add", 2, 2, 2), Ok(()));
        assert_eq!(
            check_call_arity("add", 2, 2, 1).unwrap_err(),
            "fn add expects 2 arguments, got 1"
        );
        assert_eq!(
            check_call_arity("add", 2, 2, 3).unwrap_err(),
            "fn add expects 2 arguments, got 3"
        );
        assert_eq!(check_call_arity("g", 2, 1, 1), Ok(()));
        assert_eq!(
            check_call_arity("g", 2, 1, 0).unwrap_err(),
            "fn g expects at least 1 argument, got 0"
        );
        assert_eq!(
            check_call_arity("g", 2, 1, 3).unwrap_err(),
            "fn g expects at most 2 arguments, got 3"
        );
        assert_eq!(
            check_call_arity("<lambda>", 1, 1, 2).unwrap_err(),
            "fn expects 1 argument, got 2"
        );
        assert_eq!(required_params([false, true].into_iter()), 1);
        assert_eq!(required_params([true, false].into_iter()), 2);
        assert_eq!(required_params([true, true].into_iter()), 0);
        assert_eq!(required_params(std::iter::empty::<bool>()), 0);
    }

    #[test]
    fn binary_table() {
        let ok = |o| Ok::<Outcome, String>(o);
        let cases: Vec<(BinaryOp, Operand, Operand, Result<Outcome, String>)> = vec![
            (BinaryOp::Add, Int(2), Int(3), ok(Outcome::Int(5))),
            (
                BinaryOp::Add,
                Int(i64::MAX),
                Int(1),
                ok(Outcome::Float(i64::MAX as f64 + 1.0)),
            ),
            (BinaryOp::Div, Int(7), Int(2), ok(Outcome::Int(3))),
            (
                BinaryOp::Div,
                Int(i64::MIN),
                Int(-1),
                ok(Outcome::Float(-(i64::MIN as f64))),
            ),
            (BinaryOp::Mod, Int(i64::MIN), Int(-1), ok(Outcome::Int(0))),
            (BinaryOp::Div, Int(1), Int(0), Err(DIVISION_BY_ZERO.into())),
            (BinaryOp::Mod, Int(1), Int(0), Err(MODULO_BY_ZERO.into())),
            (
                BinaryOp::Div,
                Float(1.0),
                Int(0),
                ok(Outcome::Float(f64::INFINITY)),
            ),
            (BinaryOp::Lt, Int(1), Float(1.5), ok(Outcome::Bool(true))),
            (
                BinaryOp::Lt,
                Str("abc"),
                Str("abd"),
                ok(Outcome::Bool(true)),
            ),
            (BinaryOp::GtEq, Str("b"), Str("a"), ok(Outcome::Bool(true))),
            (BinaryOp::Add, Str("a"), Str("b"), ok(Outcome::Concat)),
            (BinaryOp::Add, Str("a"), Int(1), ok(Outcome::Concat)),
            (BinaryOp::Add, Int(1), Str("a"), ok(Outcome::Concat)),
            (BinaryOp::Add, Null, Str("a"), ok(Outcome::Concat)),
            (BinaryOp::Add, Other("Array"), Str("a"), ok(Outcome::Concat)),
            (
                BinaryOp::Sub,
                Str("a"),
                Str("b"),
                Err("invalid operator for String".into()),
            ),
            (
                BinaryOp::Mul,
                Str("a"),
                Int(3),
                Err("invalid operator".into()),
            ),
            (
                BinaryOp::Lt,
                Str("a"),
                Int(3),
                Err("invalid operator".into()),
            ),
            (
                BinaryOp::Lt,
                Bool,
                Bool,
                Err("invalid operator for Bool".into()),
            ),
            (
                BinaryOp::Add,
                Null,
                Int(1),
                Err("cannot perform arithmetic on null".into()),
            ),
            (
                BinaryOp::Add,
                Other("Array"),
                Other("Array"),
                Err("cannot apply Add to Array and Array".into()),
            ),
            (
                BinaryOp::Lt,
                Other("Tuple"),
                Other("Tuple"),
                Err("tuples only support == and != operators".into()),
            ),
            (
                BinaryOp::Add,
                Bool,
                Int(1),
                Err("cannot apply Add to Bool and Int".into()),
            ),
        ];
        for (op, l, r, expected) in cases {
            assert_eq!(binary(op, l, r), expected, "{:?} {:?} {:?}", l, op, r);
        }
    }

    #[test]
    fn when_table() {
        assert!(when_matches("<", Int(5), Int(13), false));
        assert!(when_matches(">=", Float(2.5), Int(2), false));
        assert!(!when_matches("<", Str("a"), Str("b"), false));
        assert!(when_matches("==", Int(1), Str("1"), true));
        assert!(when_matches("!=", Int(1), Int(2), false));
    }

    #[test]
    fn index_table() {
        assert_eq!(normalize_index(0, 3), Some(0));
        assert_eq!(normalize_index(-1, 3), Some(2));
        assert_eq!(normalize_index(-3, 3), Some(0));
        assert_eq!(normalize_index(-4, 3), None);
        assert_eq!(normalize_index(3, 3), None);
        assert_eq!(normalize_index(0, 0), None);
        assert_eq!(
            index_out_of_bounds(10, "array", 2),
            "index out of bounds: index 10 on array of length 2"
        );
    }

    #[test]
    fn truthiness_table() {
        assert!(!is_truthy(Shape::Int(0)));
        assert!(is_truthy(Shape::Int(-1)));
        assert!(!is_truthy(Shape::Float(0.0)));
        assert!(!is_truthy(Shape::Null));
        assert!(!is_truthy(Shape::Sized(0)));
        assert!(is_truthy(Shape::Sized(1)));
        assert!(!is_truthy(Shape::ResultErr));
        assert!(!is_truthy(Shape::OptionNone));
        assert!(is_truthy(Shape::Other));
    }

    #[test]
    fn value_tails_are_block_statements() {
        use crate::parser::ast::{Expr, Stmt};
        assert!(is_value_tail(&Stmt::If {
            condition: Expr::Bool(true),
            then_body: vec![],
            else_body: None,
        }));
        assert!(is_value_tail(&Stmt::SafeBlock { body: vec![] }));
        assert!(!is_value_tail(&Stmt::Expression(Expr::Int(1))));
        assert!(!is_value_tail(&Stmt::Break));
    }
}
