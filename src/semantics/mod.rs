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

pub mod alloc;
pub mod edition;
pub mod errors;
pub mod hoist;
pub mod net;
pub mod ordered_set;
pub mod text;
pub mod types;

use edition::Edition;

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
    /// `Array + Array`: a new array with the left items, then the right
    /// ones (the engine builds it, within the collection size cap).
    ArrayConcat,
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

/// Whether `l <op> r` is `+` with exactly one String operand: implicit
/// concatenation of the displayed values in edition 2026, an error
/// ([`mixed_concat_error`], E0034) from edition 2027 on.
pub fn is_mixed_concat(op: BinaryOp, l: Operand<'_>, r: Operand<'_>) -> bool {
    op == BinaryOp::Add && (matches!(l, Operand::Str(_)) != matches!(r, Operand::Str(_)))
}

/// Edition 2027: `+` with a String and a non-String operand (E0034).
pub fn mixed_concat_error(left_type: &str, right_type: &str) -> String {
    format!(
        "cannot add {} and {}\n  hint: + does not convert to String implicitly (edition 2027); \
         convert with str(x), or interpolate: \"{{x}}\"",
        left_type, right_type
    )
}

/// Evaluate `l <op> r` for the shared operators under `edition` (the
/// edition the engine recorded when the program was prepared).
///
/// Rules (in priority order):
/// * numbers: int/int stays int (overflow promotes to float), any float
///   operand makes the operation float;
/// * `String op String`: `+` concatenates, ordering is lexicographic;
/// * `String + anything` / `anything + String`: edition 2026 concatenates
///   the displayed values; edition 2027 is an error ([`is_mixed_concat`]).
///   No other operator accepts a string next to a non-string;
/// * `Array + Array` concatenates the arrays (every edition);
/// * everything else (bools, null, collections, ...) is a type error.
pub fn binary(
    op: BinaryOp,
    l: Operand<'_>,
    r: Operand<'_>,
    edition: Edition,
) -> Result<Outcome, String> {
    use Operand::*;
    if edition >= Edition::E2027 && is_mixed_concat(op, l, r) {
        return Err(mixed_concat_error(l.type_name(), r.type_name()));
    }
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
        (Other("Array"), Other("Array")) if op == BinaryOp::Add => Ok(Outcome::ArrayConcat),
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
    // Ordering only: no edition-dependent rule applies.
    matches!(
        binary(cmp, subject, value, Edition::DEFAULT),
        Ok(Outcome::Bool(true))
    )
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
/// The hint spells out the valid range, so off-by-one mistakes are obvious.
pub fn index_out_of_bounds(index: i64, container: &str, len: usize) -> String {
    let hint = if len == 0 {
        format!("the {} is empty; check len() before indexing", container)
    } else {
        format!(
            "valid indices are 0 to {} (or -{} to -1 from the end)",
            len - 1,
            len
        )
    };
    format!(
        "index out of bounds: index {} on {} of length {}\n  hint: {}",
        index, container, len, hint
    )
}

/// Calling a value that is not a function (`5()`, `null()`).
pub fn not_callable(type_name: &str) -> String {
    format!(
        "cannot call a value of type {}\n  hint: only functions and lambdas can be called; check that the name is not shadowed by a variable",
        errors::user_type_name(type_name)
    )
}

/// Reading or writing `.field` on a value without fields. `type_name` is
/// the engine's type name of the receiver.
pub fn field_access(field: &str, type_name: &str) -> String {
    if type_name == "Null" {
        format!(
            "cannot access field '{}' on Null\n  hint: the value is null here; it may come from a function without a `return`, a failed lookup or a missing argument; check it with `if x != null` first",
            field
        )
    } else {
        format!("cannot access field '{}' on {}", field, type_name)
    }
}

/// Calling a method that a built-in type (String, Array, ...) lacks.
pub fn no_method(method: &str, type_name: &str) -> String {
    format!("no method '{}' on {}", method, type_name)
}

/// Reading a field an object does not have. `keys` are the object's
/// fields (internal `__` fields are ignored). The hint suggests a close
/// match, or lists the fields.
pub fn no_field<'a>(field: &str, keys: impl IntoIterator<Item = &'a str>) -> String {
    let mut keys: Vec<&str> = keys.into_iter().filter(|k| !k.starts_with("__")).collect();
    keys.sort_unstable();
    let hint = match errors::suggest_name(field, [keys.iter().copied()]) {
        Some(similar) => format!("did you mean '{}'?", similar),
        None if keys.is_empty() => "the object has no fields".to_string(),
        None => {
            let more = if keys.len() > 8 { ", ..." } else { "" };
            let shown: Vec<&str> = keys.iter().copied().take(8).collect();
            format!("available fields: {}{}", shown.join(", "), more)
        }
    };
    format!("no field '{}' on object\n  hint: {}", field, hint)
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
///
/// A builtin removed by an edition (`say` in 2027) is only ever undefined
/// in an edition that removed it, so its hint names the replacement.
pub fn undefined_variable(name: &str, similar: Option<&str>) -> String {
    if let Some(hint) = crate::builtins_registry::removal_hint(name) {
        return format!("undefined variable: '{}'\n  hint: {}", name, hint);
    }
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
///
/// `it` (or `self`) is usually a method's receiver, which is immutable
/// unless the method declares `mut it`; the error says so in a hint.
pub fn immutable_reassign(name: &str) -> String {
    let message = format!(
        "cannot reassign immutable variable '{}' (use 'let mut' to make it mutable)",
        name
    );
    if is_receiver_name(name) {
        format!(
            "{}\n  hint: a method's receiver is read-only; declare it `mut {}` (`fn name(mut {}, ...)`) to update the value the method is called on, or work on a copy (`let mut copy = {}`)",
            message, name, name, name
        )
    } else {
        message
    }
}

/// Whether a method's first parameter with this name is its receiver
/// (`fn area(it)`; `self` is accepted too).
pub fn is_receiver_name(name: &str) -> bool {
    name == "it" || name == "self"
}

/// Parse error for `mut` on a parameter other than a method's receiver.
pub const MUT_PARAM_NOT_RECEIVER: &str =
    "only a method's receiver can be declared `mut`: write `fn name(mut it, ...)` inside an `impl` block (other parameters are already mutable copies)";

/// A method declared `mut it` called on a value that is not a variable or
/// a field/index chain rooted at one (`make().inc()`): there is no place to
/// write the updated receiver back to, so the call is refused instead of
/// silently losing the update. Classified E0006.
pub fn temporary_receiver(method: &str) -> String {
    format!(
        "cannot update a temporary value: method '{}' updates its receiver (`mut it`)\n  hint: store the value in a `let mut` variable and call the method on that variable",
        method
    )
}

/// The collection methods that update their receiver when it is a mutable
/// variable or a field/index place rooted at one (`xs.push(v)`,
/// `g[k].push(v)`, `o.items.pop()`, `s.add(v)`, `s.remove(v)`).
pub fn is_mutating_method(method: &str, arg_count: usize) -> bool {
    matches!(
        (method, arg_count),
        ("push", 1) | ("pop", 0) | ("add", 1) | ("remove", 1)
    )
}

/// One of the [`is_mutating_method`] methods called on a frozen value.
/// `kind` names the frozen value (`"array"`, `"set"`, `"map"`; anything
/// else reads "value"): `cannot remove from a frozen map`. Both engines
/// raise every such error through this helper.
pub fn frozen_method(method: &str, kind: &str) -> String {
    let kind = match kind {
        "array" | "set" | "map" => kind,
        _ => "value",
    };
    match method {
        "push" | "add" => format!("cannot {} to a frozen {}", method, kind),
        "pop" | "remove" => format!("cannot {} from a frozen {}", method, kind),
        _ => format!("cannot mutate a frozen {}", kind),
    }
}

pub fn check_failed(displayed_value: &str) -> String {
    let shown = if displayed_value.is_empty() {
        "\"\" (empty)"
    } else {
        displayed_value
    };
    format!("check failed: {} did not pass validation", shown)
}

/// A `match` (statement or expression) whose arms all failed to match.
pub const NON_EXHAUSTIVE_MATCH: &str =
    "non-exhaustive match: no arm matched the value\n  hint: add a `_ => ...` arm to handle every other value";

/// `expr?` on a value that is not a Result.
pub const TRY_REQUIRES_RESULT: &str =
    "`?` expects a Result value (Ok(...) or Err(...))\n  hint: wrap the value in Ok(...), or use `?` only on calls that return a Result";

/// An `Err` propagated with `?` out of the program's top level, where no
/// caller can handle it. `shown` is the displayed error payload.
pub fn unhandled_error(shown: &str) -> String {
    format!("unhandled error: {}", shown)
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

/// Arity rule for `receiver.method(args)` dispatching to a user-defined
/// instance method (`impl` / `give` blocks), whose first parameter receives
/// `receiver`. `params`/`required` describe the declared parameters
/// *including* the receiver; `got` counts only the explicit arguments, and
/// so does the message: `fn area(self)` called as `c.area(5)` reports
/// "method area expects 0 arguments, got 1".
pub fn check_method_arity(
    method: &str,
    params: usize,
    required: usize,
    got: usize,
) -> Result<(), String> {
    check_call_arity(
        method,
        params.saturating_sub(1),
        required.saturating_sub(1),
        got,
    )
    .map_err(|e| match e.strip_prefix("fn ") {
        Some(rest) => format!("method {}", rest),
        None => e,
    })
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
    "exec", "time", "url", "toml", "npc", "ws", "jwt", "mysql", "os", "path", "__types",
];

/// Result of a built-in string method (see [`string_method`]), in a form
/// each engine converts to its own value representation.
#[derive(Debug, Clone, PartialEq)]
pub enum StrMethodValue {
    Str(String),
    Int(i64),
    Bool(bool),
    Strs(Vec<String>),
    Ints(Vec<i64>),
    Null,
}

/// The built-in string methods (`s.upper()`, `s.chars()`, ...), shared by
/// both engines. `index` is the integer argument of `char_at`.
pub const STRING_METHODS: &[&str] = &[
    "upper",
    "lower",
    "trim",
    "trim_start",
    "trim_end",
    "len",
    "chars",
    "bytes",
    "words",
    "is_empty",
    "is_numeric",
    "is_alpha",
    "is_alphanumeric",
    "reverse",
    "char_at",
    "encode_uri",
    "decode_uri",
];

/// Evaluate built-in string method `name` on `s`. `None` when `name` is
/// not one of [`STRING_METHODS`].
pub fn string_method(
    s: &str,
    name: &str,
    index: Option<i64>,
) -> Option<Result<StrMethodValue, String>> {
    use StrMethodValue as V;
    let strs = |it: &mut dyn Iterator<Item = String>| V::Strs(it.collect());
    Some(Ok(match name {
        "upper" => V::Str(s.to_uppercase()),
        "lower" => V::Str(s.to_lowercase()),
        "trim" => V::Str(s.trim().to_string()),
        "trim_start" => V::Str(s.trim_start().to_string()),
        "trim_end" => V::Str(s.trim_end().to_string()),
        "len" => V::Int(s.chars().count() as i64),
        "is_empty" => V::Bool(s.is_empty()),
        "is_numeric" => V::Bool(
            s.chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == '-'),
        ),
        "is_alpha" => V::Bool(!s.is_empty() && s.chars().all(|c| c.is_alphabetic())),
        "is_alphanumeric" => V::Bool(!s.is_empty() && s.chars().all(|c| c.is_alphanumeric())),
        "reverse" => V::Str(s.chars().rev().collect()),
        "chars" => strs(&mut s.chars().map(|c| c.to_string())),
        "bytes" => V::Ints(s.bytes().map(|b| b as i64).collect()),
        "words" => strs(&mut s.split_whitespace().map(str::to_string)),
        "char_at" => match index {
            Some(i) if i >= 0 => s
                .chars()
                .nth(i as usize)
                .map_or(V::Null, |c| V::Str(c.to_string())),
            Some(_) => V::Null,
            None => return Some(Err("char_at() requires an integer index".to_string())),
        },
        "encode_uri" => V::Str(
            s.chars()
                .map(|c| match c {
                    'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
                    _ => format!("%{:02X}", c as u32),
                })
                .collect(),
        ),
        "decode_uri" => {
            let mut result = String::new();
            let mut chars = s.chars();
            while let Some(c) = chars.next() {
                if c == '%' {
                    let hex: String = chars.by_ref().take(2).collect();
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        result.push(byte as char);
                    } else {
                        result.push('%');
                        result.push_str(&hex);
                    }
                } else if c == '+' {
                    result.push(' ');
                } else {
                    result.push(c);
                }
            }
            V::Str(result)
        }
        _ => return None,
    }))
}

/// Error for `contains()` arguments it cannot search (both engines).
pub const CONTAINS_USAGE: &str =
    "contains() requires (string, substring), (array, value), (object, key), or (map, key)";

/// Most elements `range()` materializes. Larger ranges are a catchable
/// error instead of a capacity-overflow panic or an out-of-memory abort.
pub const MAX_RANGE_LEN: u64 = 100_000_000;

/// Number of elements of `range(start, end)` (0 when `end <= start`), or
/// the error both engines report when it exceeds [`MAX_RANGE_LEN`].
pub fn range_len(start: i64, end: i64) -> Result<usize, String> {
    let len = (end as i128 - start as i128).max(0) as u128;
    if len > MAX_RANGE_LEN as u128 {
        return Err(format!(
            "range({}, {}): result too large ({} elements; the limit is {})",
            start, end, len, MAX_RANGE_LEN
        ));
    }
    Ok(len as usize)
}

/// Validate a user-supplied element/iteration count (`sample(xs, n)`,
/// `slay(f, n)`): negative counts and counts above [`MAX_RANGE_LEN`] are
/// errors rather than a capacity-overflow panic.
pub fn checked_count(builtin: &str, n: i64) -> Result<usize, String> {
    if n < 0 {
        return Err(format!(
            "{}() count must be non-negative, got {}",
            builtin, n
        ));
    }
    if n as u64 > MAX_RANGE_LEN {
        return Err(format!(
            "{}() count {} exceeds the limit of {}",
            builtin, n, MAX_RANGE_LEN
        ));
    }
    Ok(n as usize)
}

/// Largest string `repeat_str` / `pad_start` / `pad_end` build (1 GiB).
pub const MAX_REPEAT_BYTES: usize = 1 << 30;

/// Check that repeating a `byte_len`-byte string `n` times stays within
/// [`MAX_REPEAT_BYTES`] (a huge count used to abort with a capacity
/// overflow).
pub fn check_repeat(builtin: &str, byte_len: usize, n: usize) -> Result<(), String> {
    match byte_len.checked_mul(n) {
        Some(total) if total <= MAX_REPEAT_BYTES => Ok(()),
        _ => Err(format!(
            "{}(): result too large (more than {} bytes)",
            builtin, MAX_REPEAT_BYTES
        )),
    }
}

/// Seconds as a `Duration` for `wait`/`time.sleep`: negative and NaN mean
/// zero, values too large for a `Duration` (including infinity) saturate to
/// "forever" instead of panicking.
pub fn seconds_f64(secs: f64) -> std::time::Duration {
    std::time::Duration::try_from_secs_f64(secs.max(0.0)).unwrap_or(std::time::Duration::MAX)
}

/// The duration units `wait`, `timeout` and `schedule every` accept after
/// their number, with how many seconds one of them is. Singular and plural
/// spellings and `ms` all work; the parser normalises them to the first
/// spelling of each row (the `unit` a `schedule` block stores).
pub const DURATION_UNITS: &[(&[&str], f64)] = &[
    (&["milliseconds", "millisecond", "ms"], 0.001),
    (&["seconds", "second"], 1.0),
    (&["minutes", "minute"], 60.0),
    (&["hours", "hour"], 3600.0),
];

/// The canonical unit name (`"milliseconds"`, `"seconds"`, `"minutes"`,
/// `"hours"`) a duration word spells, if it is one.
pub fn duration_unit(word: &str) -> Option<&'static str> {
    DURATION_UNITS
        .iter()
        .find(|(names, _)| names.contains(&word))
        .map(|(names, _)| names[0])
}

