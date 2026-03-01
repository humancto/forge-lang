use crate::interpreter::Value;
use indexmap::IndexMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("name".to_string(), Value::BuiltIn("npc.name".to_string()));
    m.insert(
        "first_name".to_string(),
        Value::BuiltIn("npc.first_name".to_string()),
    );
    m.insert(
        "last_name".to_string(),
        Value::BuiltIn("npc.last_name".to_string()),
    );
    m.insert("email".to_string(), Value::BuiltIn("npc.email".to_string()));
    m.insert(
        "username".to_string(),
        Value::BuiltIn("npc.username".to_string()),
    );
    m.insert(
        "number".to_string(),
        Value::BuiltIn("npc.number".to_string()),
    );
    m.insert("pick".to_string(), Value::BuiltIn("npc.pick".to_string()));
    m.insert(
        "sentence".to_string(),
        Value::BuiltIn("npc.sentence".to_string()),
    );
    m.insert("id".to_string(), Value::BuiltIn("npc.id".to_string()));
    m.insert("bool".to_string(), Value::BuiltIn("npc.bool".to_string()));
    m.insert("phone".to_string(), Value::BuiltIn("npc.phone".to_string()));
    m.insert("color".to_string(), Value::BuiltIn("npc.color".to_string()));
    m.insert("ip".to_string(), Value::BuiltIn("npc.ip".to_string()));
    m.insert("url".to_string(), Value::BuiltIn("npc.url".to_string()));
    m.insert(
        "company".to_string(),
        Value::BuiltIn("npc.company".to_string()),
    );
    m.insert("word".to_string(), Value::BuiltIn("npc.word".to_string()));
    Value::Object(m)
}

// Simple LCG random number generator using system time as seed
fn quick_rand() -> u64 {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    // Mix bits using xorshift-like approach
    let mut x = seed;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x.wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
}

fn rand_range(min: i64, max: i64) -> i64 {
    if min >= max {
        return min;
    }
    let range = (max - min + 1) as u64;
    (quick_rand() % range) as i64 + min
}

fn pick_from(items: &[&str]) -> String {
    let idx = (quick_rand() % items.len() as u64) as usize;
    items[idx].to_string()
}

const FIRST_NAMES: &[&str] = &[
    "Alex", "Jordan", "Taylor", "Morgan", "Casey", "Riley", "Quinn", "Avery", "Parker", "Sage",
    "River", "Phoenix", "Sky", "Luna", "Kai", "Zen", "Nova", "Aria", "Leo", "Maya", "Finn", "Ivy",
    "Milo", "Ruby", "Oscar", "Zara", "Atlas", "Cleo", "Hugo", "Jade", "Remy", "Wren",
];

const LAST_NAMES: &[&str] = &[
    "Smith",
    "Chen",
    "Patel",
    "Kim",
    "Singh",
    "Nakamura",
    "Garcia",
    "Mueller",
    "Costa",
    "Ali",
    "Park",
    "Tanaka",
    "Silva",
    "Johansson",
    "Okafor",
    "Dubois",
    "Ivanov",
    "Santos",
    "Lee",
    "Brown",
    "Williams",
    "Jones",
    "Davis",
    "Wilson",
    "Martinez",
    "Anderson",
    "Thomas",
    "Jackson",
    "White",
    "Harris",
    "Clark",
    "Lewis",
];

const ADJECTIVES: &[&str] = &[
    "cool", "epic", "turbo", "mega", "ultra", "hyper", "super", "cyber", "neon", "pixel",
    "quantum", "cosmic", "stellar", "blazing", "swift", "crisp",
];

const NOUNS: &[&str] = &[
    "coder", "hacker", "ninja", "wizard", "panda", "phoenix", "dragon", "falcon", "wolf", "tiger",
    "fox", "hawk", "viper", "shark", "raven", "lynx",
];

const WORDS: &[&str] = &[
    "the",
    "quick",
    "brown",
    "fox",
    "jumps",
    "over",
    "lazy",
    "dog",
    "code",
    "runs",
    "fast",
    "data",
    "flows",
    "through",
    "every",
    "node",
    "bits",
    "and",
    "bytes",
    "dance",
    "in",
    "silicon",
    "dreams",
    "while",
    "algorithms",
    "weave",
    "patterns",
    "of",
    "digital",
    "light",
    "across",
    "networks",
];

