use crate::interpreter::Value;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::sync::OnceLock;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert(
        "connect".to_string(),
        Value::BuiltIn("mysql.connect".to_string()),
    );
    m.insert(
        "query".to_string(),
        Value::BuiltIn("mysql.query".to_string()),
    );
    m.insert(
        "execute".to_string(),
        Value::BuiltIn("mysql.execute".to_string()),
    );
    m.insert(
        "close".to_string(),
        Value::BuiltIn("mysql.close".to_string()),
    );
    // NOTE: mysql.begin/commit/rollback are intentionally absent. mysql_async's
    // pool returns a fresh connection on every get_conn(), so issuing BEGIN
    // on one call and COMMIT on a later call would target different physical
    // connections — silently broken transactions. For multi-statement work,
    // use a dedicated transaction API (future) or run all statements inside a
    // single SQL string via the underlying server's batch execution.
    Value::Object(m)
}

fn mysql_pool() -> &'static tokio::sync::Mutex<HashMap<String, mysql_async::Pool>> {
    static POOL: OnceLock<tokio::sync::Mutex<HashMap<String, mysql_async::Pool>>> = OnceLock::new();
    POOL.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

fn mysql_counter() -> &'static std::sync::Mutex<u64> {
    static COUNTER: OnceLock<std::sync::Mutex<u64>> = OnceLock::new();
    COUNTER.get_or_init(|| std::sync::Mutex::new(0))
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "mysql.connect" => mysql_connect(args),
        "mysql.query" => mysql_query(args),
        "mysql.execute" => mysql_execute(args),
        "mysql.close" => mysql_close(args),
        _ => Err(format!("unknown mysql function: {}", name)),
    }
}

fn build_connection_url(args: &[Value]) -> Result<String, String> {
    match args.len() {
        // Single arg: connection string
        1 => match &args[0] {
            Value::String(url) => Ok(url.clone()),
            _ => Err(
                "mysql.connect() requires a connection string or (host, user, pass, db)"
                    .to_string(),
            ),
        },
        // Four args: host, user, pass, db
        4 => {
            let host = match &args[0] {
                Value::String(s) => s.as_str(),
                _ => return Err("mysql.connect() host must be a string".to_string()),
            };
            let user = match &args[1] {
                Value::String(s) => s.as_str(),
                _ => return Err("mysql.connect() user must be a string".to_string()),
            };
            let pass = match &args[2] {
                Value::String(s) => s.as_str(),
                _ => return Err("mysql.connect() password must be a string".to_string()),
            };
            let db = match &args[3] {
                Value::String(s) => s.as_str(),
                _ => return Err("mysql.connect() database must be a string".to_string()),
            };
            Ok(format!("mysql://{}:{}@{}/{}", user, pass, host, db))
        }
        // Five args: host, user, pass, db, port
        5 => {
            let host = match &args[0] {
                Value::String(s) => s.as_str(),
                _ => return Err("mysql.connect() host must be a string".to_string()),
            };
            let user = match &args[1] {
                Value::String(s) => s.as_str(),
                _ => return Err("mysql.connect() user must be a string".to_string()),
            };
            let pass = match &args[2] {
                Value::String(s) => s.as_str(),
                _ => return Err("mysql.connect() password must be a string".to_string()),
            };
            let db = match &args[3] {
                Value::String(s) => s.as_str(),
                _ => return Err("mysql.connect() database must be a string".to_string()),
            };
            let port = match &args[4] {
                Value::Int(p) => *p,
                _ => return Err("mysql.connect() port must be an integer".to_string()),
            };
            Ok(format!(
                "mysql://{}:{}@{}:{}/{}",
                user, pass, host, port, db
            ))
        }
        _ => {
            Err("mysql.connect() requires 1 arg (url) or 4 args (host, user, pass, db)".to_string())
        }
    }
}

fn forge_to_mysql_param(val: &Value) -> mysql_async::Value {
    match val {
        Value::Int(n) => mysql_async::Value::Int(*n),
        Value::Float(f) => mysql_async::Value::Double(*f),
        Value::String(s) => mysql_async::Value::Bytes(s.as_bytes().to_vec()),
        Value::Bool(b) => mysql_async::Value::Int(if *b { 1 } else { 0 }),
        Value::Null | Value::None => mysql_async::Value::NULL,
        _ => mysql_async::Value::Bytes(format!("{}", val).into_bytes()),
    }
}

