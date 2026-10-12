//! Handler-controlled HTTP responses and request bodies for the server
//! (`runtime::server`). Engine-neutral: both engines hand the server a
//! handler's return value as JSON, and everything here works on that.
//!
//! # Responses
//!
//! A handler that returns an ordinary value answers `200` with the value
//! encoded as JSON (unchanged since the first server). To choose the status
//! code, headers or a non-JSON body it returns a *response value*, built by
//! `http.response(status, body?, headers?)` (`stdlib::http`):
//!
//! ```text
//! { __type__: "HttpResponse", status: 201, headers: { Location: "/items/1" }, body: { id: 1 } }
//! ```
//!
//! The body is encoded by its type: `null` sends no body, a String is sent
//! as is (`text/plain; charset=utf-8` unless the handler set a
//! `Content-Type`), anything else is JSON (`application/json` unless set).
//! [`ResponseSpec::from_json`] validates a response value — the builtin
//! runs it when the value is built, so mistakes raise at the call site, and
//! the server runs it again on what the handler returned (a handler may
//! build or modify the object by hand).
//!
//! # Request bodies
//!
//! [`read_body`] turns a request body into the handler's `body` value by
//! its `Content-Type` (JSON, form, text), keeping its exact text for
//! `raw_body`; a malformed body is answered before the handler runs, with
//! a JSON error.

use axum::http::{HeaderName, HeaderValue, StatusCode};
use indexmap::IndexMap;
use serde_json::Value as JsonValue;

use crate::interpreter::Value;

/// `__type__` of a response value.
pub const RESPONSE_TYPE: &str = "HttpResponse";

/// Largest request body the server reads (bytes). Larger bodies are
/// answered `413` before the handler runs.
pub const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

/// Headers the server computes itself; a handler may not set them.
const FRAMING_HEADERS: &[&str] = &["content-length", "transfer-encoding", "connection"];

/// The body of a validated response.
#[derive(Debug, Clone, PartialEq)]
pub enum ResponseBody {
    /// No body (`null`).
    Empty,
    /// A String body, sent as is.
    Text(String),
    /// Any other value, sent as JSON.
    Json(JsonValue),
}

/// A validated handler response.
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseSpec {
    pub status: StatusCode,
    /// In the order the handler listed them; a header given an array of
    /// values appears once per value.
    pub headers: Vec<(HeaderName, HeaderValue)>,
    pub body: ResponseBody,
}

impl ResponseSpec {
    /// The response a handler's return value asks for: `None` when the
    /// value is not a response value (it is sent as a `200` JSON body),
    /// `Some(Err(message))` when it is one but is invalid.
    pub fn from_json(value: &JsonValue) -> Option<Result<Self, String>> {
        let JsonValue::Object(map) = value else {
            return None;
        };
        if map.get("__type__").and_then(JsonValue::as_str) != Some(RESPONSE_TYPE) {
            return None;
        }
        Some(Self::from_fields(
            map.get("status").unwrap_or(&JsonValue::Null),
            map.get("body").unwrap_or(&JsonValue::Null),
            map.get("headers").unwrap_or(&JsonValue::Null),
        ))
    }

    fn from_fields(
        status: &JsonValue,
        body: &JsonValue,
        headers: &JsonValue,
    ) -> Result<Self, String> {
        let status = status_code(status)?;
        let headers = header_list(headers)?;
        let body = match body {
            JsonValue::Null => ResponseBody::Empty,
            JsonValue::String(s) => ResponseBody::Text(s.clone()),
            other => ResponseBody::Json(other.clone()),
        };
        if body != ResponseBody::Empty && !status_allows_body(status) {
            return Err(format!(
                "a {} response cannot have a body (pass null as the body)",
                status.as_u16()
            ));
        }
        Ok(Self {
            status,
            headers,
            body,
        })
    }

    /// Whether the handler set `Content-Type` itself.
    fn has_content_type(&self) -> bool {
        self.headers
            .iter()
            .any(|(name, _)| name == axum::http::header::CONTENT_TYPE)
    }

