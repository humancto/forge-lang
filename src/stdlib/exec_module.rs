use crate::interpreter::Value;
use indexmap::IndexMap;

/// Safely execute a command with arguments split from a command string.
/// Uses std::process::Command which does NOT invoke a shell,
/// preventing command injection.
pub fn call(args: Vec<Value>) -> Result<Value, String> {
    let cmd_str = match args.first() {
        Some(Value::String(s)) => s.clone(),
        _ => return Err("run_command() requires a command string".to_string()),
    };

    let parts: Vec<&str> = cmd_str.split_whitespace().collect();
    if parts.is_empty() {
        return Err("run_command() requires a non-empty command".to_string());
    }

    let program = parts[0];
    let cmd_args = &parts[1..];

    let output = std::process::Command::new(program)
        .args(cmd_args)
        .output()
        .map_err(|e| format!("command error: {}", e))?;

    let mut result = IndexMap::new();
    result.insert(
        "stdout".to_string(),
        Value::String(
            String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string(),
        ),
    );
    result.insert(
        "stderr".to_string(),
        Value::String(
            String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_string(),
        ),
    );
    result.insert(
        "status".to_string(),
        Value::Int(output.status.code().unwrap_or(-1) as i64),
    );
    result.insert("ok".to_string(), Value::Bool(output.status.success()));

    Ok(Value::Object(result))
}