/// Seconds in one `unit` (a canonical name from [`duration_unit`]; anything
/// else counts as seconds).
pub fn duration_unit_seconds(unit: &str) -> f64 {
    DURATION_UNITS
        .iter()
        .find(|(names, _)| names.contains(&unit))
        .map_or(1.0, |(_, secs)| *secs)
}

/// `schedule every <n> <unit>`: the interval between runs. `n` may be a
/// fraction (`every 0.5 seconds`, `every 250 ms`); it must be positive, and
/// absurd values saturate instead of panicking.
pub fn schedule_interval(n: f64, unit: &str) -> Result<std::time::Duration, String> {
    if n.is_nan() || n <= 0.0 {
        return Err("schedule interval must be a positive number".to_string());
    }
    // A run more often than every millisecond would only spin.
    Ok(seconds_f64((n * duration_unit_seconds(unit)).max(0.001)))
}

/// The time limit of `timeout <n> seconds { }` in seconds: negative and NaN
/// mean zero (the block fails at once).
pub fn timeout_seconds(n: f64) -> f64 {
    if n.is_nan() {
        0.0
    } else {
        n.max(0.0)
    }
}

/// Deadline `secs` seconds after `now` for a `timeout` block (fractions are
/// exact to the nanosecond). Saturates at a century, which outlives any
/// program, rather than overflowing `Instant` (a panic) for absurd durations.
pub fn timeout_deadline(now: crate::clock::Instant, secs: f64) -> crate::clock::Instant {
    const CENTURY_SECS: u64 = 100 * 365 * 24 * 60 * 60;
    let capped = seconds_f64(secs).min(std::time::Duration::from_secs(CENTURY_SECS));
    now.checked_add(capped).unwrap_or(now)
}

