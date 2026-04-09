use crate::interpreter::Value;
use indexmap::IndexMap;
use std::time::Instant;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("get".to_string(), Value::BuiltIn("http.get".to_string()));
    m.insert("post".to_string(), Value::BuiltIn("http.post".to_string()));
    m.insert("put".to_string(), Value::BuiltIn("http.put".to_string()));
    m.insert(
        "delete".to_string(),
        Value::BuiltIn("http.delete".to_string()),
    );
    m.insert(
        "patch".to_string(),
        Value::BuiltIn("http.patch".to_string()),
    );
    m.insert("head".to_string(), Value::BuiltIn("http.head".to_string()));
    m.insert(
        "download".to_string(),
        Value::BuiltIn("http.download".to_string()),
    );
    m.insert(
        "crawl".to_string(),
        Value::BuiltIn("http.crawl".to_string()),
    );
    m.insert(
        "pretty".to_string(),
        Value::BuiltIn("http.pretty".to_string()),
    );
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "http.get" => do_request("GET", &args),
        "http.post" => do_request("POST", &args),
        "http.put" => do_request("PUT", &args),
        "http.delete" => do_request("DELETE", &args),
        "http.patch" => do_request("PATCH", &args),
        "http.head" => do_request("HEAD", &args),
        "http.download" => do_download(&args),
        "http.crawl" => do_crawl(&args),
        "http.pretty" => match args.first() {
            Some(Value::Object(resp)) => {
                let status = resp
                    .get("status")
                    .map(|v| format!("{}", v))
                    .unwrap_or_default();
                let method = resp
                    .get("method")
                    .map(|v| format!("{}", v))
                    .unwrap_or("GET".to_string());
                let url = resp
                    .get("url")
                    .map(|v| format!("{}", v))
                    .unwrap_or_default();
                let time = resp
                    .get("time")
                    .map(|v| format!("{}", v))
                    .unwrap_or("?".to_string());
                let status_color = if status.starts_with('2') {
                    "32"
                } else if status.starts_with('3') {
                    "33"
                } else if status.starts_with('4') {
                    "31"
                } else {
                    "31"
                };
                eprintln!();
                eprintln!("  \x1B[1m{} {}\x1B[0m", method, url);
                eprintln!(
                    "  \x1B[{}mStatus: {}\x1B[0m  \x1B[90mTime: {}ms\x1B[0m",
                    status_color, status, time
                );
                if let Some(body) = resp.get("json") {
                    let pretty =
                        crate::stdlib::json_module::call("json.pretty", vec![body.clone()])
                            .unwrap_or_else(|_| Value::String("(no body)".to_string()));
                    if let Value::String(s) = pretty {
                        eprintln!();
                        for line in s.lines() {
                            eprintln!("  {}", line);
                        }
                    }
                }
                eprintln!();
                Ok(Value::Null)
            }
            _ => Err("http.pretty() requires a response object".to_string()),
        },
        _ => Err(format!("unknown http function: {}", name)),
    }
}

