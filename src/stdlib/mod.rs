pub mod crypto;
pub mod csv;
pub mod db;
pub mod env;
pub mod exec_module;
pub mod fs;
pub mod http;
pub mod io;
pub mod json_module;
pub mod jwt;
pub mod log;
pub mod math;
#[cfg(feature = "mysql")]
pub mod mysql;
pub mod npc;
#[cfg(feature = "postgres")]
pub mod pg;
pub mod regex_module;
pub mod term;
pub mod time;
pub mod toml_module;
pub mod url_module;
pub mod ws;

use crate::interpreter::Value;

pub fn create_math_module() -> Value {
    math::create_module()
}
pub fn create_fs_module() -> Value {
    fs::create_module()
}
pub fn create_io_module() -> Value {
    io::create_module()
}
pub fn create_crypto_module() -> Value {
    crypto::create_module()
}
pub fn create_db_module() -> Value {
    db::create_module()
}
pub fn create_env_module() -> Value {
    env::create_module()
}
pub fn create_json_module() -> Value {
    json_module::create_module()
}
pub fn create_regex_module() -> Value {
    regex_module::create_module()
}
pub fn create_log_module() -> Value {
    log::create_module()
}
#[cfg(feature = "postgres")]
pub fn create_pg_module() -> Value {
    pg::create_module()
}
pub fn create_term_module() -> Value {
    term::create_module()
}
pub fn create_http_module() -> Value {
    http::create_module()
}
pub fn create_csv_module() -> Value {
    csv::create_module()
}
pub fn create_time_module() -> Value {
    time::create_module()
}
pub fn create_npc_module() -> Value {
    npc::create_module()
}
pub fn create_url_module() -> Value {
    url_module::create_module()
}
pub fn create_toml_module() -> Value {
    toml_module::create_module()
}
pub fn create_ws_module() -> Value {
    ws::create_module()
}
pub fn create_jwt_module() -> Value {
    jwt::create_module()
}
#[cfg(feature = "mysql")]
pub fn create_mysql_module() -> Value {
    mysql::create_module()
}
