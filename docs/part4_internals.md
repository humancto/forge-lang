# PART IV: UNDER THE HOOD

---

## Chapter 25: Architecture and Internals

Every sufficiently advanced programming language eventually reveals its inner machinery to the curious developer. Understanding how Forge works beneath its friendly syntax transforms you from a user of the language into a collaborator with it. This chapter pulls back the curtain on Forge's implementation: approximately 15,500 lines of Rust spread across 45 source files, with zero `unsafe` blocks in the entire codebase.

### The Compilation Pipeline

A Forge program begins its life as a `.fg` source file—a plain text document containing human-readable code. Through a series of well-defined transformations, that text becomes executable behavior. The pipeline has no magical jumps; each stage produces a clear intermediate representation consumed by the next.

```
                         FORGE COMPILATION PIPELINE

  ┌──────────────┐    ┌──────────┐    ┌──────────┐    ┌──────────────┐
  │  Source Code  │───>│  Lexer   │───>│  Parser  │───>│ Type Checker │
  │   (.fg file)  │    │ (tokens) │    │  (AST)   │    │  (warnings)  │
  └──────────────┘    └──────────┘    └──────────┘    └──────┬───────┘
                                                             │
                                         ┌───────────────────┼──────────────┐
                                         │                   │              │
                                         v                   v              │
                                ┌─────────────────┐  ┌─────────────┐       │
                                │   Interpreter    │  │  Bytecode   │       │
                                │  (tree-walk,     │  │  Compiler   │       │
                                │   default)       │  │  (--vm flag)│       │
                                └────────┬────────┘  └──────┬──────┘       │
                                         │                   │              │
                                         │                   v              │
                                         │           ┌─────────────┐       │
                                         │           │   VM Engine  │       │
                                         │           │ (register VM)│       │
                                         │           └──────┬──────┘       │
                                         │                   │              │
                                         v                   v              │
                                ┌─────────────────────────────────┐        │
                                │           Runtime               │        │
                                │  ┌─────────┐  ┌─────────────┐  │        │
                                │  │  Stdlib  │  │ HTTP Server  │  │        │
                                │  │(15 mods) │  │   (axum)     │  │        │
                                │  └─────────┘  └─────────────┘  │        │
                                └─────────────────────────────────┘        │
                                                                           │
                        Error Reporting (ariadne) <────────────────────────┘
```

The pipeline is invoked from `main.rs` (293 lines), which uses `clap` to parse CLI arguments and dispatch to the appropriate subsystem. The core execution path is remarkably concise:

```rust
async fn run_source(source: &str, filename: &str, use_vm: bool) {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize()?;

    let mut parser = ForgeParser::new(tokens);
    let program = parser.parse_program()?;

    let mut checker = typechecker::TypeChecker::new();
    let warnings = checker.check(&program);

    if use_vm {
        vm::run(&program)?;
    } else {
        let mut interpreter = Interpreter::new();
        interpreter.run(&program)?;
    }
}
```

This linear flow—lex, parse, check, execute—is the spine of every Forge execution, whether triggered by `forge run`, `forge -e`, or the REPL.

### The Lexer: Tokenization

The lexer (`src/lexer/`, 875 lines across two files) transforms raw source text into a stream of tokens. Forge uses a hand-rolled lexer rather than a generator like `logos`, giving full control over string interpolation handling and error reporting.

#### Architecture

The `Lexer` struct maintains four pieces of state:

```rust
pub struct Lexer {
    source: Vec<char>,  // Source code as character array
    pos: usize,         // Current position in source
    line: usize,        // Current line number (1-based)
    col: usize,         // Current column number (1-based)
}
```

The `source` field stores the input as `Vec<char>` rather than operating on byte slices. This simplifies character-by-character processing at the cost of an upfront allocation. For the typical Forge program (hundreds to low thousands of lines), this tradeoff is negligible.

#### The Tokenization Loop

The main `tokenize()` method runs a single pass over the source, producing a `Vec<Spanned>` where each `Spanned` wraps a `Token` with its position information:

```rust
pub struct Spanned {
    pub token: Token,
    pub line: usize,
    pub col: usize,
    pub offset: usize,   // Byte offset from start of source
    pub len: usize,      // Length in characters
}
```

The core loop follows a standard pattern: skip whitespace (preserving newlines), examine the current character, and dispatch to the appropriate lexing method:

```
  Character          Handler              Example
  ─────────────────────────────────────────────────────
  0-9                lex_number()         42, 3.14, 1_000
  "                  lex_string()         "hello, {name}"
  """                lex_triple_string()  """raw text"""
  a-z, A-Z, _       lex_ident()          variable, let, say
  +, -, *, / ...     inline match         operators
  (, ), {, } ...     inline match         delimiters
  //                 skip_line_comment()  (consumed, no token)
  \n                 Token::Newline       (significant!)
```

#### Newline Significance

Unlike many languages, Forge treats newlines as significant tokens. The lexer skips spaces and tabs but _preserves_ newline characters as `Token::Newline`. This design enables Forge's semicolon-free syntax: newlines serve as implicit statement terminators. The parser calls `skip_newlines()` at appropriate points to consume runs of newlines where they don't carry meaning (between block elements, for instance), while relying on them for statement boundaries elsewhere.

#### The Keyword Table

When the lexer encounters an identifier, it consults a keyword lookup function before emitting an `Ident` token. Forge recognizes 80+ keywords across three categories:

```rust
pub fn keyword_from_str(s: &str) -> Option<Token> {
    match s {
        // Classic keywords (22)
        "let" => Some(Token::Let),
        "fn"  => Some(Token::Fn),
        "if"  => Some(Token::If),
        // ...

        // Natural-language keywords (18)
        "set"       => Some(Token::Set),
        "say"       => Some(Token::Say),
        "otherwise" => Some(Token::Otherwise),
        // ...

        // Innovation keywords (25+)
        "when"     => Some(Token::When),
        "must"     => Some(Token::Must),
        "timeout"  => Some(Token::Timeout),
        // ...

        _ => None,
    }
}
```

This flat `match` statement compiles to an efficient jump table in release builds. There is no separate hash map allocation; Rust's pattern matching handles the dispatch.

#### String Interpolation

Forge strings support interpolation via `{expression}` syntax. The lexer handles this by preserving the `{` and `}` characters within the string literal. Interpolation is resolved at runtime by the interpreter, which parses the `{...}` segments and evaluates them in the current scope.

Escape sequences within strings follow standard conventions with two additions for brace escaping:

| Escape | Character       |
| ------ | --------------- |
| `\n`   | Newline         |
| `\t`   | Tab             |
| `\r`   | Carriage return |
| `\\`   | Backslash       |
| `\"`   | Double quote    |
| `\{`   | Literal `{`     |
| `\}`   | Literal `}`     |

Triple-quoted strings (`"""..."""`) produce `RawStringLit` tokens with no escape processing or interpolation, useful for embedding SQL, HTML, or other verbatim text.

#### Numeric Literals

The lexer supports integer and floating-point literals with underscore separators for readability:

```
42          → Token::Int(42)
3.14        → Token::Float(3.14)
1_000_000   → Token::Int(1000000)
```

A decimal point is only treated as a float separator when followed by a digit, preventing ambiguity with method call syntax like `list.len()`.

### The Parser: Recursive Descent with Pratt Precedence

The parser (`src/parser/`, 2,147 lines across two files) transforms the token stream into an abstract syntax tree. It uses recursive descent for statement parsing and a layered precedence-climbing approach (inspired by Pratt parsing) for expressions.

#### Structure

```rust
pub struct Parser {
    tokens: Vec<Spanned>,
    pos: usize,
}
```

The parser maintains a flat token array and a position cursor. Peeking ahead, consuming tokens, and backtracking are all constant-time operations on this array.

#### Statement Parsing

The `parse_statement()` method dispatches on the current token to determine which statement form to parse:

```rust
fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
    match self.current_token() {
        Token::Let              => self.parse_let(),
        Token::Set              => self.parse_set(),
        Token::Change           => self.parse_change(),
        Token::Fn | Token::Define => self.parse_fn_def(Vec::new()),
        Token::If               => self.parse_if(),
        Token::Match            => self.parse_match(),
        Token::For              => self.parse_for(),
        Token::While            => self.parse_while(),
        Token::When             => self.parse_when(),
        Token::Check            => self.parse_check(),
        Token::Safe             => self.parse_safe_block(),
        Token::Timeout          => self.parse_timeout(),
        Token::Retry            => self.parse_retry(),
        Token::At               => self.parse_decorator_or_fn(),
        Token::Say | Token::Yell | Token::Whisper
                                => self.parse_say_yell_whisper(),
        // ... 15+ more variants
        _                       => self.parse_expr_or_assign(),
    }
}
```

Notice how both `Token::Fn` and `Token::Define` route to the same `parse_fn_def()` method, and `Token::Else`, `Token::Otherwise`, and `Token::Nah` are all treated identically when checking for else branches. This is how Forge's dual syntax is implemented: distinct tokens, shared parse logic.

#### Expression Precedence

Forge's expression parser uses a layered approach where each precedence level is a separate function that calls the next-higher level:

```
  Precedence Level    Function               Operators
  ──────────────────────────────────────────────────────────
  1 (lowest)          parse_pipeline()        |>
  2                   parse_or()              ||
  3                   parse_and()             &&
  4                   parse_equality()         ==  !=
  5                   parse_comparison()       <  >  <=  >=
  6                   parse_addition()         +  -
  7                   parse_multiplication()   *  /  %
  8                   parse_unary()            -  !  must  await  ...
  9 (highest)         parse_postfix()          ()  .  []  ?
  10                  parse_primary()          literals, idents, groups
```

Each function follows the same pattern: parse the higher-precedence sub-expression, then loop to consume operators at its own level:

```rust
fn parse_addition(&mut self) -> Result<Expr, ParseError> {
    let mut left = self.parse_multiplication()?;
    loop {
        let op = match self.current_token() {
            Token::Plus  => BinOp::Add,
            Token::Minus => BinOp::Sub,
            _ => break,
        };
        self.advance();
        let right = self.parse_multiplication()?;
        left = Expr::BinOp {
            left: Box::new(left), op, right: Box::new(right),
        };
    }
    Ok(left)
}
```

This naturally produces left-associative operators with correct precedence.

#### Handling Newlines in the Parser

The parser's `skip_newlines()` method is called at strategic points: at the top of `parse_statement()`, inside block parsing between statements, and before checking for `else`/`otherwise`/`nah` branches. This allows code like:

```forge
if condition {
    say "yes"
}
otherwise {
    say "no"
}
```

The newlines between `}` and `otherwise` are consumed by `skip_newlines()` before the parser checks for an else branch.

### The AST: Stmt and Expr Enums

The abstract syntax tree is defined in `src/parser/ast.rs` (335 lines). Forge's AST uses two central enums: `Stmt` (28 variants) for statements and `Expr` (22 variants) for expressions.

