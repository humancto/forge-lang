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
//!    | [T] | ?T | Set<T> | Map<T, T> | Result<T, T> | (T, ...)
//!    | Union<T, ...> | Named:Name
//! ```

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
        T::Tuple(items) => match value {
            Value::Tuple(vals) => {
                vals.len() == items.len() && vals.iter().zip(items).all(|(v, t)| fits(v, t))
            }
            _ => false,
        },
        T::Option(inner) => match value {
            Value::Null | Value::None => true,
            Value::Some(v) => fits(v, inner),
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
            (point, "Object", true),
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
    fn violation_message() {
        assert_eq!(
            violation("argument 'x' of 'f'", &t("Int"), &Value::String("s".into())),
            "type error: argument 'x' of 'f' must be Int, got String"
        );
    }
}
