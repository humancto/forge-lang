# Execution Model

Forge provides three execution tiers, selectable at the command line. All tiers accept the same source files; they differ in feature coverage, performance, and implementation strategy.

## Three Tiers

| Tier         | Flag      | Implementation        | Performance                      | Feature Coverage |
| ------------ | --------- | --------------------- | -------------------------------- | ---------------- |
| Interpreter  | (default) | Tree-walking          | Baseline                         | Full (100%)      |
| Bytecode VM  | `--vm`    | Register-based VM     | ~10x faster than interpreter     | Partial (~60%)   |
| JIT Compiler | `--jit`   | Cranelift native code | ~50-100x faster than interpreter | Minimal (~30%)   |

```bash
forge run program.fg          # Interpreter (default)
forge run program.fg --vm     # Bytecode VM
forge run program.fg --jit    # JIT compiler
```

## When to Use Each Tier

### Interpreter (Default)

Use the interpreter for:

- All general-purpose development
- HTTP servers (`@server`, `@get`, `@post`)
- Database access (`db`, `pg`)
- AI integration (`ask`)
- File system, crypto, terminal UI
- Any code using the full standard library

The interpreter supports every feature of the language. It is the reference implementation.

### Bytecode VM (`--vm`)

Use the VM for:

- Compute-intensive loops and numerical work
- Programs that primarily use `math`, `fs`, `io`, and basic control flow
- Benchmarking against the interpreter

The VM compiles Forge source to a register-based bytecode and executes it in a virtual machine with mark-sweep garbage collection. It does not support HTTP servers, database connections, or several stdlib modules.

### JIT Compiler (`--jit`)

Use the JIT for:

- Maximum performance on hot numerical code
- Benchmarking (e.g., `fib(30)` runs 11x faster than Python)
- Functions that are purely computational

The JIT compiles hot functions to native machine code via Cranelift. It supports the smallest subset of the language -- primarily arithmetic, function calls, and basic control flow.

## Trade-off Summary

```
Feature Coverage:  Interpreter > VM > JIT
Performance:       JIT > VM > Interpreter
Startup Time:      Interpreter < VM < JIT
```

The interpreter is always the safe default. Switch to `--vm` or `--jit` only when you need the performance and have verified your program works on that tier.