fn do_request(method: &str, args: &[Value]) -> Result<Value, String> {
    let url = match args.first() {
        Some(Value::String(s)) => s.clone(),
        _ => {
            return Err(format!(
                "http.{}() requires a URL string",
                method.to_lowercase()
            ))
        }
    };

    let opts = args.get(1);

    let mut headers_map = std::collections::HashMap::new();
    let mut body_str = None;
    let mut timeout_secs: Option<u64> = None;
    let mut final_url = url.clone();

    if let Some(Value::Object(opt_map)) = opts {
        if let Some(Value::Object(hdrs)) = opt_map.get("headers") {
            for (k, v) in hdrs {
                headers_map.insert(k.clone(), format!("{}", v));
            }
        }
        if let Some(Value::String(auth)) = opt_map.get("auth") {
            headers_map.insert("Authorization".to_string(), format!("Bearer {}", auth));
        }
        // Basic auth: { basic_auth: { user: "x", pass: "y" } }
        if let Some(Value::Object(basic)) = opt_map.get("basic_auth") {
            let user = basic
                .get("user")
                .map(|v| format!("{}", v))
                .unwrap_or_default();
            let pass = basic
                .get("pass")
                .map(|v| format!("{}", v))
                .unwrap_or_default();
            use base64::Engine;
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", user, pass));
            headers_map.insert("Authorization".to_string(), format!("Basic {}", encoded));
        }
        // Query params: { params: { key: "val" } } — appended to URL
        if let Some(Value::Object(params)) = opt_map.get("params") {
            let separator = if final_url.contains('?') { "&" } else { "?" };
            let query: Vec<String> = params
                .iter()
                .map(|(k, v)| {
                    let val = match v {
                        Value::String(s) => s.clone(),
                        other => format!("{}", other),
                    };
                    format!("{}={}", percent_encode(k), percent_encode(&val))
                })
                .collect();
            final_url = format!("{}{}{}", final_url, separator, query.join("&"));
        }
        // Form data: { form: { key: "val" } } — url-encoded form body
        if let Some(Value::Object(form)) = opt_map.get("form") {
            let pairs: Vec<String> = form
                .iter()
                .map(|(k, v)| {
                    let val = match v {
                        Value::String(s) => s.clone(),
                        other => format!("{}", other),
                    };
                    format!("{}={}", percent_encode(k), percent_encode(&val))
                })
                .collect();
            body_str = Some(pairs.join("&"));
            if !headers_map.contains_key("Content-Type") {
                headers_map.insert(
                    "Content-Type".to_string(),
                    "application/x-www-form-urlencoded".to_string(),
                );
            }
        }
        // Cookies: { cookies: { key: "val" } } — sent as Cookie header
        if let Some(Value::Object(cookies)) = opt_map.get("cookies") {
            let cookie_str: Vec<String> = cookies
                .iter()
                .map(|(k, v)| {
                    let val = match v {
                        Value::String(s) => s.clone(),
                        other => format!("{}", other),
                    };
                    format!("{}={}", k, val)
                })
                .collect();
            headers_map.insert("Cookie".to_string(), cookie_str.join("; "));
        }
        if let Some(body_val) = opt_map.get("body") {
            body_str = Some(body_val.to_json_string());
            if !headers_map.contains_key("Content-Type") {
                headers_map.insert("Content-Type".to_string(), "application/json".to_string());
            }
        }
        if let Some(Value::Int(t)) = opt_map.get("timeout") {
            timeout_secs = Some(*t as u64);
        }
    }

    let start = Instant::now();

    let headers_ref = if headers_map.is_empty() {
        None
    } else {
        Some(&headers_map)
    };

    match crate::runtime::client::fetch_blocking(
        &final_url,
        method,
        body_str,
        headers_ref,
        timeout_secs,
    ) {
        Ok(resp_val) => {
            let elapsed = start.elapsed().as_millis() as i64;
            if let Value::Object(mut resp) = resp_val {
                resp.insert("time".to_string(), Value::Int(elapsed));
                resp.insert("method".to_string(), Value::String(method.to_string()));
                Ok(Value::Object(resp))
            } else {
                Ok(resp_val)
            }
        }
        Err(e) => Err(format!("http.{} error: {}", method.to_lowercase(), e)),
    }
}

fn do_download(args: &[Value]) -> Result<Value, String> {
    let url = match args.first() {
        Some(Value::String(s)) => s.clone(),
        _ => return Err("http.download() requires a URL string".to_string()),
    };
    let dest = match args.get(1) {
        Some(Value::String(s)) => s.clone(),
        _ => url.rsplit('/').next().unwrap_or("download").to_string(),
    };

    eprintln!("  Downloading {}...", url);

    let url_clone = url.clone();
    let dest_clone = dest.clone();

    run_async(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| format!("client error: {}", e))?;

        let resp = client
            .get(&url_clone)
            .send()
            .await
            .map_err(|e| format!("download error: {}", e))?;

        let status = resp.status().as_u16();
        if status >= 400 {
            return Err(format!("download failed: HTTP {}", status));
        }

        let _content_length = resp.content_length().unwrap_or(0);
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("download read error: {}", e))?;

        std::fs::write(&dest_clone, &bytes).map_err(|e| format!("write error: {}", e))?;

        eprintln!("  Saved to {} ({} bytes)", dest_clone, bytes.len());

        let mut result = IndexMap::new();
        result.insert("path".to_string(), Value::String(dest_clone));
        result.insert("size".to_string(), Value::Int(bytes.len() as i64));
        result.insert("status".to_string(), Value::Int(status as i64));
        Ok(Value::Object(result))
    })
}

fn do_crawl(args: &[Value]) -> Result<Value, String> {
    let url = match args.first() {
        Some(Value::String(s)) => s.clone(),
        _ => return Err("http.crawl() requires a URL string".to_string()),
    };

    let url_clone = url.clone();

    run_async(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("client error: {}", e))?;

        let resp = client
            .get(&url_clone)
            .send()
            .await
            .map_err(|e| format!("crawl error: {}", e))?;

        let status = resp.status().as_u16();
        let html = resp
            .text()
            .await
            .map_err(|e| format!("crawl read error: {}", e))?;

        let title = extract_between(&html, "<title>", "</title>").unwrap_or_default();

        let mut links = Vec::new();
        let mut search_from = 0;
        while let Some(href_start) = html[search_from..].find("href=\"") {
            let abs_start = search_from + href_start + 6;
            if let Some(href_end) = html[abs_start..].find('"') {
                let link = &html[abs_start..abs_start + href_end];
                if link.starts_with("http") {
                    links.push(Value::String(link.to_string()));
                }
                search_from = abs_start + href_end;
            } else {
                break;
            }
        }

        // Extract visible text (strip tags)
        let text = strip_html_tags(&html);
        let text_trimmed = text.split_whitespace().collect::<Vec<&str>>().join(" ");
        let text_preview = if text_trimmed.len() > 500 {
            format!("{}...", &text_trimmed[..500])
        } else {
            text_trimmed
        };

        // Extract meta description
        let description = extract_meta(&html, "description").unwrap_or_default();

        let mut result = IndexMap::new();
        result.insert("url".to_string(), Value::String(url_clone));
        result.insert("status".to_string(), Value::Int(status as i64));
        result.insert("title".to_string(), Value::String(title));
        result.insert("description".to_string(), Value::String(description));
        result.insert("links".to_string(), Value::Array(links));
        result.insert("text".to_string(), Value::String(text_preview));
        result.insert("html_length".to_string(), Value::Int(html.len() as i64));

        Ok(Value::Object(result))
    })
}

