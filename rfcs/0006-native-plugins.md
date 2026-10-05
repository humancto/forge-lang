# RFC 0006: Native Plugins (Calling Rust and C from Forge)

- **Status:** Implemented
- **Author:** Archith Rapaka
- **Date:** 2026-10-05

## Summary

A Forge program can load a shared library (`.so` / `.dylib` / `.dll`) and
call the functions it exports with typed values:

```forge
import native "target/release/libimage_tools" as img
say img.resize("in.png", 640)

import { add, greet } from native "plugins/libhello"
say add(1, 2)
```

The library implements a small, versioned **C ABI** (not the Rust ABI), so
it can be written in Rust (with the `forge-plugin` SDK crate in
`crates/forge-plugin`) or in any language that can export C functions
(`crates/forge-plugin/include/forge_plugin.h`). Loading native code is
gated by a new capability, `ffi`, which is denied by default for
`forge run`, under `--sandbox`, in `forge mcp` and in embedded sandboxes.

## Motivation

Today the only way to reach native code from Forge is to shell out
(`sh("mytool ...")`) and parse text. That is slow (a process per call),
lossy (everything is a string) and needs `--allow-run`. Users who need a
fast hash, an image codec, a vendor SDK or a numeric kernel should be able
to write it once in Rust or C and call it as a function, with ints, floats,
strings, arrays and objects crossing the boundary as values.

## Design

### Forge surface

The statement extends the existing `import` grammar; there is no new
keyword (`native` and `as` are contextual):

```text
import native "<path>"                      # namespace named after the file
import native "<path>" as <ident>           # namespace with an explicit name
import { name, ... } from native "<path>"   # bind selected functions
```

* **Namespace form.** Binds one name to an object whose members are the
  library's functions: `img.resize(...)`. Without `as`, the name is the
  file stem with the platform prefix and suffix removed
  (`libhello_rust.so` → `hello_rust`); a stem that is not an identifier is
  a parse error asking for `as`.
* **Named form.** Binds each listed function directly. Asking for a
  function the library does not export is an error that lists what it does
  export.
* All bound names are known at parse time, so the VM compiler, the LSP and
  the type checker resolve them statically, exactly like
  `import { x } from "file.fg"`. Nothing is loaded at compile time.

**Path resolution** (shared by both engines, `plugins::resolve_library`):
relative paths are tried against the importing file's directory first,
then the working directory, as for `.fg` imports. If the path has no
platform library extension, the platform suffix is appended
(`.so`, `.dylib`, `.dll`), and the platform prefix is tolerated either way
(`hello` finds `libhello.so`; on Windows `libhello` finds `hello.dll`).
There is deliberately **no search path** (`LD_LIBRARY_PATH`, system
directories, `forge_modules`): a plugin is always a file the program names,
so a planted library elsewhere cannot be picked up.

**Values.** A plugin function is an ordinary callable builtin value: it can
be stored, passed to `map`, called from `spawn` blocks, and so on. Calls
check arity (`add() expects 2 arguments, got 3`), convert the arguments,
run the function, convert the result back, and turn a plugin error into a
normal Forge runtime error (catchable with `try`/`catch`, `safe`, `must`).

### ABI (version 1)

Normative definition: `crates/forge-plugin/include/forge_plugin.h`.
Mirrored, with layout tests, in `src/plugins/abi.rs` (host) and
`crates/forge-plugin/src/abi.rs` (SDK).

A library exports two C symbols:

```c
uint32_t forge_plugin_abi_version(void);          // must return 1
const ForgePlugin *forge_plugin_register(void);   // static descriptor
```

The host calls `forge_plugin_abi_version` **first** and refuses the
library (without calling anything else) when it is not the version the
host implements, so a future ABI can change every other structure.

```c
typedef struct ForgePlugin {
    uint32_t abi_version;               // FORGE_PLUGIN_ABI_VERSION
    const char *name;                   // NUL-terminated UTF-8
    const char *version;                // NUL-terminated UTF-8
    const ForgeFunction *functions;     // function_count entries
    size_t function_count;
    void (*free_value)(ForgeValue *v);  // frees values the plugin returned
} ForgePlugin;

typedef struct ForgeFunction {
    const char *name;                   // a Forge identifier, unique
    int32_t arity;                      // exact count, or -1 for variadic
    int32_t (*call)(const ForgeValue *args, size_t argc, ForgeValue *out);
} ForgeFunction;
```

Values cross the boundary as a tagged union:

| tag | Forge → plugin | plugin → Forge |
| --- | --- | --- |
| `NULL` (0) | `null`, `None` | `null` |
| `BOOL` (1) | `bool` (`as.i` = 0/1) | `bool` (non-zero = true) |
| `INT` (2) | `int` | `int` |
| `FLOAT` (3) | `float` | `float` |
| `STRING` (4) | string (UTF-8, not NUL-terminated) | string; invalid UTF-8 is an error |
| `BYTES` (5) | never sent in v1 | array of ints 0–255 |
| `ARRAY` (6) | array, tuple, set | array |
| `OBJECT` (7) | object (keys in order), map with string keys | object |

`Some(x)` is passed as `x`; `Frozen` values as their contents. Functions,
results, streams, channels and task handles cannot be passed (a clear
error names the argument). Nesting is limited to 128 levels in both
directions.

**Ownership rules** — the whole safety story of the ABI:

1. **Arguments are borrowed.** `args` and everything reachable from it are
   owned by the host and valid only until `call` returns. A plugin must
   copy what it wants to keep and must not write through them.
