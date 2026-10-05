//! The shell used by `sh`, `shell`, `sh_lines`, `sh_json`, `sh_ok` and
//! `pipe_to` on both engines, plus `which` and program resolution for
//! `run_command`. Nothing else in the crate spawns a shell. (`run_command`
//! deliberately does **not** use a shell: it splits its string on
//! whitespace and executes the program directly, so it is not subject to
//! shell injection.)
//!
//! # Which shell
//!
//! - `FORGE_SHELL` (if set) is used as `<shell> -c <command>`, except that a
//!   shell named `cmd`/`cmd.exe` gets cmd's own syntax (below).
//! - Unix: `/bin/sh -c`.
//! - Windows: a POSIX `sh` found on `PATH` (Git for Windows, MSYS2, Cygwin)
//!   so scripts stay portable; otherwise `cmd.exe`.
//!
//! `cmd.exe` is the fallback rather than PowerShell because it is the direct
//! analogue of `sh -c`: present on every Windows install, starts in
//! milliseconds (PowerShell takes hundreds), and supports the same one-line
//! operators (`|`, `>`, `<`, `&&`, `||`). It is invoked as
//! `cmd /d /s /c "<command>"` — what Node.js's `child_process.exec` uses:
//! `/d` skips the AutoRun registry hook so a user's cmd profile cannot
//! change behaviour, and `/s /c "..."` makes cmd strip exactly the outer
//! quotes and run the rest verbatim. The command line is passed with
//! `raw_arg` because cmd.exe does not follow the MSVCRT quoting rules
//! `Command::arg` encodes for (a plain `.arg()` turns `echo "a b"` into
//! `echo \"a b\"`). Scripts that want PowerShell can call it explicitly:
//! `sh("powershell -NoProfile -Command Get-Date")`.
//!
//! Output is decoded lossily as UTF-8 by the callers; `str::lines` and
//! `trim_end` already strip the `\r\n` line endings Windows tools produce.
//!
//! [`which`] searches `PATH` in-process (no `/usr/bin/which` dependency)
//! and honors `PATHEXT` on Windows, so `which("cargo")` finds `cargo.exe`
//! and `which("npm")` finds `npm.cmd`.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A `Command` that runs `cmd` through the platform shell (see module docs).
/// Stdio is left at its defaults so callers can configure it.
pub fn command(cmd: &str) -> Command {
    command_with(&shell_program(), cmd)
}

/// `cmd` through the shell `program`, using cmd.exe syntax when `program`
/// is cmd.
fn command_with(program: &str, cmd: &str) -> Command {
    let mut command = Command::new(program);
    if is_cmd(program) {
        cmd_args(&mut command, cmd);
    } else {
        command.arg("-c").arg(cmd);
    }
    command
}

#[cfg(windows)]
fn cmd_args(command: &mut Command, cmd: &str) {
    use std::os::windows::process::CommandExt;
    command.raw_arg("/d /s /c \"").raw_arg(cmd).raw_arg("\"");
}

/// Off Windows there is no raw command line; cmd-compatible shells (Wine)
/// split arguments themselves.
#[cfg(not(windows))]
fn cmd_args(command: &mut Command, cmd: &str) {
    command.arg("/C").arg(cmd);
}

fn shell_program() -> String {
    if let Ok(custom) = std::env::var("FORGE_SHELL") {
        if !custom.trim().is_empty() {
            return custom;
        }
    }
    default_shell()
}

fn is_cmd(program: &str) -> bool {
    // Split on both separators so `C:\\...\\cmd.exe` is recognised on any host.
    let name = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program)
        .to_ascii_lowercase();
    name == "cmd" || name == "cmd.exe"
}

#[cfg(not(windows))]
fn default_shell() -> String {
    "/bin/sh".to_string()
}

#[cfg(windows)]
fn default_shell() -> String {
    static POSIX_SH: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let found = POSIX_SH.get_or_init(|| which("sh").map(|p| p.display().to_string()));
    match found {
        Some(sh) => sh.clone(),
        None => std::env::var("ComSpec")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "cmd.exe".to_string()),
    }
}

/// Run `cmd` through the shell and capture stdout and stderr.
pub fn output(cmd: &str) -> io::Result<Output> {
    command(cmd).stdin(Stdio::null()).output()
}

