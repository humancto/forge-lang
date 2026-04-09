use crate::interpreter::Value;
use indexmap::IndexMap;
use std::sync::Arc;

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

/// TLS mode for pg.connect(). Defaults to [`PgTlsMode::Tls`] when no mode is
/// specified — connecting to production databases over plain TCP would leak
/// credentials and query data.
#[derive(Debug, Clone, PartialEq)]
pub enum PgTlsMode {
    /// Plain TCP — no encryption. Must be requested explicitly via "disable"
    /// (or "none"/"no-tls"/"plain"). Use only for local sockets / dev.
    NoTls,
    /// TLS with full certificate verification. The secure default.
    Tls,
    /// TLS with certificate verification disabled. Dev/testing only.
    TlsNoVerify,
}

impl PgTlsMode {
    /// Parse a mode string. Empty / unknown values fall back to the secure
    /// [`PgTlsMode::Tls`] default — explicit opt-outs require an explicit
    /// keyword like "disable".
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "" => PgTlsMode::Tls,
            "tls" | "ssl" | "require" | "verify-full" => PgTlsMode::Tls,
            "tls-no-verify" | "ssl-no-verify" | "no-verify" => PgTlsMode::TlsNoVerify,
            "disable" | "none" | "no-tls" | "plain" => PgTlsMode::NoTls,
            _ => PgTlsMode::Tls,
        }
    }
}

/// Build a rustls ClientConfig that verifies server certificates using the
/// platform's native root certificates (webpki-roots).
fn make_tls_connector(verify: bool) -> Result<tokio_postgres_rustls::MakeRustlsConnect, String> {
    // Install ring as the process-level crypto provider if none is set yet.
    // This is idempotent — safe to call multiple times.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let config = if verify {
        // Full verification: load platform/webpki roots
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    } else {
        // Skip server certificate verification — dev/test only
        rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(std::sync::Arc::new(NoVerifier))
            .with_no_client_auth()
    };
    Ok(tokio_postgres_rustls::MakeRustlsConnect::new(config))
}

/// A rustls ServerCertVerifier that accepts any certificate.
/// SECURITY: Only use for development/testing — never in production.
#[derive(Debug)]
struct NoVerifier;

impl rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        // Return all common signature schemes statically rather than calling
        // ring defaults (which requires an installed crypto provider).
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