/// The error a `timeout` block raises when its limit passes (E0031); the
/// limit is printed as a Forge number (`0.5`, `2`).
pub fn timeout_exceeded(secs: f64) -> String {
    format!("timeout: operation exceeded {} second limit", secs)
}

pub fn import_missing_name(path: &str, name: &str) -> String {
    format!(
        "import '{}' does not export '{}'\n  hint: a module exports its top-level `fn`, `let`, `struct`, `type`, variant and `interface` names and the names it imports",
        path, name
    )
}

/// `import { name } from "<module>"` for a built-in module: `Ok` when the
/// module has that member (both engines then bind `name` to
/// `module.name`), otherwise the same E0019 error as a missing file export.
pub fn check_builtin_module_import(module: &str, name: &str) -> Result<(), String> {
    let has_member = crate::builtins_registry::modules()
        .iter()
        .find(|m| m.name == module)
        .is_some_and(|m| match (m.create)() {
            crate::interpreter::Value::Object(members) => members.contains_key(name),
            _ => false,
        });
    if has_member {
        Ok(())
    } else {
        Err(format!(
            "import '{}' does not export '{}'\n  hint: '{}' is a built-in module; check the member name (`{}.<name>`)",
            module, name, module, module
        ))
    }
}

/// `import { name as alias } from "path"`: `as` renames functions and
/// variables. A struct, `type`, variant or interface keeps its name — its
/// values, methods and patterns refer to it by name — so renaming one is an
/// E0019 error on both engines.
pub fn check_import_alias(
    path: &str,
    program: &crate::parser::ast::Program,
    name: &str,
    alias: &str,
) -> Result<(), String> {
    use crate::parser::ast::Stmt;
    let kind = program.statements.iter().find_map(|s| match &s.stmt {
        Stmt::StructDef { name: n, .. } if n == name => Some("a struct"),
        Stmt::InterfaceDef { name: n, .. } if n == name => Some("an interface"),
        Stmt::TypeDef { name: n, .. } if n == name => Some("a type"),
        Stmt::TypeDef { variants, .. } if variants.iter().any(|v| v.name == name) => {
            Some("a variant")
        }
        _ => None,
    });
    match kind {
        None => Ok(()),
        Some(kind) => Err(format!(
            "cannot import '{}' as '{}' from '{}': '{}' is {}, and only functions and variables can be renamed\n  hint: import it under its own name: import {{ {} }} from \"{}\"",
            name, alias, path, name, kind, name, path
        )),
    }
}

