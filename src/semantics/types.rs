//! Runtime type checks for annotations (`forge run --strict`).
//!
//! Under `--strict`, the front end instruments every function whose
//! parameters or result are annotated (`typechecker::enforce`): the
//! instrumented program calls the hidden stdlib member
//! `__types.check(value, type, context)` at each boundary. Both engines
//! dispatch module members to the same implementation (VM values are
//! converted with `args_to_interp`), so the rule here — which value fits
//! which type, and the error text — is the single source of truth for both.
//!
//! Types are passed in a small canonical notation produced by the checker
//! (aliases already expanded, generics erased to `Any`):
//!
//! ```text
//! T := Any | Int | Float | String | Bool | Null | Object | Fn
//!    | [T] | ?T | Set<T> | Map<T, T> | Object<T> | Result<T, T> | (T, ...)
//!    | Union<T, ...> | Named:Name
//! ```
//!
//! `Object<T>` is a string-keyed object (an object literal, parsed JSON)
//! whose every value fits `T`; internal `__` keys (`__type__`, ...) are not
//! values. `Map<K, V>` is the `Map` value type, never an object.

use crate::interpreter::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeType {
    Any,
    Int,
    Float,
    String,
    Bool,
    Null,
    Object,
    Fn,
    Array(Box<RuntimeType>),
    Option(Box<RuntimeType>),
    Set(Box<RuntimeType>),
    Map(Box<RuntimeType>, Box<RuntimeType>),
    /// `Object<T>`: an object whose values all fit `T`.
    ObjectOf(Box<RuntimeType>),
    Result(Box<RuntimeType>, Box<RuntimeType>),
    Tuple(Vec<RuntimeType>),
    Union(Vec<RuntimeType>),
    /// A struct or `type` (ADT): an object whose `__type__` is the name.
    Named(String),
}

impl std::fmt::Display for RuntimeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list = |f: &mut std::fmt::Formatter<'_>, items: &[RuntimeType], sep: &str| {
            for (i, t) in items.iter().enumerate() {
                if i > 0 {
                    write!(f, "{}", sep)?;
                }
                write!(f, "{}", t)?;
            }
            Ok(())
        };
        match self {
            RuntimeType::Any => write!(f, "Any"),
            RuntimeType::Int => write!(f, "Int"),
            RuntimeType::Float => write!(f, "Float"),
            RuntimeType::String => write!(f, "String"),
            RuntimeType::Bool => write!(f, "Bool"),
            RuntimeType::Null => write!(f, "Null"),
            RuntimeType::Object => write!(f, "Object"),
            RuntimeType::Fn => write!(f, "function"),
            RuntimeType::Array(t) => write!(f, "[{}]", t),
            RuntimeType::Option(t) => write!(f, "?{}", t),
            RuntimeType::Set(t) => write!(f, "Set<{}>", t),
            RuntimeType::Map(k, v) => write!(f, "Map<{}, {}>", k, v),
            RuntimeType::ObjectOf(v) => write!(f, "Object<{}>", v),
            RuntimeType::Result(o, e) => write!(f, "Result<{}, {}>", o, e),
            RuntimeType::Tuple(items) => {
                write!(f, "(")?;
                list(f, items, ", ")?;
                write!(f, ")")
            }
            RuntimeType::Union(items) => list(f, items, " | "),
            RuntimeType::Named(n) => write!(f, "{}", n),
        }
    }
}

/// Parse the canonical notation. Errors name the offending input.
pub fn parse(src: &str) -> Result<RuntimeType, String> {
    let chars: Vec<char> = src.chars().filter(|c| !c.is_whitespace()).collect();
    let mut pos = 0;
    let ty = parse_at(&chars, &mut pos, 0)?;
    if pos != chars.len() {
        return Err(format!("invalid runtime type '{}'", src));
    }
    Ok(ty)
}

