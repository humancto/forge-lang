use crate::interpreter::Value;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

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
    m.insert(
        "begin".to_string(),
        Value::BuiltIn("mysql.begin".to_string()),
    );
    m.insert(
        "commit".to_string(),
        Value::BuiltIn("mysql.commit".to_string()),
    );
    m.insert(
        "rollback".to_string(),
        Value::BuiltIn("mysql.rollback".to_string()),
    );
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

#[derive(Clone)]
struct ActiveMysqlTx {
    origin_conn_id: String,
    conn: Arc<tokio::sync::Mutex<mysql_async::Conn>>,
}

fn mysql_transactions() -> &'static tokio::sync::Mutex<HashMap<String, ActiveMysqlTx>> {
    static TXS: OnceLock<tokio::sync::Mutex<HashMap<String, ActiveMysqlTx>>> = OnceLock::new();
    TXS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "mysql.connect" => mysql_connect(args),
        "mysql.query" => mysql_query(args),
        "mysql.execute" => mysql_execute(args),
        "mysql.close" => mysql_close(args),
        "mysql.begin" => mysql_begin(args),
        "mysql.commit" => mysql_finish_tx(args, "COMMIT", "mysql.commit"),
        "mysql.rollback" => mysql_finish_tx(args, "ROLLBACK", "mysql.rollback"),
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
    let handle_id = match args.first() {
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
        if let Some(tx_conn) = active_mysql_tx_conn(&handle_id).await {
            let mut conn = tx_conn.lock().await;
            return query_on_mysql_conn(&mut conn, &sql, params).await;
        }

        let pool_guard = mysql_pool().lock().await;
        let pool = pool_guard
            .get(&handle_id)
            .ok_or_else(|| format!("MySQL connection '{}' not found", handle_id))?;
        let pool_clone = pool.clone();
        drop(pool_guard);

        let mut conn = pool_clone
            .get_conn()
            .await
            .map_err(|e| format!("mysql.query() connection error: {}", e))?;

        query_on_mysql_conn(&mut conn, &sql, params).await
    })
}

fn mysql_execute(args: Vec<Value>) -> Result<Value, String> {
    let handle_id = match args.first() {
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
        if let Some(tx_conn) = active_mysql_tx_conn(&handle_id).await {
            let mut conn = tx_conn.lock().await;
            return execute_on_mysql_conn(&mut conn, &sql, params).await;
        }

        let pool_guard = mysql_pool().lock().await;
        let pool = pool_guard
            .get(&handle_id)
            .ok_or_else(|| format!("MySQL connection '{}' not found", handle_id))?;
        let pool_clone = pool.clone();
        drop(pool_guard);

        let mut conn = pool_clone
            .get_conn()
            .await
            .map_err(|e| format!("mysql.execute() connection error: {}", e))?;

        execute_on_mysql_conn(&mut conn, &sql, params).await
    })
}

