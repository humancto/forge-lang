use crate::interpreter::Value;
use indexmap::IndexMap;
use std::io::Write;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    // Colors
    m.insert("red".to_string(), Value::BuiltIn("term.red".to_string()));
    m.insert(
        "green".to_string(),
        Value::BuiltIn("term.green".to_string()),
    );
    m.insert("blue".to_string(), Value::BuiltIn("term.blue".to_string()));
    m.insert(
        "yellow".to_string(),
        Value::BuiltIn("term.yellow".to_string()),
    );
    m.insert("cyan".to_string(), Value::BuiltIn("term.cyan".to_string()));
    m.insert(
        "magenta".to_string(),
        Value::BuiltIn("term.magenta".to_string()),
    );
    m.insert("bold".to_string(), Value::BuiltIn("term.bold".to_string()));
    m.insert("dim".to_string(), Value::BuiltIn("term.dim".to_string()));
    // Display
    m.insert(
        "table".to_string(),
        Value::BuiltIn("term.table".to_string()),
    );
    m.insert("hr".to_string(), Value::BuiltIn("term.hr".to_string()));
    m.insert(
        "clear".to_string(),
        Value::BuiltIn("term.clear".to_string()),
    );
    m.insert(
        "spinner".to_string(),
        Value::BuiltIn("term.spinner".to_string()),
    );
    m.insert(
        "confirm".to_string(),
        Value::BuiltIn("term.confirm".to_string()),
    );
    m.insert(
        "sparkline".to_string(),
        Value::BuiltIn("term.sparkline".to_string()),
    );
    m.insert("bar".to_string(), Value::BuiltIn("term.bar".to_string()));
    m.insert(
        "banner".to_string(),
        Value::BuiltIn("term.banner".to_string()),
    );
    m.insert(
        "countdown".to_string(),
        Value::BuiltIn("term.countdown".to_string()),
    );
    m.insert(
        "emojis".to_string(),
        Value::BuiltIn("term.emojis".to_string()),
    );
    m.insert("box".to_string(), Value::BuiltIn("term.box".to_string()));
    m.insert(
        "typewriter".to_string(),
        Value::BuiltIn("term.typewriter".to_string()),
    );
    m.insert("menu".to_string(), Value::BuiltIn("term.menu".to_string()));
    m.insert("beep".to_string(), Value::BuiltIn("term.beep".to_string()));
    m.insert(
        "emoji".to_string(),
        Value::BuiltIn("term.emoji".to_string()),
    );
    m.insert(
        "gradient".to_string(),
        Value::BuiltIn("term.gradient".to_string()),
    );
    m.insert(
        "success".to_string(),
        Value::BuiltIn("term.success".to_string()),
    );
    m.insert(
        "error".to_string(),
        Value::BuiltIn("term.error".to_string()),
    );
    m.insert(
        "warning".to_string(),
        Value::BuiltIn("term.warning".to_string()),
    );
    m.insert("info".to_string(), Value::BuiltIn("term.info".to_string()));
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "term.red" => color_wrap(args, "31"),
        "term.green" => color_wrap(args, "32"),
        "term.yellow" => color_wrap(args, "33"),
        "term.blue" => color_wrap(args, "34"),
        "term.magenta" => color_wrap(args, "35"),
        "term.cyan" => color_wrap(args, "36"),
        "term.bold" => color_wrap(args, "1"),
        "term.dim" => color_wrap(args, "2"),

        "term.table" => {
            match args.first() {
                Some(Value::Array(rows)) => {
                    if rows.is_empty() {
                        return Ok(Value::Null);
                    }
                    // Extract headers from first row
                    let headers: Vec<String> = if let Some(Value::Object(first)) = rows.first() {
                        first.keys().cloned().collect()
                    } else {
                        return Err("term.table() requires array of objects".to_string());
                    };

                    // Calculate column widths
                    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
                    for row in rows {
                        if let Value::Object(map) = row {
                            for (i, header) in headers.iter().enumerate() {
                                let val_len =
                                    map.get(header).map(|v| format!("{}", v).len()).unwrap_or(0);
                                if val_len > widths[i] {
                                    widths[i] = val_len;
                                }
                            }
                        }
                    }

                    // Print header
                    let header_line: Vec<String> = headers
                        .iter()
                        .enumerate()
                        .map(|(i, h)| format!(" {:<width$} ", h, width = widths[i]))
                        .collect();
                    crate::color::ceprintln!("\x1B[1m{}\x1B[0m", header_line.join("|"));

                    // Print separator
                    let sep: Vec<String> = widths.iter().map(|w| "-".repeat(w + 2)).collect();
                    crate::color::ceprintln!("{}", sep.join("+"));

                    // Print rows
                    for row in rows {
                        if let Value::Object(map) = row {
                            let cells: Vec<String> = headers
                                .iter()
                                .enumerate()
                                .map(|(i, h)| {
                                    let val =
                                        map.get(h).map(|v| format!("{}", v)).unwrap_or_default();
                                    format!(" {:<width$} ", val, width = widths[i])
                                })
                                .collect();
                            crate::color::ceprintln!("{}", cells.join("|"));
                        }
                    }
                    Ok(Value::Null)
                }
                _ => Err("term.table() requires an array of objects".to_string()),
            }
        }

        "term.hr" => {
            let width = args
                .first()
                .and_then(|v| {
                    if let Value::Int(n) = v {
                        Some(*n as usize)
                    } else {
                        None
                    }
                })
                .unwrap_or(40);
            let ch = args
                .get(1)
                .and_then(|v| {
                    if let Value::String(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "─".to_string());
            crate::color::ceprintln!("{}", ch.repeat(width));
            Ok(Value::Null)
        }

        "term.clear" => {
            crate::color::ceprint!("\x1B[2J\x1B[1;1H");
            Ok(Value::Null)
        }

        "term.spinner" => {
            let msg = args
                .first()
                .map(|v| format!("{}", v))
                .unwrap_or_else(|| "Loading...".to_string());
            crate::color::ceprint!("\x1B[2K\r{} ⠋", msg);
            Ok(Value::Null)
        }

        "term.confirm" => {
            // Like input()/io.prompt: without `process` the host's stdin is
            // an empty stream (no prompt, no read), so a sandboxed script can
            // never block on or consume it.
            if !crate::permissions::host_stdin_allowed() {
                return Ok(Value::Bool(false));
            }
            let prompt = args
                .first()
                .map(|v| format!("{}", v))
                .unwrap_or_else(|| "Continue?".to_string());
            crate::color::ceprint!("{} [y/N] ", prompt);
            use std::io::Write;
            std::io::stderr().flush().ok();
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).ok();
            let answer = input.trim().to_lowercase();
            Ok(Value::Bool(answer == "y" || answer == "yes"))
        }

        "term.sparkline" => match args.first() {
            Some(Value::Array(nums)) => {
                let bars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
                let values: Vec<f64> = nums
                    .iter()
                    .map(|v| match v {
                        Value::Int(n) => *n as f64,
                        Value::Float(n) => *n,
                        _ => 0.0,
                    })
                    .collect();
                let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let range = if (max - min).abs() < 0.001 {
                    1.0
                } else {
                    max - min
                };
                let sparkline: String = values
                    .iter()
                    .map(|v| {
                        let idx = (((v - min) / range) * 7.0) as usize;
                        bars[idx.min(7)]
                    })
                    .collect();
                Ok(Value::String(sparkline))
            }
            _ => Err("term.sparkline() requires an array of numbers".to_string()),
        },

        "term.bar" => {
            let label = args.first().map(|v| format!("{}", v)).unwrap_or_default();
            let value = match args.get(1) {
                Some(Value::Int(n)) => *n as f64,
                Some(Value::Float(n)) => *n,
                _ => 0.0,
            };
            let max = match args.get(2) {
                Some(Value::Int(n)) => *n as f64,
                Some(Value::Float(n)) => *n,
                _ => 100.0,
            };
            let width = 30;
            let filled = ((value / max) * width as f64) as usize;
            let bar = format!(
                "{} [{}{}] {:.0}%",
                label,
                "█".repeat(filled),
                "░".repeat(width - filled),
                (value / max) * 100.0,
            );
            crate::color::ceprintln!("  {}", bar);
            Ok(Value::Null)
        }

        "term.banner" => {
            let text = args.first().map(|v| format!("{}", v)).unwrap_or_default();
            let width = text.len() + 4;
            let border = "═".repeat(width);
            crate::color::ceprintln!("╔{}╗", border);
            crate::color::ceprintln!("║  {}  ║", text);
            crate::color::ceprintln!("╚{}╝", border);
            Ok(Value::Null)
        }

        "term.countdown" => {
            let secs = match args.first() {
                Some(Value::Int(n)) => *n as u64,
                _ => 3,
            };
            for i in (1..=secs).rev() {
                crate::color::ceprint!("\r  {} ", i);
                std::io::stderr().flush().ok();
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            crate::color::ceprintln!("\r  Go! 🚀");
            Ok(Value::Null)
        }

        "term.box" => {
            let text = args.first().map(|v| format!("{}", v)).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            let max_width = lines.iter().map(|l| l.len()).max().unwrap_or(0);
            let border = "─".repeat(max_width + 2);
            crate::color::ceprintln!("┌{}┐", border);
            for line in &lines {
                crate::color::ceprintln!("│ {:<width$} │", line, width = max_width);
            }
            crate::color::ceprintln!("└{}┘", border);
            Ok(Value::Null)
        }

        "term.typewriter" => {
            let text = args.first().map(|v| format!("{}", v)).unwrap_or_default();
            let delay = args
                .get(1)
                .and_then(|v| {
                    if let Value::Int(n) = v {
                        Some(*n as u64)
                    } else {
                        None
                    }
                })
                .unwrap_or(30);
            for ch in text.chars() {
                crate::color::ceprint!("{}", ch);
                std::io::stderr().flush().ok();
                std::thread::sleep(std::time::Duration::from_millis(delay));
            }
            crate::color::ceprintln!();
            Ok(Value::Null)
        }

        "term.menu" => match args.first() {
            Some(Value::Array(_)) if !crate::permissions::host_stdin_allowed() => Ok(Value::Null),
            Some(Value::Array(options)) => {
                let prompt = args
                    .get(1)
                    .map(|v| format!("{}", v))
                    .unwrap_or_else(|| "Choose an option:".to_string());
                crate::color::ceprintln!("\n  {}", prompt);
                for (i, opt) in options.iter().enumerate() {
                    crate::color::ceprintln!("  \x1B[36m{})\x1B[0m {}", i + 1, opt);
                }
                crate::color::ceprint!("\n  Your choice: ");
                std::io::stderr().flush().ok();
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).ok();
                let choice: usize = input.trim().parse().unwrap_or(0);
                if choice > 0 && choice <= options.len() {
                    Ok(options[choice - 1].clone())
                } else {
                    Ok(Value::Null)
                }
            }
            _ => Err("term.menu() requires an array of options".to_string()),
        },

        "term.beep" => {
            crate::color::ceprint!("\x07");
            std::io::stderr().flush().ok();
            Ok(Value::Null)
        }

        "term.emojis" => {
            let emojis = [
                ("check", "✅", "also: ok, yes"),
                ("cross", "❌", "also: no, fail"),
                ("star", "⭐", "also: fav"),
                ("fire", "🔥", "also: hot"),
                ("heart", "❤️", "also: love"),
                ("rocket", "🚀", "also: launch"),
                ("warn", "⚠️", "also: warning"),
                ("info", "ℹ️", "also: information"),
                ("bug", "🐛", "also: error"),
                ("clock", "⏰", "also: time"),
                ("folder", "📁", "also: dir"),
                ("file", "📄", "also: doc"),
                ("lock", "🔒", "also: secure"),
                ("key", "🔑", ""),
                ("link", "🔗", "also: url"),
                ("mail", "📧", "also: email"),
                ("globe", "🌍", "also: web, world"),
                ("party", "🎉", "also: celebrate"),
                ("think", "🤔", "also: hmm"),
                ("wave", "👋", "also: hi, hello"),
                ("thumbsup", "👍", "also: good"),
                ("thumbsdown", "👎", "also: bad"),
                ("100", "💯", "also: perfect"),
                ("zap", "⚡", "also: bolt, fast"),
                ("gear", "⚙️", "also: settings"),
                ("tools", "🔧", "also: wrench"),
            ];
            crate::color::ceprintln!();
            crate::color::ceprintln!("  \x1B[1mAvailable Emojis:\x1B[0m  term.emoji(\"name\")");
            crate::color::ceprintln!("  {}", "─".repeat(45));
            for (name, emoji, aliases) in &emojis {
                if aliases.is_empty() {
                    crate::color::ceprintln!("  {}  {:<12}", emoji, name);
                } else {
                    crate::color::ceprintln!(
                        "  {}  {:<12} \x1B[90m{}\x1B[0m",
                        emoji,
                        name,
                        aliases
                    );
                }
            }
            crate::color::ceprintln!();
            Ok(Value::Null)
        }

        "term.emoji" => {
            let name = args.first().map(|v| format!("{}", v)).unwrap_or_default();
            let emoji = match name.as_str() {
                "check" | "ok" | "yes" => "✅",
                "cross" | "no" | "fail" => "❌",
                "star" | "fav" => "⭐",
                "fire" | "hot" => "🔥",
                "heart" | "love" => "❤️",
                "rocket" | "launch" => "🚀",
                "warn" | "warning" => "⚠️",
                "info" | "information" => "ℹ️",
                "bug" | "error" => "🐛",
                "clock" | "time" => "⏰",
                "folder" | "dir" => "📁",
                "file" | "doc" => "📄",
                "lock" | "secure" => "🔒",
                "key" => "🔑",
                "link" | "url" => "🔗",
                "mail" | "email" => "📧",
                "globe" | "web" | "world" => "🌍",
                "party" | "celebrate" => "🎉",
                "think" | "hmm" => "🤔",
                "wave" | "hi" | "hello" => "👋",
                "thumbsup" | "good" => "👍",
                "thumbsdown" | "bad" => "👎",
                "100" | "perfect" => "💯",
                "zap" | "bolt" | "fast" => "⚡",
                "gear" | "settings" => "⚙️",
                "tools" | "wrench" => "🔧",
                _ => "❓",
            };
            Ok(Value::String(emoji.to_string()))
        }

        "term.gradient" => {
            let text = args.first().map(|v| format!("{}", v)).unwrap_or_default();
            let colors = [196, 202, 208, 214, 220, 226, 190, 154, 118, 82, 46];
            let mut result = String::new();
            for (i, ch) in text.chars().enumerate() {
                let color = colors[i % colors.len()];
                result.push_str(&format!("\x1B[38;5;{}m{}", color, ch));
            }
            result.push_str("\x1B[0m");
            Ok(Value::String(result))
        }

        "term.success" => {
            let msg = args.first().map(|v| format!("{}", v)).unwrap_or_default();
            crate::color::ceprintln!("  \x1B[32m✅ {}\x1B[0m", msg);
            Ok(Value::Null)
        }

        "term.error" => {
            let msg = args.first().map(|v| format!("{}", v)).unwrap_or_default();
            crate::color::ceprintln!("  \x1B[31m❌ {}\x1B[0m", msg);
            Ok(Value::Null)
        }

        "term.warning" => {
            let msg = args.first().map(|v| format!("{}", v)).unwrap_or_default();
            crate::color::ceprintln!("  \x1B[33m⚠️  {}\x1B[0m", msg);
            Ok(Value::Null)
        }

        "term.info" => {
            let msg = args.first().map(|v| format!("{}", v)).unwrap_or_default();
            crate::color::ceprintln!("  \x1B[36mℹ️  {}\x1B[0m", msg);
            Ok(Value::Null)
        }

        _ => Err(format!("unknown term function: {}", name)),
    }
}

fn color_wrap(args: Vec<Value>, code: &str) -> Result<Value, String> {
    let text = args.first().map(|v| format!("{}", v)).unwrap_or_default();
    Ok(Value::String(format!("\x1B[{}m{}\x1B[0m", code, text)))
}
