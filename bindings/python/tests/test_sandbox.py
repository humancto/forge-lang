import _thread
import pathlib
import threading
import time

import pytest

import forge_lang
from forge_lang import (
    CancelToken,
    ForgeCancelledError,
    ForgeError,
    ForgeOutputLimitError,
    ForgePermissionError,
    ForgeRuntimeError,
    ForgeSyntaxError,
    ForgeTimeoutError,
    Sandbox,
)


def fs_path(p: pathlib.Path) -> str:
    """A path as a Forge string literal body (forward slashes on Windows)."""
    return p.as_posix()


# --- happy path -------------------------------------------------------------


def test_run_captures_stdout():
    res = Sandbox().run('say "hi"\nprint("a")\nprintln(1 + 2)')
    assert isinstance(res, forge_lang.Result)
    assert res.stdout == "hi\na3\n"


def test_natural_syntax_and_functions():
    code = """
define square(n) { return n * n }
set mut total to 0
for x in [1, 2, 3] { total = total + square(x) }
say "total: {total}"
"""
    assert Sandbox().run(code).stdout == "total: 14\n"


def test_sandbox_is_reusable_and_runs_are_isolated():
    sb = Sandbox()
    assert sb.run("let x = 41\nsay x + 1").stdout == "42\n"
    # A fresh interpreter per run: `x` from the previous run is gone.
    with pytest.raises(ForgeRuntimeError):
        sb.run("say x")


def test_version_and_capabilities():
    assert forge_lang.__version__
    assert "fs.read" in forge_lang.CAPABILITIES
    assert "net" in forge_lang.CAPABILITIES


def test_repr():
    r = repr(Sandbox(allow=["env"], max_time=1.5, max_output=10))
    assert r == "Sandbox(allow=['env'], max_time=1.5, max_output=10)"


# --- configuration validation ----------------------------------------------


def test_unknown_capability_is_rejected():
    with pytest.raises(ValueError, match="unknown capability"):
        Sandbox(allow=["teleport"])


@pytest.mark.parametrize("bad", [0, -1, float("nan"), float("inf")])
def test_invalid_max_time_is_rejected(bad):
    with pytest.raises(ValueError, match="max_time"):
        Sandbox(max_time=bad)


def test_paths_must_be_a_sequence_not_a_string():
    with pytest.raises(TypeError):
        Sandbox(allow_read="/tmp")  # type: ignore[arg-type]


def test_keyword_only_configuration():
    with pytest.raises(TypeError):
        Sandbox(["env"])  # type: ignore[misc]


# --- error types ------------------------------------------------------------


def test_syntax_error():
    with pytest.raises(ForgeSyntaxError) as exc:
        Sandbox().run("let x = 1\nlet = =")
    e = exc.value
    assert isinstance(e, ForgeError)
    assert e.kind == "syntax"
    assert e.stdout == ""
    assert e.line == 2
    assert "syntax error" in str(e)


def test_runtime_error_keeps_prior_output_and_line():
    with pytest.raises(ForgeRuntimeError) as exc:
        Sandbox().run('say "before"\nlet x = 1 / 0')
    e = exc.value
    assert e.kind == "runtime"
    assert e.stdout == "before\n"
    assert e.line == 2
    assert e.limit is None


def test_permission_error_is_default():
    with pytest.raises(ForgePermissionError) as exc:
        Sandbox().run('say "start"\nsay env.get("HOME")')
    e = exc.value
    assert e.kind == "permission_denied"
    assert str(e).startswith("permission denied: env")
    assert e.stdout == "start\n"


@pytest.mark.parametrize(
    "code, cap",
    [
        ('sh("echo pwned")', "run"),
        ('http.get("https://example.com")', "net"),
        ('db.open(":memory:")', "db"),
        ("exit(3)", "process"),
        ('let r = ask "hi"', "ai"),
    ],
)
def test_every_capability_is_denied_by_default(code, cap):
    with pytest.raises(ForgePermissionError, match=f"permission denied: {cap}"):
        Sandbox().run(code)


def test_grant_by_name():
    out = Sandbox(allow=["env"]).run(
        'env.set("FORGE_PY_T", "1")\nsay env.get("FORGE_PY_T")'
    )
    assert out.stdout == "1\n"


