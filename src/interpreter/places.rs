//! In-place reads and updates of variables.
//!
//! Interpreter values are owned (`Value::String` is a `String`, arrays are
//! `Vec`s, objects are `IndexMap`s) and have value semantics, so
//! [`Environment::get`] deep-copies whatever it returns. Evaluating a bare
//! identifier therefore costs O(size of the value). The helpers here let
//! hot paths borrow the binding instead, via [`Environment::with_value`]
//! and friends, so that:
//!
//! * `a.push(x)`, `push(a, x)`, `a.pop()`, `pop(a)`, `s.add(x)` and
//!   `s.remove(x)` on a mutable variable mutate it in place, and the
//!   updated collection they evaluate to is only copied when the caller
//!   actually uses it (`want_result`);
//! * `x = x + e` (and `x += e`) appends to a string in place, and
//!   `a = push(a, x)` pushes in place;
//! * `a[i] = v` and `o.k = v` update one slot instead of copying the
//!   container twice;
//! * `g[k].push(x)`, `o.items.pop()`, `o.seen.add(x)` (the method form on
//!   any field/index chain rooted at a variable) update the place in place
//!   (`mutate_nested_place`; the VM compiles the same rules in
//!   `try_compile_place_method`);
//! * `s.has(x)`, `m.get(k)`, `m.has(k)`, `m.len()`, `len(v)`,
//!   `contains(v, x)`, `get(o, k[, d])` and `has_key(o, k)` read the
//!   variable without copying it.
//!
//! # Evaluation order
//!
//! The fast paths evaluate operands *before* borrowing the variable (the
//! borrow holds the scope lock, so no Forge code may run during it).
//!
//! * For the mutating collection methods this is the defined semantics:
//!   the method applies to the variable's value after its arguments have
//!   been evaluated, as in reference-semantics languages. (Previously an
//!   argument that itself modified the collection had its change silently
//!   overwritten.)
//! * Everywhere else the reordering must be unobservable, so the fast path
//!   is only taken when the operand is effect-free ([`is_effect_free`]):
//!   it cannot run user code or assign variables, hence cannot change the
//!   variable being read.
//!
//! [`is_effect_free`]: Interpreter::is_effect_free
use super::*;

/// Builtins that only compute a value from their arguments: they never
/// call back into Forge code, print, or touch the environment. A call to
/// one of these (when the name still resolves to the builtin) is
/// effect-free if its arguments are.
const PURE_BUILTINS: &[&str] = &[
    "str",
    "int",
    "float",
    "len",
    "type",
    "typeof",
    "contains",
    "split",
    "join",
    "replace",
    "starts_with",
    "ends_with",
    "substring",
    "index_of",
    "pad_start",
    "pad_end",
    "repeat_str",
];

fn peel_frozen(value: &Value) -> &Value {
    match value {
        Value::Frozen(inner) => inner.as_ref(),
        other => other,
    }
}

/// `len(v)`, shared by the builtin and the in-place fast path.
pub(super) fn len_of(value: Option<&Value>) -> Result<Value, RuntimeError> {
    match value {
        Some(Value::String(s)) => Ok(Value::Int(s.chars().count() as i64)),
        Some(Value::Array(a) | Value::Tuple(a)) => Ok(Value::Int(a.len() as i64)),
        Some(Value::Set(a)) => Ok(Value::Int(a.len() as i64)),
        Some(Value::Object(o)) => Ok(Value::Int(o.len() as i64)),
        Some(Value::Map(m)) => Ok(Value::Int(m.len() as i64)),
        _ => Err(RuntimeError::new(
            "len() requires string, array, tuple, set, map, or object",
        )),
    }
}

/// `contains(haystack, needle)`, shared by the builtin and the fast path.
pub(super) fn contains_of(
    haystack: Option<&Value>,
    needle: Option<&Value>,
) -> Result<Value, RuntimeError> {
    match (haystack, needle) {
        (Some(Value::String(s)), Some(Value::String(sub))) => {
            Ok(Value::Bool(s.contains(sub.as_str())))
        }
        (Some(Value::Set(arr)), Some(val)) => Ok(Value::Bool(arr.has_value(val))),
        (Some(Value::Array(arr) | Value::Tuple(arr)), Some(val)) => {
            let needle = format!("{}", val);
            Ok(Value::Bool(arr.iter().any(|v| format!("{}", v) == needle)))
        }
        (Some(Value::Object(map)), Some(Value::String(key))) => {
            Ok(Value::Bool(map.contains_key(key)))
        }
        (Some(Value::Map(pairs)), Some(key)) => Ok(Value::Bool(pairs.has_key_value(key))),
        _ => Err(RuntimeError::new(crate::semantics::CONTAINS_USAGE)),
    }
}

/// `has_key(obj, key)`, shared by the builtin and the in-place fast path.
pub(super) fn has_key_of(obj: Option<&Value>, key: Option<&Value>) -> Result<Value, RuntimeError> {
    match (obj, key) {
        (Some(Value::Object(map)), Some(Value::String(key))) => {
            Ok(Value::Bool(map.contains_key(key)))
        }
        _ => Err(RuntimeError::new(
            &crate::semantics::builtin_args::bad_args("has_key"),
        )),
    }
}

/// `get(obj, key[, default])` / `get(array, index[, default])`, shared by
/// the builtin and the in-place fast path. Only the result is copied.
pub(super) fn get_of(
    obj: Option<&Value>,
    key: Option<&Value>,
    default: Option<&Value>,
) -> Result<Value, RuntimeError> {
    let default = || default.cloned().unwrap_or(Value::Null);
    match (obj, key) {
        (Some(obj @ Value::Object(map)), Some(Value::String(key))) => {
            // An exact key wins over the dot-path reading
            // (`get(counts, ".txt", 0)`).
            if !key.contains('.') || map.contains_key(key.as_str()) {
                return Ok(map.get(key.as_str()).cloned().unwrap_or_else(default));
            }
            let mut current = obj;
            for part in key.split('.') {
                current = match current {
                    Value::Object(m) => match m.get(part) {
                        Some(v) => v,
                        None => return Ok(default()),
                    },
                    Value::Array(arr) => {
                        match part.parse::<usize>().ok().and_then(|i| arr.get(i)) {
                            Some(v) => v,
                            None => return Ok(default()),
                        }
                    }
                    _ => return Ok(default()),
                };
            }
            Ok(current.clone())
        }
        (Some(Value::Array(arr)), Some(Value::Int(idx))) => {
            Ok(arr.get(*idx as usize).cloned().unwrap_or_else(default))
        }
        _ => Err(RuntimeError::new(
            &crate::semantics::builtin_args::bad_args("get"),
        )),
    }
}

/// Builtins that only read their first argument and copy at most a part of
/// it into their result. A call `name(place, args...)` whose first argument
/// is a variable (or a field/index chain of one) borrows that value under
/// the scope lock instead of copying the whole of it for the call
/// ([`Interpreter::try_read_in_place`]); `len`, `contains`, `get` and
/// `has_key` have their own forms there. Each one is implemented once, in
/// [`read_only_builtin`], for the builtin and the borrowed form alike.
pub(super) const BORROWING_BUILTINS: &[&str] = &[
    "slice",
    "first",
    "last",
    "join",
    "index_of",
    "last_index_of",
    "substring",
    "starts_with",
    "ends_with",
];

