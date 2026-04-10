use crate::interpreter::Value;
use crate::runtime::server::json_to_forge;
/// Forge HTTP Client — Powered by Reqwest
/// Full HTTP/HTTPS client with JSON, headers, timeouts, and safety guards.
use indexmap::IndexMap;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

/// Default ceiling on HTTP redirect chains. Applies to fetch, download, and crawl
/// unless an explicit override is supplied. Tighter than reqwest's default of 10
/// because every additional hop is another opportunity for SSRF / open-redirect
/// abuse.
pub const DEFAULT_MAX_REDIRECTS: usize = 5;

/// Default ceiling on response body size for `fetch` (bytes). Larger responses
/// abort with an error to prevent memory exhaustion.
pub const DEFAULT_FETCH_MAX_BYTES: u64 = 100 * 1024 * 1024; // 100 MiB

/// Default ceiling on download body size (bytes). Higher than fetch since
/// downloads are an explicit user intent.
pub const DEFAULT_DOWNLOAD_MAX_BYTES: u64 = 1024 * 1024 * 1024; // 1 GiB

/// Default ceiling on HTML body size for crawl (bytes).
pub const DEFAULT_CRAWL_MAX_BYTES: u64 = 16 * 1024 * 1024; // 16 MiB

/// A URL that has passed Forge's scheme/host/private-address checks, plus an
/// optional pinned `(host, socket_addr)` tuple for use with `reqwest`'s
/// `.resolve()` builder. Pinning short-circuits reqwest's own DNS lookup so
/// the connection goes to the exact address the validator checked, closing
/// the TOCTOU window between DNS resolution and TCP connect (i.e. DNS
/// rebinding). Only the *initial* URL of a request can be pinned this way;
/// redirected hops are re-validated via [`validate_url_with`] but still rely
/// on reqwest's connect-time DNS.
#[derive(Debug, Clone)]
pub struct ValidatedUrl {
    pub url: url::Url,
    /// `Some((host, addr))` when the host was a DNS name that resolved to
    /// a safe address we want reqwest to reuse. `None` when the URL already
    /// used an IP literal (nothing to pin).
    pub pinned: Option<(String, SocketAddr)>,
}

/// Full-detail variant that also returns a pinned
/// `SocketAddr` when the host is a DNS name. Use this before constructing a
/// client so the pinning can be installed via [`build_client`].
pub fn validate_url_full(raw: &str) -> Result<ValidatedUrl, String> {
    let deny_private = std::env::var("FORGE_HTTP_DENY_PRIVATE").as_deref() == Ok("1");
    validate_url_full_with(raw, deny_private)
}

/// Test-friendly variant of [`validate_url`] that accepts an explicit
/// `deny_private` flag instead of consulting the environment.
pub fn validate_url_with(raw: &str, deny_private: bool) -> Result<url::Url, String> {
    validate_url_full_with(raw, deny_private).map(|v| v.url)
}

/// Test-friendly full-detail variant of [`validate_url_full`].
pub fn validate_url_full_with(raw: &str, deny_private: bool) -> Result<ValidatedUrl, String> {
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
    let host = parsed
        .host_str()
        .ok_or_else(|| format!("url '{}' has no host", raw))?
        .to_string();
    let port = parsed.port_or_known_default().unwrap_or(match parsed.scheme() {
        "https" => 443,
        _ => 80,
    });

    // If the host is already an IP literal, there's no DNS to pin — just
    // classify it and (optionally) reject.
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if deny_private && ip_is_private(&ip) {
            return Err(format!(
                "url host '{}' is a private/loopback address (FORGE_HTTP_DENY_PRIVATE=1)",
                host
            ));
        }
        return Ok(ValidatedUrl {
            url: parsed,
            pinned: None,
        });
    }

    // DNS hostname. When `deny_private` is on we *must* resolve (and fail
    // closed on errors) so every returned address is inspected. When it's
    // off, we still try to resolve so we can pin the address into reqwest
    // (defeating DNS rebinding on the initial connection), but a resolver
    // failure is non-fatal — we just skip pinning and let reqwest do its
    // own lookup later.
    if deny_private {
        let resolved = resolve_host(&host, port)?;
        for addr in &resolved {
            if ip_is_private(&addr.ip()) {
                return Err(format!(
                    "url host '{}' resolves to a private/loopback address (FORGE_HTTP_DENY_PRIVATE=1)",
                    host
                ));
            }
        }
        let pin = resolved.into_iter().next().map(|addr| (host.clone(), addr));
        return Ok(ValidatedUrl {
            url: parsed,
            pinned: pin,
        });
    }
    let pin = resolve_host(&host, port)
        .ok()
        .and_then(|v| v.into_iter().next())
        .map(|addr| (host.clone(), addr));
    Ok(ValidatedUrl {
        url: parsed,
        pinned: pin,
    })
}

