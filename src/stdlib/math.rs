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
    Value::Object(m)
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
                    match (*exp).try_into() {
                        Ok(e) => Ok(Value::Int(base.pow(e))),
                        Err(_) => Ok(Value::Float((*base as f64).powf(*exp as f64))),
                    }
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
            Some(Value::Int(n)) => Ok(Value::Int(n.abs())),
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
            Some(Value::Float(n)) => Ok(Value::Int(n.floor() as i64)),
            Some(Value::Int(n)) => Ok(Value::Int(*n)),
            _ => Err("math.floor() requires a number".to_string()),
        },
        "math.ceil" => match args.first() {
            Some(Value::Float(n)) => Ok(Value::Int(n.ceil() as i64)),
            Some(Value::Int(n)) => Ok(Value::Int(*n)),
            _ => Err("math.ceil() requires a number".to_string()),
        },
        "math.round" => match args.first() {
            Some(Value::Float(n)) => Ok(Value::Int(n.round() as i64)),
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
        _ => Err(format!("unknown math function: {}", name)),
    }
}

/// VM-compatible math dispatch (uses VM Value types).
pub fn call_vm(
    name: &str,
    args: &[crate::vm::value::Value],
    _gc: &crate::vm::gc::Gc,
) -> Result<crate::vm::value::Value, String> {
    use crate::vm::value::Value as V;
    match name {
        "math.sqrt" => match args.first() {
            Some(V::Float(n)) => Ok(V::Float(n.sqrt())),
            Some(V::Int(n)) => Ok(V::Float((*n as f64).sqrt())),
            _ => Err("math.sqrt() requires a number".to_string()),
        },
        "math.pow" => match (args.first(), args.get(1)) {
            (Some(V::Int(b)), Some(V::Int(e))) => {
                if *e < 0 {
                    Ok(V::Float((*b as f64).powf(*e as f64)))
                } else {
                    match (*e).try_into() {
                        Ok(exp) => Ok(V::Int(b.pow(exp))),
                        Err(_) => Ok(V::Float((*b as f64).powf(*e as f64))),
                    }
                }
            }
            (Some(V::Float(b)), Some(V::Float(e))) => Ok(V::Float(b.powf(*e))),
            (Some(V::Int(b)), Some(V::Float(e))) => Ok(V::Float((*b as f64).powf(*e))),
            (Some(V::Float(b)), Some(V::Int(e))) => Ok(V::Float(b.powf(*e as f64))),
            _ => Err("math.pow() requires two numbers".to_string()),
        },
        "math.abs" => match args.first() {
            Some(V::Int(n)) => Ok(V::Int(n.abs())),
            Some(V::Float(n)) => Ok(V::Float(n.abs())),
            _ => Err("math.abs() requires a number".to_string()),
        },
        "math.max" => match (args.first(), args.get(1)) {
            (Some(V::Int(a)), Some(V::Int(b))) => Ok(V::Int(*a.max(b))),
            (Some(V::Float(a)), Some(V::Float(b))) => Ok(V::Float(a.max(*b))),
            _ => Err("math.max() requires two numbers".to_string()),
        },
        "math.min" => match (args.first(), args.get(1)) {
            (Some(V::Int(a)), Some(V::Int(b))) => Ok(V::Int(*a.min(b))),
            (Some(V::Float(a)), Some(V::Float(b))) => Ok(V::Float(a.min(*b))),
            _ => Err("math.min() requires two numbers".to_string()),
        },
        "math.floor" => match args.first() {
            Some(V::Float(n)) => Ok(V::Int(n.floor() as i64)),
            Some(V::Int(n)) => Ok(V::Int(*n)),
            _ => Err("math.floor() requires a number".to_string()),
        },
        "math.ceil" => match args.first() {
            Some(V::Float(n)) => Ok(V::Int(n.ceil() as i64)),
            Some(V::Int(n)) => Ok(V::Int(*n)),
            _ => Err("math.ceil() requires a number".to_string()),
        },
        "math.round" => match args.first() {
            Some(V::Float(n)) => Ok(V::Int(n.round() as i64)),
            Some(V::Int(n)) => Ok(V::Int(*n)),
            _ => Err("math.round() requires a number".to_string()),
        },
        "math.random" => {
            let r = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as f64
                / 1_000_000_000.0;
            Ok(V::Float(r))
        }
        "math.sin" => match args.first() {
            Some(V::Float(n)) => Ok(V::Float(n.sin())),
            Some(V::Int(n)) => Ok(V::Float((*n as f64).sin())),
            _ => Err("math.sin() requires a number".to_string()),
        },
        "math.cos" => match args.first() {
            Some(V::Float(n)) => Ok(V::Float(n.cos())),
            Some(V::Int(n)) => Ok(V::Float((*n as f64).cos())),
            _ => Err("math.cos() requires a number".to_string()),
        },
        "math.tan" => match args.first() {
            Some(V::Float(n)) => Ok(V::Float(n.tan())),
            Some(V::Int(n)) => Ok(V::Float((*n as f64).tan())),
            _ => Err("math.tan() requires a number".to_string()),
        },
        "math.log" => match args.first() {
            Some(V::Float(n)) => Ok(V::Float(n.ln())),
            Some(V::Int(n)) => Ok(V::Float((*n as f64).ln())),
            _ => Err("math.log() requires a number".to_string()),
        },
        _ => Err(format!("unknown math function: {}", name)),
    }
}
