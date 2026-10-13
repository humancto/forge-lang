//! `forge run file.fg args...` passes everything after the file to the
//! program (V3 finding 13): `io.args()` is the file path followed by those
//! arguments, never forge's own flags, on both engines, with or without
//! `--sandbox`.

use std::path::PathBuf;
use std::process::{Command, Output};

const FORGE: &str = env!("CARGO_BIN_EXE_forge");

fn tmpdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "forge-program-args-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn forge(dir: &PathBuf, args: &[&str]) -> Output {
    Command::new(FORGE)
        .current_dir(dir)
        .args(args)
        .output()
        .expect("spawn forge")
}

fn stdout(o: &Output) -> String {
    assert!(
        o.status.success(),
        "forge failed: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8_lossy(&o.stdout).into_owned()
}

const PROGRAM: &str = r#"println(io.args())
println(io.args_get("--top"))
println(io.args_has("--interp"))
println(io.args_parse())
"#;

#[test]
fn arguments_after_the_file_go_to_the_program() {
    let dir = tmpdir("basic");
    std::fs::write(dir.join("main.fg"), PROGRAM).expect("write");
    for engine in [&[][..], &["--interp"][..]] {
        for sandbox in [&[][..], &["--sandbox"][..]] {
            let mut args: Vec<&str> = engine.to_vec();
            args.extend(sandbox);
            args.extend(["--error-format", "json", "run", "main.fg"]);
            args.extend(["--top", "3", "--interp", "-x", "--", "plain"]);
            let out = stdout(&forge(&dir, &args));
            assert_eq!(
                out,
                "[main.fg, --top, 3, --interp, -x, --, plain]\n3\ntrue\n\
                 { \"--top\": \"3\", \"--interp\": \"-x\" }\n",
                "{args:?}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_arguments_the_program_sees_only_its_path() {
    let dir = tmpdir("none");
    std::fs::write(dir.join("main.fg"), PROGRAM).expect("write");
    for engine in [&[][..], &["--interp"][..]] {
        let mut args: Vec<&str> = engine.to_vec();
        args.extend(["--allow-run", "run", "--allow-run", "main.fg"]);
        let out = stdout(&forge(&dir, &args));
        assert_eq!(out, "[main.fg]\nnull\nfalse\n{  }\n", "{args:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn spawned_tasks_see_the_same_arguments() {
    let dir = tmpdir("spawn");
    std::fs::write(
        dir.join("main.fg"),
        "let t = spawn { io.args() }\nprintln(await t)\n",
    )
    .expect("write");
    for engine in [&[][..], &["--interp"][..]] {
        let mut args: Vec<&str> = engine.to_vec();
        args.extend(["run", "main.fg", "a", "b"]);
        assert_eq!(stdout(&forge(&dir, &args)), "[main.fg, a, b]\n");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