const COMPANIES: &[&str] = &[
    "TechFlow",
    "NeonByte",
    "PixelForge",
    "CloudNine",
    "DataPulse",
    "CodeCraft",
    "BitWave",
    "CyberCore",
    "QuantumLeap",
    "StarGrid",
    "ByteBloom",
    "NetVault",
    "SyncSphere",
    "HyperNode",
    "CoreStack",
    "MindMesh",
];

const DOMAINS: &[&str] = &[
    "gmail.com",
    "outlook.com",
    "proton.me",
    "hey.com",
    "icloud.com",
    "yahoo.com",
    "fastmail.com",
    "tutanota.com",
];

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "npc.first_name" => Ok(Value::String(pick_from(FIRST_NAMES))),

        "npc.last_name" => Ok(Value::String(pick_from(LAST_NAMES))),

        "npc.name" => {
            let first = pick_from(FIRST_NAMES);
            let last = pick_from(LAST_NAMES);
            Ok(Value::String(format!("{} {}", first, last)))
        }

        "npc.email" => {
            let first = pick_from(FIRST_NAMES).to_lowercase();
            let last = pick_from(LAST_NAMES).to_lowercase();
            let domain = pick_from(DOMAINS);
            let num = rand_range(1, 99);
            Ok(Value::String(format!(
                "{}.{}{:02}@{}",
                first, last, num, domain
            )))
        }

        "npc.username" => {
            let adj = pick_from(ADJECTIVES);
            let noun = pick_from(NOUNS);
            let num = rand_range(1, 999);
            Ok(Value::String(format!("{}_{}{}", adj, noun, num)))
        }

        "npc.number" => {
            let min = match args.first() {
                Some(Value::Int(n)) => *n,
                _ => 0,
            };
            let max = match args.get(1) {
                Some(Value::Int(n)) => *n,
                _ => 100,
            };
            Ok(Value::Int(rand_range(min, max)))
        }

        "npc.pick" => match args.first() {
            Some(Value::Array(items)) if !items.is_empty() => {
                let idx = (quick_rand() % items.len() as u64) as usize;
                Ok(items[idx].clone())
            }
            _ => Err("npc.pick() requires a non-empty array".to_string()),
        },

        "npc.sentence" => {
            let word_count = match args.first() {
                Some(Value::Int(n)) => *n as usize,
                _ => rand_range(5, 12) as usize,
            };
            let mut sentence: Vec<String> = (0..word_count).map(|_| pick_from(WORDS)).collect();
            if let Some(first) = sentence.first_mut() {
                let mut chars = first.chars();
                if let Some(c) = chars.next() {
                    *first = format!("{}{}", c.to_uppercase(), chars.as_str());
                }
            }
            let mut result = sentence.join(" ");
            result.push('.');
            Ok(Value::String(result))
        }

        "npc.id" => {
            let chars = "abcdefghijklmnopqrstuvwxyz0123456789";
            let chars: Vec<char> = chars.chars().collect();
            let segments = [8, 4, 4, 4, 12];
            let parts: Vec<String> = segments
                .iter()
                .map(|&len| {
                    (0..len)
                        .map(|_| {
                            let idx = (quick_rand() % chars.len() as u64) as usize;
                            chars[idx]
                        })
                        .collect::<String>()
                })
                .collect();
            Ok(Value::String(parts.join("-")))
        }

        "npc.bool" => Ok(Value::Bool(quick_rand() % 2 == 0)),

        "npc.phone" => {
            let area = rand_range(200, 999);
            let mid = rand_range(100, 999);
            let end = rand_range(1000, 9999);
            Ok(Value::String(format!("({}) {}-{}", area, mid, end)))
        }

        "npc.color" => {
            let r = rand_range(0, 255);
            let g = rand_range(0, 255);
            let b = rand_range(0, 255);
            Ok(Value::String(format!("#{:02x}{:02x}{:02x}", r, g, b)))
        }

        "npc.ip" => {
            let a = rand_range(1, 254);
            let b = rand_range(0, 255);
            let c = rand_range(0, 255);
            let d = rand_range(1, 254);
            Ok(Value::String(format!("{}.{}.{}.{}", a, b, c, d)))
        }

        "npc.url" => {
            let company = pick_from(COMPANIES).to_lowercase();
            let paths = ["api", "dashboard", "app", "docs", "status", "blog"];
            let path = pick_from(&paths);
            Ok(Value::String(format!("https://{}.io/{}", company, path)))
        }

        "npc.company" => Ok(Value::String(pick_from(COMPANIES))),

        "npc.word" => Ok(Value::String(pick_from(WORDS))),

        _ => Err(format!("unknown npc function: {}", name)),
    }
}