fn parse_at(c: &[char], pos: &mut usize, depth: usize) -> Result<RuntimeType, String> {
    if depth > 64 {
        return Err("runtime type nested too deeply".to_string());
    }
    let eat = |pos: &mut usize, ch: char| -> Result<(), String> {
        if c.get(*pos) == Some(&ch) {
            *pos += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' in runtime type", ch))
        }
    };
    let list = |pos: &mut usize, close: char| -> Result<Vec<RuntimeType>, String> {
        let mut items = vec![parse_at(c, pos, depth + 1)?];
        while c.get(*pos) == Some(&',') {
            *pos += 1;
            items.push(parse_at(c, pos, depth + 1)?);
        }
        eat(pos, close)?;
        Ok(items)
    };
    match c.get(*pos) {
        Some('[') => {
            *pos += 1;
            let inner = parse_at(c, pos, depth + 1)?;
            eat(pos, ']')?;
            return Ok(RuntimeType::Array(Box::new(inner)));
        }
        Some('?') => {
            *pos += 1;
            return Ok(RuntimeType::Option(Box::new(parse_at(c, pos, depth + 1)?)));
        }
        Some('(') => {
            *pos += 1;
            return Ok(RuntimeType::Tuple(list(pos, ')')?));
        }
        _ => {}
    }
    let start = *pos;
    while c
        .get(*pos)
        .is_some_and(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == ':')
    {
        *pos += 1;
    }
    let word: String = c[start..*pos].iter().collect();
    if let Some(name) = word.strip_prefix("Named:") {
        if name.is_empty() {
            return Err("empty type name".to_string());
        }
        return Ok(RuntimeType::Named(name.to_string()));
    }
    if c.get(*pos) == Some(&'<') {
        *pos += 1;
        let mut args = list(pos, '>')?;
        let two = |args: &mut Vec<RuntimeType>| -> Result<(RuntimeType, RuntimeType), String> {
            if args.len() != 2 {
                return Err(format!("{} takes two type arguments", word));
            }
            let b = args.pop().unwrap_or(RuntimeType::Any);
            let a = args.pop().unwrap_or(RuntimeType::Any);
            Ok((a, b))
        };
        return match word.as_str() {
            "Set" if args.len() == 1 => Ok(RuntimeType::Set(Box::new(args.remove(0)))),
            "Object" if args.len() == 1 => Ok(RuntimeType::ObjectOf(Box::new(args.remove(0)))),
            "Map" => {
                let (k, v) = two(&mut args)?;
                Ok(RuntimeType::Map(Box::new(k), Box::new(v)))
            }
            "Result" => {
                let (o, e) = two(&mut args)?;
                Ok(RuntimeType::Result(Box::new(o), Box::new(e)))
            }
            "Union" => Ok(RuntimeType::Union(args)),
            _ => Err(format!("unknown runtime type '{}'", word)),
        };
    }
    Ok(match word.as_str() {
        "Any" => RuntimeType::Any,
        "Int" => RuntimeType::Int,
        "Float" => RuntimeType::Float,
        "String" => RuntimeType::String,
        "Bool" => RuntimeType::Bool,
        "Null" => RuntimeType::Null,
        "Object" => RuntimeType::Object,
        "Fn" => RuntimeType::Fn,
        other => return Err(format!("unknown runtime type '{}'", other)),
    })
}

thread_local! {
    static CACHE: RefCell<HashMap<String, Rc<RuntimeType>>> = RefCell::new(HashMap::new());
}

/// Parse with a per-thread cache (instrumented functions check on every
/// call, always with the same few type strings).
pub fn parse_cached(src: &str) -> Result<Rc<RuntimeType>, String> {
    if let Some(hit) = CACHE.with(|c| c.borrow().get(src).cloned()) {
        return Ok(hit);
    }
    let ty = Rc::new(parse(src)?);
    CACHE.with(|c| c.borrow_mut().insert(src.to_string(), ty.clone()));
    Ok(ty)
}

/// The values of an object that `Object<T>` constrains: every field except
/// the internal `__` ones (`__type__`, `__variant__`, `__module__`).
pub fn is_object_value_key(key: &str) -> bool {
    !key.starts_with("__")
}

fn object_values(map: &indexmap::IndexMap<String, Value>) -> impl Iterator<Item = &Value> {
    map.iter()
        .filter(|(k, _)| is_object_value_key(k))
        .map(|(_, v)| v)
}

fn type_tag(map: &indexmap::IndexMap<String, Value>) -> Option<&str> {
    match map.get("__type__") {
        Some(Value::String(t)) => Some(t),
        _ => None,
    }
}

