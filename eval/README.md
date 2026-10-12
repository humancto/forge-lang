# Forge evaluation harness

This harness measures how often a model writes a **correct Forge program on the first
try**, and compares that with Python on the same tasks. It also classifies every failure,
so the next language or documentation fix is chosen from data. It is the measurement
behind edition 2027 ([RFC 0008](../rfcs/0008-edition-2027.md), section 1). No 2027 change
may lower Forge's pass rate.

All of it lives in `eval/forge_eval.py`, which needs Python 3.9 or newer and only the
standard library.

```
eval/
  forge_eval.py              the harness (validate, generate, score, compare, blind-pack, selftest)
  tasks/<category>/<id>/     tier v1: the original 103 tasks
    task.toml                prompt, input, expected output, capabilities
    reference.fg             idiomatic classic Forge (passes on both engines, both editions)
    reference.py             Python 3, standard library only
    files/                   optional: copied into the working directory before a run
    www/                     optional: served by the local HTTP fixture (http tasks)
    input.txt                optional: a large stdin (`stdin_file`)
  tasks_v2/<category>/<id>/  tier v2: 50 harder tasks, same layout (see "Tier v2" below)
  V2_FINDINGS.md             Forge bugs and llms.txt gaps found while writing tier v2
  runs/                      generated runs and reports (git-ignored)
  packs/, answers/           blind packs and the answers to them (git-ignored)
```

## Quick start

```bash
cargo build                                    # the harness finds target/debug/forge
python3 eval/forge_eval.py validate            # every reference, every engine (about 1 min)
python3 eval/forge_eval.py validate --quick    # vm/2026 + interp/2027 + Python (what cargo test runs)

# Measure a model (needs ANTHROPIC_API_KEY; ANTHROPIC_BASE_URL is optional)
python3 eval/forge_eval.py generate --provider anthropic --lang forge --context llms \
    --model claude-sonnet-5-5 --out eval/runs/sonnet-forge-llms
python3 eval/forge_eval.py score eval/runs/sonnet-forge-llms
python3 eval/forge_eval.py generate --provider anthropic --lang python --context none \
    --out eval/runs/sonnet-python
python3 eval/forge_eval.py score eval/runs/sonnet-python
python3 eval/forge_eval.py compare eval/runs/sonnet-forge-llms eval/runs/sonnet-python
```

Every command accepts `--tasks id1,id2`, `--category a,b`, `--jobs N` and `--forge PATH`. The
default forge binary is `$FORGE_BIN`, then `target/debug/forge`, then `target/release/forge`,
then `forge` on `PATH`.

`--tier v1|v2|all` selects the task set: `v1` is `eval/tasks`, `v2` is `eval/tasks_v2`.
`validate` and `score` default to `all` (`score` takes the task list from the run's
`meta.json`), `generate` and `blind-pack` to `v1`, so commands written before tier v2
existed behave as they did. To measure tier v2:

```bash
python3 eval/forge_eval.py validate --tier v2
python3 eval/forge_eval.py generate --tier v2 --provider anthropic --lang forge --context llms \
    --out eval/runs/sonnet-v2
python3 eval/forge_eval.py score eval/runs/sonnet-v2                  # VM, edition 2026
python3 eval/forge_eval.py score eval/runs/sonnet-v2 --edition 2027 --report-name report-2027
python3 eval/forge_eval.py blind-pack --tier v2 --context llms --per-pack 10 --shuffle 1
```

## Commands

### `validate`

This command runs every reference solution and compares its output with `expected_stdout`:

- Forge runs on the VM and on the interpreter, in edition 2026 and edition 2027 (four runs).
  `--quick` runs two of them: the VM in 2026 and the interpreter in 2027. A task pinned to
  editions (`editions = ["2027"]`) runs only in those: both engines, or with `--quick` the
  VM in its first and the interpreter in its last edition.
- `forge --edition 2027 check` must report no diagnostics, not even warnings, because 2027
  turns mismatches against declared types into errors (a task pinned to 2026 is checked in
  2026). The reference is checked as `main.fg` next to a copy of the task's `files/`, so the
  modules it imports are visible.
- The Python reference also runs, unless you pass `--no-python`.
- The schema of each `task.toml` is checked. The built-in TOML parser (used on Python < 3.11)
  must agree with `tomllib`.
- References are linted for natural syntax and joke builtins, which 2027 removes.

All runs go through the same sandboxed runner that `score` uses, so a reference that passes
proves that the task's `needs` grants are enough.

### `generate`

```
generate --provider {anthropic,command,dir} --lang {forge,python}
         --context {none,llms,llms+errors} [--model M] --out eval/runs/<name>
```

