# RFC 0005: Compilation Evolution — Interpreter to Native Binaries

- **Status:** Accepted
- **Created:** 2026-02-27

## Summary

This RFC defines the technical path from Forge's current tree-walk interpreter to producing standalone native binaries via AOT compilation. The evolution is progressive: each stage builds on the previous one, and all execution modes remain available.

## Motivation

Forge currently has three execution modes:

1. **Tree-walk interpreter** — walks the AST directly. Full feature support, slowest execution.
2. **Bytecode VM** — compiles AST to register-based bytecode, executes on a virtual machine. Faster than tree-walk, partial feature coverage.
3. **JIT** — compiles integer-only bytecode to native code via Cranelift. Fastest for supported functions, very limited scope.

The end goal is a fourth mode:

4. **AOT native compilation** — compiles Forge source to a standalone binary. No runtime needed. Near-Rust performance.

This requires solving four problems in sequence:

- Bytecode must be serializable (so it can be saved and loaded)
- The JIT must handle all value types (not just integers)
- Values must have a uniform in-memory representation (NaN-boxing)
- A runtime library must exist for native code to call into

## Design

### Stage 1: Bytecode Persistence

The bytecode `Chunk` structure must be serializable to disk.

**Format:** Custom binary format with magic bytes `FGC\0`, version header, and recursive chunk encoding. LEB128 for variable-length integers. Length-prefixed UTF-8 for strings.

**Why not WASM/ELF directly?** The bytecode format is Forge's intermediate representation. It's simpler than WASM, carries Forge-specific metadata (decorators, debug info), and can be interpreted or AOT-compiled.

```
.fg source → compiler → .fgc bytecode → VM (interpret)
                                       → Cranelift (AOT compile)
```

### Stage 2: NaN-Boxing

Current state: The VM uses a Rust enum for values. Each value is 40+ bytes (enum discriminant + largest variant). The JIT uses raw `i64` for integers only.

**Problem:** Native code needs a uniform 64-bit value that fits in a CPU register and can represent any Forge type without branching on a tag byte.

**Solution:** NaN-boxing. IEEE 754 doubles have a special NaN range. We use the unused bits in NaN payloads to encode non-float values:

```
If the top 12 bits are 0x7FFC or higher → it's a tagged value:
  Bits 50-48 (tag):
    000 = pointer to heap object (48-bit address space)
    001 = integer (48-bit signed)
    010 = boolean (bit 0)
    011 = null
  Bits 47-0 (payload): value or pointer

Otherwise → it's a regular IEEE 754 double
```

**Implications:**

- Integers are limited to 48 bits (±140 trillion). Overflow promotes to heap-allocated big int.
- Floats are free — no encoding/decoding.
- Pointer values use the lower 48 bits (sufficient for current architectures).

### Stage 3: Runtime Library (libforge)

Native-compiled Forge code cannot contain the entire interpreter. Instead, complex operations call into a runtime library:

```rust
// libforge provides these as extern "C" functions:
extern "C" fn forge_println(value: u64);           // NaN-boxed value
extern "C" fn forge_string_concat(a: u64, b: u64) -> u64;
extern "C" fn forge_array_new(count: u32) -> u64;
extern "C" fn forge_fetch(url: u64) -> u64;        // HTTP fetch
extern "C" fn forge_gc_alloc(size: usize) -> *mut u8;
extern "C" fn forge_server_start(port: u16, routes: *const Route);
```

**Trade-off:** Operations that are simple (arithmetic, comparison, jumps) compile to inline native code. Operations that are complex (string formatting, HTTP, GC allocation) call into the runtime. This is the same approach Go and OCaml use.

### Stage 4: AOT Compilation

The `ir_builder.rs` logic extends from JIT to AOT:

**JIT path (current):**

```
Chunk → Cranelift IR → JITModule → function pointer in memory → call
```

**AOT path (new):**

```
Chunk → Cranelift IR → ObjectModule → .o file → link with libforge.a → binary
```

The Cranelift API is nearly identical between `cranelift-jit` and `cranelift-object`. The IR generation code is shared. The difference is the output target.

**Generated binary structure:**

```
┌─────────────────────────┐
│ main()                  │  Generated: calls forge_init, runs top-level code
│ user_function_1()       │  Generated: compiled Forge function
│ user_function_2()       │  Generated: compiled Forge function
│ ...                     │
├─────────────────────────┤
│ libforge.a              │  Linked: runtime library
│   forge_println()       │
│   forge_fetch()         │
│   forge_gc_alloc()      │
│   forge_server_start()  │
│   ...                   │
└─────────────────────────┘
```

### GC in Native Code

The mark-sweep GC must know which registers hold heap pointers at GC safe points. This requires **stack maps** — metadata emitted alongside native code that tells the GC where to look.

**Safe points:** Function calls, allocation sites, loop back-edges.

**Implementation:** Cranelift supports emitting stack maps via `cranelift_codegen::ir::StackMap`. At each safe point, we record which registers contain GC-managed pointers.

## Alternatives Considered

### LLVM instead of Cranelift

LLVM produces better optimized code but has 10-100x slower compile times. Cranelift is designed for JIT/AOT with fast compilation. Since Forge already uses Cranelift, switching would be costly for marginal gain.

### Compile to C

Emit C code and use GCC/Clang. Simpler than managing Cranelift IR directly. Downside: slow compilation, harder to integrate JIT, dependency on C compiler.

### Compile to Go

Emit Go code and use `go build`. Would get goroutines for free. Downside: additional dependency, Forge would be tied to Go's runtime and GC.

### Skip native, just improve the VM

Focus on making the bytecode VM faster (register allocation, inline caching, hidden classes). This gets 5-10x improvement without the complexity of native compilation. However, it caps out well below what native code achieves.

## Migration Path

All execution modes coexist. Users choose:

```
forge run app.fg              # Interpreter (always available)
forge run --vm app.fg         # Bytecode VM
forge run --jit app.fg        # JIT (auto-promotes hot functions)
forge build app.fg            # Produces .fgc bytecode file
forge build --native app.fg   # Produces standalone binary
```

No mode is ever removed. The interpreter remains the reference implementation for correctness testing.
