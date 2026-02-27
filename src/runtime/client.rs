use crate::interpreter::Value;
use crate::runtime::server::json_to_forge;
/// Forge HTTP Client — Powered by Reqwest
/// Full HTTP/HTTPS client with JSON, headers, timeouts.
use std::collections::HashMap;

/// Perform an HTTP request — called by the fetch() builtin
pub async fn fetch(
    url: &str,
    method: &str,
    body: Option<String>,
    headers: Option<&HashMap<String, String>>,
) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("client error: {}", e))?;

    let mut req = match method.to_uppercase().as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "PATCH" => client.patch(url),
        "HEAD" => client.head(url),
        _ => return Err(format!("unsupported method: {}", method)),
    };

    // Add custom headers
    if let Some(hdrs) = headers {
        for (key, value) in hdrs {
            req = req.header(key.as_str(), value.as_str());
        }
    }

    // Add body for methods that support it
    if let Some(body) = body {
        req = req.header("Content-Type", "application/json").body(body);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;

    let status = resp.status().as_u16();
    let ok = resp.status().is_success();

    // Collect response headers
    let resp_headers: HashMap<String, Value> = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                Value::String(v.to_str().unwrap_or("").to_string()),
            )
        })
        .collect();

    let body_text = resp
        .text()
        .await
        .map_err(|e| format!("body read error: {}", e))?;

    // Build response object
    let mut response = HashMap::new();
    response.insert("status".to_string(), Value::Int(status as i64));
    response.insert("ok".to_string(), Value::Bool(ok));
    response.insert("url".to_string(), Value::String(url.to_string()));
    response.insert("body".to_string(), Value::String(body_text.clone()));
    response.insert("headers".to_string(), Value::Object(resp_headers));

    // Auto-parse JSON body
    match serde_json::from_str::<serde_json::Value>(&body_text) {
        Ok(json) => response.insert("json".to_string(), json_to_forge(json)),
        Err(_) => response.insert("json".to_string(), Value::Null),
    };

    Ok(Value::Object(response))
}

/// Blocking fetch for use from the interpreter (non-async context)
pub fn fetch_blocking(
    url: &str,
    method: &str,
    body: Option<String>,
    headers: Option<&HashMap<String, String>>,
) -> Result<Value, String> {
    // Use the existing tokio runtime handle
    let handle = tokio::runtime::Handle::try_current();

    match handle {
        Ok(handle) => {
            // We're inside a tokio runtime — use block_in_place
            let url = url.to_string();
            let method = method.to_string();
            let body = body.map(|s| s.to_string());
            let headers = headers.cloned();

            tokio::task::block_in_place(|| {
                handle.block_on(fetch(&url, &method, body, headers.as_ref()))
            })
        }
        Err(_) => {
            // No runtime — create one
            let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime error: {}", e))?;
            rt.block_on(fetch(url, method, body, headers))
        }
    }
}