/// Run `cmd` through the shell with all output discarded; true when it
/// exits successfully.
pub fn succeeds(cmd: &str) -> io::Result<bool> {
    Ok(command(cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?
        .success())
}

/// Run `cmd` through the shell with `input` on its stdin and capture its
/// output.
///
/// The input is written from a separate thread: writing it all before
/// reading would deadlock once the child blocks on a full stdout pipe
/// while we block on a full stdin pipe (both ~64 KiB). A child that exits
/// without reading all of its input is not an error (`BrokenPipe` is
/// ignored), matching `printf ... | cmd` in a shell.
pub fn pipe(cmd: &str, input: &[u8]) -> io::Result<Output> {
    let mut child = command(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdin = child.stdin.take();
    let input = input.to_vec();
    let writer = std::thread::Builder::new()
        .name("forge-pipe-stdin".to_string())
        .spawn(move || -> io::Result<()> {
            use std::io::Write;
            if let Some(mut stdin) = stdin {
                match stdin.write_all(&input) {
                    Err(e) if e.kind() != io::ErrorKind::BrokenPipe => return Err(e),
                    _ => {}
                }
                // Dropping `stdin` closes the pipe so the child sees EOF.
            }
            Ok(())
        })?;
    let output = child.wait_with_output()?;
    writer
        .join()
        .map_err(|_| io::Error::other("stdin writer thread panicked"))??;
    Ok(output)
}

/// Locate an executable the way the platform shell would: search each
/// directory in `PATH` (honoring `PATHEXT` on Windows). A name containing a
/// path separator is checked as given instead. Returns `None` when nothing
/// executable matches.
pub fn which(name: &str) -> Option<PathBuf> {
    which_in(name, std::env::var_os("PATH"), std::env::var_os("PATHEXT"))
}

/// [`which`] with explicit `PATH` / `PATHEXT` values (for tests and callers
/// that resolve against a different environment). `pathext` is ignored on
/// Unix.
pub fn which_in(name: &str, path: Option<OsString>, pathext: Option<OsString>) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let exts = executable_extensions(pathext);
    if has_path_separator(name) {
        return candidates(Path::new(name), &exts).find(|p| is_executable(p));
    }
    let path = path?;
    std::env::split_paths(&path)
        .filter(|dir| !dir.as_os_str().is_empty())
        .find_map(|dir| candidates(&dir.join(name), &exts).find(|p| is_executable(p)))
}

fn has_path_separator(name: &str) -> bool {
    name.contains('/') || (cfg!(windows) && name.contains('\\'))
}

/// The extensions to try after a bare name. Empty on Unix; on Windows the
/// `PATHEXT` list (default `.COM;.EXE;.BAT;.CMD`), lower-cased.
fn executable_extensions(pathext: Option<OsString>) -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let raw = pathext
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_string());
    raw.split(';')
        .map(str::trim)
        .filter(|e| e.starts_with('.') && e.len() > 1)
        .map(str::to_ascii_lowercase)
        .collect()
}

/// `base` itself (when it could be run as named), then `base` + each
/// extension. On Windows a name that already has an extension
/// (`git.exe`, `tool.cmd`) is tried as given first, whether or not that
/// extension is listed in `PATHEXT` (cmd.exe runs an explicitly named
/// script); a bare name (`git`) is only tried with an extension appended.
fn candidates<'a>(base: &'a Path, exts: &'a [String]) -> impl Iterator<Item = PathBuf> + 'a {
    let as_is = exts.is_empty() || base.extension().is_some();
    let first = as_is.then(|| base.to_path_buf());
    first.into_iter().chain(exts.iter().map(move |ext| {
        let mut name = base.as_os_str().to_os_string();
        name.push(ext);
        PathBuf::from(name)
    }))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file())
        .unwrap_or(false)
}

