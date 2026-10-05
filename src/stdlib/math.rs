use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("pi".to_string(), Value::Float(std::f64::consts::PI));
    m.insert("e".to_string(), Value::Float(std::f64::consts::E));
    m.insert("inf".to_string(), Value::Float(f64::INFINITY));
    m.insert("sqrt".to_string(), Value::BuiltIn("math.sqrt".to_string()));
    m.insert("pow".to_string(), Value::BuiltIn("math.pow".to_string()));
    m.insert("abs".to_string(), Value::BuiltIn("math.abs".to_string()));
    m.insert("max".to_string(), Value::BuiltIn("math.max".to_string()));
    m.insert("min".to_string(), Value::BuiltIn("math.min".to_string()));
    m.insert(
        "floor".to_string(),
        Value::BuiltIn("math.floor".to_string()),
    );
    m.insert("ceil".to_string(), Value::BuiltIn("math.ceil".to_string()));
    m.insert(
        "round".to_string(),
        Value::BuiltIn("math.round".to_string()),
    );
    m.insert(
        "random".to_string(),
        Value::BuiltIn("math.random".to_string()),
    );
    m.insert("sin".to_string(), Value::BuiltIn("math.sin".to_string()));
    m.insert("cos".to_string(), Value::BuiltIn("math.cos".to_string()));
    m.insert("tan".to_string(), Value::BuiltIn("math.tan".to_string()));
    m.insert("log".to_string(), Value::BuiltIn("math.log".to_string()));
    m.insert(
        "random_int".to_string(),
        Value::BuiltIn("math.random_int".to_string()),
    );
    m.insert(
        "clamp".to_string(),
        Value::BuiltIn("math.clamp".to_string()),
    );
    Value::Object(m)
}

/// Numeric result shared by both engines' math implementations.
///
/// Integer overflow policy (matches `+`/`*` arithmetic, e.g. `i64::MAX + 1`):
/// a result that does not fit in i64 is promoted to a float instead of
/// wrapping or panicking. Keeping the policy in these helpers means the
/// interpreter and the VM cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    fn into_interp(self) -> Value {
        match self {
            Num::Int(n) => Value::Int(n),
            Num::Float(f) => Value::Float(f),
        }
    }
}

/// `base ** exp` for integers; negative exponents and overflow give floats.
pub(crate) fn int_pow(base: i64, exp: i64) -> Num {
    if exp < 0 {
        return Num::Float((base as f64).powf(exp as f64));
    }
    match u32::try_from(exp).ok().and_then(|e| base.checked_pow(e)) {
        Some(n) => Num::Int(n),
        None => Num::Float((base as f64).powf(exp as f64)),
    }
}

/// `|n|`; `i64::MIN` (whose magnitude does not fit) becomes a float.
pub(crate) fn int_abs(n: i64) -> Num {
    match n.checked_abs() {
        Some(a) => Num::Int(a),
        None => Num::Float((n as f64).abs()),
    }
}

/// Convert an already-rounded float to an int when it is representable;
/// NaN, infinities and out-of-range magnitudes stay floats (no saturation).
pub(crate) fn rounded_float_to_num(f: f64) -> Num {
    // 2^63 is exactly representable; i64 range is [-2^63, 2^63).
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    if f.is_finite() && f >= -LIMIT && f < LIMIT {
        Num::Int(f as i64)
    } else {
        Num::Float(f)
    }
}