/// The array methods that are these builtins (`xs.slice(1, 3)`, ...):
/// `known_methods` dispatch calls the global of the same name with the
/// receiver first.
const BORROWING_ARRAY_METHODS: &[&str] = &[
    "slice", "first", "last", "join", "index_of", "len", "contains",
];

/// `slice(v, start?, end?)` on an array or string; negative bounds count
/// from the end.
fn slice_of(
    value: Option<&Value>,
    start: Option<&Value>,
    end: Option<&Value>,
) -> Result<Value, RuntimeError> {
    fn bounds(len: usize, start: Option<&Value>, end: Option<&Value>) -> (usize, usize) {
        let start = match start {
            Some(Value::Int(s)) if *s < 0 => (len as i64 + s).max(0) as usize,
            Some(Value::Int(s)) => *s as usize,
            _ => 0,
        };
        let end = match end {
            Some(Value::Int(e)) if *e < 0 => (len as i64 + e).max(0) as usize,
            Some(Value::Int(e)) => (*e as usize).min(len),
            _ => len,
        };
        (start, end)
    }
    match value {
        Some(Value::Array(arr)) => {
            let (start, end) = bounds(arr.len(), start, end);
            if start >= end || start >= arr.len() {
                return Ok(Value::Array(vec![]));
            }
            Ok(Value::Array(arr[start..end].to_vec()))
        }
        Some(Value::String(s)) => {
            let chars: Vec<char> = s.chars().collect();
            let (start, end) = bounds(chars.len(), start, end);
            if start >= end || start >= chars.len() {
                return Ok(Value::String(String::new()));
            }
            Ok(Value::String(chars[start..end].iter().collect()))
        }
        _ => Err(RuntimeError::new(
            "slice() requires an array or string as first argument",
        )),
    }
}

/// `substring(s, start, end?)`.
fn substring_of(
    value: Option<&Value>,
    start: Option<&Value>,
    end: Option<&Value>,
) -> Result<Value, RuntimeError> {
    match value {
        Some(Value::String(s)) => {
            let start = match start {
                Some(Value::Int(n)) => *n as usize,
                _ => {
                    return Err(RuntimeError::new(
                        "substring() requires (string, start, end?)",
                    ))
                }
            };
            let chars: Vec<char> = s.chars().collect();
            let end = match end {
                Some(Value::Int(n)) => (*n as usize).min(chars.len()),
                _ => chars.len(),
            };
            // `end` is clamped to the length, so this also covers a
            // start past the end (and `start > end`, which used to panic).
            if start >= end {
                return Ok(Value::String(String::new()));
            }
            Ok(Value::String(chars[start..end].iter().collect()))
        }
        _ => Err(RuntimeError::new(
            "substring() requires a string as first argument",
        )),
    }
}

/// `index_of(haystack, needle)` on a string or array.
fn index_of_of(value: Option<&Value>, needle: Option<&Value>) -> Result<Value, RuntimeError> {
    match value {
        Some(Value::String(s)) => match needle {
            Some(Value::String(substr)) => Ok(Value::Int(
                s.find(substr.as_str()).map(|i| i as i64).unwrap_or(-1),
            )),
            _ => Err(RuntimeError::new("index_of() requires (string, substring)")),
        },
        Some(Value::Array(arr)) => {
            let Some(needle) = needle else {
                return Err(RuntimeError::new("index_of() requires 2 arguments"));
            };
            let idx = arr.iter().position(|v| v == needle);
            Ok(Value::Int(idx.map(|i| i as i64).unwrap_or(-1)))
        }
        _ => Err(RuntimeError::new(
            "index_of() requires a string or array as first argument",
        )),
    }
}

/// The builtins in [`BORROWING_BUILTINS`] on a borrowed first argument
/// (`rest` are the other arguments). `None` for any other name.
pub(super) fn read_only_builtin(
    name: &str,
    first: Option<&Value>,
    rest: &[Value],
) -> Option<Result<Value, RuntimeError>> {
    let (a, b) = (rest.first(), rest.get(1));
    let result = match name {
        "slice" => slice_of(first, a, b),
        "substring" => substring_of(first, a, b),
        "index_of" => index_of_of(first, a),
        "first" => match first {
            Some(Value::Array(items)) => Ok(items.first().cloned().unwrap_or(Value::Null)),
            _ => Err(RuntimeError::new("first() requires an array")),
        },
        "last" => match first {
            Some(Value::Array(items)) => Ok(items.last().cloned().unwrap_or(Value::Null)),
            _ => Err(RuntimeError::new("last() requires an array")),
        },
        "join" => match (first, a) {
            (Some(Value::Array(items)), Some(Value::String(sep))) => {
                let parts: Vec<String> = items.iter().map(|v| format!("{}", v)).collect();
                Ok(Value::String(parts.join(sep)))
            }
            (Some(Value::Array(items)), None) => {
                let parts: Vec<String> = items.iter().map(|v| format!("{}", v)).collect();
                Ok(Value::String(parts.join("")))
            }
            _ => Err(RuntimeError::new(
                "join() requires (array[, separator_string])",
            )),
        },
        "last_index_of" => match (first, a) {
            (Some(Value::String(s)), Some(Value::String(substr))) => Ok(Value::Int(
                s.rfind(substr.as_str()).map(|i| i as i64).unwrap_or(-1),
            )),
            _ => Err(RuntimeError::new(
                "last_index_of() requires (string, substring)",
            )),
        },
        "starts_with" => match (first, a) {
            (Some(Value::String(s)), Some(Value::String(prefix))) => {
                Ok(Value::Bool(s.starts_with(prefix.as_str())))
            }
            _ => Err(RuntimeError::new(
                "starts_with() requires (string, prefix_string)",
            )),
        },
        "ends_with" => match (first, a) {
            (Some(Value::String(s)), Some(Value::String(suffix))) => {
                Ok(Value::Bool(s.ends_with(suffix.as_str())))
            }
            _ => Err(RuntimeError::new(
                "ends_with() requires (string, suffix_string)",
            )),
        },
        "len" => len_of(first),
        "contains" => contains_of(first, a),
        _ => return None,
    };
    // Argument errors read the same on both engines
    // (`semantics::builtin_args`).
    Some(match crate::semantics::builtin_args::expected(name) {
        Some(_) => {
            result.map_err(|_| RuntimeError::new(&crate::semantics::builtin_args::bad_args(name)))
        }
        None => result,
    })
}

