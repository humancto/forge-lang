# Eval results log

First-try correctness (pass@1) on the 103 tasks in `eval/tasks/`. Solvers
are "blind" agents: they see only the task prompts and (for Forge)
`llms.txt`, and cannot run code. Solutions are scored with
`python3 eval/forge_eval.py score` in the sandbox (`--sandbox` plus the
task's grants). Same solver model for every row.

Blind solvers are a proxy for API runs (no API key was available); rerun
with `generate --provider anthropic` to measure a specific model.

## 2026-10-11: edition 2027 work (RFC 0008)

| Run | Answers written against | Scored on | Forge 2026 | Forge 2027 | Python |
|---|---|---|---:|---:|---:|
| Round 1 | `llms.txt` of v0.10.0 | v0.10.0 | 31.1% | – | 97.1%* |
| Round 1 | same answers | + `/dev/stdin` and `get()` fixes | 86.4% | – | |
| Round 1 | same answers | final branch | 95.1% | 96.1% | 98.1% |
| Round 2 | updated `llms.txt` | before the interpolation fix | 79.6% | 79.6% | |
| Round 2 | same answers | final branch | 96.1% | 96.1% | |

\* scored before the `append_log` fixture was committed (a harness bug,
not a solver error); 98.1% after.

### What each fix was worth

Every language change below was chosen from the failure table of the
previous run, not in advance.

| Failure cause (count) | Fix |
|---|---|
| `fs.read("/dev/stdin")` denied, reading input undocumented (62 of 71) | `/dev/stdin` reads stdin on both engines; `llms.txt` "Reading input" |
| Interpolation containing string literals, `"{join(xs, ", ")}"` (10 of 21 in round 2) | Lexer tracks nested strings inside `{...}`; digit-only braces are regex quantifiers |
| Common words reserved (`order`, `from`, `emit`, `safe`) (6) | Soft keywords in every edition |
| `Name {` after a capitalised constant in `if`/`while` parsed as a struct literal (1) | Struct literal only when `field:` or `}` follows |
| `get(m, ".txt", 0)` read as a dot-path (1) | Exact key wins |
| `http.post` re-encoded a String body (1) | String bodies are sent as is |
| `"{}"` was an empty-interpolation error (1) | `{}` is text |

### Remaining failures after #154 (all fixed in 0.11.0)

- `push(x, v)` also updated a mutable `x` (n_queens): edition 2027 makes
  `push` pure; a discarded `push(xs, v)` is T0019.
- `abs(x)` / `floor(x)` were not globals (mode_and_median, order_totals):
  global `abs`, `min`, `max`, `round(x, digits?)`, `floor`, `ceil`,
  `to_fixed`.
- No compact JSON (compact_json): `json.compact`; `json.parse` keeps key
  order.
- Top-level functions were not hoisted (csv_column_stats): they are.

## 2026-10-11: 0.11.0 (third blind round)

Fresh solvers, the 0.11.0 `llms.txt`, scored on the release branch:

| Run | Forge 2026 | Forge 2027 | Python |
|---|---:|---:|---:|
| Round 3 | **100%** (103/103) | **100%** (103/103) | 98.1% |

Round 3's only failure before `floor`/`ceil` became globals was a call to
`floor(x)`.