fn mysql_close(args: Vec<Value>) -> Result<Value, String> {
    let conn_id = match args.first() {
        Some(Value::String(s)) => s.clone(),
        _ => return Err("mysql.close() requires a connection ID".to_string()),
    };

    run_mysql(async move {
        let outstanding: Vec<String> = {
            let tx_guard = mysql_transactions().lock().await;
            tx_guard
                .iter()
                .filter_map(|(tx_id, tx)| {
                    if tx.origin_conn_id == conn_id {
                        Some(tx_id.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };
        if !outstanding.is_empty() {
            return Err(format!(
                "mysql.close() refused: active transactions for {}: {}",
                conn_id,
                outstanding.join(", ")
            ));
        }

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

async fn active_mysql_tx_conn(id: &str) -> Option<Arc<tokio::sync::Mutex<mysql_async::Conn>>> {
    let tx_guard = mysql_transactions().lock().await;
    tx_guard.get(id).map(|tx| Arc::clone(&tx.conn))
}

async fn query_on_mysql_conn(
    conn: &mut mysql_async::Conn,
    sql: &str,
    params: Vec<mysql_async::Value>,
) -> Result<Value, String> {
    use mysql_async::prelude::*;

    let rows: Vec<mysql_async::Row> = if params.is_empty() {
        conn.query(sql)
            .await
            .map_err(|e| format!("mysql.query() error: {}", e))?
    } else {
        conn.exec(sql, mysql_async::Params::Positional(params))
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
}

async fn execute_on_mysql_conn(
    conn: &mut mysql_async::Conn,
    sql: &str,
    params: Vec<mysql_async::Value>,
) -> Result<Value, String> {
    use mysql_async::prelude::*;

    if params.is_empty() {
        conn.query_drop(sql)
            .await
            .map_err(|e| format!("mysql.execute() error: {}", e))?;
    } else {
        conn.exec_drop(sql, mysql_async::Params::Positional(params))
            .await
            .map_err(|e| format!("mysql.execute() error: {}", e))?;
    }

    Ok(Value::Int(conn.affected_rows() as i64))
}

fn mysql_begin(args: Vec<Value>) -> Result<Value, String> {
    let conn_id = match args.first() {
        Some(Value::String(s)) => s.clone(),
        _ => return Err("mysql.begin() requires a connection ID".to_string()),
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
            .map_err(|e| format!("mysql.begin() connection error: {}", e))?;

        use mysql_async::prelude::*;
        conn.query_drop("BEGIN")
            .await
            .map_err(|e| format!("mysql.begin() error: {}", e))?;

        let tx_id = format!("mysql_tx_{}", uuid::Uuid::new_v4());
        mysql_transactions().lock().await.insert(
            tx_id.clone(),
            ActiveMysqlTx {
                origin_conn_id: conn_id,
                conn: Arc::new(tokio::sync::Mutex::new(conn)),
            },
        );

        Ok(Value::String(tx_id))
    })
}

fn mysql_finish_tx(
    args: Vec<Value>,
    stmt: &'static str,
    label: &'static str,
) -> Result<Value, String> {
    let tx_id = match args.first() {
        Some(Value::String(s)) => s.clone(),
        _ => return Err(format!("{}() requires a transaction ID", label)),
    };

    run_mysql(async move {
        let mut tx_guard = mysql_transactions().lock().await;
        let tx = tx_guard
            .get(&tx_id)
            .cloned()
            .ok_or_else(|| format!("MySQL transaction '{}' not found", tx_id))?;

        let mut conn = tx.conn.lock().await;
        use mysql_async::prelude::*;
        conn.query_drop(stmt)
            .await
            .map_err(|e| format!("{}() error: {}", label, e))?;
        tx_guard.remove(&tx_id);

        Ok(Value::Null)
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
            assert!(m.contains_key("begin"));
            assert!(m.contains_key("commit"));
            assert!(m.contains_key("rollback"));
            assert_eq!(m.len(), 7);
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

    #[test]
    fn test_begin_missing_conn_id() {
        let result = mysql_begin(vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("connection ID"));
    }

    #[test]
    fn test_begin_unknown_conn_id() {
        let result = mysql_begin(vec![Value::String("mysql_missing".to_string())]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_commit_missing_tx_id() {
        let result = mysql_finish_tx(vec![], "COMMIT", "mysql.commit");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("transaction ID"));
    }

    #[test]
    fn test_commit_unknown_tx_id() {
        let result = mysql_finish_tx(
            vec![Value::String("mysql_tx_missing".to_string())],
            "COMMIT",
            "mysql.commit",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    #[ignore = "requires FORGE_MYSQL_TEST_URL pointing at a disposable MySQL database"]
    fn live_transactions_commit_and_rollback() {
        let url = match std::env::var("FORGE_MYSQL_TEST_URL") {
            Ok(url) => url,
            Err(_) => return,
        };
        let table = format!("forge_tx_test_{}", std::process::id());
        let rt = tokio::runtime::Runtime::new().expect("test runtime");

        rt.block_on(async {
            let conn = call("mysql.connect", vec![Value::String(url)]).expect("connect");
            let conn_id = match conn {
                Value::String(id) => id,
                other => panic!("expected connection id, got {other:?}"),
            };

            call(
                "mysql.execute",
                vec![
                    Value::String(conn_id.clone()),
                    Value::String(format!("DROP TABLE IF EXISTS {table}")),
                ],
            )
            .expect("drop table");
            call(
                "mysql.execute",
                vec![
                    Value::String(conn_id.clone()),
                    Value::String(format!("CREATE TABLE {table} (id INTEGER PRIMARY KEY)")),
                ],
            )
            .expect("create table");

            let rollback_tx = call("mysql.begin", vec![Value::String(conn_id.clone())])
                .expect("begin rollback tx");
            let rollback_tx_id = match rollback_tx {
                Value::String(id) => id,
                other => panic!("expected tx id, got {other:?}"),
            };
            call(
                "mysql.execute",
                vec![
                    Value::String(rollback_tx_id.clone()),
                    Value::String(format!("INSERT INTO {table} VALUES (1)")),
                ],
            )
            .expect("insert rollback row");
            call("mysql.rollback", vec![Value::String(rollback_tx_id)]).expect("rollback");
            assert_eq!(count_rows(&conn_id, &table), 0);

            let commit_tx =
                call("mysql.begin", vec![Value::String(conn_id.clone())]).expect("begin commit tx");
            let commit_tx_id = match commit_tx {
                Value::String(id) => id,
                other => panic!("expected tx id, got {other:?}"),
            };
            call(
                "mysql.execute",
                vec![
                    Value::String(commit_tx_id.clone()),
                    Value::String(format!("INSERT INTO {table} VALUES (2)")),
                ],
            )
            .expect("insert commit row");
            call("mysql.commit", vec![Value::String(commit_tx_id)]).expect("commit");
            assert_eq!(count_rows(&conn_id, &table), 1);

            call(
                "mysql.execute",
                vec![
                    Value::String(conn_id.clone()),
                    Value::String(format!("DROP TABLE IF EXISTS {table}")),
                ],
            )
            .expect("cleanup table");
            call("mysql.close", vec![Value::String(conn_id)]).expect("close");
        });
    }

    fn count_rows(conn_id: &str, table: &str) -> i64 {
        let result = call(
            "mysql.query",
            vec![
                Value::String(conn_id.to_string()),
                Value::String(format!("SELECT COUNT(*) AS n FROM {table}")),
            ],
        )
        .expect("count rows");
        match result {
            Value::Array(rows) => match rows.first() {
                Some(Value::Object(row)) => match row.get("n") {
                    Some(Value::Int(n)) => *n,
                    other => panic!("expected integer count, got {other:?}"),
                },
                other => panic!("expected first row, got {other:?}"),
            },
            other => panic!("expected rows array, got {other:?}"),
        }
    }
}
