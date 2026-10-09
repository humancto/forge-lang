pub mod crypto;
pub mod csv;
#[cfg(feature = "host")]
pub mod db;
pub mod env;
pub mod exec_module;
pub mod fs;
#[cfg(feature = "host")]
pub mod http;
pub mod io;
pub mod json_module;
pub mod jwt;
pub mod log;
pub mod math;
#[cfg(feature = "mysql")]
pub mod mysql;
pub mod npc;
#[cfg(feature = "host")]
pub mod os_module;
pub mod path_module;
#[cfg(feature = "postgres")]
pub mod pg;
pub mod regex_module;
pub mod term;
pub mod time;
pub mod toml_module;
pub mod types_module;
pub mod unavailable;
pub mod url_module;
#[cfg(feature = "host")]
pub mod ws;

// Modules marked `host` need an operating system; builds without the
// feature register `unavailable` stand-ins for them instead.
//
// Module objects and dispatch are registered through
// `crate::builtins_registry::modules()`, the single list both engines use.