def test_timeout_of_infinite_loop():
    start = time.monotonic()
    with pytest.raises(ForgeTimeoutError) as exc:
        Sandbox(max_time=0.3).run('say "start"\nwhile true { }')
    elapsed = time.monotonic() - start
    e = exc.value
    assert e.kind == "timeout"
    assert e.limit == pytest.approx(0.3)
    assert e.stdout == "start\n"
    assert elapsed < 5


def test_timeout_does_not_wait_for_blocking_sleep():
    start = time.monotonic()
    with pytest.raises(ForgeTimeoutError):
        Sandbox(max_time=0.2).run("wait(30)")
    assert time.monotonic() - start < 5


def test_output_limit():
    with pytest.raises(ForgeOutputLimitError) as exc:
        Sandbox(max_output=1000, max_time=20).run('while true { say "spam spam" }')
    e = exc.value
    assert e.kind == "output_limit"
    assert e.limit == 1000
    assert len(e.stdout) <= 1000
    assert e.stdout.startswith("spam")
    # Under the limit is fine.
    assert Sandbox(max_output=10).run('say "ok"').stdout == "ok\n"


def test_cancel_token_from_another_thread():
    token = CancelToken()
    assert not token.cancelled
    timer = threading.Timer(0.2, token.cancel)
    timer.start()
    start = time.monotonic()
    try:
        with pytest.raises(ForgeCancelledError) as exc:
            Sandbox().run('say "started"\nwhile true { }', cancel=token)
    finally:
        timer.cancel()
    assert exc.value.kind == "cancelled"
    assert exc.value.stdout == "started\n"
    assert token.cancelled
    assert time.monotonic() - start < 5
    # An already-cancelled token never starts the program.
    with pytest.raises(ForgeCancelledError):
        Sandbox().run('say "never"', cancel=token)


def test_keyboard_interrupt_stops_the_run():
    # Simulates Ctrl-C: the waiting thread wakes up, cancels the run and
    # re-raises KeyboardInterrupt instead of hanging until the loop ends.
    timer = threading.Timer(0.3, _thread.interrupt_main)
    timer.start()
    start = time.monotonic()
    try:
        with pytest.raises(KeyboardInterrupt):
            Sandbox().run("while true { }")
    finally:
        timer.cancel()
    assert time.monotonic() - start < 5


# --- filesystem permissions -------------------------------------------------


def test_fs_denied_without_grant(tmp_path):
    (tmp_path / "in.txt").write_text("hello")
    with pytest.raises(ForgePermissionError, match="permission denied: fs.read"):
        Sandbox().run(f'say fs.read("{fs_path(tmp_path / "in.txt")}")')
    with pytest.raises(ForgePermissionError, match="permission denied: fs.write"):
        Sandbox().run(f'fs.write("{fs_path(tmp_path / "out.txt")}", "x")')
    assert not (tmp_path / "out.txt").exists()


def test_fs_scoped_to_temp_dir(tmp_path):
    data = tmp_path / "data"
    data.mkdir()
    (data / "in.txt").write_text("hello")
    (tmp_path / "secret.txt").write_text("s3cret")
    # Paths may be str or os.PathLike.
    sb = Sandbox(allow_read=[data], allow_write=[str(data)])
    d = fs_path(data)
    out = sb.run(
        f'fs.write("{d}/out.txt", fs.read("{d}/in.txt") + "!")\n'
        f'say fs.read("{d}/out.txt")'
    )
    assert out.stdout == "hello!\n"
    assert (data / "out.txt").read_text() == "hello!"
    with pytest.raises(ForgePermissionError, match="fs.read"):
        sb.run(f'say fs.read("{d}/../secret.txt")')
    with pytest.raises(ForgePermissionError, match="fs.write"):
        sb.run(f'fs.write("{fs_path(tmp_path)}/x.txt", "x")')
    assert not (tmp_path / "x.txt").exists()


def test_read_only_grant_does_not_allow_writes(tmp_path):
    sb = Sandbox(allow_read=[tmp_path])
    with pytest.raises(ForgePermissionError, match="fs.write"):
        sb.run(f'fs.write("{fs_path(tmp_path)}/x.txt", "x")')


