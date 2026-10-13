use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert(
        "parse".to_string(),
        Value::BuiltIn("json.parse".to_string()),
    );
    m.insert(
        "stringify".to_string(),
        Value::BuiltIn("json.stringify".to_string()),
    );
    m.insert(
        "pretty".to_string(),
        Value::BuiltIn("json.pretty".to_string()),
    );
    m.insert(
        "compact".to_string(),
        Value::BuiltIn("json.compact".to_string()),
    );
    m.insert(
        "valid".to_string(),
        Value::BuiltIn("json.valid".to_string()),
    );
    m.insert(
        "merge".to_string(),
        Value::BuiltIn("json.merge".to_string()),
    );
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "json.parse" => match args.first() {
            Some(Value::String(s)) => parse_str(s).map_err(|e| parse_error("", &e)),
            _ => Err("json.parse() requires a string".to_string()),
        },
        "json.stringify" => match args.first() {
            Some(v) => {
                validate_json_value(v)?;
                Ok(Value::String(forge_to_json_line(v, &SPACED)))
            }
            None => Err("json.stringify() requires a value".to_string()),
        },
        "json.compact" => match args.first() {
            Some(v) => {
                validate_json_value(v)?;
                Ok(Value::String(forge_to_json_line(v, &COMPACT)))
            }
            None => Err("json.compact() requires a value".to_string()),
        },
        "json.pretty" => match args.first() {
            Some(v) => {
                validate_json_value(v)?;
                let indent = match args.get(1) {
                    None | Some(Value::Null) => 2,
                    Some(Value::Int(n)) if (0..=MAX_INDENT).contains(n) => *n as usize,
                    Some(_) => {
                        return Err(format!(
                            "json.pretty() requires an indent from 0 to {}",
                            MAX_INDENT
                        ))
                    }
                };
                Ok(Value::String(forge_to_json_pretty(v, 0, indent)))
            }
            None => Err("json.pretty() requires a value".to_string()),
        },
        "json.valid" => match args.first() {
            Some(Value::String(s)) => Ok(Value::Bool(
                serde_json::from_str::<serde_json::Value>(s).is_ok(),
            )),
            _ => Ok(Value::Bool(false)),
        },
        "json.merge" => match (args.first(), args.get(1)) {
            (Some(Value::Object(a)), Some(Value::Object(b))) => Ok(Value::Object(deep_merge(a, b))),
            _ => Err("json.merge() requires two objects".to_string()),
        },
        _ => Err(format!("unknown json function: {}", name)),
    }
}

fn deep_merge(
    a: &indexmap::IndexMap<String, Value>,
    b: &indexmap::IndexMap<String, Value>,
) -> indexmap::IndexMap<String, Value> {
    let mut result = a.clone();
    for (key, b_val) in b {
        match (result.get(key), b_val) {
            (Some(Value::Object(a_inner)), Value::Object(b_inner)) => {
                result.insert(key.clone(), Value::Object(deep_merge(a_inner, b_inner)));
            }
            _ => {
                result.insert(key.clone(), b_val.clone());
            }
        }
    }
    result
}

/// Parse JSON text into a Forge value. Object keys keep the order they
/// have in the text (Forge objects are insertion-ordered); a repeated key
/// keeps its first position and its last value. Use this rather than
/// `serde_json::Value` + [`json_to_forge`]: `serde_json::Value` objects
/// (without the `preserve_order` feature) sort their keys.
///
/// Numbers: a number written without a fraction or exponent is an Int when
/// it fits in 64 bits (`-0` is the Int `0`, as in Forge source); a larger
/// one becomes a Float (precision may be lost), the way Int arithmetic that
/// overflows becomes a Float. A number with `.` or an exponent is a Float
/// (`1.0`, `1e2`, `-0.0`).
pub fn parse_str(text: &str) -> Result<Value, serde_json::Error> {
    let text = int_negative_zero(text);
    serde_json::from_str::<Ordered>(&text).map(|o| o.0)
}

