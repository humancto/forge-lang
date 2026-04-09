use crate::interpreter::Value;
use crate::runtime::server::json_to_forge;
/// Forge HTTP Client — Powered by Reqwest
/// Full HTTP/HTTPS client with JSON, headers, timeouts, and safety guards.
use indexmap::IndexMap;
use std::collections::HashMap;
use std::time::Duration;

/// Default ceiling on HTTP redirect chains. Applies to fetch, download, and crawl
/// unless an explicit override is supplied.
pub const DEFAULT_MAX_REDIRECTS: usize = 10;

/// Default ceiling on response body size for `fetch` (bytes). Larger responses
/// abort with an error to prevent memory exhaustion.
pub const DEFAULT_FETCH_MAX_BYTES: u64 = 100 * 1024 * 1024; // 100 MiB

/// Default ceiling on download body size (bytes). Higher than fetch since
/// downloads are an explicit user intent.
pub const DEFAULT_DOWNLOAD_MAX_BYTES: u64 = 1024 * 1024 * 1024; // 1 GiB

/// Default ceiling on HTML body size for crawl (bytes).
pub const DEFAULT_CRAWL_MAX_BYTES: u64 = 16 * 1024 * 1024; // 16 MiB

/// Validate a URL string for use as an HTTP request target. Reads the
/// `FORGE_HTTP_DENY_PRIVATE` env var; when set to `1`, also rejects private,
/// loopback, link-local, and multicast addresses (basic SSRF defence).
pub fn validate_url(raw: &str) -> Result<url::Url, String> {
    let deny_private = std::env::var("FORGE_HTTP_DENY_PRIVATE").as_deref() == Ok("1");
    validate_url_with(raw, deny_private)
}

/// Test-friendly variant of [`validate_url`] that accepts an explicit
/// `deny_private` flag instead of consulting the environment.
pub fn validate_url_with(raw: &str, deny_private: bool) -> Result<url::Url, String> {
    let parsed =
        url::Url::parse(raw).map_err(|e| format!("invalid url '{}': {}", raw, e))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(format!(
                "unsupported url scheme '{}': only http/https allowed",
                other
            ))
        }
    }
    if deny_private {
        let host = parsed
            .host_str()
            .ok_or_else(|| format!("url '{}' has no host", raw))?;
        if host_resolves_to_private(host)? {
            return Err(format!(
                "url host '{}' resolves to a private/loopback address (FORGE_HTTP_DENY_PRIVATE=1)",
                host
            ));
        }
    }
    Ok(parsed)
}

fn host_resolves_to_private(host: &str) -> Result<bool, String> {
    use std::net::{IpAddr, ToSocketAddrs};
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(ip_is_private(&ip));
    }
    let addrs = (host, 80u16)
        .to_socket_addrs()
        .map_err(|e| format!("dns resolution failed for '{}': {}", host, e))?;
    for addr in addrs {
        if ip_is_private(&addr.ip()) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn ip_is_private(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.is_unspecified()
                || v4.is_broadcast()
        }
        std::net::IpAddr::V6(v6) => {
            let segs = v6.segments();
            v6.is_loopback()
                || v6.is_multicast()
                || v6.is_unspecified()
                || (segs[0] & 0xfe00) == 0xfc00 // ULA fc00::/7
                || (segs[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
        }
    }
}

/// Build a reqwest client with timeout + redirect cap configured. All Forge
/// HTTP entry points must go through this so safety policy stays in one place.
pub fn build_client(timeout: Duration, max_redirects: usize) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::limited(max_redirects))
        .build()
        .map_err(|e| format!("client error: {}", e))
}

