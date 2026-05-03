#![cfg(feature = "otel")]

use std::net::TcpListener;
use std::time::Duration;

use forge_lang::runtime::tracing_init;

struct EnvGuard {
    vars: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    fn set(vars: Vec<(&'static str, String)>) -> Self {
        let vars = vars
            .into_iter()
            .map(|(key, value)| {
                let old = std::env::var(key).ok();
                std::env::set_var(key, value);
                (key, old)
            })
            .collect();
        Self { vars }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, old) in &self.vars {
            if let Some(value) = old {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }
}

fn unused_local_endpoint() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn otel_export_path_initializes_and_flushes_without_hanging() {
    let endpoint = unused_local_endpoint();
    let _env = EnvGuard::set(vec![
        ("OTEL_EXPORTER_OTLP_ENDPOINT", endpoint),
        ("OTEL_EXPORTER_OTLP_TIMEOUT", "1000".to_string()),
        ("OTEL_BSP_EXPORT_TIMEOUT", "2000".to_string()),
    ]);

    tracing_init::init_otel();
    assert!(
        tracing_init::otel_is_active(),
        "init_otel should mark OTel active when exporter construction succeeds"
    );

    tracing_init::init_subscriber();
    {
        let span = tracing::info_span!("otel_smoke", smoke = true);
        let _entered = span.enter();
        tracing::info!(target: "forge.test", "smoke span event");
    }

    tokio::time::timeout(
        Duration::from_secs(5),
        tokio::task::spawn_blocking(tracing_init::flush_otel),
    )
    .await
    .expect("flush_otel timed out")
    .expect("flush_otel task panicked");
}