/// Does `value` fit `ty`? Ints fit `Float`; `?T` accepts null, `None`,
/// `Some(t)` and a plain `t`; collections are checked element by element.
pub fn fits(value: &Value, ty: &RuntimeType) -> bool {
    use RuntimeType as T;
    if let Value::Frozen(inner) = value {
        return fits(inner, ty);
    }
    match ty {
        T::Any => true,
        T::Int => matches!(value, Value::Int(_)),
        T::Float => matches!(value, Value::Int(_) | Value::Float(_)),
        T::String => matches!(value, Value::String(_)),
        T::Bool => matches!(value, Value::Bool(_)),
        T::Null => matches!(value, Value::Null),
        T::Object => matches!(value, Value::Object(_)),
        T::Fn => matches!(
            value,
            Value::Function(_) | Value::Lambda { .. } | Value::BuiltIn(_)
        ),
        T::Array(elem) => match value {
            Value::Array(items) => items.iter().all(|v| fits(v, elem)),
            _ => false,
        },
        T::Set(elem) => match value {
            Value::Set(items) => items.iter().all(|v| fits(v, elem)),
            _ => false,
        },
        T::Map(k, v) => match value {
            Value::Map(pairs) => pairs.iter().all(|(a, b)| fits(a, k) && fits(b, v)),
            _ => false,
        },
        T::ObjectOf(v) => match value {
            Value::Object(map) => object_values(map).all(|x| fits(x, v)),
            _ => false,
        },
        T::Tuple(items) => match value {
            Value::Tuple(vals) => {
                vals.len() == items.len() && vals.iter().zip(items).all(|(v, t)| fits(v, t))
            }
            _ => false,
        },
        T::Option(inner) => match value {
            Value::Null | Value::None => true,
            Value::Some(v) => fits(v, inner),
            // The VM's Option values reach here as tagged objects
            // (`{__type__: "Option", __variant__: "Some", _0: v}`).
            Value::Object(map) if type_tag(map) == Some("Option") => {
                match (map.get("__variant__"), map.get("_0")) {
                    (Some(Value::String(v)), Some(payload)) if v == "Some" => fits(payload, inner),
                    (Some(Value::String(v)), _) => v == "None",
                    _ => false,
                }
            }
            other => fits(other, inner),
        },
        T::Result(ok, err) => match value {
            Value::ResultOk(v) => fits(v, ok),
            Value::ResultErr(e) => fits(e, err),
            _ => false,
        },
        T::Union(members) => members.iter().any(|m| fits(value, m)),
        T::Named(name) => match value {
            Value::Object(map) => type_tag(map) == Some(name.as_str()),
            _ => false,
        },
    }
}

