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

## 2026-10-12: tier v2 added

Tier v1 no longer finds anything to fix, so `eval/tasks_v2/` adds 50 harder tasks
(servers, HTTP clients, multi-file programs, concurrency, error handling, edition-2027
typed programs, large data inputs). No solver has been scored on it yet; every reference
passes on both engines and the editions it applies to. Writing the references found the
issues in [`V2_FINDINGS.md`](V2_FINDINGS.md). Score a run on both editions:
`score <run> --tier v2` and `score <run> --tier v2 --edition 2027` (the `typed` tasks
count only in 2027).

## 2026-10-12: tier v2, two blind rounds

Same solver model as before; Forge solvers see only `llms.txt`.

| Run | Answers written against | Scored on | Forge 2026 | Forge 2027 | Python |
|---|---|---|---:|---:|---:|
| v2 round 1 | `llms.txt` of v0.11.0 | v0.11.0 | 95.5% (42/44) | 96.0% (48/50) | |
| v2 round 1 | same answers | after this round's fixes | 100% | 100% | |
| All tiers, round 2 (fresh) | updated `llms.txt` | integration branch | 98.6% (145/147) | 98.7% (151/153) | 94.0% (v2 only, 47/50) |
| All tiers, round 2 | same answers | + else-if expressions, `\p{L}` | 100% | 100% | |

Python is scored on tier v2 only. Its three misses are wrong error text and
a wrong status code (405 vs 501); the multi-file prompts are Forge-specific,
so its solver reimplemented the modules instead of importing them.

### What each fix was worth

| Failure cause | Fix |
|---|---|
| `obj[k] = v` copied the object (word count over 100k words: 57 s on the VM) — 2 timeouts | In-place updates of single-owner containers on the VM; interpreter `get`/`has_key` borrow |
| A struct inside `Result<Interface, _>` / `[Interface]` was a T0003 | Interface satisfaction is checked inside generic types |
| `if a { } else if b { } else { }` as an expression was a syntax error | `else if` chains in if-expressions |
| `"\\p{L}"` interpolated `L` | `\p{..}` / `\P{..}` stay literal, like `{4}` |

Writing the v2 references found 26 issues (`V2_FINDINGS.md`). All the bugs
are fixed in this round. Two items stay open: `Map` operations are still
linear per call (sets are hash-indexed now; objects are the usual
dictionary), and there is no typed dictionary that accepts object literals.

## 2026-10-13: tier v3 added

Tier v2 is saturated too, so `eval/tasks_v3/` adds 36 tasks: multi-file answers behind
provided drivers, SQLite services under concurrent requests, 80,000-250,000-record
generated inputs, deep recursion, error handling with exit statuses and stderr, channel
pipelines and CLI/text edge cases (`eval/README.md`, "Tier v3"). No solver has been
scored on it yet. Writing the references found 22 issues
([`V3_FINDINGS.md`](V3_FINDINGS.md)); six tasks are `known_failing` until their finding
is fixed.
