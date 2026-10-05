//! Native plugins end to end (`import native`, rfcs/0006-native-plugins.md).
//!
//! * Builds `examples/plugins/hello_rust` (a cdylib using the
//!   `crates/forge-plugin` SDK) with cargo, and on Unix compiles
//!   `examples/plugins/hello_c` with the system C compiler against
//!   `crates/forge-plugin/include/forge_plugin.h`.
//! * Runs each example script on **both engines** and requires identical,
//!   expected output — the differential check for plugin calls.
//! * Checks the `ffi` capability: denied without `--allow-ffi`, scoped by
//!   path, denied under `--sandbox`.
//!
//! The plugin build uses a persistent target directory under
//! `CARGO_TARGET_TMPDIR`, so only the first run pays for compiling `syn`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scratch_dir(tag: &str) -> PathBuf {
    // A per-process counter makes the name unique even when parallel tests
    // read the clock in the same tick (macOS `SystemTime` is microsecond
    // resolution, which let two tests share and clobber one directory).
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "forge-native-{}-{}-{}-{}",
        tag,
        std::process::id(),
        n,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// File name cargo gives a cdylib called `name` on this platform.
fn cdylib_file_name(name: &str) -> String {
    format!(
        "{}{}{}",
        std::env::consts::DLL_PREFIX,
        name,
        std::env::consts::DLL_SUFFIX
    )
}

/// Build the example Rust plugin once per test process; returns the path
/// of the shared library.
fn hello_rust_library() -> &'static Path {
    static LIB: OnceLock<PathBuf> = OnceLock::new();
    LIB.get_or_init(|| {
        let target_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("native-plugin-target");
        let manifest = repo_root().join("examples/plugins/hello_rust/Cargo.toml");
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let output = Command::new(cargo)
            .arg("build")
            .arg("--locked")
            .arg("--manifest-path")
            .arg(&manifest)
            .env("CARGO_TARGET_DIR", &target_dir)
            .output()
            .expect("run cargo build for the example plugin");
        assert!(
            output.status.success(),
            "building examples/plugins/hello_rust failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let lib = target_dir
            .join("debug")
            .join(cdylib_file_name("hello_rust"));
        assert!(lib.is_file(), "expected {}", lib.display());
        lib
    })
}

/// A scratch copy of `examples/plugins/hello_rust/main.fg` with the built
/// library where the script expects it (`target/debug/`).
fn hello_rust_app() -> PathBuf {
    let dir = scratch_dir("rust");
    fs::copy(
        repo_root().join("examples/plugins/hello_rust/main.fg"),
        dir.join("main.fg"),
    )
    .expect("copy main.fg");
    let lib_dir = dir.join("target/debug");
    fs::create_dir_all(&lib_dir).expect("mkdir target/debug");
    let lib = hello_rust_library();
    fs::copy(lib, lib_dir.join(lib.file_name().expect("file name"))).expect("copy library");
    dir
}

fn forge(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(args)
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .output()
        .expect("run forge")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).replace("\r\n", "\n")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

/// Run `main.fg` in `dir` on both engines; both must succeed with the
/// same output, which is returned.
fn run_on_both_engines(dir: &Path, flags: &[&str]) -> String {
    let mut outputs = Vec::new();
    for interp in [false, true] {
        let mut args: Vec<&str> = Vec::new();
        if interp {
            args.push("--interp");
        }
        args.extend_from_slice(flags);
        args.extend_from_slice(&["run", "main.fg"]);
        let out = forge(dir, &args);
        assert!(
            out.status.success(),
            "engine {} failed:\nstdout:\n{}\nstderr:\n{}",
            if interp { "interp" } else { "vm" },
            stdout(&out),
            stderr(&out)
        );
        assert!(
            !stderr(&out).contains("falling back to interpreter"),
            "the VM must run native imports itself: {}",
            stderr(&out)
        );
        outputs.push(stdout(&out));
    }
    assert_eq!(outputs[0], outputs[1], "VM and interpreter disagree");
    outputs.remove(0)
}

const HELLO_RUST_EXPECTED: &str = "\
42
Hello, Forge! (from Rust)
3.5
6.5
5
jumps
[the, quick]
e768e8afb3fb4470
[3, 2, 1]
[11, 12, 13]
caught: division by zero
caught: explode() panicked: boom
caught: add(): argument 2 (b) expected int, got string
caught: add() expects 2 arguments, got 1
";

