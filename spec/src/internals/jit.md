# JIT Compiler

The Forge JIT compiler translates hot bytecode functions into native machine code using the **Cranelift** code generator. It provides the highest performance tier, achieving approximately 11x faster execution than Python on recursive benchmarks like `fib(30)`.

## Invocation

```bash
forge run program.fg --jit
```

## Architecture

```
Source -> Lexer -> Parser -> AST -> Compiler -> Bytecode -> JIT -> Native Code
                                                             |
                                                      Cranelift IR
                                                             |
                                                    Machine Code (x86_64/AArch64)
```

Key source files:

- `src/vm/jit/ir_builder.rs` (~276 lines) -- Bytecode to Cranelift IR translation
- `src/vm/jit/jit_module.rs` (~47 lines) -- JIT module management

## How It Works

1. The program is first compiled to bytecode (same as the `--vm` path).
2. Functions selected for JIT compilation are translated from bytecode into Cranelift's intermediate representation (IR).
3. Cranelift compiles the IR to native machine code for the host architecture.
4. The native code is loaded into memory and called directly, bypassing the bytecode interpreter.

## Cranelift IR Translation

The IR builder walks the bytecode instruction stream and emits Cranelift IR operations:

- Arithmetic bytecodes (`ADD`, `SUB`, `MUL`, `DIV`) map to Cranelift `iadd`, `isub`, `imul`, `sdiv`.
- Comparison bytecodes map to Cranelift `icmp` with the appropriate condition.
- Jump bytecodes map to Cranelift branch and block terminators.
- Function calls generate Cranelift `call` instructions.

### Jump Offset Encoding

The VM pre-increments the instruction pointer (IP) before applying jump offsets. When translating jumps to Cranelift blocks, the JIT target is calculated as:

```
target = ip + 1 + sbx
```

This is a critical detail. Using `ip + sbx` produces incorrect branch targets.

## Performance

On `fib(30)`:

| Engine            | Time     | Relative |
| ----------------- | -------- | -------- |
| Python 3          | ~330ms   | 1x       |
| Forge interpreter | ~6,600ms | 0.05x    |
| Forge VM          | ~660ms   | 0.5x     |
| Forge JIT         | ~30ms    | 11x      |

The JIT excels at tight numerical loops and recursive functions where the overhead of interpretation dominates.

## Supported Features

The JIT supports the most restricted subset:

- Integer and float arithmetic
- Function calls and recursion
- Basic control flow (if/else, loops)
- Local variables
- Comparisons and boolean logic

## Limitations

- No string operations
- No object or array construction
- No standard library access
- No closures or higher-order functions
- No error handling (try/catch, Result)
- Compilation overhead makes it unsuitable for short-running programs
- Only beneficial for compute-heavy inner loops

## Platform Support

Cranelift supports:

- x86_64 (macOS, Linux, Windows)
- AArch64 / ARM64 (macOS Apple Silicon, Linux)
