use crate::interpreter::Value;
use indexmap::IndexMap;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde_json;
use std::collections::BTreeMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("sign".to_string(), Value::BuiltIn("jwt.sign".to_string()));
    m.insert(
        "verify".to_string(),
        Value::BuiltIn("jwt.verify".to_string()),
    );
    m.insert(
        "decode".to_string(),
        Value::BuiltIn("jwt.decode".to_string()),
    );
    m.insert("valid".to_string(), Value::BuiltIn("jwt.valid".to_string()));
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "jwt.sign" => jwt_sign(args),
        "jwt.verify" => jwt_verify(args),
        "jwt.decode" => jwt_decode(args),
        "jwt.valid" => jwt_valid(args),
        _ => Err(format!("unknown jwt function: {}", name)),
    }
}

fn parse_duration(s: &str) -> Result<i64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty duration string".to_string());
    }
    let (num_str, unit) = if s.ends_with("ms") {
        (&s[..s.len() - 2], "ms")
    } else if s.ends_with('s') {
        (&s[..s.len() - 1], "s")
    } else if s.ends_with('m') {
        (&s[..s.len() - 1], "m")
    } else if s.ends_with('h') {
        (&s[..s.len() - 1], "h")
    } else if s.ends_with('d') {
        (&s[..s.len() - 1], "d")
    } else if s.ends_with('w') {
        (&s[..s.len() - 1], "w")
    } else {
        return Err(format!("invalid duration unit in '{}'", s));
    };
    let n: i64 = num_str
        .parse()
        .map_err(|_| format!("invalid duration number in '{}'", s))?;
    match unit {
        "ms" => Ok(n / 1000), // sub-second → round to 0 for very small
        "s" => Ok(n),
        "m" => Ok(n * 60),
        "h" => Ok(n * 3600),
        "d" => Ok(n * 86400),
        "w" => Ok(n * 604800),
        _ => Err(format!("invalid duration unit in '{}'", s)),
    }
}

fn parse_algorithm(s: &str) -> Result<Algorithm, String> {
    match s.to_uppercase().as_str() {
        "HS256" => Ok(Algorithm::HS256),
        "HS384" => Ok(Algorithm::HS384),
        "HS512" => Ok(Algorithm::HS512),
        "RS256" => Ok(Algorithm::RS256),
        "RS384" => Ok(Algorithm::RS384),
        "RS512" => Ok(Algorithm::RS512),
        "ES256" => Ok(Algorithm::ES256),
        "ES384" => Ok(Algorithm::ES384),
        "NONE" | "none" => Err("algorithm 'none' is not allowed for security reasons".to_string()),
        other => Err(format!("unsupported JWT algorithm: {}", other)),
    }
}

fn value_to_json(val: &Value) -> serde_json::Value {
    match val {
        Value::Int(n) => serde_json::Value::Number(serde_json::Number::from(*n)),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Null | Value::None => serde_json::Value::Null,
        Value::Array(arr) => serde_json::Value::Array(arr.iter().map(value_to_json).collect()),
        Value::Object(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k.clone(), value_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
        _ => serde_json::Value::Null,
    }
}

fn json_to_value(json: serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(arr) => Value::Array(arr.into_iter().map(json_to_value).collect()),
        serde_json::Value::Object(map) => {
            let mut obj = IndexMap::new();
            for (k, v) in map {
                obj.insert(k, json_to_value(v));
            }
            Value::Object(obj)
        }
    }
}

