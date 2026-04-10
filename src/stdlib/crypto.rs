use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert(
        "sha256".to_string(),
        Value::BuiltIn("crypto.sha256".to_string()),
    );
    m.insert("md5".to_string(), Value::BuiltIn("crypto.md5".to_string()));
    m.insert(
        "base64_encode".to_string(),
        Value::BuiltIn("crypto.base64_encode".to_string()),
    );
    m.insert(
        "base64_decode".to_string(),
        Value::BuiltIn("crypto.base64_decode".to_string()),
    );
    m.insert(
        "hex_encode".to_string(),
        Value::BuiltIn("crypto.hex_encode".to_string()),
    );
    m.insert(
        "hex_decode".to_string(),
        Value::BuiltIn("crypto.hex_decode".to_string()),
    );
    m.insert(
        "hmac_sha256".to_string(),
        Value::BuiltIn("crypto.hmac_sha256".to_string()),
    );
    m.insert(
        "sha512".to_string(),
        Value::BuiltIn("crypto.sha512".to_string()),
    );
    m.insert(
        "random_bytes".to_string(),
        Value::BuiltIn("crypto.random_bytes".to_string()),
    );
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "crypto.sha256" => match args.first() {
            Some(Value::String(s)) => {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(s.as_bytes());
                let result = hasher.finalize();
                Ok(Value::String(hex::encode(result)))
            }
            _ => Err("crypto.sha256() requires a string".to_string()),
        },
        "crypto.md5" => match args.first() {
            Some(Value::String(s)) => {
                use md5::{Digest, Md5};
                let mut hasher = Md5::new();
                hasher.update(s.as_bytes());
                let result = hasher.finalize();
                Ok(Value::String(hex::encode(result)))
            }
            _ => Err("crypto.md5() requires a string".to_string()),
        },
        "crypto.base64_encode" => match args.first() {
            Some(Value::String(s)) => {
                use base64::Engine;
                Ok(Value::String(
                    base64::engine::general_purpose::STANDARD.encode(s.as_bytes()),
                ))
            }
            _ => Err("crypto.base64_encode() requires a string".to_string()),
        },
        "crypto.base64_decode" => match args.first() {
            Some(Value::String(s)) => {
                use base64::Engine;
                match base64::engine::general_purpose::STANDARD.decode(s.as_bytes()) {
                    Ok(bytes) => Ok(Value::String(String::from_utf8_lossy(&bytes).to_string())),
                    Err(e) => Err(format!("base64 decode error: {}", e)),
                }
            }
            _ => Err("crypto.base64_decode() requires a string".to_string()),
        },
        "crypto.hex_encode" => match args.first() {
            Some(Value::String(s)) => Ok(Value::String(hex::encode(s.as_bytes()))),
            _ => Err("crypto.hex_encode() requires a string".to_string()),
        },
        "crypto.hex_decode" => match args.first() {
            Some(Value::String(s)) => match hex::decode(s) {
                Ok(bytes) => Ok(Value::String(String::from_utf8_lossy(&bytes).to_string())),
                Err(e) => Err(format!("hex decode error: {}", e)),
            },
            _ => Err("crypto.hex_decode() requires a string".to_string()),
        },
        "crypto.hmac_sha256" => match (args.first(), args.get(1)) {
            (Some(Value::String(message)), Some(Value::String(key))) => {
                use hmac::{Hmac, Mac};
                use sha2::Sha256;
                type HmacSha256 = Hmac<Sha256>;
                let mut mac =
                    HmacSha256::new_from_slice(key.as_bytes()).map_err(|e| format!("{}", e))?;
                mac.update(message.as_bytes());
                let result = mac.finalize();
                Ok(Value::String(hex::encode(result.into_bytes())))
            }
            _ => Err("crypto.hmac_sha256() requires (message, key) strings".to_string()),
        },
        "crypto.sha512" => match args.first() {
            Some(Value::String(s)) => {
                use sha2::{Digest, Sha512};
                let mut hasher = Sha512::new();
                hasher.update(s.as_bytes());
                let result = hasher.finalize();
                Ok(Value::String(hex::encode(result)))
            }
            _ => Err("crypto.sha512() requires a string".to_string()),
        },
        "crypto.random_bytes" => match args.first() {
            Some(Value::Int(n)) => {
                let n = *n as usize;
                let mut bytes = vec![0u8; n];
                for byte in &mut bytes {
                    *byte = rand_byte();
                }
                Ok(Value::String(hex::encode(&bytes)))
            }
            _ => Err("crypto.random_bytes() requires an integer length".to_string()),
        },
        _ => Err(format!("unknown crypto function: {}", name)),
    }
}