pub fn call_vm(name: &str, args: Vec<Value>) -> Result<Value, String> {
    call(name, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_npc_name() {
        let result = call("npc.name", vec![]).unwrap();
        if let Value::String(s) = result {
            assert!(s.contains(' '), "name should have space: {}", s);
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_npc_email() {
        let result = call("npc.email", vec![]).unwrap();
        if let Value::String(s) = result {
            assert!(s.contains('@'), "email should have @: {}", s);
            assert!(s.contains('.'), "email should have dot: {}", s);
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_npc_username() {
        let result = call("npc.username", vec![]).unwrap();
        if let Value::String(s) = result {
            assert!(s.contains('_'), "username should have underscore: {}", s);
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_npc_number_range() {
        let result = call("npc.number", vec![Value::Int(10), Value::Int(20)]).unwrap();
        if let Value::Int(n) = result {
            assert!(n >= 10 && n <= 20, "number {} out of range 10-20", n);
        } else {
            panic!("expected int");
        }
    }

    #[test]
    fn test_npc_pick() {
        let arr = Value::Array(vec![
            Value::String("a".to_string()),
            Value::String("b".to_string()),
            Value::String("c".to_string()),
        ]);
        let result = call("npc.pick", vec![arr]).unwrap();
        if let Value::String(s) = result {
            assert!(
                s == "a" || s == "b" || s == "c",
                "pick should return one of a/b/c: {}",
                s
            );
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_npc_id() {
        let result = call("npc.id", vec![]).unwrap();
        if let Value::String(s) = result {
            let parts: Vec<&str> = s.split('-').collect();
            assert_eq!(parts.len(), 5, "id should have 5 parts: {}", s);
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_npc_bool() {
        let result = call("npc.bool", vec![]).unwrap();
        assert!(matches!(result, Value::Bool(_)));
    }

    #[test]
    fn test_npc_sentence() {
        let result = call("npc.sentence", vec![Value::Int(5)]).unwrap();
        if let Value::String(s) = result {
            assert!(s.ends_with('.'), "sentence should end with period: {}", s);
            let words: Vec<&str> = s.trim_end_matches('.').split_whitespace().collect();
            assert_eq!(words.len(), 5, "should have 5 words: {}", s);
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_npc_phone() {
        let result = call("npc.phone", vec![]).unwrap();
        if let Value::String(s) = result {
            assert!(s.starts_with('('), "phone should start with (: {}", s);
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_npc_color() {
        let result = call("npc.color", vec![]).unwrap();
        if let Value::String(s) = result {
            assert!(s.starts_with('#'), "color should start with #: {}", s);
            assert_eq!(s.len(), 7, "color should be #rrggbb: {}", s);
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_npc_ip() {
        let result = call("npc.ip", vec![]).unwrap();
        if let Value::String(s) = result {
            let parts: Vec<&str> = s.split('.').collect();
            assert_eq!(parts.len(), 4, "IP should have 4 octets: {}", s);
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_npc_url() {
        let result = call("npc.url", vec![]).unwrap();
        if let Value::String(s) = result {
            assert!(
                s.starts_with("https://"),
                "url should start with https://: {}",
                s
            );
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_npc_company() {
        let result = call("npc.company", vec![]).unwrap();
        assert!(matches!(result, Value::String(_)));
    }
}