# --- check ------------------------------------------------------------------


def test_check_clean_program():
    assert forge_lang.check('say "fine"') == []
    assert Sandbox().check('say "fine"') == []


def test_check_reports_parse_error_with_location():
    diags = forge_lang.check("let x = 1\nlet y = (")
    assert len(diags) == 1
    d = diags[0]
    assert isinstance(d, forge_lang.Diagnostic)
    assert d.is_error
    assert d.severity == "error"
    assert d.line == 2
    assert d.message
    assert str(d).startswith("line 2:")


def test_check_does_not_run_code(tmp_path):
    target = tmp_path / "never.txt"
    assert forge_lang.check(f'fs.write("{fs_path(target)}", "x")') == []
    assert not target.exists()


# --- concurrency ------------------------------------------------------------


def test_concurrent_runs_from_many_threads():
    sb = Sandbox(max_time=30)
    results = {}
    errors = []

    def worker(i):
        try:
            code = f"let mut s = 0\nfor k in range(0, 2000) {{ s = s + k }}\nsay {i} + s"
            results[i] = sb.run(code).stdout
        except Exception as e:  # pragma: no cover - reported below
            errors.append(e)

    threads = [threading.Thread(target=worker, args=(i,)) for i in range(16)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(60)
    assert not errors, errors
    expected_sum = sum(range(2000))
    assert results == {i: f"{i + expected_sum}\n" for i in range(16)}


def test_policies_do_not_leak_between_concurrent_sandboxes():
    allowed = Sandbox(allow=["env"])
    denied = Sandbox()
    outcomes = []

    def run(sb, name):
        for _ in range(20):
            try:
                sb.run('say env.has("PATH")')
                outcomes.append((name, "ok"))
            except ForgePermissionError:
                outcomes.append((name, "denied"))

    t1 = threading.Thread(target=run, args=(allowed, "allowed"))
    t2 = threading.Thread(target=run, args=(denied, "denied"))
    t1.start()
    t2.start()
    t1.join(60)
    t2.join(60)
    assert len(outcomes) == 40
    assert outcomes.count(("allowed", "ok")) == 20
    assert outcomes.count(("denied", "denied")) == 20


def test_gil_is_released_while_running():
    ticks = 0
    stop = threading.Event()

    def ticker():
        nonlocal ticks
        while not stop.is_set():
            ticks += 1
            time.sleep(0.001)

    t = threading.Thread(target=ticker)
    t.start()
    try:
        Sandbox().run("wait(0.5)")
    finally:
        stop.set()
        t.join()
    # With the GIL held for 0.5s the ticker could not have run at all.
    assert ticks > 20


# --- the README's agent "code mode" pattern ---------------------------------


def test_code_mode_tool_handler(tmp_path):
    workspace = tmp_path / "workspace"
    out_dir = workspace / "out"
    out_dir.mkdir(parents=True)
    (workspace / "names.txt").write_text("ada\ngrace\nlinus")
    sandbox = Sandbox(
        allow_read=[workspace], allow_write=[out_dir], max_time=10, max_output=32_000
    )

    def run_forge(code):
        problems = [str(d) for d in forge_lang.check(code) if d.is_error]
        if problems:
            return {"ok": False, "error": "syntax", "details": problems}
        try:
            return {"ok": True, "stdout": sandbox.run(code).stdout}
        except ForgeError as e:
            return {"ok": False, "error": e.kind, "message": str(e), "stdout": e.stdout}

    w = fs_path(workspace)
    ok = run_forge(
        f'let names = split(fs.read("{w}/names.txt"), "\\n")\n'
        f'fs.write("{w}/out/upper.txt", join(map(names, fn(n) {{ return upper(n) }}), ","))\n'
        'say "wrote {len(names)} names"'
    )
    assert ok == {"ok": True, "stdout": "wrote 3 names\n"}
    assert (out_dir / "upper.txt").read_text() == "ADA,GRACE,LINUS"

    assert run_forge("let y = (")["error"] == "syntax"
    denied = run_forge(f'say "hi"\nfs.write("{w}/escape.txt", "x")')
    assert denied["error"] == "permission_denied"
    assert denied["stdout"] == "hi\n"