/// Int → Float widening of a value that fits `ty` ([`fits`]): every Int at a
/// position `ty` declares `Float` becomes that Float (`3` checked against
/// `Float` is `3.0`, `[1, 2]` against `[Float]` is `[1.0, 2.0]`), so code
/// written against a declared `Float` never does integer arithmetic.
/// `None` when nothing changes. A union keeps an Int that one of its
/// members accepts as it is (`Int | Float`); otherwise the first member
/// the value fits decides. Sets and map keys are left alone (widening
/// could merge two elements).
pub fn widen(value: &Value, ty: &RuntimeType) -> Option<Value> {
    use RuntimeType as T;
    if !mentions_float(ty) {
        return None;
    }
    let _level = crate::runtime::recursion::enter_value_level()?;
    let all = |items: &[Value], elem: &RuntimeType| -> Option<Vec<Value>> {
        let changed: Vec<Option<Value>> = items.iter().map(|v| widen(v, elem)).collect();
        if changed.iter().all(Option::is_none) {
            return None;
        }
        Some(
            items
                .iter()
                .zip(changed)
                .map(|(v, w)| w.unwrap_or_else(|| v.clone()))
                .collect(),
        )
    };
    match (ty, value) {
        (_, Value::Frozen(inner)) => widen(inner, ty).map(|v| Value::Frozen(Box::new(v))),
        (T::Float, Value::Int(n)) => Some(Value::Float(*n as f64)),
        (T::Array(elem), Value::Array(items)) => all(items, elem).map(Value::Array),
        (T::Tuple(types), Value::Tuple(items)) if types.len() == items.len() => {
            let changed: Vec<Option<Value>> =
                items.iter().zip(types).map(|(v, t)| widen(v, t)).collect();
            if changed.iter().all(Option::is_none) {
                return None;
            }
            Some(Value::Tuple(
                items
                    .iter()
                    .zip(changed)
                    .map(|(v, w)| w.unwrap_or_else(|| v.clone()))
                    .collect(),
            ))
        }
        (T::Map(_, vt), Value::Map(pairs)) => {
            let values: Vec<Value> = pairs.iter().map(|(_, v)| v.clone()).collect();
            let widened = all(&values, vt)?;
            Some(Value::Map(
                pairs
                    .iter()
                    .zip(widened)
                    .map(|((k, _), v)| (k.clone(), v))
                    .collect(),
            ))
        }
        (T::ObjectOf(vt), Value::Object(map)) => {
            let changed: Vec<(String, Value)> = map
                .iter()
                .filter(|(k, _)| is_object_value_key(k))
                .filter_map(|(k, v)| widen(v, vt).map(|w| (k.clone(), w)))
                .collect();
            if changed.is_empty() {
                return None;
            }
            let mut map = map.clone();
            for (k, v) in changed {
                map.insert(k, v);
            }
            Some(Value::Object(map))
        }
        (T::Result(ok, _), Value::ResultOk(v)) => {
            widen(v, ok).map(|v| Value::ResultOk(Box::new(v)))
        }
        (T::Result(_, err), Value::ResultErr(v)) => {
            widen(v, err).map(|v| Value::ResultErr(Box::new(v)))
        }
        (T::Option(_), Value::Null | Value::None) => None,
        (T::Option(inner), Value::Some(v)) => widen(v, inner).map(|v| Value::Some(Box::new(v))),
        (T::Option(inner), Value::Object(map)) if type_tag(map) == Some("Option") => {
            let widened = widen(map.get("_0")?, inner)?;
            let mut map = map.clone();
            map.insert("_0".to_string(), widened);
            Some(Value::Object(map))
        }
        (T::Option(inner), other) => widen(other, inner),
        (T::Union(members), v) => {
            if members.iter().any(|m| fits(v, m) && widen(v, m).is_none()) {
                return None;
            }
            members
                .iter()
                .find(|m| fits(v, m))
                .and_then(|m| widen(v, m))
        }
        _ => None,
    }
}

/// Does `ty` declare `Float` anywhere a value can be widened?
pub fn mentions_float(ty: &RuntimeType) -> bool {
    use RuntimeType as T;
    match ty {
        T::Float => true,
        T::Array(t) | T::Option(t) => mentions_float(t),
        T::Map(_, v) | T::ObjectOf(v) => mentions_float(v),
        T::Result(o, e) => mentions_float(o) || mentions_float(e),
        T::Tuple(items) | T::Union(items) => items.iter().any(mentions_float),
        T::Any
        | T::Int
        | T::String
        | T::Bool
        | T::Null
        | T::Object
        | T::Fn
        | T::Set(_)
        | T::Named(_) => false,
    }
}

/// What one argument says about a generic type parameter at run time, in
/// canonical notation: its outer type (`Int`, `Named:Point`, `[Any]`, ...).
/// `None` when it says nothing (`Option` values, streams, tasks,
/// channels). Element types are not looked at, so binding is O(1) per
/// argument.
pub fn param_shape(value: &Value) -> Option<String> {
    Some(match value {
        Value::Frozen(inner) => return param_shape(inner),
        Value::Int(_) => "Int".into(),
        Value::Float(_) => "Float".into(),
        Value::String(_) => "String".into(),
        Value::Bool(_) => "Bool".into(),
        Value::Null => "Null".into(),
        Value::Object(map) => match type_tag(map) {
            Some(name) => format!("Named:{}", name),
            None => "Object".into(),
        },
        Value::Function(_) | Value::Lambda { .. } | Value::BuiltIn(_) => "Fn".into(),
        Value::Array(_) => "[Any]".into(),
        Value::Set(_) => "Set<Any>".into(),
        Value::Map(_) => "Map<Any,Any>".into(),
        Value::ResultOk(_) | Value::ResultErr(_) => "Result<Any,Any>".into(),
        Value::Tuple(items) => format!("({})", vec!["Any"; items.len()].join(",")),
        Value::Some(_)
        | Value::None
        | Value::Stream(_)
        | Value::TaskHandle(_)
        | Value::Channel(_) => return None,
    })
}