fn mysql_val_to_forge(val: mysql_async::Value) -> Value {
    match val {
        mysql_async::Value::NULL => Value::Null,
        mysql_async::Value::Int(n) => Value::Int(n),
        mysql_async::Value::UInt(n) => Value::Int(n as i64),
        mysql_async::Value::Float(f) => Value::Float(f as f64),
        mysql_async::Value::Double(f) => Value::Float(f),
        mysql_async::Value::Bytes(b) => Value::String(String::from_utf8_lossy(&b).to_string()),
        mysql_async::Value::Date(y, m, d, h, min, s, _us) => Value::String(format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            y, m, d, h, min, s
        )),
        mysql_async::Value::Time(neg, d, h, m, s, _us) => {
            let sign = if neg { "-" } else { "" };
            Value::String(format!("{}{}:{:02}:{:02}", sign, d * 24 + h as u32, m, s))
        }
    }
}

fn mysql_connect(args: Vec<Value>) -> Result<Value, String> {
    let url = build_connection_url(&args)?;

    let id = {
        let mut counter = mysql_counter().lock().map_err(|e| format!("{}", e))?;
        *counter += 1;
        format!("mysql_{}", *counter)
    };
    let id_clone = id.clone();

    run_mysql(async move {
        let opts = mysql_async::Opts::from_url(&url)
            .map_err(|e| format!("mysql.connect() invalid URL: {}", e))?;
        let pool = mysql_async::Pool::new(opts);

        // Test the connection by getting a conn and dropping it
        let conn = pool
            .get_conn()
            .await
            .map_err(|e| format!("mysql.connect() error: {}", e))?;
        drop(conn);

        mysql_pool().lock().await.insert(id_clone.clone(), pool);
        Ok(Value::String(id_clone))
    })
}

fn mysql_query(args: Vec<Value>) -> Result<Value, String> {
    let conn_id = match args.first() {
        Some(Value::String(s)) => s.clone(),
        _ => return Err("mysql.query() requires a connection ID as first argument".to_string()),
    };
    let sql = match args.get(1) {
        Some(Value::String(s)) => s.clone(),
        _ => return Err("mysql.query() requires a SQL string as second argument".to_string()),
    };
    let params: Vec<mysql_async::Value> = match args.get(2) {
        Some(Value::Array(arr)) => arr.iter().map(forge_to_mysql_param).collect(),
        None => Vec::new(),
        _ => return Err("mysql.query() third argument must be an array of parameters".to_string()),
    };

    run_mysql(async move {
        let pool_guard = mysql_pool().lock().await;
        let pool = pool_guard
            .get(&conn_id)
            .ok_or_else(|| format!("MySQL connection '{}' not found", conn_id))?;
        let pool_clone = pool.clone();
        drop(pool_guard);

        let mut conn = pool_clone
            .get_conn()
            .await
            .map_err(|e| format!("mysql.query() connection error: {}", e))?;

        use mysql_async::prelude::*;

        let rows: Vec<mysql_async::Row> = if params.is_empty() {
            conn.query(&sql)
                .await
                .map_err(|e| format!("mysql.query() error: {}", e))?
        } else {
            conn.exec(&sql, mysql_async::Params::Positional(params))
                .await
                .map_err(|e| format!("mysql.query() error: {}", e))?
        };

        let mut results = Vec::new();
        for row in rows {
            let mut map = IndexMap::new();
            let columns: Vec<String> = row
                .columns_ref()
                .iter()
                .map(|c| c.name_str().to_string())
                .collect();
            for (i, col_name) in columns.iter().enumerate() {
                let val: mysql_async::Value = row.get(i).unwrap_or(mysql_async::Value::NULL);
                map.insert(col_name.clone(), mysql_val_to_forge(val));
            }
            results.push(Value::Object(map));
        }
        Ok(Value::Array(results))
    })
}

fn mysql_execute(args: Vec<Value>) -> Result<Value, String> {
    let conn_id = match args.first() {
        Some(Value::String(s)) => s.clone(),
        _ => return Err("mysql.execute() requires a connection ID as first argument".to_string()),
    };
    let sql = match args.get(1) {
        Some(Value::String(s)) => s.clone(),
        _ => return Err("mysql.execute() requires a SQL string as second argument".to_string()),
    };
    let params: Vec<mysql_async::Value> = match args.get(2) {
        Some(Value::Array(arr)) => arr.iter().map(forge_to_mysql_param).collect(),
        None => Vec::new(),
        _ => {
            return Err("mysql.execute() third argument must be an array of parameters".to_string())
        }
    };

    run_mysql(async move {
        let pool_guard = mysql_pool().lock().await;
        let pool = pool_guard
            .get(&conn_id)
            .ok_or_else(|| format!("MySQL connection '{}' not found", conn_id))?;
        let pool_clone = pool.clone();
        drop(pool_guard);

        let mut conn = pool_clone
            .get_conn()
            .await
            .map_err(|e| format!("mysql.execute() connection error: {}", e))?;

        use mysql_async::prelude::*;

        if params.is_empty() {
            conn.query_drop(&sql)
                .await
                .map_err(|e| format!("mysql.execute() error: {}", e))?;
        } else {
            conn.exec_drop(&sql, mysql_async::Params::Positional(params))
                .await
                .map_err(|e| format!("mysql.execute() error: {}", e))?;
        }

        Ok(Value::Int(conn.affected_rows() as i64))
    })
}

