pub mod crypto;
pub mod csv;
pub mod db;
pub mod env;
pub mod exec_module;
pub mod fs;
pub mod http;
pub mod io;
pub mod json_module;
pub mod log;
pub mod math;
pub mod pg;
pub mod regex_module;
pub mod term;

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