The system prompt asks for a complete program in the target language, written as exactly one
fenced code block. The user message is the task's `prompt`. With `--context llms` (Forge
only), the full `llms.txt` is appended to the system prompt. With `--context none`, Forge is
introduced in one sentence.

`generate` writes `meta.json`, plus `prompt.txt`, `response.txt` and `solution.fg` (or `.py`)
for each task. The first block fenced as `forge`/`fg` (or `python`/`py`) is used. Without
one, the longest fenced block is used, and without any fence, the raw reply.

Providers:

| Provider | Behaviour |
|---|---|
| `anthropic` | `POST $ANTHROPIC_BASE_URL/v1/messages` (default `https://api.anthropic.com`) with `ANTHROPIC_API_KEY`. The default model is `claude-sonnet-5-5`. Retries on 429/5xx. `--temperature` and `--max-tokens` are optional. |
| `command` | `--command "<shell command>"`. The conversation (system prompt, then `### USER` / `### ASSISTANT` turns) is piped to the command's stdin and its stdout is the reply. `FORGE_EVAL_TASK_ID` is set. |
| `dir` | `--from PATH`. PATH is a directory with `<id>.fg`, `<id>/solution.fg` or `<category>/<id>/{solution,reference}.fg`, or Markdown answer files (`## <task id>` followed by a code block). Use `--from eval/tasks` to score the references themselves. |

`--context llms+errors` adds one repair round, for which `anthropic` or `command` is
required. After a failing first attempt, the harness sends the diagnostics back once and
saves `feedback.txt`, `response2.txt` and `solution2.fg`. For Forge, those diagnostics are the
`--error-format json` lines (`{code, severity, message, line, col, hint, phase}`), and for
Python the traceback. A wrong output is described by the program's actual output, never by the
expected output. `score` then reports **pass@2** next to **pass@1**. pass@2 measures how
useful Forge's error messages are, separately from first-try correctness. `--engine` and
`--edition` choose how the first attempt runs.

### `score`

```
score eval/runs/<name> [--engine vm|interp] [--edition 2027] [--report-name report]
```

`score` runs each solution in a fresh temporary directory, with the task's `files/` copied in
and the solution saved as `main.fg` or `main.py`. Forge runs as:

```
forge [--interp] [--edition E] --sandbox --error-format json --max-time T
      [--allow-read=<dir>] [--allow-write=<dir>] [--allow-net=127.0.0.1,localhost]
      [--allow-env] [--allow-run] [--allow-db] run main.fg
```

Only the grants named in the task's `needs` are passed. Tasks pinned to other editions than
the one being scored are left out of the report and listed as not scored (`skipped` in
`report.json`); in a Forge prompt, a task pinned to one edition says which edition runs it. Python runs as `python3 main.py`
with a timeout and no sandbox. `score` writes `report.json`, which is machine-readable and
holds every task's result, stdout, stderr and diff, and `report.md`.

### `compare`

`compare runA runB [...]` prints a side-by-side table: overall pass@1 (and pass@2), each
category, and the failure classes. With exactly two runs, it also lists the tasks that passed
in only one of them. `--out FILE` also writes the table to FILE.

### `blind-pack`

With no API key, other agents can act as the models. `blind-pack` writes
`eval/packs/<n>.md`. Each pack holds solving instructions and a batch of task prompts
(`--per-pack 10`; `--shuffle SEED` mixes categories). The solver may not run any code, and it
answers with a `## <task id>` heading plus one code block for each task, in
`eval/answers/<n>.md`.

With `--context none` (the default), the solver gets no documentation at all. With
`--context llms`, it may read `llms.txt` and nothing else. Collect and score the answers
with:

```bash
python3 eval/forge_eval.py blind-pack --per-pack 20 --shuffle 1
# ... agents write eval/answers/1.md, 2.md, ...
python3 eval/forge_eval.py generate --provider dir --from eval/answers --lang forge \
    --context none --out eval/runs/blind-none
python3 eval/forge_eval.py score eval/runs/blind-none
```

### `selftest`

`selftest` scores known-bad programs (a syntax error, an undefined name, an index out of
bounds, an infinite loop, a wrong output and a missing solution) and checks that each one is
classified correctly. It also checks answer parsing, code extraction and the TOML parser,
and, with throwaway tasks, the tier-v2 harness features: tier loading, edition pinning,
`[[case]]` runs with exit codes and `stdin_file`, the server runner (a Forge and a Python
server, `status_only`, request headers, a server that exits early) and the fixture's
per-run `flaky/`, `auth/` and `headers` routes and `{closed_url}`. CI and
`cargo test --test eval_tasks` run it.

