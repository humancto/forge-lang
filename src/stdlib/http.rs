use crate::interpreter::Value;
use indexmap::IndexMap;
use std::time::Instant;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("get".to_string(), Value::BuiltIn("http.get".to_string()));
    m.insert("post".to_string(), Value::BuiltIn("http.post".to_string()));
    m.insert("put".to_string(), Value::BuiltIn("http.put".to_string()));
    m.insert("delete".to_string(), Value::BuiltIn("http.delete".to_string()));
    m.insert("patch".to_string(), Value::BuiltIn("http.patch".to_string()));
    m.insert("head".to_string(), Value::BuiltIn("http.head".to_string()));
    m.insert("download".to_string(), Value::BuiltIn("http.download".to_string()));
    m.insert("crawl".to_string(), Value::BuiltIn("http.crawl".to_string()));
    m.insert("pretty".to_string(), Value::BuiltIn("http.pretty".to_string()));
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
        "http.pretty" => {
            match args.first() {
                Some(Value::Object(resp)) => {
                    let status = resp.get("status").map(|v| format!("{}", v)).unwrap_or_default();
                    let method = resp.get("method").map(|v| format!("{}", v)).unwrap_or("GET".to_string());
                    let url = resp.get("url").map(|v| format!("{}", v)).unwrap_or_default();
                    let time = resp.get("time").map(|v| format!("{}", v)).unwrap_or("?".to_string());
                    let status_color = if status.starts_with('2') { "32" }
                        else if status.starts_with('3') { "33" }
                        else if status.starts_with('4') { "31" }
                        else { "31" };
                    eprintln!();
                    eprintln!("  \x1B[1m{} {}\x1B[0m", method, url);
                    eprintln!("  \x1B[{}mStatus: {}\x1B[0m  \x1B[90mTime: {}ms\x1B[0m", status_color, status, time);
                    if let Some(body) = resp.get("json") {
                        let pretty = crate::stdlib::json_module::call("json.pretty", vec![body.clone()])
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
            }
        }
        _ => Err(format!("unknown http function: {}", name)),
    }
}

fn do_request(method: &str, args: &[Value]) -> Result<Value, String> {
    let url = match args.first() {
        Some(Value::String(s)) => s.clone(),
        _ => return Err(format!("http.{}() requires a URL string", method.to_lowercase())),
    };

    let opts = args.get(1);

    let mut headers_map = std::collections::HashMap::new();
    let mut body_str = None;
    let mut _timeout_secs = 30u64;

    if let Some(Value::Object(opt_map)) = opts {
        if let Some(Value::Object(hdrs)) = opt_map.get("headers") {
            for (k, v) in hdrs {
                headers_map.insert(k.clone(), format!("{}", v));
            }
        }
        if let Some(Value::String(auth)) = opt_map.get("auth") {
            headers_map.insert("Authorization".to_string(), format!("Bearer {}", auth));
        }
        if let Some(body_val) = opt_map.get("body") {
            body_str = Some(body_val.to_json_string());
            if !headers_map.contains_key("Content-Type") {
                headers_map.insert("Content-Type".to_string(), "application/json".to_string());
            }
        }
        if let Some(Value::Int(t)) = opt_map.get("timeout") {
            _timeout_secs = *t as u64;
        }
    }

    let start = Instant::now();

    let headers_ref = if headers_map.is_empty() { None } else { Some(&headers_map) };

    match crate::runtime::client::fetch_blocking(&url, method, body_str, headers_ref) {
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
        _ => {
            url.rsplit('/').next().unwrap_or("download").to_string()
        }
    };

    eprintln!("  Downloading {}...", url);

    let url_clone = url.clone();
    let dest_clone = dest.clone();

    run_async(async move {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .map_err(|e| format!("client error: {}", e))?;

            let resp = client.get(&url_clone).send().await
                .map_err(|e| format!("download error: {}", e))?;

            let status = resp.status().as_u16();
            if status >= 400 {
                return Err(format!("download failed: HTTP {}", status));
            }

            let _content_length = resp.content_length().unwrap_or(0);
            let bytes = resp.bytes().await
                .map_err(|e| format!("download read error: {}", e))?;

            std::fs::write(&dest_clone, &bytes)
                .map_err(|e| format!("write error: {}", e))?;

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

            let resp = client.get(&url_clone).send().await
                .map_err(|e| format!("crawl error: {}", e))?;

            let status = resp.status().as_u16();
            let html = resp.text().await
                .map_err(|e| format!("crawl read error: {}", e))?;

            let title = extract_between(&html, "<title>", "</title>")
                .unwrap_or_default();

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
            let text_trimmed = text.split_whitespace()
                .collect::<Vec<&str>>()
                .join(" ");
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
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| format!("runtime error: {}", e))?;
        rt.block_on(future)
    });
    handle.join().map_err(|_| "async execution panicked".to_string())?
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

fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let in_script = false;
    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
            continue;
        }
        if ch == '>' {
            in_tag = false;
            continue;
        }
        if !in_tag && !in_script {
            result.push(ch);
        }
    }
    result
}