/// Bind a generic type parameter from the arguments at the positions
/// declared exactly `T` (`fn pair<T>(a: T, b: T)`), consistently within one
/// call: every argument must have the shape of the first one that has a
/// shape, except that Int and Float make `Float` (an Int fits Float). On
/// success, the canonical type `T` stands for in this call (`Any` when no
/// argument says); on failure, `(index, first_index, bound)`: argument
/// `index` does not fit the type bound by argument `first_index`.
pub fn bind_param(values: &[Value]) -> Result<String, (usize, usize, String)> {
    bind_shapes(values.iter().map(param_shape))
}

/// [`bind_param`] over the arguments' shapes ([`param_shape`]); engines
/// that compute shapes on their own values (the VM) share the rule.
pub fn bind_shapes(
    shapes: impl IntoIterator<Item = Option<String>>,
) -> Result<String, (usize, usize, String)> {
    let mut bound: Option<(usize, String)> = None;
    for (i, shape) in shapes.into_iter().enumerate() {
        let Some(shape) = shape else {
            continue;
        };
        match &mut bound {
            None => bound = Some((i, shape)),
            Some((_, b)) if *b == shape => {}
            Some((_, b)) if b == "Float" && shape == "Int" => {}
            Some((_, b)) if b == "Int" && shape == "Float" => *b = "Float".into(),
            Some((first, b)) => return Err((i, *first, b.clone())),
        }
    }
    Ok(bound.map_or_else(|| "Any".to_string(), |(_, b)| b))
}

/// The run-time error for an argument that does not fit the type its
/// generic parameter was bound to earlier in the same call.
pub fn param_violation(
    function: &str,
    param: &str,
    type_param: &str,
    first: &str,
    bound: &str,
    value: &Value,
) -> String {
    let bound = parse(bound).map_or_else(|_| bound.to_string(), |t| t.to_string());
    format!(
        "type error: argument '{}' of '{}' must be {} like argument '{}' (type parameter {}), got {}",
        param,
        function,
        bound,
        first,
        type_param,
        describe(value)
    )
}

/// The type name shown for a value in a violation message.
pub fn describe(value: &Value) -> String {
    match value {
        Value::Int(_) => "Int".into(),
        Value::Float(_) => "Float".into(),
        Value::String(_) => "String".into(),
        Value::Bool(_) => "Bool".into(),
        Value::Null => "Null".into(),
        Value::Array(_) => "Array".into(),
        Value::Tuple(_) => "Tuple".into(),
        Value::Set(_) => "Set".into(),
        Value::Map(_) => "Map".into(),
        Value::Object(map) => type_tag(map).unwrap_or("Object").to_string(),
        Value::Function(_) | Value::Lambda { .. } | Value::BuiltIn(_) => "function".into(),
        Value::ResultOk(_) | Value::ResultErr(_) => "Result".into(),
        Value::Some(_) | Value::None => "Option".into(),
        Value::Frozen(inner) => describe(inner),
        Value::Stream(_) => "Stream".into(),
        Value::TaskHandle(_) => "Task".into(),
        Value::Channel(_) => "Channel".into(),
    }
}

