# forge-plugin

Write native [Forge](https://github.com/humancto/forge-lang) plugins in Rust.

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
forge-plugin = { path = "path/to/forge-lang/crates/forge-plugin" }
```

```rust
use forge_plugin::{export, forge_fn};

#[forge_fn]
fn add(a: i64, b: i64) -> i64 {
    a + b
}

#[forge_fn]
fn divide(a: f64, b: f64) -> Result<f64, String> {
    if b == 0.0 { Err("division by zero".into()) } else { Ok(a / b) }
}

export!(name = "hello", functions = [add, divide]);
```

```forge
import native "target/release/libhello" as hello
say hello.add(1, 2)
```

```bash
forge --allow-ffi run app.fg
```

- `#[forge_fn]` generates the `extern "C"` wrapper: argument conversion
  (errors name the parameter), `Result` → Forge error, and `catch_unwind`
  so panics never cross into Forge. Rename with `#[forge_fn(name = "x")]`.
- `export!` generates `forge_plugin_abi_version` / `forge_plugin_register`.
- Supported types: `()`, `bool`, integers, `f64`/`f32`, `String`/`&str`,
  `Bytes`, `Vec<T>`, `Option<T>`, `HashMap/BTreeMap<String, T>`, `Value`.
- Functions must be thread-safe; do not build with `panic = "abort"`.
- **Loading a plugin is full trust** — it runs outside every Forge
  permission, which is why Forge requires `--allow-ffi`.

C and other languages: implement `include/forge_plugin.h` (normative ABI,
version 1). Design and ownership rules: `rfcs/0006-native-plugins.md` in the
forge-lang repository.
