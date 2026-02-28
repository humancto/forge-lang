use std::io::{self, Write};

const LESSONS: &[(&str, &str, &str, &str)] = &[
    (
        "Hello World",
        "Let's start with the basics. In Forge, you use 'say' to print output.",
        r#"say "Hello, World!""#,
        "Hello, World!",
    ),
    (
        "Variables",
        "Use 'set' to create variables. They're immutable by default.",
        r#"set name to "Forge"
say "Welcome to {name}!""#,
        "Welcome to Forge!",
    ),
    (
        "Mutable Variables",
        "Add 'mut' to make a variable mutable, then use 'change' to update it.",
        r#"set mut score to 0
change score to score + 10
say "Score: {score}""#,
        "Score: 10",
    ),
    (
        "Functions",
        "Use 'define' (or 'fn') to create functions.",
        r#"define greet(name) {
    return "Hello, {name}!"
}
say greet("Developer")"#,
        "Hello, Developer!",
    ),
    (
        "The Fun Trio",
        "Forge has three output styles: say (normal), yell (LOUD), whisper (quiet).",
        r#"say "I'm normal"
yell "I'm loud!"
whisper "I'm quiet""#,
        "I'm normal\nI'M LOUD!\ni'm quiet",
    ),
    (
        "Arrays & Loops",
        "Create arrays with [...] and loop with 'for each'.",
        r#"set colors to ["red", "green", "blue"]
for each color in colors {
    say color
}"#,
        "red\ngreen\nblue",
    ),
    (
        "Objects",
        "Objects use { key: value } syntax, like JSON.",
        r#"set user to { name: "Alice", age: 30 }
say user"#,
        "{ ... }",
    ),
    (
        "Repeat",
        "Use 'repeat N times' for counted loops -- unique to Forge!",
        r#"set mut count to 0
repeat 5 times {
    change count to count + 1
}
say "Count: {count}""#,
        "Count: 5",
    ),
    (
        "Destructuring",
        "Use 'unpack' to extract values from objects and arrays.",
        r#"set user to { name: "Bob", age: 25 }
unpack { name, age } from user
say "Name: {name}, Age: {age}""#,
        "Name: Bob, Age: 25",
    ),
    (
        "Error Handling",
        "Use 'must' to crash on errors, 'safe' to swallow them, or 'try/catch' to handle them.",
        r#"safe {
    let x = 1 / 0
}
say "I survived a division by zero!"

try {
    let y = 1 / 0
} catch err {
    say "Caught: {err}"
}"#,
        "I survived...\nCaught: division by zero",
    ),
    (
        "When Guards",
        "Use 'when' for cleaner conditional logic -- unique to Forge!",
        r#"set age to 25
when age {
    < 13 -> "kid"
    < 20 -> "teen"
    < 65 -> "adult"
    else -> "senior"
}"#,
        "(evaluates to matched value)",
    ),
    (
        "HTTP & APIs",
        "Forge has built-in HTTP. Use http.get, http.post, or the 'grab' keyword.",
        r#"let resp = http.get("https://httpbin.org/get")
say "Status: {resp.status}"
say "Time: {resp.time}ms""#,
        "Status: 200\nTime: Xms",
    ),
    (
        "Terminal Colors",
        "Make your output beautiful with the term module.",
        r#"say term.red("Error!")
say term.green("Success!")
say term.bold("Important!")"#,
        "(colored output)",
    ),
    (
        "Testing",
        "Write tests with @test and assert, then run with 'forge test'.",
        r#"@test
define should_add() {
    assert(1 + 1 == 2)
    assert_eq(math.sqrt(16), 4.0)
}"#,
        "(run with: forge test)",
    ),
];

pub fn run_learn(lesson_num: Option<usize>) {
    if let Some(n) = lesson_num {
        if n == 0 || n > LESSONS.len() {
            eprintln!(
                "Lesson {} doesn't exist. There are {} lessons.",
                n,
                LESSONS.len()
            );
            return;
        }
        show_lesson(n - 1);
        return;
    }

    println!();
    println!("  \x1B[1;36m╔══════════════════════════════════════╗\x1B[0m");
    println!("  \x1B[1;36m║     Welcome to Forge Academy!        ║\x1B[0m");
    println!("  \x1B[1;36m╚══════════════════════════════════════╝\x1B[0m");
    println!();
    println!("  {} interactive lessons available:", LESSONS.len());
    println!();
    for (i, (title, _, _, _)) in LESSONS.iter().enumerate() {
        println!("    \x1B[1m{:>2}.\x1B[0m {}", i + 1, title);
    }
    println!();
    println!("  Run a lesson:  \x1B[36mforge learn <number>\x1B[0m");
    println!("  Example:       \x1B[36mforge learn 1\x1B[0m");
    println!();
}

fn show_lesson(idx: usize) {
    let (title, explanation, code, expected) = LESSONS[idx];

    println!();
    println!(
        "  \x1B[1;33m━━━ Lesson {} of {}: {} ━━━\x1B[0m",
        idx + 1,
        LESSONS.len(),
        title
    );
    println!();
    println!("  \x1B[90m{}\x1B[0m", explanation);
    println!();
    println!("  \x1B[1mCode:\x1B[0m");
    for line in code.lines() {
        println!("    \x1B[36m{}\x1B[0m", line);
    }
    println!();
    println!("  \x1B[1mExpected output:\x1B[0m");
    println!("    \x1B[32m{}\x1B[0m", expected);
    println!();

    print!("  \x1B[90mPress Enter to run this code (or 'q' to quit): \x1B[0m");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    if input.trim() == "q" {
        return;
    }

    println!();
    println!("  \x1B[1mOutput:\x1B[0m");
    print!("    ");

    let mut lexer = crate::lexer::Lexer::new(code);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("    \x1B[31mLex error: {}\x1B[0m", e);
            return;
        }
    };
    let mut parser = crate::parser::Parser::new(tokens);
    let program = match parser.parse_program() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("    \x1B[31mParse error: {}\x1B[0m", e);
            return;
        }
    };
    let mut interp = crate::interpreter::Interpreter::new();
    match interp.run(&program) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("    \x1B[31m{}\x1B[0m", e);
        }
    }

    println!();
    if idx + 1 < LESSONS.len() {
        println!(
            "  \x1B[90mNext lesson: \x1B[36mforge learn {}\x1B[0m",
            idx + 2
        );
    } else {
        println!("  \x1B[1;32mYou've completed all lessons! You're a Forge developer now!\x1B[0m");
    }
    println!();
}
