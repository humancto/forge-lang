//! Static types and the inference context (type variables + unification).
//!
//! Forge is gradually typed: [`Ty::Any`] is the type of every value the
//! checker cannot pin down (an unannotated parameter, `json.parse(...)`,
//! ...). `Any` is compatible with every type in both directions, so the
//! checker only reports a problem when both sides are known — that is what
//! keeps unannotated programs free of false positives.
//!
//! Inference variables ([`Ty::Var`]) stand for types that are being solved:
//! the type parameters of a generic function at one call site, the element
//! type of an empty array literal, the parameters of a lambda passed to a
//! higher-order builtin. [`InferCtx::assign`] solves them while checking
//! that a value of one type can be used where another is expected.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    /// Unknown / dynamic. Compatible with everything.
    Any,
    /// The type of an expression that never produces a value (`bruh()`,
    /// `exit()`). Assignable to everything.
    Never,
    Int,
    Float,
    String,
    Bool,
    Null,
    Array(Box<Ty>),
    Tuple(Vec<Ty>),
    Set(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    /// A dynamic object (`{ ... }` literals, `Json`, parsed JSON). Field
    /// access on it is unchecked.
    Object,
    Fn(FnTy),
    Option(Box<Ty>),
    Result(Box<Ty>, Box<Ty>),
    /// A user-defined struct, `type` (ADT) or interface, with type arguments.
    Named(String, Vec<Ty>),
    /// A generic type parameter, inside the function or struct declaring it.
    Param(String),
    /// A union alias member set (`type Id = Int | String`).
    Union(Vec<Ty>),
    /// An inference variable (see [`InferCtx`]).
    Var(u32),
}

/// A function type. `required` parameters must be passed; the rest have
/// defaults. `variadic` functions (most output builtins) accept any number
/// of extra arguments of type `params.last()`.
#[derive(Debug, Clone, PartialEq)]
pub struct FnTy {
    pub type_params: Vec<String>,
    pub params: Vec<Ty>,
    pub required: usize,
    pub variadic: bool,
    pub ret: Box<Ty>,
}

impl FnTy {
    pub fn new(params: Vec<Ty>, ret: Ty) -> Self {
        let required = params.len();
        Self {
            type_params: Vec::new(),
            params,
            required,
            variadic: false,
            ret: Box::new(ret),
        }
    }
}

impl Ty {
    pub fn array(elem: Ty) -> Ty {
        Ty::Array(Box::new(elem))
    }

    pub fn option(inner: Ty) -> Ty {
        Ty::Option(Box::new(inner))
    }

    pub fn result(ok: Ty, err: Ty) -> Ty {
        Ty::Result(Box::new(ok), Box::new(err))
    }

    pub fn func(params: Vec<Ty>, ret: Ty) -> Ty {
        Ty::Fn(FnTy::new(params, ret))
    }

    pub fn is_any(&self) -> bool {
        matches!(self, Ty::Any)
    }

