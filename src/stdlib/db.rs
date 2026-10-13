use crate::interpreter::Value;
use indexmap::IndexMap;
use rusqlite::Connection;

use super::db_share::Handle;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("open".to_string(), Value::BuiltIn("db.open".to_string()));
    m.insert("query".to_string(), Value::BuiltIn("db.query".to_string()));
    m.insert(
        "execute".to_string(),
        Value::BuiltIn("db.execute".to_string()),
    );
    m.insert("close".to_string(), Value::BuiltIn("db.close".to_string()));
    m.insert(
        "last_insert_rowid".to_string(),
        Value::BuiltIn("db.last_insert_rowid".to_string()),
    );
    m.insert("begin".to_string(), Value::BuiltIn("db.begin".to_string()));
    m.insert(
        "commit".to_string(),
        Value::BuiltIn("db.commit".to_string()),
    );
    m.insert(
        "rollback".to_string(),
        Value::BuiltIn("db.rollback".to_string()),
    );
    Value::Object(m)
}

/// A SQLite connection shared by the program's threads (`db_share`).
pub struct Sqlite {
    conn: Connection,
    /// `db.last_insert_rowid()` per transaction owner: concurrent requests
    /// share the connection, whose own last rowid is whichever INSERT ran
    /// last. Entries go away when their owner releases the connection.
    rowids: std::collections::HashMap<u64, i64>,
}

thread_local! {
    /// The connection current on this thread (shared with the program's
    /// other threads, see `db_share`).
    static DB_CONN: std::cell::RefCell<Option<Handle<Sqlite>>> = const { std::cell::RefCell::new(None) };
}

/// The connection current on this thread.
pub(crate) fn current_handle() -> Option<Handle<Sqlite>> {
    DB_CONN.with(|c| c.borrow().clone())
}

/// Make `handle` current on this thread; returns the previous one.
pub(crate) fn replace_handle(handle: Option<Handle<Sqlite>>) -> Option<Handle<Sqlite>> {
    DB_CONN.with(|c| std::mem::replace(&mut *c.borrow_mut(), handle))
}

/// `handle`'s owner is done: roll back the transaction it left open.
pub(crate) fn release(handle: &Handle<Sqlite>) {
    let owner = handle.owner;
    handle.release(|db, tx_open| {
        if tx_open {
            let _ = db.conn.execute_batch("ROLLBACK");
        }
        db.rowids.remove(&owner);
    });
}

fn current_or_err() -> Result<Handle<Sqlite>, String> {
    current_handle().ok_or_else(|| "no database connection open (call db.open() first)".to_string())
}

/// Whether `sql` starts with INSERT or REPLACE (after blanks and comments).
fn starts_insert(sql: &str) -> bool {
    let mut rest = sql.trim_start();
    loop {
        if let Some(r) = rest.strip_prefix("--") {
            rest = r
                .split_once('\n')
                .map(|(_, r)| r)
                .unwrap_or("")
                .trim_start();
        } else if let Some(r) = rest.strip_prefix("/*") {
            rest = r
                .split_once("*/")
                .map(|(_, r)| r)
                .unwrap_or("")
                .trim_start();
        } else {
            break;
        }
    }
    let word: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase();
    word == "INSERT" || word == "REPLACE"
}