/// The value a field/index chain names inside a variable's value, for a
/// borrowed read: plain objects and arrays only (a frozen container on the
/// way is left to the ordinary path, which wraps what it reads).
fn borrowed_leaf<'v>(value: &'v Value, steps: &[PlaceStep<'_>]) -> Option<&'v Value> {
    let mut cur = value;
    for step in steps {
        cur = match (cur, step) {
            (Value::Object(map), PlaceStep::Field(field)) => map.get(*field)?,
            (Value::Object(map), PlaceStep::Index(Value::String(key))) => map.get(key.as_str())?,
            (Value::Array(items), PlaceStep::Index(Value::Int(i))) => {
                &items[crate::semantics::normalize_index(*i, items.len())?]
            }
            _ => return None,
        };
    }
    Some(cur)
}

/// One step of a nested assignment target below its root variable:
/// `.field` or `[index]` (index already evaluated).
pub(super) enum PlaceStep<'a> {
    Field(&'a str),
    Index(Value),
}

/// What the receiver of a mutating method (`push`/`pop`/`add`/`remove`)
/// holds: a collection the method updates in place, a frozen value (the
/// frozen error, named after what it holds), or anything else (an ordinary
/// method call).
enum ReceiverShape {
    InPlace,
    Frozen(String),
    Other,
}

fn receiver_shape(value: &Value, method: &str) -> ReceiverShape {
    match (value, method) {
        (Value::Array(_), "push" | "pop") | (Value::Set(_), "add" | "remove") => {
            ReceiverShape::InPlace
        }
        (Value::Frozen(inner), _) => ReceiverShape::Frozen(inner.type_name().to_ascii_lowercase()),
        _ => ReceiverShape::Other,
    }
}

impl Interpreter {
    /// True when evaluating `expr` cannot run user code or assign any
    /// variable, so evaluating it earlier or later than a read of some
    /// variable gives the same result. (It may still fail, e.g. on an
    /// undefined name; callers keep error order intact separately.)
    pub(super) fn is_effect_free(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) | Expr::StringLit(_) | Expr::Ident(_) => {
                true
            }
            Expr::StringInterp(parts) => parts.iter().all(|part| match part {
                StringPart::Literal(_) => true,
                StringPart::Expr(e) => self.is_effect_free(e),
            }),
            Expr::Array(items) | Expr::Tuple(items) => {
                items.iter().all(|item| self.is_effect_free(item))
            }
            Expr::Object(fields) => fields.iter().all(|(_, e)| self.is_effect_free(e)),
            Expr::Spread(inner) | Expr::Freeze(inner) => self.is_effect_free(inner),
            Expr::BinOp { left, right, .. } => {
                self.is_effect_free(left) && self.is_effect_free(right)
            }
            Expr::UnaryOp { operand, .. } => self.is_effect_free(operand),
            Expr::FieldAccess { object, .. } => self.is_effect_free(object),
            Expr::Index { object, index } => {
                self.is_effect_free(object) && self.is_effect_free(index)
            }
            Expr::Call { function, args } => match function.as_ref() {
                Expr::Ident(name) if PURE_BUILTINS.contains(&name.as_str()) => {
                    self.resolves_to_builtin(name) && args.iter().all(|a| self.is_effect_free(a))
                }
                // The intrinsic reads of a map or set held by a variable
                // (`m.get(k)`, `m.has(k)`, `m.len()`, `s.has(x)`) run no
                // user code.
                Expr::FieldAccess { object, field } => {
                    let Expr::Ident(var) = object.as_ref() else {
                        return false;
                    };
                    let method = field.as_str();
                    let arity_ok = match method {
                        "get" | "has" => args.len() == 1,
                        "len" => args.is_empty(),
                        _ => false,
                    };
                    arity_ok
                        && args.iter().all(|a| self.is_effect_free(a))
                        && self
                            .env
                            .with_value(var, |v| match peel_frozen(v) {
                                Value::Map(_) => true,
                                Value::Set(_) => method == "has",
                                _ => false,
                            })
                            .unwrap_or(false)
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// True when `name` currently names the builtin of the same name
    /// (i.e. the user has not shadowed it).
    fn resolves_to_builtin(&self, name: &str) -> bool {
        self.env
            .with_value(name, |v| matches!(v, Value::BuiltIn(b) if b == name))
            .unwrap_or(false)
    }

    /// Statement-level evaluation: like `eval_expr`, but when the value is
    /// not needed (`want_value == false`) the in-place mutating forms skip
    /// copying the updated collection.
    pub(super) fn eval_expr_stmt(
        &mut self,
        expr: &Expr,
        want_value: bool,
    ) -> Result<Value, RuntimeError> {
        match self.try_mutate_in_place(expr, want_value) {
            Some(result) => result,
            None => self.eval_expr(expr),
        }
    }

    /// In-place forms of the mutating collection methods on a mutable
    /// variable: `v.push(x)` / `push(v, x)`, `v.pop()` / `pop(v)`,
    /// `v.add(x)`, `v.remove(x)` (sets). Returns `None` when `expr` is not
    /// one of them, so the caller falls back to ordinary dispatch.
    ///
    /// `push`, `add` and `remove` evaluate to the updated collection; it is
    /// copied out only when `want_result` is true.
    ///
    /// From edition 2027 the free function `push(v, x)` is pure (it never
    /// updates `v`; docs/editions/2027.md), so it is not one of these forms
    /// there; the method form `v.push(x)` still is.
    pub(super) fn try_mutate_in_place(
        &mut self,
        expr: &Expr,
        want_result: bool,
    ) -> Option<Result<Value, RuntimeError>> {
        let free_push = self.edition < crate::semantics::edition::Edition::E2027;
        self.mutate_in_place(expr, want_result, free_push)
    }

    /// [`try_mutate_in_place`](Self::try_mutate_in_place), with the free
    /// `push(v, x)` form included when `free_push` is true.
    fn mutate_in_place(
        &mut self,
        expr: &Expr,
        want_result: bool,
        free_push: bool,
    ) -> Option<Result<Value, RuntimeError>> {
        let Expr::Call { function, args } = expr else {
            return None;
        };
        let (var, method, rest): (&str, &str, &[Expr]) = match function.as_ref() {
            Expr::FieldAccess { object, field } => match object.as_ref() {
                Expr::Ident(v) => (v.as_str(), field.as_str(), args.as_slice()),
                place @ (Expr::FieldAccess { .. } | Expr::Index { .. }) => {
                    return self.mutate_nested_place(place, field, args, want_result)
                }
                _ => return None,
            },
            Expr::Ident(f)
                if (f == "push" && args.len() == 2 && free_push)
                    || (f == "pop" && args.len() == 1) =>
            {
                match &args[0] {
                    Expr::Ident(v) => (v.as_str(), f.as_str(), &args[1..]),
                    _ => return None,
                }
            }
            _ => return None,
        };
        if self.env.is_mutable(var) != Some(true) {
            return None;
        }
        if matches!(function.as_ref(), Expr::FieldAccess { .. }) {
            if !crate::semantics::is_mutating_method(method, rest.len()) {
                return None;
            }
            return Some(self.mutate_var_method(var, method, rest, want_result));
        }
        // The free forms `push(v, x)` (edition 2026) and `pop(v)`.
        // In place only on an array: anything else takes the ordinary path
        // (the builtin's argument error annotated with the argument types).
        if matches!(method, "push" | "pop")
            && !self.env.with_value(var, |v| matches!(v, Value::Array(_)))?
        {
            return None;
        }
        match (method, rest.len()) {
            ("push", 1) => {
                let val = match self.eval_expr(&rest[0]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                Some(self.update_var(var, |cur| {
                    match cur {
                        Value::Array(items) => items.push(val),
                        _ => {
                            return Err(RuntimeError::new(
                                &crate::semantics::builtin_args::bad_args("push"),
                            ))
                        }
                    }
                    Ok(if want_result {
                        cur.clone()
                    } else {
                        Value::Null
                    })
                }))
            }
            ("pop", 0) => Some(self.update_var(var, |cur| match cur {
                Value::Array(items) => Ok(items.pop().unwrap_or(Value::Null)),
                _ => Err(RuntimeError::new(
                    &crate::semantics::builtin_args::bad_args("pop"),
                )),
            })),
            _ => None,
        }
    }

    /// `v.push(x)`, `v.pop()`, `v.add(x)` and `v.remove(x)` on a mutable
    /// variable `v` (the method form; both engines): the arguments are
    /// evaluated first, then the variable is read. An array (`push`/`pop`)
    /// or a set (`add`/`remove`) is updated in place; a frozen value is the
    /// frozen error; anything else is an ordinary method call on the value
    /// (a map's `remove`, a struct's own `push`, ...), whose updated
    /// receiver is stored back when the method is declared `mut it`.
    fn mutate_var_method(
        &mut self,
        var: &str,
        method: &str,
        args: &[Expr],
        want_result: bool,
    ) -> Result<Value, RuntimeError> {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(self.eval_expr(arg)?);
        }
        let shape = self
            .env
            .with_value(var, |v| receiver_shape(v, method))
            .ok_or_else(|| RuntimeError::new(&format!("undefined: {}", var)))?;
        match shape {
            ReceiverShape::InPlace => self.update_var(var, |cur| {
                apply_mutating_method(cur, method, values.pop(), want_result)
            }),
            ReceiverShape::Frozen(kind) => Err(RuntimeError::new(
                &crate::semantics::frozen_method(method, &kind),
            )),
            ReceiverShape::Other => {
                let receiver = self
                    .env
                    .get(var)
                    .ok_or_else(|| RuntimeError::new(&format!("undefined: {}", var)))?;
                let (result, updated) = self.call_method_on_values(receiver, method, values)?;
                if let Some(updated) = updated {
                    self.assign_receiver_place(var, &[], updated)?;
                }
                Ok(result)
            }
        }
    }

    /// Store the updated receiver of a method declared `mut it` into the
    /// place the method was called on (`c`, `xs[i]`, `o.f`): like
    /// `place = updated`, so an immutable root is the immutable-assignment
    /// error.
    pub(super) fn assign_receiver_place(
        &mut self,
        root: &str,
        steps: &[PlaceStep<'_>],
        updated: Value,
    ) -> Result<(), RuntimeError> {
        self.env
            .with_binding_mut(root, |cur, mutable| {
                if !mutable {
                    return Err(RuntimeError::new(&crate::semantics::immutable_reassign(
                        root,
                    )));
                }
                let mut slot = cur;
                for step in steps {
                    slot = place_child(root, slot, step)?;
                }
                *slot = updated;
                Ok(())
            })
            .unwrap_or_else(|| Err(RuntimeError::new(&format!("undefined: {}", root))))
    }

    /// `place.push(x)`, `place.pop()`, `place.add(x)` and `place.remove(x)`
    /// where `place` is a chain of fields and indexes rooted at a variable
    /// (`groups[k].push(x)`, `o.items.pop()`, `m[0].add(x)`): the method form
    /// updates the place, exactly as `xs.push(x)` updates a variable.
    ///
    /// Order (both engines): the index expressions left to right, then the
    /// arguments, then the place is read and updated. When the value at the
    /// place has no in-place form for the method (an object with its own
    /// `push`, a set receiving `push`, ...) the call is an ordinary method
    /// call on that value and nothing is stored.
    ///
    /// A frozen receiver raises `cannot push a frozen value` (like the
    /// variable form); an immutable root raises the error of `o.a = v` on
    /// it, and so does a frozen value along the path.
    fn mutate_nested_place(
        &mut self,
        place: &Expr,
        method: &str,
        args: &[Expr],
        want_result: bool,
    ) -> Option<Result<Value, RuntimeError>> {
        if !crate::semantics::is_mutating_method(method, args.len()) {
            return None;
        }
        let (root, exprs) = place_path(place)?;
        if exprs.is_empty() || !self.env.contains(root) {
            // An unbound root: the ordinary path reports it.
            return None;
        }
        Some(self.mutate_nested_steps(root, &exprs, method, args, want_result))
    }

    fn mutate_nested_steps(
        &mut self,
        root: &str,
        exprs: &[Result<&str, &Expr>],
        method: &str,
        args: &[Expr],
        want_result: bool,
    ) -> Result<Value, RuntimeError> {
        let mut steps = Vec::with_capacity(exprs.len());
        for step in exprs {
            steps.push(match step {
                Ok(field) => PlaceStep::Field(field),
                Err(index) => PlaceStep::Index(self.eval_expr(index)?),
            });
        }
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(self.eval_expr(arg)?);
        }
        let updated = self
            .env
            .with_binding_mut(root, |cur, mutable| {
                let in_place = match place_leaf(cur, &steps).map(|v| receiver_shape(v, method)) {
                    Some(ReceiverShape::Frozen(kind)) => {
                        return Err(RuntimeError::new(&crate::semantics::frozen_method(
                            method, &kind,
                        )))
                    }
                    Some(ReceiverShape::InPlace) => true,
                    _ => false,
                };
                if !in_place {
                    return Ok(None);
                }
                if !mutable {
                    return Err(RuntimeError::new(&crate::semantics::immutable_reassign(
                        root,
                    )));
                }
                let mut slot = cur;
                for step in &steps {
                    slot = place_child(root, slot, step)?;
                }
                apply_mutating_method(slot, method, values.pop(), want_result).map(Some)
            })
            .unwrap_or_else(|| Err(RuntimeError::new(&format!("undefined: {}", root))))?;
        if let Some(result) = updated {
            return Ok(result);
        }
        // No in-place form: read the place like an expression would and
        // call the method on its value.
        let mut receiver = self
            .env
            .get(root)
            .ok_or_else(|| RuntimeError::new(&format!("undefined: {}", root)))?;
        for step in &steps {
            receiver = match step {
                PlaceStep::Field(field) => self.field_of(&receiver, field)?,
                PlaceStep::Index(index) => Self::index_of(&receiver, index)?,
            };
        }
        let (result, updated) = self.call_method_on_values(receiver, method, values)?;
        if let Some(updated) = updated {
            self.assign_receiver_place(root, &steps, updated)?;
        }
        Ok(result)
    }

    /// `receiver.method(args...)` for already-evaluated values: a user
    /// method directly (with its updated receiver when it is declared
    /// `mut it`), anything else through the ordinary method dispatch (bound
    /// to hidden names in a scratch scope, so nothing is evaluated twice).
    fn call_method_on_values(
        &mut self,
        receiver: Value,
        method: &str,
        args: Vec<Value>,
    ) -> Result<(Value, Option<Value>), RuntimeError> {
        if let Some(found) = self.user_method_of(&receiver, method) {
            return self.call_found_method(found, &receiver, method, args);
        }
        const RECEIVER: &str = "\u{0}receiver";
        self.env.push_scope();
        self.env
            .define_with_mutability(RECEIVER.to_string(), receiver, false);
        let mut arg_exprs = Vec::with_capacity(args.len());
        for (i, value) in args.into_iter().enumerate() {
            let name = format!("\u{0}arg{}", i);
            self.env.define_with_mutability(name.clone(), value, false);
            arg_exprs.push(Expr::Ident(name));
        }
        let call = Expr::Call {
            function: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident(RECEIVER.to_string())),
                field: method.to_string(),
            }),
            args: arg_exprs,
        };
        let result = self.eval_expr(&call);
        self.env.pop_scope();
        Ok((result?, None))
    }

    /// Apply `f` to the mutable variable `var` in place.
    fn update_var(
        &self,
        var: &str,
        f: impl FnOnce(&mut Value) -> Result<Value, RuntimeError>,
    ) -> Result<Value, RuntimeError> {
        self.env.with_value_mut(var, f)?
    }

    /// Read-only calls answered by borrowing the receiver variable:
    /// `s.has(x)` on sets, `m.get(k)` / `m.has(k)` / `m.len()` on maps, and
    /// the builtins `len(v)` / `contains(v, x)` and [`BORROWING_BUILTINS`]
    /// (also as array methods, `xs.slice(i, j)`), on a variable or a
    /// field/index chain of one. Returns `None` when `expr` is not one of
    /// them (or an argument is not effect-free), so the caller falls back
    /// to ordinary dispatch.
    pub(super) fn try_read_in_place(&mut self, expr: &Expr) -> Option<Result<Value, RuntimeError>> {
        let Expr::Call { function, args } = expr else {
            return None;
        };
        match function.as_ref() {
            Expr::FieldAccess { object, field } => {
                if BORROWING_ARRAY_METHODS.contains(&field.as_str())
                    && self.borrows_array_method(object, field, args.len())
                {
                    if let Some(result) = self.borrowed_builtin_call(field, object, args, true) {
                        return Some(result);
                    }
                }
                let Expr::Ident(var) = object.as_ref() else {
                    return None;
                };
                let method = field.as_str();
                let arity_ok = match method {
                    "has" | "get" => args.len() == 1,
                    "len" => args.is_empty(),
                    _ => false,
                };
                if !arity_ok || !args.iter().all(|a| self.is_effect_free(a)) {
                    return None;
                }
                // Only sets and maps have these as intrinsic methods; for
                // anything else dispatch may reach user code.
                let applies = self
                    .env
                    .with_value(var, |v| match (peel_frozen(v), method) {
                        (Value::Set(_), "has") => true,
                        (Value::Map(_), _) => true,
                        _ => false,
                    })?;
                if !applies {
                    return None;
                }
                let arg = match args.first().map(|a| self.eval_expr(a)) {
                    Some(Err(e)) => return Some(Err(e)),
                    Some(Ok(v)) => Some(v),
                    None => None,
                };
                self.env
                    .with_value(var, |v| match (peel_frozen(v), method, &arg) {
                        (Value::Set(items), "has", Some(x)) => {
                            Some(Value::Bool(items.has_value(x)))
                        }
                        (Value::Map(pairs), "has", Some(key)) => {
                            Some(Value::Bool(pairs.has_key_value(key)))
                        }
                        (Value::Map(pairs), "get", Some(key)) => {
                            Some(pairs.get_value(key).cloned().unwrap_or(Value::Null))
                        }

                        (Value::Map(pairs), "len", None) => Some(Value::Int(pairs.len() as i64)),
                        _ => None,
                    })
                    .flatten()
                    .map(Ok)
            }
            Expr::Ident(name)
                if (name == "get" && (args.len() == 2 || args.len() == 3))
                    || (name == "has_key" && args.len() == 2) =>
            {
                // `get(counts, w, 0)` / `has_key(counts, w)`: borrow the
                // object instead of copying it for the call.
                let Expr::Ident(var) = &args[0] else {
                    return None;
                };
                if !self.resolves_to_builtin(name)
                    || !self.env.contains(var)
                    || !args[1..].iter().all(|a| self.is_effect_free(a))
                {
                    return None;
                }
                let mut rest = Vec::with_capacity(args.len() - 1);
                for arg in &args[1..] {
                    match self.eval_expr(arg) {
                        Ok(v) => rest.push(v),
                        Err(e) => return Some(Err(e)),
                    }
                }
                let result = self.env.with_value(var, |v| {
                    if name == "get" {
                        get_of(Some(v), rest.first(), rest.get(1))
                    } else {
                        has_key_of(Some(v), rest.first())
                    }
                })?;
                // Errors take the ordinary path, which annotates them with
                // the argument types (the operands are effect-free, so
                // evaluating them again is unobservable).
                result.ok().map(Ok)
            }
            Expr::Ident(name) if name == "len" || name == "contains" => {
                let expected = if name == "len" { 1 } else { 2 };
                if args.len() != expected || !self.resolves_to_builtin(name) {
                    return None;
                }
                let Expr::Ident(var) = &args[0] else {
                    // `len(o.items)`: borrow through the chain.
                    return self.borrowed_builtin_call(name, &args[0], &args[1..], false);
                };
                if !self.env.contains(var) {
                    return None;
                }
                let needle = match args.get(1) {
                    Some(a) if !self.is_effect_free(a) => return None,
                    Some(a) => match self.eval_expr(a) {
                        Ok(v) => Some(v),
                        Err(e) => return Some(Err(e)),
                    },
                    None => None,
                };
                let result = self.env.with_value(var, |v| {
                    if name == "len" {
                        len_of(Some(v))
                    } else {
                        contains_of(Some(v), needle.as_ref())
                    }
                })?;
                // Errors take the ordinary path, which annotates them with
                // the argument types, as for `get`/`has_key` above.
                result.ok().map(Ok)
            }
            Expr::Ident(name) if BORROWING_BUILTINS.contains(&name.as_str()) => {
                // `slice(chars, i, j)`, `join(o.parts, ",")`, ...: borrow the
                // first argument instead of copying it for the call.
                let first = args.first()?;
                if crate::builtins_registry::check_arity(name, args.len()).is_err()
                    || !self.resolves_to_builtin(name)
                {
                    return None;
                }
                self.borrowed_builtin_call(name, first, &args[1..], false)
            }
            _ => None,
        }
    }

    /// `name(place, rest...)` for a [`BORROWING_BUILTINS`] (or array-method)
    /// builtin whose first argument is a variable or a field/index chain of
    /// one: the other arguments and the chain's indexes are evaluated first
    /// (they are effect-free, so the order is unobservable), then the
    /// builtin runs on a reference to the value under the scope lock.
    /// `None` (the caller takes the ordinary path, which evaluates the
    /// effect-free operands again) when the shape does not apply or the
    /// builtin fails: errors are raised, and annotated, by the ordinary path.
    /// With `require_array` (the method form) the value must be a plain
    /// array: other receivers dispatch methods differently.
    fn borrowed_builtin_call(
        &mut self,
        name: &str,
        first: &Expr,
        rest: &[Expr],
        require_array: bool,
    ) -> Option<Result<Value, RuntimeError>> {
        let (root, steps) = place_path(first)?;
        if !rest.iter().all(|a| self.is_effect_free(a))
            || !steps
                .iter()
                .all(|s| s.as_ref().map_or_else(|i| self.is_effect_free(i), |_| true))
            || !self.env.contains(root)
        {
            return None;
        }
        let mut keys = Vec::with_capacity(steps.len());
        for step in &steps {
            keys.push(match step {
                Ok(field) => PlaceStep::Field(field),
                Err(index) => match self.eval_expr(index) {
                    Ok(v) => PlaceStep::Index(v),
                    Err(e) => return Some(Err(e)),
                },
            });
        }
        // A receiver that is not an array may reject the call before its
        // arguments are evaluated (a module namespace): check it first.
        if require_array
            && !self
                .env
                .with_value(root, |v| {
                    matches!(borrowed_leaf(v, &keys), Some(Value::Array(_)))
                })
                .unwrap_or(false)
        {
            return None;
        }
        let mut values = Vec::with_capacity(rest.len());
        for arg in rest {
            match self.eval_expr(arg) {
                Ok(v) => values.push(v),
                Err(e) => return Some(Err(e)),
            }
        }
        let result = self
            .env
            .with_value(root, |v| {
                let leaf = borrowed_leaf(v, &keys)?;
                if require_array && !matches!(leaf, Value::Array(_)) {
                    return None;
                }
                read_only_builtin(name, Some(leaf), &values)
            })
            .flatten()?;
        result.ok().map(Ok)
    }

    /// Whether `object.method(args)` (with `argc` arguments) may run as the
    /// global builtin `method` on a borrowed array: that is what
    /// `known_methods` dispatch does for an array receiver when `method`
    /// still names the builtin. (The receiver being a plain array is
    /// checked when it is borrowed.)
    fn borrows_array_method(&self, object: &Expr, method: &str, argc: usize) -> bool {
        place_path(object).is_some()
            && self.resolves_to_builtin(method)
            && crate::builtins_registry::check_arity(method, argc + 1).is_ok()
    }

    /// In-place forms of assignment. Returns `None` when `target = value`
    /// has no in-place form; the caller then evaluates it normally.
    ///
    /// * `x = push(x, e)` on a mutable `x`: push in place (no copy).
    /// * `x = x <op> e` (including `x op= e`) on a mutable `x` with an
    ///   effect-free `e`: string concatenation appends in place; other
    ///   operators compute from the borrowed value.
    /// * `name.field = v` and `name[i] = v`: update the slot in place, with
    ///   the same checks and error order as copying the container.
    /// * `g[i][j] = v`, `o.a.b = v`, `rows[i].name = v` (any chain of
    ///   fields and indexes on a variable): update the innermost slot in
    ///   place. Collections have value semantics, so other variables that
    ///   were copied from `g` are unaffected.
    pub(super) fn try_assign_in_place(
        &mut self,
        target: &Expr,
        value: &Expr,
    ) -> Option<Result<(), RuntimeError>> {
        if let Some(result) = self.try_update_place(target, value) {
            return Some(result);
        }
        match target {
            Expr::Ident(x) => self.try_update_ident(x, value),
            Expr::FieldAccess { object, field } => match object.as_ref() {
                Expr::Ident(name) => Some(self.assign_field_in_place(name, field, value)),
                _ => self.assign_path_in_place(target, value),
            },
            Expr::Index { object, index } => match object.as_ref() {
                Expr::Ident(name) => Some(self.assign_index_in_place(name, index, value)),
                _ => self.assign_path_in_place(target, value),
            },
            _ => None,
        }
    }

    /// Nested assignment `root<step><step>... = value`. Returns `None` when
    /// the target is not a chain of fields/indexes rooted at a variable.
    /// Order: the value, then the index expressions left to right, then
    /// the update (same as the single-step forms).
    fn assign_path_in_place(
        &mut self,
        target: &Expr,
        value: &Expr,
    ) -> Option<Result<(), RuntimeError>> {
        let mut exprs = Vec::new();
        let mut cur = target;
        let root = loop {
            match cur {
                Expr::FieldAccess { object, field } => {
                    exprs.push(Ok(field.as_str()));
                    cur = object;
                }
                Expr::Index { object, index } => {
                    exprs.push(Err(index.as_ref()));
                    cur = object;
                }
                Expr::Ident(name) => break name.as_str(),
                _ => return None,
            }
        };
        exprs.reverse();
        Some(self.assign_path_steps(root, &exprs, value))
    }

    fn assign_path_steps(
        &mut self,
        root: &str,
        exprs: &[Result<&str, &Expr>],
        value: &Expr,
    ) -> Result<(), RuntimeError> {
        let val = self.eval_expr(value)?;
        let mut steps = Vec::with_capacity(exprs.len());
        for step in exprs {
            steps.push(match step {
                Ok(field) => PlaceStep::Field(field),
                Err(index) => PlaceStep::Index(self.eval_expr(index)?),
            });
        }
        self.env
            .with_binding_mut(root, |cur, mutable| {
                if !mutable {
                    return Err(RuntimeError::new(&crate::semantics::immutable_reassign(
                        root,
                    )));
                }
                let (last, init) = steps.split_last().expect("BUG: nested place has steps");
                let mut slot = cur;
                for step in init {
                    slot = place_child(root, slot, step)?;
                }
                place_store(root, slot, last, val)
            })
            .unwrap_or_else(|| Err(RuntimeError::new(&format!("undefined: {}", root))))
    }

    fn try_update_ident(&mut self, x: &str, value: &Expr) -> Option<Result<(), RuntimeError>> {
        match value {
            Expr::Call { function, args }
                if matches!(function.as_ref(), Expr::Ident(f) if f == "push")
                    && args.len() == 2
                    && matches!(&args[0], Expr::Ident(v) if v == x) =>
            {
                // `x = push(x, e)`: the in-place push leaves `x` holding
                // exactly the value the assignment would store (any
                // edition: it is the explicit form of a pure `push`).
                self.mutate_in_place(value, false, true)
                    .map(|result| result.map(|_| ()))
            }
            Expr::Call { function, args } => match function.as_ref() {
                Expr::FieldAccess { object, field } if matches!(object.as_ref(), Expr::Ident(o) if o == x) => {
                    self.try_update_map_ident(x, field, args)
                }
                _ => None,
            },
            Expr::BinOp { left, op, right }
                if matches!(left.as_ref(), Expr::Ident(l) if l == x)
                    && !matches!(op, BinOp::And | BinOp::Or) =>
            {
                if self.env.is_mutable(x) != Some(true) || !self.is_effect_free(right) {
                    return None;
                }
                let rhs = match self.eval_expr(right) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let rhs = peel_frozen(&rhs);
                let caps = self.caps;
                Some(self.env.with_value_mut(x, |cur| {
                    if let (Value::Array(items), BinOp::Add, Value::Array(more)) =
                        (&mut *cur, op, rhs)
                    {
                        // `xs = xs + [v]`: append in place (the result is
                        // the same new array the assignment would store).
                        caps.check_collection(items.len() + more.len())
                            .map_err(|m| RuntimeError::new(&m))?;
                        items.extend(more.iter().cloned());
                        return Ok(());
                    }
                    if let (Value::String(s), BinOp::Add) = (&mut *cur, op) {
                        let concat = matches!(
                            crate::semantics::binary(
                                crate::semantics::BinaryOp::Add,
                                crate::semantics::Operand::Str(s),
                                semantic_operand(rhs),
                                self.edition(),
                            ),
                            Ok(crate::semantics::Outcome::Concat)
                        );
                        if concat {
                            use std::fmt::Write as _;
                            let before = s.len();
                            let _ = write!(s, "{}", rhs);
                            if let Err(m) = caps.check_string(s.len()) {
                                s.truncate(before);
                                return Err(RuntimeError::new(&m));
                            }
                            return Ok(());
                        }
                    }
                    let updated = self.eval_binop(peel_frozen(cur), op, rhs)?;
                    *cur = updated;
                    Ok(())
                }))
                .map(|r| r.and_then(|inner| inner))
            }
            _ => None,
        }
    }

    /// `m = m.set(k, v)` / `m = m.remove(k)` on a mutable `m` holding a
    /// (not frozen) Map, with effect-free arguments: update the entries in
    /// place. The method returns a copy of `m` with that change and the
    /// assignment stores it into `m`, so updating `m` in place leaves it
    /// holding exactly that value, without copying the map twice per call
    /// (building a map in a loop is O(1) per entry instead of O(n)).
    fn try_update_map_ident(
        &mut self,
        x: &str,
        method: &str,
        args: &[Expr],
    ) -> Option<Result<(), RuntimeError>> {
        if !matches!((method, args.len()), ("set", 2) | ("remove", 1))
            || self.env.is_mutable(x) != Some(true)
            || !args.iter().all(|a| self.is_effect_free(a))
            || !self.env.with_value(x, |v| matches!(v, Value::Map(_)))?
        {
            return None;
        }
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            match self.eval_expr(arg) {
                Ok(v) => values.push(v),
                Err(e) => return Some(Err(e)),
            }
        }
        let mut values = values.into_iter();
        let (key, value) = (values.next()?, values.next());
        Some(
            self.env
                .with_value_mut(x, |cur| {
                    let Value::Map(entries) = cur else {
                        return Err(RuntimeError::new("BUG: map receiver changed"));
                    };
                    match value {
                        Some(value) => entries.insert_value(key, value),
                        None => {
                            entries.remove_key(&key);
                        }
                    }
                    Ok(())
                })
                .and_then(|inner| inner),
        )
    }

    /// `p = p <op> e` (and `p <op>= e`) where `p` is a field/index place
    /// rooted at a mutable variable (`o.n`, `a[i]`, `o.items[0].n`), `e`
    /// and every index are effect-free: read, combine and store under one
    /// scope lock.
    ///
    /// Besides skipping two copies of the container, this makes the update
    /// atomic with respect to other tasks that share the variable's scope
    /// (squad `spawn`s calling a shared closure): no other thread can write
    /// the place between our read and our write, so concurrent `o.n += 1`
    /// never loses an increment. [`try_update_ident`](Self::try_update_ident)
    /// gives plain variables the same guarantee.
    ///
    /// Returns `None` (caller takes the general path, which reports the
    /// precise error) when the shape does not match, an operand fails to
    /// evaluate, or the place cannot be walked (missing key, frozen value,
    /// immutable binding).
    fn try_update_place(
        &mut self,
        target: &Expr,
        value: &Expr,
    ) -> Option<Result<(), RuntimeError>> {
        let Expr::BinOp { left, op, right } = value else {
            return None;
        };
        if matches!(op, BinOp::And | BinOp::Or) || !same_place(target, left) {
            return None;
        }
        let mut exprs = Vec::new();
        let mut cur = target;
        let root = loop {
            match cur {
                Expr::FieldAccess { object, field } => {
                    exprs.push(Ok(field.as_str()));
                    cur = object;
                }
                Expr::Index { object, index } => {
                    exprs.push(Err(index.as_ref()));
                    cur = object;
                }
                Expr::Ident(name) => break name.as_str(),
                _ => return None,
            }
        };
        if exprs.is_empty() || self.env.is_mutable(root) != Some(true) {
            return None;
        }
        exprs.reverse();
        let indexes_pure = exprs.iter().all(|step| match step {
            Ok(_) => true,
            Err(index) => self.is_effect_free(index),
        });
        if !indexes_pure || !self.is_effect_free(right) {
            return None;
        }
        // Effect-free operands may be evaluated early (and again by the
        // general path on fallback) without any observable difference.
        let rhs = self.eval_expr(right).ok()?;
        let mut steps = Vec::with_capacity(exprs.len());
        for step in &exprs {
            steps.push(match step {
                Ok(field) => PlaceStep::Field(field),
                Err(index) => PlaceStep::Index(self.eval_expr(index).ok()?),
            });
        }
        let rhs = peel_frozen(&rhs);
        self.env
            .with_binding_mut(root, |cur, mutable| {
                if !mutable {
                    return None;
                }
                let mut slot = cur;
                for step in &steps {
                    slot = place_child(root, slot, step).ok()?;
                }
                Some(
                    self.eval_binop(peel_frozen(slot), op, rhs)
                        .map(|updated| *slot = updated),
                )
            })
            .flatten()
    }

    fn assign_field_in_place(
        &mut self,
        name: &str,
        field: &str,
        value: &Expr,
    ) -> Result<(), RuntimeError> {
        let val = self.eval_expr(value)?;
        self.env
            .with_binding_mut(name, |cur, mutable| {
                if cur.is_frozen() {
                    return Err(RuntimeError::new(&format!(
                        "cannot modify frozen value '{}': field '{}'",
                        name, field
                    )));
                }
                if !mutable {
                    return Err(RuntimeError::new(&crate::semantics::immutable_reassign(
                        name,
                    )));
                }
                match cur {
                    Value::Object(map) => {
                        map.insert(field.to_string(), val);
                        Ok(())
                    }
                    // Used to be silently ignored; the VM always errored.
                    _ => Err(RuntimeError::new("cannot set field on non-object")),
                }
            })
            .unwrap_or_else(|| Err(RuntimeError::new(&format!("undefined: {}", name))))
    }

    fn assign_index_in_place(
        &mut self,
        name: &str,
        index: &Expr,
        value: &Expr,
    ) -> Result<(), RuntimeError> {
        let val = self.eval_expr(value)?;
        let idx = self.eval_expr(index)?;
        self.env
            .with_binding_mut(name, |cur, mutable| {
                if cur.is_frozen() {
                    return Err(RuntimeError::new(&format!(
                        "cannot modify frozen value '{}': index assignment",
                        name
                    )));
                }
                let type_name = cur.type_name().to_string();
                match (&mut *cur, &idx) {
                    (Value::Array(items), Value::Int(i)) => {
                        let len = items.len();
                        let slot = crate::semantics::normalize_index(*i, len).ok_or_else(|| {
                            RuntimeError::new(&crate::semantics::index_out_of_bounds(
                                *i, "array", len,
                            ))
                        })?;
                        if !mutable {
                            return Err(RuntimeError::new(&crate::semantics::immutable_reassign(
                                name,
                            )));
                        }
                        items[slot] = val;
                    }
                    (Value::Object(map), Value::String(key)) => {
                        if !mutable {
                            return Err(RuntimeError::new(&crate::semantics::immutable_reassign(
                                name,
                            )));
                        }
                        map.insert(key.clone(), val);
                    }
                    (Value::Array(_) | Value::Object(_), other) => {
                        return Err(RuntimeError::new(&crate::semantics::invalid_index(
                            &type_name,
                            other.type_name(),
                        )));
                    }
                    (other, _) => {
                        return Err(RuntimeError::new(&crate::semantics::invalid_index_assign(
                            other.type_name(),
                        )));
                    }
                }
                Ok(())
            })
            .unwrap_or_else(|| Err(RuntimeError::new(&format!("undefined: {}", name))))
    }
}

