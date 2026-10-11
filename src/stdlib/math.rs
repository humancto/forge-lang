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
    pub(crate) fn into_interp(self) -> Value {
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
    use crate::clock::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as i128;
    let range = (max as i128) - (min as i128) + 1;
    (min as i128 + nanos % range) as i64
}

// ===== Global numeric builtins (`abs`, `min`, `max`, `round`, `to_fixed`) =====
//
// Both engines convert their arguments to `Option<Num>` (`None` for a
// value that is not a number) and call these, so results and error texts
// cannot differ between the interpreter and the VM.

/// Largest `digits` accepted by `round(x, digits)` and `to_fixed`.
pub(crate) const MAX_FIXED_DIGITS: i64 = 100;

/// `abs(x)`: same as `math.abs` (Int stays Int; `i64::MIN` promotes).
pub(crate) fn global_abs(x: Option<Num>) -> Result<Num, String> {
    match x {
        Some(Num::Int(n)) => Ok(int_abs(n)),
        Some(Num::Float(f)) => Ok(Num::Float(f.abs())),
        None => Err("abs() requires a number".to_string()),
    }
}

/// `floor(x)` / `ceil(x)`: same as `math.floor` / `math.ceil` (an Int
/// result, or a Float when it does not fit in an Int).
pub(crate) fn global_floor_ceil(x: Option<Num>, ceil: bool) -> Result<Num, String> {
    match x {
        Some(Num::Int(n)) => Ok(Num::Int(n)),
        Some(Num::Float(f)) => Ok(rounded_float_to_num(if ceil {
            f.ceil()
        } else {
            f.floor()
        })),
        None => Err(format!(
            "{}() requires a number",
            if ceil { "ceil" } else { "floor" }
        )),
    }
}

/// Arguments of `min` / `max`: one array, or the values themselves.
pub(crate) enum MinMaxArgs {
    /// `min(xs)`: the elements of the array.
    Array(Vec<Option<Num>>),
    /// `min(a, b, ...)`.
    Values(Vec<Option<Num>>),
}

/// `min(a, b, ...)` / `min(xs)` (and `max`). All Ints give an Int; any
/// Float makes the result a Float (as in `min_of`).
pub(crate) fn global_min_max(name: &str, args: MinMaxArgs, min: bool) -> Result<Num, String> {
    let items = match args {
        MinMaxArgs::Array(items) if items.is_empty() => {
            return Err(format!("{}() requires a non-empty array", name))
        }
        MinMaxArgs::Array(items) => items,
        MinMaxArgs::Values(items) if items.len() < 2 => {
            return Err(format!(
                "{}() requires two or more numbers, or one array of numbers",
                name
            ))
        }
        MinMaxArgs::Values(items) => items,
    };
    let mut result: Option<Num> = None;
    for item in items {
        let Some(item) = item else {
            return Err(format!("{}() requires numbers", name));
        };
        result = Some(match (result, item) {
            (None, x) => x,
            (Some(Num::Int(a)), Num::Int(b)) => Num::Int(if min { a.min(b) } else { a.max(b) }),
            (Some(a), b) => {
                let (a, b) = (num_to_f64(a), num_to_f64(b));
                Num::Float(if min { a.min(b) } else { a.max(b) })
            }
        });
    }
    result.ok_or_else(|| format!("{}() requires numbers", name))
}

fn num_to_f64(n: Num) -> f64 {
    match n {
        Num::Int(i) => i as f64,
        Num::Float(f) => f,
    }
}

/// The `digits` argument of `round` / `to_fixed`: given or not, and
/// whether it is an Int.
fn fixed_digits(name: &str, digits: Option<Option<i64>>) -> Result<Option<usize>, String> {
    match digits {
        None => Ok(None),
        Some(Some(d)) if (0..=MAX_FIXED_DIGITS).contains(&d) => Ok(Some(d as usize)),
        Some(_) => Err(format!(
            "{}() requires the number of digits to be an Int from 0 to {}",
            name, MAX_FIXED_DIGITS
        )),
    }
}

/// `round(x)` rounds half away from zero to an Int (like `math.round`);
/// `round(x, digits)` rounds to `digits` decimals and keeps the type (an
/// Int is returned unchanged, a Float stays a Float).
pub(crate) fn global_round(x: Option<Num>, digits: Option<Option<i64>>) -> Result<Num, String> {
    let digits = fixed_digits("round", digits)?;
    match (x, digits) {
        (None, _) => Err("round() requires a number".to_string()),
        (Some(Num::Int(n)), _) => Ok(Num::Int(n)),
        (Some(Num::Float(f)), None) => Ok(rounded_float_to_num(f.round())),
        (Some(Num::Float(f)), Some(_)) if !f.is_finite() => Ok(Num::Float(f)),
        (Some(Num::Float(f)), Some(d)) => {
            let text = fixed_decimal(f, d);
            // `fixed_decimal` of a finite float is always a valid decimal.
            Ok(Num::Float(text.parse().unwrap_or(f)))
        }
    }
}

