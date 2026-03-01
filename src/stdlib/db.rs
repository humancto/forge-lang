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

/// Convert a Vec<Value> into rusqlite params
fn value_to_sql_params(params: &[Value]) -> Vec<Box<dyn rusqlite::types::ToSql>> {
    params
        .iter()
        .map(|v| -> Box<dyn rusqlite::types::ToSql> {
            match v {
                Value::Int(n) => Box::new(*n),
                Value::Float(n) => Box::new(*n),
                Value::String(s) => Box::new(s.clone()),
                Value::Bool(b) => Box::new(*b),
                Value::Null => Box::new(rusqlite::types::Null),
                other => Box::new(format!("{}", other)),
            }
        })
        .collect()
}

fn query_rows(
    c: &Connection,
    sql: &str,
    params: &[Box<dyn rusqlite::types::ToSql>],
) -> Result<Value, String> {
    let mut stmt = c
        .prepare(sql)
        .map_err(|e| format!("db.query error: {}", e))?;
    let col_count = stmt.column_count();
    let col_names: Vec<String> = (0..col_count)
        .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
        .collect();

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt
        .query_map(param_refs.as_slice(), |row| {
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
                // Check for optional params array as second argument
                if let Some(Value::Array(params)) = args.get(1) {
                    let sql_params = value_to_sql_params(params);
                    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                        sql_params.iter().map(|p| p.as_ref()).collect();
                    c.execute(sql, param_refs.as_slice())
                        .map(|_| Value::Null)
                        .map_err(|e| format!("db.execute error: {}", e))
                } else {
                    c.execute_batch(sql)
                        .map(|_| Value::Null)
                        .map_err(|e| format!("db.execute error: {}", e))
                }
            }
            _ => Err("db.execute() requires a SQL string".to_string()),
        },

        "db.query" => match args.first() {
            Some(Value::String(sql)) => {
                let conn = get_conn()?;
                let c = conn.lock().map_err(|e| format!("lock error: {}", e))?;
                // Check for optional params array as second argument
                let params = if let Some(Value::Array(p)) = args.get(1) {
                    value_to_sql_params(p)
                } else {
                    vec![]
                };
                query_rows(&c, sql, &params)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_parameterized_query() {
        call("db.open".into(), vec![Value::String(":memory:".into())]).unwrap();
        call(
            "db.execute".into(),
            vec![Value::String(
                "CREATE TABLE t (id INTEGER, name TEXT)".into(),
            )],
        )
        .unwrap();
        call(
            "db.execute".into(),
            vec![
                Value::String("INSERT INTO t VALUES (?, ?)".into()),
                Value::Array(vec![Value::Int(1), Value::String("alice".into())]),
            ],
        )
        .unwrap();
        call(
            "db.execute".into(),
            vec![
                Value::String("INSERT INTO t VALUES (?, ?)".into()),
                Value::Array(vec![Value::Int(2), Value::String("bob".into())]),
            ],
        )
        .unwrap();

        let result = call(
            "db.query".into(),
            vec![
                Value::String("SELECT * FROM t WHERE id = ?".into()),
                Value::Array(vec![Value::Int(1)]),
            ],
        )
        .unwrap();

        if let Value::Array(rows) = result {
            assert_eq!(rows.len(), 1);
            if let Value::Object(row) = &rows[0] {
                assert_eq!(row.get("name"), Some(&Value::String("alice".into())));
            } else {
                panic!("expected object row");
            }
        } else {
            panic!("expected array result");
        }
        call("db.close".into(), vec![]).unwrap();
    }

    #[test]
    fn db_still_works_without_params() {
        call("db.open".into(), vec![Value::String(":memory:".into())]).unwrap();
        call(
            "db.execute".into(),
            vec![Value::String(
                "CREATE TABLE t2 (id INTEGER); INSERT INTO t2 VALUES (42);".into(),
            )],
        )
        .unwrap();
        let result = call(
            "db.query".into(),
            vec![Value::String("SELECT * FROM t2".into())],
        )
        .unwrap();
        if let Value::Array(rows) = result {
            assert_eq!(rows.len(), 1);
        } else {
            panic!("expected array");
        }
        call("db.close".into(), vec![]).unwrap();
    }
}