/// `json.compact(v)`: the JSON text a value is sent as (HTTP request
/// bodies, `ws.send`).
pub fn to_compact(v: &Value) -> Result<String, String> {
    validate_json_value(v)?;
    Ok(forge_to_json_line(v, &COMPACT))
}

/// Largest `json.pretty` indent (a bigger one is a mistake, and the
/// padding is built per line).
const MAX_INDENT: i64 = 16;

/// The error for JSON text that does not parse (E0039, `semantics::errors`):
/// `JSON parse error: ...`, after `context: ` when the text came from
/// somewhere other than `json.parse` (`fs.read_json: JSON parse error: ...`).
pub fn parse_error(context: &str, e: &serde_json::Error) -> String {
    if context.is_empty() {
        format!("{}: {}", crate::semantics::JSON_PARSE_ERROR, e)
    } else {
        format!("{}: {}: {}", context, crate::semantics::JSON_PARSE_ERROR, e)
    }
}

/// JSON text of a Float. It always has a fraction or an exponent (`1.0`,
/// `2.5`, `1e+21`), so it parses back as a Float; NaN and the infinities,
/// which JSON cannot represent, are `null` (as in JavaScript). The digits
/// are the shortest that round-trip, as in the HTTP server's responses
/// (both go through `serde_json`).
pub fn float_text(f: f64) -> String {
    match serde_json::Number::from_f64(f) {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    }
}

/// `serde_json` reads the integer `-0` as the Float `-0.0`, which the
/// visitor cannot tell from `-0.0`. Rewrite each `-0` number token (one
/// that starts a value and has no fraction or exponent) to ` 0`: same
/// length, so error positions do not move. Strings are skipped.
fn int_negative_zero(text: &str) -> std::borrow::Cow<'_, str> {
    let bytes = text.as_bytes();
    let mut hits = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    // The last byte outside strings that was not whitespace.
    let mut prev: Option<u8> = None;
    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
                prev = Some(b);
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b' ' | b'\t' | b'\n' | b'\r' => continue,
            b'-' if matches!(prev, None | Some(b'[' | b',' | b':'))
                && bytes.get(i + 1) == Some(&b'0')
                && !matches!(bytes.get(i + 2), Some(b'.' | b'e' | b'E' | b'0'..=b'9')) =>
            {
                hits.push(i);
            }
            _ => {}
        }
        prev = Some(b);
    }
    if hits.is_empty() {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = text.to_string().into_bytes();
    for i in hits {
        out[i] = b' ';
    }
    // Only ASCII `-` bytes were replaced by ASCII spaces.
    std::borrow::Cow::Owned(String::from_utf8(out).expect("BUG: ASCII-for-ASCII replacement"))
}

/// A Forge value deserialized straight from JSON, object keys in document
/// order (see [`parse_str`]). Also the HTTP server's request-body type.
pub struct Ordered(pub Value);

impl<'de> serde::Deserialize<'de> for Ordered {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(OrderedVisitor).map(Ordered)
    }
}

struct OrderedVisitor;

impl<'de> serde::de::Visitor<'de> for OrderedVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, b: bool) -> Result<Value, E> {
        Ok(Value::Bool(b))
    }

    fn visit_i64<E>(self, n: i64) -> Result<Value, E> {
        Ok(Value::Int(n))
    }

    // Same mapping as `json_to_forge`: beyond i64 becomes a Float.
    fn visit_u64<E>(self, n: u64) -> Result<Value, E> {
        Ok(i64::try_from(n).map_or(Value::Float(n as f64), Value::Int))
    }

    fn visit_f64<E>(self, f: f64) -> Result<Value, E> {
        Ok(Value::Float(f))
    }

    fn visit_str<E>(self, s: &str) -> Result<Value, E> {
        Ok(Value::String(s.to_owned()))
    }

    fn visit_string<E>(self, s: String) -> Result<Value, E> {
        Ok(Value::String(s))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: serde::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        <Ordered as serde::Deserialize>::deserialize(d).map(|o| o.0)
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(Ordered(v)) = seq.next_element()? {
            items.push(v);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut out = IndexMap::new();
        while let Some((k, Ordered(v))) = map.next_entry::<String, Ordered>()? {
            out.insert(k, v);
        }
        Ok(Value::Object(out))
    }
}