/// `to_fixed(x, digits = 0)`: `x` as text with exactly `digits` decimals
/// (`to_fixed(2.5, 2)` is `"2.50"`). Rounds half away from zero on the
/// exact value of `x`; NaN and infinities print as `str(x)` does.
pub(crate) fn global_to_fixed(
    x: Option<Num>,
    digits: Option<Option<i64>>,
) -> Result<String, String> {
    let digits = fixed_digits("to_fixed", digits)?.unwrap_or(0);
    match x {
        None => Err("to_fixed() requires a number".to_string()),
        Some(Num::Int(n)) if digits == 0 => Ok(n.to_string()),
        Some(Num::Int(n)) => Ok(format!("{}.{}", n, "0".repeat(digits))),
        Some(Num::Float(f)) if !f.is_finite() => Ok(format!("{}", f)),
        Some(Num::Float(f)) => Ok(fixed_decimal(f, digits)),
    }
}

/// Finite `f` with exactly `digits` decimals, rounding half away from zero
/// on the exact binary value (so `1.005` → `"1.00"`, since it is really
/// 1.00499..., and `0.125` → `"0.13"`). A result that is all zeros has no
/// minus sign.
fn fixed_decimal(f: f64, digits: usize) -> String {
    let magnitude = f.abs();
    let body = if is_decimal_tie(magnitude, digits) {
        // The exact value has `digits + 1` decimals and ends in 5, so this
        // formatting is exact; round the digit string up by hand.
        let exact = format!("{:.*}", digits + 1, magnitude);
        round_up_decimal(&exact[..exact.len() - 1])
    } else {
        // Not halfway between two results: Rust's (correctly rounded)
        // formatting agrees with any tie rule.
        format!("{:.*}", digits, magnitude)
    };
    let body = body.trim_end_matches('.').to_string();
    if f.is_sign_negative() && body.bytes().any(|b| (b'1'..=b'9').contains(&b)) {
        format!("-{}", body)
    } else {
        body
    }
}

/// Whether finite `x >= 0` lies exactly halfway between two multiples of
/// `10^-digits`, i.e. `x * 10^digits * 2` is an odd integer. With
/// `x = m * 2^e` (`m` odd) that product is `m * 5^digits * 2^(e + digits + 1)`,
/// which is an odd integer exactly when `e == -(digits + 1)`.
fn is_decimal_tie(x: f64, digits: usize) -> bool {
    if x == 0.0 || !x.is_finite() {
        return false;
    }
    let bits = x.to_bits();
    let exp_bits = ((bits >> 52) & 0x7ff) as i64;
    let frac = bits & ((1u64 << 52) - 1);
    let (mut m, mut e) = if exp_bits == 0 {
        (frac, -1074)
    } else {
        (frac | (1u64 << 52), exp_bits - 1075)
    };
    let tz = m.trailing_zeros();
    m >>= tz;
    e += tz as i64;
    m != 0 && e == -(digits as i64 + 1)
}

