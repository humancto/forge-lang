use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn build_native_launcher(source: &str, source_path: &Path) -> Result<PathBuf, String> {
    #[cfg(not(unix))]
    {
        let _ = source;
        let _ = source_path;
        return Err("--native is currently supported on Unix-like systems only".to_string());
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
            .map_err(|e| format!("failed to create native launcher timestamp: {}", e))?
            .as_nanos();
        let c_path = env::temp_dir().join(format!(
            "forge-native-{}-{}.c",
            std::process::id(),
            build_id
        ));
        let c_source = native_launcher_c_source(source.as_bytes(), &default_forge_bin);
        fs::write(&c_path, c_source)
            .map_err(|e| format!("failed to write native launcher source: {}", e))?;

        let status = Command::new("cc")
            .arg("-O2")
            .arg(&c_path)
            .arg("-o")
            .arg(&output_path)
            .status()
            .map_err(|e| format!("failed to invoke C compiler for --native: {}", e))?;
        let _ = fs::remove_file(&c_path);

        if !status.success() {
            return Err(format!(
                "native launcher compilation failed for '{}'",
                output_path.display()
            ));
        }

        Ok(output_path)
    }
}

pub fn build_native_aot(bytecode: &[u8], source_path: &Path) -> Result<PathBuf, String> {
    #[cfg(not(unix))]
    {
        let _ = bytecode;
        let _ = source_path;
        return Err("--aot is currently supported on Unix-like systems only".to_string());
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
            .map_err(|e| format!("failed to create AOT timestamp: {}", e))?
            .as_nanos();
        let c_path =
            env::temp_dir().join(format!("forge-aot-{}-{}.c", std::process::id(), build_id));
        let c_source = aot_launcher_c_source(bytecode, &default_forge_bin);
        fs::write(&c_path, c_source)
            .map_err(|e| format!("failed to write AOT launcher source: {}", e))?;

        let status = Command::new("cc")
            .arg("-O2")
            .arg(&c_path)
            .arg("-o")
            .arg(&output_path)
            .status()
            .map_err(|e| format!("failed to invoke C compiler for --aot: {}", e))?;
        let _ = fs::remove_file(&c_path);

        if !status.success() {
            return Err(format!(
                "AOT launcher compilation failed for '{}'",
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

fn native_launcher_c_source(source: &[u8], default_forge_bin: &str) -> String {
    let byte_list = source
        .iter()
        .map(|byte| byte.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        r#"#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static const unsigned char FORGE_PROGRAM[] = {{ {byte_list} }};
static const size_t FORGE_PROGRAM_LEN = sizeof(FORGE_PROGRAM);
static const char *DEFAULT_FORGE_BIN = "{default_forge_bin}";

int main(int argc, char **argv) {{
    char tmp_template[] = "/tmp/forge-native-XXXXXX";
    int fd = mkstemp(tmp_template);
    if (fd == -1) {{
        perror("mkstemp");
        return 1;
    }}

    char program_path[sizeof(tmp_template) + 4];
    if (snprintf(program_path, sizeof(program_path), "%s.fg", tmp_template) < 0) {{
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
    if (fwrite(FORGE_PROGRAM, 1, FORGE_PROGRAM_LEN, program) != FORGE_PROGRAM_LEN) {{
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
        byte_list = byte_list,
        default_forge_bin = c_string_escape(default_forge_bin)
    )
}

fn aot_launcher_c_source(bytecode: &[u8], default_forge_bin: &str) -> String {
    let byte_list = bytecode
        .iter()
        .map(|byte| byte.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        r#"#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static const unsigned char FORGE_BYTECODE[] = {{ {byte_list} }};
static const size_t FORGE_BYTECODE_LEN = sizeof(FORGE_BYTECODE);
static const char *DEFAULT_FORGE_BIN = "{default_forge_bin}";

int main(int argc, char **argv) {{
    char tmp_template[] = "/tmp/forge-aot-XXXXXX";
    int fd = mkstemp(tmp_template);
    if (fd == -1) {{
        perror("mkstemp");
        return 1;
    }}

    /* Rename to .fgc so forge recognizes it as compiled bytecode */
    char program_path[sizeof(tmp_template) + 5];
    if (snprintf(program_path, sizeof(program_path), "%s.fgc", tmp_template) < 0) {{
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

    /* Write bytecode through the fd obtained from mkstemp (avoids TOCTOU) */
    FILE *program = fdopen(fd, "wb");
    if (!program) {{
        perror("fdopen");
        close(fd);
        unlink(program_path);
        return 1;
    }}
    if (fwrite(FORGE_BYTECODE, 1, FORGE_BYTECODE_LEN, program) != FORGE_BYTECODE_LEN) {{
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
        byte_list = byte_list,
        default_forge_bin = c_string_escape(default_forge_bin)
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
        // Must contain bytecode array, not source text
        assert!(c_source.contains("static const unsigned char FORGE_BYTECODE[]"));
        assert!(c_source.contains("/tmp/forge-bin"));
        // Must use .fgc extension for temp file
        assert!(c_source.contains(".fgc"));
        assert!(c_source.contains("forge-aot-"));
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

        // Fake bytecode — we only verify the binary is produced and executable
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
