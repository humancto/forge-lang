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
                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        PG_CLIENT.with(|cell| {
                            let borrow = cell.borrow();
                            let client = borrow.as_ref().ok_or("no pg connection open")?;

                            let rt = tokio::runtime::Handle::current();
                            let rows = rt
                                .block_on(client.query(&sql as &str, &[]))
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
                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        PG_CLIENT.with(|cell| {
                            let borrow = cell.borrow();
                            let client = borrow.as_ref().ok_or("no pg connection open")?;
                            let rt = tokio::runtime::Handle::current();
                            let count = rt
                                .block_on(client.execute(&sql as &str, &[]))
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
