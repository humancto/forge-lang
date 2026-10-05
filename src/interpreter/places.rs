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
//! * `s.has(x)`, `m.get(k)`, `m.has(k)`, `m.len()`, `len(v)` and
//!   `contains(v, x)` read the variable without copying it.
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
        Some(Value::Array(a) | Value::Tuple(a) | Value::Set(a)) => Ok(Value::Int(a.len() as i64)),
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
        (Some(Value::Set(arr)), Some(val)) => {
            Ok(Value::Bool(arr.iter().any(|v| Value::container_eq(v, val))))
        }
        (Some(Value::Array(arr) | Value::Tuple(arr)), Some(val)) => {
            let needle = format!("{}", val);
            Ok(Value::Bool(arr.iter().any(|v| format!("{}", v) == needle)))
        }
        (Some(Value::Object(map)), Some(Value::String(key))) => {
            Ok(Value::Bool(map.contains_key(key)))
        }
        (Some(Value::Map(pairs)), Some(key)) => Ok(Value::Bool(
            pairs.iter().any(|(k, _)| Value::container_eq(k, key)),
        )),
        _ => Err(RuntimeError::new(
            "contains() requires (string, substring), (array, value), (object, key), or (map, key)",
        )),
    }
}

/// One step of a nested assignment target below its root variable:
/// `.field` or `[index]` (index already evaluated).
enum PlaceStep<'a> {
    Field(&'a str),
    Index(Value),
}

/// What a mutable variable currently holds, for choosing a set fast path.
enum SetReceiver {
    Set,
    Frozen,
    Other,
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
    pub(super) fn try_mutate_in_place(
        &mut self,
        expr: &Expr,
        want_result: bool,
    ) -> Option<Result<Value, RuntimeError>> {
        let Expr::Call { function, args } = expr else {
            return None;
        };
        let (var, method, rest): (&str, &str, &[Expr]) = match function.as_ref() {
            Expr::FieldAccess { object, field } => match object.as_ref() {
                Expr::Ident(v) => (v.as_str(), field.as_str(), args.as_slice()),
                _ => return None,
            },
            Expr::Ident(f)
                if (f == "push" && args.len() == 2) || (f == "pop" && args.len() == 1) =>
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
        match (method, rest.len()) {
            ("push", 1) => {
                let val = match self.eval_expr(&rest[0]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                Some(self.update_var(var, |cur| {
                    match cur {
                        Value::Array(items) => items.push(val),
                        _ => return Err(RuntimeError::new("push() first argument must be array")),
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
                _ => Err(RuntimeError::new("pop() requires array")),
            })),
            ("add", 1) | ("remove", 1) => {
                let receiver = self.env.with_value(var, |v| match v {
                    Value::Set(_) => SetReceiver::Set,
                    Value::Frozen(_) => SetReceiver::Frozen,
                    _ => SetReceiver::Other,
                })?;
                match receiver {
                    SetReceiver::Other => return None,
                    SetReceiver::Frozen => {
                        return Some(Err(RuntimeError::new(if method == "add" {
                            "cannot add to a frozen set"
                        } else {
                            "cannot remove from a frozen set"
                        })))
                    }
                    SetReceiver::Set => {}
                }
                let val = match self.eval_expr(&rest[0]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let adding = method == "add";
                Some(self.update_var(var, |cur| {
                    let Value::Set(items) = cur else {
                        return Err(RuntimeError::new(&format!("{}() requires a set", method)));
                    };
                    if adding {
                        if !items.iter().any(|v| Value::container_eq(v, &val)) {
                            items.push(val);
                        }
                    } else {
                        items.retain(|v| !Value::container_eq(v, &val));
                    }
                    Ok(if want_result {
                        cur.clone()
                    } else {
                        Value::Null
                    })
                }))
            }
            _ => None,
        }
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
    /// the builtins `len(v)` / `contains(v, x)`. Returns `None` when `expr`
    /// is not one of them (or an argument is not effect-free), so the
    /// caller falls back to ordinary dispatch.
    pub(super) fn try_read_in_place(&mut self, expr: &Expr) -> Option<Result<Value, RuntimeError>> {
        let Expr::Call { function, args } = expr else {
            return None;
        };
        match function.as_ref() {
            Expr::FieldAccess { object, field } => {
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
                            Some(Value::Bool(items.iter().any(|v| Value::container_eq(v, x))))
                        }
                        (Value::Map(pairs), "has", Some(key)) => Some(Value::Bool(
                            pairs.iter().any(|(k, _)| Value::container_eq(k, key)),
                        )),
                        (Value::Map(pairs), "get", Some(key)) => Some(
                            pairs
                                .iter()
                                .find(|(k, _)| Value::container_eq(k, key))
                                .map(|(_, v)| v.clone())
                                .unwrap_or(Value::Null),
                        ),
                        (Value::Map(pairs), "len", None) => Some(Value::Int(pairs.len() as i64)),
                        _ => None,
                    })
                    .flatten()
                    .map(Ok)
            }
            Expr::Ident(name) if name == "len" || name == "contains" => {
                let expected = if name == "len" { 1 } else { 2 };
                if args.len() != expected || !self.resolves_to_builtin(name) {
                    return None;
                }
                let Expr::Ident(var) = &args[0] else {
                    return None;
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
                self.env.with_value(var, |v| {
                    if name == "len" {
                        len_of(Some(v))
                    } else {
                        contains_of(Some(v), needle.as_ref())
                    }
                })
            }
            _ => None,
        }
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
                // exactly the value the assignment would store.
                self.try_mutate_in_place(value, false)
                    .map(|result| result.map(|_| ()))
            }
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
                Some(self.env.with_value_mut(x, |cur| {
                    if let (Value::String(s), BinOp::Add) = (&mut *cur, op) {
                        let concat = matches!(
                            crate::semantics::binary(
                                crate::semantics::BinaryOp::Add,
                                crate::semantics::Operand::Str(s),
                                semantic_operand(rhs),
                            ),
                            Ok(crate::semantics::Outcome::Concat)
                        );
                        if concat {
                            use std::fmt::Write as _;
                            let _ = write!(s, "{}", rhs);
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
                if let Value::Object(map) = cur {
                    map.insert(field.to_string(), val);
                }
                Ok(())
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
