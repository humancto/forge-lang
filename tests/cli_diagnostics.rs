//! CLI-level checks for diagnostics and terminal hygiene: error snippets
//! name the file, piped output carries no ANSI color, block comments work.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const FORGE: &str = env!("CARGO_BIN_EXE_forge");

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forge-cli-diag-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn forge_in(cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(FORGE);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .env_remove("NO_COLOR")
        .env_remove("FORCE_COLOR")
        .env_remove("CLICOLOR_FORCE");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run forge")
}

#[test]
fn error_snippets_name_the_file_relative_to_cwd() {
    let dir = scratch("snippet");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/bad.fg"), "let x = 1\nsay x +\n").unwrap();
    std::fs::write(dir.join("src/boom.fg"), "let a = [1]\nsay a[5]\n").unwrap();

    for engine in [&["--interp"][..], &[][..]] {
        for (file, line) in [("src/bad.fg", 2), ("src/boom.fg", 2)] {
            let abs = dir.join(file).to_string_lossy().into_owned();
            let mut args = engine.to_vec();
            args.extend(["run", abs.as_str()]);
            let out = forge_in(&dir, &args, &[]);
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(!out.status.success());
            assert!(!stderr.contains("<source>"), "{}", stderr);
            assert!(
                stderr.contains(&format!("{}:{}:", file, line)),
                "expected `{}:{}:` in {:?} ({:?})",
                file,
                line,
                stderr,
                engine
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn piped_cli_output_has_no_color() {
    let dir = scratch("color");
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    std::fs::write(
        dir.join("tests/a_test.fg"),
        "@test\ndefine passes() {\n    assert_eq(1, 1)\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("log.fg"),
        "log.info(\"hello\")\nsay term.red(\"r\")\n",
    )
    .unwrap();

    // stdout/stderr are pipes here, so no chrome may be colored...
    let out = forge_in(&dir, &["test"], &[]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("passes"), "{}", text);
    assert!(!text.contains('\x1B'), "{:?}", text);

    // ...but values a program builds on purpose keep their escape codes.
    let out = forge_in(&dir, &["run", "log.fg"], &[]);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "\x1B[31mr\x1B[0m\n");
    assert!(!String::from_utf8_lossy(&out.stderr).contains('\x1B'));

    // FORCE_COLOR re-enables chrome color; NO_COLOR wins over it.
    let forced = forge_in(&dir, &["test"], &[("FORCE_COLOR", "1")]);
    assert!(String::from_utf8_lossy(&forced.stdout).contains('\x1B'));
    let both = forge_in(&dir, &["test"], &[("FORCE_COLOR", "1"), ("NO_COLOR", "1")]);
    assert!(!String::from_utf8_lossy(&both.stdout).contains('\x1B'));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn block_comments_run_on_both_engines() {
    let dir = scratch("comments");
    std::fs::write(
        dir.join("c.fg"),
        "/* header\n   spans lines */\nlet x = /* inline */ 41\nlet y = x + 1 /* a\n b */ say y\n/* unterminated? no */\n",
    )
    .unwrap();
    for engine in [&["--interp"][..], &[][..]] {
        let mut args = engine.to_vec();
        args.extend(["run", "c.fg"]);
        let out = forge_in(&dir, &args, &[]);
        assert!(out.status.success(), "{:?}", out);
        assert_eq!(String::from_utf8_lossy(&out.stdout), "42\n");
    }
    std::fs::write(dir.join("u.fg"), "say 1\n/* never closed\nsay 2\n").unwrap();
    let out = forge_in(&dir, &["run", "u.fg"], &[]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("unterminated block comment"), "{}", stderr);
    assert!(stderr.contains("u.fg:2:1"), "{}", stderr);
    let _ = std::fs::remove_dir_all(&dir);
}