    /// The axum response: status, the handler's headers, then the body
    /// with its default content type when the handler did not set one.
    pub fn into_response(self) -> axum::response::Response {
        use axum::response::IntoResponse;
        let default_type = match &self.body {
            ResponseBody::Empty => None,
            ResponseBody::Text(_) => Some("text/plain; charset=utf-8"),
            ResponseBody::Json(_) => Some("application/json"),
        };
        let set_type = !self.has_content_type();
        let bytes: Vec<u8> = match self.body {
            ResponseBody::Empty => Vec::new(),
            ResponseBody::Text(s) => s.into_bytes(),
            // Serializing a `serde_json::Value` cannot fail.
            ResponseBody::Json(v) => serde_json::to_vec(&v).unwrap_or_default(),
        };
        let mut response = (self.status, bytes).into_response();
        let headers = response.headers_mut();
        // `(StatusCode, Vec<u8>)` labels the body application/octet-stream.
        headers.remove(axum::http::header::CONTENT_TYPE);
        if let (true, Some(content_type)) = (set_type, default_type) {
            headers.insert(
                axum::http::header::CONTENT_TYPE,
                HeaderValue::from_static(content_type),
            );
        }
        for (name, value) in self.headers {
            headers.append(name, value);
        }
        response
    }
}

/// 1xx, 204 and 304 responses never carry a body (RFC 9110).
fn status_allows_body(status: StatusCode) -> bool {
    !(status.is_informational()
        || status == StatusCode::NO_CONTENT
        || status == StatusCode::NOT_MODIFIED)
}

fn status_code(value: &JsonValue) -> Result<StatusCode, String> {
    let code = value
        .as_i64()
        .filter(|n| (200..=599).contains(n))
        .ok_or_else(|| {
            format!(
                "status must be an Int from 200 to 599, got {}",
                describe(value)
            )
        })?;
    StatusCode::from_u16(code as u16).map_err(|e| format!("invalid status {}: {}", code, e))
}

fn header_list(value: &JsonValue) -> Result<Vec<(HeaderName, HeaderValue)>, String> {
    let map = match value {
        JsonValue::Null => return Ok(Vec::new()),
        JsonValue::Object(map) => map,
        other => {
            return Err(format!(
                "headers must be an object of name: value pairs, got {}",
                describe(other)
            ))
        }
    };
    let mut out = Vec::new();
    for (name, value) in map {
        let header = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| format!("invalid header name {:?}", name))?;
        if FRAMING_HEADERS.contains(&header.as_str()) {
            return Err(format!(
                "header '{}' is set by the server and cannot be set by a handler",
                name
            ));
        }
        let values: Vec<&JsonValue> = match value {
            JsonValue::Array(items) => items.iter().collect(),
            single => vec![single],
        };
        for v in values {
            let text = match v {
                JsonValue::String(s) => s.clone(),
                JsonValue::Number(n) => n.to_string(),
                JsonValue::Bool(b) => b.to_string(),
                other => {
                    return Err(format!(
                    "header '{}' must be a String, a number, a Bool or an array of them, got {}",
                    name,
                    describe(other)
                ))
                }
            };
            // `HeaderValue` rejects CR, LF and other control characters,
            // so a value can never inject another header line.
            let header_value = HeaderValue::from_str(&text).map_err(|_| {
                format!(
                    "invalid value for header '{}': header values cannot contain line breaks or control characters",
                    name
                )
            })?;
            out.push((header.clone(), header_value));
        }
    }
    Ok(out)
}

/// A Forge type name for a JSON value, for messages.
fn describe(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "Bool",
        JsonValue::Number(n) if n.is_f64() => "Float",
        JsonValue::Number(_) => "Int",
        JsonValue::String(_) => "String",
        JsonValue::Array(_) => "Array",
        JsonValue::Object(_) => "Object",
    }
}

/// `http.response(status, body?, headers?)`: the response value, validated.
pub fn make_response(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() || args.len() > 3 {
        return Err(
            "http.response() requires (status, body?, headers?), e.g. http.response(201, { id: 1 })"
                .to_string(),
        );
    }
    let status = args[0].clone();
    let body = args.get(1).cloned().unwrap_or(Value::Null);
    let headers = match args.get(2) {
        None | Some(Value::Null) => Value::Object(IndexMap::new()),
        Some(h) => h.clone(),
    };
    let to_json = crate::runtime::server::forge_to_json;
    ResponseSpec::from_fields(&to_json(&status), &to_json(&body), &to_json(&headers))
        .map_err(|e| format!("http.response(): {}", e))?;
    let mut fields = IndexMap::new();
    fields.insert(
        "__type__".to_string(),
        Value::String(RESPONSE_TYPE.to_string()),
    );
    fields.insert("status".to_string(), status);
    fields.insert("headers".to_string(), headers);
    fields.insert("body".to_string(), body);
    Ok(Value::Object(fields))
}

/// Why a request body was refused before the handler ran.
#[derive(Debug, Clone, PartialEq)]
pub struct BodyError {
    pub status: StatusCode,
    pub message: String,
}