/// Uniform-ish integer in `[min, max]` without overflowing on wide ranges.
pub(crate) fn random_int_between(min: i64, max: i64) -> i64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as i128;
    let range = (max as i128) - (min as i128) + 1;
    (min as i128 + nanos % range) as i64
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "math.sqrt" => match args.first() {
            Some(Value::Float(n)) => Ok(Value::Float(n.sqrt())),
            Some(Value::Int(n)) => Ok(Value::Float((*n as f64).sqrt())),
            _ => Err("math.sqrt() requires a number".to_string()),
        },
        "math.pow" => match (args.first(), args.get(1)) {
            (Some(Value::Float(base)), Some(Value::Float(exp))) => {
                Ok(Value::Float(base.powf(*exp)))
            }
            (Some(Value::Int(base)), Some(Value::Int(exp))) => {
                if *exp < 0 {
                    Ok(Value::Float((*base as f64).powf(*exp as f64)))
                } else {
                    Ok(int_pow(*base, *exp).into_interp())
                }
            }
            (Some(Value::Int(base)), Some(Value::Float(exp))) => {
                Ok(Value::Float((*base as f64).powf(*exp)))
            }
            (Some(Value::Float(base)), Some(Value::Int(exp))) => {
                Ok(Value::Float(base.powf(*exp as f64)))
            }
            _ => Err("math.pow() requires two numbers".to_string()),
        },
        "math.abs" => match args.first() {
            Some(Value::Int(n)) => Ok(int_abs(*n).into_interp()),
            Some(Value::Float(n)) => Ok(Value::Float(n.abs())),
            _ => Err("math.abs() requires a number".to_string()),
        },
        "math.max" => match (args.first(), args.get(1)) {
            (Some(Value::Int(a)), Some(Value::Int(b))) => Ok(Value::Int(*a.max(b))),
            (Some(Value::Float(a)), Some(Value::Float(b))) => Ok(Value::Float(a.max(*b))),
            (Some(Value::Int(a)), Some(Value::Float(b))) => Ok(Value::Float((*a as f64).max(*b))),
            (Some(Value::Float(a)), Some(Value::Int(b))) => Ok(Value::Float(a.max(*b as f64))),
            _ => Err("math.max() requires two numbers".to_string()),
        },
        "math.min" => match (args.first(), args.get(1)) {
            (Some(Value::Int(a)), Some(Value::Int(b))) => Ok(Value::Int(*a.min(b))),
            (Some(Value::Float(a)), Some(Value::Float(b))) => Ok(Value::Float(a.min(*b))),
            (Some(Value::Int(a)), Some(Value::Float(b))) => Ok(Value::Float((*a as f64).min(*b))),
            (Some(Value::Float(a)), Some(Value::Int(b))) => Ok(Value::Float(a.min(*b as f64))),
            _ => Err("math.min() requires two numbers".to_string()),
        },
        "math.floor" => match args.first() {
            Some(Value::Float(n)) => Ok(rounded_float_to_num(n.floor()).into_interp()),
            Some(Value::Int(n)) => Ok(Value::Int(*n)),
            _ => Err("math.floor() requires a number".to_string()),
        },
        "math.ceil" => match args.first() {
            Some(Value::Float(n)) => Ok(rounded_float_to_num(n.ceil()).into_interp()),
            Some(Value::Int(n)) => Ok(Value::Int(*n)),
            _ => Err("math.ceil() requires a number".to_string()),
        },
        "math.round" => match args.first() {
            Some(Value::Float(n)) => Ok(rounded_float_to_num(n.round()).into_interp()),
            Some(Value::Int(n)) => Ok(Value::Int(*n)),
            _ => Err("math.round() requires a number".to_string()),
        },
        "math.random" => {
            let r: f64 = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as f64
                / 1_000_000_000.0;
            Ok(Value::Float(r))
        }
        "math.sin" => match args.first() {
            Some(Value::Float(n)) => Ok(Value::Float(n.sin())),
            Some(Value::Int(n)) => Ok(Value::Float((*n as f64).sin())),
            _ => Err("math.sin() requires a number".to_string()),
        },
        "math.cos" => match args.first() {
            Some(Value::Float(n)) => Ok(Value::Float(n.cos())),
            Some(Value::Int(n)) => Ok(Value::Float((*n as f64).cos())),
            _ => Err("math.cos() requires a number".to_string()),
        },
        "math.tan" => match args.first() {
            Some(Value::Float(n)) => Ok(Value::Float(n.tan())),
            Some(Value::Int(n)) => Ok(Value::Float((*n as f64).tan())),
            _ => Err("math.tan() requires a number".to_string()),
        },
        "math.log" => match args.first() {
            Some(Value::Float(n)) => Ok(Value::Float(n.ln())),
            Some(Value::Int(n)) => Ok(Value::Float((*n as f64).ln())),
            _ => Err("math.log() requires a number".to_string()),
        },
        "math.random_int" => match (args.first(), args.get(1)) {
            (Some(Value::Int(min)), Some(Value::Int(max))) => {
                if min > max {
                    return Err(format!(
                        "math.random_int() requires min <= max, got {} > {}",
                        min, max
                    ));
                }
                Ok(Value::Int(random_int_between(*min, *max)))
            }
            _ => Err("math.random_int() requires two integers (min, max)".to_string()),
        },
        "math.clamp" => match (args.first(), args.get(1), args.get(2)) {
            (Some(Value::Int(val)), Some(Value::Int(min)), Some(Value::Int(max))) => {
                Ok(Value::Int((*val).max(*min).min(*max)))
            }
            (Some(Value::Float(val)), Some(Value::Float(min)), Some(Value::Float(max))) => {
                Ok(Value::Float(val.max(*min).min(*max)))
            }
            (Some(Value::Int(val)), Some(Value::Float(min)), Some(Value::Float(max))) => {
                Ok(Value::Float((*val as f64).max(*min).min(*max)))
            }
            (Some(Value::Float(val)), Some(Value::Int(min)), Some(Value::Int(max))) => {
                Ok(Value::Float(val.max(*min as f64).min(*max as f64)))
            }
            _ => Err("math.clamp() requires (value, min, max) numbers".to_string()),
        },
        _ => Err(format!("unknown math function: {}", name)),
    }
}