pub fn json_to_forge(v: serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(items) => {
            Value::Array(items.into_iter().map(json_to_forge).collect())
        }
        serde_json::Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, json_to_forge(v)))
                .collect(),
        ),
    }
}

/// A string as a JSON string literal: `"` and `\` escaped, the short
/// escapes `\b \f \n \r \t`, any other control character as `\u00XX`.
/// Also how both engines show strings inside collections.
pub fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if c < '\x20' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Recursively validate that a value is JSON-serializable. Fails if any
/// Map contains a non-string key — JSON objects require string keys and
/// we refuse to silently coerce.
fn validate_json_value(v: &Value) -> Result<(), String> {
    // Checked before serializing, so the (recursive) writers below never
    // see a value nested deeper than this.
    let Some(_level) = crate::runtime::recursion::enter_value_level() else {
        crate::runtime::recursion::take_value_too_deep();
        return Err(format!(
            "json: {}",
            crate::runtime::recursion::value_too_deep_message()
        ));
    };
    match v {
        Value::Array(items) | Value::Tuple(items) => {
            for item in items {
                validate_json_value(item)?;
            }
            Ok(())
        }
        Value::Set(items) => {
            for item in items.iter() {
                validate_json_value(item)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for v in map.values() {
                validate_json_value(v)?;
            }
            Ok(())
        }
        Value::Map(pairs) => {
            for (k, v) in pairs {
                if !matches!(k, Value::String(_)) {
                    return Err(format!(
                        "json: cannot serialize map with non-string key ({})",
                        k.type_name()
                    ));
                }
                validate_json_value(v)?;
            }
            Ok(())
        }
        Value::Frozen(inner) => validate_json_value(inner),
        Value::Stream(_) => Err(
            "Stream cannot cross the VM/interpreter boundary; call .collect() first to materialize"
                .to_string(),
        ),
        _ => Ok(()),
    }
}

/// Separators of a one-line encoding: between items, and after a key.
struct Separators {
    item: &'static str,
    key: &'static str,
}

/// `json.stringify`: `{"a": 1, "b": [1, 2]}`.
const SPACED: Separators = Separators {
    item: ", ",
    key: ": ",
};

/// `json.compact`: `{"a":1,"b":[1,2]}` (no whitespace at all).
const COMPACT: Separators = Separators {
    item: ",",
    key: ":",
};

/// One-line JSON for a validated value; keys keep their order.
fn forge_to_json_line(v: &Value, sep: &Separators) -> String {
    let mut out = String::new();
    write_json_line(&mut out, v, sep);
    out
}

fn write_json_items<'a>(
    out: &mut String,
    items: impl Iterator<Item = &'a Value>,
    sep: &Separators,
) {
    out.push('[');
    for (i, item) in items.enumerate() {
        if i > 0 {
            out.push_str(sep.item);
        }
        write_json_line(out, item, sep);
    }
    out.push(']');
}

fn write_json_line(out: &mut String, v: &Value, sep: &Separators) {
    match v {
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::Float(n) => out.push_str(&float_text(*n)),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Null => out.push_str("null"),
        Value::String(s) => out.push_str(&escape_json_string(s)),
        Value::Array(items) | Value::Tuple(items) => write_json_items(out, items.iter(), sep),
        Value::Set(items) => write_json_items(out, items.iter(), sep),
        Value::Object(map) => {
            out.push('{');
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(sep.item);
                }
                out.push_str(&escape_json_string(k));
                out.push_str(sep.key);
                write_json_line(out, v, sep);
            }
            out.push('}');
        }
        Value::Map(pairs) => {
            out.push('{');
            for (i, (k, v)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push_str(sep.item);
                }
                // Validation guarantees all keys are strings at this point.
                let key = match k {
                    Value::String(s) => s.as_str(),
                    _ => "",
                };
                out.push_str(&escape_json_string(key));
                out.push_str(sep.key);
                write_json_line(out, v, sep);
            }
            out.push('}');
        }
        Value::Frozen(inner) => write_json_line(out, inner, sep),
        _ => out.push_str("null"),
    }
}