/// Convert a Forge Value into a boxed tokio_postgres ToSql parameter.
fn forge_to_pg_param(val: &Value) -> Box<dyn tokio_postgres::types::ToSql + Sync + Send> {
    match val {
        Value::Int(n) => Box::new(*n),
        Value::Float(f) => Box::new(*f),
        Value::String(s) => Box::new(s.clone()),
        Value::Bool(b) => Box::new(*b),
        Value::Null | Value::None => Box::new(Option::<String>::None),
        other => Box::new(format!("{}", other)),
    }
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        // ── pg.connect(conn_str)                  → TLS with verification (secure default)
        // ── pg.connect(conn_str, "tls")           → TLS + cert verification
        // ── pg.connect(conn_str, "tls-no-verify") → TLS, skip cert verify (dev)
        // ── pg.connect(conn_str, "disable")       → plain TCP, no TLS
        "pg.connect" => match args.first() {
            Some(Value::String(conn_str)) => {
                let tls_mode = args
                    .get(1)
                    .and_then(|v| {
                        if let Value::String(s) = v {
                            Some(PgTlsMode::parse(s))
                        } else {
                            None
                        }
                    })
                    .unwrap_or(PgTlsMode::Tls);

                let handle = tokio::runtime::Handle::try_current()
                    .map_err(|_| "pg.connect requires async runtime".to_string())?;

                let conn_str = conn_str.clone();
                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        match tls_mode {
                            PgTlsMode::NoTls => {
                                let (client, connection) =
                                    tokio_postgres::connect(&conn_str, tokio_postgres::NoTls)
                                        .await
                                        .map_err(|e| format!("pg.connect error: {}", e))?;
                                tokio::spawn(async move {
                                    if let Err(e) = connection.await {
                                        eprintln!("pg connection error: {}", e);
                                    }
                                });
                                PG_CLIENT.with(|cell| *cell.borrow_mut() = Some(Arc::new(client)));
                            }
                            PgTlsMode::Tls => {
                                let tls = make_tls_connector(true)?;
                                let (client, connection) = tokio_postgres::connect(&conn_str, tls)
                                    .await
                                    .map_err(|e| format!("pg.connect (TLS) error: {}", e))?;
                                tokio::spawn(async move {
                                    if let Err(e) = connection.await {
                                        eprintln!("pg TLS connection error: {}", e);
                                    }
                                });
                                PG_CLIENT.with(|cell| *cell.borrow_mut() = Some(Arc::new(client)));
                            }
                            PgTlsMode::TlsNoVerify => {
                                let tls = make_tls_connector(false)?;
                                let (client, connection) =
                                    tokio_postgres::connect(&conn_str, tls).await.map_err(|e| {
                                        format!("pg.connect (TLS-no-verify) error: {}", e)
                                    })?;
                                tokio::spawn(async move {
                                    if let Err(e) = connection.await {
                                        eprintln!("pg TLS-no-verify connection error: {}", e);
                                    }
                                });
                                PG_CLIENT.with(|cell| *cell.borrow_mut() = Some(Arc::new(client)));
                            }
                        }
                        Ok::<Value, String>(Value::Bool(true))
                    })
                })
            }
            _ => Err("pg.connect() requires a connection string".to_string()),
        },

        "pg.query" => match args.first() {
            Some(Value::String(sql)) => {
                let handle = tokio::runtime::Handle::try_current()
                    .map_err(|_| "pg.query requires async runtime".to_string())?;

                let sql = sql.clone();
                let param_vals: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> =
                    match args.get(1) {
                        Some(Value::Array(arr)) => arr.iter().map(forge_to_pg_param).collect(),
                        _ => vec![],
                    };

                // Clone the Arc out of the thread-local before doing any async work.
                // This drops the RefCell borrow immediately and gives the async task
                // independent ownership — no raw pointers, no `unsafe`, and the client
                // stays alive even if the slot is later cleared.
                let client = PG_CLIENT
                    .with(|cell| cell.borrow().as_ref().map(Arc::clone))
                    .ok_or_else(|| "no pg connection open".to_string())?;

                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                            param_vals
                                .iter()
                                .map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync))
                                .collect();

                        // Await directly — no nested block_on
                        let rows = client
                            .query(sql.as_str(), param_refs.as_slice())
                            .await
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
            }
            _ => Err("pg.query() requires a SQL string".to_string()),
        },

        "pg.execute" => match args.first() {
            Some(Value::String(sql)) => {
                let handle = tokio::runtime::Handle::try_current()
                    .map_err(|_| "pg.execute requires async runtime".to_string())?;

                let sql = sql.clone();
                let param_vals: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> =
                    match args.get(1) {
                        Some(Value::Array(arr)) => arr.iter().map(forge_to_pg_param).collect(),
                        _ => vec![],
                    };

                // Clone the Arc out of the thread-local before any async work — same
                // pattern as pg.query.
                let client = PG_CLIENT
                    .with(|cell| cell.borrow().as_ref().map(Arc::clone))
                    .ok_or_else(|| "no pg connection open".to_string())?;

                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                            param_vals
                                .iter()
                                .map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync))
                                .collect();

                        // Await directly — no nested block_on
                        let count = client
                            .execute(sql.as_str(), param_refs.as_slice())
                            .await
                            .map_err(|e| format!("pg.execute error: {}", e))?;
                        Ok(Value::Int(count as i64))
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
    static PG_CLIENT: std::cell::RefCell<Option<Arc<tokio_postgres::Client>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Module structure ─────────────────────────────────────────────────────

    #[test]
    fn test_create_module_has_all_functions() {
        let module = create_module();
        if let Value::Object(m) = module {
            assert!(m.contains_key("connect"), "missing connect");
            assert!(m.contains_key("query"), "missing query");
            assert!(m.contains_key("execute"), "missing execute");
            assert!(m.contains_key("close"), "missing close");
            assert_eq!(m.len(), 4, "unexpected extra keys");
        } else {
            panic!("expected Object, got {:?}", module);
        }
    }

    // ── PgTlsMode parsing ────────────────────────────────────────────────────

    #[test]
    fn tls_mode_explicit_disable_keywords() {
        // Plain TCP only when explicitly requested via libpq-style "disable"
        // or its aliases.
        assert_eq!(PgTlsMode::parse("disable"), PgTlsMode::NoTls);
        assert_eq!(PgTlsMode::parse("none"), PgTlsMode::NoTls);
        assert_eq!(PgTlsMode::parse("no-tls"), PgTlsMode::NoTls);
        assert_eq!(PgTlsMode::parse("plain"), PgTlsMode::NoTls);
        assert_eq!(PgTlsMode::parse("DISABLE"), PgTlsMode::NoTls);
    }

    #[test]
    fn tls_mode_default_is_secure() {
        // Empty string and unknown values must default to TLS (secure-by-default).
        assert_eq!(PgTlsMode::parse(""), PgTlsMode::Tls);
        assert_eq!(PgTlsMode::parse("garbage"), PgTlsMode::Tls);
        assert_eq!(PgTlsMode::parse("verify-full"), PgTlsMode::Tls);
    }

    #[test]
    fn tls_mode_tls_variants() {
        assert_eq!(PgTlsMode::parse("tls"), PgTlsMode::Tls);
        assert_eq!(PgTlsMode::parse("ssl"), PgTlsMode::Tls);
        assert_eq!(PgTlsMode::parse("require"), PgTlsMode::Tls);
        assert_eq!(PgTlsMode::parse("TLS"), PgTlsMode::Tls); // case-insensitive
        assert_eq!(PgTlsMode::parse("SSL"), PgTlsMode::Tls);
    }

    #[test]
    fn tls_mode_no_verify_variants() {
        assert_eq!(PgTlsMode::parse("tls-no-verify"), PgTlsMode::TlsNoVerify);
        assert_eq!(PgTlsMode::parse("ssl-no-verify"), PgTlsMode::TlsNoVerify);
        assert_eq!(PgTlsMode::parse("no-verify"), PgTlsMode::TlsNoVerify);
        assert_eq!(PgTlsMode::parse("TLS-NO-VERIFY"), PgTlsMode::TlsNoVerify);
    }

    // ── forge_to_pg_param ────────────────────────────────────────────────────

    #[test]
    fn param_int() {
        let _ = forge_to_pg_param(&Value::Int(42));
    }

    #[test]
    fn param_float() {
        let _ = forge_to_pg_param(&Value::Float(3.14));
    }

    #[test]
    fn param_string() {
        let _ = forge_to_pg_param(&Value::String("hello".to_string()));
    }

    #[test]
    fn param_bool_true() {
        let _ = forge_to_pg_param(&Value::Bool(true));
    }

    #[test]
    fn param_bool_false() {
        let _ = forge_to_pg_param(&Value::Bool(false));
    }

    #[test]
    fn param_null() {
        let _ = forge_to_pg_param(&Value::Null);
    }

    #[test]
    fn param_none() {
        let _ = forge_to_pg_param(&Value::None);
    }

    // ── Error paths (no runtime / wrong args) ────────────────────────────────

    #[test]
    fn connect_no_runtime_fails_gracefully() {
        let result = call(
            "pg.connect",
            vec![Value::String("host=localhost".to_string())],
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("async runtime") || err.contains("pg.connect"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn connect_tls_no_runtime_fails_gracefully() {
        let result = call(
            "pg.connect",
            vec![
                Value::String("host=localhost".to_string()),
                Value::String("tls".to_string()),
            ],
        );
        assert!(result.is_err());
    }

    #[test]
    fn connect_tls_no_verify_no_runtime_fails_gracefully() {
        let result = call(
            "pg.connect",
            vec![
                Value::String("host=localhost".to_string()),
                Value::String("tls-no-verify".to_string()),
            ],
        );
        assert!(result.is_err());
    }

    #[test]
    fn connect_wrong_arg_type() {
        let result = call("pg.connect", vec![Value::Int(42)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("connection string"));
    }

    #[test]
    fn connect_no_args() {
        let result = call("pg.connect", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("connection string"));
    }

    #[test]
    fn query_wrong_arg_type() {
        let result = call("pg.query", vec![Value::Int(99)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("SQL string"));
    }

    #[test]
    fn query_no_args() {
        let result = call("pg.query", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("SQL string"));
    }

    #[test]
    fn execute_wrong_arg_type() {
        let result = call("pg.execute", vec![Value::Bool(true)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("SQL string"));
    }

    #[test]
    fn execute_no_args() {
        let result = call("pg.execute", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("SQL string"));
    }

    #[test]
    fn close_with_no_connection_is_ok() {
        // close() when nothing is open should succeed silently
        let result = call("pg.close", vec![]);
        assert!(result.is_ok());
    }

    #[test]
    fn unknown_function_returns_error() {
        let result = call("pg.whatever", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown pg function"));
    }

    // ── TLS connector construction ────────────────────────────────────────────

    #[test]
    fn make_tls_connector_verified_builds() {
        // Should succeed — just building the rustls config
        let result = make_tls_connector(true);
        assert!(result.is_ok(), "TLS connector (verified) failed");
    }

    #[test]
    fn make_tls_connector_no_verify_builds() {
        let result = make_tls_connector(false);
        assert!(result.is_ok(), "TLS connector (no-verify) failed");
    }
}