/// A request body as the handler sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct RequestBody {
    /// `body` / `req.body`, decoded by content type; `None` when the
    /// request has no body.
    pub decoded: Option<Value>,
    /// `raw_body` / `req.raw_body`: the exact bytes as text (`""` when the
    /// request has no body), e.g. for checking a webhook signature.
    pub raw: String,
}

/// The handler's view of a request body of `content_type`: `raw` is the
/// exact text, and `decoded` the handler's `body`:
///
/// - empty → `None` (the `body` parameter then gets `{}`, `req.body` null);
/// - not UTF-8 → `415`, whatever the content type (so `raw_body` is always
///   the exact text);
/// - `application/json` or `*/*+json` → the parsed JSON (`400` if invalid);
/// - `application/x-www-form-urlencoded` → an object of Strings;
/// - anything else, or no content type → the body as a String.
pub fn read_body(content_type: Option<&str>, bytes: &[u8]) -> Result<RequestBody, BodyError> {
    if bytes.is_empty() {
        return Ok(RequestBody {
            decoded: None,
            raw: String::new(),
        });
    }
    let essence = content_type
        .and_then(|ct| ct.split(';').next())
        .map(|ct| ct.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Err(BodyError {
            status: StatusCode::UNSUPPORTED_MEDIA_TYPE,
            message: format!(
                "request body is not UTF-8 text (content type '{}'); send JSON, a form or text",
                if essence.is_empty() {
                    "none"
                } else {
                    essence.as_str()
                }
            ),
        });
    };
    let decoded = if essence == "application/json" || essence.ends_with("+json") {
        serde_json::from_str::<crate::stdlib::json_module::Ordered>(text)
            .map(|v| v.0)
            .map_err(|e| BodyError {
                status: StatusCode::BAD_REQUEST,
                message: format!("invalid JSON body: {}", e),
            })?
    } else if essence == "application/x-www-form-urlencoded" {
        let mut fields = IndexMap::new();
        for (k, v) in url::form_urlencoded::parse(bytes) {
            fields.insert(k.into_owned(), Value::String(v.into_owned()));
        }
        Value::Object(fields)
    } else {
        Value::String(text.to_string())
    };
    Ok(RequestBody {
        decoded: Some(decoded),
        raw: text.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse_body(content_type: Option<&str>, bytes: &[u8]) -> Result<Option<Value>, BodyError> {
        read_body(content_type, bytes).map(|b| b.decoded)
    }

    fn spec(v: JsonValue) -> Result<ResponseSpec, String> {
        ResponseSpec::from_json(&v).expect("a response value")
    }

    fn response(status: JsonValue, body: JsonValue, headers: JsonValue) -> JsonValue {
        json!({ "__type__": RESPONSE_TYPE, "status": status, "body": body, "headers": headers })
    }

    #[test]
    fn ordinary_values_are_not_responses() {
        assert!(ResponseSpec::from_json(&json!({ "status": 201 })).is_none());
        assert!(ResponseSpec::from_json(&json!([1])).is_none());
        assert!(ResponseSpec::from_json(&json!({ "__type__": "Other" })).is_none());
    }

    #[test]
    fn body_kinds() {
        let s = spec(response(json!(201), json!({ "id": 1 }), json!(null))).unwrap();
        assert_eq!(s.status, StatusCode::CREATED);
        assert_eq!(s.body, ResponseBody::Json(json!({ "id": 1 })));
        let s = spec(response(json!(200), json!("hi"), json!({}))).unwrap();
        assert_eq!(s.body, ResponseBody::Text("hi".into()));
        let s = spec(response(json!(204), json!(null), json!({}))).unwrap();
        assert_eq!(s.body, ResponseBody::Empty);
    }

    #[test]
    fn status_is_validated() {
        for bad in [
            json!(99),
            json!(600),
            json!(100),
            json!("200"),
            json!(200.5),
            json!(null),
        ] {
            let err = spec(response(bad.clone(), json!(null), json!({}))).unwrap_err();
            assert!(
                err.contains("status must be an Int from 200 to 599"),
                "{bad}: {err}"
            );
        }
        let err = spec(response(json!(204), json!("x"), json!({}))).unwrap_err();
        assert!(err.contains("204 response cannot have a body"), "{err}");
        let err = spec(response(json!(304), json!({}), json!({}))).unwrap_err();
        assert!(err.contains("304"), "{err}");
    }

    #[test]
    fn headers_are_validated() {
        let s = spec(response(
            json!(200),
            json!(null),
            json!({ "X-Count": 3, "Set-Cookie": ["a=1", "b=2"], "X-Ok": true }),
        ))
        .unwrap();
        let rendered: Vec<(String, String)> = s
            .headers
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_str().unwrap().to_string()))
            .collect();
        assert!(rendered.contains(&("x-count".into(), "3".into())));
        assert!(rendered.contains(&("set-cookie".into(), "a=1".into())));
        assert!(rendered.contains(&("set-cookie".into(), "b=2".into())));
        assert!(rendered.contains(&("x-ok".into(), "true".into())));

        let err = spec(response(
            json!(200),
            json!(null),
            json!({ "X-A": "a\r\nSet-Cookie: x=1" }),
        ))
        .unwrap_err();
        assert!(err.contains("line breaks"), "{err}");
        let err = spec(response(json!(200), json!(null), json!({ "X-A": "a\nb" }))).unwrap_err();
        assert!(err.contains("line breaks"), "{err}");
        let err = spec(response(
            json!(200),
            json!(null),
            json!({ "Bad Name": "x" }),
        ))
        .unwrap_err();
        assert!(err.contains("invalid header name"), "{err}");
        let err = spec(response(json!(200), json!(null), json!({ "X\r\nY": "x" }))).unwrap_err();
        assert!(err.contains("invalid header name"), "{err}");
        let err = spec(response(
            json!(200),
            json!(null),
            json!({ "Content-Length": "3" }),
        ))
        .unwrap_err();
        assert!(err.contains("set by the server"), "{err}");
        let err = spec(response(
            json!(200),
            json!(null),
            json!({ "X": { "a": 1 } }),
        ))
        .unwrap_err();
        assert!(err.contains("must be a String"), "{err}");
        let err = spec(response(json!(200), json!(null), json!(["x"]))).unwrap_err();
        assert!(err.contains("headers must be an object"), "{err}");
    }

    #[test]
    fn make_response_validates_at_the_call() {
        let ok = make_response(&[Value::Int(201), Value::String("made".into())]).unwrap();
        let Value::Object(fields) = ok else {
            panic!("object")
        };
        assert_eq!(fields.get("status"), Some(&Value::Int(201)));
        assert!(make_response(&[]).is_err());
        let err = make_response(&[Value::Int(700)]).unwrap_err();
        assert!(err.starts_with("http.response(): status"), "{err}");
    }

    #[test]
    fn bodies_by_content_type() {
        assert_eq!(parse_body(Some("application/json"), b""), Ok(None));
        assert_eq!(
            parse_body(Some("application/json; charset=utf-8"), br#"{"a":1}"#),
            Ok(Some(crate::stdlib::json_module::json_to_forge(
                json!({ "a": 1 })
            )))
        );
        let err = parse_body(Some("application/json"), b"nope").unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(
            err.message.starts_with("invalid JSON body: "),
            "{}",
            err.message
        );
        assert!(parse_body(Some("application/vnd.api+json"), b"[1]").is_ok());

        let form = parse_body(Some("application/x-www-form-urlencoded"), b"a=1&b=x+y&a=2")
            .unwrap()
            .unwrap();
        let Value::Object(form) = form else {
            panic!("form object")
        };
        assert_eq!(form.get("a"), Some(&Value::String("2".into())));
        assert_eq!(form.get("b"), Some(&Value::String("x y".into())));

        assert_eq!(
            parse_body(None, b"hello"),
            Ok(Some(Value::String("hello".into())))
        );
        assert_eq!(
            parse_body(Some("text/csv"), b"a,b"),
            Ok(Some(Value::String("a,b".into())))
        );
        let err = parse_body(Some("application/octet-stream"), &[0xff, 0xfe]).unwrap_err();
        assert_eq!(err.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        // Whatever the content type, so `raw_body` is always exact text.
        for ct in [
            Some("application/x-www-form-urlencoded"),
            Some("application/json"),
            None,
        ] {
            let err = parse_body(ct, b"a=\xff").unwrap_err();
            assert_eq!(err.status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{ct:?}");
        }
    }

    #[test]
    fn raw_body_is_the_exact_text() {
        let body = read_body(Some("application/json"), b"{ \"a\" :1 }\n").unwrap();
        assert_eq!(body.raw, "{ \"a\" :1 }\n");
        assert_eq!(
            body.decoded,
            Some(crate::stdlib::json_module::json_to_forge(json!({ "a": 1 })))
        );
        let form = read_body(Some("application/x-www-form-urlencoded"), b"a=x+y%21").unwrap();
        assert_eq!(form.raw, "a=x+y%21");
        let empty = read_body(Some("application/json"), b"").unwrap();
        assert_eq!((empty.decoded, empty.raw.as_str()), (None, ""));
    }
}
