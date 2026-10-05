// Host runtime (feature `host`): the async runtime, HTTP server and client,
// background `schedule`/`watch` tasks and structured logging. Without the
// feature (the browser playground) `client` and `host` are replaced by
// `no_host`, whose entry points fail with a clear "not available" error, so
// the engines compile and call them unchanged.
#[cfg(feature = "host")]
pub mod client;
#[cfg(feature = "host")]
pub mod embedded;
#[cfg(feature = "host")]
pub mod host;
pub mod imports;
pub mod metadata;
#[cfg(not(feature = "host"))]
mod no_host;
pub mod recursion;
#[cfg(feature = "host")]
pub mod server;
pub mod shell;
pub mod stdio;
#[cfg(feature = "host")]
pub mod tracing_init;

#[cfg(not(feature = "host"))]
pub use no_host::{client, host};

/// The error text for a host capability this build does not have (the
/// browser playground, built with `--no-default-features`). `what` names
/// the feature, e.g. `"http.get"` or `"network access"`.
pub fn unavailable_message(what: &str) -> String {
    format!(
        "{} is not available in the browser playground (install the Forge CLI to use it)",
        what
    )
}