/// Add one unit in the last place to a non-negative decimal string
/// (`"0.12"` → `"0.13"`, `"9.99"` → `"10.00"`, `"0."` → `"1."`).
fn round_up_decimal(text: &str) -> String {
    let mut bytes = text.as_bytes().to_vec();
    let mut i = bytes.len();
    loop {
        if i == 0 {
            bytes.insert(0, b'1');
            break;
        }
        i -= 1;
        match bytes[i] {
            b'.' => continue,
            b'9' => bytes[i] = b'0',
            d => {
                bytes[i] = d + 1;
                break;
            }
        }
    }
    String::from_utf8(bytes).unwrap_or_default()
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
            let r: f64 = crate::clock::SystemTime::now()
                .duration_since(crate::clock::UNIX_EPOCH)
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
            let r = crate::clock::SystemTime::now()
                .duration_since(crate::clock::UNIX_EPOCH)
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

    fn fixed(x: f64, digits: i64) -> String {
        global_to_fixed(Some(Num::Float(x)), Some(Some(digits))).unwrap()
    }

    #[test]
    fn to_fixed_rounds_half_away_from_zero_on_the_exact_value() {
        assert_eq!(fixed(2.5, 2), "2.50");
        assert_eq!(fixed(0.125, 2), "0.13"); // exact tie
        assert_eq!(fixed(-0.125, 2), "-0.13");
        assert_eq!(fixed(2.5, 0), "3");
        assert_eq!(fixed(0.5, 0), "1");
        assert_eq!(fixed(1.5, 0), "2");
        assert_eq!(fixed(-2.5, 0), "-3");
        assert_eq!(fixed(2.675, 2), "2.67"); // really 2.67499...
        assert_eq!(fixed(1.005, 2), "1.00"); // really 1.00499...
        assert_eq!(fixed(9.9951, 2), "10.00");
        assert_eq!(fixed(99.5, 0), "100");
        assert_eq!(fixed(0.0, 0), "0");
        assert_eq!(fixed(-0.0, 2), "0.00");
        assert_eq!(fixed(-0.004, 2), "0.00");
        assert_eq!(fixed(5e-324, 3), "0.000");
        assert_eq!(fixed(f64::MAX, 0).len(), 309);
        assert_eq!(fixed(0.1, 100).len(), 102);
        assert_eq!(fixed(f64::NAN, 2), "NaN");
        assert_eq!(fixed(f64::INFINITY, 2), "inf");
    }

    #[test]
    fn to_fixed_ints_and_errors() {
        let int = |n, d| global_to_fixed(Some(Num::Int(n)), d);
        assert_eq!(int(3, Some(Some(2))).unwrap(), "3.00");
        assert_eq!(int(-3, None).unwrap(), "-3");
        assert_eq!(
            int(i64::MIN, Some(Some(1))).unwrap(),
            "-9223372036854775808.0"
        );
        assert!(int(3, Some(Some(101))).is_err());
        assert!(int(3, Some(Some(-1))).is_err());
        assert!(int(3, Some(None)).is_err());
        assert!(global_to_fixed(None, None).is_err());
    }

    #[test]
    fn decimal_ties_are_detected_exactly() {
        assert!(is_decimal_tie(0.5, 0));
        assert!(is_decimal_tie(0.125, 2));
        assert!(is_decimal_tie(2.5, 0));
        assert!(!is_decimal_tie(2.5, 1));
        assert!(!is_decimal_tie(2.675, 2));
        assert!(!is_decimal_tie(0.1, 0));
        assert!(!is_decimal_tie(0.0, 0));
        // 2^49 + 0.125 is a tie at 2 decimals even though its ulp is 0.125.
        let big = (1u64 << 49) as f64 + 0.125;
        assert!(is_decimal_tie(big, 2));
        assert_eq!(fixed(big, 2), "562949953421312.13");
        assert_eq!(round_up_decimal("9.99"), "10.00");
        assert_eq!(round_up_decimal("0."), "1.");
    }

    #[test]
    fn round_with_and_without_digits() {
        let f = |x: f64, d: Option<Option<i64>>| global_round(Some(Num::Float(x)), d).unwrap();
        assert_eq!(f(2.5, None), Num::Int(3));
        assert_eq!(f(-2.5, None), Num::Int(-3));
        assert_eq!(f(3.14159, Some(Some(2))), Num::Float(3.14));
        assert_eq!(f(2.5, Some(Some(0))), Num::Float(3.0));
        assert_eq!(f(f64::INFINITY, Some(Some(2))), Num::Float(f64::INFINITY));
        assert_eq!(
            global_round(Some(Num::Int(7)), Some(Some(3))).unwrap(),
            Num::Int(7)
        );
        assert!(global_round(Some(Num::Int(7)), Some(Some(-1))).is_err());
        assert!(global_round(None, None).is_err());
    }

    #[test]
    fn min_max_rules() {
        use MinMaxArgs::*;
        let ints = |v: &[i64]| v.iter().map(|&n| Some(Num::Int(n))).collect::<Vec<_>>();
        assert_eq!(
            global_min_max("min", Values(ints(&[3, 1, 2])), true).unwrap(),
            Num::Int(1)
        );
        assert_eq!(
            global_min_max("max", Array(ints(&[3, 1, 2])), false).unwrap(),
            Num::Int(3)
        );
        assert_eq!(
            global_min_max(
                "min",
                Values(vec![Some(Num::Int(1)), Some(Num::Float(2.5))]),
                true
            )
            .unwrap(),
            Num::Float(1.0)
        );
        assert!(global_min_max("min", Values(ints(&[1])), true).is_err());
        assert!(global_min_max("min", Array(vec![]), true).is_err());
        assert!(global_min_max("max", Values(vec![Some(Num::Int(1)), None]), false).is_err());
        assert_eq!(
            global_abs(Some(Num::Int(i64::MIN))).unwrap(),
            int_abs(i64::MIN)
        );
        assert!(global_abs(None).is_err());
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