    /// True when the type says nothing (Any, an unsolved variable or a
    /// generic parameter): no diagnostic may depend on it.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Ty::Any | Ty::Var(_) | Ty::Param(_))
    }

    /// Does the type mention any inference variable?
    pub fn has_vars(&self) -> bool {
        let mut found = false;
        self.visit(&mut |t| {
            if matches!(t, Ty::Var(_)) {
                found = true;
            }
        });
        found
    }

    /// Pre-order walk over the type and its components.
    pub fn visit(&self, f: &mut dyn FnMut(&Ty)) {
        f(self);
        match self {
            Ty::Array(t) | Ty::Set(t) | Ty::Option(t) => t.visit(f),
            Ty::Map(k, v) | Ty::Result(k, v) => {
                k.visit(f);
                v.visit(f);
            }
            Ty::Tuple(items) | Ty::Union(items) | Ty::Named(_, items) => {
                for t in items {
                    t.visit(f);
                }
            }
            Ty::Fn(fun) => {
                for p in &fun.params {
                    p.visit(f);
                }
                fun.ret.visit(f);
            }
            _ => {}
        }
    }

    /// Replace generic parameters by name.
    pub fn subst_params(&self, subst: &BTreeMap<String, Ty>) -> Ty {
        if subst.is_empty() {
            return self.clone();
        }
        self.map(&mut |t| match t {
            Ty::Param(name) => subst.get(name).cloned(),
            _ => None,
        })
    }

    /// Rebuild the type bottom-up, replacing every node for which `f`
    /// returns `Some`.
    pub fn map(&self, f: &mut dyn FnMut(&Ty) -> Option<Ty>) -> Ty {
        if let Some(t) = f(self) {
            return t;
        }
        match self {
            Ty::Array(t) => Ty::Array(Box::new(t.map(f))),
            Ty::Set(t) => Ty::Set(Box::new(t.map(f))),
            Ty::Option(t) => Ty::Option(Box::new(t.map(f))),
            Ty::Map(k, v) => Ty::Map(Box::new(k.map(f)), Box::new(v.map(f))),
            Ty::Result(k, v) => Ty::Result(Box::new(k.map(f)), Box::new(v.map(f))),
            Ty::Tuple(items) => Ty::Tuple(items.iter().map(|t| t.map(f)).collect()),
            Ty::Union(items) => Ty::Union(items.iter().map(|t| t.map(f)).collect()),
            Ty::Named(n, items) => Ty::Named(n.clone(), items.iter().map(|t| t.map(f)).collect()),
            Ty::Fn(fun) => Ty::Fn(FnTy {
                type_params: fun.type_params.clone(),
                params: fun.params.iter().map(|t| t.map(f)).collect(),
                required: fun.required,
                variadic: fun.variadic,
                ret: Box::new(fun.ret.map(f)),
            }),
            other => other.clone(),
        }
    }

    /// The runtime type name a value of this type reports (`type(v)`), when
    /// it is fixed: used to evaluate operators with the shared semantics.
    pub fn runtime_name(&self) -> Option<&'static str> {
        Some(match self {
            Ty::Int => "Int",
            Ty::Float => "Float",
            Ty::String => "String",
            Ty::Bool => "Bool",
            Ty::Null => "Null",
            Ty::Array(_) => "Array",
            Ty::Tuple(_) => "Tuple",
            Ty::Set(_) => "Set",
            Ty::Map(_, _) => "Map",
            Ty::Object => "Object",
            Ty::Fn(_) => "Function",
            Ty::Result(_, _) => "Result",
            _ => return None,
        })
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Any => write!(f, "Any"),
            Ty::Never => write!(f, "Never"),
            Ty::Int => write!(f, "Int"),
            Ty::Float => write!(f, "Float"),
            Ty::String => write!(f, "String"),
            Ty::Bool => write!(f, "Bool"),
            Ty::Null => write!(f, "Null"),
            Ty::Array(t) => write!(f, "[{}]", t),
            Ty::Tuple(items) => {
                write!(f, "(")?;
                for (i, t) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", t)?;
                }
                if items.len() == 1 {
                    write!(f, ",")?;
                }
                write!(f, ")")
            }
            Ty::Set(t) => write!(f, "Set<{}>", t),
            Ty::Map(k, v) => write!(f, "Map<{}, {}>", k, v),
            Ty::Object => write!(f, "Object"),
            Ty::Fn(fun) => {
                write!(f, "fn(")?;
                for (i, p) in fun.params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    if fun.variadic && i + 1 == fun.params.len() {
                        write!(f, "...")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, ") -> {}", fun.ret)
            }
            Ty::Option(t) => match t.as_ref() {
                Ty::Fn(_) | Ty::Union(_) => write!(f, "?({})", t),
                _ => write!(f, "?{}", t),
            },
            Ty::Result(ok, err) => write!(f, "Result<{}, {}>", ok, err),
            Ty::Named(name, args) => {
                write!(f, "{}", name)?;
                if !args.is_empty() {
                    write!(f, "<")?;
                    for (i, a) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", a)?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
            Ty::Param(name) => write!(f, "{}", name),
            Ty::Union(items) => {
                for (i, t) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{}", t)?;
                }
                Ok(())
            }
            // Unsolved variables print as `Any`: to a user they are unknown.
            Ty::Var(_) => write!(f, "Any"),
        }
    }
}