/// Resolve the program for `run_command` (which runs without a shell). On
/// Windows `CreateProcess` only appends `.exe`, so a bare `npm` would not
/// find `npm.cmd`; resolving through [`which`] applies `PATHEXT` the way a
/// shell would. Elsewhere the name is used as given.
pub fn resolve_program(program: &str) -> OsString {
    if cfg!(windows) {
        if let Some(found) = which(program) {
            return found.into_os_string();
        }
    }
    OsString::from(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalized(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes)
            .replace("\r\n", "\n")
            .trim_end()
            .to_string()
    }

    fn stdout_of(cmd: &str) -> String {
        normalized(&output(cmd).expect("shell should start").stdout)
    }

    #[test]
    fn cmd_detection_uses_the_file_name() {
        assert!(is_cmd("cmd"));
        assert!(is_cmd("CMD.EXE"));
        assert!(is_cmd(r"C:\Windows\System32\cmd.exe"));
        assert!(!is_cmd("/bin/sh"));
        assert!(!is_cmd("bash"));
    }

    // The tests below use syntax shared by POSIX sh and cmd.exe (`echo`,
    // `exit N`, `cd`, `&&`, `||`, `1>&2`), so they hold for whichever shell
    // the platform default picks.

    #[test]
    fn runs_a_command_through_the_platform_shell() {
        let out = command("echo forge").output().expect("shell runs");
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "forge");
    }

    #[test]
    fn exit_status_is_reported() {
        let out = output("exit 3").expect("shell should start");
        assert_eq!(out.status.code(), Some(3));
        assert!(succeeds("exit 0").expect("start"));
        assert!(!succeeds("exit 1").expect("start"));
    }

    #[test]
    fn shell_operators_are_interpreted() {
        assert_eq!(stdout_of("echo one&& echo two"), "one\ntwo");
        assert_eq!(
            stdout_of("cd forge-no-such-dir|| echo fallback"),
            "fallback"
        );
    }

    #[test]
    fn stderr_is_captured_separately() {
        let out = output("echo oops 1>&2").expect("start");
        assert_eq!(normalized(&out.stderr), "oops");
        assert!(out.stdout.is_empty());
    }

    /// A stdin filter present under every default shell.
    #[cfg(not(windows))]
    const FILTER: &str = "cat";
    #[cfg(windows)]
    const FILTER: &str = "sort";

    #[test]
    fn pipe_feeds_stdin() {
        let out = pipe(FILTER, b"alpha\nbeta\n").expect("start");
        assert_eq!(normalized(&out.stdout), "alpha\nbeta");
    }

    /// Writing all input before reading output deadlocks once both pipes
    /// fill; 4 MiB is far beyond any OS pipe buffer.
    #[test]
    fn pipe_does_not_deadlock_on_large_io() {
        let line = "x".repeat(1023) + "\n";
        let input = line.repeat(4096);
        let out = pipe(FILTER, input.as_bytes()).expect("start");
        assert!(out.status.success());
        let got = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        assert_eq!(got.len(), input.len());
    }

    #[test]
    fn pipe_tolerates_child_ignoring_stdin() {
        let input = vec![b'y'; 1 << 20];
        let out = pipe("echo done", &input).expect("start");
        assert_eq!(normalized(&out.stdout), "done");
    }

    #[test]
    fn which_rejects_empty_and_missing() {
        assert_eq!(which(""), None);
        assert_eq!(which("forge-surely-not-a-real-program-xyz"), None);
    }

    #[test]
    fn which_without_path_finds_nothing() {
        assert_eq!(which_in("anything", None, None), None);
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "forge-shell-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[cfg(unix)]
    #[test]
    fn which_finds_sh_and_requires_exec_bit() {
        use std::os::unix::fs::PermissionsExt;
        let sh = which("sh").expect("sh is on PATH");
        assert!(sh.is_absolute(), "{}", sh.display());

        let dir = scratch_dir("which");
        let tool = dir.join("forge-tool");
        std::fs::write(&tool, "#!/bin/sh\n").expect("write");
        let path = Some(dir.clone().into_os_string());
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        assert_eq!(which_in("forge-tool", path.clone(), None), None);
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        assert_eq!(which_in("forge-tool", path, None), Some(tool.clone()));
        // A name with a separator is checked as given, not searched.
        assert_eq!(
            which_in(tool.to_str().expect("utf8"), None, None),
            Some(tool.clone())
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The cmd.exe path, forced (the default may be a POSIX sh on PATH).
    #[cfg(windows)]
    fn cmd_stdout(cmd: &str) -> String {
        let out = command_with("cmd.exe", cmd).output().expect("cmd runs");
        normalized(&out.stdout)
    }

    #[cfg(windows)]
    #[test]
    fn cmd_fallback_runs_commands_verbatim() {
        // `ver` is a cmd builtin; it fails under any other shell.
        assert!(cmd_stdout("ver").contains("Windows"));
        // Quotes reach cmd unescaped (`.arg()` would send `\"a b\"`).
        assert_eq!(cmd_stdout("echo \"a b\""), "\"a b\"");
        assert_eq!(cmd_stdout("echo one&& echo two"), "one\ntwo");
        assert_eq!(cmd_stdout("echo a | findstr a"), "a");
        let status = command_with("cmd.exe", "exit 4")
            .status()
            .expect("cmd runs");
        assert_eq!(status.code(), Some(4));
    }

    #[cfg(windows)]
    #[test]
    fn which_honors_pathext() {
        let cmd = which("cmd").expect("cmd.exe is on PATH");
        assert!(
            cmd.to_string_lossy()
                .to_ascii_lowercase()
                .ends_with("cmd.exe"),
            "{}",
            cmd.display()
        );

        let dir = scratch_dir("pathext");
        let script = dir.join("forge-tool.CMD");
        std::fs::write(&script, "@echo off\r\necho from-script\r\n").expect("write");
        let path = Some(dir.clone().into_os_string());
        // Found through PATHEXT, case-insensitively.
        let found = which_in("forge-tool", path.clone(), Some(".EXE;.CMD".into()))
            .expect("found via PATHEXT");
        assert_eq!(
            found.to_string_lossy().to_ascii_lowercase(),
            script.to_string_lossy().to_ascii_lowercase()
        );
        // Not found when .CMD is absent from PATHEXT.
        assert_eq!(
            which_in("forge-tool", path.clone(), Some(".EXE".into())),
            None
        );
        // An explicit extension is used as given.
        assert!(which_in("forge-tool.cmd", path, Some(".EXE".into())).is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(windows)]
    #[test]
    fn resolve_program_applies_pathext() {
        let resolved = resolve_program("cmd");
        assert!(resolved
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with("cmd.exe"));
    }
}