fn jwt_sign(args: Vec<Value>) -> Result<Value, String> {
    let claims_val = match args.first() {
        Some(Value::Object(_)) => args.first().unwrap(),
        _ => return Err("jwt.sign() requires an object as first argument (claims)".to_string()),
    };
    let secret = match args.get(1) {
        Some(Value::String(s)) => s.clone(),
        _ => return Err("jwt.sign() requires a string as second argument (secret)".to_string()),
    };

    let mut algorithm = Algorithm::HS256;
    let mut claims_json = match value_to_json(claims_val) {
        serde_json::Value::Object(map) => map,
        _ => return Err("jwt.sign() claims must be an object".to_string()),
    };

    // Process options (third argument, optional)
    if let Some(Value::Object(opts)) = args.get(2) {
        if let Some(Value::String(alg_str)) = opts.get("algorithm") {
            algorithm = parse_algorithm(alg_str)?;
        }
        if let Some(Value::String(exp_str)) = opts.get("expires") {
            let secs = parse_duration(exp_str)?;
            let now = chrono::Utc::now().timestamp();
            claims_json.insert(
                "exp".to_string(),
                serde_json::Value::Number(serde_json::Number::from(now + secs)),
            );
        }
        if let Some(Value::String(iss)) = opts.get("issuer") {
            claims_json.insert("iss".to_string(), serde_json::Value::String(iss.clone()));
        }
        if let Some(Value::String(aud)) = opts.get("audience") {
            claims_json.insert("aud".to_string(), serde_json::Value::String(aud.clone()));
        }
        if let Some(Value::String(sub)) = opts.get("subject") {
            claims_json.insert("sub".to_string(), serde_json::Value::String(sub.clone()));
        }
        if let Some(Value::String(nbf_str)) = opts.get("not_before") {
            let secs = parse_duration(nbf_str)?;
            let now = chrono::Utc::now().timestamp();
            claims_json.insert(
                "nbf".to_string(),
                serde_json::Value::Number(serde_json::Number::from(now + secs)),
            );
        }
    }

    // Add iat (issued at) if not present
    if !claims_json.contains_key("iat") {
        let now = chrono::Utc::now().timestamp();
        claims_json.insert(
            "iat".to_string(),
            serde_json::Value::Number(serde_json::Number::from(now)),
        );
    }

    let header = Header::new(algorithm);

    // Convert claims to BTreeMap for jsonwebtoken
    let claims_btree: BTreeMap<String, serde_json::Value> = claims_json.into_iter().collect();

    let key = match algorithm {
        Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 => {
            EncodingKey::from_rsa_pem(secret.as_bytes())
                .map_err(|e| format!("invalid RSA PEM key: {}", e))?
        }
        Algorithm::ES256 | Algorithm::ES384 => EncodingKey::from_ec_pem(secret.as_bytes())
            .map_err(|e| format!("invalid EC PEM key: {}", e))?,
        _ => EncodingKey::from_secret(secret.as_bytes()),
    };

    let token =
        encode(&header, &claims_btree, &key).map_err(|e| format!("jwt.sign error: {}", e))?;

    Ok(Value::String(token))
}

fn jwt_verify(args: Vec<Value>) -> Result<Value, String> {
    let token = match args.first() {
        Some(Value::String(s)) => s.clone(),
        _ => return Err("jwt.verify() requires a string as first argument (token)".to_string()),
    };
    let secret = match args.get(1) {
        Some(Value::String(s)) => s.clone(),
        _ => return Err("jwt.verify() requires a string as second argument (secret)".to_string()),
    };

    // Peek at header to determine algorithm
    let header =
        jsonwebtoken::decode_header(&token).map_err(|e| format!("JWT decode error: {}", e))?;

    if header.alg == Algorithm::default() {
        // Check for alg:none attempt
    }

    let alg = header.alg;
    let key = match alg {
        Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 => {
            DecodingKey::from_rsa_pem(secret.as_bytes())
                .map_err(|e| format!("invalid RSA PEM key: {}", e))?
        }
        Algorithm::ES256 | Algorithm::ES384 => DecodingKey::from_ec_pem(secret.as_bytes())
            .map_err(|e| format!("invalid EC PEM key: {}", e))?,
        _ => DecodingKey::from_secret(secret.as_bytes()),
    };

    let mut validation = Validation::new(alg);
    validation.validate_exp = true;
    validation.validate_aud = false;
    validation.leeway = 0;
    // Don't require specific aud/iss — just validate exp + signature
    validation.set_required_spec_claims::<String>(&[]);

    let token_data = decode::<BTreeMap<String, serde_json::Value>>(&token, &key, &validation)
        .map_err(|e| match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => "JWT expired".to_string(),
            jsonwebtoken::errors::ErrorKind::InvalidSignature => {
                "JWT signature verification failed".to_string()
            }
            jsonwebtoken::errors::ErrorKind::InvalidToken => "JWT invalid token format".to_string(),
            jsonwebtoken::errors::ErrorKind::InvalidAlgorithm => {
                "JWT algorithm mismatch".to_string()
            }
            _ => format!("JWT verification error: {}", e),
        })?;

    let json_obj: serde_json::Map<String, serde_json::Value> =
        token_data.claims.into_iter().collect();
    Ok(json_to_value(serde_json::Value::Object(json_obj)))
}