/// The key of a module namespace object (`import "path" as m`) that holds
/// the path as written. Internal (`__`) keys are not members.
pub const NAMESPACE_MODULE_KEY: &str = "__module__";

/// One member of a module namespace (`import "path" as m`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamespaceMember {
    /// A top-level function, variable, variant, `type` name, interface or
    /// a name the module imported (a re-export): `m.name` is the value the
    /// module's top level left in it (read when the import runs, like
    /// `import { name }`).
    Value(String),
    /// A struct: `m.Name` is the struct marker
    /// (`{__kind__: "struct", name: "Name"}`, the VM's value of a struct
    /// name) and `m.Name { ... }` builds an instance.
    Struct(String),
}

impl NamespaceMember {
    pub fn name(&self) -> &str {
        match self {
            NamespaceMember::Value(n) | NamespaceMember::Struct(n) => n,
        }
    }
}

/// Whether `type name = ...` binds `name` as a value: its type description
/// `{__kind__: "type", name, variants}` (the same object as the type's
/// metadata), as a struct or interface name is a value. Not when a variant
/// has the type's own name (`type Wrap = Wrap(Int)`): the name is that
/// variant's constructor.
pub fn type_name_is_value(name: &str, variants: &[crate::parser::ast::Variant]) -> bool {
    !variants.iter().any(|v| v.name == name)
}