#### Statement Variants

```
  Stmt Variant         Syntax It Represents
  ─────────────────────────────────────────────────────────
  Let                  let x = 5 / set x to 5
  Assign               x = 10 / change x to 10
  FnDef                fn name() {} / define name() {}
  StructDef            struct Point { x: Int, y: Int }
  TypeDef              type Shape = Circle(Float) | Rect(Float, Float)
  InterfaceDef         interface Printable { print() }
  Return               return expr
  If                   if / else / otherwise / nah
  Match                match subject { pattern => body }
  When                 when subject { < 10 -> "small" }
  For                  for x in items / for each x in items
  While                while condition { }
  Loop                 loop { }
  Break                break
  Continue             continue
  Spawn                spawn { }
  Import               import "path"
  TryCatch             try { } catch err { }
  CheckStmt            check name is not empty
  SafeBlock            safe { }
  TimeoutBlock         timeout 5 seconds { }
  RetryBlock           retry 3 times { }
  ScheduleBlock        schedule every 5 minutes { }
  WatchBlock           watch "file.txt" { }
  PromptDef            prompt summarize(text) { }
  AgentDef             agent researcher(query) { }
  Destructure          unpack {a, b} from obj
  YieldStmt            yield expr / emit expr
  Expression           (bare expression as statement)
```

#### Expression Variants

```
  Expr Variant         Syntax It Represents
  ─────────────────────────────────────────────────────────
  Int, Float, Bool     42, 3.14, true
  StringLit            "hello"
  StringInterp         "hello, {name}"
  Array                [1, 2, 3]
  Object               { name: "Alice", age: 30 }
  Ident                variable_name
  BinOp                a + b, x == y
  UnaryOp              -x, !done
  FieldAccess          user.name
  Index                list[0]
  Call                 fn(args)
  MethodCall           obj.method(args)
  Pipeline             data |> transform
  Lambda               (x) => x * 2
  Try                  risky_call()?
  Await                await promise / hold promise
  Spread               ...args
  Must                 must dangerous_call()
  Freeze               freeze value
  Ask                  ask "what is the meaning of life?"
  WhereFilter          users where age > 21
  PipeChain            data >> keep where active >> take 10
  StructInit           Point { x: 1, y: 2 }
  Block                { stmts }
```

#### Supporting Types

The AST includes several supporting structures:

| Type                 | Purpose                                                               |
| -------------------- | --------------------------------------------------------------------- |
| `Program`            | Top-level container: `Vec<Stmt>`                                      |
| `Param`              | Function parameter with optional type and default                     |
| `TypeAnn`            | Type annotation: `Simple`, `Array`, `Generic`, `Function`, `Optional` |
| `Decorator`          | `@name(args)` metadata                                                |
| `MatchArm`           | Pattern + body for `match` expressions                                |
| `WhenArm`            | Operator + value + result for `when` guards                           |
| `Pattern`            | `Wildcard`, `Literal`, `Binding`, `Constructor`                       |
| `BinOp`              | 12 binary operators (Add through Or)                                  |
| `UnaryOp`            | 2 unary operators (Neg, Not)                                          |
| `PipeStep`           | Steps in a `>>` pipeline: Keep, Sort, Take, Apply                     |
| `DestructurePattern` | Object or array destructuring shapes                                  |
| `Variant`            | ADT variant with typed fields                                         |
| `CheckKind`          | Validation type: IsNotEmpty, Contains, Between, IsTrue                |
| `StringPart`         | Literal or Expr component of interpolated strings                     |

### The Interpreter: Tree-Walk Evaluation

The interpreter (`src/interpreter/mod.rs`, 4,584 lines) is the default execution engine. It walks the AST directly, evaluating each node recursively. While slower than bytecode execution, the tree-walk interpreter supports the full Forge feature set including async/await, the HTTP server, and all stdlib modules.

#### Runtime Values

The interpreter's `Value` enum maps closely to Forge's type system:

```rust
pub enum Value {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Array(Vec<Value>),
    Object(IndexMap<String, Value>),
    Function {
        name: String,
        params: Vec<Param>,
        body: Vec<Stmt>,
        closure: Environment,    // Captured scope
        decorators: Vec<Decorator>,
    },
    Lambda {
        params: Vec<Param>,
        body: Vec<Stmt>,
        closure: Environment,
    },
    ResultOk(Box<Value>),
    ResultErr(Box<Value>),
    BuiltIn(String),             // Name of built-in function
    Null,
}
```

Objects use `IndexMap` (from the `indexmap` crate) rather than `HashMap` to preserve insertion order, matching the behavior users expect from JSON-like object literals.

#### The Scope Stack

Variable scoping is managed by the `Environment` struct, which maintains a stack of hash maps:

```rust
pub struct Environment {
    scopes: Vec<HashMap<String, Value>>,
    mutability: Vec<HashMap<String, bool>>,
}
```

```
  SCOPE STACK (searched bottom to top for variable lookup)

  ┌───────────────────────────────────────┐
  │  Scope 3 (innermost block)            │  ← top (searched first)
  │  { temp: 42, flag: true }             │
  ├───────────────────────────────────────┤
  │  Scope 2 (function body)              │
  │  { x: 10, y: 20 }                    │
  ├───────────────────────────────────────┤
  │  Scope 1 (module level)               │
  │  { greet: <fn>, data: [...] }         │
  ├───────────────────────────────────────┤
  │  Scope 0 (global / builtins)          │  ← bottom (searched last)
  │  { print: <builtin>, math: {...},     │
  │    len: <builtin>, say: <builtin> }   │
  └───────────────────────────────────────┘
```

Key operations:

- **`push_scope()`**: Enters a new block. Pushes an empty HashMap onto both `scopes` and `mutability`.
- **`pop_scope()`**: Exits a block. Pops the top scope, discarding its variables.
- **`define(name, value)`**: Inserts a binding into the current (topmost) scope.
- **`get(name)`**: Searches scopes from top to bottom, returning the first match.
- **`set(name, value)`**: Searches scopes from top to bottom; updates the first matching scope. Returns an error if the variable is immutable.

The parallel `mutability` stack tracks whether each variable was declared with `mut`. Attempting to reassign an immutable variable produces a clear error with a hint:

```
cannot reassign immutable variable 'name' (use 'let mut' to make it mutable)
```

#### The "Did You Mean?" Feature

When variable lookup fails, the interpreter doesn't just report "undefined variable"—it searches all scopes for similar names using Levenshtein distance:

```rust
pub fn suggest_similar(&self, name: &str) -> Option<String> {
    let mut best: Option<(String, usize)> = None;
    for scope in &self.scopes {
        for key in scope.keys() {
            let dist = levenshtein(name, key);
            if dist <= 2 && dist < name.len() {
                // Track the closest match
            }
        }
    }
    best.map(|(s, _)| s)
}
```

This turns `undefined variable: naem` into `undefined variable: naem (did you mean: name?)`, a small touch that saves real debugging time.

#### Control Flow with Signals

The interpreter uses a `Signal` enum to propagate control flow across recursive evaluation:

```rust
enum Signal {
    None,       // Normal execution
    Return(Value),  // Function return
    Break,      // Loop break
    Continue,   // Loop continue
}
```

Every `exec_stmt()` call returns `Result<Signal, RuntimeError>`. The main execution loop checks each returned signal: `Signal::Return` short-circuits function execution, `Signal::Break` exits the nearest enclosing loop, and `Signal::Continue` skips to the next iteration. Encountering `Break` or `Continue` outside a loop produces a runtime error.

#### Closure Implementation

Closures in Forge capture their environment at the point of definition. When a function or lambda is created, the interpreter clones the current `Environment`:

```rust
Stmt::FnDef { name, params, body, decorators, is_async } => {
    let closure = self.env.clone();
    let func = Value::Function {
        name: name.clone(),
        params: params.clone(),
        body: body.clone(),
        closure,
        decorators: decorators.clone(),
    };
    self.env.define(name.clone(), func);
}
```

When the function is later called, the interpreter temporarily replaces the current environment with the captured closure, pushes a new scope for the function's parameters, executes the body, then restores the original environment. This correctly handles nested closures and avoids variable shadowing issues.

### How Builtins Are Registered and Dispatched

On initialization, the interpreter's `register_builtins()` method populates the global scope with two categories of values:

**1. Standard library modules** are registered as `Value::Object` instances, each containing named functions:

```rust
fn register_builtins(&mut self) {
    self.env.define("math".to_string(), crate::stdlib::create_math_module());
    self.env.define("fs".to_string(),   crate::stdlib::create_fs_module());
    self.env.define("crypto".to_string(), crate::stdlib::create_crypto_module());
    // ... 12 more modules
}
```

**2. Global built-in functions** are registered as `Value::BuiltIn(name)`:

```rust
for name in &["print", "println", "len", "type", "str", "int",
              "push", "pop", "keys", "values", "contains", "range",
              "map", "filter", "reduce", "sort", "reverse",
              "say", "yell", "whisper", "assert", "assert_eq",
              "Ok", "Err", "unwrap", "fetch", "time", "uuid",
              /* ... 40+ more ... */] {
    self.env.define(name.to_string(), Value::BuiltIn(name.to_string()));
}
```

When the interpreter encounters a `Call` expression and resolves the callee to `Value::BuiltIn(name)`, it dispatches through a large `match` statement that handles each built-in. This approach has the advantage of zero-overhead dispatch (no dynamic function pointers or vtables) and allows builtins to directly access interpreter internals.

### How the HTTP Server Integrates

Forge's HTTP server is built on axum (a Tokio-based web framework). The integration is clever: the server is _not_ invoked during normal script execution. Instead, after the interpreter finishes running the program, the runtime checks whether a `@server` decorator was defined:

```rust
let server_config = runtime::server::extract_server_config(&program);
let routes = runtime::server::extract_routes(&program);

if let Some(config) = server_config {
    runtime::server::start_server(interpreter, &config, &routes).await?;
}
```

The `extract_routes()` function scans the AST for functions decorated with `@get`, `@post`, `@put`, or `@delete`, collecting their URL patterns and handler names. The interpreter instance (with all defined functions in its environment) is wrapped in `Arc<Mutex<Interpreter>>` and passed as shared state to axum handlers.

When an HTTP request arrives, the axum handler locks the interpreter, constructs a request object, calls the Forge handler function, and converts the returned `Value` into an HTTP response. This design means each request briefly locks the interpreter—acceptable for development servers but not designed for production-scale concurrent workloads.

---

## Chapter 26: The Bytecode VM

While the tree-walk interpreter provides full-featured execution, some workloads benefit from the tighter execution loop of a bytecode virtual machine. Forge includes an experimental register-based VM activated with the `--vm` flag. This chapter examines its design, instruction set, and runtime subsystems.

### Why a VM?

Tree-walk interpreters pay a tax on every AST node they visit: pattern matching on the enum variant, navigating Box pointers, and recursing through the call stack. For tight loops and numeric computation, this overhead dominates the actual work. A bytecode VM eliminates these costs by compiling the AST into a flat array of instructions that a simple loop can decode and execute without recursion.