fn jwt_decode(args: Vec<Value>) -> Result<Value, String> {
    let token = match args.first() {
        Some(Value::String(s)) => s.clone(),
        _ => return Err("jwt.decode() requires a string as first argument (token)".to_string()),
    };

    // Split token into parts
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("jwt.decode() invalid token format (expected 3 parts)".to_string());
    }

    use base64::Engine;
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;

    // Decode header
    let header_bytes = b64
        .decode(parts[0])
        .map_err(|e| format!("jwt.decode() invalid header encoding: {}", e))?;
    let header_json: serde_json::Value = serde_json::from_slice(&header_bytes)
        .map_err(|e| format!("jwt.decode() invalid header JSON: {}", e))?;

    // Decode payload
    let payload_bytes = b64
        .decode(parts[1])
        .map_err(|e| format!("jwt.decode() invalid payload encoding: {}", e))?;
    let payload_json: serde_json::Value = serde_json::from_slice(&payload_bytes)
        .map_err(|e| format!("jwt.decode() invalid payload JSON: {}", e))?;

    let mut result = IndexMap::new();
    result.insert("header".to_string(), json_to_value(header_json));
    result.insert("payload".to_string(), json_to_value(payload_json));

    Ok(Value::Object(result))
}

fn jwt_valid(args: Vec<Value>) -> Result<Value, String> {
    match jwt_verify(args) {
        Ok(_) => Ok(Value::Bool(true)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_claims() -> Value {
        let mut m = IndexMap::new();
        m.insert("user_id".to_string(), Value::Int(123));
        m.insert("role".to_string(), Value::String("admin".to_string()));
        Value::Object(m)
    }

    #[test]
    fn test_sign_basic() {
        let token = jwt_sign(vec![make_claims(), Value::String("secret".to_string())]).unwrap();
        if let Value::String(t) = &token {
            assert_eq!(t.split('.').count(), 3, "JWT should have 3 parts");
        } else {
            panic!("expected string token");
        }
    }

    #[test]
    fn test_sign_and_verify() {
        let secret = Value::String("my-secret-key".to_string());
        let token = jwt_sign(vec![make_claims(), secret.clone()]).unwrap();
        let claims = jwt_verify(vec![token, secret]).unwrap();
        if let Value::Object(map) = claims {
            assert_eq!(map.get("user_id"), Some(&Value::Int(123)));
            assert_eq!(map.get("role"), Some(&Value::String("admin".to_string())));
        } else {
            panic!("expected object claims");
        }
    }

    #[test]
    fn test_verify_wrong_secret() {
        let token = jwt_sign(vec![
            make_claims(),
            Value::String("correct-secret".to_string()),
        ])
        .unwrap();
        let result = jwt_verify(vec![token, Value::String("wrong-secret".to_string())]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("signature"));
    }

    #[test]
    fn test_verify_expired() {
        // Create claims with exp set to 1 second in the past
        let mut claims = IndexMap::new();
        claims.insert("user_id".to_string(), Value::Int(123));
        let past = chrono::Utc::now().timestamp() - 10;
        claims.insert("exp".to_string(), Value::Int(past));
        let token = jwt_sign(vec![
            Value::Object(claims),
            Value::String("secret".to_string()),
        ])
        .unwrap();
        let result = jwt_verify(vec![token, Value::String("secret".to_string())]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expired"));
    }

    #[test]
    fn test_decode_no_verification() {
        let token = jwt_sign(vec![make_claims(), Value::String("secret".to_string())]).unwrap();
        let decoded = jwt_decode(vec![token]).unwrap();
        if let Value::Object(map) = decoded {
            assert!(map.contains_key("header"));
            assert!(map.contains_key("payload"));
            if let Some(Value::Object(header)) = map.get("header") {
                assert_eq!(header.get("alg"), Some(&Value::String("HS256".to_string())));
            }
            if let Some(Value::Object(payload)) = map.get("payload") {
                assert_eq!(payload.get("user_id"), Some(&Value::Int(123)));
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_valid_returns_bool() {
        let secret = Value::String("secret".to_string());
        let token = jwt_sign(vec![make_claims(), secret.clone()]).unwrap();
        assert_eq!(
            jwt_valid(vec![token.clone(), secret]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            jwt_valid(vec![token, Value::String("wrong".to_string())]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_sign_with_options() {
        let mut opts = IndexMap::new();
        opts.insert("expires".to_string(), Value::String("1h".to_string()));
        opts.insert("issuer".to_string(), Value::String("myapp".to_string()));
        opts.insert("audience".to_string(), Value::String("users".to_string()));
        opts.insert("subject".to_string(), Value::String("user-123".to_string()));

        let secret = Value::String("secret".to_string());
        let token = jwt_sign(vec![make_claims(), secret.clone(), Value::Object(opts)]).unwrap();
        let claims = jwt_verify(vec![token, secret]).unwrap();
        if let Value::Object(map) = claims {
            assert_eq!(map.get("iss"), Some(&Value::String("myapp".to_string())));
            assert_eq!(map.get("sub"), Some(&Value::String("user-123".to_string())));
            assert!(map.contains_key("exp"));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_sign_hs384() {
        let mut opts = IndexMap::new();
        opts.insert("algorithm".to_string(), Value::String("HS384".to_string()));
        let secret = Value::String("secret".to_string());
        let token = jwt_sign(vec![make_claims(), secret.clone(), Value::Object(opts)]).unwrap();
        let decoded = jwt_decode(vec![token]).unwrap();
        if let Value::Object(map) = decoded {
            if let Some(Value::Object(header)) = map.get("header") {
                assert_eq!(header.get("alg"), Some(&Value::String("HS384".to_string())));
            }
        }
    }

    #[test]
    fn test_sign_hs512() {
        let mut opts = IndexMap::new();
        opts.insert("algorithm".to_string(), Value::String("HS512".to_string()));
        let secret = Value::String("secret".to_string());
        let token = jwt_sign(vec![make_claims(), secret.clone(), Value::Object(opts)]).unwrap();
        let decoded = jwt_decode(vec![token]).unwrap();
        if let Value::Object(map) = decoded {
            if let Some(Value::Object(header)) = map.get("header") {
                assert_eq!(header.get("alg"), Some(&Value::String("HS512".to_string())));
            }
        }
    }

    #[test]
    fn test_duration_parsing() {
        assert_eq!(parse_duration("60s").unwrap(), 60);
        assert_eq!(parse_duration("30m").unwrap(), 1800);
        assert_eq!(parse_duration("1h").unwrap(), 3600);
        assert_eq!(parse_duration("7d").unwrap(), 604800);
        assert_eq!(parse_duration("1w").unwrap(), 604800);
        assert_eq!(parse_duration("365d").unwrap(), 31536000);
    }

    #[test]
    fn test_reject_alg_none() {
        let result = parse_algorithm("none");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not allowed"));
    }

    #[test]
    fn test_nested_object_claims() {
        let mut user = IndexMap::new();
        user.insert("id".to_string(), Value::Int(1));
        user.insert(
            "roles".to_string(),
            Value::Array(vec![Value::String("admin".to_string())]),
        );
        let mut claims = IndexMap::new();
        claims.insert("user".to_string(), Value::Object(user));

        let secret = Value::String("secret".to_string());
        let token = jwt_sign(vec![Value::Object(claims), secret.clone()]).unwrap();
        let result = jwt_verify(vec![token, secret]).unwrap();
        if let Value::Object(map) = result {
            if let Some(Value::Object(user)) = map.get("user") {
                assert_eq!(user.get("id"), Some(&Value::Int(1)));
            } else {
                panic!("expected user object");
            }
        }
    }

    #[test]
    fn test_large_payload() {
        let mut claims = IndexMap::new();
        for i in 0..50 {
            claims.insert(format!("field_{}", i), Value::Int(i));
        }
        let secret = Value::String("secret".to_string());
        let token = jwt_sign(vec![Value::Object(claims), secret.clone()]).unwrap();
        let result = jwt_verify(vec![token, secret]).unwrap();
        if let Value::Object(map) = result {
            assert_eq!(map.get("field_0"), Some(&Value::Int(0)));
            assert_eq!(map.get("field_49"), Some(&Value::Int(49)));
        }
    }

    #[test]
    fn test_create_module() {
        let module = create_module();
        if let Value::Object(m) = module {
            assert!(m.contains_key("sign"));
            assert!(m.contains_key("verify"));
            assert!(m.contains_key("decode"));
            assert!(m.contains_key("valid"));
            assert_eq!(m.len(), 4);
        } else {
            panic!("expected object module");
        }
    }
}