fn rand_byte() -> u8 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    (nanos ^ (nanos >> 8) ^ (nanos >> 16)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Value {
        Value::String(v.to_string())
    }

    #[test]
    fn module_has_all_functions() {
        if let Value::Object(m) = create_module() {
            for k in [
                "sha256",
                "md5",
                "base64_encode",
                "base64_decode",
                "hex_encode",
                "hex_decode",
                "hmac_sha256",
                "sha512",
                "random_bytes",
            ] {
                assert!(m.contains_key(k), "missing {}", k);
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn sha256_known_vector() {
        // Empty string SHA-256 is the canonical e3b0c4… digest.
        let result = call("crypto.sha256", vec![s("")]).unwrap();
        assert_eq!(
            result,
            s("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
    }

    #[test]
    fn sha256_abc() {
        let result = call("crypto.sha256", vec![s("abc")]).unwrap();
        assert_eq!(
            result,
            s("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
    }

    #[test]
    fn sha512_known_vector() {
        let result = call("crypto.sha512", vec![s("abc")]).unwrap();
        // Canonical SHA-512("abc")
        assert_eq!(
            result,
            s("ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f")
        );
    }

    #[test]
    fn md5_known_vector() {
        let result = call("crypto.md5", vec![s("abc")]).unwrap();
        assert_eq!(result, s("900150983cd24fb0d6963f7d28e17f72"));
    }

    #[test]
    fn base64_round_trip() {
        let encoded = call("crypto.base64_encode", vec![s("hello world")]).unwrap();
        assert_eq!(encoded, s("aGVsbG8gd29ybGQ="));
        let decoded = call("crypto.base64_decode", vec![encoded]).unwrap();
        assert_eq!(decoded, s("hello world"));
    }

    #[test]
    fn base64_decode_invalid_errors() {
        let result = call("crypto.base64_decode", vec![s("not!base64@@")]);
        assert!(result.is_err());
    }

    #[test]
    fn hex_round_trip() {
        let encoded = call("crypto.hex_encode", vec![s("abc")]).unwrap();
        assert_eq!(encoded, s("616263"));
        let decoded = call("crypto.hex_decode", vec![encoded]).unwrap();
        assert_eq!(decoded, s("abc"));
    }

    #[test]
    fn hex_decode_invalid_errors() {
        let result = call("crypto.hex_decode", vec![s("zzzz")]);
        assert!(result.is_err());
    }

    #[test]
    fn hmac_sha256_known_vector() {
        // RFC 4231 test case 1: key=0x0b*20, message="Hi There"
        // Expected: b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7
        let result = call(
            "crypto.hmac_sha256",
            vec![
                s("Hi There"),
                Value::String(String::from_utf8(vec![0x0b; 20]).unwrap()),
            ],
        )
        .unwrap();
        assert_eq!(
            result,
            s("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7")
        );
    }

    #[test]
    fn hmac_sha256_requires_two_strings() {
        let result = call("crypto.hmac_sha256", vec![s("only one")]);
        assert!(result.is_err());
    }

    #[test]
    fn random_bytes_returns_hex_of_correct_length() {
        let result = call("crypto.random_bytes", vec![Value::Int(16)]).unwrap();
        if let Value::String(s) = result {
            // 16 bytes = 32 hex chars
            assert_eq!(s.len(), 32);
            assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn random_bytes_zero() {
        let result = call("crypto.random_bytes", vec![Value::Int(0)]).unwrap();
        assert_eq!(result, s(""));
    }

    #[test]
    fn wrong_arg_types_error() {
        assert!(call("crypto.sha256", vec![Value::Int(1)]).is_err());
        assert!(call("crypto.md5", vec![]).is_err());
        assert!(call("crypto.base64_encode", vec![Value::Bool(true)]).is_err());
        assert!(call("crypto.random_bytes", vec![s("hi")]).is_err());
    }

    #[test]
    fn unknown_function_errors() {
        let result = call("crypto.bogus", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown crypto function"));
    }
}