/// The names a module's top-level imports bind in it, in order: what it
/// re-exports (`import { f } from "m.fg"` binds `f`, `as` binds the alias;
/// `import "m.fg" as m` binds `m`; native imports bind their names). A
/// bare `import "m.fg"` is not listed: its names depend on the other file.
pub fn import_bindings(program: &crate::parser::ast::Program) -> Vec<String> {
    use crate::parser::ast::{NativeBinding, Stmt};
    let mut out = Vec::new();
    for s in &program.statements {
        match &s.stmt {
            Stmt::Import {
                names: Some(names), ..
            } => out.extend(names.iter().map(|n| n.binding().to_string())),
            Stmt::ImportNamespace { name, .. } => out.push(name.clone()),
            Stmt::ImportNative { binding, .. } => match binding {
                NativeBinding::Namespace(name) => out.push(name.clone()),
                NativeBinding::Names(names) => out.extend(names.iter().cloned()),
            },
            _ => {}
        }
    }
    out
}

/// The members of `import "path" as m`, in definition order: the module's
/// top-level `fn`s, `let`s (destructuring included), `type` names and
/// their variants, interfaces, structs and the names its imports bind
/// (re-exports, as `import { name }` can import them). Both engines build
/// the namespace object from this list, so it prints and compares the
/// same on both.
pub fn namespace_members(program: &crate::parser::ast::Program) -> Vec<NamespaceMember> {
    use crate::parser::ast::{DestructurePattern, NativeBinding, Stmt};
    let mut out: Vec<NamespaceMember> = Vec::new();
    let mut push = |m: NamespaceMember| {
        if !out.iter().any(|o| o.name() == m.name()) {
            out.push(m);
        }
    };
    for s in &program.statements {
        match &s.stmt {
            Stmt::FnDef { name, .. } | Stmt::Let { name, .. } => {
                push(NamespaceMember::Value(name.clone()))
            }
            Stmt::Destructure { pattern, .. } => {
                let names: Vec<&String> = match pattern {
                    DestructurePattern::Object(fields) => {
                        fields.iter().map(|(_, name)| name).collect()
                    }
                    DestructurePattern::Array { items, rest } => {
                        items.iter().chain(rest.iter()).collect()
                    }
                    DestructurePattern::Tuple(names) => names.iter().collect(),
                };
                for name in names {
                    push(NamespaceMember::Value(name.clone()));
                }
            }
            Stmt::TypeDef { name, variants } => {
                if type_name_is_value(name, variants) {
                    push(NamespaceMember::Value(name.clone()));
                }
                for v in variants {
                    push(NamespaceMember::Value(v.name.clone()));
                }
            }
            Stmt::InterfaceDef { name, .. } => push(NamespaceMember::Value(name.clone())),
            Stmt::StructDef { name, .. } => push(NamespaceMember::Struct(name.clone())),
            Stmt::Import {
                names: Some(names), ..
            } => {
                for n in names {
                    push(NamespaceMember::Value(n.binding().to_string()));
                }
            }
            Stmt::ImportNamespace { name, .. } => push(NamespaceMember::Value(name.clone())),
            Stmt::ImportNative { binding, .. } => match binding {
                NativeBinding::Namespace(name) => push(NamespaceMember::Value(name.clone())),
                NativeBinding::Names(names) => {
                    for n in names {
                        push(NamespaceMember::Value(n.clone()));
                    }
                }
            },
            _ => {}
        }
    }
    out
}

