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
    say "Caught: {err.message}"
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
    (
        "File I/O",
        "Read and write files with the fs module. Forge makes file operations simple.",
        r#"fs.write("hello.txt", "Hello from Forge!")
let content = fs.read("hello.txt")
say "Read: {content}"
say "Exists? {fs.exists("hello.txt")}"
fs.remove("hello.txt")
say "After delete: {fs.exists("hello.txt")}"
"#,
        "Read: Hello from Forge!\nExists? true\nAfter delete: false",
    ),
    (
        "Data Processing",
        "Use map, filter, and reduce to transform data — functional programming in Forge.",
        r#"let people = [
    { name: "Alice", age: 30 },
    { name: "Bob", age: 17 },
    { name: "Charlie", age: 25 }
]
let adults = filter(people, fn(p) { return p.age >= 18 })
let names = map(adults, fn(p) { return p.name })
say join(names, ", ")"#,
        "Alice, Charlie",
    ),
    (
        "Database",
        "Forge has built-in SQLite. Open a database, create tables, and query with parameters.",
        r#"db.open(":memory:")
db.execute("CREATE TABLE users (name TEXT, age INT)")
db.execute("INSERT INTO users VALUES (?, ?)", ["Alice", 30])
db.execute("INSERT INTO users VALUES (?, ?)", ["Bob", 25])
let rows = db.query("SELECT * FROM users")
for each row in rows {
    say "{row.name} is {row.age}"
}
db.close()"#,
        "Alice is 30\nBob is 25",
    ),
    (
        "Shell & System",
        "Run shell commands and interact with the system — perfect for scripting.",
        r#"let user = sh("whoami")
say "User: {user}"
say "Command works? {sh_ok("echo test")}"
let dir = cwd()
say "Directory: {dir}"
let sh_path = which("sh")
say "sh is at: {sh_path}""#,
        "(your user, true, current dir, sh path)",
    ),
    (
        "Channels & Spawn",
        "Forge has built-in concurrency with spawn (background tasks) and channels (communication).",
        r#"let ch = channel()
spawn {
    send(ch, "Hello from background!")
}
let msg = receive(ch)
say msg

let handle = spawn { return 21 * 2 }
let result = await handle
say "Computed: {result}""#,
        "Hello from background!\nComputed: 42",
    ),
    (
        "Pattern Matching & ADTs",
        "Define custom types with ADTs and use match for type-safe branching.",
        r#"type Shape = Circle(Float) | Square(Float)

define area(s) {
    match s {
        Circle(r) => return 3.14 * r * r
        Square(side) => return side * side
    }
}
say area(Circle(5.0))
say area(Square(4.0))"#,
        "78.5\n16",
    ),
    (
        "Result & Option",
        "Use Result (Ok/Err) and Option (Some/None) for safe error handling.",
        r#"let greeting = Ok("Hello!")
let missing = Err("not found")
say "Ok? {is_ok(greeting)}"
say "Value: {unwrap(greeting)}"
let fallback = unwrap_or(missing, "default")
say "Fallback: {fallback}"

let name = Some("Alice")
let empty = None
say "Has name? {is_some(name)}"
let name_val = unwrap_or(empty, "nobody")
say "Empty becomes: {name_val}""#,
        "Ok? true\nValue: Hello!\nFallback: default\nHas name? true\nEmpty becomes: nobody",
    ),
    (
        "String Processing",
        "Split, join, replace, and search strings with built-in functions and regex.",
        r#"let text = "Hello, World!"
let parts = split(text, ", ")
say "Parts: {parts}"
say replace(text, "World", "Forge")
say "Starts with Hello? {starts_with(text, "Hello")}"
say "Length: {len(text)}"

let found = regex.find("Order #42 shipped", "\\d+")
say "Found number: {found}""#,
        "Parts: [Hello, World!]\nHello, Forge!\nStarts with Hello? true\nLength: 13\nFound number: 42",
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
