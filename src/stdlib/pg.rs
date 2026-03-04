use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert(
        "connect".to_string(),
        Value::BuiltIn("pg.connect".to_string()),
    );
    m.insert("query".to_string(), Value::BuiltIn("pg.query".to_string()));
    m.insert(
        "execute".to_string(),
        Value::BuiltIn("pg.execute".to_string()),
    );
    m.insert("close".to_string(), Value::BuiltIn("pg.close".to_string()));
    Value::Object(m)
}

/// Convert a Forge Value into a boxed tokio_postgres ToSql parameter.
fn forge_to_pg_param(val: &Value) -> Box<dyn tokio_postgres::types::ToSql + Sync + Send> {
    match val {
        Value::Int(n) => Box::new(*n),
        Value::Float(f) => Box::new(*f),
        Value::String(s) => Box::new(s.clone()),
        Value::Bool(b) => Box::new(*b),
        Value::Null | Value::None => Box::new(Option::<String>::None),
        other => Box::new(format!("{}", other)),
    }
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "pg.connect" => match args.first() {
            Some(Value::String(conn_str)) => {
                let handle = tokio::runtime::Handle::try_current()
                    .map_err(|_| "pg.connect requires async runtime".to_string())?;

                let conn_str = conn_str.clone();
                let result = tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        let (client, connection) =
                            tokio_postgres::connect(&conn_str, tokio_postgres::NoTls)
                                .await
                                .map_err(|e| format!("pg.connect error: {}", e))?;

                        tokio::spawn(async move {
                            if let Err(e) = connection.await {
                                eprintln!("pg connection error: {}", e);
                            }
                        });

                        PG_CLIENT.with(|cell| {
                            *cell.borrow_mut() = Some(client);
                        });

                        Ok::<Value, String>(Value::Bool(true))
                    })
                });
                result
            }
            _ => Err("pg.connect() requires a connection string".to_string()),
        },

        "pg.query" => match args.first() {
            Some(Value::String(sql)) => {
                let handle = tokio::runtime::Handle::try_current()
                    .map_err(|_| "pg.query requires async runtime".to_string())?;

                let sql = sql.clone();
                // Collect optional params from second argument
                let param_vals: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> =
                    match args.get(1) {
                        Some(Value::Array(arr)) => arr.iter().map(forge_to_pg_param).collect(),
                        _ => vec![],
                    };

                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        PG_CLIENT.with(|cell| {
                            let borrow = cell.borrow();
                            let client = borrow.as_ref().ok_or("no pg connection open")?;

                            let rt = tokio::runtime::Handle::current();
                            // Build a slice of &dyn ToSql references for the query
                            let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                                param_vals.iter().map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

                            let rows = rt
                                .block_on(client.query(sql.as_str(), param_refs.as_slice()))
                                .map_err(|e| format!("pg.query error: {}", e))?;

                            let mut results = Vec::new();
                            for row in &rows {
                                let mut map = IndexMap::new();
                                for (i, col) in row.columns().iter().enumerate() {
                                    let val = if let Ok(v) = row.try_get::<_, i64>(i) {
                                        Value::Int(v)
                                    } else if let Ok(v) = row.try_get::<_, f64>(i) {
                                        Value::Float(v)
                                    } else if let Ok(v) = row.try_get::<_, String>(i) {
                                        Value::String(v)
                                    } else if let Ok(v) = row.try_get::<_, bool>(i) {
                                        Value::Bool(v)
                                    } else {
                                        Value::Null
                                    };
                                    map.insert(col.name().to_string(), val);
                                }
                                results.push(Value::Object(map));
                            }
                            Ok(Value::Array(results))
                        })
                    })
                })
            }
            _ => Err("pg.query() requires a SQL string".to_string()),
        },

        "pg.execute" => match args.first() {
            Some(Value::String(sql)) => {
                let handle = tokio::runtime::Handle::try_current()
                    .map_err(|_| "pg.execute requires async runtime".to_string())?;

                let sql = sql.clone();
                // Collect optional params from second argument
                let param_vals: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> =
                    match args.get(1) {
                        Some(Value::Array(arr)) => arr.iter().map(forge_to_pg_param).collect(),
                        _ => vec![],
                    };

                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        PG_CLIENT.with(|cell| {
                            let borrow = cell.borrow();
                            let client = borrow.as_ref().ok_or("no pg connection open")?;
                            let rt = tokio::runtime::Handle::current();

                            // Build a slice of &dyn ToSql references for the execute
                            let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                                param_vals.iter().map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

                            let count = rt
                                .block_on(client.execute(sql.as_str(), param_refs.as_slice()))
                                .map_err(|e| format!("pg.execute error: {}", e))?;
                            Ok(Value::Int(count as i64))
                        })
                    })
                })
            }
            _ => Err("pg.execute() requires a SQL string".to_string()),
        },

        "pg.close" => {
            PG_CLIENT.with(|cell| {
                *cell.borrow_mut() = None;
            });
            Ok(Value::Null)
        }

        _ => Err(format!("unknown pg function: {}", name)),
    }
}

thread_local! {
    static PG_CLIENT: std::cell::RefCell<Option<tokio_postgres::Client>> = const { std::cell::RefCell::new(None) };
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
    fn test_forge_to_pg_param_int() {
        let p = forge_to_pg_param(&Value::Int(42));
        // We can't easily call ToSql directly in tests without a pg connection,
        // but we can verify it doesn't panic and produces a box.
        let _ = p;
    }

    #[test]
    fn test_forge_to_pg_param_string() {
        let p = forge_to_pg_param(&Value::String("hello".to_string()));
        let _ = p;
    }

    #[test]
    fn test_forge_to_pg_param_float() {
        let p = forge_to_pg_param(&Value::Float(3.14));
        let _ = p;
    }

    #[test]
    fn test_forge_to_pg_param_bool() {
        let p = forge_to_pg_param(&Value::Bool(true));
        let _ = p;
    }

    #[test]
    fn test_forge_to_pg_param_null() {
        let p = forge_to_pg_param(&Value::Null);
        let _ = p;
    }

    #[test]
    fn test_unknown_function() {
        let result = call("pg.unknown", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown pg function"));
    }

    #[test]
    fn test_query_requires_no_runtime() {
        // Without a runtime, pg.query should fail with a clear message
        let result = call(
            "pg.query",
            vec![Value::String("SELECT 1".to_string())],
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("async runtime") || err.contains("pg.query"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_execute_requires_no_runtime() {
        let result = call(
            "pg.execute",
            vec![Value::String("DELETE FROM t WHERE id = $1".to_string()),
                 Value::Array(vec![Value::Int(1)])],
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("async runtime") || err.contains("pg.execute"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_query_missing_sql() {
        let result = call("pg.query", vec![Value::Int(42)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("SQL string"));
    }

    #[test]
    fn test_execute_missing_sql() {
        let result = call("pg.execute", vec![Value::Int(42)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("SQL string"));
    }
}