/// Type-variable store. Variables are created per call site / literal and
/// solved by [`InferCtx::assign`].
#[derive(Debug, Default, Clone)]
pub struct InferCtx {
    vars: Vec<Option<Ty>>,
}

/// A saved state of the variable store (see [`InferCtx::snapshot`]).
pub struct Snapshot(Vec<Option<Ty>>);

impl InferCtx {
    pub fn fresh(&mut self) -> Ty {
        self.vars.push(None);
        Ty::Var((self.vars.len() - 1) as u32)
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot(self.vars.clone())
    }

    pub fn rollback(&mut self, snapshot: Snapshot) {
        self.vars = snapshot.0;
    }

    fn binding(&self, v: u32) -> Option<&Ty> {
        self.vars.get(v as usize).and_then(|b| b.as_ref())
    }

    /// Follow variable bindings at the top level only.
    pub fn shallow(&self, ty: &Ty) -> Ty {
        let mut current = ty.clone();
        let mut guard = 0;
        while let Ty::Var(v) = current {
            match self.binding(v) {
                Some(t) if guard < 64 => {
                    current = t.clone();
                    guard += 1;
                }
                _ => break,
            }
        }
        current
    }

    /// Substitute every solved variable; unsolved ones stay `Var`.
    pub fn resolve(&self, ty: &Ty) -> Ty {
        self.resolve_depth(ty, 0)
    }

    fn resolve_depth(&self, ty: &Ty, depth: usize) -> Ty {
        if depth > 32 {
            return Ty::Any;
        }
        ty.map(&mut |t| match t {
            Ty::Var(v) => Some(match self.binding(*v) {
                Some(bound) => self.resolve_depth(bound, depth + 1),
                None => t.clone(),
            }),
            _ => None,
        })
    }

    /// Like [`InferCtx::resolve`], but unsolved variables become `Any`
    /// (for display and for storing a final type).
    pub fn finalize(&self, ty: &Ty) -> Ty {
        self.resolve(ty).map(&mut |t| match t {
            Ty::Var(_) => Some(Ty::Any),
            _ => None,
        })
    }

    fn occurs(&self, v: u32, ty: &Ty) -> bool {
        let resolved = self.resolve(ty);
        let mut found = false;
        resolved.visit(&mut |t| {
            if *t == Ty::Var(v) {
                found = true;
            }
        });
        found
    }

    fn bind(&mut self, v: u32, ty: &Ty) -> bool {
        if *ty == Ty::Var(v) {
            return true;
        }
        if self.occurs(v, ty) {
            return false;
        }
        if let Some(slot) = self.vars.get_mut(v as usize) {
            *slot = Some(ty.clone());
        }
        true
    }

