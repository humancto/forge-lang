use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn build_native_launcher(source: &str, source_path: &Path) -> Result<PathBuf, String> {
    let c_source_fn = |forge_bin: &str| native_launcher_c_source(source.as_bytes(), forge_bin);
    compile_launcher(source_path, "native", c_source_fn)
}

pub fn build_native_aot(bytecode: &[u8], source_path: &Path) -> Result<PathBuf, String> {
    // Try standalone build first (links against libforge.a — no forge needed at runtime)
    if let Some(lib_dir) = find_libforge_dir() {
        return build_standalone_aot(bytecode, source_path, &lib_dir);
    }
    // Fall back to launcher mode (requires forge at runtime)
    let c_source_fn = |forge_bin: &str| aot_launcher_c_source(bytecode, forge_bin);
    compile_launcher(source_path, "aot", c_source_fn)
}

/// Find the directory containing libforge_lang.a
pub fn find_libforge_dir() -> Option<PathBuf> {
    // Check FORGE_LIB_DIR env var first
    if let Ok(dir) = env::var("FORGE_LIB_DIR") {
        let path = PathBuf::from(&dir).join("libforge_lang.a");
        if path.exists() {
            return Some(PathBuf::from(dir));
        }
    }
    // Check next to the forge binary
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let path = dir.join("libforge_lang.a");
            if path.exists() {
                return Some(dir.to_path_buf());
            }
        }
    }
    None
}

/// Build a standalone AOT binary that links against libforge.a
#[cfg(unix)]
fn build_standalone_aot(
    bytecode: &[u8],
    source_path: &Path,
    lib_dir: &Path,
) -> Result<PathBuf, String> {
    let output_path = native_output_path(source_path);
    let byte_list = bytecode
        .iter()
        .map(|byte| byte.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    let c_source = format!(
        r#"#include <stddef.h>
#include <stdint.h>

extern int32_t forge_execute_bytecode(const uint8_t *bytecode, size_t len);

static const unsigned char FORGE_BYTECODE[] = {{ {byte_list} }};
static const size_t FORGE_BYTECODE_LEN = sizeof(FORGE_BYTECODE);

int main(void) {{
    return (int)forge_execute_bytecode(FORGE_BYTECODE, FORGE_BYTECODE_LEN);
}}
"#,
        byte_list = byte_list,
    );

    let build_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("failed to create timestamp: {e}"))?
        .as_nanos();
    let c_path = env::temp_dir().join(format!("forge-aot-{}-{}.c", std::process::id(), build_id));
    fs::write(&c_path, c_source).map_err(|e| format!("failed to write AOT source: {e}"))?;

    let mut cmd = Command::new("cc");
    cmd.arg("-O2")
        .arg(&c_path)
        .arg("-o")
        .arg(&output_path)
        .arg(format!("-L{}", lib_dir.display()))
        .arg("-lforge_lang")
        .arg("-lm")
        .arg("-lpthread")
        .arg("-lresolv");

    // macOS requires additional frameworks for system dependencies
    #[cfg(target_os = "macos")]
    {
        cmd.arg("-framework").arg("CoreFoundation");
        cmd.arg("-framework").arg("Security");
        cmd.arg("-framework").arg("SystemConfiguration");
        cmd.arg("-framework").arg("IOKit");
        cmd.arg("-liconv");
    }

    // Linux requires libdl
    #[cfg(target_os = "linux")]
    {
        cmd.arg("-ldl");
    }

    let status = cmd.status().map_err(|e| {
        let _ = fs::remove_file(&c_path);
        format!("failed to invoke C compiler for standalone AOT: {e}")
    })?;
    let _ = fs::remove_file(&c_path);

    if !status.success() {
        return Err(format!(
            "standalone AOT compilation failed for '{}' (try without FORGE_LIB_DIR for launcher mode)",
            output_path.display()
        ));
    }

    Ok(output_path)
}

#[cfg(not(unix))]
fn build_standalone_aot(
    _bytecode: &[u8],
    _source_path: &Path,
    _lib_dir: &Path,
) -> Result<PathBuf, String> {
    Err("standalone AOT is currently supported on Unix-like systems only".to_string())
}