2. **Results are owned by the plugin's allocator.** The host initialises
   `*out` to `NULL`, the plugin fills it, the host copies it into Forge
   values and then hands it back to the plugin's `free_value`. The host
   never frees plugin memory itself, so plugins may use any allocator.
3. **Status.** `call` returns `0` (`FORGE_OK`, `*out` is the result) or `1`
   (`FORGE_ERR`, `*out` should be a `STRING` message, which becomes the
   Forge error text). Anything else is treated as an error.
4. **Empty buffers** may use a `NULL` pointer with length 0; a `NULL`
   pointer with a non-zero length is rejected.
5. **No unwinding.** A Rust panic or C++ exception must never cross the
   boundary. The Rust SDK wraps every function in `catch_unwind` and turns
   a panic into `FORGE_ERR` (`add() panicked: ...`). Plugins must not be
   built with `panic = "abort"` if they want panics reported rather than
   the process aborted.
6. **Thread safety.** Forge may call a plugin function from several
   threads at once (`spawn`, server handlers); functions must be
   thread-safe. The descriptor and the strings it points to must stay valid
   for the life of the process.
7. **Libraries are never unloaded.** A loaded library is kept until the
   process exits (function values may outlive any scope), and loading the
   same file twice returns the same handle.

### Why a tagged value type and not JSON

JSON-encoding every call would have been simpler to implement, but it costs
a serialise/parse on both sides of every call, loses the int/float
distinction for whole floats, cannot carry bytes, and forces C plugins to
ship a JSON parser. The tagged type is 24 bytes per value on 64-bit
targets, needs no parsing, is trivial to produce from C, and the two
ownership rules above keep it memory-safe across allocators. The cost is
that the layout is now ABI: changing it requires bumping
`FORGE_PLUGIN_ABI_VERSION`.

### Rust SDK (`crates/forge-plugin`)

```rust
use forge_plugin::{forge_fn, export};

#[forge_fn]
fn add(a: i64, b: i64) -> i64 { a + b }

#[forge_fn]
fn divide(a: f64, b: f64) -> Result<f64, String> {
    if b == 0.0 { Err("division by zero".into()) } else { Ok(a / b) }
}

export!(name = "hello", functions = [add, divide]);
```

* `#[forge_fn]` keeps the function and generates an `extern "C"` wrapper
  that converts arguments (`FromForge`), calls it inside `catch_unwind`,
  and converts the result (`IntoForge`; `Result<T, E: Display>` becomes a
  Forge error).
* `export!` emits `forge_plugin_abi_version`, `forge_plugin_register`, the
  static descriptor and `free_value`.
* Argument errors name the function and parameter:
  `add(): argument 2 (b) expected int, got string`.
* Supported types: `()`, `bool`, `i64`/`i32`/`u32`/`usize`, `f64`/`f32`,
  `String`, `Bytes`, `Vec<T>`, `Option<T>`, `HashMap<String, T>`,
  `BTreeMap<String, T>`, and the dynamic `forge_plugin::Value`.

The SDK is a standalone crate (with its own empty `[workspace]`), not a
member of a workspace with `forge-lang`: the main package keeps building,
testing, installing and publishing exactly as before, and the host does not
depend on the SDK (it has its own mirror of the ABI types, checked by
layout tests and by loading real Rust and C plugins in
`tests/native_plugins.rs`).

### Security

Loading a native library runs arbitrary machine code with the full
privileges of the process. No Forge permission applies inside it. The `ffi`
capability therefore:

* is denied by default for `forge run` and `forge test` and needs
  `--allow-ffi` (all libraries) or `--allow-ffi=PATHS` (only libraries at
  or under those paths), or `allow-ffi` in `forge.toml` `[permissions]` —
  it is strictly more powerful than `run`, which is also opt-in;
* is allowed in the REPL and `forge -e` (a person is typing the code),
  unless `--sandbox` is given;
* is denied under `--sandbox`, in `forge mcp` and in embedded `Sandbox`es
  unless explicitly granted.

The check runs against the resolved, canonical library path before the
library is opened, so `..` and symlinks cannot widen a scoped grant.

## Alternatives Considered

* **`let m = native.load("...")` stdlib function.** Simple, but the bound
  names would only be known at run time, the path could be computed (and
  so not reviewable), and it adds a global. The `import` form keeps
  loading declarative and statically resolvable.
* **Rust ABI / `abi_stable`.** Ties plugins to the exact compiler version
  or to a large dependency; excludes C.
* **JSON-only v1.** See above.
* **WebAssembly plugins.** Sandboxable and portable, and a good future
  addition, but it does not solve "call this existing native library".

## Implementation Notes

* `src/plugins/abi.rs` — `#[repr(C)]` mirror + layout tests.
* `src/plugins/mod.rs` — resolution, permission check, `libloading`,
  validation of the descriptor, value conversion, the process-wide registry
  of loaded libraries, and `call`. Plugin functions are
  `Value::BuiltIn("native:<lib>:<fn>")`; both engines route that prefix to
  `plugins::call` (the VM converts at the boundary with `args_to_interp`).
* Parser: `Stmt::ImportNative { path, binding }`.
* Interpreter: executes the statement directly. VM: compiles it to the
  `__forge_import_native` intrinsic plus field reads into locals.
* `src/permissions.rs`: `Capability::Ffi` with a path scope;
  `src/main.rs`: `--allow-ffi[=PATHS]`; `src/manifest.rs`: `allow-ffi`.
* Tests: `tests/native_plugins.rs` builds `examples/plugins/hello_rust`
  with cargo and `examples/plugins/hello_c` with `cc` and runs the scripts
  on both engines; `tests/parity/supported/native_import_errors.fg` covers
  the error paths in the differential corpus.
