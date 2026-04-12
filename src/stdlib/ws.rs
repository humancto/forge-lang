use crate::interpreter::Value;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert(
        "connect".to_string(),
        Value::BuiltIn("ws.connect".to_string()),
    );
    m.insert("send".to_string(), Value::BuiltIn("ws.send".to_string()));
    m.insert(
        "receive".to_string(),
        Value::BuiltIn("ws.receive".to_string()),
    );
    m.insert("close".to_string(), Value::BuiltIn("ws.close".to_string()));
    Value::Object(m)
}

type WsSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tokio_tungstenite::tungstenite::Message,
>;

type WsStream = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

#[allow(dead_code)]
struct WsConnection {
    write: Arc<Mutex<WsSink>>,
    read: Arc<Mutex<WsStream>>,
    url: String,
}

fn ws_pool() -> &'static Mutex<HashMap<String, WsConnection>> {
    static POOL: OnceLock<Mutex<HashMap<String, WsConnection>>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ws_counter() -> &'static std::sync::Mutex<u64> {
    static COUNTER: OnceLock<std::sync::Mutex<u64>> = OnceLock::new();
    COUNTER.get_or_init(|| std::sync::Mutex::new(0))
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "ws.connect" => {
            let url = match args.first() {
                Some(Value::String(s)) => s.clone(),
                _ => return Err("ws.connect() requires a URL string".to_string()),
            };
            ws_connect(&url)
        }
        "ws.send" => {
            let id = match args.first() {
                Some(Value::String(s)) => s.clone(),
                _ => return Err("ws.send() requires a connection ID".to_string()),
            };
            let msg = match args.get(1) {
                Some(Value::String(s)) => s.clone(),
                Some(v @ Value::Object(_)) | Some(v @ Value::Array(_)) => v.to_json_string(),
                _ => return Err("ws.send() requires a message string or object".to_string()),
            };
            ws_send(&id, &msg)
        }
        "ws.receive" => {
            let id = match args.first() {
                Some(Value::String(s)) => s.clone(),
                _ => return Err("ws.receive() requires a connection ID".to_string()),
            };
            let timeout_ms = match args.get(1) {
                Some(Value::Int(t)) => *t as u64,
                _ => 30000,
            };
            ws_receive(&id, timeout_ms)
        }
        "ws.close" => {
            let id = match args.first() {
                Some(Value::String(s)) => s.clone(),
                _ => return Err("ws.close() requires a connection ID".to_string()),
            };
            ws_close(&id)
        }
        _ => Err(format!("unknown ws function: {}", name)),
    }
}

fn ws_connect(url: &str) -> Result<Value, String> {
    let url = url.to_string();

    // Generate ID before entering async block (std::sync::Mutex isn't Send)
    let id = {
        let mut counter = ws_counter().lock().map_err(|e| format!("{}", e))?;
        *counter += 1;
        format!("ws_{}", *counter)
    };
    let id_clone = id.clone();

    run_ws(async move {
        use futures_util::StreamExt;
        use tokio_tungstenite::connect_async;

        let (ws_stream, _) = connect_async(&url)
            .await
            .map_err(|e| format!("WebSocket connect error: {}", e))?;

        let (write, read) = ws_stream.split();

        let id = id_clone;

        let conn = WsConnection {
            write: Arc::new(Mutex::new(write)),
            read: Arc::new(Mutex::new(read)),
            url: url.clone(),
        };

        ws_pool().lock().await.insert(id.clone(), conn);

        let mut result = IndexMap::new();
        result.insert("id".to_string(), Value::String(id));
        result.insert("url".to_string(), Value::String(url));
        result.insert("connected".to_string(), Value::Bool(true));
        Ok(Value::Object(result))
    })
}

