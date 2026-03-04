/// Interpreter — Builtin Function Dispatch
/// Extracted from interpreter/mod.rs to keep that file navigable.
/// This is a continuation of `impl Interpreter` — same struct, separate file.
/// Do NOT change logic here; this is a pure structural extraction.
///
/// HOW IT WORKS: Rust allows multiple `impl` blocks for the same struct across
/// files within the same module. `mod builtins` is declared in mod.rs, so this
/// compiles as part of the `interpreter` module. `pub(super)` makes call_builtin
/// accessible to the rest of the module where it is called.
use super::*;

impl Interpreter {
    pub fn call_builtin(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
        match name {
            "print" => {
                let text: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
                let output = text.join(" ");
                print!("{}", output);
                Ok(Value::Null)
            }
            "println" => {
                let text: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
                let output = text.join(" ");
                println!("{}", output);
                Ok(Value::Null)
            }
            "len" => match args.first() {
                Some(Value::String(s)) => Ok(Value::Int(s.len() as i64)),
                Some(Value::Array(a)) => Ok(Value::Int(a.len() as i64)),
                Some(Value::Object(o)) => Ok(Value::Int(o.len() as i64)),
                _ => Err(RuntimeError::new("len() requires string, array, or object")),
            },
            "type" | "typeof" => match args.first() {
                Some(v) => Ok(Value::String(v.type_name().to_string())),
                None => Err(RuntimeError::new("typeof() requires an argument")),
            },
            "str" => match args.first() {
                Some(v) => Ok(Value::String(format!("{}", v))),
                None => Ok(Value::String(String::new())),
            },
            "int" => match args.first() {
                Some(Value::Int(n)) => Ok(Value::Int(*n)),
                Some(Value::Float(n)) => Ok(Value::Int(*n as i64)),
                Some(Value::Bool(b)) => Ok(Value::Int(if *b { 1 } else { 0 })),
                Some(Value::String(s)) => s
                    .parse::<i64>()
                    .map(Value::Int)
                    .map_err(|_| RuntimeError::new(&format!("cannot convert '{}' to Int", s))),
                _ => Err(RuntimeError::new("int() requires number, bool, or string")),
            },
            "float" => match args.first() {
                Some(Value::Int(n)) => Ok(Value::Float(*n as f64)),
                Some(Value::Float(n)) => Ok(Value::Float(*n)),
                Some(Value::String(s)) => s
                    .parse::<f64>()
                    .map(Value::Float)
                    .map_err(|_| RuntimeError::new(&format!("cannot convert '{}' to Float", s))),
                _ => Err(RuntimeError::new("float() requires number or string")),
            },
            "push" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new("push() requires array and value"));
                }
                if let Value::Array(mut items) = args[0].clone() {
                    items.push(args[1].clone());
                    Ok(Value::Array(items))
                } else {
                    Err(RuntimeError::new("push() first argument must be array"))
                }
            }
            "pop" => match args.first() {
                Some(Value::Array(items)) => {
                    // Returns the last element (the popped item), not the remaining array
                    Ok(items.last().cloned().unwrap_or(Value::Null))
                }
                _ => Err(RuntimeError::new("pop() requires array")),
            },
            "keys" => match args.first() {
                Some(Value::Object(map)) => Ok(Value::Array(
                    map.keys().map(|k| Value::String(k.clone())).collect(),
                )),
                _ => Err(RuntimeError::new("keys() requires object")),
            },
            "values" => match args.first() {
                Some(Value::Object(map)) => Ok(Value::Array(map.values().cloned().collect())),
                _ => Err(RuntimeError::new("values() requires object")),
            },
            "contains" => match (args.first(), args.get(1)) {
                (Some(Value::String(s)), Some(Value::String(sub))) => {
                    Ok(Value::Bool(s.contains(sub.as_str())))
                }
                (Some(Value::Array(arr)), Some(val)) => Ok(Value::Bool(
                    arr.iter().any(|v| format!("{}", v) == format!("{}", val)),
                )),
                (Some(Value::Object(map)), Some(Value::String(key))) => {
                    Ok(Value::Bool(map.contains_key(key)))
                }
                _ => Err(RuntimeError::new(
                    "contains() requires (string, substring), (array, value), or (object, key)",
                )),
            },
            "has_key" => match (args.first(), args.get(1)) {
                (Some(Value::Object(map)), Some(Value::String(key))) => {
                    Ok(Value::Bool(map.contains_key(key)))
                }
                _ => Err(RuntimeError::new("has_key() requires (object, key_string)")),
            },
            "get" => match (args.first(), args.get(1)) {
                (Some(obj @ Value::Object(_)), Some(Value::String(key))) => {
                    let default = args.get(2).cloned().unwrap_or(Value::Null);
                    if key.contains('.') {
                        let parts: Vec<&str> = key.split('.').collect();
                        let mut current = obj.clone();
                        for part in &parts {
                            match current {
                                Value::Object(ref m) => {
                                    current = match m.get(*part) {
                                        Some(v) => v.clone(),
                                        None => return Ok(default),
                                    };
                                }
                                Value::Array(ref arr) => {
                                    if let Ok(idx) = part.parse::<usize>() {
                                        current = match arr.get(idx) {
                                            Some(v) => v.clone(),
                                            None => return Ok(default),
                                        };
                                    } else {
                                        return Ok(default);
                                    }
                                }
                                _ => return Ok(default),
                            }
                        }
                        Ok(current)
                    } else if let Value::Object(map) = obj {
                        Ok(map.get(key).cloned().unwrap_or(default))
                    } else {
                        Ok(default)
                    }
                }
                (Some(Value::Array(arr)), Some(Value::Int(idx))) => {
                    let default = args.get(2).cloned().unwrap_or(Value::Null);
                    Ok(arr.get(*idx as usize).cloned().unwrap_or(default))
                }
                _ => Err(RuntimeError::new(
                    "get() requires (object, key) or (array, index)",
                )),
            },
            "pick" => match (args.first(), args.get(1)) {
                (Some(Value::Object(map)), Some(Value::Array(field_list))) => {
                    let mut result = IndexMap::new();
                    for field in field_list {
                        if let Value::String(key) = field {
                            if let Some(val) = map.get(key) {
                                result.insert(key.clone(), val.clone());
                            }
                        }
                    }
                    Ok(Value::Object(result))
                }
                _ => Err(RuntimeError::new("pick() requires (object, [field_names])")),
            },
            "omit" => match (args.first(), args.get(1)) {
                (Some(Value::Object(map)), Some(Value::Array(field_list))) => {
                    let omit_keys: Vec<String> = field_list
                        .iter()
                        .filter_map(|v| {
                            if let Value::String(s) = v {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                        .collect();
                    let result: IndexMap<String, Value> = map
                        .iter()
                        .filter(|(k, _)| !omit_keys.contains(k))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    Ok(Value::Object(result))
                }
                _ => Err(RuntimeError::new("omit() requires (object, [field_names])")),
            },
            "merge" => {
                let mut result = IndexMap::new();
                for arg in &args {
                    if let Value::Object(map) = arg {
                        for (k, v) in map {
                            result.insert(k.clone(), v.clone());
                        }
                    } else {
                        return Err(RuntimeError::new(
                            "merge() requires all arguments to be objects",
                        ));
                    }
                }
                Ok(Value::Object(result))
            }
            "find" => match (args.first(), args.get(1)) {
                (Some(Value::Array(arr)), Some(func)) => {
                    for item in arr {
                        let result = self.call_function(func.clone(), vec![item.clone()])?;
                        if result.is_truthy() {
                            return Ok(item.clone());
                        }
                    }
                    Ok(Value::Null)
                }
                _ => Err(RuntimeError::new("find() requires (array, predicate_fn)")),
            },
            "flat_map" => match (args.first(), args.get(1)) {
                (Some(Value::Array(arr)), Some(func)) => {
                    let mut result = Vec::new();
                    for item in arr {
                        let mapped = self.call_function(func.clone(), vec![item.clone()])?;
                        match mapped {
                            Value::Array(inner) => result.extend(inner),
                            other => result.push(other),
                        }
                    }
                    Ok(Value::Array(result))
                }
                _ => Err(RuntimeError::new("flat_map() requires (array, fn)")),
            },
            "entries" => match args.first() {
                Some(Value::Object(map)) => {
                    let pairs: Vec<Value> = map
                        .iter()
                        .map(|(k, v)| Value::Array(vec![Value::String(k.clone()), v.clone()]))
                        .collect();
                    Ok(Value::Array(pairs))
                }
                _ => Err(RuntimeError::new("entries() requires an object")),
            },
            "from_entries" => match args.first() {
                Some(Value::Array(pairs)) => {
                    let mut result = IndexMap::new();
                    for pair in pairs {
                        if let Value::Array(kv) = pair {
                            if let (Some(Value::String(k)), Some(v)) = (kv.first(), kv.get(1)) {
                                result.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    Ok(Value::Object(result))
                }
                _ => Err(RuntimeError::new(
                    "from_entries() requires an array of [key, value] pairs",
                )),
            },
            "range" => match (args.first(), args.get(1)) {
                (Some(Value::Int(start)), Some(Value::Int(end))) => {
                    Ok(Value::Array((*start..*end).map(Value::Int).collect()))
                }
                (Some(Value::Int(end)), None) => {
                    Ok(Value::Array((0..*end).map(Value::Int).collect()))
                }
                _ => Err(RuntimeError::new("range() requires integer arguments")),
            },
            "enumerate" => match args.first() {
                Some(Value::Array(items)) => {
                    let mut pairs = Vec::with_capacity(items.len());
                    for (idx, item) in items.iter().enumerate() {
                        let mut row = IndexMap::new();
                        row.insert("index".to_string(), Value::Int(idx as i64));
                        row.insert("value".to_string(), item.clone());
                        pairs.push(Value::Object(row));
                    }
                    Ok(Value::Array(pairs))
                }
                _ => Err(RuntimeError::new("enumerate() requires array")),
            },
            "map" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new("map() requires (array, function)"));
                }
                let items = match &args[0] {
                    Value::Array(items) => items.clone(),
                    _ => return Err(RuntimeError::new("map() first argument must be array")),
                };
                let func = args[1].clone();
                let mut out = Vec::with_capacity(items.len());

                for item in items {
                    out.push(self.call_function(func.clone(), vec![item])?);
                }

                Ok(Value::Array(out))
            }
            "filter" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new("filter() requires (array, function)"));
                }
                let items = match &args[0] {
                    Value::Array(items) => items.clone(),
                    _ => return Err(RuntimeError::new("filter() first argument must be array")),
                };
                let func = args[1].clone();
                let mut out = Vec::new();

                for item in items {
                    let keep = self.call_function(func.clone(), vec![item.clone()])?;
                    if keep.is_truthy() {
                        out.push(item);
                    }
                }

                Ok(Value::Array(out))
            }
            "Ok" | "ok" => {
                let value = args.first().cloned().unwrap_or(Value::Null);
                Ok(Value::ResultOk(Box::new(value)))
            }
            "Err" | "err" => {
                let value = args
                    .first()
                    .cloned()
                    .unwrap_or(Value::String("error".to_string()));
                Ok(Value::ResultErr(Box::new(value)))
            }
            "Some" => {
                let value = args.first().cloned().unwrap_or(Value::Null);
                Ok(Value::Some(Box::new(value)))
            }
            "is_ok" => match args.first() {
                Some(Value::ResultOk(_)) => Ok(Value::Bool(true)),
                Some(Value::ResultErr(_)) => Ok(Value::Bool(false)),
                _ => Err(RuntimeError::new("is_ok() requires a Result value")),
            },
            "is_err" => match args.first() {
                Some(Value::ResultOk(_)) => Ok(Value::Bool(false)),
                Some(Value::ResultErr(_)) => Ok(Value::Bool(true)),
                _ => Err(RuntimeError::new("is_err() requires a Result value")),
            },
            "unwrap" => match args.first() {
                Some(Value::ResultOk(value)) => Ok((**value).clone()),
                Some(Value::ResultErr(err)) => {
                    Err(RuntimeError::new(&format!("unwrap() on Err: {}", err)))
                }
                Some(Value::Some(value)) => Ok((**value).clone()),
                Some(Value::None) => Err(RuntimeError::new("unwrap() called on None")),
                _ => Err(RuntimeError::new(
                    "unwrap() requires a Result or Option value",
                )),
            },
            "unwrap_or" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new("unwrap_or() requires (value, default)"));
                }
                match &args[0] {
                    Value::ResultOk(value) => Ok((**value).clone()),
                    Value::ResultErr(_) => Ok(args[1].clone()),
                    Value::Some(value) => Ok((**value).clone()),
                    Value::None => Ok(args[1].clone()),
                    _ => Err(RuntimeError::new(
                        "unwrap_or() requires a Result or Option value as first argument",
                    )),
                }
            }
            "unwrap_err" => match args.first() {
                Some(Value::ResultErr(msg)) => Ok(Value::String(format!("{}", msg))),
                Some(Value::ResultOk(_)) => Err(RuntimeError::new("unwrap_err() called on Ok")),
                _ => Err(RuntimeError::new("unwrap_err() requires a Result value")),
            },
            "fetch" => match args.first() {
                Some(Value::String(url)) => {
                    let method = match args.get(1) {
                        Some(Value::Object(opts)) => opts
                            .get("method")
                            .and_then(|v| {
                                if let Value::String(s) = v {
                                    Some(s.to_uppercase())
                                } else {
                                    None
                                }
                            })
                            .unwrap_or_else(|| "GET".to_string()),
                        _ => "GET".to_string(),
                    };

                    let body = match args.get(1) {
                        Some(Value::Object(opts)) => opts.get("body").map(|v| v.to_json_string()),
                        _ => None,
                    };

                    match crate::runtime::client::fetch_blocking(url, &method, body, None, None) {
                        Ok(value) => Ok(value),
                        Err(e) => Err(RuntimeError::new(&format!("fetch error: {}", e))),
                    }
                }
                _ => Err(RuntimeError::new("fetch() requires a URL string")),
            },
            "time" => {
                crate::stdlib::time::call("time.now", args).map_err(|e| RuntimeError::new(&e))
            }
            "json" => match args.first() {
                Some(Value::String(s)) => match serde_json::from_str::<serde_json::Value>(s) {
                    Ok(v) => Ok(json_to_value(v)),
                    Err(e) => Err(RuntimeError::new(&format!("JSON parse error: {}", e))),
                },
                Some(v) => Ok(Value::String(v.to_json_string())),
                None => Err(RuntimeError::new("json() requires an argument")),
            },
            "uuid" => Ok(Value::String(uuid::Uuid::new_v4().to_string())),
            "say" => {
                let text: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
                println!("{}", text.join(" "));
                Ok(Value::Null)
            }
            "yell" => {
                let text: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
                println!("{}", text.join(" ").to_uppercase());
                Ok(Value::Null)
            }
            "whisper" => {
                let text: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
                println!("{}", text.join(" ").to_lowercase());
                Ok(Value::Null)
            }
            "wait" => match args.first() {
                Some(Value::Int(secs)) => {
                    let total_ms = ((*secs).max(0) as u64) * 1000;
                    let mut elapsed = 0u64;
                    while elapsed < total_ms {
                        if self.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                            return Err(RuntimeError::new("cancelled"));
                        }
                        let chunk = std::cmp::min(100, total_ms - elapsed);
                        std::thread::sleep(std::time::Duration::from_millis(chunk));
                        elapsed += chunk;
                    }
                    Ok(Value::Null)
                }
                Some(Value::Float(secs)) => {
                    let total_ms = (secs.max(0.0) * 1000.0) as u64;
                    let mut elapsed = 0u64;
                    while elapsed < total_ms {
                        if self.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                            return Err(RuntimeError::new("cancelled"));
                        }
                        let chunk = std::cmp::min(100, total_ms - elapsed);
                        std::thread::sleep(std::time::Duration::from_millis(chunk));
                        elapsed += chunk;
                    }
                    Ok(Value::Null)
                }
                _ => Err(RuntimeError::new("wait() requires a number of seconds")),
            },
            "channel" => {
                let capacity = match args.first() {
                    Some(Value::Int(n)) => (*n).max(1) as usize,
                    _ => 32,
                };
                let (tx, rx) = std::sync::mpsc::sync_channel::<Value>(capacity);
                Ok(Value::Channel(Arc::new(ChannelInner {
                    tx: std::sync::Mutex::new(Some(tx)),
                    rx: std::sync::Mutex::new(Some(rx)),
                    capacity,
                })))
            }
            "send" => {
                if args.len() < 2 {
                    return Err(RuntimeError::new(
                        "send(channel, value) requires 2 arguments",
                    ));
                }
                let val = args[1].clone();
                match &args[0] {
                    Value::Channel(ch) => {
                        if let Ok(guard) = ch.tx.lock() {
                            if let Some(ref sender) = *guard {
                                sender
                                    .send(val)
                                    .map_err(|_| RuntimeError::new("channel closed"))?;
                                return Ok(Value::Null);
                            }
                        }
                        Err(RuntimeError::new("channel closed"))
                    }
                    _ => Err(RuntimeError::new(
                        "send() requires a channel as first argument",
                    )),
                }
            }
            "receive" => {
                let ch = match args.first() {
                    Some(v) => v,
                    None => return Err(RuntimeError::new("receive(channel) requires 1 argument")),
                };
                match ch {
                    Value::Channel(inner) => {
                        if let Ok(guard) = inner.rx.lock() {
                            if let Some(ref receiver) = *guard {
                                match receiver.recv() {
                                    Ok(val) => return Ok(val),
                                    Err(_) => return Ok(Value::Null),
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    _ => Err(RuntimeError::new(
                        "receive() requires a channel as first argument",
                    )),
                }
            }
            "reduce" => {
                if args.len() != 3 {
                    return Err(RuntimeError::new(
                        "reduce() requires (array, initial, function)",
                    ));
                }
                let items = match &args[0] {
                    Value::Array(items) => items.clone(),
                    _ => return Err(RuntimeError::new("reduce() first argument must be array")),
                };
                let mut acc = args[1].clone();
                let func = args[2].clone();
                for item in items {
                    acc = self.call_function(func.clone(), vec![acc, item])?;
                }
                Ok(acc)
            }
            "sort" => match args.first() {
                Some(Value::Array(items)) => {
                    let mut sorted = items.clone();
                    if let Some(comparator) = args.get(1) {
                        // Custom comparator: sort(arr, fn(a, b) -> -1|0|1)
                        let comparator = comparator.clone();
                        let mut error: Option<RuntimeError> = None;
                        sorted.sort_by(|a, b| {
                            if error.is_some() {
                                return std::cmp::Ordering::Equal;
                            }
                            match self.call_function(comparator.clone(), vec![a.clone(), b.clone()])
                            {
                                Ok(Value::Int(n)) => {
                                    if n < 0 {
                                        std::cmp::Ordering::Less
                                    } else if n > 0 {
                                        std::cmp::Ordering::Greater
                                    } else {
                                        std::cmp::Ordering::Equal
                                    }
                                }
                                Ok(_) => std::cmp::Ordering::Equal,
                                Err(e) => {
                                    error = Some(e);
                                    std::cmp::Ordering::Equal
                                }
                            }
                        });
                        if let Some(e) = error {
                            return Err(e);
                        }
                    } else {
                        sorted.sort_by(|a, b| match (a, b) {
                            (Value::Int(x), Value::Int(y)) => x.cmp(y),
                            (Value::Float(x), Value::Float(y)) => {
                                x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                            }
                            (Value::String(x), Value::String(y)) => x.cmp(y),
                            _ => std::cmp::Ordering::Equal,
                        });
                    }
                    Ok(Value::Array(sorted))
                }
                _ => Err(RuntimeError::new("sort() requires an array")),
            },
            "reverse" => match args.first() {
                Some(Value::Array(items)) => {
                    let mut reversed = items.clone();
                    reversed.reverse();
                    Ok(Value::Array(reversed))
                }
                Some(Value::String(s)) => Ok(Value::String(s.chars().rev().collect())),
                _ => Err(RuntimeError::new("reverse() requires an array or string")),
            },
            "sort_by" => match (args.first(), args.get(1)) {
                (Some(Value::Array(items)), Some(key_fn)) => {
                    let key_fn = key_fn.clone();
                    let mut pairs: Vec<(Value, Value)> = Vec::new();
                    for item in items {
                        let key = self.call_function(key_fn.clone(), vec![item.clone()])?;
                        pairs.push((key, item.clone()));
                    }
                    pairs.sort_by(|(ka, _), (kb, _)| match (ka, kb) {
                        (Value::Int(a), Value::Int(b)) => a.cmp(b),
                        (Value::Float(a), Value::Float(b)) => {
                            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                        }
                        (Value::String(a), Value::String(b)) => a.cmp(b),
                        _ => std::cmp::Ordering::Equal,
                    });
                    Ok(Value::Array(pairs.into_iter().map(|(_, v)| v).collect()))
                }
                _ => Err(RuntimeError::new(
                    "sort_by() requires (array, key_function)",
                )),
            },
            "first" => match args.first() {
                Some(Value::Array(items)) => Ok(items.first().cloned().unwrap_or(Value::Null)),
                _ => Err(RuntimeError::new("first() requires an array")),
            },
            "last" => match args.first() {
                Some(Value::Array(items)) => Ok(items.last().cloned().unwrap_or(Value::Null)),
                _ => Err(RuntimeError::new("last() requires an array")),
            },
            "compact" => match args.first() {
                Some(Value::Array(items)) => {
                    let filtered: Vec<Value> = items
                        .iter()
                        .filter(|v| !matches!(v, Value::Null | Value::None | Value::Bool(false)))
                        .cloned()
                        .collect();
                    Ok(Value::Array(filtered))
                }
                _ => Err(RuntimeError::new("compact() requires an array")),
            },
            "take_n" => match (args.first(), args.get(1)) {
                (Some(Value::Array(items)), Some(Value::Int(n))) => {
                    let n = (*n as usize).min(items.len());
                    Ok(Value::Array(items[..n].to_vec()))
                }
                _ => Err(RuntimeError::new("take() requires (array, count)")),
            },
            "skip" => match (args.first(), args.get(1)) {
                (Some(Value::Array(items)), Some(Value::Int(n))) => {
                    let n = (*n as usize).min(items.len());
                    Ok(Value::Array(items[n..].to_vec()))
                }
                _ => Err(RuntimeError::new("skip() requires (array, count)")),
            },
            "frequencies" => match args.first() {
                Some(Value::Array(items)) => {
                    let mut counts = IndexMap::new();
                    for item in items {
                        let key = format!("{}", item);
                        let count = counts
                            .get(&key)
                            .and_then(|v| {
                                if let Value::Int(n) = v {
                                    Some(*n)
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(0);
                        counts.insert(key, Value::Int(count + 1));
                    }
                    Ok(Value::Object(counts))
                }
                _ => Err(RuntimeError::new("frequencies() requires an array")),
            },
            "for_each" => match (args.first(), args.get(1)) {
                (Some(Value::Array(items)), Some(func)) => {
                    let func = func.clone();
                    for item in items {
                        self.call_function(func.clone(), vec![item.clone()])?;
                    }
                    Ok(Value::Null)
                }
                _ => Err(RuntimeError::new("for_each() requires (array, function)")),
            },
            "split" => match (args.first(), args.get(1)) {
                (Some(Value::String(s)), Some(Value::String(delim))) => {
                    let parts: Vec<Value> = if delim.is_empty() {
                        // Empty delimiter: split into individual characters
                        s.chars().map(|c| Value::String(c.to_string())).collect()
                    } else {
                        s.split(delim.as_str())
                            .map(|part| Value::String(part.to_string()))
                            .collect()
                    };
                    Ok(Value::Array(parts))
                }
                _ => Err(RuntimeError::new(
                    "split() requires (string, delimiter_string)",
                )),
            },
            "join" => match (args.first(), args.get(1)) {
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
            "replace" => match (args.first(), args.get(1), args.get(2)) {
                (Some(Value::String(s)), Some(Value::String(from)), Some(Value::String(to))) => {
                    Ok(Value::String(s.replace(from.as_str(), to.as_str())))
                }
                _ => Err(RuntimeError::new(
                    "replace() requires (string, from_string, to_string)",
                )),
            },
            "starts_with" => match (args.first(), args.get(1)) {
                (Some(Value::String(s)), Some(Value::String(prefix))) => {
                    Ok(Value::Bool(s.starts_with(prefix.as_str())))
                }
                _ => Err(RuntimeError::new(
                    "starts_with() requires (string, prefix_string)",
                )),
            },
            "ends_with" => match (args.first(), args.get(1)) {
                (Some(Value::String(s)), Some(Value::String(suffix))) => {
                    Ok(Value::Bool(s.ends_with(suffix.as_str())))
                }
                _ => Err(RuntimeError::new(
                    "ends_with() requires (string, suffix_string)",
                )),
            },
            "is_some" => match args.first() {
                Some(Value::Some(_)) => Ok(Value::Bool(true)),
                Some(Value::None) => Ok(Value::Bool(false)),
                // Backward compat: ADT-encoded Option objects
                Some(Value::Object(obj)) => {
                    let is_opt = obj
                        .get("__type__")
                        .is_some_and(|v| matches!(v, Value::String(s) if s == "Option"));
                    if is_opt {
                        let variant = obj.get("__variant__").map(|v| format!("{}", v));
                        Ok(Value::Bool(
                            variant.as_deref() == std::option::Option::Some("Some"),
                        ))
                    } else {
                        Ok(Value::Bool(true))
                    }
                }
                Some(Value::Null) => Ok(Value::Bool(false)),
                Some(_) => Ok(Value::Bool(true)),
                std::option::Option::None => {
                    Err(RuntimeError::new("is_some() requires an argument"))
                }
            },
            "is_none" => match args.first() {
                Some(Value::None) => Ok(Value::Bool(true)),
                Some(Value::Some(_)) => Ok(Value::Bool(false)),
                // Backward compat: ADT-encoded Option objects
                Some(Value::Object(obj)) => {
                    let is_opt = obj
                        .get("__type__")
                        .is_some_and(|v| matches!(v, Value::String(s) if s == "Option"));
                    if is_opt {
                        let variant = obj.get("__variant__").map(|v| format!("{}", v));
                        Ok(Value::Bool(
                            variant.as_deref() == std::option::Option::Some("None"),
                        ))
                    } else {
                        Ok(Value::Bool(false))
                    }
                }
                Some(Value::Null) => Ok(Value::Bool(true)),
                Some(_) => Ok(Value::Bool(false)),
                std::option::Option::None => {
                    Err(RuntimeError::new("is_none() requires an argument"))
                }
            },
            "satisfies" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new("satisfies() requires (value, interface)"));
                }
                let value = &args[0];
                let iface = &args[1];
                if let Value::Object(iface_obj) = iface {
                    if let Some(Value::Array(methods)) = iface_obj.get("methods") {
                        // First check structural satisfaction (existing behavior)
                        let structural = check_interface_satisfaction(value, methods, &self.env);
                        if structural {
                            return Ok(Value::Bool(true));
                        }
                        // Then check method_tables from give/impl blocks
                        if let Value::Object(obj) = value {
                            if let Some(Value::String(type_name)) = obj.get("__type__") {
                                if let Some(type_methods) = self.method_tables.get(type_name) {
                                    let all_satisfied = methods.iter().all(|spec| {
                                        if let Value::Object(s) = spec {
                                            if let Some(Value::String(mname)) = s.get("name") {
                                                return type_methods.contains_key(mname);
                                            }
                                        }
                                        false
                                    });
                                    return Ok(Value::Bool(all_satisfied));
                                }
                            }
                        }
                    }
                }
                Ok(Value::Bool(false))
            }
            "assert" => {
                let condition = args.first().cloned().unwrap_or(Value::Bool(false));
                if !condition.is_truthy() {
                    let msg = args
                        .get(1)
                        .map(|v| format!("{}", v))
                        .unwrap_or_else(|| "assertion failed".to_string());
                    return Err(RuntimeError::new(&format!("assertion failed: {}", msg)));
                }
                Ok(Value::Null)
            }
            "assert_eq" => {
                if args.len() < 2 {
                    return Err(RuntimeError::new(
                        "assert_eq() requires at least 2 arguments",
                    ));
                }
                let left = format!("{}", args[0]);
                let right = format!("{}", args[1]);
                if left != right {
                    let msg = args.get(2).map(|v| format!("{}", v)).unwrap_or_default();
                    let detail = if msg.is_empty() {
                        format!("expected `{}`, got `{}`", right, left)
                    } else {
                        format!("{}: expected `{}`, got `{}`", msg, right, left)
                    };
                    return Err(RuntimeError::new(&format!("assertion failed: {}", detail)));
                }
                Ok(Value::Null)
            }
            "assert_ne" => {
                if args.len() < 2 {
                    return Err(RuntimeError::new(
                        "assert_ne() requires at least 2 arguments",
                    ));
                }
                let left = format!("{}", args[0]);
                let right = format!("{}", args[1]);
                if left == right {
                    let msg = args.get(2).map(|v| format!("{}", v)).unwrap_or_default();
                    let detail = if msg.is_empty() {
                        format!("expected values to differ, both are `{}`", left)
                    } else {
                        format!("{}: expected values to differ, both are `{}`", msg, left)
                    };
                    return Err(RuntimeError::new(&format!("assertion failed: {}", detail)));
                }
                Ok(Value::Null)
            }
            "assert_throws" => {
                if args.is_empty() {
                    return Err(RuntimeError::new("assert_throws() requires a function"));
                }
                let func = args[0].clone();
                match self.call_function(func, vec![]) {
                    Err(_) => Ok(Value::Bool(true)),
                    Ok(_) => Err(RuntimeError::new(
                        "assertion failed: expected function to throw an error, but it succeeded",
                    )),
                }
            }
            // ===== String Operations =====
            "substring" => match args.first() {
                Some(Value::String(s)) => {
                    let start = match args.get(1) {
                        Some(Value::Int(n)) => *n as usize,
                        _ => {
                            return Err(RuntimeError::new(
                                "substring() requires (string, start, end?)",
                            ))
                        }
                    };
                    let chars: Vec<char> = s.chars().collect();
                    let end = match args.get(2) {
                        Some(Value::Int(n)) => (*n as usize).min(chars.len()),
                        _ => chars.len(),
                    };
                    if start > chars.len() {
                        return Ok(Value::String(String::new()));
                    }
                    Ok(Value::String(chars[start..end].iter().collect()))
                }
                _ => Err(RuntimeError::new(
                    "substring() requires a string as first argument",
                )),
            },
            "index_of" => match args.first() {
                Some(Value::String(s)) => match args.get(1) {
                    Some(Value::String(substr)) => Ok(Value::Int(
                        s.find(substr.as_str()).map(|i| i as i64).unwrap_or(-1),
                    )),
                    _ => Err(RuntimeError::new("index_of() requires (string, substring)")),
                },
                Some(Value::Array(arr)) => {
                    let needle = match args.get(1) {
                        Some(v) => v,
                        None => return Err(RuntimeError::new("index_of() requires 2 arguments")),
                    };
                    let idx = arr.iter().position(|v| v == needle);
                    Ok(Value::Int(idx.map(|i| i as i64).unwrap_or(-1)))
                }
                _ => Err(RuntimeError::new(
                    "index_of() requires a string or array as first argument",
                )),
            },
            "last_index_of" => match (args.first(), args.get(1)) {
                (Some(Value::String(s)), Some(Value::String(substr))) => Ok(Value::Int(
                    s.rfind(substr.as_str()).map(|i| i as i64).unwrap_or(-1),
                )),
                _ => Err(RuntimeError::new(
                    "last_index_of() requires (string, substring)",
                )),
            },
            "pad_start" => match (args.first(), args.get(1)) {
                (Some(Value::String(s)), Some(Value::Int(target_len))) => {
                    let pad_char = match args.get(2) {
                        Some(Value::String(c)) => c.chars().next().unwrap_or(' '),
                        _ => ' ',
                    };
                    let target = *target_len as usize;
                    let char_count = s.chars().count();
                    if char_count >= target {
                        Ok(Value::String(s.clone()))
                    } else {
                        let padding: String = std::iter::repeat(pad_char)
                            .take(target - char_count)
                            .collect();
                        Ok(Value::String(format!("{}{}", padding, s)))
                    }
                }
                _ => Err(RuntimeError::new("pad_start() requires (string, length)")),
            },
            "pad_end" => match (args.first(), args.get(1)) {
                (Some(Value::String(s)), Some(Value::Int(target_len))) => {
                    let pad_char = match args.get(2) {
                        Some(Value::String(c)) => c.chars().next().unwrap_or(' '),
                        _ => ' ',
                    };
                    let target = *target_len as usize;
                    let char_count = s.chars().count();
                    if char_count >= target {
                        Ok(Value::String(s.clone()))
                    } else {
                        let padding: String = std::iter::repeat(pad_char)
                            .take(target - char_count)
                            .collect();
                        Ok(Value::String(format!("{}{}", s, padding)))
                    }
                }
                _ => Err(RuntimeError::new("pad_end() requires (string, length)")),
            },
            "capitalize" => match args.first() {
                Some(Value::String(s)) => {
                    let mut chars = s.chars();
                    let result = match chars.next() {
                        Some(c) => {
                            let upper: String = c.to_uppercase().collect();
                            let rest: String = chars.collect::<String>().to_lowercase();
                            format!("{}{}", upper, rest)
                        }
                        None => String::new(),
                    };
                    Ok(Value::String(result))
                }
                _ => Err(RuntimeError::new("capitalize() requires a string")),
            },
            "title" => match args.first() {
                Some(Value::String(s)) => {
                    let result = s
                        .split_whitespace()
                        .map(|word| {
                            let mut chars = word.chars();
                            match chars.next() {
                                Some(c) => {
                                    let upper: String = c.to_uppercase().collect();
                                    let rest: String = chars.collect::<String>().to_lowercase();
                                    format!("{}{}", upper, rest)
                                }
                                None => String::new(),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    Ok(Value::String(result))
                }
                _ => Err(RuntimeError::new("title() requires a string")),
            },
            "repeat_str" => match (args.first(), args.get(1)) {
                (Some(Value::String(s)), Some(Value::Int(n))) => {
                    if *n < 0 {
                        return Err(RuntimeError::new("repeat_str() count must be non-negative"));
                    }
                    Ok(Value::String(s.repeat(*n as usize)))
                }
                _ => Err(RuntimeError::new("repeat_str() requires (string, count)")),
            },
            "count" => match (args.first(), args.get(1)) {
                (Some(Value::String(s)), Some(Value::String(substr))) => {
                    if substr.is_empty() {
                        return Ok(Value::Int((s.len() + 1) as i64));
                    }
                    Ok(Value::Int(s.matches(substr.as_str()).count() as i64))
                }
                _ => Err(RuntimeError::new("count() requires (string, substring)")),
            },
            // ===== Numeric Aggregates =====
            "sum" => match args.first() {
                Some(Value::Array(arr)) => {
                    let mut has_float = false;
                    let mut int_sum: i64 = 0;
                    let mut float_sum: f64 = 0.0;
                    for item in arr {
                        match item {
                            Value::Int(n) => {
                                int_sum += n;
                                float_sum += *n as f64;
                            }
                            Value::Float(n) => {
                                has_float = true;
                                float_sum += n;
                            }
                            _ => return Err(RuntimeError::new("sum() requires array of numbers")),
                        }
                    }
                    if has_float {
                        Ok(Value::Float(float_sum))
                    } else {
                        Ok(Value::Int(int_sum))
                    }
                }
                _ => Err(RuntimeError::new("sum() requires an array")),
            },
            "min_of" => match args.first() {
                Some(Value::Array(arr)) => {
                    if arr.is_empty() {
                        return Err(RuntimeError::new("min_of() requires a non-empty array"));
                    }
                    let mut result = arr[0].clone();
                    for item in &arr[1..] {
                        result = match (&result, item) {
                            (Value::Int(a), Value::Int(b)) => Value::Int(*a.min(b)),
                            (Value::Float(a), Value::Float(b)) => Value::Float(a.min(*b)),
                            (Value::Int(a), Value::Float(b)) => Value::Float((*a as f64).min(*b)),
                            (Value::Float(a), Value::Int(b)) => Value::Float(a.min(*b as f64)),
                            _ => {
                                return Err(RuntimeError::new("min_of() requires array of numbers"))
                            }
                        };
                    }
                    Ok(result)
                }
                _ => Err(RuntimeError::new("min_of() requires an array")),
            },
            "max_of" => match args.first() {
                Some(Value::Array(arr)) => {
                    if arr.is_empty() {
                        return Err(RuntimeError::new("max_of() requires a non-empty array"));
                    }
                    let mut result = arr[0].clone();
                    for item in &arr[1..] {
                        result = match (&result, item) {
                            (Value::Int(a), Value::Int(b)) => Value::Int(*a.max(b)),
                            (Value::Float(a), Value::Float(b)) => Value::Float(a.max(*b)),
                            (Value::Int(a), Value::Float(b)) => Value::Float((*a as f64).max(*b)),
                            (Value::Float(a), Value::Int(b)) => Value::Float(a.max(*b as f64)),
                            _ => {
                                return Err(RuntimeError::new("max_of() requires array of numbers"))
                            }
                        };
                    }
                    Ok(result)
                }
                _ => Err(RuntimeError::new("max_of() requires an array")),
            },
            // ===== Collection Operations =====
            "any" => {
                if args.len() < 2 {
                    return Err(RuntimeError::new("any() requires (array, predicate)"));
                }
                let arr = match &args[0] {
                    Value::Array(a) => a.clone(),
                    _ => return Err(RuntimeError::new("any() first argument must be an array")),
                };
                let func = args[1].clone();
                for item in arr {
                    let result = self.call_function(func.clone(), vec![item])?;
                    if result.is_truthy() {
                        return Ok(Value::Bool(true));
                    }
                }
                Ok(Value::Bool(false))
            }
            "all" => {
                if args.len() < 2 {
                    return Err(RuntimeError::new("all() requires (array, predicate)"));
                }
                let arr = match &args[0] {
                    Value::Array(a) => a.clone(),
                    _ => return Err(RuntimeError::new("all() first argument must be an array")),
                };
                let func = args[1].clone();
                for item in arr {
                    let result = self.call_function(func.clone(), vec![item])?;
                    if !result.is_truthy() {
                        return Ok(Value::Bool(false));
                    }
                }
                Ok(Value::Bool(true))
            }
            "unique" => match args.first() {
                Some(Value::Array(arr)) => {
                    let mut seen = Vec::new();
                    let mut result = Vec::new();
                    for item in arr {
                        let key = format!("{}", item);
                        if !seen.contains(&key) {
                            seen.push(key);
                            result.push(item.clone());
                        }
                    }
                    Ok(Value::Array(result))
                }
                _ => Err(RuntimeError::new("unique() requires an array")),
            },
            "zip" => match (args.first(), args.get(1)) {
                (Some(Value::Array(a)), Some(Value::Array(b))) => {
                    let pairs: Vec<Value> = a
                        .iter()
                        .zip(b.iter())
                        .map(|(x, y)| Value::Array(vec![x.clone(), y.clone()]))
                        .collect();
                    Ok(Value::Array(pairs))
                }
                _ => Err(RuntimeError::new("zip() requires two arrays")),
            },
            "flatten" => match args.first() {
                Some(Value::Array(arr)) => {
                    let mut result = Vec::new();
                    for item in arr {
                        match item {
                            Value::Array(inner) => result.extend(inner.clone()),
                            other => result.push(other.clone()),
                        }
                    }
                    Ok(Value::Array(result))
                }
                _ => Err(RuntimeError::new("flatten() requires an array")),
            },
            "group_by" => {
                if args.len() < 2 {
                    return Err(RuntimeError::new("group_by() requires (array, function)"));
                }
                let arr = match &args[0] {
                    Value::Array(a) => a.clone(),
                    _ => {
                        return Err(RuntimeError::new(
                            "group_by() first argument must be an array",
                        ))
                    }
                };
                let func = args[1].clone();
                let mut groups: IndexMap<String, Vec<Value>> = IndexMap::new();
                for item in arr {
                    let key = self.call_function(func.clone(), vec![item.clone()])?;
                    let key_str = format!("{}", key);
                    groups.entry(key_str).or_default().push(item);
                }
                let result: IndexMap<String, Value> = groups
                    .into_iter()
                    .map(|(k, v)| (k, Value::Array(v)))
                    .collect();
                Ok(Value::Object(result))
            }
            "chunk" => match (args.first(), args.get(1)) {
                (Some(Value::Array(arr)), Some(Value::Int(size))) => {
                    if *size <= 0 {
                        return Err(RuntimeError::new("chunk() size must be positive"));
                    }
                    let chunks: Vec<Value> = arr
                        .chunks(*size as usize)
                        .map(|c| Value::Array(c.to_vec()))
                        .collect();
                    Ok(Value::Array(chunks))
                }
                _ => Err(RuntimeError::new("chunk() requires (array, size)")),
            },
            "slice" => match args.first() {
                Some(Value::Array(arr)) => {
                    let start = match args.get(1) {
                        Some(Value::Int(n)) => {
                            let s = *n;
                            if s < 0 {
                                (arr.len() as i64 + s).max(0) as usize
                            } else {
                                s as usize
                            }
                        }
                        _ => 0,
                    };
                    let end = match args.get(2) {
                        Some(Value::Int(n)) => {
                            let e = *n;
                            if e < 0 {
                                (arr.len() as i64 + e).max(0) as usize
                            } else {
                                (e as usize).min(arr.len())
                            }
                        }
                        _ => arr.len(),
                    };
                    if start >= end || start >= arr.len() {
                        return Ok(Value::Array(vec![]));
                    }
                    Ok(Value::Array(arr[start..end].to_vec()))
                }
                Some(Value::String(s)) => {
                    let chars: Vec<char> = s.chars().collect();
                    let start = match args.get(1) {
                        Some(Value::Int(n)) => {
                            let st = *n;
                            if st < 0 {
                                (chars.len() as i64 + st).max(0) as usize
                            } else {
                                st as usize
                            }
                        }
                        _ => 0,
                    };
                    let end = match args.get(2) {
                        Some(Value::Int(n)) => {
                            let e = *n;
                            if e < 0 {
                                (chars.len() as i64 + e).max(0) as usize
                            } else {
                                (e as usize).min(chars.len())
                            }
                        }
                        _ => chars.len(),
                    };
                    if start >= end || start >= chars.len() {
                        return Ok(Value::String(String::new()));
                    }
                    Ok(Value::String(chars[start..end].iter().collect()))
                }
                _ => Err(RuntimeError::new(
                    "slice() requires an array or string as first argument",
                )),
            },
            // ===== Channel Operations =====
            "try_send" => {
                if args.len() < 2 {
                    return Err(RuntimeError::new("try_send() requires (channel, value)"));
                }
                let ch = match &args[0] {
                    Value::Channel(c) => c.clone(),
                    _ => {
                        return Err(RuntimeError::new(
                            "try_send() first argument must be a channel",
                        ))
                    }
                };
                let val = args[1].clone();
                let tx_guard = ch
                    .tx
                    .lock()
                    .map_err(|e| RuntimeError::new(&format!("channel lock error: {}", e)))?;
                match tx_guard.as_ref() {
                    Some(tx) => match tx.try_send(val) {
                        Ok(()) => Ok(Value::Bool(true)),
                        Err(_) => Ok(Value::Bool(false)),
                    },
                    None => Ok(Value::Bool(false)),
                }
            }
            "try_receive" => {
                if args.is_empty() {
                    return Err(RuntimeError::new("try_receive() requires a channel"));
                }
                let ch = match &args[0] {
                    Value::Channel(c) => c.clone(),
                    _ => {
                        return Err(RuntimeError::new(
                            "try_receive() argument must be a channel",
                        ))
                    }
                };
                let rx_guard = ch
                    .rx
                    .lock()
                    .map_err(|e| RuntimeError::new(&format!("channel lock error: {}", e)))?;
                match rx_guard.as_ref() {
                    Some(rx) => match rx.try_recv() {
                        Ok(val) => Ok(Value::Some(Box::new(val))),
                        Err(_) => Ok(Value::None),
                    },
                    None => Ok(Value::None),
                }
            }
            _ if name.starts_with("math.") => {
                crate::stdlib::math::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("fs.") => {
                crate::stdlib::fs::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("io.") => {
                crate::stdlib::io::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("crypto.") => {
                crate::stdlib::crypto::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("db.") => {
                crate::stdlib::db::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("env.") => {
                crate::stdlib::env::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("json.") => {
                crate::stdlib::json_module::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("regex.") => {
                crate::stdlib::regex_module::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("log.") => {
                crate::stdlib::log::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("pg.") => {
                crate::stdlib::pg::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("term.") => {
                crate::stdlib::term::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("http.") => {
                crate::stdlib::http::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("csv.") => {
                crate::stdlib::csv::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("time.") => {
                crate::stdlib::time::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("npc.") => {
                crate::stdlib::npc::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("url.") => {
                crate::stdlib::url_module::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("toml.") => {
                crate::stdlib::toml_module::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("ws.") => {
                crate::stdlib::ws::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("jwt.") => {
                crate::stdlib::jwt::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("mysql.") => {
                crate::stdlib::mysql::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            "input" => {
                use std::io::Read;
                let mut buffer = String::new();
                std::io::stdin().read_to_string(&mut buffer).ok();
                Ok(Value::String(buffer.trim_end().to_string()))
            }
            "exit" => {
                let code = match args.first() {
                    Some(Value::Int(n)) => *n as i32,
                    _ => 0,
                };
                std::process::exit(code);
            }
            "run_command" => {
                crate::stdlib::exec_module::call(args).map_err(|e| RuntimeError::new(&e))
            }
            "shell" => {
                let cmd = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("shell() requires a command string")),
                };
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| RuntimeError::new(&format!("shell error: {}", e)))?;
                let stdout = String::from_utf8_lossy(&output.stdout)
                    .trim_end()
                    .to_string();
                let stderr = String::from_utf8_lossy(&output.stderr)
                    .trim_end()
                    .to_string();
                let mut result = IndexMap::new();
                result.insert("stdout".to_string(), Value::String(stdout));
                result.insert("stderr".to_string(), Value::String(stderr));
                result.insert(
                    "status".to_string(),
                    Value::Int(output.status.code().unwrap_or(-1) as i64),
                );
                result.insert("ok".to_string(), Value::Bool(output.status.success()));
                Ok(Value::Object(result))
            }
            "sh" => {
                let cmd = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("sh() requires a command string")),
                };
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| RuntimeError::new(&format!("sh error: {}", e)))?;
                Ok(Value::String(
                    String::from_utf8_lossy(&output.stdout)
                        .trim_end()
                        .to_string(),
                ))
            }
            "sh_lines" => {
                let cmd = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("sh_lines() requires a command string")),
                };
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| RuntimeError::new(&format!("sh_lines error: {}", e)))?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let lines: Vec<Value> = stdout
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|l| Value::String(l.to_string()))
                    .collect();
                Ok(Value::Array(lines))
            }
            "sh_json" => {
                let cmd = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("sh_json() requires a command string")),
                };
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| RuntimeError::new(&format!("sh_json error: {}", e)))?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let json: serde_json::Value = serde_json::from_str(stdout.trim())
                    .map_err(|e| RuntimeError::new(&format!("sh_json parse error: {}", e)))?;
                Ok(crate::runtime::server::json_to_forge(json))
            }
            "sh_ok" => {
                let cmd = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("sh_ok() requires a command string")),
                };
                let status = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map_err(|e| RuntimeError::new(&format!("sh_ok error: {}", e)))?;
                Ok(Value::Bool(status.success()))
            }
            "which" => {
                let cmd = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("which() requires a command name")),
                };
                let result = std::process::Command::new("/usr/bin/which")
                    .arg(&cmd)
                    .output();
                match result {
                    Ok(output) if output.status.success() => Ok(Value::String(
                        String::from_utf8_lossy(&output.stdout).trim().to_string(),
                    )),
                    _ => Ok(Value::Null),
                }
            }
            "cwd" => {
                let path = std::env::current_dir()
                    .map_err(|e| RuntimeError::new(&format!("cwd error: {}", e)))?;
                Ok(Value::String(path.display().to_string()))
            }
            "cd" => {
                let path = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("cd() requires a path string")),
                };
                std::env::set_current_dir(&path)
                    .map_err(|e| RuntimeError::new(&format!("cd error: {}", e)))?;
                Ok(Value::String(path))
            }
            "lines" => match args.first() {
                Some(Value::String(s)) => {
                    let result: Vec<Value> =
                        s.lines().map(|l| Value::String(l.to_string())).collect();
                    Ok(Value::Array(result))
                }
                _ => Err(RuntimeError::new("lines() requires a string")),
            },
            "pipe_to" => {
                let (input, cmd) = match (args.first(), args.get(1)) {
                    (Some(Value::String(data)), Some(Value::String(cmd))) => {
                        (data.clone(), cmd.clone())
                    }
                    _ => {
                        return Err(RuntimeError::new(
                            "pipe_to() requires (data_string, command_string)",
                        ))
                    }
                };
                use std::io::Write;
                let mut child = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| RuntimeError::new(&format!("pipe_to error: {}", e)))?;
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(input.as_bytes());
                }
                let output = child
                    .wait_with_output()
                    .map_err(|e| RuntimeError::new(&format!("pipe_to error: {}", e)))?;
                let mut result = IndexMap::new();
                result.insert(
                    "stdout".to_string(),
                    Value::String(
                        String::from_utf8_lossy(&output.stdout)
                            .trim_end()
                            .to_string(),
                    ),
                );
                result.insert(
                    "stderr".to_string(),
                    Value::String(
                        String::from_utf8_lossy(&output.stderr)
                            .trim_end()
                            .to_string(),
                    ),
                );
                result.insert(
                    "status".to_string(),
                    Value::Int(output.status.code().unwrap_or(-1) as i64),
                );
                result.insert("ok".to_string(), Value::Bool(output.status.success()));
                Ok(Value::Object(result))
            }
            // ========== GenZ Debug Kit ==========
            "sus" => {
                // sus(value) — inspect a value with attitude, returns it (pass-through like Rust's dbg!)
                if args.is_empty() {
                    return Err(RuntimeError::new(
                        "sus() needs something to inspect, bestie",
                    ));
                }
                let val = &args[0];
                let type_str = val.type_name();
                let display = match val {
                    Value::String(s) => format!("\"{}\"", s),
                    other => format!("{}", other),
                };
                eprintln!(
                    "\x1b[33m🔍 SUS CHECK:\x1b[0m {} \x1b[2m({})\x1b[0m",
                    display, type_str
                );
                Ok(args.into_iter().next().unwrap())
            }
            "bruh" => {
                // bruh(msg) — panic with GenZ energy
                let msg = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    Some(other) => format!("{}", other),
                    None => "something ain't right".to_string(),
                };
                Err(RuntimeError::new(&format!("BRUH: {}", msg)))
            }
            "bet" => {
                // bet(condition, msg?) — assert with swagger
                let condition = match args.first() {
                    Some(Value::Bool(b)) => *b,
                    Some(_) => true,
                    None => return Err(RuntimeError::new("bet() needs a condition, no cap")),
                };
                if condition {
                    Ok(Value::Bool(true))
                } else {
                    let msg = match args.get(1) {
                        Some(Value::String(s)) => s.clone(),
                        _ => "condition was false".to_string(),
                    };
                    Err(RuntimeError::new(&format!("LOST THE BET: {}", msg)))
                }
            }
            "no_cap" => {
                // no_cap(a, b) — assert_eq but GenZ
                if args.len() < 2 {
                    return Err(RuntimeError::new(
                        "no_cap() needs two values to compare, fr fr",
                    ));
                }
                let a = &args[0];
                let b = &args[1];
                if a == b {
                    Ok(Value::Bool(true))
                } else {
                    Err(RuntimeError::new(&format!("CAP DETECTED: {} ≠ {}", a, b)))
                }
            }
            "ick" => {
                // ick(condition, msg?) — assert something is FALSE
                let condition = match args.first() {
                    Some(Value::Bool(b)) => *b,
                    Some(_) => true,
                    None => return Err(RuntimeError::new("ick() needs a condition to reject")),
                };
                if !condition {
                    Ok(Value::Bool(true))
                } else {
                    let msg = match args.get(1) {
                        Some(Value::String(s)) => s.clone(),
                        _ => "that's an ick".to_string(),
                    };
                    Err(RuntimeError::new(&format!("ICK: {}", msg)))
                }
            }

            // ========== Execution Helpers ==========
            "cook" => {
                // cook(fn) — time execution with personality
                let func = match args.first() {
                    Some(f @ Value::Lambda { .. }) | Some(f @ Value::Function { .. }) => f.clone(),
                    _ => return Err(RuntimeError::new("cook() needs a function — let him cook!")),
                };
                let start = std::time::Instant::now();
                let result = self.call_function(func, vec![])?;
                let elapsed = start.elapsed();
                let ms = elapsed.as_secs_f64() * 1000.0;
                if ms < 1.0 {
                    eprintln!(
                        "\x1b[32m👨‍🍳 COOKED:\x1b[0m done in {:.2}µs — \x1b[2mspeed demon fr\x1b[0m",
                        elapsed.as_secs_f64() * 1_000_000.0
                    );
                } else if ms < 100.0 {
                    eprintln!("\x1b[32m👨‍🍳 COOKED:\x1b[0m done in {:.2}ms — \x1b[2mno cap that was fast\x1b[0m", ms);
                } else if ms < 1000.0 {
                    eprintln!("\x1b[33m👨‍🍳 COOKED:\x1b[0m done in {:.0}ms — \x1b[2mit's giving adequate\x1b[0m", ms);
                } else {
                    eprintln!("\x1b[31m👨‍🍳 COOKED:\x1b[0m done in {:.2}s — \x1b[2mbruh that took a minute\x1b[0m", elapsed.as_secs_f64());
                }
                Ok(result)
            }
            "yolo" => {
                // yolo(fn) — swallow ALL errors, return None on failure
                let func = match args.first() {
                    Some(f @ Value::Lambda { .. }) | Some(f @ Value::Function { .. }) => f.clone(),
                    _ => return Err(RuntimeError::new("yolo() needs a function to send it on")),
                };
                match self.call_function(func, vec![]) {
                    Ok(val) => Ok(val),
                    Err(_) => Ok(Value::None),
                }
            }
            "ghost" => {
                // ghost(fn) — capture all println/say output, return as string
                // Note: In a real implementation this would redirect stdout.
                // For now, we execute and return the result silently.
                let func = match args.first() {
                    Some(f @ Value::Lambda { .. }) | Some(f @ Value::Function { .. }) => f.clone(),
                    _ => return Err(RuntimeError::new("ghost() needs a function to haunt")),
                };
                // Execute the function, capturing its return value
                let result = self.call_function(func, vec![])?;
                Ok(result)
            }
            "slay" => {
                // slay(fn, n?) — benchmark function n times, return stats
                let func = match args.first() {
                    Some(f @ Value::Lambda { .. }) | Some(f @ Value::Function { .. }) => f.clone(),
                    _ => return Err(RuntimeError::new("slay() needs a function to benchmark")),
                };
                let n = match args.get(1) {
                    Some(Value::Int(n)) => *n as usize,
                    _ => 100,
                };
                let mut times: Vec<f64> = Vec::with_capacity(n);
                let mut last_result = Value::Null;
                for _ in 0..n {
                    let start = std::time::Instant::now();
                    last_result = self.call_function(func.clone(), vec![])?;
                    times.push(start.elapsed().as_secs_f64() * 1000.0);
                }
                times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let avg = times.iter().sum::<f64>() / times.len() as f64;
                let min = times.first().copied().unwrap_or(0.0);
                let max = times.last().copied().unwrap_or(0.0);
                let p99_idx = ((times.len() as f64) * 0.99) as usize;
                let p99 = times
                    .get(p99_idx.min(times.len() - 1))
                    .copied()
                    .unwrap_or(0.0);
                let mut stats = IndexMap::new();
                stats.insert("avg_ms".to_string(), Value::Float(avg));
                stats.insert("min_ms".to_string(), Value::Float(min));
                stats.insert("max_ms".to_string(), Value::Float(max));
                stats.insert("p99_ms".to_string(), Value::Float(p99));
                stats.insert("runs".to_string(), Value::Int(n as i64));
                stats.insert("result".to_string(), last_result);
                eprintln!(
                    "\x1b[35m💅 SLAYED:\x1b[0m {}x runs — avg {:.3}ms, min {:.3}ms, max {:.3}ms, p99 {:.3}ms",
                    n, avg, min, max, p99
                );
                Ok(Value::Object(stats))
            }

            // ========== String Utils ==========
            "slugify" => {
                // slugify(str) — URL-friendly string
                let s = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("slugify() requires a string")),
                };
                let slug: String = s
                    .to_lowercase()
                    .chars()
                    .map(|c| if c.is_alphanumeric() { c } else { '-' })
                    .collect::<String>()
                    .split('-')
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<&str>>()
                    .join("-");
                Ok(Value::String(slug))
            }
            "snake_case" => {
                // snake_case(str) — convert camelCase/PascalCase/spaces to snake_case
                let s = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("snake_case() requires a string")),
                };
                let chars: Vec<char> = s.chars().collect();
                let mut result = String::new();
                for i in 0..chars.len() {
                    let c = chars[i];
                    if c.is_uppercase() {
                        if i > 0 {
                            let prev = chars[i - 1];
                            if prev.is_lowercase() || prev.is_numeric() {
                                result.push('_');
                            } else if prev.is_uppercase()
                                && i + 1 < chars.len()
                                && chars[i + 1].is_lowercase()
                            {
                                // Handle transitions like "APIKey" → "api_key"
                                result.push('_');
                            }
                        }
                        result.push(c.to_lowercase().next().unwrap_or(c));
                    } else if c == ' ' || c == '-' {
                        result.push('_');
                    } else {
                        result.push(c);
                    }
                }
                Ok(Value::String(result))
            }
            "camel_case" => {
                // camel_case(str) — convert snake_case/spaces to camelCase
                let s = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("camel_case() requires a string")),
                };
                let parts: Vec<&str> = s
                    .split(|c: char| c == '_' || c == ' ' || c == '-')
                    .filter(|s| !s.is_empty())
                    .collect();
                let mut result = String::new();
                for (i, part) in parts.iter().enumerate() {
                    if i == 0 {
                        result.push_str(&part.to_lowercase());
                    } else {
                        let mut chars = part.chars();
                        if let Some(first) = chars.next() {
                            result.push(first.to_uppercase().next().unwrap_or(first));
                            result.push_str(&chars.as_str().to_lowercase());
                        }
                    }
                }
                Ok(Value::String(result))
            }

            // ========== Array Utils ==========
            "sample" => {
                // sample(arr, n?) — random N items from array
                match args.first() {
                    Some(Value::Array(items)) => {
                        let n = match args.get(1) {
                            Some(Value::Int(n)) => *n as usize,
                            _ => 1,
                        };
                        if items.is_empty() {
                            return Ok(Value::Array(vec![]));
                        }
                        use std::time::{SystemTime, UNIX_EPOCH};
                        let seed = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos() as u64;
                        let mut result = Vec::with_capacity(n);
                        for i in 0..n {
                            let mut x = seed.wrapping_add(i as u64);
                            x ^= x << 13;
                            x ^= x >> 7;
                            x ^= x << 17;
                            let idx = (x % items.len() as u64) as usize;
                            result.push(items[idx].clone());
                        }
                        if n == 1 {
                            Ok(result.into_iter().next().unwrap_or(Value::Null))
                        } else {
                            Ok(Value::Array(result))
                        }
                    }
                    _ => Err(RuntimeError::new("sample() requires an array")),
                }
            }
            "shuffle" => {
                // shuffle(arr) — Fisher-Yates shuffle
                match args.into_iter().next() {
                    Some(Value::Array(mut items)) => {
                        use std::time::{SystemTime, UNIX_EPOCH};
                        let mut seed = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos() as u64;
                        for i in (1..items.len()).rev() {
                            seed ^= seed << 13;
                            seed ^= seed >> 7;
                            seed ^= seed << 17;
                            let j = (seed % (i as u64 + 1)) as usize;
                            items.swap(i, j);
                        }
                        Ok(Value::Array(items))
                    }
                    _ => Err(RuntimeError::new("shuffle() requires an array")),
                }
            }
            "partition" => {
                // partition(arr, fn) — split into [matching, non-matching]
                if args.len() < 2 {
                    return Err(RuntimeError::new(
                        "partition() requires an array and a function",
                    ));
                }
                let items = match &args[0] {
                    Value::Array(items) => items.clone(),
                    _ => return Err(RuntimeError::new("partition() first arg must be an array")),
                };
                let func = args[1].clone();
                let mut matches = Vec::new();
                let mut rest = Vec::new();
                for item in items {
                    let result = self.call_function(func.clone(), vec![item.clone()])?;
                    if result.is_truthy() {
                        matches.push(item);
                    } else {
                        rest.push(item);
                    }
                }
                Ok(Value::Array(vec![
                    Value::Array(matches),
                    Value::Array(rest),
                ]))
            }
            "diff" => {
                // diff(a, b) — deep object comparison
                if args.len() < 2 {
                    return Err(RuntimeError::new("diff() requires two values to compare"));
                }
                let a = &args[0];
                let b = &args[1];
                fn diff_values(a: &Value, b: &Value) -> Value {
                    if a == b {
                        return Value::Null;
                    }
                    match (a, b) {
                        (Value::Object(map_a), Value::Object(map_b)) => {
                            let mut changes = IndexMap::new();
                            // Check keys in a
                            for (key, val_a) in map_a {
                                if key.starts_with("__") {
                                    continue;
                                }
                                match map_b.get(key) {
                                    Some(val_b) => {
                                        let d = diff_values(val_a, val_b);
                                        if d != Value::Null {
                                            let mut change = IndexMap::new();
                                            change.insert("from".to_string(), val_a.clone());
                                            change.insert("to".to_string(), val_b.clone());
                                            changes.insert(key.clone(), Value::Object(change));
                                        }
                                    }
                                    None => {
                                        let mut change = IndexMap::new();
                                        change.insert("removed".to_string(), val_a.clone());
                                        changes.insert(key.clone(), Value::Object(change));
                                    }
                                }
                            }
                            // Check keys only in b
                            for (key, val_b) in map_b {
                                if key.starts_with("__") {
                                    continue;
                                }
                                if !map_a.contains_key(key) {
                                    let mut change = IndexMap::new();
                                    change.insert("added".to_string(), val_b.clone());
                                    changes.insert(key.clone(), Value::Object(change));
                                }
                            }
                            if changes.is_empty() {
                                Value::Null
                            } else {
                                Value::Object(changes)
                            }
                        }
                        _ => {
                            let mut change = IndexMap::new();
                            change.insert("from".to_string(), a.clone());
                            change.insert("to".to_string(), b.clone());
                            Value::Object(change)
                        }
                    }
                }
                let result = diff_values(a, b);
                Ok(result)
            }

            _ if name.starts_with("adt:") => {
                let parts: Vec<&str> = name.splitn(4, ':').collect();
                if parts.len() == 4 {
                    let type_name = parts[1];
                    let variant_name = parts[2];
                    let field_count: usize = parts[3].parse().unwrap_or(0);
                    if args.len() != field_count {
                        return Err(RuntimeError::new(&format!(
                            "{}() expects {} argument(s), got {}",
                            variant_name,
                            field_count,
                            args.len()
                        )));
                    }
                    let mut obj = IndexMap::new();
                    obj.insert("__type__".to_string(), Value::String(type_name.to_string()));
                    obj.insert(
                        "__variant__".to_string(),
                        Value::String(variant_name.to_string()),
                    );
                    for (i, arg) in args.into_iter().enumerate() {
                        obj.insert(format!("_{}", i), arg);
                    }
                    Ok(Value::Object(obj))
                } else {
                    Err(RuntimeError::new(&format!(
                        "invalid ADT constructor: {}",
                        name
                    )))
                }
            }
            _ => Err(RuntimeError::new(&format!("unknown builtin: {}", name))),
        }
    }
}