    /// Can a value of type `actual` be used where `expected` is required?
    /// Solves variables on either side as a side effect. The rules:
    ///
    /// * `Any` (either side) and `Never` (actual) always fit;
    /// * `Int` fits `Float` (numeric promotion), not the reverse;
    /// * `?T` accepts `null`, `T` and `?U` with `U` fitting `T`;
    /// * a union accepts any member; a union value fits if every member does;
    /// * structs and objects are interchangeable (`Object` is the dynamic
    ///   view of a struct instance);
    /// * function types are contravariant in parameters, covariant in the
    ///   result;
    /// * collections compare element-wise.
    pub fn assign(&mut self, expected: &Ty, actual: &Ty) -> bool {
        let e = self.shallow(expected);
        let a = self.shallow(actual);
        match (&e, &a) {
            (Ty::Any, _) | (_, Ty::Any) | (_, Ty::Never) => true,
            (Ty::Var(v), _) => self.bind(*v, &a),
            (_, Ty::Var(v)) => self.bind(*v, &e),
            (Ty::Param(x), Ty::Param(y)) => x == y,
            (Ty::Float, Ty::Int) => true,
            (Ty::Option(_), Ty::Null) => true,
            (Ty::Option(inner), Ty::Option(actual_inner)) => {
                let (inner, actual_inner) = (inner.clone(), actual_inner.clone());
                self.assign(&inner, &actual_inner)
            }
            (Ty::Option(inner), _) => {
                let inner = inner.clone();
                self.assign(&inner, &a)
            }
            (Ty::Union(members), _) => {
                let members = members.clone();
                for m in &members {
                    let snap = self.snapshot();
                    if self.assign(m, &a) {
                        return true;
                    }
                    self.rollback(snap);
                }
                false
            }
            (_, Ty::Union(members)) => {
                let members = members.clone();
                members.iter().all(|m| self.assign(&e, m))
            }
            (Ty::Array(x), Ty::Array(y)) | (Ty::Set(x), Ty::Set(y)) => {
                let (x, y) = (x.clone(), y.clone());
                self.assign(&x, &y)
            }
            (Ty::Map(k1, v1), Ty::Map(k2, v2)) | (Ty::Result(k1, v1), Ty::Result(k2, v2)) => {
                let (k1, v1, k2, v2) = (k1.clone(), v1.clone(), k2.clone(), v2.clone());
                self.assign(&k1, &k2) && self.assign(&v1, &v2)
            }
            (Ty::Tuple(xs), Ty::Tuple(ys)) => {
                if xs.len() != ys.len() {
                    return false;
                }
                let (xs, ys) = (xs.clone(), ys.clone());
                xs.iter().zip(&ys).all(|(x, y)| self.assign(x, y))
            }
            (Ty::Named(n1, args1), Ty::Named(n2, args2)) => {
                if n1 != n2 {
                    return false;
                }
                let (args1, args2) = (args1.clone(), args2.clone());
                args1.iter().zip(&args2).all(|(x, y)| self.assign(x, y))
            }
            (Ty::Object, Ty::Named(..)) | (Ty::Named(..), Ty::Object) => true,
            (Ty::Fn(f1), Ty::Fn(f2)) => {
                let (f1, f2) = (f1.clone(), f2.clone());
                // A function that requires more arguments than the expected
                // type supplies cannot be called through it.
                if f2.required > f1.params.len() && !f1.variadic {
                    return false;
                }
                let params_ok = f1
                    .params
                    .iter()
                    .zip(&f2.params)
                    .all(|(p1, p2)| self.assign(p2, p1));
                params_ok && self.assign(&f1.ret, &f2.ret)
            }
            _ => e == a,
        }
    }

