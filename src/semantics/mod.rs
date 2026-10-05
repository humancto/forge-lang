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
pub mod errors;
pub mod types;

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

/// `schedule every <n> <unit>` interval in seconds, saturating instead of
/// overflowing for absurd `n`.
pub fn schedule_interval_secs(n: u64, unit: &str) -> u64 {
    match unit {
        "minutes" => n.saturating_mul(60),
        "hours" => n.saturating_mul(3600),
        _ => n, // "seconds" or default
    }
}

/// Deadline `secs` seconds after `now` for a `timeout` block. Saturates at
/// a century, which outlives any program, rather than overflowing
/// `Instant` (a panic) for absurd durations.
pub fn timeout_deadline(now: crate::clock::Instant, secs: u64) -> crate::clock::Instant {
    const CENTURY_SECS: u64 = 100 * 365 * 24 * 60 * 60;
    let capped = std::time::Duration::from_secs(secs.min(CENTURY_SECS));
    now.checked_add(capped).unwrap_or(now)
}

pub fn import_missing_name(path: &str, name: &str) -> String {
    format!(
        "import '{}' does not export '{}'\n  hint: only top-level `fn`, `let`, `type` and `struct` definitions can be imported",
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
        assert_eq!(schedule_interval_secs(2, "minutes"), 120);
        assert_eq!(schedule_interval_secs(2, "hours"), 7200);
        assert_eq!(schedule_interval_secs(u64::MAX, "hours"), u64::MAX);
        assert_eq!(schedule_interval_secs(7, "seconds"), 7);
        let now = Instant::now();
        assert_eq!(timeout_deadline(now, 2), now + Duration::from_secs(2));
        assert!(timeout_deadline(now, u64::MAX) > now + Duration::from_secs(1 << 30));
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