fn forge_to_json_pretty(v: &Value, depth: usize, indent: usize) -> String {
    let pad = " ".repeat(depth * indent);
    let inner_pad = " ".repeat((depth + 1) * indent);
    match v {
        Value::Int(n) => n.to_string(),
        Value::Float(n) => float_text(*n),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::String(s) => escape_json_string(s),
        Value::Array(_) | Value::Tuple(_) | Value::Set(_) => {
            let items: Vec<&Value> = match v {
                Value::Array(items) | Value::Tuple(items) => items.iter().collect(),
                Value::Set(items) => items.iter().collect(),
                _ => Vec::new(),
            };
            if items.is_empty() {
                return "[]".to_string();
            }
            let entries: Vec<String> = items
                .into_iter()
                .map(|v| {
                    format!(
                        "{}{}",
                        inner_pad,
                        forge_to_json_pretty(v, depth + 1, indent)
                    )
                })
                .collect();
            format!("[\n{}\n{}]", entries.join(",\n"), pad)
        }
        Value::Frozen(inner) => forge_to_json_pretty(inner, depth, indent),
        Value::Object(map) => {
            if map.is_empty() {
                return "{}".to_string();
            }
            let entries: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}{}: {}",
                        inner_pad,
                        escape_json_string(k),
                        forge_to_json_pretty(v, depth + 1, indent)
                    )
                })
                .collect();
            format!("{{\n{}\n{}}}", entries.join(",\n"), pad)
        }
        Value::Map(pairs) => {
            if pairs.is_empty() {
                return "{}".to_string();
            }
            let entries: Vec<String> = pairs
                .iter()
                .map(|(k, v)| {
                    let ks = if let Value::String(s) = k {
                        escape_json_string(s)
                    } else {
                        escape_json_string("")
                    };
                    format!(
                        "{}{}: {}",
                        inner_pad,
                        ks,
                        forge_to_json_pretty(v, depth + 1, indent)
                    )
                })
                .collect();
            format!("{{\n{}\n{}}}", entries.join(",\n"), pad)
        }
        _ => "null".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Value {
        Value::String(v.to_string())
    }

    #[test]
    fn module_has_all_functions() {
        if let Value::Object(m) = create_module() {
            for k in ["parse", "stringify", "pretty", "valid", "merge"] {
                assert!(m.contains_key(k), "missing {}", k);
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn parse_primitives() {
        assert_eq!(call("json.parse", vec![s("null")]).unwrap(), Value::Null);
        assert_eq!(
            call("json.parse", vec![s("true")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call("json.parse", vec![s("false")]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(call("json.parse", vec![s("42")]).unwrap(), Value::Int(42));
        assert_eq!(
            call("json.parse", vec![s("3.14")]).unwrap(),
            Value::Float(3.14)
        );
        assert_eq!(call("json.parse", vec![s("\"hi\"")]).unwrap(), s("hi"));
    }

    #[test]
    fn parse_array() {
        let result = call("json.parse", vec![s("[1, 2, 3]")]).unwrap();
        assert_eq!(
            result,
            Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    #[test]
    fn parse_object() {
        let result = call("json.parse", vec![s("{\"a\": 1, \"b\": \"two\"}")]).unwrap();
        if let Value::Object(m) = result {
            assert_eq!(m.get("a"), Some(&Value::Int(1)));
            assert_eq!(m.get("b"), Some(&s("two")));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn parse_nested() {
        let result = call(
            "json.parse",
            vec![s("{\"users\": [{\"id\": 1}, {\"id\": 2}]}")],
        )
        .unwrap();
        if let Value::Object(m) = result {
            if let Some(Value::Array(users)) = m.get("users") {
                assert_eq!(users.len(), 2);
            } else {
                panic!("expected users array");
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn parse_invalid_errors() {
        let result = call("json.parse", vec![s("not json")]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("JSON parse error"));
    }

    #[test]
    fn stringify_primitives() {
        assert_eq!(
            call("json.stringify", vec![Value::Null]).unwrap(),
            s("null")
        );
        assert_eq!(
            call("json.stringify", vec![Value::Bool(true)]).unwrap(),
            s("true")
        );
        assert_eq!(
            call("json.stringify", vec![Value::Int(42)]).unwrap(),
            s("42")
        );
        assert_eq!(call("json.stringify", vec![s("hi")]).unwrap(), s("\"hi\""));
    }

    #[test]
    fn stringify_array() {
        let result = call(
            "json.stringify",
            vec![Value::Array(vec![Value::Int(1), Value::Int(2)])],
        )
        .unwrap();
        assert_eq!(result, s("[1, 2]"));
    }

    #[test]
    fn stringify_escapes_quotes() {
        let result = call("json.stringify", vec![s("she said \"hi\"")]).unwrap();
        assert_eq!(result, s("\"she said \\\"hi\\\"\""));
    }

    #[test]
    fn stringify_rejects_non_string_map_keys() {
        let result = call(
            "json.stringify",
            vec![Value::Map(vec![(Value::Int(1), s("one"))])],
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("non-string key"));
    }

    #[test]
    fn pretty_indents_object() {
        let mut obj = IndexMap::new();
        obj.insert("a".to_string(), Value::Int(1));
        let result = call("json.pretty", vec![Value::Object(obj)]).unwrap();
        if let Value::String(out) = result {
            assert!(out.contains("\n"));
            assert!(out.contains("\"a\": 1"));
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn pretty_empty_collections() {
        let result = call("json.pretty", vec![Value::Array(vec![])]).unwrap();
        assert_eq!(result, s("[]"));
        let result = call("json.pretty", vec![Value::Object(IndexMap::new())]).unwrap();
        assert_eq!(result, s("{}"));
    }

    #[test]
    fn pretty_rejects_non_string_map_keys() {
        let result = call(
            "json.pretty",
            vec![Value::Map(vec![(Value::Bool(true), Value::Int(1))])],
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("non-string key"));
    }

    #[test]
    fn valid_returns_bool() {
        assert_eq!(
            call("json.valid", vec![s("{\"a\": 1}")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call("json.valid", vec![s("not json")]).unwrap(),
            Value::Bool(false)
        );
        // Non-string input -> false (not error)
        assert_eq!(
            call("json.valid", vec![Value::Int(1)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn merge_shallow() {
        let mut a = IndexMap::new();
        a.insert("x".to_string(), Value::Int(1));
        let mut b = IndexMap::new();
        b.insert("y".to_string(), Value::Int(2));
        let result = call("json.merge", vec![Value::Object(a), Value::Object(b)]).unwrap();
        if let Value::Object(m) = result {
            assert_eq!(m.get("x"), Some(&Value::Int(1)));
            assert_eq!(m.get("y"), Some(&Value::Int(2)));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn merge_deep() {
        // Nested objects should merge recursively rather than replace.
        let mut a_inner = IndexMap::new();
        a_inner.insert("p".to_string(), Value::Int(1));
        let mut a = IndexMap::new();
        a.insert("nested".to_string(), Value::Object(a_inner));

        let mut b_inner = IndexMap::new();
        b_inner.insert("q".to_string(), Value::Int(2));
        let mut b = IndexMap::new();
        b.insert("nested".to_string(), Value::Object(b_inner));

        let result = call("json.merge", vec![Value::Object(a), Value::Object(b)]).unwrap();
        if let Value::Object(outer) = result {
            if let Some(Value::Object(nested)) = outer.get("nested") {
                assert_eq!(nested.get("p"), Some(&Value::Int(1)));
                assert_eq!(nested.get("q"), Some(&Value::Int(2)));
            } else {
                panic!("expected nested object");
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn merge_b_overrides_a_for_scalars() {
        let mut a = IndexMap::new();
        a.insert("x".to_string(), Value::Int(1));
        let mut b = IndexMap::new();
        b.insert("x".to_string(), Value::Int(99));
        let result = call("json.merge", vec![Value::Object(a), Value::Object(b)]).unwrap();
        if let Value::Object(m) = result {
            assert_eq!(m.get("x"), Some(&Value::Int(99)));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn merge_requires_two_objects() {
        let result = call(
            "json.merge",
            vec![Value::Object(IndexMap::new()), Value::Int(1)],
        );
        assert!(result.is_err());
    }

    #[test]
    fn round_trip_object() {
        let json_str = "{\"name\": \"Forge\", \"version\": 4}";
        let parsed = call("json.parse", vec![s(json_str)]).unwrap();
        let stringified = call("json.stringify", vec![parsed]).unwrap();
        // Re-parse and verify equality of the parsed forms.
        let reparsed = call("json.parse", vec![stringified]).unwrap();
        let original = call("json.parse", vec![s(json_str)]).unwrap();
        assert_eq!(reparsed, original);
    }

    #[test]
    fn numbers_keep_their_kind() {
        let parse = |t: &str| call("json.parse", vec![s(t)]).unwrap();
        assert_eq!(parse("1.0"), Value::Float(1.0));
        assert_eq!(parse("1e2"), Value::Float(100.0));
        assert_eq!(parse("-0"), Value::Int(0));
        assert_eq!(
            parse("[-0, -0.0, -0e1, -1.5]"),
            Value::Array(vec![
                Value::Int(0),
                Value::Float(-0.0),
                Value::Float(-0.0),
                Value::Float(-1.5),
            ])
        );
        assert_eq!(parse("{\"-0\": -0}"), {
            let mut m = IndexMap::new();
            m.insert("-0".to_string(), Value::Int(0));
            Value::Object(m)
        });
        assert_eq!(parse("9223372036854775807"), Value::Int(i64::MAX));
        assert_eq!(
            parse("9223372036854775808"),
            Value::Float(9223372036854775808.0)
        );
        // `-0` inside an invalid document is left alone: same error.
        assert!(call("json.parse", vec![s("[1 -0]")]).is_err());
        assert!(call("json.parse", vec![s("-00")]).is_err());

        let compact = |v: Value| call("json.compact", vec![v]).unwrap();
        assert_eq!(compact(Value::Float(1.0)), s("1.0"));
        assert_eq!(compact(Value::Float(-0.0)), s("-0.0"));
        assert_eq!(compact(Value::Float(0.1)), s("0.1"));
        assert_eq!(compact(Value::Float(1e21)), s("1e+21"));
        assert_eq!(compact(Value::Float(f64::NAN)), s("null"));
        assert_eq!(compact(Value::Float(f64::INFINITY)), s("null"));
        assert_eq!(compact(Value::Int(1)), s("1"));
        assert_eq!(
            call("json.pretty", vec![Value::Array(vec![Value::Float(2.0)])]).unwrap(),
            s("[\n  2.0\n]")
        );
        // Every Float survives a round trip as a Float.
        for f in [1.0, -0.0, 2.5, 1e21, 1e-7, 123456789.0, f64::MAX] {
            let Value::String(text) = compact(Value::Float(f)) else {
                panic!("compact returns a String")
            };
            assert_eq!(parse(&text), Value::Float(f), "{text}");
        }
    }

    #[test]
    fn short_escapes() {
        assert_eq!(
            call("json.compact", vec![s("\u{8}\u{c}\n\r\t\u{1}")]).unwrap(),
            s("\"\\b\\f\\n\\r\\t\\u0001\"")
        );
        assert_eq!(
            call("json.parse", vec![s("\"\\b\\f\"")]).unwrap(),
            s("\u{8}\u{c}")
        );
    }

    #[test]
    fn pretty_indent_is_checked() {
        assert!(call("json.pretty", vec![Value::Int(1), Value::Int(-1)]).is_err());
        assert!(call("json.pretty", vec![Value::Int(1), s("x")]).is_err());
        assert_eq!(
            call(
                "json.pretty",
                vec![Value::Array(vec![Value::Int(1)]), Value::Int(0)]
            )
            .unwrap(),
            s("[\n1\n]")
        );
    }

    #[test]
    fn unknown_function_errors() {
        let result = call("json.bogus", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown json function"));
    }
}
