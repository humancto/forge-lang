# Bytecode VM

The Forge bytecode VM is a **register-based virtual machine** that compiles Forge source to 32-bit instructions and executes them in a loop. It provides significantly better performance than the tree-walking interpreter at the cost of reduced feature coverage.

## Invocation

```bash
forge run program.fg --vm
```

## Architecture

```
Source -> Lexer -> Parser -> AST -> Compiler -> Bytecode Chunks -> Machine -> Result
                                                                     |
                                                              Mark-Sweep GC
                                                                     |
                                                              Green Threads
```

Key source files:

- `src/vm/compiler.rs` (~927 lines) -- AST to bytecode compilation
- `src/vm/machine.rs` (~2,483 lines) -- bytecode execution engine
- `src/vm/bytecode.rs` -- instruction set definition
- `src/vm/gc.rs` -- mark-sweep garbage collector
- `src/vm/frame.rs` -- call frame management
- `src/vm/value.rs` -- VM-specific value type
- `src/vm/green.rs` -- green thread scheduler

## Instruction Encoding

All instructions are 32 bits wide. Three encoding formats:

### ABC Format: `[op:8][a:8][b:8][c:8]`

Used for register-to-register operations. `a` is typically the destination register; `b` and `c` are source registers.

### ABx Format: `[op:8][a:8][bx:16]`

Used for instructions with a larger operand, such as constant loading. `bx` is an unsigned 16-bit index.

### AsBx Format: `[op:8][a:8][sbx:16]`

Used for jump instructions. `sbx` is a signed 16-bit offset stored as unsigned (with bias).

> **Important:** The VM pre-increments IP before applying jump offsets. The JIT target address is `ip + 1 + sbx`, not `ip + sbx`.

## Register Machine

The VM uses a register-based architecture rather than a stack-based one. Each call frame has its own register window. Registers are addressed by 8-bit indices, allowing up to 256 registers per frame.

Benefits:

- Fewer instructions than a stack VM (no push/pop for every operand)
- Better cache locality for register access
- Natural fit for the JIT tier

## Constant Pool

Each compiled function (called a "Chunk") has a constant pool for literals, strings, and function prototypes. Constants are deduplicated via `identical()` comparison to avoid wasting pool slots.

## Garbage Collection

The VM uses a **mark-sweep garbage collector**. Heap-allocated objects (strings, arrays, objects, closures) are tracked by the GC. Collection is triggered when the allocation count exceeds a threshold.

The mark phase walks from GC roots (registers, global environment, call stack). The sweep phase frees unreachable objects.

## Green Threads

The VM includes a cooperative green thread scheduler (`src/vm/green.rs`). Green threads are multiplexed over a single OS thread with explicit yield points.

## Supported Features

The VM supports core language features:

- Variables, functions, closures
- Control flow (if/else, for, while, match)
- Arrays and objects
- Arithmetic and comparison operators
- String operations
- `math`, `fs`, `io`, `npc` modules
- Basic built-in functions

## Unsupported Features

The following require the interpreter:

- HTTP server and client
- Database connections
- AI integration
- Terminal UI widgets
- Most execution helpers