```
  TREE-WALK INTERPRETER

  eval_expr(BinOp { left, op, right })
    ├── eval_expr(left)        ← recursive call, enum match
    ├── eval_expr(right)       ← recursive call, enum match
    └── apply_op(op, l, r)     ← actual computation

  VM EXECUTION LOOP

  loop {
      let instruction = code[ip];   ← array index
      ip += 1;
      match decode_op(instruction) {
          OpCode::Add => {           ← flat switch
              regs[a] = regs[b] + regs[c];
          }
      }
  }
```

The VM's execution loop touches less memory, has better branch prediction, and avoids recursion entirely for non-call instructions. For compute-heavy programs, this can yield 2-5x speedups.

### Register-Based vs. Stack-Based VMs

Forge's VM uses a register-based architecture (like Lua 5 and Dalvik) rather than a stack-based one (like the JVM or CPython). In a register-based VM, instructions specify source and destination registers explicitly:

```
  Stack-Based (JVM-style):        Register-Based (Forge VM):

  PUSH a                          ADD  R2, R0, R1
  PUSH b                          (one instruction, three operands)
  ADD
  (three instructions, implicit
   stack manipulation)
```

The register approach produces fewer instructions (though each instruction is wider), reduces memory traffic to the operand stack, and simplifies optimization.

### Instruction Encoding

Each instruction is encoded as a single 32-bit word with three possible formats:

```
  Format ABC:   [  op:8  |  A:8  |  B:8  |  C:8  ]
  Format ABx:   [  op:8  |  A:8  |    Bx:16      ]
  Format AsBx:  [  op:8  |  A:8  |   sBx:16      ]
```

| Field | Size    | Purpose                                    |
| ----- | ------- | ------------------------------------------ |
| `op`  | 8 bits  | Opcode identifier (up to 256 instructions) |
| `A`   | 8 bits  | Destination register or primary operand    |
| `B`   | 8 bits  | Second register operand                    |
| `C`   | 8 bits  | Third register operand                     |
| `Bx`  | 16 bits | Unsigned extended operand (constant index) |
| `sBx` | 16 bits | Signed extended operand (jump offset)      |

Encoding and decoding functions are fully inlined for performance:

```rust
pub fn encode_abc(op: OpCode, a: u8, b: u8, c: u8) -> u32 {
    ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
}

#[inline(always)]
pub fn decode_op(instruction: u32) -> u8 {
    (instruction >> 24) as u8
}
```

### The Bytecode Instruction Set

Forge's VM defines 42 opcodes organized into seven categories:

**Loading Constants and Values**

| Opcode      | Format | Description                                     |
| ----------- | ------ | ----------------------------------------------- |
| `LoadConst` | ABx    | Load constant pool entry `Bx` into register `A` |
| `LoadNull`  | A      | Load `null` into register `A`                   |
| `LoadTrue`  | A      | Load `true` into register `A`                   |
| `LoadFalse` | A      | Load `false` into register `A`                  |

**Arithmetic and Logic**

| Opcode  | Format | Description             |
| ------- | ------ | ----------------------- |
| `Add`   | ABC    | `R[A] = R[B] + R[C]`    |
| `Sub`   | ABC    | `R[A] = R[B] - R[C]`    |
| `Mul`   | ABC    | `R[A] = R[B] * R[C]`    |
| `Div`   | ABC    | `R[A] = R[B] / R[C]`    |
| `Mod`   | ABC    | `R[A] = R[B] % R[C]`    |
| `Neg`   | AB     | `R[A] = -R[B]`          |
| `Eq`    | ABC    | `R[A] = R[B] == R[C]`   |
| `NotEq` | ABC    | `R[A] = R[B] != R[C]`   |
| `Lt`    | ABC    | `R[A] = R[B] < R[C]`    |
| `Gt`    | ABC    | `R[A] = R[B] > R[C]`    |
| `LtEq`  | ABC    | `R[A] = R[B] <= R[C]`   |
| `GtEq`  | ABC    | `R[A] = R[B] >= R[C]`   |
| `And`   | ABC    | `R[A] = R[B] && R[C]`   |
| `Or`    | ABC    | `R[A] = R[B] \|\| R[C]` |
| `Not`   | AB     | `R[A] = !R[B]`          |

**Variable Access**

| Opcode      | Format | Description                 |
| ----------- | ------ | --------------------------- |
| `Move`      | AB     | `R[A] = R[B]`               |
| `GetLocal`  | AB     | `R[A] = locals[B]`          |
| `SetLocal`  | AB     | `locals[A] = R[B]`          |
| `GetGlobal` | ABx    | `R[A] = globals[const[Bx]]` |
| `SetGlobal` | ABx    | `globals[const[Bx]] = R[A]` |

**Data Structures**

| Opcode         | Format | Description                                          |
| -------------- | ------ | ---------------------------------------------------- |
| `NewArray`     | ABC    | Create array from registers `B..B+C` into `R[A]`     |
| `NewObject`    | ABx    | Create object with `B` key-value pairs into `R[A]`   |
| `GetField`     | ABC    | `R[A] = R[B].field(const[C])`                        |
| `SetField`     | ABC    | `R[A].field(const[B]) = R[C]`                        |
| `GetIndex`     | ABC    | `R[A] = R[B][R[C]]`                                  |
| `SetIndex`     | ABC    | `R[A][R[B]] = R[C]`                                  |
| `Concat`       | ABC    | `R[A] = str(R[B]) + str(R[C])`                       |
| `Len`          | AB     | `R[A] = len(R[B])`                                   |
| `Interpolate`  | ABC    | Interpolate `C` parts starting at `R[B]` into `R[A]` |
| `ExtractField` | ABC    | Extract tuple field `C` from `R[B]` into `R[A]`      |

**Control Flow**

| Opcode        | Format | Description                            |
| ------------- | ------ | -------------------------------------- |
| `Jump`        | sBx    | `ip += sBx`                            |
| `JumpIfFalse` | AsBx   | If `R[A]` is falsy, `ip += sBx`        |
| `JumpIfTrue`  | AsBx   | If `R[A]` is truthy, `ip += sBx`       |
| `Loop`        | sBx    | `ip += sBx` (negative, jumps backward) |

**Functions**

| Opcode       | Format | Description                                    |
| ------------ | ------ | ---------------------------------------------- |
| `Call`       | ABC    | Call `R[A]` with `B` args, result in `R[C]`    |
| `Return`     | A      | Return `R[A]` from current function            |
| `ReturnNull` | —      | Return `null` from current function            |
| `Closure`    | ABx    | Create closure from prototype `Bx` into `R[A]` |

**Special**

| Opcode  | Format | Description                            |
| ------- | ------ | -------------------------------------- |
| `Try`   | AB     | `R[A] = try R[B]` (wrap in Result)     |
| `Spawn` | A      | Spawn green thread with closure `R[A]` |
| `Pop`   | —      | Discard top value (cleanup)            |

### The Compiler: AST to Bytecode

The bytecode compiler (`src/vm/compiler.rs`, 772 lines) performs a single pass over the AST, emitting instructions into a `Chunk`:

```rust
pub struct Compiler {
    chunk: Chunk,            // Output bytecode
    locals: Vec<Local>,      // Local variable tracking
    scope_depth: usize,      // Current nesting depth
    next_register: u8,       // Next available register
    max_register: u8,        // High-water mark
    loops: Vec<LoopContext>, // Active loop tracking for break/continue
}
```

Each `Chunk` contains the bytecode, a constant pool, line number information, and nested prototypes (for closures):

```rust
pub struct Chunk {
    pub code: Vec<u32>,          // Bytecode instructions
    pub constants: Vec<Constant>, // Constant pool
    pub lines: Vec<usize>,       // Line numbers (parallel to code)
    pub name: String,            // Function name
    pub prototypes: Vec<Chunk>,  // Nested function prototypes
    pub max_registers: u8,       // Register count needed
    pub upvalue_count: u8,       // Captured variable count
    pub arity: u8,               // Parameter count
}
```

The compiler tracks local variables with their scope depth and assigned register, using a register allocation strategy that simply increments a counter and reclaims registers when scopes close.

**Jump Patching**: Forward jumps (for `if`/`else`, `while`, etc.) are emitted with a placeholder offset, then patched once the target address is known:

```rust
fn emit_jump(&mut self, op: OpCode, a: u8, line: usize) -> usize {
    let idx = self.chunk.code_len();
    self.emit(encode_asbx(op, a, 0), line);  // Placeholder
    idx
}

fn patch_jump(&mut self, offset: usize) {
    let target = self.chunk.code_len();
    self.chunk.patch_jump(offset, target);
}
```

### The Execution Loop

The VM's core (`src/vm/machine.rs`, 1,807 lines) runs a tight decode-dispatch loop:

```rust
pub struct VM {
    pub registers: Vec<Value>,       // Flat register array
    pub frames: Vec<CallFrame>,      // Call stack
    pub globals: HashMap<String, Value>,
    pub gc: Gc,                      // Garbage collector
    pub output: Vec<String>,         // Captured output
}
```

The call stack uses `CallFrame` structs that point into the flat register array:

```rust
pub struct CallFrame {
    pub closure: GcRef,   // The executing closure
    pub ip: usize,        // Instruction pointer
    pub base: usize,      // Base register offset
}

pub const MAX_FRAMES: usize = 256;
pub const MAX_REGISTERS: usize = MAX_FRAMES * 256;  // 65,536
```

Each frame owns a "window" of 256 registers starting at `base`. Register references in instructions are relative to the current frame's base, providing isolation between function invocations without copying.

### Mark-Sweep Garbage Collection

