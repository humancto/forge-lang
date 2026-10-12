//! A database the program's top level opens is shared by its HTTP handlers
//! (`stdlib::db_share`), on both serving engines.
//!
//! Hammers one SQLite connection from concurrent requests and checks that
//! no update is lost, that a transaction stays private to the request that
//! opened it (a read-modify-write inside `db.begin()`/`db.commit()` is
//! exact under concurrency), that a transaction a handler leaves open is
//! rolled back when its request ends, and that `db.last_insert_rowid()` is
//! the rowid of the request's own INSERT.

#[path = "support/server.rs"]
mod support;

use std::sync::Arc;
use std::time::Duration;

use support::{spawn_server_on, Engine};

macro_rules! on_both_engines {
    ($($name:ident),* $(,)?) => {$(
        mod $name {
            #[test]
            fn vm() {
                super::$name(super::Engine::Vm)
            }

            #[test]
            fn interpreter() {
                super::$name(super::Engine::Interpreter)
            }
        }
    )*};
}

on_both_engines!(
    top_level_database_is_shared_by_concurrent_handlers,
    in_memory_database_is_shared_by_handlers,
);

const PROGRAM: &str = r#"
@server(port: __PORT__)

db.open(__DB__)
db.execute("CREATE TABLE IF NOT EXISTS counter (id INTEGER PRIMARY KEY, n INTEGER)")
db.execute("CREATE TABLE IF NOT EXISTS items (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)")
db.execute("INSERT INTO counter (id, n) VALUES (1, 0), (2, 0)")

@get("/ping")
fn ping() {
    return { ok: true }
}

// One statement: atomic on its own.
@post("/bump")
fn bump() {
    db.execute("UPDATE counter SET n = n + 1 WHERE id = 1")
    return { ok: true }
}

// Read-modify-write: exact only if no other request's statement runs
// inside this transaction.
@post("/tx-bump")
fn tx_bump() {
    db.begin()
    let n = db.query("SELECT n FROM counter WHERE id = 2")[0].n
    db.execute("UPDATE counter SET n = ? WHERE id = 2", [n + 1])
    db.commit()
    return { n: n + 1 }
}

// Leaves its transaction open: rolled back when the request ends.
@post("/leak")
fn leak() {
    db.begin()
    db.execute("UPDATE counter SET n = n + 1000 WHERE id = 1")
    return { leaked: true }
}

@post("/items")
fn add_item(i) {
    db.execute("INSERT INTO items (name) VALUES (?)", ["item {i}"])
    let id = db.last_insert_rowid()
    let row = db.query("SELECT name FROM items WHERE id = ?", [id])
    return { id: id, name: row[0].name }
}

@get("/counts")
fn counts() {
    let rows = db.query("SELECT id, n FROM counter ORDER BY id")
    return { plain: rows[0].n, tx: rows[1].n, items: db.query("SELECT COUNT(*) AS c FROM items")[0].c }
}
"#;

const CLIENTS: usize = 8;
const PER_CLIENT: usize = 25;

fn temp_db(tag: &str, engine: Engine) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "forge_server_db_{}_{:?}_{}.sqlite",
        tag,
        engine,
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .expect("client")
}

fn json(response: reqwest::blocking::Response) -> serde_json::Value {
    let status = response.status();
    let text = response.text().expect("body");
    assert!(status.is_success(), "{}: {}", status, text);
    serde_json::from_str(&text).expect("JSON")
}

/// Run `CLIENTS` threads that each send `PER_CLIENT` requests built by
/// `request(client, i)`; returns every response body.
fn hammer(
    port: u16,
    request: impl Fn(&reqwest::blocking::Client, &str, usize) -> reqwest::blocking::Response
        + Send
        + Sync
        + 'static,
) -> Vec<serde_json::Value> {
    let request = Arc::new(request);
    let base = format!("http://127.0.0.1:{}", port);
    let threads: Vec<_> = (0..CLIENTS)
        .map(|c| {
            let request = Arc::clone(&request);
            let base = base.clone();
            std::thread::spawn(move || {
                let client = client();
                (0..PER_CLIENT)
                    .map(|i| json(request(&client, &base, c * PER_CLIENT + i)))
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    threads
        .into_iter()
        .flat_map(|t| t.join().expect("client thread"))
        .collect()
}

fn run_program(engine: Engine, db: &str) {
    let source = PROGRAM.replace("__DB__", db);
    let port = spawn_server_on(
        &source,
        engine,
        forge_lang::runtime::recursion::configure_runtime,
    );
    let base = format!("http://127.0.0.1:{}", port);
    let total = (CLIENTS * PER_CLIENT) as i64;

    hammer(port, |c, base, _| {
        c.post(format!("{}/bump", base)).send().expect("bump")
    });
    let tx = hammer(port, |c, base, _| {
        c.post(format!("{}/tx-bump", base)).send().expect("tx-bump")
    });
    // Every read-modify-write saw a distinct value: no two transactions
    // interleaved.
    let mut seen: Vec<i64> = tx.iter().map(|v| v["n"].as_i64().expect("n")).collect();
    seen.sort_unstable();
    assert_eq!(seen, (1..=total).collect::<Vec<_>>());

    // A leaked transaction is rolled back at the end of its request; the
    // next requests are not blocked by it.
    let c = client();
    json(c.post(format!("{}/leak", base)).send().expect("leak"));
    json(
        c.post(format!("{}/bump", base))
            .send()
            .expect("bump after leak"),
    );

    let items = hammer(port, |c, base, i| {
        c.post(format!("{}/items?i={}", base, i))
            .send()
            .expect("items")
    });
    // Each request read back the row its own INSERT created.
    for item in &items {
        let id = item["id"].as_i64().expect("id");
        assert!(id > 0, "{}", item);
        assert!(item["name"].as_str().is_some(), "{}", item);
    }
    let mut ids: Vec<i64> = items
        .iter()
        .map(|v| v["id"].as_i64().unwrap_or(0))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), items.len(), "two requests saw the same rowid");

    let counts = json(c.get(format!("{}/counts", base)).send().expect("counts"));
    assert_eq!(counts["plain"], total + 1, "lost updates: {}", counts);
    assert_eq!(counts["tx"], total, "{}", counts);
    assert_eq!(counts["items"], total, "{}", counts);
}

fn top_level_database_is_shared_by_concurrent_handlers(engine: Engine) {
    let path = temp_db("file", engine);
    let literal = format!("{:?}", path.display().to_string());
    run_program(engine, &literal);
    let _ = std::fs::remove_file(&path);
}

fn in_memory_database_is_shared_by_handlers(engine: Engine) {
    run_program(engine, "\":memory:\"");
}
