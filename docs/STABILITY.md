# Forge Stability Policy

This document says what Forge promises not to break, starting with 1.0,
and how changes that would break something are made anyway. Until 1.0
(the 0.x series) Forge follows the same rules on a best-effort basis, with
one exception: a 0.x minor release may still make a breaking change, and
the CHANGELOG lists it under **Changed** or **Removed**.

Version numbers follow [Semantic Versioning](https://semver.org): a patch
release (1.2.3 → 1.2.4) only fixes bugs, a minor release (1.2 → 1.3) adds
things, and only a major release (1.x → 2.0) or a new **edition** (below)
may remove or change them.

## What 1.0 guarantees

| Surface | Guarantee | Where it is defined |
| --- | --- | --- |
| **Syntax** | A program that parses under an edition keeps parsing, with the same meaning, under every later Forge release that supports that edition. New syntax is added only where it was previously a syntax error. New keywords are reserved by an edition, never by a minor release. | `src/lexer`, `src/parser`, the spec in `docs/spec/` |
| **Semantics** | Both engines (the default bytecode VM and `--interp`) produce the same output, exit status and error codes for the same program. A divergence is a bug, not behaviour (`tests/engine_diff.rs`, `tests/error_codes.rs`). Performance is not part of the contract. | `src/semantics/` |
| **Standard library signatures** | Builtin and stdlib member names, accepted arities and argument types do not shrink. New optional parameters and new members may be added in minor releases. Output formats that programs parse (`json.stringify`, `csv.stringify`, `str()` of numbers) do not change. | `src/builtins_registry.rs` (the single list both engines register from), `llms.txt` |
| **Runtime error codes** | `E0000`–`E0033` and `T0001`–`T0018` keep their meaning forever; codes are never reused or renumbered, new ones are appended. Message *wording* may improve in any release — match on `code`, not on text. | `src/semantics/errors.rs`, `src/typechecker/diagnostics.rs`, `forge explain` |
| **Machine-readable diagnostics** | The fields of `--error-format json` / `forge check --format json` (`code`, `severity`, `message`, `file`, `line`, `col`, `hint`, `phase`) and of `forge mcp`'s `run_forge`/`check_forge` structured output keep their names and types. Fields may be added. | `src/errors.rs` (`ProgramDiagnostic::to_json`), `src/mcp.rs` |
| **CLI** | Subcommands and flags listed by `forge help` keep working; a renamed flag keeps its old spelling as an alias with a deprecation warning. Exit codes: 0 success, 1 program or usage error, 124 `--max-time` expired. | `src/main.rs` (clap `Command`) |
| **Permissions** | A capability that is denied by default stays denied by default. A minor release may only make defaults stricter for security fixes, announced under **Security** in the CHANGELOG. | `src/permissions.rs`, `SECURITY.md` |
| **Bytecode (`.fgc`) format** | Files start with `FGC\0` and a `major.minor` format version (currently 1.3). A Forge release reads every format version with the same major and an equal or lower minor; it refuses a newer one with a clear error rather than misreading it. Adding an opcode or a field bumps the minor version; changing the meaning of existing bytes bumps the major version, which only happens in a major Forge release. `.fgc` files are a cache, not an archive: recompile from source when in doubt. | `src/vm/serialize.rs` |
| **Embedding API** (`forge_lang::Sandbox`, `Output`, `SandboxError`, `CancelHandle`, `Capabilities`, `Capability`, `PermissionError`) | Semver for the Rust crate: no removal or signature change of these items outside a major release. `SandboxError` is matched by `kind()`, whose strings are stable; new variants may be added (treat unknown kinds as errors). The Python package `forge-lang` follows the same rules. Every other `pub` module of the crate (`interpreter`, `vm`, `parser`, `lexer`, `runtime`, ...) is internal and may change in any release. | `src/lib.rs`, `src/sandbox.rs`, `bindings/python` |
| **Native plugin ABI v1** | A plugin built against ABI v1 (`crates/forge-plugin/include/forge_plugin.h`) loads and works in every Forge release that implements v1. Changing any type or symbol in the header is a new ABI version; hosts keep loading v1 plugins for at least one major release after v2 ships, and refuse plugins whose `forge_plugin_abi_version()` they do not implement. | `src/plugins/abi.rs`, `rfcs/0006-native-plugins.md` |
| **`forge.toml`** | Existing keys keep their meaning. Unknown top-level tables are ignored so that newer manifests stay readable; `[permissions]` and `edition` are validated strictly, because silently ignoring them would fail open. | `src/manifest.rs` |

Not covered: the text of error messages and warnings, the human (non-JSON)
CLI output, performance characteristics, `--jit` eligibility, internal
crate modules, undocumented `__forge_*` intrinsics, and behaviour that the
CHANGELOG or the spec calls *experimental* (for example `timeout` blocks).

## Editions

An **edition** is how Forge makes a source-breaking change after 1.0
without breaking existing code. A project chooses its edition in
`forge.toml`:

```toml
[project]
name = "my-app"
edition = "2026"
```

* Projects without the key use the default edition, `2026`. `forge new`
  writes the key explicitly.
* Only one edition exists today. `forge run`, `forge test`, `forge check`
  and `forge mcp` validate the key and refuse an unknown edition (for
  example a project written for a newer Forge) instead of running it under
  the wrong rules.
* Every edition stays supported by every later Forge release. Upgrading the
  toolchain never changes a program's meaning; changing the `edition` key
  may, and the release notes for the edition list exactly what changes.
* Editions are per project; a project can depend on packages written for
  other editions, and each package is compiled under its own edition.

How a future breaking change is gated:

1. The change is proposed in an RFC (`rfcs/`) that names the new edition
   and gives a migration path (ideally automatic, via `forge fmt` or a
   `forge fix` subcommand).
2. The new edition is added to `manifest::Edition` (a new variant, ordered
   after the existing ones) and to the table below.
3. The implementation keeps the old behaviour and checks the edition where
   the behaviour differs — `if edition >= Edition::E20xx { new } else { old }` —
   in shared code (`src/semantics/`, the parser, the type checker), so both
   engines change together. The type checker warns in the old edition
   wherever the code would change meaning in the new one.
4. Until the edition is released, the new behaviour is available only
   behind the unreleased edition string, so it can be tested.

| Edition | Status | Changes |
| --- | --- | --- |
| `2026` | current, default | The language as of Forge 0.9 / 1.0. |

## Deprecation policy

Nothing is removed without warning:

1. **Deprecate.** The item keeps working and produces a warning the first
   time it is used in a process. For builtins this is a row in
   `DEPRECATED` in `src/builtins_registry.rs` (name, the release that
   deprecated it, the earliest release that may remove it, the
   replacement); both engines check it on every builtin call, so the
   warning is identical on the VM and the interpreter:

   ```text
   warning: `typeof` is deprecated since v1.1.0 and may be removed in v2.0.0; use `type` instead
   ```

   CLI flags get the same treatment through clap aliases, and language
   constructs through a type-checker warning. The deprecation is listed
   under **Deprecated** in the CHANGELOG, and `llms.txt` stops showing the
   old form.
2. **Wait.** The deprecated item stays for **at least one minor release**
   (before 1.0) and until the next major release or edition (after 1.0).
3. **Remove.** Removal is listed under **Removed** in the CHANGELOG with the
   replacement. After 1.0, removing syntax or changing semantics also
   requires a new edition (above).

Today no builtin is deprecated (`DEPRECATED` is empty); the mechanism is
tested in `builtins_registry::tests::deprecated_builtins_warn_once`.

## Error codes

Runtime errors carry a stable code from one table
(`src/semantics/errors.rs`) that both engines share, so the same failure
has the same code on the VM and the interpreter:

```text
$ forge run app.fg
[E0009] Error: index out of bounds: index 3 on array of length 3
   ╭─[app.fg:2:1]
 2 │ say a[3]
   │ ┬
   │ ╰── index out of bounds: index 3 on array of length 3
   │ Help: valid indices are 0 to 2 (or -3 to -1 from the end)
   │ Note: run `forge explain E0009` for details
```

* `forge explain` lists every code; `forge explain E0009` (or `T0006`)
  prints the explanation with an example and the fix.
* `forge run --error-format json` writes one JSON object per diagnostic to
  stderr; `forge check [--format json] <file>` parses and type-checks
  without running.
* `try { ... } catch e { e.code }` exposes the code to programs (`e.type`
  remains for compatibility).
* Every code has a fixture under `tests/errors/` that triggers it on both
  engines, checked by `tests/error_codes.rs`.

To add a code: append an `ErrorCode` to `RUNTIME_ERRORS` (next number,
title, one-line hint, explanation with `Example:` and `Fix:`, and a
matcher on the stable prefix of the message), raise the message from a
helper in `src/semantics/` that both engines call, add a row to
`helpers_classify_to_their_codes`, and add a fixture.
