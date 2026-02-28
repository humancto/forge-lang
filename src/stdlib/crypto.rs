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
        _ => Err(format!("unknown crypto function: {}", name)),
    }
}