/// VM-compatible math dispatch (uses VM Value types).
///
/// Integer results are built with `Value::int`, which heap-boxes values
/// outside the 48-bit inline range, so no input can make this panic.
pub fn call_vm(
    name: &str,
    args: &[crate::vm::value::Value],
    gc: &mut crate::vm::gc::Gc,
) -> Result<crate::vm::value::Value, String> {
    use crate::vm::value::Value as V;
    Ok(match call_vm_num(name, args, gc)? {
        Num::Int(n) => V::int(n, gc),
        Num::Float(f) => V::float(f),
    })
}

fn call_vm_num(
    name: &str,
    args: &[crate::vm::value::Value],
    gc: &crate::vm::gc::Gc,
) -> Result<Num, String> {
    use crate::vm::value::{Value as V, ValueKind as VK};

    // Helper to extract a number from a VM value
    let as_num = |v: &V| -> Option<(Option<i64>, Option<f64>)> {
        match v.classify(gc) {
            VK::Int(n) => Some((Some(n), None)),
            VK::Float(f) => Some((None, Some(f))),
            _ => None,
        }
    };

    let as_f64 = |v: &V| -> Option<f64> {
        match v.classify(gc) {
            VK::Int(n) => Some(n as f64),
            VK::Float(f) => Some(f),
            _ => None,
        }
    };

    match name {
        "math.sqrt" => {
            let f = as_f64(args.first().ok_or("math.sqrt() requires a number")?)
                .ok_or("math.sqrt() requires a number".to_string())?;
            Ok(Num::Float(f.sqrt()))
        }
        "math.pow" => {
            let (b, e) = (args.first(), args.get(1));
            let (Some(b), Some(e)) = (b, e) else {
                return Err("math.pow() requires two numbers".to_string());
            };
            let (bn, en) = (as_num(b), as_num(e));
            match (bn, en) {
                (Some((Some(bi), _)), Some((Some(ei), _))) => Ok(int_pow(bi, ei)),
                _ => {
                    let bf = as_f64(b).ok_or("math.pow() requires two numbers".to_string())?;
                    let ef = as_f64(e).ok_or("math.pow() requires two numbers".to_string())?;
                    Ok(Num::Float(bf.powf(ef)))
                }
            }
        }
        "math.abs" => {
            let v = args.first().ok_or("math.abs() requires a number")?;
            match v.classify(gc) {
                VK::Int(n) => Ok(int_abs(n)),
                VK::Float(f) => Ok(Num::Float(f.abs())),
                _ => Err("math.abs() requires a number".to_string()),
            }
        }
        "math.max" => {
            let (Some(a), Some(b)) = (args.first(), args.get(1)) else {
                return Err("math.max() requires two numbers".to_string());
            };
            match (a.classify(gc), b.classify(gc)) {
                (VK::Int(a), VK::Int(b)) => Ok(Num::Int(a.max(b))),
                _ => {
                    let af = as_f64(a).ok_or("math.max() requires two numbers".to_string())?;
                    let bf = as_f64(b).ok_or("math.max() requires two numbers".to_string())?;
                    Ok(Num::Float(af.max(bf)))
                }
            }
        }
        "math.min" => {
            let (Some(a), Some(b)) = (args.first(), args.get(1)) else {
                return Err("math.min() requires two numbers".to_string());
            };
            match (a.classify(gc), b.classify(gc)) {
                (VK::Int(a), VK::Int(b)) => Ok(Num::Int(a.min(b))),
                _ => {
                    let af = as_f64(a).ok_or("math.min() requires two numbers".to_string())?;
                    let bf = as_f64(b).ok_or("math.min() requires two numbers".to_string())?;
                    Ok(Num::Float(af.min(bf)))
                }
            }
        }
        "math.floor" => {
            let v = args.first().ok_or("math.floor() requires a number")?;
            match v.classify(gc) {
                VK::Float(n) => Ok(rounded_float_to_num(n.floor())),
                VK::Int(n) => Ok(Num::Int(n)),
                _ => Err("math.floor() requires a number".to_string()),
            }
        }
        "math.ceil" => {
            let v = args.first().ok_or("math.ceil() requires a number")?;
            match v.classify(gc) {
                VK::Float(n) => Ok(rounded_float_to_num(n.ceil())),
                VK::Int(n) => Ok(Num::Int(n)),
                _ => Err("math.ceil() requires a number".to_string()),
            }
        }
        "math.round" => {
            let v = args.first().ok_or("math.round() requires a number")?;
            match v.classify(gc) {
                VK::Float(n) => Ok(rounded_float_to_num(n.round())),
                VK::Int(n) => Ok(Num::Int(n)),
                _ => Err("math.round() requires a number".to_string()),
            }
        }
        "math.random" => {
            let r = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as f64
                / 1_000_000_000.0;
            Ok(Num::Float(r))
        }
        "math.sin" => {
            let f = as_f64(args.first().ok_or("math.sin() requires a number")?)
                .ok_or("math.sin() requires a number".to_string())?;
            Ok(Num::Float(f.sin()))
        }
        "math.cos" => {
            let f = as_f64(args.first().ok_or("math.cos() requires a number")?)
                .ok_or("math.cos() requires a number".to_string())?;
            Ok(Num::Float(f.cos()))
        }
        "math.tan" => {
            let f = as_f64(args.first().ok_or("math.tan() requires a number")?)
                .ok_or("math.tan() requires a number".to_string())?;
            Ok(Num::Float(f.tan()))
        }
        "math.log" => {
            let f = as_f64(args.first().ok_or("math.log() requires a number")?)
                .ok_or("math.log() requires a number".to_string())?;
            Ok(Num::Float(f.ln()))
        }
        "math.random_int" => {
            let (Some(a), Some(b)) = (args.first(), args.get(1)) else {
                return Err("math.random_int() requires two integers (min, max)".to_string());
            };
            let (Some(min), Some(max)) = (a.as_int(gc), b.as_int(gc)) else {
                return Err("math.random_int() requires two integers (min, max)".to_string());
            };
            if min > max {
                return Err(format!(
                    "math.random_int() requires min <= max, got {} > {}",
                    min, max
                ));
            }
            Ok(Num::Int(random_int_between(min, max)))
        }
        "math.clamp" => {
            let (Some(v), Some(lo), Some(hi)) = (args.first(), args.get(1), args.get(2)) else {
                return Err("math.clamp() requires (value, min, max) numbers".to_string());
            };
            match (v.classify(gc), lo.classify(gc), hi.classify(gc)) {
                (VK::Int(val), VK::Int(min), VK::Int(max)) => Ok(Num::Int(val.max(min).min(max))),
                _ => {
                    let vf = as_f64(v)
                        .ok_or("math.clamp() requires (value, min, max) numbers".to_string())?;
                    let lf = as_f64(lo)
                        .ok_or("math.clamp() requires (value, min, max) numbers".to_string())?;
                    let hf = as_f64(hi)
                        .ok_or("math.clamp() requires (value, min, max) numbers".to_string())?;
                    Ok(Num::Float(vf.max(lf).min(hf)))
                }
            }
        }
        _ => Err(format!("unknown math function: {}", name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_math_random_int_in_range() {
        let result = call("math.random_int", vec![Value::Int(1), Value::Int(10)]).unwrap();
        if let Value::Int(n) = result {
            assert!(n >= 1 && n <= 10);
        } else {
            panic!("expected Int");
        }
    }

    #[test]
    fn test_math_random_int_same_bounds() {
        let result = call("math.random_int", vec![Value::Int(5), Value::Int(5)]).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn test_math_random_int_invalid_range() {
        let result = call("math.random_int", vec![Value::Int(10), Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_math_clamp_int() {
        assert_eq!(
            call(
                "math.clamp",
                vec![Value::Int(5), Value::Int(1), Value::Int(10)]
            )
            .unwrap(),
            Value::Int(5)
        );
        assert_eq!(
            call(
                "math.clamp",
                vec![Value::Int(-5), Value::Int(0), Value::Int(10)]
            )
            .unwrap(),
            Value::Int(0)
        );
        assert_eq!(
            call(
                "math.clamp",
                vec![Value::Int(15), Value::Int(0), Value::Int(10)]
            )
            .unwrap(),
            Value::Int(10)
        );
    }

    #[test]
    fn test_math_clamp_float() {
        assert_eq!(
            call(
                "math.clamp",
                vec![Value::Float(5.5), Value::Float(1.0), Value::Float(10.0)]
            )
            .unwrap(),
            Value::Float(5.5)
        );
        assert_eq!(
            call(
                "math.clamp",
                vec![Value::Float(-1.0), Value::Float(0.0), Value::Float(1.0)]
            )
            .unwrap(),
            Value::Float(0.0)
        );
    }
}