fn resolve_host(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    use std::net::ToSocketAddrs;
    let iter = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("dns resolution failed for '{}': {}", host, e))?;
    Ok(iter.collect())
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
            // IPv4-mapped IPv6 (::ffff:0:0/96) must be classified against the
            // wrapped IPv4 address. Otherwise an attacker could bypass the guard
            // with e.g. http://[::ffff:127.0.0.1]/.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return ip_is_private(&std::net::IpAddr::V4(v4));
            }
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
///
/// If `pinned` is `Some((host, addr))`, reqwest will use the pinned address
/// for `host` instead of doing its own DNS lookup. This defeats DNS rebinding
/// on the *initial* request because we hand reqwest the exact address that
/// passed our private-address check. Redirected hops to a different host go
/// through reqwest's normal resolver.
///
/// The redirect policy is custom rather than `Policy::limited` so that every
/// redirect target is re-checked for scheme and (if `FORGE_HTTP_DENY_PRIVATE=1`)
/// for private-address resolution. A malicious server that 302s to
/// `http://127.0.0.1/` or `http://169.254.169.254/` gets rejected at the
/// client level, not discovered at the TCP layer.
pub fn build_client(
    timeout: Duration,
    max_redirects: usize,
    pinned: Option<(String, SocketAddr)>,
) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(timeout);
    if let Some((host, addr)) = pinned {
        builder = builder.resolve(&host, addr);
    }
    // Capture the env var *now* so the policy's behaviour matches the
    // state at build time rather than shifting mid-request.
    let deny_private = std::env::var("FORGE_HTTP_DENY_PRIVATE").as_deref() == Ok("1");
    let policy = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= max_redirects {
            return attempt.error(format!(
                "too many redirects (cap {})",
                max_redirects
            ));
        }
        // Re-validate the next hop: scheme + host + (optional) private-IP
        // resolution. This closes open-redirect-to-file:// and redirect-to-
        // internal-host classes of attack that `Policy::limited` would let
        // through.
        if let Err(e) = validate_url_with(attempt.url().as_str(), deny_private) {
            return attempt.error(format!("redirect rejected: {}", e));
        }
        attempt.follow()
    });
    builder
        .redirect(policy)
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
    let validated = validate_url_full(url)?;
    let timeout = Duration::from_secs(timeout_secs.unwrap_or(30));
    let redirects = max_redirects.unwrap_or(DEFAULT_MAX_REDIRECTS);
    let cap = max_bytes.unwrap_or(DEFAULT_FETCH_MAX_BYTES);
    let client = build_client(timeout, redirects, validated.pinned.clone())?;

    let url_str = validated.url.as_str();
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

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Process-wide guard for tests that mutate `FORGE_HTTP_DENY_PRIVATE`.
    /// Cargo runs unit tests in parallel by default, so any test that
    /// reads or writes that env var must hold this lock to avoid races.
    /// Recover from poisoning by reusing the inner guard — a poisoned
    /// mutex here just means an earlier test panicked, not that the
    /// shared state is invalid.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

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
        let _guard = env_lock();
        std::env::remove_var("FORGE_HTTP_DENY_PRIVATE");
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
        let _guard = env_lock();
        std::env::remove_var("FORGE_HTTP_DENY_PRIVATE");
        let addr = spawn_redirect_loop_server().await;
        let url = format!("http://{}/", addr);
        let result = fetch(&url, "GET", None, None, None, Some(2), None).await;
        assert!(result.is_err(), "expected redirect-cap error, got {:?}", result);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_aborts_on_oversized_streamed_body() {
        let _guard = env_lock();
        std::env::remove_var("FORGE_HTTP_DENY_PRIVATE");
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
        let _guard = env_lock();
        std::env::remove_var("FORGE_HTTP_DENY_PRIVATE");
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
        let _guard = env_lock();
        std::env::remove_var("FORGE_HTTP_DENY_PRIVATE");
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
        // IPv4-mapped IPv6 must be classified against the wrapped IPv4
        // address. These previously bypassed the guard.
        assert!(
            ip_is_private(&"::ffff:127.0.0.1".parse::<IpAddr>().unwrap()),
            "::ffff:127.0.0.1 should be classified as private (loopback)"
        );
        assert!(
            ip_is_private(&"::ffff:10.0.0.1".parse::<IpAddr>().unwrap()),
            "::ffff:10.0.0.1 should be classified as private (RFC1918)"
        );
        assert!(
            ip_is_private(&"::ffff:169.254.169.254".parse::<IpAddr>().unwrap()),
            "::ffff:169.254.169.254 should be classified as private (link-local)"
        );
        assert!(!ip_is_private(&"8.8.8.8".parse::<IpAddr>().unwrap()));
        assert!(!ip_is_private(&"1.1.1.1".parse::<IpAddr>().unwrap()));
        assert!(!ip_is_private(&"2606:4700:4700::1111".parse::<IpAddr>().unwrap()));
        // Public IPv4 wrapped in IPv6 must NOT trip the guard.
        assert!(!ip_is_private(&"::ffff:8.8.8.8".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn validate_rejects_ipv4_mapped_ipv6_private() {
        // Bracketed IPv4-mapped IPv6 syntax is the documented bypass: a
        // request for http://[::ffff:127.0.0.1]/ used to slip past the
        // guard because the IPv6 arm of `ip_is_private` only inspected the
        // segment pattern, not the wrapped IPv4 octets.
        assert!(validate_url_with("http://[::ffff:127.0.0.1]", true).is_err());
        assert!(validate_url_with("http://[::ffff:10.0.0.1]", true).is_err());
        assert!(validate_url_with("http://[::ffff:169.254.169.254]", true).is_err());
    }

    /// Server that returns a 302 to a target URL (one-shot, not a loop).
    async fn spawn_redirect_to_server(target: String) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(x) => x,
                    Err(_) => return,
                };
                let target = target.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 2048];
                    let _ = sock.read(&mut buf).await;
                    let resp = format!(
                        "HTTP/1.1 302 Found\r\nLocation: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        target
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_does_not_follow_redirect_to_file_scheme() {
        let _guard = env_lock();
        std::env::remove_var("FORGE_HTTP_DENY_PRIVATE");
        // A 302 to file:///etc/passwd must NOT be followed. reqwest itself
        // refuses to dispatch a request to an unsupported scheme, so the
        // observable outcome is that we get the 302 back as the *final*
        // response with an empty body — never the file contents. This test
        // is a guard against any future change that would trick us into
        // dispatching the redirected request.
        let addr = spawn_redirect_to_server("file:///etc/passwd".to_string()).await;
        let url = format!("http://{}/", addr);
        let result = fetch(&url, "GET", None, None, None, None, None).await;
        match result {
            Ok(Value::Object(resp)) => {
                if let Some(Value::Int(status)) = resp.get("status") {
                    assert_eq!(*status, 302, "should report 302, not 200 from file://");
                }
                if let Some(Value::String(body)) = resp.get("body") {
                    assert!(body.is_empty(), "body must be empty, never file contents");
                    assert!(!body.contains("root:"), "must not leak /etc/passwd");
                }
            }
            Ok(other) => panic!("expected response object, got {:?}", other),
            // An error here is also acceptable (and arguably more
            // user-friendly) — what we must NOT see is a 200 with file
            // contents.
            Err(_) => {}
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_rejects_invalid_scheme_redirect_target() {
        // A 302 whose Location is an *unsupported* scheme (e.g. ftp) is
        // caught by the custom redirect policy's validate_url_with
        // callback. reqwest itself would also stop, but going through
        // our policy means the error surfaces as "redirect rejected:
        // unsupported url scheme" rather than a silent stop — a clearer
        // signal when users hit unexpected redirects.
        let _guard = env_lock();
        std::env::remove_var("FORGE_HTTP_DENY_PRIVATE");
        let addr = spawn_redirect_to_server("ftp://example.com/".to_string()).await;
        let url = format!("http://{}/", addr);
        let result = fetch(&url, "GET", None, None, None, None, None).await;
        match result {
            Ok(Value::Object(resp)) => {
                // Either we get the 302 back unchanged (reqwest filtered
                // it) or an error (our policy fired). Both are safe.
                if let Some(Value::Int(status)) = resp.get("status") {
                    assert_eq!(*status, 302, "must not successfully resolve ftp://");
                }
            }
            Ok(other) => panic!("unexpected response value: {:?}", other),
            Err(_) => {}
        }
    }

    #[test]
    fn validate_url_full_pins_dns_host_when_resolvable() {
        // Best-effort: when the resolver succeeds we should produce a pin.
        // If DNS isn't available (offline test runner) the call still
        // succeeds with `pinned = None` — we just can't assert the Some
        // case in that environment, so we skip the body.
        if let Ok(v) = validate_url_full_with("http://localhost", false) {
            // localhost should resolve everywhere; if it does, the pin's
            // host string must match.
            if let Some((host, _addr)) = v.pinned {
                assert_eq!(host, "localhost");
            }
        }
    }

    #[test]
    fn validate_url_full_skips_pin_for_ip_literal() {
        let v = validate_url_full_with("http://127.0.0.1:8080", false).unwrap();
        assert!(
            v.pinned.is_none(),
            "IP-literal URLs have nothing to pin"
        );
    }
}
