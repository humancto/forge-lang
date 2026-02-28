use std::io::{self, Write};

pub fn run_chat() {
    let api_key = std::env::var("FORGE_AI_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .unwrap_or_default();

    println!();
    println!("  \x1B[1;35m╔══════════════════════════════════════╗\x1B[0m");
    println!("  \x1B[1;35m║        Forge AI Chat                 ║\x1B[0m");
    println!("  \x1B[1;35m╚══════════════════════════════════════╝\x1B[0m");
    println!();

    if api_key.is_empty() {
        println!("  \x1B[33mNo API key found.\x1B[0m");
        println!("  Set FORGE_AI_KEY or OPENAI_API_KEY environment variable.");
        println!();
        println!("  Example:");
        println!("    export FORGE_AI_KEY=your-api-key-here");
        println!("    forge chat");
        println!();
        return;
    }

    println!("  Connected! Type your message, or 'exit' to quit.");
    println!("  Type '/forge <code>' to run Forge code inline.");
    println!();

    let model = std::env::var("FORGE_AI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let url = std::env::var("FORGE_AI_URL")
        .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string());

    let mut history: Vec<serde_json::Value> = vec![serde_json::json!({
        "role": "system",
        "content": "You are a helpful assistant. You also know the Forge programming language. When asked to write code, use Forge syntax (say, set, define, etc.)."
    })];

    loop {
        print!("  \x1B[1;35myou>\x1B[0m ");
        io::stdout().flush().ok();

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }

        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "exit" || trimmed == "quit" {
            println!("  Goodbye!");
            break;
        }

        if let Some(code) = trimmed.strip_prefix("/forge ") {
            println!();
            let mut lexer = crate::lexer::Lexer::new(code);
            if let Ok(tokens) = lexer.tokenize() {
                let mut parser = crate::parser::Parser::new(tokens);
                if let Ok(program) = parser.parse_program() {
                    let mut interp = crate::interpreter::Interpreter::new();
                    match interp.run(&program) {
                        Ok(_) => {}
                        Err(e) => eprintln!("  \x1B[31m{}\x1B[0m", e),
                    }
                }
            }
            println!();
            continue;
        }

        history.push(serde_json::json!({
            "role": "user",
            "content": trimmed
        }));

        let body = serde_json::json!({
            "model": model,
            "messages": history,
            "max_tokens": 1000
        });

        let mut headers = std::collections::HashMap::new();
        headers.insert("Authorization".to_string(), format!("Bearer {}", api_key));
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        print!("  \x1B[1;36mai>\x1B[0m ");
        io::stdout().flush().ok();

        match crate::runtime::client::fetch_blocking(
            &url,
            "POST",
            Some(body.to_string()),
            Some(&headers),
        ) {
            Ok(crate::interpreter::Value::Object(resp)) => {
                if let Some(crate::interpreter::Value::Object(json_body)) = resp.get("json") {
                    if let Some(crate::interpreter::Value::Array(choices)) =
                        json_body.get("choices")
                    {
                        if let Some(crate::interpreter::Value::Object(choice)) = choices.first() {
                            if let Some(crate::interpreter::Value::Object(msg)) =
                                choice.get("message")
                            {
                                if let Some(crate::interpreter::Value::String(content)) =
                                    msg.get("content")
                                {
                                    println!("{}", content);
                                    history.push(serde_json::json!({
                                        "role": "assistant",
                                        "content": content
                                    }));
                                    println!();
                                    continue;
                                }
                            }
                        }
                    }
                }
                println!("\x1B[31mFailed to parse response\x1B[0m");
            }
            _ => {
                println!("\x1B[31mFailed to reach AI API\x1B[0m");
            }
        }
        println!();
    }
}