    /// The least type covering both (for `if`/`match` branches, array
    /// elements, return statements). Falls back to `Any` when the two are
    /// unrelated — the checker never invents a union nobody wrote.
    pub fn join(&mut self, a: &Ty, b: &Ty) -> Ty {
        let a = self.resolve(a);
        let b = self.resolve(b);
        match (&a, &b) {
            (Ty::Never, _) => b,
            (_, Ty::Never) => a,
            _ if a == b => a,
            (Ty::Int, Ty::Float) | (Ty::Float, Ty::Int) => Ty::Float,
            (Ty::Null, Ty::Option(_)) => b,
            (Ty::Option(_), Ty::Null) => a,
            (Ty::Null, other) | (other, Ty::Null) if !other.is_unknown() => {
                Ty::option(other.clone())
            }
            (Ty::Option(x), Ty::Option(y)) => {
                let (x, y) = (x.clone(), y.clone());
                Ty::option(self.join(&x, &y))
            }
            (Ty::Option(x), other) | (other, Ty::Option(x)) => {
                let (x, other) = (x.clone(), other.clone());
                Ty::option(self.join(&x, &other))
            }
            (Ty::Array(x), Ty::Array(y)) => {
                let (x, y) = (x.clone(), y.clone());
                Ty::array(self.join(&x, &y))
            }
            (Ty::Result(o1, e1), Ty::Result(o2, e2)) => {
                let (o1, e1, o2, e2) = (o1.clone(), e1.clone(), o2.clone(), e2.clone());
                Ty::result(self.join(&o1, &o2), self.join(&e1, &e2))
            }
            (Ty::Var(_), _) => {
                let (a2, b2) = (a.clone(), b.clone());
                if self.assign(&a2, &b2) {
                    self.resolve(&a2)
                } else {
                    Ty::Any
                }
            }
            (_, Ty::Var(_)) => {
                let (a2, b2) = (a.clone(), b.clone());
                if self.assign(&b2, &a2) {
                    self.resolve(&b2)
                } else {
                    Ty::Any
                }
            }
            _ => Ty::Any,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_fits_everything_both_ways() {
        let mut cx = InferCtx::default();
        assert!(cx.assign(&Ty::Int, &Ty::Any));
        assert!(cx.assign(&Ty::Any, &Ty::String));
    }

    #[test]
    fn int_promotes_to_float_only() {
        let mut cx = InferCtx::default();
        assert!(cx.assign(&Ty::Float, &Ty::Int));
        assert!(!cx.assign(&Ty::Int, &Ty::Float));
    }

    #[test]
    fn option_accepts_null_inner_and_option() {
        let mut cx = InferCtx::default();
        let opt = Ty::option(Ty::Int);
        assert!(cx.assign(&opt, &Ty::Null));
        assert!(cx.assign(&opt, &Ty::Int));
        assert!(cx.assign(&opt, &Ty::option(Ty::Int)));
        assert!(!cx.assign(&opt, &Ty::String));
        assert!(!cx.assign(&Ty::Int, &opt));
    }

    #[test]
    fn variables_are_solved_by_assignment() {
        let mut cx = InferCtx::default();
        let t = cx.fresh();
        assert!(cx.assign(&Ty::array(t.clone()), &Ty::array(Ty::String)));
        assert_eq!(cx.resolve(&t), Ty::String);
        // Once solved, the variable constrains later uses.
        assert!(!cx.assign(&t, &Ty::Int));
    }

    #[test]
    fn union_membership() {
        let mut cx = InferCtx::default();
        let u = Ty::Union(vec![Ty::String, Ty::Int]);
        assert!(cx.assign(&u, &Ty::Int));
        assert!(!cx.assign(&u, &Ty::Bool));
        assert!(cx.assign(&Ty::Float, &Ty::Union(vec![Ty::Int, Ty::Float])));
    }

    #[test]
    fn function_types_are_contravariant_in_parameters() {
        let mut cx = InferCtx::default();
        let takes_float = Ty::func(vec![Ty::Float], Ty::Int);
        let takes_int = Ty::func(vec![Ty::Int], Ty::Int);
        // A fn(Float) can stand in for a fn(Int), not the other way round.
        assert!(cx.assign(&takes_int, &takes_float));
        assert!(!cx.assign(&takes_float, &takes_int));
    }

    #[test]
    fn join_widens_numbers_and_nulls() {
        let mut cx = InferCtx::default();
        assert_eq!(cx.join(&Ty::Int, &Ty::Float), Ty::Float);
        assert_eq!(cx.join(&Ty::Null, &Ty::String), Ty::option(Ty::String));
        assert_eq!(cx.join(&Ty::Int, &Ty::String), Ty::Any);
    }

    #[test]
    fn display_is_annotation_syntax() {
        assert_eq!(Ty::array(Ty::Int).to_string(), "[Int]");
        assert_eq!(Ty::option(Ty::String).to_string(), "?String");
        assert_eq!(
            Ty::func(vec![Ty::Int, Ty::String], Ty::Bool).to_string(),
            "fn(Int, String) -> Bool"
        );
        assert_eq!(Ty::Tuple(vec![Ty::Int]).to_string(), "(Int,)");
    }
}