/// `import "math" as m`: built-in modules are always in scope under their
/// own name; a namespace import of one is an E0019 error on both engines.
pub fn namespace_of_builtin_module(path: &str, name: &str) -> String {
    format!(
        "cannot import '{}' as '{}': '{}' is a built-in module and is always available as `{}`\n  hint: call its functions directly ({}.<name>), or import some of them: import {{ name }} from \"{}\"",
        path, name, path, path, path, path
    )
}

/// Reading `m.field` where `m` is a module namespace without that member.
/// `keys` are the namespace's keys (internal `__` keys are ignored).
pub fn no_module_member<'a>(
    module: &str,
    field: &str,
    keys: impl IntoIterator<Item = &'a str>,
) -> String {
    let mut keys: Vec<&str> = keys.into_iter().filter(|k| !k.starts_with("__")).collect();
    keys.sort_unstable();
    let hint = match errors::suggest_name(field, [keys.iter().copied()]) {
        Some(similar) => format!("did you mean '{}'?", similar),
        None if keys.is_empty() => format!("module '{}' defines no top-level names", module),
        None => {
            let more = if keys.len() > 8 { ", ..." } else { "" };
            let shown: Vec<&str> = keys.iter().copied().take(8).collect();
            format!("its members are: {}{}", shown.join(", "), more)
        }
    };
    format!(
        "module '{}' has no member '{}'\n  hint: {}",
        module, field, hint
    )
}

/// The error for reading a missing field of an object: the namespace form
/// ([`no_module_member`]) when the object is a module namespace (its
/// `__module__` key is `module`), [`no_field`] otherwise.
pub fn missing_member<'a>(
    module: Option<&str>,
    field: &str,
    keys: impl IntoIterator<Item = &'a str>,
) -> String {
    match module {
        Some(module) => no_module_member(module, field, keys),
        None => no_field(field, keys),
    }
}

