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
    (
        "String Transformations",
        "Forge has built-in case conversion, slugification, and text manipulation.",
        r#"say slugify("Hello World! @2024")
say snake_case("myAPIKey")
say camel_case("hello_world")
say capitalize("hello")
say title("the quick fox")
say pad_start("42", 6, "0")
say repeat_str("-", 20)"#,
        "hello-world-2024\nmy_api_key\nhelloWorld\nHello\nThe Quick Fox\n000042\n--------------------",
    ),
    (
        "Collection Power Tools",
        "Sum, filter, group, chunk, zip, flatten — process data like a pro.",
        r#"let nums = [1, 2, 3, 4, 5, 6]
say "Sum: {sum(nums)}"
say "Min: {min_of(nums)}, Max: {max_of(nums)}"
say "Any > 5? {any(nums, fn(x) { return x > 5 })}"
say "All > 0? {all(nums, fn(x) { return x > 0 })}"
say "Unique: {unique([1, 1, 2, 2, 3])}"
say "Chunks: {chunk(nums, 2)}"

let groups = group_by(nums, fn(x) {
    if x % 2 == 0 { return "even" }
    return "odd"
})
say "Even: {groups.even}"
say "Odd: {groups.odd}""#,
        "Sum: 21\nMin: 1, Max: 6\nAny > 5? true\nAll > 0? true\nUnique: [1, 2, 3]\nChunks: [[1, 2], [3, 4], [5, 6]]\nEven: [2, 4, 6]\nOdd: [1, 3, 5]",
    ),
    (
        "GenZ Debug Kit",
        "Debug with attitude! sus() inspects values, bet() asserts with swagger, yolo() ignores errors.",
        r#"// sus() — like Rust's dbg! but cooler
let x = sus(42)
say "x is still {x}"

// bet() — assert with swagger
bet(1 + 1 == 2, "math is broken")

// no_cap() — assert_eq, GenZ style
no_cap(len("hello"), 5)

// yolo() — swallow ALL errors
let result = yolo(fn() {
    let x = 1 / 0
    return "nope"
})
say "yolo result: {result}"

// ick() — assert something is FALSE
ick(1 > 100, "math shouldn't break")"#,
        "SUS CHECK: 42 (Int)\nx is still 42\nyolo result: None",
    ),
    (
        "Performance Profiling",
        "cook() times your code, slay() benchmarks it — built-in performance tools.",
        r#"// cook() — time a function with personality
let answer = cook(fn() {
    let mut total = 0
    for i in range(1, 1001) {
        total = total + i
    }
    return total
})
say "Answer: {answer}"

// slay() — benchmark with stats
let stats = slay(fn() {
    return 1 + 1
}, 10)
say "Runs: {stats.runs}"
say "Result: {stats.result}""#,
        "COOKED: done in Xms\nAnswer: 500500\nSLAYED: 10x runs\nRuns: 10\nResult: 2",
    ),
    (
        "NPC — Fake Data",
        "Generate realistic fake data instantly with the npc module — perfect for testing.",
        r#"say "Name: {npc.name()}"
say "Email: {npc.email()}"
say "User: {npc.username()}"
say "Phone: {npc.phone()}"
say "Color: {npc.color()}"
say "IP: {npc.ip()}"
say "ID: {npc.id()}"
say "Company: {npc.company()}"

// Pick random items
let foods = ["pizza", "sushi", "tacos"]
say "Dinner: {npc.pick(foods)}"

// Random number in range
say "Dice: {npc.number(1, 6)}"

// Random sentence
say npc.sentence(5)"#,
        "(random data each run)",
    ),
    (
        "Smart Array Ops",
        "partition splits arrays by condition, diff compares objects, sample picks random items.",
        r#"// partition — split into [matches, rest]
let nums = [1, 2, 3, 4, 5, 6]
let parts = partition(nums, fn(x) { return x > 3 })
say "Big: {parts[0]}"
say "Small: {parts[1]}"

// diff — deep object comparison
let before = { name: "Alice", role: "user" }
let after = { name: "Alice", role: "admin", level: 5 }
let changes = diff(before, after)
say "Role changed: {changes.role.from} -> {changes.role.to}"
say "Level added: {changes.level.added}"

// shuffle and sample
let deck = [1, 2, 3, 4, 5]
say "Sample: {sample(deck, 2)}"
say "Shuffled: {shuffle(deck)}""#,
        "Big: [4, 5, 6]\nSmall: [1, 2, 3]\nRole changed: user -> admin\nLevel added: 5\n(random samples)",
    ),
    (
        "Advanced Testing",
        "assert_ne, assert_throws, @skip, @before/@after hooks — professional test infrastructure.",
        r#"// assert_ne — values must NOT be equal
@test
define test_not_equal() {
    assert_ne(1, 2)
    assert_ne("hello", "world")
}

// assert_throws — verify errors are thrown
@test
define test_division_error() {
    assert_throws(fn() {
        let x = 1 / 0
    })
}

// @skip — temporarily disable a test
@test
@skip
define test_wip() {
    assert(false, "not ready yet")
}

// Structured errors have .message and .type
@test
define test_error_fields() {
    try { let x = 1 / 0 } catch err {
        assert_eq(err.type, "ArithmeticError")
        assert(contains(err.message, "zero"))
    }
}"#,
        "(run with: forge test)",
    ),
    (
        "File Path Utils",
        "Work with file paths, read lines, and explore the filesystem.",
        r#"// Path manipulation
say fs.dirname("/home/user/file.txt")
say fs.basename("/home/user/file.txt")
say fs.join_path("/home", "user", "docs")

// File type checks
say "Is directory? {fs.is_dir("/tmp")}"
say "Temp dir: {fs.temp_dir()}"

// Read file as lines
fs.write("/tmp/test.txt", "line1\nline2\nline3")
let lines = fs.lines("/tmp/test.txt")
say "Lines: {len(lines)}"
fs.remove("/tmp/test.txt")

// Math extras
say "Clamped: {math.clamp(150, 0, 100)}"
say "Random: {math.random_int(1, 10)}""#,
        "/home/user\nfile.txt\n/home/user/docs\nIs directory? true\n...",
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