fn compile_launcher<F>(source_path: &Path, mode: &str, make_c_source: F) -> Result<PathBuf, String>
where
    F: FnOnce(&str) -> String,
{
    #[cfg(not(unix))]
    {
        let _ = (source_path, mode, make_c_source);
        return Err(format!(
            "--{mode} is currently supported on Unix-like systems only"
        ));
    }

    #[cfg(unix)]
    {
        let output_path = native_output_path(source_path);
        let default_forge_bin = env::var("FORGE_NATIVE_FORGE_BIN")
            .ok()
            .filter(|value| !value.is_empty())
            .or_else(|| {
                env::current_exe()
                    .ok()
                    .map(|path| path.display().to_string())
            })
            .unwrap_or_else(|| "forge".to_string());

        let build_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("failed to create {mode} timestamp: {e}"))?
            .as_nanos();
        let c_path = env::temp_dir().join(format!(
            "forge-{mode}-{}-{}.c",
            std::process::id(),
            build_id
        ));
        let c_source = make_c_source(&default_forge_bin);
        fs::write(&c_path, c_source)
            .map_err(|e| format!("failed to write {mode} launcher source: {e}"))?;

        let status = Command::new("cc")
            .arg("-O2")
            .arg(&c_path)
            .arg("-o")
            .arg(&output_path)
            .status()
            .map_err(|e| format!("failed to invoke C compiler for --{mode}: {e}"))?;
        let _ = fs::remove_file(&c_path);

        if !status.success() {
            return Err(format!(
                "{mode} launcher compilation failed for '{}'",
                output_path.display()
            ));
        }

        Ok(output_path)
    }
}

pub fn native_output_path(source_path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        return source_path.with_extension("exe");
    }

    #[cfg(not(windows))]
    {
        let mut output_path = source_path.to_path_buf();
        output_path.set_extension("");
        output_path
    }
}

fn launcher_c_template(
    data_var: &str,
    len_var: &str,
    byte_list: &str,
    default_forge_bin: &str,
    tmp_prefix: &str,
    file_ext: &str,
    ext_len: usize,
) -> String {
    format!(
        r#"#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static const unsigned char {data_var}[] = {{ {byte_list} }};
static const size_t {len_var} = sizeof({data_var});
static const char *DEFAULT_FORGE_BIN = "{default_forge_bin}";

int main(int argc, char **argv) {{
    const char *tmpdir = getenv("TMPDIR");
    if (!tmpdir) tmpdir = "/tmp";
    char tmp_template[256];
    snprintf(tmp_template, sizeof(tmp_template), "%s/{tmp_prefix}-XXXXXX", tmpdir);
    int fd = mkstemp(tmp_template);
    if (fd == -1) {{
        perror("mkstemp");
        return 1;
    }}

    char program_path[sizeof(tmp_template) + {ext_len}];
    if (snprintf(program_path, sizeof(program_path), "%s.{file_ext}", tmp_template) < 0) {{
        perror("snprintf");
        close(fd);
        unlink(tmp_template);
        return 1;
    }}
    if (rename(tmp_template, program_path) != 0) {{
        perror("rename");
        close(fd);
        unlink(tmp_template);
        return 1;
    }}

    FILE *program = fdopen(fd, "wb");
    if (!program) {{
        perror("fdopen");
        close(fd);
        unlink(program_path);
        return 1;
    }}
    if (fwrite({data_var}, 1, {len_var}, program) != {len_var}) {{
        perror("fwrite");
        fclose(program);
        unlink(program_path);
        return 1;
    }}
    if (fclose(program) != 0) {{
        perror("fclose");
        unlink(program_path);
        return 1;
    }}

    const char *forge_bin = getenv("FORGE_NATIVE_FORGE_BIN");
    if (!forge_bin || !forge_bin[0]) {{
        forge_bin = DEFAULT_FORGE_BIN;
    }}

    char **args = calloc((size_t)argc + 3, sizeof(char *));
    if (!args) {{
        fprintf(stderr, "calloc failed\n");
        unlink(program_path);
        return 1;
    }}

    args[0] = (char *)forge_bin;
    args[1] = "run";
    args[2] = program_path;
    for (int i = 1; i < argc; ++i) {{
        args[i + 2] = argv[i];
    }}

    pid_t child = fork();
    if (child == 0) {{
        execv(forge_bin, args);
        if (strcmp(forge_bin, "forge") != 0) {{
            args[0] = "forge";
            execvp("forge", args);
        }}
        perror("exec forge");
        _exit(127);
    }}
    if (child < 0) {{
        perror("fork");
        unlink(program_path);
        free(args);
        return 1;
    }}

    int status = 0;
    if (waitpid(child, &status, 0) < 0) {{
        perror("waitpid");
        unlink(program_path);
        free(args);
        return 1;
    }}

    unlink(program_path);
    free(args);
    if (WIFEXITED(status)) {{
        return WEXITSTATUS(status);
    }}
    if (WIFSIGNALED(status)) {{
        return 128 + WTERMSIG(status);
    }}
    return 1;
}}
"#,
        data_var = data_var,
        len_var = len_var,
        byte_list = byte_list,
        default_forge_bin = c_string_escape(default_forge_bin),
        tmp_prefix = tmp_prefix,
        file_ext = file_ext,
        ext_len = ext_len,
    )
}

