//! Programs read piped input with `input()` or `fs.read("/dev/stdin")` /
//! `fs.lines("/dev/stdin")`, on both engines, also under `--sandbox`
//! (stdin is not a file grant: the path resolves to a pipe).

use std::io::Write;
use std::process::{Command, Stdio};

fn run(engine_args: &[&str], code: &str, stdin: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(engine_args)
        .args(["--sandbox", "-e", code])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn forge");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success(), "{:?}", out);
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn piped_input_is_readable_on_both_engines() {
    for engine in [&[][..], &["--interp"][..]] {
        assert_eq!(
            run(
                engine,
                "println(len(fs.lines(\"/dev/stdin\")))",
                "a\nb\nc\n"
            ),
            "3\n"
        );
        assert_eq!(
            run(
                engine,
                "let t = fs.read(\"/dev/stdin\")\nprint(t)",
                "x y\nz\n"
            ),
            "x y\nz\n"
        );
        assert_eq!(run(engine, "println(input())", "one\ntwo\n"), "one\ntwo\n");
    }
}