fn mysql_close(args: Vec<Value>) -> Result<Value, String> {
    let conn_id = match args.first() {
        Some(Value::String(s)) => s.clone(),
        _ => return Err("mysql.close() requires a connection ID".to_string()),
    };

    run_mysql(async move {
        let mut pool_guard = mysql_pool().lock().await;
        if let Some(pool) = pool_guard.remove(&conn_id) {
            drop(pool_guard);
            pool.disconnect()
                .await
                .map_err(|e| format!("mysql.close() error: {}", e))?;
            Ok(Value::Bool(true))
        } else {
            Ok(Value::Bool(false))
        }
    })
}

fn run_mysql<F>(future: F) -> Result<Value, String>
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_module() {
        let module = create_module();
        if let Value::Object(m) = module {
            assert!(m.contains_key("connect"));
            assert!(m.contains_key("query"));
            assert!(m.contains_key("execute"));
            assert!(m.contains_key("close"));
            assert_eq!(m.len(), 4);
        } else {
            panic!("expected object module");
        }
    }

    #[test]
    fn test_build_url_single_arg() {
        let url = build_connection_url(&[Value::String(
            "mysql://root:pass@localhost/mydb".to_string(),
        )])
        .unwrap();
        assert_eq!(url, "mysql://root:pass@localhost/mydb");
    }

    #[test]
    fn test_build_url_four_args() {
        let url = build_connection_url(&[
            Value::String("localhost".to_string()),
            Value::String("root".to_string()),
            Value::String("password".to_string()),
            Value::String("mydb".to_string()),
        ])
        .unwrap();
        assert_eq!(url, "mysql://root:password@localhost/mydb");
    }

    #[test]
    fn test_build_url_five_args() {
        let url = build_connection_url(&[
            Value::String("localhost".to_string()),
            Value::String("root".to_string()),
            Value::String("password".to_string()),
            Value::String("mydb".to_string()),
            Value::Int(3307),
        ])
        .unwrap();
        assert_eq!(url, "mysql://root:password@localhost:3307/mydb");
    }

    #[test]
    fn test_build_url_invalid() {
        let result = build_connection_url(&[Value::Int(42)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_forge_to_mysql_param() {
        assert_eq!(
            forge_to_mysql_param(&Value::Int(42)),
            mysql_async::Value::Int(42)
        );
        assert_eq!(
            forge_to_mysql_param(&Value::Float(3.14)),
            mysql_async::Value::Double(3.14)
        );
        assert_eq!(
            forge_to_mysql_param(&Value::String("hello".to_string())),
            mysql_async::Value::Bytes(b"hello".to_vec())
        );
        assert_eq!(
            forge_to_mysql_param(&Value::Bool(true)),
            mysql_async::Value::Int(1)
        );
        assert_eq!(forge_to_mysql_param(&Value::Null), mysql_async::Value::NULL);
    }

    #[test]
    fn test_mysql_val_to_forge() {
        assert_eq!(mysql_val_to_forge(mysql_async::Value::NULL), Value::Null);
        assert_eq!(
            mysql_val_to_forge(mysql_async::Value::Int(42)),
            Value::Int(42)
        );
        assert_eq!(
            mysql_val_to_forge(mysql_async::Value::Double(3.14)),
            Value::Float(3.14)
        );
        assert_eq!(
            mysql_val_to_forge(mysql_async::Value::Bytes(b"hello".to_vec())),
            Value::String("hello".to_string())
        );
    }

    #[test]
    fn test_connection_ids_unique() {
        let id1 = {
            let mut c = mysql_counter().lock().unwrap();
            *c += 1;
            format!("mysql_{}", *c)
        };
        let id2 = {
            let mut c = mysql_counter().lock().unwrap();
            *c += 1;
            format!("mysql_{}", *c)
        };
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_call_unknown_function() {
        let result = call("mysql.unknown", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown mysql function"));
    }

    #[test]
    fn test_query_missing_conn_id() {
        let result = mysql_query(vec![Value::Int(1)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("connection ID"));
    }

    #[test]
    fn test_execute_missing_sql() {
        let result = mysql_execute(vec![Value::String("mysql_1".to_string())]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("SQL string"));
    }
}