## Metrics

- **pass@1**: the share of tasks whose first solution printed exactly the expected output
  and exited successfully. Reports give it overall and per category. A run covers one
  language, so `compare` puts languages side by side. Forge runs can be scored per engine
  and per edition.
- **pass@2** (`llms+errors` only): the share of tasks that passed on the first try or after
  the single repair round.
- **Failure classes**: each failed task gets exactly one class.

| Class | Meaning |
|---|---|
| `syntax_error` | Lexer or parser error (Forge `E0001`/`E0002`, Python `SyntaxError`/`IndentationError`) |
| `type_error` | A type-checker error that stopped the run (Forge diagnostics with `phase: "type"` and severity error, as under `--strict` or edition 2027) |
| `runtime_error` | The program failed while running. The code is the Forge error code (`E0003` undefined name, `E0015` bad builtin argument, `E0020` permission denied, ...) or the Python exception name |
| `wrong_output` | The program exited 0 but its output differs. The report shows the first differing lines |
| `timeout` | The run passed `timeout` seconds (`--max-time`, exit code 124), or ran out of fuel or memory |
| `no_code` | No solution file, or an empty one |

- **Top failure causes**: failures grouped by (class, code, message prefix). Quoted names
  and numbers in the message are normalised, so `undefined variable: 'foo'` and
  `undefined variable: 'bar'` count as the same cause. Type-checker warnings on a failed
  Forge run are listed with it. A `T0006` warning next to a wrong output often points at
  the real cause.

Output comparison normalises line endings, strips trailing whitespace on every line and
ignores leading and trailing blank lines. A task can opt into `sort_lines = true` (for order
that is not deterministic) or `float_tolerance = 1e-6` (numeric tokens compared with a
tolerance).

## Adding a task

1. Create `eval/tasks/<category>/<id>/` (tier v1) or `eval/tasks_v2/<category>/<id>/`
   (tier v2). The `id` must equal the directory name and be unique across both tiers. The
   v1 categories are `basics`, `strings`, `collections`, `structs_adts`, `errors`,
   `json_csv`, `functional`, `algorithms`, `concurrency`, `http`, `files`; the v2 ones are
   `servers`, `http_client`, `multifile`, `parallel`, `error_handling`, `typed`, `data`.
2. Write `task.toml`:

   ```toml
   id = "word_frequency"
   category = "strings"
   title = "Top words in a text"
   prompt = """
   Read all of standard input as text. ... Print the 5 most frequent words as `<word> <count>` ...
   """
   stdin = """..."""            # optional; or stdin_file = "input.txt" (in the task dir)
   needs = []                   # any of "read", "write", "net", "env", "run", "db"
   timeout = 10                 # seconds (default 10)
   expected_stdout = """..."""
   # optional: exit_code = 1    (the status the program must exit with; default 0)
   # optional: editions = ["2027"]  (scored and validated only in these editions)
   # optional: output_files = ["report.txt"]  (appended as "--- report.txt ---\n<content>")
   # optional: sort_lines = true, float_tolerance = 1e-6, trim = false
   ```

   To run the program several times with different inputs, replace `stdin`/`stdin_file`,
   `expected_stdout` and `exit_code` with `[[case]]` tables, each with an optional `name`,
   `stdin` or `stdin_file`, `expected_stdout` and `exit_code`. The task passes when every
   case prints its output and exits with its status; reports show each case's output under
   `=== case <name> (exit <status>)`.

   Write the prompt the way a user would ask, without naming a language. Spell out the exact
   output format, including decimals, separators and tie-breaking. Programs get their input
   on stdin, not on the command line, because `forge run` passes no script arguments.
3. Write `reference.py` (standard library only). The usual way to fill in `expected_stdout`
   is to run it.
4. Write `reference.fg` in idiomatic classic Forge, following `llms.txt`. Use `println` (not
   `say`), `fn`, `else`, and annotations where they are natural. Do not concatenate a string
   with a number: use `str(x)` or `"{x}"`. It must pass on both engines and under
   `--edition 2027`, and `forge --edition 2027 check` must report no warnings.
5. Run `python3 eval/forge_eval.py validate --tasks <id>`.

Make tasks realistic and varied, and include the places where Forge differs from what models
expect. Some examples: `regex.*(text, pattern)` argument order, `{` starting interpolation in
normal strings, integer division, comparator-based `sort`, `Result` handling, and JSON key
order. A task that fails on Forge for a model shows something to fix.

### HTTP tasks