/// True when `a` and `b` name the same place syntactically: the same
/// variable followed by the same fields and literal (or same-variable)
/// indexes. Conservative: anything else is "different".
fn same_place(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Ident(x), Expr::Ident(y)) => x == y,
        (
            Expr::FieldAccess {
                object: o1,
                field: f1,
            },
            Expr::FieldAccess {
                object: o2,
                field: f2,
            },
        ) => f1 == f2 && same_place(o1, o2),
        (
            Expr::Index {
                object: o1,
                index: i1,
            },
            Expr::Index {
                object: o2,
                index: i2,
            },
        ) => {
            let same_index = match (i1.as_ref(), i2.as_ref()) {
                (Expr::Int(x), Expr::Int(y)) => x == y,
                (Expr::StringLit(x), Expr::StringLit(y)) => x == y,
                (Expr::Ident(x), Expr::Ident(y)) => x == y,
                _ => false,
            };
            same_index && same_place(o1, o2)
        }
        _ => false,
    }
}

/// A chain of fields and indexes rooted at a variable (`g[k].a`): the root
/// name and the steps in order (`Ok(field)` / `Err(index expression)`).
/// `None` when `target` is not such a chain.
fn place_path(target: &Expr) -> Option<(&str, Vec<Result<&str, &Expr>>)> {
    let mut steps = Vec::new();
    let mut cur = target;
    let root = loop {
        match cur {
            Expr::FieldAccess { object, field } => {
                steps.push(Ok(field.as_str()));
                cur = object;
            }
            Expr::Index { object, index } => {
                steps.push(Err(index.as_ref()));
                cur = object;
            }
            Expr::Ident(name) => break name.as_str(),
            _ => return None,
        }
    };
    steps.reverse();
    Some((root, steps))
}

