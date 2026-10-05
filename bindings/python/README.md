# forge-lang for Python

Run untrusted [Forge](https://github.com/humancto/forge-lang) code from
Python in a **deny-by-default sandbox**. Built for agent frameworks: let a
model write a short program instead of making ten tool calls, and run it with
exactly the capabilities you grant.

```bash
pip install forge-lang
```

```python
from forge_lang import Sandbox

print(Sandbox().run('say "hello from Forge"').stdout)   # hello from Forge
```

Wheels are `abi3` (one wheel per platform for CPython 3.9+) for Linux,
macOS and Windows. Nothing else to install: the Forge runtime is compiled
into the extension.

## The sandbox

A fresh `Sandbox()` grants **nothing**: no filesystem, network, environment
variables, databases, subprocesses, LLM calls, or process control (`exit`,
`cd`). Every grant is explicit:

```python
import tempfile
from forge_lang import Sandbox

work = tempfile.mkdtemp()
sb = Sandbox(
    allow=["env"],                     # capabilities granted without restriction
    allow_read=[work],                 # fs.read only under these directories
    allow_write=[work],                # fs.write only under these directories
    allow_net=["api.github.com"],      # net only to these hosts (*.x.com, host:port)
    max_time=5.0,                      # wall-clock limit per run, seconds
    max_output=64_000,                 # stop after printing this many bytes
)

result = sb.run(f'''
fs.write("{work}/notes.txt", "remember the milk")
say fs.read("{work}/notes.txt")
''')
print(result.stdout)                   # remember the milk
```

Capability names: `fs.read`, `fs.write`, `net`, `env`, `db`, `run`, `ai`,
`process` (also listed in `forge_lang.CAPABILITIES`). Scoped filesystem
grants resolve `..` and symlinks before checking, and `fs.exists` never
reveals what is outside the grant.

A `Sandbox` is immutable and thread-safe. Every `run` gets a fresh
interpreter on its own thread, so nothing leaks between runs, and the GIL is
released while Forge code executes.

## Errors

Failures raise typed exceptions, all subclasses of `ForgeError`:

| Exception | `kind` | When |
|---|---|---|
| `ForgeSyntaxError` | `syntax` | The source does not parse (`line`, `column` set) |
| `ForgePermissionError` | `permission_denied` | The program used an ungranted capability |
| `ForgeRuntimeError` | `runtime` | Any other runtime failure (`line` set when known) |
| `ForgeTimeoutError` | `timeout` | `max_time` exceeded (`limit` in seconds) |
| `ForgeOutputLimitError` | `output_limit` | `max_output` exceeded (`limit` in bytes) |
| `ForgeCancelledError` | `cancelled` | A `CancelToken` was triggered |

Every exception carries `stdout`: what the program printed before it failed.

```python
from forge_lang import Sandbox, ForgePermissionError

try:
    Sandbox().run('say "listing..."\nsay sh("ls /")')
except ForgePermissionError as e:
    print(e)          # permission denied: run ...
    print(e.stdout)   # listing...
```

## Checking without running

```python
import forge_lang

for d in forge_lang.check("let x = 1\nlet y = ("):
    print(d)          # line 2:...: error: ...
```

`check` lexes, parses and type-checks. It returns a list of `Diagnostic`
(`line`, `column`, `is_error`, `severity`, `message`); an empty list means no
problems. It is the same check `forge mcp` exposes as `check_forge`.

## Cancelling

```python
import threading
from forge_lang import Sandbox, CancelToken, ForgeCancelledError

token = CancelToken()
threading.Timer(1.0, token.cancel).start()
try:
    Sandbox().run("while true { }", cancel=token)
except ForgeCancelledError:
    pass
```

Ctrl-C (`KeyboardInterrupt`) also stops a run that is in progress.

## Agent "code mode"

Give the model one tool that runs Forge, instead of many narrow tools. The
model writes a small program; the host decides what it may touch.

```python
from forge_lang import Sandbox, ForgeError, check

SANDBOX = Sandbox(
    allow_read=["./workspace"],
    allow_write=["./workspace/out"],
    allow_net=["api.github.com"],
    max_time=10,
    max_output=32_000,
)

RUN_FORGE_TOOL = {
    "name": "run_forge",
    "description": (
        "Run a Forge program and return what it prints with `say`. "
        "Filesystem access is limited to ./workspace (writes to ./workspace/out); "
        "HTTP only to api.github.com. 10 second limit."
    ),
    "input_schema": {
        "type": "object",
        "properties": {"code": {"type": "string", "description": "Forge source"}},
        "required": ["code"],
    },
}


def run_forge(code: str) -> dict:
    """Tool handler: never raises, always returns something the model can read."""
    problems = [str(d) for d in check(code) if d.is_error]
    if problems:
        return {"ok": False, "error": "syntax", "details": problems}
    try:
        return {"ok": True, "stdout": SANDBOX.run(code).stdout}
    except ForgeError as e:
        # e.kind tells the model what went wrong; e.stdout shows how far it got.
        return {"ok": False, "error": e.kind, "message": str(e), "stdout": e.stdout}
```

A program the model might send:

```forge
let repos = http.get("https://api.github.com/orgs/rust-lang/repos").json
let names = map(repos, fn(r) { return r.name })
fs.write("./workspace/out/repos.txt", join(names, "\n"))
say "saved {len(names)} repos"
```

`llms.txt` at the root of the Forge repository is a compact language
reference written for models; include it in the system prompt.

## Current limits

* Programs run on Forge's tree-walking interpreter. HTTP servers
  (`@server`), `schedule` and `watch` blocks are not started.
* There is a wall-clock limit and an output limit, but no memory or
  instruction ("fuel") limit yet; call depth is bounded by the engine's
  recursion limit. When the core sandbox gains them they will appear as
  `Sandbox(...)` keyword arguments with their own exception classes.
* stderr output (`log`, `term`) is not captured, and `input()` reads the
  host's stdin.

## Development

```bash
cd bindings/python
python -m venv .venv && . .venv/bin/activate
pip install maturin pytest
maturin develop          # builds the extension into the venv
pytest
```

The crate in this directory is its own Cargo workspace and is never
published to crates.io; it depends on the `forge-lang` crate by path.