fn run_async<F, T>(future: F) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, String>> + Send + 'static,
    T: Send + 'static,
{
    // Always create a fresh runtime on a separate thread to avoid nesting issues
    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime error: {}", e))?;
        rt.block_on(future)
    });
    handle
        .join()
        .map_err(|_| "async execution panicked".to_string())?
}

fn extract_between(html: &str, start_tag: &str, end_tag: &str) -> Option<String> {
    let start = html.find(start_tag)? + start_tag.len();
    let end = html[start..].find(end_tag)?;
    Some(html[start..start + end].trim().to_string())
}

fn extract_meta(html: &str, name: &str) -> Option<String> {
    let pattern = format!("name=\"{}\"", name);
    let pos = html.find(&pattern)?;
    let content_start = html[pos..].find("content=\"")? + pos + 9;
    let content_end = html[content_start..].find('"')?;
    Some(html[content_start..content_start + content_end].to_string())
}

fn percent_encode(s: &str) -> String {
    // Simple percent-encoding for URL query parameters
    let mut result = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut in_script = false;
    let mut tag_name = String::new();
    let mut collecting_tag_name = false;

    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
            tag_name.clear();
            collecting_tag_name = true;
            continue;
        }
        if ch == '>' {
            in_tag = false;
            collecting_tag_name = false;
            let tag_lower = tag_name.to_lowercase();
            if tag_lower == "script" || tag_lower == "style" {
                in_script = true;
            } else if tag_lower == "/script" || tag_lower == "/style" {
                in_script = false;
            }
            continue;
        }
        if in_tag {
            if collecting_tag_name {
                if ch.is_whitespace() || ch == '/' && tag_name.is_empty() {
                    // Allow leading '/' for closing tags like </script>
                    if ch == '/' && tag_name.is_empty() {
                        tag_name.push(ch);
                    } else {
                        collecting_tag_name = false;
                    }
                } else {
                    tag_name.push(ch);
                }
            }
            continue;
        }
        if !in_script {
            result.push(ch);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_basic() {
        assert_eq!(strip_html_tags("<p>Hello</p>"), "Hello");
    }

    #[test]
    fn strip_html_nested_tags() {
        assert_eq!(
            strip_html_tags("<div><span>Hello</span> <b>World</b></div>"),
            "Hello World"
        );
    }

    #[test]
    fn strip_html_script_content_removed() {
        let html = "<p>Before</p><script>var x = 1; alert('xss');</script><p>After</p>";
        let result = strip_html_tags(html);
        assert_eq!(result, "BeforeAfter");
        assert!(
            !result.contains("alert"),
            "script content should be removed"
        );
    }

    #[test]
    fn strip_html_style_content_removed() {
        let html = "<p>Hello</p><style>body { color: red; }</style><p>World</p>";
        let result = strip_html_tags(html);
        assert_eq!(result, "HelloWorld");
        assert!(!result.contains("color"), "style content should be removed");
    }

    #[test]
    fn strip_html_script_with_attributes() {
        let html = r#"<p>Text</p><script type="text/javascript">evil();</script><p>More</p>"#;
        let result = strip_html_tags(html);
        assert_eq!(result, "TextMore");
    }

    #[test]
    fn strip_html_case_insensitive_script() {
        let html = "<p>A</p><SCRIPT>bad();</SCRIPT><p>B</p>";
        let result = strip_html_tags(html);
        assert_eq!(result, "AB");
    }

    #[test]
    fn strip_html_multiple_scripts() {
        let html = "<script>a();</script>Hello<script>b();</script>World<style>.x{}</style>!";
        let result = strip_html_tags(html);
        assert_eq!(result, "HelloWorld!");
    }

    #[test]
    fn strip_html_no_tags() {
        assert_eq!(strip_html_tags("Just plain text"), "Just plain text");
    }

    #[test]
    fn strip_html_empty() {
        assert_eq!(strip_html_tags(""), "");
    }

    #[test]
    fn strip_html_self_closing_tags() {
        assert_eq!(
            strip_html_tags("Hello<br/>World<img src='x'/>!"),
            "HelloWorld!"
        );
    }
}
