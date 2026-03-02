use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("parse".to_string(), Value::BuiltIn("url.parse".to_string()));
    m.insert(
        "encode".to_string(),
        Value::BuiltIn("url.encode".to_string()),
    );
    m.insert(
        "decode".to_string(),
        Value::BuiltIn("url.decode".to_string()),
    );
    m.insert("build".to_string(), Value::BuiltIn("url.build".to_string()));
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "url.parse" => match args.first() {
            Some(Value::String(s)) => {
                let parsed = url::Url::parse(s).map_err(|e| format!("URL parse error: {}", e))?;
                let mut result = IndexMap::new();
                result.insert(
                    "scheme".to_string(),
                    Value::String(parsed.scheme().to_string()),
                );
                result.insert(
                    "host".to_string(),
                    parsed
                        .host_str()
                        .map(|h| Value::String(h.to_string()))
                        .unwrap_or(Value::Null),
                );
                result.insert(
                    "port".to_string(),
                    parsed
                        .port()
                        .map(|p| Value::Int(p as i64))
                        .unwrap_or(Value::Null),
                );
                result.insert("path".to_string(), Value::String(parsed.path().to_string()));
                result.insert(
                    "query".to_string(),
                    parsed
                        .query()
                        .map(|q| Value::String(q.to_string()))
                        .unwrap_or(Value::Null),
                );
                result.insert(
                    "fragment".to_string(),
                    parsed
                        .fragment()
                        .map(|f| Value::String(f.to_string()))
                        .unwrap_or(Value::Null),
                );
                // Parse query params into an object
                let mut params = IndexMap::new();
                for (key, value) in parsed.query_pairs() {
                    params.insert(key.to_string(), Value::String(value.to_string()));
                }
                result.insert("params".to_string(), Value::Object(params));
                result.insert(
                    "origin".to_string(),
                    Value::String(parsed.origin().ascii_serialization()),
                );
                result.insert(
                    "username".to_string(),
                    if parsed.username().is_empty() {
                        Value::Null
                    } else {
                        Value::String(parsed.username().to_string())
                    },
                );
                result.insert(
                    "password".to_string(),
                    parsed
                        .password()
                        .map(|p| Value::String(p.to_string()))
                        .unwrap_or(Value::Null),
                );
                Ok(Value::Object(result))
            }
            _ => Err("url.parse() requires a URL string".to_string()),
        },
        "url.encode" => match args.first() {
            Some(Value::String(s)) => {
                let encoded: String = s
                    .chars()
                    .map(|c| match c {
                        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
                        _ => format!("%{:02X}", c as u32),
                    })
                    .collect();
                Ok(Value::String(encoded))
            }
            _ => Err("url.encode() requires a string".to_string()),
        },
        "url.decode" => match args.first() {
            Some(Value::String(s)) => {
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
                Ok(Value::String(result))
            }
            _ => Err("url.decode() requires a string".to_string()),
        },
        "url.build" => match args.first() {
            Some(Value::Object(opts)) => {
                let scheme = match opts.get("scheme") {
                    Some(Value::String(s)) => s.clone(),
                    _ => "https".to_string(),
                };
                let host = match opts.get("host") {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err("url.build() requires a 'host' field".to_string()),
                };
                let port = opts.get("port").and_then(|v| {
                    if let Value::Int(n) = v {
                        Some(*n as u16)
                    } else {
                        None
                    }
                });
                let path = match opts.get("path") {
                    Some(Value::String(s)) => s.clone(),
                    _ => "/".to_string(),
                };

                let mut url_str = if let Some(p) = port {
                    format!("{}://{}:{}{}", scheme, host, p, path)
                } else {
                    format!("{}://{}{}", scheme, host, path)
                };

                // Add query params
                if let Some(Value::Object(params)) = opts.get("params") {
                    if !params.is_empty() {
                        let pairs: Vec<String> = params
                            .iter()
                            .map(|(k, v)| format!("{}={}", k, format!("{}", v)))
                            .collect();
                        url_str.push('?');
                        url_str.push_str(&pairs.join("&"));
                    }
                }

                if let Some(Value::String(frag)) = opts.get("fragment") {
                    url_str.push('#');
                    url_str.push_str(frag);
                }

                Ok(Value::String(url_str))
            }
            _ => Err("url.build() requires an options object".to_string()),
        },
        _ => Err(format!("unknown url function: {}", name)),
    }
}