The VM uses a mark-sweep garbage collector (`src/vm/gc.rs`, 113 lines) to manage heap-allocated objects. Unlike the interpreter (which uses Rust's ownership and cloning for memory management), the VM needs explicit GC because objects may be referenced from multiple locations.

#### Object Representation

Heap objects are stored as `GcObject` instances in a flat vector:

```rust
pub struct GcObject {
    pub kind: ObjKind,
    pub marked: bool,      // GC mark flag
}

pub enum ObjKind {
    String(String),
    Array(Vec<Value>),
    Object(IndexMap<String, Value>),
    Function(ObjFunction),
    Closure(ObjClosure),
    NativeFunction(NativeFn),
    Upvalue(ObjUpvalue),
    ResultOk(Value),
    ResultErr(Value),
}
```

References to heap objects are `GcRef(usize)` indices into the GC's object vector.

#### Collection Algorithm

```
  MARK-SWEEP GC CYCLE

  1. MARK PHASE                      2. SWEEP PHASE
  ┌────────────────────┐             ┌────────────────────┐
  │ Start from roots:  │             │ Walk all objects:   │
  │  - registers       │             │  - marked=true?     │
  │  - globals         │             │    → unmark, keep   │
  │  - call frames     │             │  - marked=false?    │
  │                    │             │    → free, add to   │
  │ Worklist-based     │             │      free list      │
  │ traversal:         │             │                     │
  │  For each root:    │             │ Update threshold:   │
  │   mark it          │             │  next_gc =          │
  │   trace children   │             │    alloc_count * 2  │
  │   add to worklist  │             │                     │
  └────────────────────┘             └────────────────────┘
```

The GC triggers when allocations exceed a threshold (initially 256). After collection, the threshold is set to twice the surviving object count, implementing a simple adaptive strategy. The minimum threshold is clamped to 256 to avoid pathologically frequent collections with few live objects.

The mark phase uses an explicit worklist rather than recursion:

```rust
fn mark(&mut self, roots: &[GcRef]) {
    let mut worklist: Vec<GcRef> = roots.to_vec();
    while let Some(r) = worklist.pop() {
        if let Some(obj) = self.objects.get_mut(r.0).and_then(|o| o.as_mut()) {
            if obj.marked { continue; }
            obj.marked = true;
            obj.trace(&mut worklist);  // Add referenced objects
        }
    }
}
```

Freed object slots are added to a free list for reuse, avoiding vector growth when objects churn.

### Green Thread Scheduler

The VM includes a scaffold for cooperative green threads (`src/vm/green.rs`, 83 lines). Currently, `spawn` blocks execute synchronously—the scheduler runs each spawned chunk to completion before starting the next:

```rust
pub fn run_all(&mut self, vm: &mut VM) -> Result<(), VMError> {
    while let Some(thread) = self.threads.iter_mut()
        .find(|t| t.state == ThreadState::Ready)
    {
        thread.state = ThreadState::Running;
        vm.execute(&thread.chunk)?;
        thread.state = ThreadState::Completed;
    }
    Ok(())
}
```

The data structures for genuine cooperative scheduling are in place—thread states (`Ready`, `Running`, `Yielded`, `Completed`), a thread ID system, and an active count tracker. Future work will integrate with Tokio for preemption at function calls and loop back-edges.

### Using the --vm Flag

To run a program with the bytecode VM:

```bash
forge run program.fg --vm
```

To compile a program to bytecode and view compilation statistics:

```bash
forge build program.fg
```

This outputs:

```
Compiled program.fg -> program.fgc
  47 instructions
  12 constants
  3 prototypes
  8 max registers
```

The `--vm` flag is experimental. It supports core language features (variables, functions, closures, control flow, data structures, error handling) but does not yet support async/await, the HTTP server, or all standard library modules.

---

## Chapter 27: Tooling Deep Dive

A programming language is only as good as its tools. Forge ships with a comprehensive toolchain that handles formatting, testing, project scaffolding, compilation, package management, editor integration, interactive learning, and AI-assisted development—all from a single binary.

### forge fmt: Code Formatter

The formatter (`src/formatter.rs`, 147 lines) normalizes whitespace and indentation across Forge source files.

**How it works**: The formatter operates on text lines rather than the AST, making it fast and robust (it never fails, even on syntactically invalid code). It tracks brace nesting depth to compute indentation:

1. For each line, trim leading whitespace
2. If the line starts with `}`, decrease indent level
3. Apply the computed indent (4 spaces per level)
4. If the line ends with `{`, increase indent level for the next line
5. Collapse multiple consecutive blank lines into one
6. Ensure a trailing newline

**What it normalizes**:

- Consistent 4-space indentation
- Removal of trailing whitespace
- Collapse of consecutive blank lines
- Consistent line endings

**Usage**:

```bash
forge fmt                    # Format all .fg files in current directory
forge fmt src/main.fg        # Format specific file
forge fmt src/ lib/          # Format multiple paths
```

The formatter recursively discovers `.fg` files, skipping directories named `.` (dot-prefixed), `target`, and `node_modules`.

### forge test: Test Runner

The test runner (`src/testing/mod.rs`, 170 lines) discovers and executes test functions marked with the `@test` decorator.

**Test runner architecture**:

```
  1. Discover .fg files in tests/ directory
  2. For each file:
     a. Lex → Parse (report errors)
     b. Find functions with @test decorator
     c. Run full program (defines all functions)
     d. For each @test function:
        - Call it with no arguments
        - Time execution
        - Report pass/fail with duration
  3. Print summary: passed, failed, total
```

**Writing tests**:

```forge
@test
define should_add_numbers() {
    assert(1 + 1 == 2)
    assert_eq(2 * 3, 6)
}

@test
define should_handle_strings() {
    set name to "Forge"
    assert(len(name) == 5)
    assert(starts_with(name, "For"))
}
```

**Available assertion functions**:

| Function             | Description                          |
| -------------------- | ------------------------------------ |
| `assert(expr)`       | Fails if `expr` is falsy             |
| `assert_eq(a, b)`    | Fails if `a != b`, shows both values |
| `satisfies(val, fn)` | Fails if `fn(val)` returns false     |

**Output format**:

```
  tests/math_test.fg
    ok    should_add_numbers (2ms)
    ok    should_handle_strings (1ms)

  tests/api_test.fg
    FAIL  should_validate_input (3ms)
          assertion failed: expected true, got false

  2 passed, 1 failed, 3 total
```

The test runner exits with code 1 if any test fails, making it suitable for CI pipelines.

### forge new: Project Scaffolding

The `forge new` command (`src/scaffold.rs`, 71 lines) creates a project directory with a standard structure:

```bash
forge new my-app
```

Generates:

```
  my-app/
  ├── forge.toml          # Project manifest
  ├── main.fg             # Entry point
  ├── tests/
  │   └── basic_test.fg   # Starter test
  └── .gitignore          # Ignores *.fgc files
```

**forge.toml** contents:

```toml
[project]
name = "my-app"
version = "0.1.0"
description = ""

[test]
directory = "tests"
```

The manifest is read by `forge test` to determine the test directory and by future tooling for package metadata.

### forge build: Bytecode Compilation

The `forge build` command compiles a Forge source file to bytecode using the VM's compiler:

```bash
forge build program.fg
```

This runs the lexer, parser, and bytecode compiler, then reports statistics about the compiled output. The compiled bytecode is represented as a `Chunk` structure containing instructions, constants, and nested prototypes.

### forge install: Package Management

The package manager (`src/package.rs`, 118 lines) supports two installation sources:

**Git installation**:

```bash
forge install https://github.com/user/forge-utils.git
```

Clones the repository into `.forge/packages/forge-utils/`. Subsequent runs pull updates.

**Local installation**:

```bash
forge install ../shared-lib
```

Copies the directory into `.forge/packages/shared-lib/`.

**Import resolution** checks paths in order:

1. Direct file path (relative to current file)
2. `.forge/packages/<name>`
3. `.forge/packages/<name>/main.fg`

### forge lsp: Language Server Protocol

The LSP server (`src/lsp/mod.rs`, 261 lines) provides IDE integration over stdin/stdout using the Language Server Protocol:

**Supported capabilities**:

- **Diagnostics**: Real-time parse error reporting as you type
- **Completions**: Keyword and built-in function suggestions triggered by `.`

**Architecture**: The LSP runs as a long-lived process communicating via JSON-RPC. It re-lexes and re-parses the document on every change, sending diagnostics back to the editor. Completion requests return the full keyword list and standard library function names.

**Editor setup** (VS Code example):

```json
{
  "forge.lsp.path": "forge",
  "forge.lsp.args": ["lsp"]
}
```

### forge learn: Interactive Tutorials

The tutorial system (`src/learn.rs`, 229 lines) provides 14 progressive lessons built into the binary:

```bash
forge learn      # List all lessons
forge learn 1    # Start lesson 1
```

Each lesson includes a title, explanation, example code, and expected output. The system displays the lesson content, lets the user study the example, and provides the expected output for verification. Lessons cover:

| Lesson | Topic                           |
| ------ | ------------------------------- |
| 1      | Hello World                     |
| 2      | Variables                       |
| 3      | Mutable Variables               |
| 4      | Functions                       |
| 5      | The Fun Trio (say/yell/whisper) |
| 6      | Arrays & Loops                  |
| 7      | Objects                         |
| 8      | Repeat Loops                    |
| 9      | Destructuring                   |
| 10     | Error Handling                  |
| 11     | Pattern Matching                |
| 12     | Pipelines                       |
| 13     | HTTP Requests                   |
| 14     | Building APIs                   |

### forge chat: AI Integration

The `forge chat` command (`src/chat.rs`, 131 lines) starts an interactive AI chat session. It reads an API key from the `OPENAI_API_KEY` environment variable and communicates with the OpenAI API to provide conversational assistance about Forge programming.

### The forge.toml Manifest

The manifest file (`src/manifest.rs`, 68 lines) uses TOML format with the following schema:

```toml
[project]
name = "project-name"       # Required: project name
version = "0.1.0"           # Required: semver version
description = "A Forge app" # Optional: project description
entry = "main.fg"           # Optional: entry point file

[test]
directory = "tests"          # Optional: test directory (default: "tests")
```

The manifest is parsed using the `toml` and `serde` crates with default values for all optional fields.

---

# APPENDICES

---

## Appendix A: Complete Keyword Reference

Forge recognizes 80+ keywords divided into three categories: classic keywords familiar from mainstream languages, natural-language keywords that provide English-like alternatives, and innovation keywords unique to Forge.

### Table A-1: Classic Keywords

| Keyword     | Purpose                    | Example                           | Notes                         |
| ----------- | -------------------------- | --------------------------------- | ----------------------------- |
| `let`       | Declare variable           | `let x = 42`                      | Immutable by default          |
| `mut`       | Make variable mutable      | `let mut count = 0`               | Used with `let` or `set`      |
| `fn`        | Define function            | `fn greet(name) { }`              | Synonym of `define`           |
| `return`    | Return from function       | `return value`                    | Optional for last expression  |
| `if`        | Conditional branch         | `if x > 0 { }`                    | —                             |
| `else`      | Alternative branch         | `else { }`                        | Synonym of `otherwise`, `nah` |
| `match`     | Pattern matching           | `match value { 1 => "one" }`      | Exhaustive patterns           |
| `for`       | Loop over iterable         | `for x in items { }`              | Supports destructuring        |
| `in`        | Iterable marker            | `for x in range(10)`              | Used with `for`, `each`       |
| `while`     | Conditional loop           | `while running { }`               | —                             |
| `loop`      | Infinite loop              | `loop { if done { break } }`      | Use `break` to exit           |
| `break`     | Exit loop                  | `break`                           | —                             |
| `continue`  | Skip to next iteration     | `continue`                        | —                             |
| `struct`    | Define structure           | `struct Point { x: Int, y: Int }` | Named product type            |
| `type`      | Define algebraic data type | `type Color = Red \| Blue`        | Sum types with variants       |
| `interface` | Define interface           | `interface Printable { print() }` | Method signatures             |
| `impl`      | Implement interface        | `impl Printable for Point { }`    | Reserved for future use       |
| `pub`       | Public visibility          | `pub fn api() { }`                | Reserved for future use       |
| `import`    | Import module              | `import "utils.fg"`               | File or package import        |
| `spawn`     | Launch concurrent task     | `spawn { heavy_work() }`          | Currently synchronous         |
| `true`      | Boolean true               | `let flag = true`                 | —                             |
| `false`     | Boolean false              | `let done = false`                | —                             |
| `try`       | Begin try block            | `try { risky() }`                 | Paired with `catch`           |
| `catch`     | Handle error               | `catch err { log(err) }`          | Receives error value          |
| `async`     | Async function             | `async fn fetch_data() { }`       | Synonym of `forge` (keyword)  |
| `await`     | Await async result         | `await fetch("url")`              | Synonym of `hold`             |
| `yield`     | Yield from generator       | `yield value`                     | Synonym of `emit`             |

### Table A-2: Natural-Language Keywords

| Keyword     | Purpose                     | Example                     | Classic Equivalent        |
| ----------- | --------------------------- | --------------------------- | ------------------------- |
| `set`       | Declare variable            | `set name to "Alice"`       | `let name = "Alice"`      |
| `to`        | Assignment marker           | `set x to 42`               | `=` in `let`              |
| `change`    | Reassign variable           | `change score to score + 1` | `score = score + 1`       |
| `define`    | Define function             | `define greet(n) { }`       | `fn greet(n) { }`         |
| `otherwise` | Alternative branch          | `otherwise { }`             | `else { }`                |
| `nah`       | Alternative branch (casual) | `nah { }`                   | `else { }`                |
| `each`      | Loop marker                 | `for each item in list { }` | `for item in list`        |
| `repeat`    | Counted loop                | `repeat 5 times { }`        | `for _ in range(5)`       |
| `times`     | Repeat unit                 | `repeat 3 times { }`        | —                         |
| `grab`      | HTTP fetch                  | `grab resp from "url"`      | `let resp = fetch("url")` |
| `from`      | Source marker               | `grab data from url`        | —                         |
| `wait`      | Sleep / pause               | `wait 2 seconds`            | `sleep(2000)`             |
| `seconds`   | Time unit                   | `wait 5 seconds`            | —                         |
| `say`       | Print output                | `say "hello"`               | `println("hello")`        |
| `yell`      | Print uppercase             | `yell "loud"`               | — (unique)                |
| `whisper`   | Print lowercase             | `whisper "quiet"`           | — (unique)                |
| `forge`     | Async function              | `forge fetch_data() { }`    | `async fn fetch_data()`   |
| `hold`      | Await result                | `hold fetch("url")`         | `await fetch("url")`      |
| `emit`      | Yield value                 | `emit computed_value`       | `yield computed_value`    |
| `unpack`    | Destructure                 | `unpack {a, b} from obj`    | `let {a, b} = obj`        |

### Table A-3: Innovation Keywords

| Keyword     | Purpose                   | Example                          | Notes                        |
| ----------- | ------------------------- | -------------------------------- | ---------------------------- |
| `when`      | Guard-based matching      | `when age { < 13 -> "kid" }`     | Comparison-based match       |
| `unless`    | Negated conditional       | `do_thing() unless disabled`     | Postfix condition            |
| `until`     | Negated while             | `retry until success`            | Postfix loop condition       |
| `must`      | Crash on error            | `must parse(data)`               | Unwrap or panic with message |
| `check`     | Declarative validation    | `check name is not empty`        | Built-in validators          |
| `safe`      | Null-safe execution       | `safe { risky_call() }`          | Returns null on error        |
| `where`     | Collection filter         | `users where age > 21`           | SQL-like filtering           |
| `timeout`   | Time-limited execution    | `timeout 5 seconds { fetch() }`  | Cancels after duration       |
| `retry`     | Automatic retry           | `retry 3 times { connect() }`    | Retries on failure           |
| `schedule`  | Periodic execution        | `schedule every 5 minutes { }`   | Cron-like scheduling         |
| `every`     | Schedule interval         | `schedule every 10 seconds { }`  | Used with `schedule`         |
| `watch`     | File change detection     | `watch "config.fg" { reload() }` | File system watcher          |
| `ask`       | AI/LLM query              | `ask "summarize this text"`      | Calls language model         |
| `prompt`    | Define AI prompt template | `prompt summarize(text) { }`     | Structured LLM call          |
| `transform` | Data transformation       | `transform data { upper() }`     | Pipeline transform           |
| `table`     | Tabular display           | `table [row1, row2]`             | Terminal table output        |
| `select`    | Query projection          | `from users select name, age`    | SQL-like projection          |
| `order`     | Sort clause               | `order by name`                  | Used with `select`           |
| `by`        | Sort/order marker         | `sort by score`                  | —                            |
| `limit`     | Result limiting           | `limit 10`                       | Used with queries            |
| `keep`      | Pipeline filter           | `>> keep where active`           | Used with `>>` pipes         |
| `take`      | Pipeline slice            | `>> take 5`                      | Used with `>>` pipes         |
| `freeze`    | Deep immutable copy       | `freeze config`                  | Prevents mutation            |
| `download`  | Download file             | `download "url" to "file"`       | HTTP file download           |
| `crawl`     | Web scraping              | `crawl "https://example.com"`    | Returns page content         |
| `any`       | Existential check         | `any x in items`                 | Used in conditions           |

---

## Appendix B: Built-in Functions Quick Reference

Forge provides 50+ built-in functions available without imports. Standard library modules add 90+ more functions organized into 15 namespaces.

### Output Functions

| Function  | Signature        | Description                  | Example                  |
| --------- | ---------------- | ---------------------------- | ------------------------ |
| `print`   | `print(value)`   | Print without newline        | `print("loading...")`    |
| `println` | `println(value)` | Print with newline           | `println("done")`        |
| `say`     | `say(value)`     | Print with newline (natural) | `say "hello, world!"`    |
| `yell`    | `yell(value)`    | Print uppercase with newline | `yell "alert"` → `ALERT` |
| `whisper` | `whisper(value)` | Print lowercase with newline | `whisper "SHH"` → `shh`  |

### Type Conversion Functions

| Function | Signature                 | Description           | Example                  |
| -------- | ------------------------- | --------------------- | ------------------------ |
| `str`    | `str(value) -> String`    | Convert to string     | `str(42)` → `"42"`       |
| `int`    | `int(value) -> Int`       | Convert to integer    | `int("42")` → `42`       |
| `float`  | `float(value) -> Float`   | Convert to float      | `float("3.14")` → `3.14` |
| `type`   | `type(value) -> String`   | Get type name         | `type(42)` → `"Int"`     |
| `typeof` | `typeof(value) -> String` | Get type name (alias) | `typeof([])` → `"Array"` |

### Collection Functions

| Function       | Signature                       | Description                          | Example                                     |
| -------------- | ------------------------------- | ------------------------------------ | ------------------------------------------- |
| `len`          | `len(collection) -> Int`        | Get length/size                      | `len([1,2,3])` → `3`                        |
| `push`         | `push(array, value) -> Array`   | Append element                       | `push(list, 4)`                             |
| `pop`          | `pop(array) -> Value`           | Remove and return last               | `pop(stack)`                                |
| `keys`         | `keys(object) -> Array`         | Get object keys                      | `keys({a:1})` → `["a"]`                     |
| `values`       | `values(object) -> Array`       | Get object values                    | `values({a:1})` → `[1]`                     |
| `contains`     | `contains(coll, val) -> Bool`   | Check membership                     | `contains([1,2], 2)` → `true`               |
| `range`        | `range(n) -> Array`             | Generate `[0..n-1]`                  | `range(3)` → `[0,1,2]`                      |
| `enumerate`    | `enumerate(array) -> Array`     | Index-value pairs                    | `enumerate(["a"])` → `[[0,"a"]]`            |
| `has_key`      | `has_key(obj, key) -> Bool`     | Check if object has key              | `has_key(user, "email")` → `true`           |
| `get`          | `get(obj, key, default)`        | Safe access with fallback, dot-paths | `get(obj, "a.b", "x")`                      |
| `pick`         | `pick(obj, [keys]) -> Object`   | Extract specific fields              | `pick(user, ["name"])`                      |
| `omit`         | `omit(obj, [keys]) -> Object`   | Remove specific fields               | `omit(user, ["password"])`                  |
| `merge`        | `merge(obj1, obj2) -> Object`   | Combine objects (later wins)         | `merge(a, b)`                               |
| `entries`      | `entries(obj) -> Array`         | Object to [[key, val], ...]          | `entries({a: 1})` → `[["a", 1]]`            |
| `from_entries` | `from_entries(pairs) -> Object` | [[key, val], ...] to object          | `from_entries([["a", 1]])`                  |
| `find`         | `find(array, fn) -> Value`      | First matching element               | `find(arr, fn(x) { return x > 5 })`         |
| `flat_map`     | `flat_map(array, fn) -> Array`  | Map and flatten                      | `flat_map([[1,2],[3]], fn(x) { return x })` |
| `lines`        | `lines(string) -> Array`        | Split string by newlines             | `lines("a\nb")` → `["a", "b"]`              |

### Functional Programming Functions

| Function  | Signature                          | Description            | Example                            |
| --------- | ---------------------------------- | ---------------------- | ---------------------------------- |
| `map`     | `map(array, fn) -> Array`          | Transform each element | `map([1,2,3], (x) => x * 2)`       |
| `filter`  | `filter(array, fn) -> Array`       | Keep matching elements | `filter(nums, (x) => x > 0)`       |
| `reduce`  | `reduce(array, init, fn) -> Value` | Fold to single value   | `reduce([1,2,3], 0, (a,b) => a+b)` |
| `sort`    | `sort(array) -> Array`             | Sort elements          | `sort([3,1,2])` → `[1,2,3]`        |
| `reverse` | `reverse(array) -> Array`          | Reverse order          | `reverse([1,2,3])` → `[3,2,1]`     |

### String Functions

| Function      | Signature                          | Description         | Example                               |
| ------------- | ---------------------------------- | ------------------- | ------------------------------------- |
| `split`       | `split(str, delim) -> Array`       | Split string        | `split("a,b", ",")` → `["a","b"]`     |
| `join`        | `join(array, delim) -> String`     | Join with delimiter | `join(["a","b"], "-")` → `"a-b"`      |
| `replace`     | `replace(str, old, new) -> String` | Replace substring   | `replace("hi", "h", "H")` → `"Hi"`    |
| `starts_with` | `starts_with(str, prefix) -> Bool` | Check prefix        | `starts_with("hello", "he")` → `true` |
| `ends_with`   | `ends_with(str, suffix) -> Bool`   | Check suffix        | `ends_with("hello", "lo")` → `true`   |

### Result Functions

| Function    | Signature                             | Description           | Example                        |
| ----------- | ------------------------------------- | --------------------- | ------------------------------ |
| `Ok`        | `Ok(value) -> Result`                 | Create success result | `Ok(42)`                       |
| `Err`       | `Err(value) -> Result`                | Create error result   | `Err("failed")`                |
| `is_ok`     | `is_ok(result) -> Bool`               | Check if Ok           | `is_ok(Ok(1))` → `true`        |
| `is_err`    | `is_err(result) -> Bool`              | Check if Err          | `is_err(Err("x"))` → `true`    |
| `unwrap`    | `unwrap(result) -> Value`             | Extract Ok or panic   | `unwrap(Ok(42))` → `42`        |
| `unwrap_or` | `unwrap_or(result, default) -> Value` | Extract Ok or default | `unwrap_or(Err("x"), 0)` → `0` |

### Option Functions

| Function  | Signature                 | Description           | Example                     |
| --------- | ------------------------- | --------------------- | --------------------------- |
| `Some`    | `Some(value) -> Option`   | Create present option | `Some(42)`                  |
| `None`    | —                         | Absent option value   | `None`                      |
| `is_some` | `is_some(option) -> Bool` | Check if present      | `is_some(Some(1))` → `true` |
| `is_none` | `is_none(option) -> Bool` | Check if absent       | `is_none(None)` → `true`    |

### System Functions

| Function      | Signature                    | Description                    | Example                            |
| ------------- | ---------------------------- | ------------------------------ | ---------------------------------- |
| `time`        | `time() -> Float`            | Current Unix timestamp         | `time()` → `1709136000.0`          |
| `uuid`        | `uuid() -> String`           | Generate UUID v4               | `uuid()` → `"a1b2c3..."`           |
| `exit`        | `exit(code)`                 | Exit with status code          | `exit(1)`                          |
| `input`       | `input(prompt) -> String`    | Read line from stdin           | `input("Name: ")`                  |
| `wait`        | `wait(seconds)`              | Sleep for duration             | `wait(2)`                          |
| `shell`       | `shell(command) -> String`   | Execute shell command          | `shell("ls -la")`                  |
| `sh`          | `sh(command) -> String`      | Execute shell (alias)          | `sh("date")`                       |
| `run_command` | `run_command(cmd) -> String` | Execute command                | `run_command("echo hi")`           |
| `fetch`       | `fetch(url) -> Object`       | HTTP GET request               | `fetch("https://api.example.com")` |
| `sh_lines`    | `sh_lines(cmd) -> Array`     | Run command, return lines      | `sh_lines("ls")`                   |
| `sh_json`     | `sh_json(cmd)`               | Run command, parse JSON output | `sh_json("echo '[1]'")`            |
| `sh_ok`       | `sh_ok(cmd) -> Bool`         | Run command, return bool       | `sh_ok("which git")`               |
| `which`       | `which(cmd) -> String`       | Find command path              | `which("git")` → `"/usr/bin/git"`  |
| `cwd`         | `cwd() -> String`            | Current directory              | `cwd()` → `"/home/user"`           |
| `cd`          | `cd(path)`                   | Change directory               | `cd("/tmp")`                       |
| `pipe_to`     | `pipe_to(data, cmd)`         | Pipe string data into command  | `pipe_to(csv, "sort")`             |

### Assertion Functions

| Function    | Signature                     | Description                     | Example                         |
| ----------- | ----------------------------- | ------------------------------- | ------------------------------- |
| `assert`    | `assert(condition)`           | Fail if falsy                   | `assert(x > 0)`                 |
| `assert_eq` | `assert_eq(actual, expected)` | Fail if not equal               | `assert_eq(len(s), 5)`          |
| `satisfies` | `satisfies(value, predicate)` | Fail if predicate returns false | `satisfies(age, (a) => a >= 0)` |

### Standard Library Module Functions

Access via `module.function()` syntax after the module is available in scope:

**math** — `math.sqrt(x)`, `math.pow(x,n)`, `math.abs(x)`, `math.max(a,b)`, `math.min(a,b)`, `math.floor(x)`, `math.ceil(x)`, `math.round(x)`, `math.pi()`, `math.e()`, `math.sin(x)`, `math.cos(x)`, `math.tan(x)`, `math.log(x)`, `math.random()`

**fs** — `fs.read(path)`, `fs.write(path, data)`, `fs.append(path, data)`, `fs.exists(path)`, `fs.list(dir)`, `fs.remove(path)`, `fs.mkdir(path)`, `fs.copy(src, dst)`, `fs.rename(old, new)`, `fs.size(path)`, `fs.ext(path)`, `fs.read_json(path)`, `fs.write_json(path, data)`

**io** — `io.prompt(msg)`, `io.print(val)`, `io.args()`

**crypto** — `crypto.sha256(data)`, `crypto.md5(data)`, `crypto.base64_encode(data)`, `crypto.base64_decode(data)`, `crypto.hex_encode(data)`, `crypto.hex_decode(data)`

**db** — `db.open(path)`, `db.query(db, sql)`, `db.execute(db, sql)`, `db.close(db)`

**pg** — `pg.connect(url)`, `pg.query(conn, sql)`, `pg.execute(conn, sql)`, `pg.close(conn)`

**env** — `env.get(key)`, `env.set(key, val)`, `env.has(key)`, `env.keys()`

**json** — `json.parse(str)`, `json.stringify(val)`, `json.pretty(val)`

**regex** — `regex.test(pattern, str)`, `regex.find(pattern, str)`, `regex.find_all(pattern, str)`, `regex.replace(pattern, str, replacement)`, `regex.split(pattern, str)`

**log** — `log.info(msg)`, `log.warn(msg)`, `log.error(msg)`, `log.debug(msg)`

**http** — `http.get(url)`, `http.post(url, body)`, `http.put(url, body)`, `http.delete(url)`, `http.patch(url, body)`, `http.head(url)`, `http.download(url, path)`, `http.crawl(url)`

**csv** — `csv.parse(str)`, `csv.stringify(data)`, `csv.read(path)`, `csv.write(path, data)`

**term** — `term.red(str)`, `term.green(str)`, `term.blue(str)`, `term.yellow(str)`, `term.bold(str)`, `term.dim(str)`, `term.table(data)`, `term.hr()`, `term.sparkline(data)`, `term.bar(label, value, max)`, `term.banner(text)`, `term.countdown(seconds)`, `term.confirm(prompt)`

**exec** — `exec.run_command(cmd)`

---

## Appendix C: Operator Precedence Table

Operators are listed from lowest precedence (evaluated last) to highest precedence (evaluated first). Operators at the same precedence level are evaluated according to their associativity.

| Precedence   | Operator(s)                        | Description                      | Associativity  |
| ------------ | ---------------------------------- | -------------------------------- | -------------- |
| 1 (lowest)   | `\|>`                              | Pipeline                         | Left           |
| 2            | `\|\|`                             | Logical OR                       | Left           |
| 3            | `&&`                               | Logical AND                      | Left           |
| 4            | `==` `!=`                          | Equality                         | Left           |
| 5            | `<` `>` `<=` `>=`                  | Comparison                       | Left           |
| 6            | `+` `-`                            | Addition, Subtraction            | Left           |
| 7            | `*` `/` `%`                        | Multiplication, Division, Modulo | Left           |
| 8            | `-x` `!x`                          | Unary negation, NOT              | Right (prefix) |
| 8            | `must` `await`/`hold`              | Must-unwrap, Await               | Right (prefix) |
| 8            | `...` `freeze` `ask`               | Spread, Freeze, AI query         | Right (prefix) |
| 9            | `()`                               | Function call                    | Left (postfix) |
| 9            | `.`                                | Field access                     | Left (postfix) |
| 9            | `[]`                               | Index access                     | Left (postfix) |
| 9            | `?`                                | Try operator                     | Left (postfix) |
| 10 (highest) | Literals, identifiers, `()` groups | Primary                          | —              |

**Compound assignment operators** (`+=`, `-=`, `*=`, `/=`) are parsed as statements, not expressions. They desugar to `x = x op value` internally.

**The `>>` pipe operator** is parsed separately from `|>` and chains pipeline steps (`keep`, `sort`, `take`, `apply`).

---

## Appendix D: CLI Reference

### Synopsis

```
forge [OPTIONS] [COMMAND]
```

### Global Options

| Option              | Description                                                   |
| ------------------- | ------------------------------------------------------------- |
| `-e, --eval <CODE>` | Evaluate a Forge expression inline                            |
| `--vm`              | Use the bytecode VM (experimental, faster but fewer features) |
| `-h, --help`        | Print help information                                        |
| `-V, --version`     | Print version number                                          |

### Commands

#### `forge run <FILE>`

Run a Forge source file.

```bash
forge run main.fg
forge run main.fg --vm
```

| Argument | Description                 |
| -------- | --------------------------- |
| `FILE`   | Path to a `.fg` source file |

#### `forge repl`

Start the interactive REPL (Read-Eval-Print Loop). Also the default when no command is given.

```bash
forge repl
forge          # Same as forge repl
```

Features: command history, tab completion for keywords and built-ins, multi-line input.

#### `forge version`

Display version information.

```bash
forge version
# Output: Forge v0.2.0
#         Internet-native programming language
#         Bytecode VM with mark-sweep GC
```

#### `forge fmt [FILES...]`

Format Forge source files. With no arguments, formats all `.fg` files in the current directory recursively.

```bash
forge fmt                  # Format all .fg files
forge fmt main.fg          # Format specific file
forge fmt src/ lib/        # Format directories
```

#### `forge test [DIR]`

Run tests in the specified directory (default: `tests`).

```bash
forge test                 # Run tests in tests/
forge test integration     # Run tests in integration/
```

| Argument | Default | Description                     |
| -------- | ------- | ------------------------------- |
| `DIR`    | `tests` | Directory containing test files |

If a `forge.toml` exists, the `[test].directory` field overrides the default.

#### `forge new <NAME>`

Create a new Forge project with standard directory structure.

```bash
forge new my-api
```

| Argument | Description                                     |
| -------- | ----------------------------------------------- |
| `NAME`   | Project name (creates directory with this name) |

#### `forge build <FILE>`

Compile a Forge source file to bytecode and display compilation statistics.

```bash
forge build main.fg
```

| Argument | Description                    |
| -------- | ------------------------------ |
| `FILE`   | Path to source file to compile |

#### `forge install <SOURCE>`

Install a Forge package from a git URL or local path.

```bash
forge install https://github.com/user/package.git
forge install ../local-package
```

| Argument | Description                                         |
| -------- | --------------------------------------------------- |
| `SOURCE` | Git URL (https:// or git@) or local filesystem path |

#### `forge lsp`

Start the Language Server Protocol server for editor integration.

```bash
forge lsp
```

Communicates via stdin/stdout using JSON-RPC. Provides diagnostics and completions.

#### `forge learn [LESSON]`

Launch the interactive tutorial system.

```bash
forge learn          # List all 30 lessons
forge learn 1        # Start lesson 1 (Hello World)
forge learn 30       # Start lesson 30 (File Path Utilities)
```

| Argument | Description                   |
| -------- | ----------------------------- |
| `LESSON` | Optional lesson number (1–14) |

#### `forge chat`

Start an AI chat session for Forge programming assistance.

```bash
forge chat
```

Requires the `OPENAI_API_KEY` environment variable to be set.

#### `forge -e <CODE>`

Evaluate inline Forge code without creating a file.

```bash
forge -e 'say "hello!"'
forge -e 'println(math.sqrt(144))'
forge -e 'say range(5) |> map((x) => x * x)'
```

---

## Appendix E: Error Messages Guide

Forge produces clear, source-mapped error messages using the `ariadne` crate. This appendix catalogs common errors, explains what causes them, and shows how to fix them.

### Undefined Variable

```
error: undefined variable: naem
  ┌─ <source>:3:5
  │
3 │ say naem
  │     ^^^^ undefined variable: naem (did you mean: name?)
```

**Cause**: Using a variable that hasn't been declared in any accessible scope.

**Fix**: Check for typos. The "did you mean?" suggestion uses Levenshtein distance to find variables within an edit distance of 2. Ensure the variable is declared before use and is in scope.

```forge
// Wrong
say naem

// Right
set name to "Alice"
say name
```

### Unexpected Token

```
error: unexpected token: Semicolon
  ┌─ <source>:1:12
  │
1 │ let x = 42;
  │            ^ unexpected token: Semicolon
```

**Cause**: Forge uses newlines as statement terminators. Semicolons are recognized but not used as terminators in normal code.

**Fix**: Remove the semicolon. Forge does not require (or expect) semicolons at the end of statements.

```forge
// Wrong
let x = 42;

// Right
let x = 42
```

### Immutable Variable Reassignment

```
error: cannot reassign immutable variable 'count' (use 'let mut' to make it mutable)
  ┌─ <source>:2:1
  │
2 │ count = count + 1
  │ ^ cannot reassign immutable variable 'count'
```

**Cause**: Attempting to reassign a variable declared without `mut`.

**Fix**: Declare the variable as mutable:

```forge
// Wrong
let count = 0
count = count + 1

// Right
let mut count = 0
count = count + 1

// Or using natural syntax:
set mut count to 0
change count to count + 1
```

### Division by Zero

```
error: division by zero
  hint: check the divisor before dividing
  ┌─ <source>:1:9
  │
1 │ let x = 10 / 0
  │         ^^^^^^ division by zero
```

**Cause**: Dividing an integer or float by zero.

**Fix**: Guard against zero divisors:

```forge
if divisor != 0 {
    let result = value / divisor
} otherwise {
    say "Cannot divide by zero"
}
```

### Type Mismatch (Warning)

```
warning: type mismatch: expected Int, got String
  ┌─ <source>:2:10
  │
2 │ let x: Int = "hello"
  │              ^^^^^^^ expected Int
```

**Cause**: A type annotation doesn't match the assigned value. Forge uses gradual typing—type annotations are checked but violations produce warnings, not errors.

**Fix**: Either correct the value or remove/update the type annotation:

```forge
// Option 1: fix the value
let x: Int = 42

// Option 2: fix the annotation
let x: String = "hello"

// Option 3: remove the annotation
let x = "hello"
```

### Cannot Call on Type

```
error: cannot call value of type String
  ┌─ <source>:3:1
  │
3 │ name(42)
  │ ^^^^ cannot call value of type String
```

**Cause**: Attempting to call a value that is not a function, lambda, or built-in.

**Fix**: Ensure the identifier refers to a callable value:

```forge
// Wrong
let name = "Alice"
name(42)  // name is a String, not a function

// Right
fn greet(name) { say "Hello, {name}" }
greet("Alice")
```

### Index Out of Bounds

```
error: index out of bounds: index 5, length 3
  ┌─ <source>:2:1
  │
2 │ list[5]
  │ ^^^^^^^ index out of bounds: index 5, length 3
```

**Cause**: Accessing an array element at an index beyond its length.

**Fix**: Check the array length before indexing, or use a safe access pattern:

```forge
let list = [10, 20, 30]

// Guard with length check
if index < len(list) {
    say list[index]
}

// Or use safe block
safe {
    let val = list[index]
    say val
}
```

### Unterminated String

```
error: unterminated string (newline in string literal)
  ┌─ <source>:1:11
  │
1 │ let x = "hello
  │               ^ unterminated string
```

**Cause**: A string literal contains an unescaped newline or reaches end-of-file without a closing quote.

**Fix**: Close the string on the same line, use `\n` for embedded newlines, or use a triple-quoted string for multi-line text:

```forge
// Single-line string with escape
let x = "hello\nworld"

// Multi-line with triple quotes
let x = """
hello
world
"""
```

### Unknown Escape Sequence

```
error: unknown escape: \q
  ┌─ <source>:1:12
  │
1 │ let x = "\q"
  │            ^ unknown escape: \q
```

**Cause**: Using an escape character that Forge doesn't recognize.

**Fix**: Use one of the supported escape sequences: `\n`, `\t`, `\r`, `\\`, `\"`, `\{`, `\}`.

### Break/Continue Outside Loop

```
error: break outside of loop
  ┌─ <source>:1:1
  │
1 │ break
  │ ^^^^^ break outside of loop
```

**Cause**: Using `break` or `continue` outside of a `for`, `while`, `loop`, or `repeat` block.

**Fix**: Ensure these keywords only appear inside loop bodies.

---

## Appendix F: Forge vs. Other Languages

This appendix provides detailed comparison tables showing how Forge stacks up against popular languages. These comparisons highlight where Forge simplifies common tasks, where it innovates, and where other languages may be more appropriate.

### Table F-1: Forge vs. Python

| Feature              | Forge                                       | Python                                  |
| -------------------- | ------------------------------------------- | --------------------------------------- |
| Variable declaration | `let x = 5` / `set x to 5`                  | `x = 5`                                 |
| Immutability         | Built-in: `let` (immutable by default)      | Convention only (UPPER_CASE)            |
| Type annotations     | Optional: `let x: Int = 5`                  | Optional: `x: int = 5`                  |
| Function definition  | `fn add(a, b) { }` / `define add(a, b) { }` | `def add(a, b):`                        |
| Lambda               | `(x) => x * 2`                              | `lambda x: x * 2`                       |
| String interpolation | `"Hello, {name}"`                           | `f"Hello, {name}"`                      |
| Print                | `say "hello"` / `println("hello")`          | `print("hello")`                        |
| HTTP GET             | `fetch("url")` / `grab data from "url"`     | `requests.get("url")` (external lib)    |
| HTTP server          | Built-in: `@server` + `@get` decorators     | Flask/FastAPI (external)                |
| Pattern matching     | `match x { 1 => "one" }`                    | `match x: case 1: "one"` (3.10+)        |
| Error handling       | `try/catch`, `must`, `safe`, `?` operator   | `try/except`                            |
| Null safety          | `safe { }` blocks, `?` operator             | No built-in null safety                 |
| Concurrency          | `spawn { }`, `forge fn() { }`               | `asyncio`, threading                    |
| Package install      | `forge install <url>`                       | `pip install <pkg>`                     |
| Test runner          | Built-in: `@test` + `forge test`            | `pytest` (external)                     |
| Formatter            | Built-in: `forge fmt`                       | `black` (external)                      |
| REPL                 | Built-in: `forge repl`                      | Built-in: `python`                      |
| Database access      | Built-in: `db.query()`, `pg.query()`        | `sqlite3`, `psycopg2` (stdlib/external) |
| Retry logic          | `retry 3 times { }`                         | Manual loop or `tenacity` library       |
| AI integration       | Built-in: `ask "prompt"`                    | `openai` library (external)             |
| Learning mode        | Built-in: `forge learn`                     | None built-in                           |
| Semicolons           | Not required (newline-based)                | Not required (newline-based)            |

### Table F-2: Forge vs. JavaScript/Node.js

| Feature              | Forge                                     | JavaScript/Node.js                  |
| -------------------- | ----------------------------------------- | ----------------------------------- |
| Variable declaration | `let x = 5`                               | `let x = 5` / `const x = 5`         |
| Immutability         | `let` is immutable, `let mut` is mutable  | `const` is shallow-immutable        |
| Function definition  | `fn greet(name) { }`                      | `function greet(name) { }`          |
| Arrow functions      | `(x) => x * 2`                            | `(x) => x * 2`                      |
| String interpolation | `"Hello, {name}"`                         | `` `Hello, ${name}` ``              |
| Destructuring        | `unpack {a, b} from obj`                  | `const {a, b} = obj`                |
| Spread operator      | `...args`                                 | `...args`                           |
| Pipeline operator    | `data \|> transform`                      | Stage 2 proposal (TC39)             |
| HTTP server          | `@server` + `@get fn()`                   | Express/Fastify (external)          |
| HTTP client          | `fetch("url")` (built-in)                 | `fetch()` (built-in, Node 18+)      |
| Async/await          | `async fn` / `forge fn`, `await` / `hold` | `async function`, `await`           |
| Error handling       | `try/catch` + `must` + `safe` + `?`       | `try/catch`                         |
| Null safety          | `safe { }` blocks                         | Optional chaining `?.`              |
| Type system          | Gradual (optional annotations)            | None (use TypeScript)               |
| Pattern matching     | `match value { pattern => body }`         | None (proposal stage)               |
| Package manager      | `forge install`                           | `npm install`                       |
| Test framework       | Built-in `@test`                          | Jest/Vitest (external)              |
| Formatter            | Built-in `forge fmt`                      | Prettier (external)                 |
| Module system        | `import "file.fg"`                        | `import`/`require`                  |
| Database             | Built-in SQLite + PostgreSQL              | `better-sqlite3`, `pg` (external)   |
| Integers             | True 64-bit integers                      | `Number` (64-bit float) or `BigInt` |
| Counted loops        | `repeat 5 times { }`                      | `for (let i=0; i<5; i++) { }`       |
| AI built-in          | `ask "prompt"`                            | None built-in                       |
| Compilation          | Single binary, instant start              | V8 JIT, ~50ms startup               |

### Table F-3: Forge vs. Go

| Feature              | Forge                                 | Go                                        |
| -------------------- | ------------------------------------- | ----------------------------------------- |
| Variable declaration | `let x = 5`                           | `x := 5` / `var x int = 5`                |
| Function definition  | `fn add(a, b) { return a + b }`       | `func add(a, b int) int { return a + b }` |
| Type system          | Gradual (optional)                    | Static (required)                         |
| Error handling       | `try/catch`, `must`, `safe`, `Result` | `if err != nil { return err }`            |
| HTTP server          | `@server` + decorator-based routing   | `http.HandleFunc` / gorilla mux           |
| HTTP client          | `fetch("url")` (one function)         | `http.Get()` + response body handling     |
| Concurrency          | `spawn { }`                           | `go func() { }()`                         |
| Generics             | Dynamic typing                        | Generics (Go 1.18+)                       |
| Compilation          | Interpreted (or bytecode VM)          | Compiled to native binary                 |
| Performance          | Interpreted speed                     | Near-C performance                        |
| String interpolation | `"Hello, {name}"`                     | `fmt.Sprintf("Hello, %s", name)`          |
| Pattern matching     | `match` / `when` guards               | `switch` statement                        |
| Null handling        | `safe { }`, `None`/`Some`             | Nil checks                                |
| Package management   | `forge install`                       | `go get` + `go mod`                       |
| Test framework       | Built-in `@test`                      | Built-in `testing` package                |
| REPL                 | Built-in                              | None                                      |
| AI integration       | Built-in `ask`                        | External library                          |
| Learning curve       | Low (designed for readability)        | Low (25 keywords)                         |
| Binary size          | ~15MB (Rust compiled)                 | ~5-10MB per binary                        |
| Ecosystem maturity   | New, growing                          | Mature, large ecosystem                   |

### Table F-4: Forge vs. Rust

| Feature              | Forge                               | Rust                             |
| -------------------- | ----------------------------------- | -------------------------------- |
| Type system          | Gradual, dynamic                    | Static, strict                   |
| Memory management    | GC (VM) / clone-based (interpreter) | Ownership + borrowing            |
| Error handling       | `try/catch`, `must`, `safe`         | `Result<T,E>`, `?` operator      |
| Null handling        | `Null` value + `safe` blocks        | `Option<T>`, no null             |
| Compilation speed    | Instant (interpreted)               | Slow (full compile)              |
| Runtime performance  | Interpreted speed                   | Native speed                     |
| Learning curve       | Low                                 | High                             |
| String interpolation | `"Hello, {name}"`                   | `format!("Hello, {name}")`       |
| HTTP server          | 3 lines (`@server`, `@get fn`)      | ~30 lines (axum setup)           |
| HTTP client          | `fetch("url")`                      | `reqwest::get("url").await?`     |
| Concurrency          | `spawn { }`                         | `tokio::spawn(async { })`        |
| Pattern matching     | `match value { }`                   | `match value { }`                |
| Closures             | `(x) => x * 2` (captures env)       | `\|x\| x * 2` (lifetime-tracked) |
| Package manager      | `forge install`                     | `cargo add`                      |
| Unsafe code          | Zero `unsafe` in Forge itself       | Powerful but dangerous           |

### Table F-5: Forge vs. Ruby

| Feature              | Forge                                     | Ruby                         |
| -------------------- | ----------------------------------------- | ---------------------------- |
| Variable declaration | `let x = 5` / `set x to 5`                | `x = 5`                      |
| Function definition  | `fn greet(n) { }` / `define greet(n) { }` | `def greet(n) ... end`       |
| Blocks               | `{ }` braces                              | `do...end` or `{ }`          |
| String interpolation | `"Hello, {name}"`                         | `"Hello, #{name}"`           |
| Functional methods   | `map`, `filter`, `reduce`                 | `map`, `select`, `reduce`    |
| HTTP server          | Built-in `@server` decorators             | Sinatra/Rails (external)     |
| HTTP client          | Built-in `fetch()`                        | `net/http` or `httparty`     |
| Type annotations     | Optional: `let x: Int = 5`                | None (use Sorbet)            |
| Pattern matching     | `match value { }`                         | `case value in` (Ruby 3.0+)  |
| Error handling       | `try/catch`, `must`, `safe`               | `begin/rescue/end`           |
| Test framework       | Built-in `@test`                          | RSpec / Minitest             |
| REPL                 | Built-in `forge repl`                     | Built-in `irb`               |
| Null safety          | `safe { }` blocks                         | `&.` safe navigation         |
| Natural syntax       | `say`, `define`, `otherwise`              | Ruby is already English-like |
| AI integration       | Built-in `ask "prompt"`                   | None built-in                |

### Lines of Code Comparison: Common Tasks

#### HTTP Server with JSON Endpoint

**Forge (8 lines)**:

```forge
@server(port: 3000)

@get("/hello/:name")
fn hello(params) {
    return {
        message: "Hello, {params.name}!"
    }
}
```

**Python + Flask (9 lines)**:

```python
from flask import Flask, jsonify
app = Flask(__name__)

@app.route("/hello/<name>")
def hello(name):
    return jsonify(message=f"Hello, {name}!")

if __name__ == "__main__":
    app.run(port=3000)
```

**Go (19 lines)**:

```go
package main
import (
    "encoding/json"
    "fmt"
    "net/http"
)
func hello(w http.ResponseWriter, r *http.Request) {
    name := r.URL.Path[len("/hello/"):]
    json.NewEncoder(w).Encode(map[string]string{
        "message": fmt.Sprintf("Hello, %s!", name),
    })
}
func main() {
    http.HandleFunc("/hello/", hello)
    http.ListenAndServe(":3000", nil)
}
```

#### Read File + Process Lines

**Forge (4 lines)**:

```forge
let lines = split(fs.read("data.txt"), "\n")
let non_empty = filter(lines, (l) => len(l) > 0)
say "Found {len(non_empty)} non-empty lines"
```

**Python (4 lines)**:

```python
with open("data.txt") as f:
    lines = f.readlines()
non_empty = [l for l in lines if l.strip()]
print(f"Found {len(non_empty)} non-empty lines")
```

**JavaScript (5 lines)**:

```javascript
const fs = require("fs");
const lines = fs.readFileSync("data.txt", "utf8").split("\n");
const nonEmpty = lines.filter((l) => l.trim().length > 0);
console.log(`Found ${nonEmpty.length} non-empty lines`);
```

#### Database Query

**Forge (4 lines)**:

```forge
let conn = db.open("app.db")
let users = db.query(conn, "SELECT name, age FROM users WHERE age > 21")
say users
db.close(conn)
```

**Python (7 lines)**:

```python
import sqlite3
conn = sqlite3.connect("app.db")
cursor = conn.cursor()
users = cursor.execute("SELECT name, age FROM users WHERE age > 21").fetchall()
print(users)
conn.close()
```

**Go (15 lines)**:

```go
db, _ := sql.Open("sqlite3", "app.db")
defer db.Close()
rows, _ := db.Query("SELECT name, age FROM users WHERE age > 21")
defer rows.Close()
for rows.Next() {
    var name string; var age int
    rows.Scan(&name, &age)
    fmt.Printf("%s: %d\n", name, age)
}
```

---

## Appendix G: Project Statistics and Credits

### Codebase Statistics

| Metric                       | Value   |
| ---------------------------- | ------- |
| Total Rust source lines      | ~15,500 |
| Total source files           | 45      |
| Rust tests                   | 189     |
| Forge integration tests      | 25      |
| Unsafe blocks                | 0       |
| Keywords recognized          | 80+     |
| Built-in functions           | 160+    |
| Standard library modules     | 15      |
| CLI commands                 | 13      |
| Interactive tutorial lessons | 14      |
| Example programs             | 10+     |

### Largest Source Files

| File                     | Lines | Component                |
| ------------------------ | ----- | ------------------------ |
| `src/interpreter/mod.rs` | 4,584 | Tree-walk interpreter    |
| `src/parser/parser.rs`   | 1,808 | Recursive descent parser |
| `src/vm/machine.rs`      | 1,807 | Bytecode VM engine       |
| `src/vm/compiler.rs`     | 772   | AST to bytecode compiler |
| `src/lexer/lexer.rs`     | 606   | Lexer / tokenizer        |
| `src/stdlib/term.rs`     | 478   | Terminal UI module       |
| `src/runtime/server.rs`  | 352   | HTTP server (axum)       |
| `src/parser/ast.rs`      | 335   | AST definitions          |
| `src/repl/mod.rs`        | 299   | Interactive REPL         |
| `src/main.rs`            | 293   | CLI entry point          |

### Technology Stack

| Component       | Technology         | Purpose                           |
| --------------- | ------------------ | --------------------------------- |
| Language        | Rust               | Core implementation               |
| CLI framework   | clap               | Argument parsing, subcommands     |
| HTTP server     | axum               | Async HTTP server runtime         |
| HTTP client     | reqwest + rustls   | HTTPS requests (pure Rust TLS)    |
| Async runtime   | tokio              | Async I/O, task scheduling        |
| SQLite          | rusqlite           | Embedded database support         |
| PostgreSQL      | tokio-postgres     | PostgreSQL client                 |
| Error reporting | ariadne            | Source-mapped error diagnostics   |
| REPL            | rustyline          | Line editing, history, completion |
| Ordered maps    | indexmap           | Insertion-order-preserving maps   |
| JSON            | serde + serde_json | JSON parsing and serialization    |
| TOML            | toml               | Manifest file parsing             |
| CORS            | tower-http         | Cross-Origin Resource Sharing     |
| Regex           | regex              | Regular expression engine         |
| UUID            | uuid               | UUID v4 generation                |
| Crypto          | sha2, md5, base64  | Cryptographic hash functions      |
| CSV             | csv                | CSV parsing and writing           |

### Design Principles

1. **Zero unsafe code** — The entire codebase uses safe Rust. No `unsafe` blocks, no raw pointer manipulation, no undefined behavior.

2. **Batteries included** — HTTP client and server, database access, cryptography, file I/O, regex, CSV, JSON, terminal UI, and AI integration ship with the language. No dependency hunting for common tasks.

3. **Dual syntax** — Every construct can be expressed in either classic programming syntax or natural English-like syntax. Both are first-class; neither is deprecated or secondary.

4. **Progressive complexity** — Simple programs are simple. `say "hello"` is a complete program. Advanced features (async, HTTP servers, database queries) are available but never required.

5. **Developer experience first** — Built-in formatter, test runner, REPL, tutorials, LSP, and project scaffolding. The toolchain is complete from day one.

### Version History

| Version | Milestone                                                                                                       |
| ------- | --------------------------------------------------------------------------------------------------------------- |
| 0.1.0   | Initial release: lexer, parser, interpreter, 8 stdlib modules                                                   |
| 0.2.0   | Bytecode VM, mark-sweep GC, 15 stdlib modules, LSP, tutorials, AI chat, formatter, test runner, package manager |

### Acknowledgments

Forge is written by **Archith Rapaka**. The language draws inspiration from:

- **Python** for its readability and gentle learning curve
- **Go** for its simplicity and built-in tooling philosophy
- **Rust** for its safety guarantees and error handling patterns
- **Ruby** for its expressive, human-friendly syntax
- **JavaScript** for its object literal syntax and async patterns
- **Lua** for its register-based VM design
- **Swift** for its optional handling and guard statements

Special thanks to the Rust ecosystem for the excellent crates that power Forge's runtime: `tokio`, `axum`, `reqwest`, `rusqlite`, `ariadne`, `clap`, `rustyline`, `serde`, and `indexmap`.

---

_This concludes Part IV and the Appendices of_ Programming Forge*. The complete source code is available at the project repository. Contributions, bug reports, and feature requests are welcome.*