/// The value a field/index chain names inside `value`, read the way an
/// expression reads it (through frozen containers). `None` when a step
/// does not resolve to a plain field, key or element.
fn place_leaf<'v>(value: &'v Value, steps: &[PlaceStep<'_>]) -> Option<&'v Value> {
    let mut cur = value;
    for step in steps {
        cur = match (peel_frozen(cur), step) {
            (Value::Object(map), PlaceStep::Field(field)) => map.get(*field)?,
            (Value::Object(map), PlaceStep::Index(Value::String(key))) => map.get(key.as_str())?,
            (Value::Array(items), PlaceStep::Index(Value::Int(i))) => {
                &items[crate::semantics::normalize_index(*i, items.len())?]
            }
            _ => return None,
        };
    }
    Some(cur)
}

/// `push` / `pop` on an array or `add` / `remove` on a set, in place.
/// Returns what the method call evaluates to: the popped element, or the
/// updated collection (copied only when `want_result`).
fn apply_mutating_method(
    slot: &mut Value,
    method: &str,
    arg: Option<Value>,
    want_result: bool,
) -> Result<Value, RuntimeError> {
    let applied = match (&mut *slot, method, arg) {
        (Value::Array(items), "push", Some(value)) => {
            items.push(value);
            true
        }
        (Value::Array(items), "pop", None) => return Ok(items.pop().unwrap_or(Value::Null)),
        (Value::Set(items), "add", Some(value)) => {
            items.add_value(value);
            true
        }
        (Value::Set(items), "remove", Some(value)) => {
            items.remove_value(&value);
            true
        }
        _ => false,
    };
    if !applied {
        return Err(RuntimeError::new(&format!(
            "BUG: {}() has no in-place form for {}",
            method,
            slot.type_name()
        )));
    }
    Ok(if want_result {
        slot.clone()
    } else {
        Value::Null
    })
}

fn frozen_error(root: &str) -> RuntimeError {
    RuntimeError::new(&format!(
        "cannot modify frozen value '{}': nested assignment",
        root
    ))
}

/// The slot `step` names inside `container`, for walking a nested target.
fn place_child<'v>(
    root: &str,
    container: &'v mut Value,
    step: &PlaceStep<'_>,
) -> Result<&'v mut Value, RuntimeError> {
    if container.is_frozen() {
        return Err(frozen_error(root));
    }
    let type_name = container.type_name().to_string();
    match (container, step) {
        (Value::Array(items), PlaceStep::Index(Value::Int(i))) => {
            let len = items.len();
            let slot = crate::semantics::normalize_index(*i, len).ok_or_else(|| {
                RuntimeError::new(&crate::semantics::index_out_of_bounds(*i, "array", len))
            })?;
            Ok(&mut items[slot])
        }
        (Value::Object(map), PlaceStep::Index(Value::String(key))) => map
            .get_mut(key.as_str())
            .ok_or_else(|| RuntimeError::new(&crate::semantics::missing_key(key))),
        (Value::Object(map), PlaceStep::Field(field)) => map
            .get_mut(*field)
            .ok_or_else(|| RuntimeError::new(&crate::semantics::missing_key(field))),
        (Value::Array(_) | Value::Object(_), PlaceStep::Index(other)) => Err(RuntimeError::new(
            &crate::semantics::invalid_index(&type_name, other.type_name()),
        )),
        (_, PlaceStep::Field(field)) => Err(RuntimeError::new(&crate::semantics::field_access(
            field, &type_name,
        ))),
        (_, PlaceStep::Index(_)) => Err(RuntimeError::new(
            &crate::semantics::invalid_index_assign(&type_name),
        )),
    }
}

/// Store `val` into the slot `step` names inside `container`.
fn place_store(
    root: &str,
    container: &mut Value,
    step: &PlaceStep<'_>,
    val: Value,
) -> Result<(), RuntimeError> {
    if container.is_frozen() {
        return Err(frozen_error(root));
    }
    match (container, step) {
        (Value::Object(map), PlaceStep::Field(field)) => {
            map.insert(field.to_string(), val);
            Ok(())
        }
        (Value::Object(map), PlaceStep::Index(Value::String(key))) => {
            map.insert(key.clone(), val);
            Ok(())
        }
        (container, step) => {
            let slot = place_child(root, container, step)?;
            *slot = val;
            Ok(())
        }
    }
}
