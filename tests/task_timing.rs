//! Spawned tasks run concurrently and `wait` is not slower than asked, on
//! both engines (eval/V2_FINDINGS.md, finding 27: a three-stage channel
//! pipeline ran as slowly as a serial one on the VM on macOS, whose timers
//! oversleep, because the VM's `wait` added up 50 ms slices and so paid the
//! oversleep twice per `wait(0.1)`).
//!
//! The margins are generous (debug build, loaded CI machines): each check
//! fails only when the run is close to what the serialized / per-slice
//! behaviour would cost.

use std::process::Command;
use std::time::{Duration, Instant};

const FORGE: &str = env!("CARGO_BIN_EXE_forge");
const ENGINES: [&[&str]; 2] = [&[], &["--interp"]];

fn script(tag: &str, source: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "forge_task_timing_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let file = dir.join("main.fg");
    std::fs::write(&file, source).expect("write script");
    file
}

/// Run `cmd`, check that it succeeds with `expected` on stdout, and return
/// how long it took.
fn timed(mut cmd: Command, expected: &str) -> Duration {
    let start = Instant::now();
    let out = cmd.output().expect("spawn forge");
    let took = start.elapsed();
    assert!(
        out.status.success(),
        "forge failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), expected);
    took
}

fn forge(engine: &[&str], file: &std::path::Path) -> Command {
    let mut cmd = Command::new(FORGE);
    cmd.args(engine).arg("--sandbox").arg("run").arg(file);
    cmd
}

#[test]
fn spawned_tasks_wait_concurrently_on_both_engines() {
    // Eight tasks of 0.5 s each: 4 s if they ran one after another.
    let file = script(
        "spawn",
        "let mut tasks = []\n\
         repeat 8 times {\n    tasks.push(spawn { wait(0.5)\n return 1 })\n}\n\
         let mut n = 0\n\
         for t in tasks { n += await t }\n\
         println(n)\n",
    );
    for engine in ENGINES {
        let took = timed(forge(engine, &file), "8");
        assert!(
            took < Duration::from_millis(2500),
            "{:?}: 8 tasks waiting 0.5 s each took {:?}",
            engine,
            took
        );
    }
}

#[test]
fn channel_pipeline_stages_overlap_on_both_engines() {
    // The shape of eval task `staged_pipeline`: three stages, bounded
    // channels, 0.1 s per record and stage. One stage after another:
    // 12 * 3 * 0.1 = 3.6 s; overlapped: about 1.4 s.
    let file = script(
        "pipeline",
        "let a = channel(2)\n\
         let b = channel(2)\n\
         let p = spawn {\n    for i in range(12) { wait(0.1)\n send(a, i) }\n    close(a)\n    return null\n}\n\
         let d = spawn {\n    loop {\n        let v = receive(a)\n        if v == null { break }\n        wait(0.1)\n        send(b, v * 2)\n    }\n    close(b)\n    return null\n}\n\
         let mut sum = 0\n\
         loop {\n    let v = receive(b)\n    if v == null { break }\n    sum += v\n    wait(0.1)\n}\n\
         println(sum)\n",
    );
    for engine in ENGINES {
        let took = timed(forge(engine, &file), "132");
        assert!(
            took < Duration::from_millis(2800),
            "{:?}: the pipeline took {:?}",
            engine,
            took
        );
    }
}

/// Coarse timers, reproduced on Linux: a 100 ms timer slack
/// (`PR_SET_TIMERSLACK`, inherited by the child and its threads) lets every
/// timed sleep end up to 100 ms late, like the macOS CI runners. `wait` must
/// then be late at most once per call, not once per internal slice.
#[cfg(target_os = "linux")]
#[test]
fn wait_oversleeps_once_per_call_with_coarse_timers() {
    use std::os::unix::process::CommandExt;

    const PR_SET_TIMERSLACK: libc::c_int = 29;
    const SLACK_NS: libc::c_ulong = 100_000_000;

    // Four waits of 0.25 s: at least 1 s, at most about 1.4 s when each is
    // late once. Counting 50 ms slices (each late) took about 3 s.
    let file = script(
        "slack",
        "repeat 4 times { wait(0.25) }\nprintln(\"done\")\n",
    );
    for engine in ENGINES {
        let mut cmd = forge(engine, &file);
        // SAFETY: `prctl` is async-signal-safe and touches only the
        // calling (child) thread's timer slack.
        unsafe {
            cmd.pre_exec(|| {
                if libc::prctl(PR_SET_TIMERSLACK, SLACK_NS, 0, 0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let took = timed(cmd, "done");
        assert!(
            took >= Duration::from_secs(1),
            "{:?}: wait returned early ({:?})",
            engine,
            took
        );
        assert!(
            took < Duration::from_millis(2000),
            "{:?}: four wait(0.25) took {:?} with 100 ms timer slack",
            engine,
            took
        );
    }
}