/// `ns.Name { ... }` where `ns.Name` is not a struct of a module namespace.
pub fn not_a_namespace_struct(namespace: &str, name: &str) -> String {
    format!(
        "'{}.{}' is not a struct\n  hint: `{}.{} {{ ... }}` builds a struct defined in the module imported with `import \"...\" as {}`",
        namespace, name, namespace, name, namespace
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

    fn parse(source: &str) -> crate::parser::ast::Program {
        let tokens = crate::lexer::Lexer::new(source).tokenize().expect("lexes");
        crate::parser::Parser::new(tokens)
            .parse_program()
            .expect("parses")
    }

    #[test]
    fn namespace_members_list_every_export_in_order() {
        let program = parse(
            "import { a, b as c } from \"x.fg\"\nimport \"y.fg\" as y\nimport \"z.fg\"\n\
             fn f() { return 1 }\nlet {lo, hi: top} = {lo: 1, hi: 2}\n\
             struct P { x: Int }\ninterface I { fn n() -> Int }\n\
             type T = A(Int) | B\ntype W = W(Int)\n",
        );
        let members = namespace_members(&program);
        let names: Vec<&str> = members.iter().map(NamespaceMember::name).collect();
        assert_eq!(
            names,
            ["a", "c", "y", "f", "lo", "top", "P", "I", "T", "A", "B", "W"]
        );
        assert!(members.contains(&NamespaceMember::Struct("P".into())));
        assert_eq!(import_bindings(&program), ["a", "c", "y"]);
    }

    #[test]
    fn builtin_module_imports_check_members() {
        assert_eq!(check_builtin_module_import("math", "sqrt"), Ok(()));
        let err = check_builtin_module_import("math", "sqrtx").unwrap_err();
        assert!(err.starts_with("import 'math' does not export 'sqrtx'"));
        assert_eq!(crate::semantics::errors::classify(&err).code, "E0019");
        // `exec` is accepted by `import "exec"` but has no members.
        assert!(check_builtin_module_import("exec", "run_command").is_err());
    }

    #[test]
    fn range_len_is_bounded() {
        assert_eq!(range_len(0, 3), Ok(3));
        assert_eq!(range_len(5, 2), Ok(0));
        assert_eq!(range_len(-2, 2), Ok(4));
        assert!(range_len(0, MAX_RANGE_LEN as i64).is_ok());
        assert!(range_len(0, MAX_RANGE_LEN as i64 + 1).is_err());
        let e = range_len(i64::MIN, i64::MAX).unwrap_err();
        assert!(e.contains("limit"), "{e}");
    }

    #[test]
    fn durations_saturate_instead_of_panicking() {
        use std::time::{Duration, Instant};
        assert_eq!(seconds_f64(1.5), Duration::from_millis(1500));
        assert_eq!(seconds_f64(-3.0), Duration::ZERO);
        assert_eq!(seconds_f64(f64::NAN), Duration::ZERO);
        assert_eq!(seconds_f64(f64::INFINITY), Duration::MAX);
        assert_eq!(seconds_f64(1e300), Duration::MAX);
        assert_eq!(
            schedule_interval(2.0, "minutes"),
            Ok(Duration::from_secs(120))
        );
        assert_eq!(
            schedule_interval(2.0, "hours"),
            Ok(Duration::from_secs(7200))
        );
        assert_eq!(schedule_interval(f64::MAX, "hours"), Ok(Duration::MAX));
        assert_eq!(
            schedule_interval(7.0, "seconds"),
            Ok(Duration::from_secs(7))
        );
        assert_eq!(
            schedule_interval(0.5, "seconds"),
            Ok(Duration::from_millis(500))
        );
        assert_eq!(
            schedule_interval(250.0, "milliseconds"),
            Ok(Duration::from_millis(250))
        );
        assert!(schedule_interval(0.0, "seconds").is_err());
        assert!(schedule_interval(-1.0, "seconds").is_err());
        assert!(schedule_interval(f64::NAN, "seconds").is_err());
        let now = Instant::now();
        assert_eq!(timeout_deadline(now, 2.0), now + Duration::from_secs(2));
        assert_eq!(
            timeout_deadline(now, 0.25),
            now + Duration::from_millis(250)
        );
        assert!(timeout_deadline(now, f64::MAX) > now + Duration::from_secs(1 << 30));
        assert!(timeout_deadline(now, f64::INFINITY) > now + Duration::from_secs(1 << 30));
        assert_eq!(timeout_seconds(f64::NAN), 0.0);
        assert_eq!(timeout_seconds(-2.0), 0.0);
        assert_eq!(
            timeout_exceeded(0.5),
            "timeout: operation exceeded 0.5 second limit"
        );
        assert_eq!(
            timeout_exceeded(2.0),
            "timeout: operation exceeded 2 second limit"
        );
    }

    #[test]
    fn duration_units() {
        for (word, unit) in [
            ("ms", "milliseconds"),
            ("millisecond", "milliseconds"),
            ("milliseconds", "milliseconds"),
            ("second", "seconds"),
            ("seconds", "seconds"),
            ("minute", "minutes"),
            ("hours", "hours"),
        ] {
            assert_eq!(duration_unit(word), Some(unit), "{word}");
        }
        assert_eq!(duration_unit("days"), None);
        assert_eq!(duration_unit_seconds("milliseconds"), 0.001);
        assert_eq!(duration_unit_seconds("hours"), 3600.0);
    }

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
                ok(Outcome::ArrayConcat),
            ),
            (
                BinaryOp::Sub,
                Other("Array"),
                Other("Array"),
                Err("cannot apply Sub to Array and Array".into()),
            ),
            (
                BinaryOp::Add,
                Other("Array"),
                Other("Tuple"),
                Err("cannot apply Add to Array and Tuple".into()),
            ),
            (
                BinaryOp::Add,
                Other("Array"),
                Int(1),
                Err("cannot apply Add to Array and Int".into()),
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
            assert_eq!(
                binary(op, l, r, Edition::E2026),
                expected,
                "{:?} {:?} {:?}",
                l,
                op,
                r
            );
            // Edition 2027 differs only for `+` with exactly one String.
            if !is_mixed_concat(op, l, r) {
                assert_eq!(
                    binary(op, l, r, Edition::E2027),
                    expected,
                    "2027 {:?} {:?} {:?}",
                    l,
                    op,
                    r
                );
            }
        }
    }

    #[test]
    fn edition_2027_rejects_mixed_concat() {
        let e27 = Edition::E2027;
        // Exactly one String operand: an error naming both types.
        for (l, r, types) in [
            (Str("a"), Int(1), ("String", "Int")),
            (Int(1), Str("a"), ("Int", "String")),
            (Str("a"), Float(1.5), ("String", "Float")),
            (Str("a"), Bool, ("String", "Bool")),
            (Str("a"), Null, ("String", "Null")),
            (Null, Str("a"), ("Null", "String")),
            (Str("a"), Other("Array"), ("String", "Array")),
            (Other("Object"), Str("a"), ("Object", "String")),
        ] {
            assert!(is_mixed_concat(BinaryOp::Add, l, r));
            let err = binary(BinaryOp::Add, l, r, e27).unwrap_err();
            assert_eq!(err, mixed_concat_error(types.0, types.1));
            assert!(
                err.starts_with(&format!("cannot add {} and {}\n  hint: ", types.0, types.1)),
                "{}",
                err
            );
            assert!(err.contains("str(x)") && err.contains("\"{x}\""), "{}", err);
            // Edition 2026 keeps the implicit conversion.
            assert_eq!(
                binary(BinaryOp::Add, l, r, Edition::E2026),
                Ok(Outcome::Concat)
            );
        }
        // String + String still concatenates; other operators are unchanged.
        assert_eq!(
            binary(BinaryOp::Add, Str("a"), Str("b"), e27),
            Ok(Outcome::Concat)
        );
        assert!(!is_mixed_concat(BinaryOp::Add, Str("a"), Str("b")));
        assert!(!is_mixed_concat(BinaryOp::Mul, Str("a"), Int(3)));
        assert!(!is_mixed_concat(BinaryOp::Add, Int(1), Int(2)));
        assert_eq!(
            binary(BinaryOp::Mul, Str("a"), Int(3), e27),
            Err("invalid operator".into())
        );
        assert_eq!(
            binary(BinaryOp::Add, Int(1), Int(2), e27),
            Ok(Outcome::Int(3))
        );
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
            "index out of bounds: index 10 on array of length 2\n  hint: valid indices are 0 to 1 (or -2 to -1 from the end)"
        );
        assert!(index_out_of_bounds(0, "array", 0).contains("the array is empty"));
        assert_eq!(
            no_field("nmae", ["name", "__type__"]),
            "no field 'nmae' on object\n  hint: did you mean 'name'?"
        );
        assert_eq!(
            no_field("zzz", ["b", "a"]),
            "no field 'zzz' on object\n  hint: available fields: a, b"
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
    fn method_arity_excludes_the_receiver() {
        assert_eq!(check_method_arity("area", 1, 1, 0), Ok(()));
        assert_eq!(
            check_method_arity("area", 1, 1, 1).unwrap_err(),
            "method area expects 0 arguments, got 1"
        );
        assert_eq!(
            check_method_arity("scale", 2, 2, 0).unwrap_err(),
            "method scale expects 1 argument, got 0"
        );
        assert_eq!(check_method_arity("opt", 3, 2, 1), Ok(()));
        assert_eq!(check_method_arity("opt", 3, 2, 2), Ok(()));
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