/// The run-time error for a value that does not fit its annotation.
/// `context` says where (`argument 'x' of 'add'`, `return value of 'add'`).
pub fn violation(context: &str, expected: &RuntimeType, value: &Value) -> String {
    format!(
        "type error: {} must be {}, got {}",
        context,
        expected,
        describe(value)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> RuntimeType {
        parse(s).unwrap_or_else(|e| panic!("{}: {}", s, e))
    }

    #[test]
    fn parses_canonical_notation() {
        assert_eq!(t("[Int]"), RuntimeType::Array(Box::new(RuntimeType::Int)));
        assert_eq!(
            t("Map<String, ?Int>"),
            RuntimeType::Map(
                Box::new(RuntimeType::String),
                Box::new(RuntimeType::Option(Box::new(RuntimeType::Int)))
            )
        );
        assert_eq!(
            t("Union<Int,Named:Point>"),
            RuntimeType::Union(vec![RuntimeType::Int, RuntimeType::Named("Point".into())])
        );
        assert_eq!(t("(Int, String)").to_string(), "(Int, String)");
        assert_eq!(
            t("Object<[Int]>"),
            RuntimeType::ObjectOf(Box::new(RuntimeType::Array(Box::new(RuntimeType::Int))))
        );
        assert_eq!(t("Object<?Float>").to_string(), "Object<?Float>");
        assert!(parse("Object<Int, Int>").is_err());
        assert!(parse("Strin").is_err());
        assert!(parse("[Int").is_err());
        assert!(parse("Int extra").is_err());
    }

    #[test]
    fn fits_table() {
        let point = {
            let mut m = indexmap::IndexMap::new();
            m.insert("__type__".to_string(), Value::String("Point".into()));
            Value::Object(m)
        };
        let cases: Vec<(Value, &str, bool)> = vec![
            (Value::Int(1), "Int", true),
            (Value::Float(1.5), "Int", false),
            (Value::Int(1), "Float", true),
            (Value::String("a".into()), "?String", true),
            (Value::Null, "?String", true),
            (Value::None, "?String", true),
            (Value::Some(Box::new(Value::Int(1))), "?String", false),
            (
                Value::Array(vec![Value::Int(1), Value::Int(2)]),
                "[Int]",
                true,
            ),
            (
                Value::Array(vec![Value::Int(1), Value::String("x".into())]),
                "[Int]",
                false,
            ),
            (point.clone(), "Named:Point", true),
            (point.clone(), "Named:Other", false),
            (point.clone(), "Object", true),
            (point, "Object<Int>", true),
            (
                Value::Object(
                    [
                        ("a".to_string(), Value::Int(1)),
                        ("b".to_string(), Value::Int(2)),
                    ]
                    .into_iter()
                    .collect(),
                ),
                "Object<Int>",
                true,
            ),
            (
                Value::Object(
                    [
                        ("a".to_string(), Value::Int(1)),
                        ("b".to_string(), Value::String("x".into())),
                    ]
                    .into_iter()
                    .collect(),
                ),
                "Object<Int>",
                false,
            ),
            (Value::Object(Default::default()), "Object<String>", true),
            (Value::Map(vec![]), "Object<Int>", false),
            (
                Value::Object([("a".to_string(), Value::Int(1))].into_iter().collect()),
                "Map<String, Int>",
                false,
            ),
            (
                Value::ResultErr(Box::new(Value::String("e".into()))),
                "Result<Int, String>",
                true,
            ),
            (Value::Bool(true), "Union<Int,String>", false),
            (Value::Frozen(Box::new(Value::Int(3))), "Int", true),
        ];
        for (value, ty, expected) in cases {
            assert_eq!(fits(&value, &t(ty)), expected, "{:?} : {}", value, ty);
        }
    }

    #[test]
    fn generic_parameters_bind_consistently() {
        let s = |x: &str| Value::String(x.into());
        assert_eq!(bind_param(&[s("a"), s("b")]), Ok("String".to_string()));
        assert_eq!(
            bind_param(&[Value::Int(1), Value::Float(2.5)]),
            Ok("Float".to_string())
        );
        assert_eq!(
            bind_param(&[Value::Float(2.5), Value::Int(1)]),
            Ok("Float".to_string())
        );
        assert_eq!(
            bind_param(&[Value::None, Value::None]),
            Ok("Any".to_string())
        );
        assert_eq!(
            bind_param(&[Value::Int(1), s("x")]),
            Err((1, 0, "Int".to_string()))
        );
        assert_eq!(
            bind_param(&[Value::Array(vec![]), Value::Array(vec![Value::Int(1)])]),
            Ok("[Any]".to_string())
        );
        // Every shape is valid canonical notation.
        for v in [
            Value::Tuple(vec![Value::Int(1), Value::Null]),
            Value::Map(vec![]),
        ] {
            let shape = param_shape(&v).expect("has a shape");
            assert!(fits(&v, &t(&shape)), "{}", shape);
        }
        assert_eq!(
            param_violation("pair", "b", "T", "a", "Int", &s("x")),
            "type error: argument 'b' of 'pair' must be Int like argument 'a' (type parameter T), got String"
        );
    }

    #[test]
    fn violation_message() {
        assert_eq!(
            violation("argument 'x' of 'f'", &t("Int"), &Value::String("s".into())),
            "type error: argument 'x' of 'f' must be Int, got String"
        );
    }
}
