use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const FIXTURE_SOURCE: &str = r#"println("ok")"#;
const CHILD_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct Config {
    forge: PathBuf,
    reps: usize,
    warmups: usize,
}

#[derive(Debug)]
struct Mode {
    name: &'static str,
    command: PathBuf,
    args: Vec<String>,
    envs: Vec<(String, String)>,
}

#[derive(Debug)]
struct Stats {
    min: Duration,
    median: Duration,
    p95: Duration,
    max: Duration,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("startup_time: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let config = parse_args()?;
    let lib_dir = env::var("FORGE_LIB_DIR").map(PathBuf::from).map_err(|_| {
        "FORGE_LIB_DIR must point at a directory containing libforge_lang.a".to_string()
    })?;
    let lib_dir = fs::canonicalize(&lib_dir).map_err(|err| {
        format!(
            "failed to canonicalize FORGE_LIB_DIR {}: {err}",
            lib_dir.display()
        )
    })?;
    let lib_path = lib_dir.join("libforge_lang.a");
    if !lib_path.exists() {
        return Err(format!("{} does not exist", lib_path.display()));
    }

    let workdir = unique_workdir()?;
    let result = run_in_workdir(&config, &lib_dir, &workdir);
    let _ = fs::remove_dir_all(&workdir);
    result
}

fn parse_args() -> Result<Config, String> {
    let mut forge = None;
    let mut reps = 20usize;
    let mut warmups = 3usize;
    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--forge" => {
                i += 1;
                forge = args.get(i).map(PathBuf::from);
            }
            "--reps" => {
                i += 1;
                reps = args
                    .get(i)
                    .ok_or("--reps requires a value")?
                    .parse()
                    .map_err(|_| "--reps must be a positive integer".to_string())?;
            }
            "--warmups" => {
                i += 1;
                warmups = args
                    .get(i)
                    .ok_or("--warmups requires a value")?
                    .parse()
                    .map_err(|_| "--warmups must be a non-negative integer".to_string())?;
            }
            "--help" | "-h" => {
                println!(
                    "Usage: startup_time --forge <path> [--reps N] [--warmups N]\n\
                     Requires FORGE_LIB_DIR to contain libforge_lang.a."
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
        i += 1;
    }

    let forge = forge.ok_or("--forge <path> is required")?;
    let forge = fs::canonicalize(&forge).map_err(|err| {
        format!(
            "failed to canonicalize forge binary {}: {err}",
            forge.display()
        )
    })?;
    if reps == 0 {
        return Err("--reps must be greater than zero".to_string());
    }

    Ok(Config {
        forge,
        reps,
        warmups,
    })
}

fn unique_workdir() -> Result<PathBuf, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("system clock before unix epoch: {err}"))?
        .as_nanos();
    let dir = env::temp_dir().join(format!("forge-startup-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    Ok(dir)
}

fn run_in_workdir(config: &Config, lib_dir: &Path, workdir: &Path) -> Result<(), String> {
    let source_run = write_fixture(workdir, "source_run.fg")?;
    let bytecode_run = write_fixture(workdir, "bytecode_run.fg")?;
    let native_source = write_fixture(workdir, "native_source.fg")?;
    let aot_bytecode = write_fixture(workdir, "aot_bytecode.fg")?;

    checked_command(
        Command::new(&config.forge)
            .arg("build")
            .arg(&bytecode_run)
            .current_dir(workdir),
        "build bytecode fixture",
    )?;
    checked_command(
        Command::new(&config.forge)
            .arg("build")
            .arg("--native")
            .arg(&native_source)
            .env("FORGE_LIB_DIR", lib_dir)
            .current_dir(workdir),
        "build native source-runtime fixture",
    )?;
    checked_command(
        Command::new(&config.forge)
            .arg("build")
            .arg("--aot")
            .arg(&aot_bytecode)
            .env("FORGE_LIB_DIR", lib_dir)
            .current_dir(workdir),
        "build native bytecode fixture",
    )?;

    let modes = vec![
        Mode {
            name: "source_run",
            command: config.forge.clone(),
            args: vec!["run".to_string(), source_run.display().to_string()],
            envs: vec![],
        },
        Mode {
            name: "bytecode_run",
            command: config.forge.clone(),
            args: vec![
                "run".to_string(),
                bytecode_run.with_extension("fgc").display().to_string(),
            ],
            envs: vec![],
        },
        Mode {
            name: "native_source_runtime",
            command: native_source.with_extension(""),
            args: vec![],
            envs: vec![],
        },
        Mode {
            name: "aot_bytecode",
            command: aot_bytecode.with_extension(""),
            args: vec![],
            envs: vec![],
        },
    ];

    println!(
        "startup_time reps={} warmups={} forge={}",
        config.reps,
        config.warmups,
        config.forge.display()
    );
    for mode in modes {
        let stats = measure_mode(&mode, config.reps, config.warmups)?;
        println!(
            "startup.{name} min_ms={:.3} median_ms={:.3} p95_ms={:.3} max_ms={:.3}",
            millis(stats.min),
            millis(stats.median),
            millis(stats.p95),
            millis(stats.max),
            name = mode.name
        );
    }

    Ok(())
}

fn write_fixture(workdir: &Path, name: &str) -> Result<PathBuf, String> {
    let path = workdir.join(name);
    fs::write(&path, FIXTURE_SOURCE)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(path)
}

fn checked_command(command: &mut Command, label: &str) -> Result<Output, String> {
    let output = command
        .output()
        .map_err(|err| format!("{label}: failed to spawn: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "{label}: failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output)
}

fn measure_mode(mode: &Mode, reps: usize, warmups: usize) -> Result<Stats, String> {
    for _ in 0..warmups {
        run_child(mode)?;
    }

    let mut times = Vec::with_capacity(reps);
    for _ in 0..reps {
        let started = Instant::now();
        run_child(mode)?;
        times.push(started.elapsed());
    }
    times.sort_unstable();

    let min = times[0];
    let median = times[times.len() / 2];
    let p95_idx = ((times.len() * 95).div_ceil(100)).saturating_sub(1);
    let p95 = times[p95_idx.min(times.len() - 1)];
    let max = times[times.len() - 1];

    Ok(Stats {
        min,
        median,
        p95,
        max,
    })
}

fn run_child(mode: &Mode) -> Result<(), String> {
    let mut command = Command::new(&mode.command);
    command.args(&mode.args);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    for (key, value) in &mode.envs {
        command.env(key, value);
    }
    let mut child = command
        .spawn()
        .map_err(|err| format!("{}: failed to spawn: {err}", mode.name))?;
    let deadline = Instant::now() + CHILD_TIMEOUT;
    loop {
        if child
            .try_wait()
            .map_err(|err| format!("{}: failed to poll child: {err}", mode.name))?
            .is_some()
        {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "{}: child timed out after {:.1}s",
                mode.name,
                CHILD_TIMEOUT.as_secs_f64()
            ));
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    let output = child
        .wait_with_output()
        .map_err(|err| format!("{}: failed to collect child output: {err}", mode.name))?;
    if !output.status.success() {
        return Err(format!(
            "{}: failed with status {}\nstdout:\n{}\nstderr:\n{}",
            mode.name,
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim() != "ok" {
        return Err(format!(
            "{}: expected stdout 'ok', got {:?}\nstderr:\n{}",
            mode.name,
            stdout,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
