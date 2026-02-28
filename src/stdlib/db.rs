use crate::interpreter::Value;
use indexmap::IndexMap;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("open".to_string(), Value::BuiltIn("db.open".to_string()));
    m.insert("query".to_string(), Value::BuiltIn("db.query".to_string()));
    m.insert(
        "execute".to_string(),
        Value::BuiltIn("db.execute".to_string()),
    );
    m.insert("close".to_string(), Value::BuiltIn("db.close".to_string()));
    Value::Object(m)
}

thread_local! {
    static DB_CONN: std::cell::RefCell<Option<Arc<Mutex<Connection>>>> = const { std::cell::RefCell::new(None) };
}

fn get_conn() -> Result<Arc<Mutex<Connection>>, String> {
    DB_CONN.with(|c| {
        c.borrow()
            .clone()
            .ok_or_else(|| "no database connection open (call db.open() first)".to_string())
    })
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "db.open" => match args.first() {
            Some(Value::String(path)) => {
                let conn = if path == ":memory:" {
                    Connection::open_in_memory()
                } else {
                    Connection::open(path)
                };
                match conn {
                    Ok(c) => {
                        let arc = Arc::new(Mutex::new(c));
                        DB_CONN.with(|cell| {
                            *cell.borrow_mut() = Some(arc);
                        });
                        Ok(Value::Bool(true))
                    }
                    Err(e) => Err(format!("db.open error: {}", e)),
                }
            }
            _ => Err("db.open() requires a path string (use ':memory:' for in-memory)".to_string()),
        },

        "db.execute" => match args.first() {
            Some(Value::String(sql)) => {
                let conn = get_conn()?;
                let c = conn.lock().map_err(|e| format!("lock error: {}", e))?;
                c.execute_batch(sql)
                    .map(|_| Value::Null)
                    .map_err(|e| format!("db.execute error: {}", e))
            }
            _ => Err("db.execute() requires a SQL string".to_string()),
        },

        "db.query" => match args.first() {
            Some(Value::String(sql)) => {
                let conn = get_conn()?;
                let c = conn.lock().map_err(|e| format!("lock error: {}", e))?;
                let mut stmt = c
                    .prepare(sql)
                    .map_err(|e| format!("db.query error: {}", e))?;
                let col_count = stmt.column_count();
                let col_names: Vec<String> = (0..col_count)
                    .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
                    .collect();

                let rows = stmt
                    .query_map([], |row| {
                        let mut map = IndexMap::new();
                        for (i, name) in col_names.iter().enumerate() {
                            let val = match row.get_ref(i) {
                                Ok(rusqlite::types::ValueRef::Null) => Value::Null,
                                Ok(rusqlite::types::ValueRef::Integer(n)) => Value::Int(n),
                                Ok(rusqlite::types::ValueRef::Real(n)) => Value::Float(n),
                                Ok(rusqlite::types::ValueRef::Text(s)) => {
                                    Value::String(String::from_utf8_lossy(s).to_string())
                                }
                                Ok(rusqlite::types::ValueRef::Blob(b)) => {
                                    Value::String(format!("<blob {} bytes>", b.len()))
                                }
                                Err(_) => Value::Null,
                            };
                            map.insert(name.clone(), val);
                        }
                        Ok(Value::Object(map))
                    })
                    .map_err(|e| format!("db.query error: {}", e))?;

                let mut results = Vec::new();
                for row in rows {
                    match row {
                        Ok(v) => results.push(v),
                        Err(e) => return Err(format!("db.query row error: {}", e)),
                    }
                }
                Ok(Value::Array(results))
            }
            _ => Err("db.query() requires a SQL string".to_string()),
        },

        "db.close" => {
            DB_CONN.with(|cell| {
                *cell.borrow_mut() = None;
            });
            Ok(Value::Null)
        }

        _ => Err(format!("unknown db function: {}", name)),
    }
}
