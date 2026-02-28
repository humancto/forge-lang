/// Forge HTTP Server — Axum + Tokio
/// Production-grade: async, CORS, JSON, path/query params.
use indexmap::IndexMap;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Json, Path, Query, State},
    http::StatusCode,
    response::Json as JsonResponse,
    routing::{delete, get, post, put},
    Router,
};
use serde_json::Value as JsonValue;
use tower_http::cors::CorsLayer;

use crate::interpreter::{Interpreter, RuntimeError, Value};
use crate::parser::ast::*;

pub type AppState = Arc<Mutex<Interpreter>>;

#[derive(Clone, Debug)]
pub struct Route {
    pub method: String,
    pub pattern: String,
    pub handler_name: String,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
}

pub fn extract_routes(program: &Program) -> Vec<Route> {
    let mut routes = Vec::new();
    for stmt in &program.statements {
        if let Stmt::FnDef {
            name, decorators, ..
        } = stmt
        {
            for dec in decorators {
                let method = match dec.name.as_str() {
                    "get" => "GET",
                    "post" => "POST",
                    "put" => "PUT",
                    "delete" => "DELETE",
                    "ws" => "WS",
                    _ => continue,
                };
                let path = dec
                    .args
                    .iter()
                    .find_map(|arg| match arg {
                        DecoratorArg::Positional(Expr::StringLit(s)) => Some(s.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| format!("/{}", name));

                routes.push(Route {
                    method: method.to_string(),
                    pattern: path,
                    handler_name: name.clone(),
                });
            }
        }
    }
    routes
}

pub fn extract_server_config(program: &Program) -> Option<ServerConfig> {
    for stmt in &program.statements {
        if let Stmt::DecoratorStmt(dec) = stmt {
            if dec.name == "server" {
                let mut config = ServerConfig {
                    port: 8080,
                    host: "127.0.0.1".to_string(),
                };
                for arg in &dec.args {
                    match arg {
                        DecoratorArg::Named(key, Expr::Int(n)) if key == "port" => {
                            config.port = *n as u16
                        }
                        DecoratorArg::Named(key, Expr::StringLit(s)) if key == "host" => {
                            config.host = s.clone()
                        }
                        _ => {}
                    }
                }
                return Some(config);
            }
        }
    }
    None
}

fn to_axum_path(forge_path: &str) -> String {
    forge_path
        .split('/')
        .map(|s| {
            if s.starts_with(':') {
                format!("{{{}}}", &s[1..])
            } else {
                s.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn call_handler(
    interp: &mut Interpreter,
    handler_name: &str,
    path_params: &HashMap<String, String>,
    query_params: &HashMap<String, String>,
    body: Option<JsonValue>,
) -> (StatusCode, JsonValue) {
    let handler = match interp.env.get(handler_name).cloned() {
        Some(v) => v,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": format!("handler '{}' not found", handler_name)}),
            )
        }
    };

    let mut args: Vec<Value> = Vec::new();
    if let Value::Function { ref params, .. } = handler {
        for param in params {
            if let Some(val) = path_params.get(&param.name) {
                args.push(Value::String(val.clone()));
            } else if param.name == "body" || param.name == "data" {
                args.push(
                    body.as_ref()
                        .map(|b| json_to_forge(b.clone()))
                        .unwrap_or(Value::Object(IndexMap::new())),
                );
            } else if param.name == "query" || param.name == "qs" {
                let obj: IndexMap<String, Value> = query_params
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                    .collect();
                args.push(Value::Object(obj));
            } else if let Some(val) = query_params.get(&param.name) {
                args.push(Value::String(val.clone()));
            } else {
                args.push(Value::Null);
            }
        }
    }

    match interp.call_function(handler, args) {
        Ok(value) => (StatusCode::OK, forge_to_json(&value)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({"error": e.message}),
        ),
    }
}

pub async fn start_server(
    interpreter: Interpreter,
    config: &ServerConfig,
    routes: &[Route],
) -> Result<(), RuntimeError> {
    let state: AppState = Arc::new(Mutex::new(interpreter));
    let mut app = Router::new();

    for route in routes {
        let axum_path = to_axum_path(&route.pattern);
        let hn = route.handler_name.clone();

        match route.method.as_str() {
            "GET" => {
                let hn = hn.clone();
                app = app.route(&axum_path, get(move |
                    State(state): State<AppState>,
                    path: Option<Path<HashMap<String, String>>>,
                    Query(query): Query<HashMap<String, String>>,
                | async move {
                    let params = path.map(|Path(p)| p).unwrap_or_default();
                    let mut interp = match state.lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    let (status, json) = call_handler(&mut interp, &hn, &params, &query, None);
                    (status, JsonResponse(json))
                }));
            }
            "POST" | "PUT" => {
                let hn = hn.clone();
                let method = route.method.clone();
                let handler = move |State(state): State<AppState>,
                                    path: Option<Path<HashMap<String, String>>>,
                                    Query(query): Query<HashMap<String, String>>,
                                    Json(body): Json<JsonValue>| async move {
                    let params = path.map(|Path(p)| p).unwrap_or_default();
                    let mut interp = match state.lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    let (status, json) =
                        call_handler(&mut interp, &hn, &params, &query, Some(body));
                    (status, JsonResponse(json))
                };
                if method == "POST" {
                    app = app.route(&axum_path, post(handler));
                } else {
                    app = app.route(&axum_path, put(handler));
                }
            }
            "DELETE" => {
                let hn = hn.clone();
                app = app.route(&axum_path, delete(move |
                    State(state): State<AppState>,
                    path: Option<Path<HashMap<String, String>>>,
                    Query(query): Query<HashMap<String, String>>,
                | async move {
                    let params = path.map(|Path(p)| p).unwrap_or_default();
                    let mut interp = match state.lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    let (status, json) = call_handler(&mut interp, &hn, &params, &query, None);
                    (status, JsonResponse(json))
                }));
            }
            "WS" => {
                let hn = hn.clone();
                app = app.route(
                    &axum_path,
                    get(
                        move |State(state): State<AppState>,
                              ws: axum::extract::WebSocketUpgrade| {
                            let state = state.clone();
                            let hn = hn.clone();
                            async move {
                                ws.on_upgrade(move |mut socket| async move {
                                    use axum::extract::ws::Message;
                                    while let Some(Ok(msg)) = socket.recv().await {
                                        if let Message::Text(text) = msg {
                                            let response = {
                                                let mut interp = match state.lock() {
                                                    Ok(g) => g,
                                                    Err(poisoned) => poisoned.into_inner(),
                                                };
                                                let handler = interp.env.get(&hn).cloned();
                                                if let Some(h) = handler {
                                                    match interp.call_function(
                                                        h,
                                                        vec![Value::String(text.to_string())],
                                                    ) {
                                                        Ok(v) => format!("{}", v),
                                                        Err(e) => format!("error: {}", e.message),
                                                    }
                                                } else {
                                                    "handler not found".to_string()
                                                }
                                            };
                                            let _ =
                                                socket.send(Message::Text(response.into())).await;
                                        }
                                    }
                                })
                            }
                        },
                    ),
                );
            }
            _ => {}
        }
    }

    let app = app.layer(CorsLayer::permissive()).with_state(state);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .map_err(|e| RuntimeError::new(&format!("invalid address: {}", e)))?;

    println!();
    println!("  \x1B[1;32m🔥 Forge server running\x1B[0m");
    println!("  \x1B[1m   http://{}\x1B[0m", addr);
    println!();
    for route in routes {
        println!("  \x1B[36m{:>6}\x1B[0m  {}", route.method, route.pattern);
    }
    println!();
    println!("  \x1B[90mPowered by axum + tokio | Ctrl+C to stop\x1B[0m");
    println!();

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| RuntimeError::new(&format!("bind failed: {}", e)))?;
    axum::serve(listener, app)
        .await
        .map_err(|e| RuntimeError::new(&format!("server error: {}", e)))?;

    Ok(())
}

pub fn json_to_forge(v: JsonValue) -> Value {
    match v {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(b) => Value::Bool(b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        JsonValue::String(s) => Value::String(s),
        JsonValue::Array(a) => Value::Array(a.into_iter().map(json_to_forge).collect()),
        JsonValue::Object(m) => {
            Value::Object(m.into_iter().map(|(k, v)| (k, json_to_forge(v))).collect())
        }
    }
}

pub fn forge_to_json(v: &Value) -> JsonValue {
    match v {
        Value::Null => JsonValue::Null,
        Value::Bool(b) => JsonValue::Bool(*b),
        Value::Int(n) => JsonValue::Number((*n).into()),
        Value::Float(n) => serde_json::Number::from_f64(*n)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        Value::String(s) => JsonValue::String(s.clone()),
        Value::Array(a) => JsonValue::Array(a.iter().map(forge_to_json).collect()),
        Value::ResultOk(v) => {
            let mut obj = serde_json::Map::new();
            obj.insert("Ok".to_string(), forge_to_json(v));
            JsonValue::Object(obj)
        }
        Value::ResultErr(v) => {
            let mut obj = serde_json::Map::new();
            obj.insert("Err".to_string(), forge_to_json(v));
            JsonValue::Object(obj)
        }
        Value::Object(m) => {
            let obj: serde_json::Map<String, JsonValue> = m
                .iter()
                .map(|(k, v)| (k.clone(), forge_to_json(v)))
                .collect();
            JsonValue::Object(obj)
        }
        _ => JsonValue::String(format!("<{}>", v.type_name())),
    }
}