fn native_launcher_c_source(source: &[u8], default_forge_bin: &str) -> String {
    let byte_list = source
        .iter()
        .map(|byte| byte.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    launcher_c_template(
        "FORGE_PROGRAM",
        "FORGE_PROGRAM_LEN",
        &byte_list,
        default_forge_bin,
        "forge-native",
        "fg",
        4, // ".fg" + null
    )
}

fn aot_launcher_c_source(bytecode: &[u8], default_forge_bin: &str) -> String {
    let byte_list = bytecode
        .iter()
        .map(|byte| byte.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    launcher_c_template(
        "FORGE_BYTECODE",
        "FORGE_BYTECODE_LEN",
        &byte_list,
        default_forge_bin,
        "forge-aot",
        "fgc",
        5, // ".fgc" + null
    )
}

fn c_string_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '\\' => ['\\', '\\'].into_iter().collect::<Vec<_>>(),
            '"' => ['\\', '"'].into_iter().collect::<Vec<_>>(),
            '\n' => ['\\', 'n'].into_iter().collect::<Vec<_>>(),
            '\r' => ['\\', 'r'].into_iter().collect::<Vec<_>>(),
            '\t' => ['\\', 't'].into_iter().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_output_path_drops_source_extension() {
        let path = Path::new("/tmp/hello.fg");
        assert_eq!(native_output_path(path), PathBuf::from("/tmp/hello"));
    }

    #[test]
    fn native_launcher_source_embeds_program_and_runtime_path() {
        let source = "println(\"hi\")";
        let c_source = native_launcher_c_source(source.as_bytes(), "/tmp/forge-bin");
        assert!(c_source.contains("static const unsigned char FORGE_PROGRAM[]"));
        assert!(c_source.contains("/tmp/forge-bin"));
        assert!(c_source.contains("execv"));
    }

    #[cfg(unix)]
    #[test]
    fn build_native_launcher_emits_binary() {
        if Command::new("cc").arg("--version").output().is_err() {
            return;
        }

        let temp_root = std::env::temp_dir().join(format!(
            "forge-native-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_root).unwrap();
        let source_path = temp_root.join("hello.fg");
        std::fs::write(&source_path, "println(\"hi\")").unwrap();

        let output_path = build_native_launcher("println(\"hi\")", &source_path).unwrap();
        assert!(output_path.exists());
        let metadata = std::fs::metadata(&output_path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert!(metadata.permissions().mode() & 0o111 != 0);
        }

        let _ = std::fs::remove_file(output_path);
        let _ = std::fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn aot_launcher_source_embeds_bytecode_not_source() {
        let bytecode: Vec<u8> = vec![0xF0, 0x01, 0x02, 0x03, 42, 99];
        let c_source = aot_launcher_c_source(&bytecode, "/tmp/forge-bin");
        assert!(c_source.contains("static const unsigned char FORGE_BYTECODE[]"));
        assert!(c_source.contains("/tmp/forge-bin"));
        assert!(c_source.contains(".fgc"));
        assert!(c_source.contains("forge-aot-"));
        assert!(!c_source.contains("FORGE_PROGRAM[]"));
    }

    #[cfg(unix)]
    #[test]
    fn build_native_aot_emits_binary() {
        if Command::new("cc").arg("--version").output().is_err() {
            return;
        }

        let temp_root = std::env::temp_dir().join(format!(
            "forge-aot-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_root).unwrap();
        let source_path = temp_root.join("hello.fg");
        std::fs::write(&source_path, "println(\"hi\")").unwrap();

        let bytecode = vec![0x00, 0x01, 0x02, 0x03];
        let output_path = build_native_aot(&bytecode, &source_path).unwrap();
        assert!(output_path.exists());
        let metadata = std::fs::metadata(&output_path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert!(metadata.permissions().mode() & 0o111 != 0);
        }

        let _ = std::fs::remove_file(output_path);
        let _ = std::fs::remove_dir_all(&temp_root);
    }
}
