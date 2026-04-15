# Plan: 7E.2 — Add `os` and `path` stdlib modules

## Goal

Add two new stdlib modules providing OS info and path manipulation — table-stakes for real programs.

## API Design

### `os` module (5 functions)

| Function        | Return                             | Implementation                                             |
| --------------- | ---------------------------------- | ---------------------------------------------------------- |
| `os.hostname()` | String                             | `gethostname::gethostname().to_string_lossy()`             |
| `os.platform()` | String ("macos"/"linux"/"windows") | `std::env::consts::OS` mapped to friendlier names          |
| `os.arch()`     | String ("x86_64"/"aarch64"/...)    | `std::env::consts::ARCH`                                   |
| `os.pid()`      | Int                                | `std::process::id()`                                       |
| `os.cpus()`     | Int                                | `std::thread::available_parallelism().unwrap_or(1)`        |
| `os.homedir()`  | String                             | `dirs::home_dir()` or `$HOME`/`%USERPROFILE%` env fallback |

### `path` module (7 functions + 1 property)

| Function                  | Return            | Implementation                                                            |
| ------------------------- | ----------------- | ------------------------------------------------------------------------- |
| `path.join(a, b, ...)`    | String            | `PathBuf::push` — coerce all args to string, skip non-strings             |
| `path.resolve(p)`         | String            | `std::fs::canonicalize` — errors if path doesn't exist                    |
| `path.relative(from, to)` | String            | Manual: find common prefix, count `..` from `from`, append `to` remainder |
| `path.is_absolute(p)`     | Bool              | `std::path::Path::is_absolute`                                            |
| `path.dirname(p)`         | String            | `Path::parent()` — return `""` for empty/root                             |
| `path.basename(p)`        | String            | `Path::file_name()` — return `""` for empty                               |
| `path.extname(p)`         | String            | `Path::extension()` — return `""` for no extension                        |
| `path.separator`          | String (property) | `MAIN_SEPARATOR_STR` — static value in module, NOT a callable             |

**Dropped `path.normalize`** — near-identical to `path.resolve`, and lexical normalization without filesystem access is complex and error-prone. `resolve` covers the real use case.

## Decisions from expert review

1. **Hostname crate**: Use `gethostname` crate. Zero deps, 50M+ downloads, cross-platform. Add to `Cargo.toml`.
2. **OsString safety**: Use `to_string_lossy()` to avoid panics on non-UTF8 hostnames.
3. **`cpus()` fallback**: `available_parallelism().map(|n| n.get()).unwrap_or(1)` — return 1 on failure.
4. **VM value conversion**: Use the richer conversion pattern (like `json.*`) that handles Int/Float/Bool/Array/Object, not just strings. `path.*` takes string args but defensiveness matters.
5. **`path.separator`**: Static `Value::String` in the module object. No `call()` arm needed.
6. **Empty path edge cases**: Return `""` for `dirname("")`, `basename("")`, `extname("")`. `dirname("/")` returns `"/"`.
7. **`path.relative` algorithm**: Canonicalize both paths first (require existence), find common prefix by iterating components, emit `..` for each remaining `from` component, append remaining `to` components. If canonicalization fails, return error.
8. **Non-string args to path fns**: Coerce to string representation or return error, not silent null.
9. **fs module overlap**: `path.dirname`/`basename`/`extname` intentionally duplicate `fs.dirname`/`basename`/`ext`. Same logic, separate call namespace. Acceptable for now — shared implementation would couple modules.

## Files to touch

1. **`Cargo.toml`** — Add `gethostname` dependency
2. **NEW: `src/stdlib/os_module.rs`** — `create_module()` + `call()`
3. **NEW: `src/stdlib/path_module.rs`** — `create_module()` + `call()`
4. **`src/stdlib/mod.rs`** — Add `pub mod os_module; pub mod path_module;` + wrapper fns
5. **`src/interpreter/mod.rs`** — Register `os` and `path` modules
6. **`src/vm/compiler.rs`** — Add `"os"` and `"path"` to builtin_modules list
7. **`src/vm/builtins.rs`** — Add delegation blocks for `os.*` and `path.*`

## Test strategy

- Rust unit tests in each module file for all functions
- Add `tests/parity/os_path.fg` parity fixture
- Verify with `cargo test` and `forge run` on the fixture

## Rollback

Revert the feature branch. All changes are additive.