#[test]
fn rust_plugin_runs_on_both_engines() {
    let dir = hello_rust_app();
    let out = run_on_both_engines(&dir, &["--allow-ffi"]);
    assert_eq!(out, HELLO_RUST_EXPECTED);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ffi_is_denied_by_default_and_scoped_by_path() {
    let dir = hello_rust_app();
    for engine in [&[][..], &["--interp"][..]] {
        // No grant: `forge run` scripts need --allow-ffi.
        let mut args = engine.to_vec();
        args.extend_from_slice(&["run", "main.fg"]);
        let out = forge(&dir, &args);
        assert!(!out.status.success(), "must be denied");
        assert!(
            stderr(&out).contains("permission denied: ffi") && stderr(&out).contains("--allow-ffi"),
            "{}",
            stderr(&out)
        );

        // A grant for another directory does not cover the library.
        fs::create_dir_all(dir.join("elsewhere")).expect("mkdir");
        let mut args = engine.to_vec();
        args.extend_from_slice(&["--allow-ffi=elsewhere", "run", "main.fg"]);
        let out = forge(&dir, &args);
        assert!(!out.status.success(), "scoped grant must not cover target/");
        assert!(
            stderr(&out).contains("permission denied: ffi"),
            "{}",
            stderr(&out)
        );

        // --sandbox denies it even though everything else is granted below.
        let mut args = engine.to_vec();
        args.extend_from_slice(&["--sandbox", "--allow-read", "run", "main.fg"]);
        let out = forge(&dir, &args);
        assert!(!out.status.success(), "--sandbox must deny ffi");
        assert!(
            stderr(&out).contains("permission denied: ffi"),
            "{}",
            stderr(&out)
        );
    }
    // A grant scoped to the library's directory works.
    let out = run_on_both_engines(&dir, &["--allow-ffi=target"]);
    assert_eq!(out, HELLO_RUST_EXPECTED);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn import_errors_name_the_problem() {
    let dir = hello_rust_app();
    let cases: &[(&str, &str)] = &[
        (
            "import { nope } from native \"target/debug/libhello_rust\"\n",
            "native library 'target/debug/libhello_rust' has no function 'nope' (it exports: add, greet, divide, sum, word_stats, fnv1a, reverse_bytes, explode)",
        ),
        (
            "import native \"target/debug/libmissing\" as m\n",
            "native library 'target/debug/libmissing' not found",
        ),
    ];
    for (source, expected) in cases {
        fs::write(dir.join("main.fg"), source).expect("write script");
        for engine in [&[][..], &["--interp"][..]] {
            let mut args = engine.to_vec();
            args.extend_from_slice(&["--allow-ffi", "run", "main.fg"]);
            let out = forge(&dir, &args);
            assert!(!out.status.success());
            assert!(
                stderr(&out).contains(expected),
                "{:?}: expected {:?} in:\n{}",
                engine,
                expected,
                stderr(&out)
            );
        }
    }
    // A file that is not a plugin is rejected without crashing.
    let junk = dir.join(cdylib_file_name("junk"));
    fs::write(&junk, b"definitely not a shared library").expect("write junk");
    fs::write(dir.join("main.fg"), "import native \"junk\" as j\n").expect("write script");
    for engine in [&[][..], &["--interp"][..]] {
        let mut args = engine.to_vec();
        args.extend_from_slice(&["--allow-ffi", "run", "main.fg"]);
        let out = forge(&dir, &args);
        assert!(!out.status.success());
        assert!(
            stderr(&out).contains("cannot load native library 'junk'"),
            "{}",
            stderr(&out)
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

/// Compile a C plugin with the system compiler (`$CC` or `cc`).
#[cfg(unix)]
fn compile_c_plugin(source: &Path, out_dir: &Path, name: &str) -> PathBuf {
    let lib = out_dir.join(cdylib_file_name(name));
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let mut cmd = Command::new(&cc);
    cmd.arg("-std=c11")
        .arg("-Wall")
        .arg("-Werror")
        .arg("-shared")
        .arg("-fPIC")
        .arg("-I")
        .arg(repo_root().join("crates/forge-plugin/include"))
        .arg(source)
        .arg("-o")
        .arg(&lib);
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("run C compiler '{}': {}", cc, e));
    assert!(
        output.status.success(),
        "compiling {} failed:\n{}",
        source.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    lib
}

#[cfg(unix)]
#[test]
fn c_plugin_implements_the_same_abi() {
    let dir = scratch_dir("c");
    compile_c_plugin(
        &repo_root().join("examples/plugins/hello_c/hello.c"),
        &dir,
        "hello_c",
    );
    fs::copy(
        repo_root().join("examples/plugins/hello_c/main.fg"),
        dir.join("main.fg"),
    )
    .expect("copy main.fg");
    let out = run_on_both_engines(&dir, &["--allow-ffi"]);
    assert_eq!(
        out,
        "42\nHELLO FROM C\n0\n3\n3\ncaught: checked_div(): division by zero\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn plugins_built_for_another_abi_version_are_refused() {
    let dir = scratch_dir("abi");
    let source = dir.join("future.c");
    fs::write(
        &source,
        "#include \"forge_plugin.h\"\n\
         __attribute__((visibility(\"default\"))) uint32_t forge_plugin_abi_version(void) { return 2; }\n\
         __attribute__((visibility(\"default\"))) const ForgePlugin *forge_plugin_register(void) { return 0; }\n",
    )
    .expect("write source");
    compile_c_plugin(&source, &dir, "future");
    fs::write(dir.join("main.fg"), "import native \"future\" as f\n").expect("write script");
    for engine in [&[][..], &["--interp"][..]] {
        let mut args = engine.to_vec();
        args.extend_from_slice(&["--allow-ffi", "run", "main.fg"]);
        let out = forge(&dir, &args);
        assert!(!out.status.success());
        assert!(
            stderr(&out)
                .contains("built for plugin ABI version 2, but this Forge supports version 1"),
            "{}",
            stderr(&out)
        );
    }
    let _ = fs::remove_dir_all(&dir);
}