- **Client tasks** put `{base_url}` in `stdin`. The harness replaces it with the address of a
  local fixture server. Under `/t/<task id>/`, the fixture serves the task's `www/` files,
  `echo` (any method, which returns `{method, path, query, body, content_type}`) and
  `status/<code>`. The Forge HTTP client needs `FORGE_HTTP_ALLOW_PRIVATE=1` for loopback,
  and the harness sets it.
- Per run, `{base_url}` carries a fresh run token (`/t/<task id>~r<n>`), so state the
  fixture keeps per run never leaks between runs. More routes: `flaky/<n>/<key>` answers
  503 `{"error": "unavailable", "attempt": k}` to the first n requests of the run for that
  key, then 200 `{"key", "attempt"}`; `auth/<file>` serves a `www/` file only with the
  header `Authorization: Bearer eval-token-7f3a`, else 401 `{"error": "unauthorized"}`;
  `headers` answers with the request's `x-*`, `accept` and `authorization` headers.
  `{closed_url}` is replaced with `http://127.0.0.1:<port>` where nothing listens
  (connection refused).
- **Server tasks** set `mode = "server"` and list `[[request]]` tables (`method`, `path`,
  optional `body`, optional `headers` table, optional `status_only = true`). The harness
  starts the program, waits for the port, sends the requests, and compares the transcript
  `STATUS <code>` plus the canonical JSON body (only the status line with `status_only`)
  with `expected_stdout`. Forge's `@server(port: ...)` needs a literal port, so a server
  task names a fixed `port` in its prompt and in `port = ...` (tier v1 uses 18471, tier v2
  18501-18510). Runs that share a port are serialised within one harness process. The
  server's output goes to files, so a server that logs every request cannot block.

## Tier v2

Tier v1 is saturated: blind solvers score 100% on both editions. Tier v2 has 50 harder
tasks (each still one file of roughly 40-150 lines) aimed at what trips models and matters
in production:

| Category | Tasks | What it exercises |
|---|---:|---|
| `servers` | 10 | `@server`/`@get`/`@post`/`@put`/`@delete` JSON APIs: path and query parameters, body validation, state that must survive requests (a JSON file, SQLite), data loaded at start-up |
| `http_client` | 6 | pagination, retrying 5xx but not 4xx, bearer tokens and headers, POST/PUT/DELETE with bodies and query parameters, refused connections and invalid JSON, URL encoding |
| `multifile` | 6 | `main.fg` imports modules the task provides in `files/` (their source is in the prompt): structs with methods, a library importing a sibling, a `forge_modules` package, two modules exporting the same names, module state |
| `parallel` | 6 | `spawn`/`await`, `squad`, bounded channels, `select` with an idle timeout, a shared deadline, isolating failures, map-reduce; the time limits only pass when the work really overlaps |
| `error_handling` | 6 | Result and `?`, `try`/`catch`, exit statuses (`[[case]]` runs with `exit_code`), precise error messages |
| `typed` | 6 | edition 2027 only (`editions = ["2027"]`): interfaces, algebraic types and `match`, generic structs, typed JSON decoding, a recursive expression type |
| `data` | 10 | quoted CSV with embedded newlines, nested JSON reshaping, ISO timestamps with offsets, business days, logfmt, joins with rounding rules, and 20,000-100,000-line inputs where quadratic code times out |

Design rules:

- Prompts stay language-neutral where they can. `multifile` and `typed` prompts are
  Forge-shaped (they show Forge modules or ask for declared types), so the Python
  references there only produce the expected output; compare Python pass rates on the
  other categories.
- Forge handlers can only answer status 200 (or 500 when the handler raises), so server
  prompts report errors in the JSON body; unknown routes and methods are checked with
  `status_only` requests. See `V2_FINDINGS.md`.
- Concurrency tasks never depend on scheduling order: results are printed in input order
  or per producer, and the references return values from tasks or use channels instead of
  mutating captured state. Time limits leave large margins (a sequential solution takes
  at least twice the limit; the concurrent one about half of it).
- Rounding is specified exactly ("halves rounded up", integer arithmetic in the
  references), so Python and Forge agree.

Writing the references surfaced the Forge bugs, performance cliffs and `llms.txt` gaps
listed in [`V2_FINDINGS.md`](V2_FINDINGS.md); where a reference avoids one of them, a
comment in it says so.

## Notes

- Forge solutions always run under `--sandbox`. Python solutions are not sandboxed, so
  review model output before scoring it on a machine you care about.
- Expected outputs never depend on wall-clock time, randomness or scheduling order. Tasks
  that read concurrent results sort them or collect them in order.