/// Stream a response body up to `max_bytes` then abort. Pre-checks
/// `Content-Length` for fast-fail when the server advertises an oversized body.
pub async fn read_body_capped(
    resp: reqwest::Response,
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    if let Some(len) = resp.content_length() {
        if len > max_bytes {
            return Err(format!(
                "response body advertises {} bytes which exceeds cap of {}",
                len, max_bytes
            ));
        }
    }
    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("body read error: {}", e))?;
        if buf.len() as u64 + chunk.len() as u64 > max_bytes {
            return Err(format!(
                "response body exceeded cap of {} bytes",
                max_bytes
            ));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// Perform an HTTP request — called by the fetch() builtin
#[allow(clippy::too_many_arguments)]
pub async fn fetch(
    url: &str,
    method: &str,
    body: Option<String>,
    headers: Option<&HashMap<String, String>>,
    timeout_secs: Option<u64>,
    max_redirects: Option<usize>,
    max_bytes: Option<u64>,
) -> Result<Value, String> {
    let parsed = validate_url(url)?;
    let timeout = Duration::from_secs(timeout_secs.unwrap_or(30));
    let redirects = max_redirects.unwrap_or(DEFAULT_MAX_REDIRECTS);
    let cap = max_bytes.unwrap_or(DEFAULT_FETCH_MAX_BYTES);
    let client = build_client(timeout, redirects)?;

    let url_str = parsed.as_str();
    let mut req = match method.to_uppercase().as_str() {
        "GET" => client.get(url_str),
        "POST" => client.post(url_str),
        "PUT" => client.put(url_str),
        "DELETE" => client.delete(url_str),
        "PATCH" => client.patch(url_str),
        "HEAD" => client.head(url_str),
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
        // Only set Content-Type if not already provided by headers
        let has_content_type = headers
            .map(|h| h.keys().any(|k| k.eq_ignore_ascii_case("content-type")))
            .unwrap_or(false);
        if !has_content_type {
            req = req.header("Content-Type", "application/json");
        }
        req = req.body(body);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;

    let status = resp.status().as_u16();
    let ok = resp.status().is_success();

    // Collect response headers
    let resp_headers: IndexMap<String, Value> = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                Value::String(v.to_str().unwrap_or("").to_string()),
            )
        })
        .collect();

    let body_bytes = read_body_capped(resp, cap).await?;
    let body_text = String::from_utf8_lossy(&body_bytes).into_owned();

    // Build response object
    let mut response = IndexMap::new();
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
#[allow(clippy::too_many_arguments)]
pub fn fetch_blocking(
    url: &str,
    method: &str,
    body: Option<String>,
    headers: Option<&HashMap<String, String>>,
    timeout_secs: Option<u64>,
    max_redirects: Option<usize>,
    max_bytes: Option<u64>,
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
                handle.block_on(fetch(
                    &url,
                    &method,
                    body,
                    headers.as_ref(),
                    timeout_secs,
                    max_redirects,
                    max_bytes,
                ))
            })
        }
        Err(_) => {
            // No runtime — create one
            let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime error: {}", e))?;
            rt.block_on(fetch(
                url,
                method,
                body,
                headers,
                timeout_secs,
                max_redirects,
                max_bytes,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_non_http_schemes() {
        assert!(validate_url_with("file:///etc/passwd", false).is_err());
        assert!(validate_url_with("ftp://example.com", false).is_err());
        assert!(validate_url_with("javascript:alert(1)", false).is_err());
        assert!(validate_url_with("data:text/plain,abc", false).is_err());
        assert!(validate_url_with("gopher://example.com", false).is_err());
    }

    #[test]
    fn validate_accepts_http_and_https() {
        assert!(validate_url_with("http://example.com", false).is_ok());
        assert!(validate_url_with("https://example.com/path?q=1", false).is_ok());
    }

    #[test]
    fn validate_rejects_garbage() {
        assert!(validate_url_with("not a url", false).is_err());
        assert!(validate_url_with("", false).is_err());
    }

    #[test]
    fn validate_rejects_private_when_enforced() {
        assert!(validate_url_with("http://127.0.0.1", true).is_err());
        assert!(validate_url_with("http://10.0.0.1", true).is_err());
        assert!(validate_url_with("http://169.254.169.254", true).is_err());
        assert!(validate_url_with("http://192.168.1.1", true).is_err());
        assert!(validate_url_with("http://172.16.0.1", true).is_err());
        assert!(validate_url_with("http://[::1]", true).is_err());
        assert!(validate_url_with("http://[fe80::1]", true).is_err());
        assert!(validate_url_with("http://[fc00::1]", true).is_err());
    }

    #[test]
    fn validate_allows_private_when_not_enforced() {
        assert!(validate_url_with("http://127.0.0.1", false).is_ok());
        assert!(validate_url_with("http://10.0.0.1", false).is_ok());
    }

    // === Live HTTP server tests for redirect cap & body cap ===

    use std::net::SocketAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Start a server that always responds with a 302 to itself, forever.
    /// Used to verify the redirect limit is enforced.
    async fn spawn_redirect_loop_server() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(x) => x,
                    Err(_) => return,
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 2048];
                    let _ = sock.read(&mut buf).await;
                    let resp = format!(
                        "HTTP/1.1 302 Found\r\nLocation: http://{}/loop\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        addr
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        addr
    }

    /// Start a server that returns a body of `body_size` bytes (no
    /// Content-Length header so the streaming reader has to enforce the cap).
    async fn spawn_giant_body_server(body_size: usize) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(x) => x,
                    Err(_) => return,
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 2048];
                    let _ = sock.read(&mut buf).await;
                    // Use chunked transfer-encoding so no Content-Length is sent.
                    let _ = sock
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
                        )
                        .await;
                    let chunk = vec![b'a'; 16 * 1024];
                    let mut sent = 0usize;
                    while sent < body_size {
                        let take = std::cmp::min(chunk.len(), body_size - sent);
                        let header = format!("{:x}\r\n", take);
                        if sock.write_all(header.as_bytes()).await.is_err() {
                            break;
                        }
                        if sock.write_all(&chunk[..take]).await.is_err() {
                            break;
                        }
                        if sock.write_all(b"\r\n").await.is_err() {
                            break;
                        }
                        sent += take;
                    }
                    let _ = sock.write_all(b"0\r\n\r\n").await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        addr
    }

    /// Start a server that returns a body advertising a huge Content-Length.
    /// The cap should fast-fail without reading any bytes.
    async fn spawn_advertised_giant_server(advertised: u64) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(x) => x,
                    Err(_) => return,
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 2048];
                    let _ = sock.read(&mut buf).await;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        advertised
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    // Don't actually send the body; the cap should fail before read.
                    let _ = sock.shutdown().await;
                });
            }
        });
        addr
    }

    /// Server that returns a small fixed body. Used as a happy-path baseline.
    async fn spawn_ok_server(body: &'static str) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(x) => x,
                    Err(_) => return,
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 2048];
                    let _ = sock.read(&mut buf).await;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_aborts_on_redirect_loop_default_cap() {
        let addr = spawn_redirect_loop_server().await;
        let url = format!("http://{}/", addr);
        let result = fetch(&url, "GET", None, None, None, None, None).await;
        assert!(result.is_err(), "expected redirect-cap error, got {:?}", result);
        let err = result.unwrap_err().to_lowercase();
        assert!(
            err.contains("redirect") || err.contains("too many"),
            "expected redirect error, got: {}",
            err
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_aborts_on_redirect_loop_custom_cap() {
        let addr = spawn_redirect_loop_server().await;
        let url = format!("http://{}/", addr);
        let result = fetch(&url, "GET", None, None, None, Some(2), None).await;
        assert!(result.is_err(), "expected redirect-cap error, got {:?}", result);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_aborts_on_oversized_streamed_body() {
        // Server streams 4 MiB; cap at 1 MiB.
        let addr = spawn_giant_body_server(4 * 1024 * 1024).await;
        let url = format!("http://{}/", addr);
        let result = fetch(&url, "GET", None, None, None, None, Some(1024 * 1024)).await;
        assert!(result.is_err(), "expected size-cap error, got {:?}", result);
        let err = result.unwrap_err();
        assert!(
            err.contains("cap") || err.contains("exceed"),
            "expected size-cap error, got: {}",
            err
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_aborts_on_advertised_oversized_content_length() {
        // Content-Length says 50 MB but cap is 1 MB. Should fast-fail.
        let addr = spawn_advertised_giant_server(50 * 1024 * 1024).await;
        let url = format!("http://{}/", addr);
        let result = fetch(&url, "GET", None, None, None, None, Some(1024 * 1024)).await;
        assert!(result.is_err(), "expected fast-fail error, got {:?}", result);
        let err = result.unwrap_err();
        assert!(
            err.contains("advertises") || err.contains("exceed") || err.contains("cap"),
            "expected size-cap error, got: {}",
            err
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_succeeds_with_small_body_under_cap() {
        let addr = spawn_ok_server("hello world").await;
        let url = format!("http://{}/", addr);
        let result = fetch(&url, "GET", None, None, None, None, Some(1024 * 1024)).await;
        assert!(result.is_ok(), "expected ok, got {:?}", result);
        if let Ok(crate::interpreter::Value::Object(resp)) = result {
            if let Some(crate::interpreter::Value::String(body)) = resp.get("body") {
                assert_eq!(body, "hello world");
            } else {
                panic!("missing body field");
            }
        } else {
            panic!("expected response object");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_rejects_invalid_scheme_before_network() {
        // Should fail at validate_url, never touch the network.
        let result = fetch(
            "file:///etc/passwd",
            "GET",
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("scheme"));
    }

    #[test]
    fn ip_private_classification() {
        use std::net::IpAddr;
        assert!(ip_is_private(&"127.0.0.1".parse::<IpAddr>().unwrap()));
        assert!(ip_is_private(&"10.5.5.5".parse::<IpAddr>().unwrap()));
        assert!(ip_is_private(&"192.168.0.1".parse::<IpAddr>().unwrap()));
        assert!(ip_is_private(&"169.254.169.254".parse::<IpAddr>().unwrap()));
        assert!(ip_is_private(&"172.20.0.1".parse::<IpAddr>().unwrap()));
        assert!(ip_is_private(&"::1".parse::<IpAddr>().unwrap()));
        assert!(ip_is_private(&"fe80::1".parse::<IpAddr>().unwrap()));
        assert!(ip_is_private(&"fc00::1".parse::<IpAddr>().unwrap()));
        assert!(!ip_is_private(&"8.8.8.8".parse::<IpAddr>().unwrap()));
        assert!(!ip_is_private(&"1.1.1.1".parse::<IpAddr>().unwrap()));
        assert!(!ip_is_private(&"2606:4700:4700::1111".parse::<IpAddr>().unwrap()));
    }
}
