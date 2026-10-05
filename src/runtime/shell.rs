//! The shell used by `sh`, `shell`, `sh_lines`, `sh_json`, `sh_ok` and
//! `pipe_to` on both engines.
//!
//! - `FORGE_SHELL` (if set) is used as `<shell> -c <command>`, except that a
//!   shell named `cmd`/`cmd.exe` gets `/C`.
//! - Unix: `/bin/sh -c`.
//! - Windows: a POSIX `sh` found on `PATH` (Git for Windows, MSYS2, Cygwin)
//!   so scripts stay portable; otherwise `cmd /C`.

use std::process::Command;

/// A `Command` that runs `cmd` through the platform shell (see module docs).
pub fn command(cmd: &str) -> Command {
    let (program, flag) = shell_program();
    let mut command = Command::new(program);
    command.arg(flag).arg(cmd);
    command
}

fn shell_program() -> (String, &'static str) {
    if let Ok(custom) = std::env::var("FORGE_SHELL") {
        if !custom.trim().is_empty() {
            let flag = if is_cmd(&custom) { "/C" } else { "-c" };
            return (custom, flag);
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
fn default_shell() -> (String, &'static str) {
    ("/bin/sh".to_string(), "-c")
}

#[cfg(windows)]
fn default_shell() -> (String, &'static str) {
    static POSIX_SH: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let found = POSIX_SH.get_or_init(|| {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|dir| dir.join("sh.exe"))
            .find(|candidate| candidate.is_file())
            .map(|p| p.display().to_string())
    });
    match found {
        Some(sh) => (sh.clone(), "-c"),
        None => ("cmd".to_string(), "/C"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_detection_uses_the_file_name() {
        assert!(is_cmd("cmd"));
        assert!(is_cmd("CMD.EXE"));
        assert!(is_cmd(r"C:\Windows\System32\cmd.exe"));
        assert!(!is_cmd("/bin/sh"));
        assert!(!is_cmd("bash"));
    }

    #[test]
    fn runs_a_command_through_the_platform_shell() {
        let out = command("echo forge").output().expect("shell runs");
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "forge");
    }
}