/// Run `op` on the current connection. The connection's transaction owner
/// follows SQLite's own state, so `db.begin()` and `BEGIN` in SQL both
/// make the transaction private to this thread's owner (`db_share`). An
/// INSERT `sql` records its rowid for this owner.
fn with_conn<R>(
    sql: Option<&str>,
    op: impl FnOnce(&Connection) -> Result<R, String>,
) -> Result<R, String> {
    let handle = current_or_err()?;
    let owner = handle.owner;
    handle.run(
        |db| {
            let before = (db.conn.last_insert_rowid(), db.conn.total_changes());
            let result = op(&db.conn);
            let after = (db.conn.last_insert_rowid(), db.conn.total_changes());
            let inserted =
                after.1 != before.1 && (after.0 != before.0 || sql.is_some_and(starts_insert));
            if result.is_ok() && inserted {
                db.rowids.insert(owner, after.0);
            }
            result
        },
        |db, _, _| !db.conn.is_autocommit(),
    )?
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

/// Whether the active policy bounds the filesystem. SQLite can open files
/// by itself (`ATTACH DATABASE '/any/path'`, `VACUUM INTO '/any/path'`,
/// `file:` URIs), so those features are only available to a script whose
/// `fs.read` and `fs.write` are both unrestricted.
fn sqlite_is_confined() -> bool {
    let caps = crate::permissions::current();
    !(caps.is_unrestricted(crate::permissions::Capability::Read)
        && caps.is_unrestricted(crate::permissions::Capability::Write))
}

/// Disable SQLite's own file access (ATTACH, and VACUUM INTO, which attaches
/// its target) on a connection opened under a bounded filesystem policy.
fn confine_sqlite(conn: &Connection) -> rusqlite::Result<()> {
    if sqlite_is_confined() {
        conn.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_ATTACHED, 0)?;
    }
    Ok(())
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    crate::permissions::require(crate::permissions::Capability::Db, name)?;
    match name {
        "db.open" => match args.first() {
            Some(Value::String(path)) => {
                // Reopening replaces this thread's connection: close it
                // first (other threads keep theirs), which gives its slot
                // back unless another thread still uses it.
                if let Some(old) = replace_handle(None) {
                    release(&old);
                }
                let file =
                    crate::runtime::limits::acquire(crate::runtime::limits::Resource::Files)?;
                let conn = if path == ":memory:" {
                    Connection::open_in_memory()
                } else {
                    // Opening a database file reads and writes it; open the
                    // path that was checked, never the original string.
                    crate::stdlib::fs::confine_read(path)?;
                    let checked = crate::stdlib::fs::confine_write(path)?;
                    if sqlite_is_confined() {
                        // No `file:` URI filenames: SQLite would otherwise
                        // reinterpret the checked path as a URI and open a
                        // different file than the one approved.
                        Connection::open_with_flags(
                            checked,
                            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                                | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
                                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
                        )
                    } else {
                        Connection::open(checked)
                    }
                };
                let conn = conn.and_then(|c| {
                    confine_sqlite(&c)?;
                    Ok(c)
                });
                match conn {
                    Ok(conn) => {
                        let db = Sqlite {
                            conn,
                            rowids: std::collections::HashMap::new(),
                        };
                        replace_handle(Some(Handle::open(db, file)));
                        Ok(Value::Bool(true))
                    }
                    Err(e) => Err(format!("db.open error: {}", e)),
                }
            }
            _ => Err("db.open() requires a path string (use ':memory:' for in-memory)".to_string()),
        },

        "db.execute" => match args.first() {
            Some(Value::String(sql)) => with_conn(Some(sql), |c| {
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
            }),
            _ => Err("db.execute() requires a SQL string".to_string()),
        },

        "db.query" => match args.first() {
            Some(Value::String(sql)) => with_conn(Some(sql), |c| {
                // Check for optional params array as second argument
                let params = if let Some(Value::Array(p)) = args.get(1) {
                    value_to_sql_params(p)
                } else {
                    vec![]
                };
                query_rows(c, sql, &params)
            }),
            _ => Err("db.query() requires a SQL string".to_string()),
        },

        "db.close" => {
            // Closes this thread's connection; a transaction it left open
            // is rolled back. Other threads keep using a shared connection.
            if let Some(old) = replace_handle(None) {
                release(&old);
            }
            Ok(Value::Null)
        }

        // The rowid of this thread's (request's) last INSERT, not of
        // whichever request inserted last on the shared connection.
        "db.last_insert_rowid" => {
            let handle = current_or_err()?;
            let owner = handle.owner;
            handle.run(
                |db| Value::Int(db.rowids.get(&owner).copied().unwrap_or(0)),
                |db, _, _| !db.conn.is_autocommit(),
            )
        }

        "db.begin" => with_conn(None, |c| {
            c.execute_batch("BEGIN")
                .map(|_| Value::Null)
                .map_err(|e| format!("db.begin error: {}", e))
        }),

        "db.commit" => with_conn(None, |c| {
            c.execute_batch("COMMIT")
                .map(|_| Value::Null)
                .map_err(|e| format!("db.commit error: {}", e))
        }),

        "db.rollback" => with_conn(None, |c| {
            c.execute_batch("ROLLBACK")
                .map(|_| Value::Null)
                .map_err(|e| format!("db.rollback error: {}", e))
        }),

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

    #[test]
    fn db_last_insert_rowid() {
        call("db.open".into(), vec![Value::String(":memory:".into())]).unwrap();
        call(
            "db.execute".into(),
            vec![Value::String(
                "CREATE TABLE items (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)".into(),
            )],
        )
        .unwrap();

        // Insert using parameterized query
        call(
            "db.execute".into(),
            vec![
                Value::String("INSERT INTO items (name) VALUES (?)".into()),
                Value::Array(vec![Value::String("forge".into())]),
            ],
        )
        .unwrap();

        let rowid = call("db.last_insert_rowid".into(), vec![]).unwrap();
        assert_eq!(rowid, Value::Int(1));

        // Insert again — rowid should increment
        call(
            "db.execute".into(),
            vec![
                Value::String("INSERT INTO items (name) VALUES (?)".into()),
                Value::Array(vec![Value::String("lang".into())]),
            ],
        )
        .unwrap();

        let rowid2 = call("db.last_insert_rowid".into(), vec![]).unwrap();
        assert_eq!(rowid2, Value::Int(2));

        call("db.close".into(), vec![]).unwrap();
    }

    #[test]
    fn db_transaction_commit_persists() {
        call("db.open".into(), vec![Value::String(":memory:".into())]).unwrap();
        call(
            "db.execute".into(),
            vec![Value::String("CREATE TABLE tx (id INTEGER)".into())],
        )
        .unwrap();

        call("db.begin".into(), vec![]).unwrap();
        call(
            "db.execute".into(),
            vec![
                Value::String("INSERT INTO tx VALUES (?)".into()),
                Value::Array(vec![Value::Int(1)]),
            ],
        )
        .unwrap();
        call("db.commit".into(), vec![]).unwrap();

        let result = call(
            "db.query".into(),
            vec![Value::String("SELECT * FROM tx".into())],
        )
        .unwrap();
        if let Value::Array(rows) = result {
            assert_eq!(rows.len(), 1);
        } else {
            panic!("expected array");
        }
        call("db.close".into(), vec![]).unwrap();
    }

    #[test]
    fn db_transaction_rollback_discards() {
        call("db.open".into(), vec![Value::String(":memory:".into())]).unwrap();
        call(
            "db.execute".into(),
            vec![Value::String("CREATE TABLE tx2 (id INTEGER)".into())],
        )
        .unwrap();
        // Seed one row outside any transaction.
        call(
            "db.execute".into(),
            vec![
                Value::String("INSERT INTO tx2 VALUES (?)".into()),
                Value::Array(vec![Value::Int(99)]),
            ],
        )
        .unwrap();

        call("db.begin".into(), vec![]).unwrap();
        call(
            "db.execute".into(),
            vec![
                Value::String("INSERT INTO tx2 VALUES (?)".into()),
                Value::Array(vec![Value::Int(1)]),
            ],
        )
        .unwrap();
        call("db.rollback".into(), vec![]).unwrap();

        let result = call(
            "db.query".into(),
            vec![Value::String("SELECT * FROM tx2".into())],
        )
        .unwrap();
        if let Value::Array(rows) = result {
            // Only the seeded row should remain.
            assert_eq!(rows.len(), 1);
            if let Value::Object(row) = &rows[0] {
                assert_eq!(row.get("id"), Some(&Value::Int(99)));
            } else {
                panic!("expected object row");
            }
        } else {
            panic!("expected array");
        }
        call("db.close".into(), vec![]).unwrap();
    }
}