fn ws_send(id: &str, msg: &str) -> Result<Value, String> {
    let id = id.to_string();
    let msg = msg.to_string();

    run_ws(async move {
        use futures_util::SinkExt;
        use tokio_tungstenite::tungstenite::Message;

        let pool = ws_pool().lock().await;
        let conn = pool
            .get(&id)
            .ok_or_else(|| format!("WebSocket connection '{}' not found", id))?;
        let write = conn.write.clone();
        drop(pool);

        let mut writer = write.lock().await;
        writer
            .send(Message::Text(msg.into()))
            .await
            .map_err(|e| format!("WebSocket send error: {}", e))?;
        Ok(Value::Bool(true))
    })
}

fn ws_receive(id: &str, timeout_ms: u64) -> Result<Value, String> {
    let id = id.to_string();

    run_ws(async move {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::Message;

        let pool = ws_pool().lock().await;
        let conn = pool
            .get(&id)
            .ok_or_else(|| format!("WebSocket connection '{}' not found", id))?;
        let read = conn.read.clone();
        drop(pool);

        let mut reader = read.lock().await;
        let timeout = tokio::time::Duration::from_millis(timeout_ms);

        match tokio::time::timeout(timeout, reader.next()).await {
            Ok(Some(Ok(msg))) => match msg {
                Message::Text(text) => {
                    let text_str = text.to_string();
                    match serde_json::from_str::<serde_json::Value>(&text_str) {
                        Ok(json) => {
                            let mut result = IndexMap::new();
                            result.insert("type".to_string(), Value::String("text".to_string()));
                            result.insert(
                                "data".to_string(),
                                crate::runtime::server::json_to_forge(json),
                            );
                            result.insert("raw".to_string(), Value::String(text_str));
                            Ok(Value::Object(result))
                        }
                        Err(_) => {
                            let mut result = IndexMap::new();
                            result.insert("type".to_string(), Value::String("text".to_string()));
                            result.insert("data".to_string(), Value::String(text_str.clone()));
                            result.insert("raw".to_string(), Value::String(text_str));
                            Ok(Value::Object(result))
                        }
                    }
                }
                Message::Binary(data) => {
                    let mut result = IndexMap::new();
                    result.insert("type".to_string(), Value::String("binary".to_string()));
                    result.insert("data".to_string(), Value::String(hex::encode(&data)));
                    result.insert("size".to_string(), Value::Int(data.len() as i64));
                    Ok(Value::Object(result))
                }
                Message::Close(_) => {
                    let mut result = IndexMap::new();
                    result.insert("type".to_string(), Value::String("close".to_string()));
                    Ok(Value::Object(result))
                }
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {
                    let mut result = IndexMap::new();
                    result.insert("type".to_string(), Value::String("control".to_string()));
                    Ok(Value::Object(result))
                }
            },
            Ok(Some(Err(e))) => Err(format!("WebSocket receive error: {}", e)),
            Ok(None) => {
                let mut result = IndexMap::new();
                result.insert("type".to_string(), Value::String("close".to_string()));
                Ok(Value::Object(result))
            }
            Err(_) => {
                let mut result = IndexMap::new();
                result.insert("type".to_string(), Value::String("timeout".to_string()));
                Ok(Value::Object(result))
            }
        }
    })
}

fn ws_close(id: &str) -> Result<Value, String> {
    let id = id.to_string();

    run_ws(async move {
        use futures_util::SinkExt;
        use tokio_tungstenite::tungstenite::Message;

        let mut pool = ws_pool().lock().await;
        if let Some(conn) = pool.remove(&id) {
            drop(pool);
            let mut writer = conn.write.lock().await;
            let _ = writer.send(Message::Close(None)).await;
            Ok(Value::Bool(true))
        } else {
            Ok(Value::Bool(false))
        }
    })
}

fn run_ws<F>(future: F) -> Result<Value, String>
where
    F: std::future::Future<Output = Result<Value, String>> + Send + 'static,
{
    let handle = tokio::runtime::Handle::try_current();
    match handle {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(future)),
        Err(_) => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime error: {}", e))?;
            rt.block_on(future)
        }
    }
}
