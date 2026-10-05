//! Stable-toolchain smoke run of the cargo-fuzz targets in `fuzz/`.
//!
//! cargo-fuzz needs nightly and runs for minutes; this harness runs the
//! *same* target bodies (`fuzz/src/harness.rs`, included below) on stable
//! as an ordinary test, so every `cargo test` exercises them:
//!
//! * every committed crasher in `fuzz/regressions/<target>/` is replayed;
//! * the `.fg` programs in the repository seed a small mutational fuzzer
//!   (byte edits, token insertions, splices) for the `parse`, `compile`
//!   and `bytecode` targets;
//! * the grammar-based generator produces random programs for the
//!   `differential` (interpreter vs VM) target.
//!
//! Inputs come from a fixed-seed PRNG, so failures reproduce. Knobs:
//!
//! * `FORGE_FUZZ_ITERS=<n>` — mutations per target (default 2000; the
//!   differential target runs a quarter as many programs).
//! * `FORGE_FUZZ_SEED=<n>` — PRNG seed (default fixed).
//!
//! A failure prints the offending input; minimize it and commit it under
//! `fuzz/regressions/<target>/` together with the fix.

#[path = "../fuzz/src/gen.rs"]
mod gen;
#[path = "../fuzz/src/harness.rs"]
mod harness;

use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn iterations() -> u64 {
    env_u64("FORGE_FUZZ_ITERS", 2000)
}

/// splitmix64: tiny, fast, good enough to drive mutations.
struct Rng(u64);

impl Rng {
    fn new(salt: u64) -> Self {
        Rng(env_u64("FORGE_FUZZ_SEED", 0x5EED_F025) ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }
}

fn fg_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            fg_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "fg") {
            out.push(p);
        }
    }
}

/// Source seeds: the repository's own Forge programs.
fn seeds() -> Vec<Vec<u8>> {
    let mut files = Vec::new();
    for dir in ["examples", "tests"] {
        fg_files(&repo().join(dir), &mut files);
    }
    let seeds: Vec<Vec<u8>> = files.iter().filter_map(|p| fs::read(p).ok()).collect();
    assert!(
        seeds.len() > 20,
        "expected .fg seed programs in examples/ and tests/"
    );
    seeds
}

const TOKENS: &[&str] = &[
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    "\"",
    "\"{",
    "}\"",
    "\"\"\"",
    ",",
    ":",
    ";",
    ".",
    "..",
    "...",
    "=",
    "==",
    "=>",
    "->",
    "+",
    "-",
    "*",
    "/",
    "%",
    "<",
    ">",
    "!",
    "&&",
    "||",
    "?",
    "@",
    "\n",
    " ",
    "0",
    "1",
    "9223372036854775807",
    "1e309",
    "0.5",
    "-",
    "_",
    "x",
    "fn ",
    "let ",
    "mut ",
    "if ",
    "else ",
    "for ",
    "in ",
    "while ",
    "loop ",
    "match ",
    "return ",
    "break",
    "continue",
    "try ",
    "catch e ",
    "spawn ",
    "when ",
    "set ",
    "to ",
    "change ",
    "define ",
    "unpack ",
    "from ",
    "type ",
    "struct ",
    "impl ",
    "interface ",
    "import ",
    "timeout ",
    "seconds ",
    "retry ",
    "times ",
    "repeat ",
    "safe ",
    "must ",
    "check ",
    "is ",
    "not ",
    "empty",
    "async ",
    "await ",
    "yield ",
    "squad ",
    "say ",
    "null",
    "true",
    "Ok(",
    "Err(",
    "Some(",
    "None",
    "\\",
    "\\u{",
    "//",
    "/*",
    "*/",
    "#",
    "$",
    "`",
    "\t",
    "\r",
];

/// Apply a few random edits to `input`.
fn mutate(rng: &mut Rng, input: &[u8], seeds: &[Vec<u8>]) -> Vec<u8> {
    let mut out = input.to_vec();
    for _ in 0..1 + rng.below(4) {
        let at = rng.below(out.len() + 1);
        match rng.below(6) {
            0 => {
                let tok = TOKENS[rng.below(TOKENS.len())].as_bytes();
                out.splice(at..at, tok.iter().copied());
            }
            1 if !out.is_empty() => {
                let end = (at + 1 + rng.below(16)).min(out.len());
                out.drain(at.min(end)..end);
            }
            2 if !out.is_empty() => {
                let start = rng.below(out.len());
                let end = (start + 1 + rng.below(32)).min(out.len());
                let piece = out[start..end].to_vec();
                out.splice(at..at, piece);
            }
            3 => {
                let other = &seeds[rng.below(seeds.len())];
                if !other.is_empty() {
                    let start = rng.below(other.len());
                    let end = (start + 1 + rng.below(64)).min(other.len());
                    out.splice(at..at, other[start..end].iter().copied());
                }
            }
            4 if !out.is_empty() => {
                let i = rng.below(out.len());
                out[i] = rng.next() as u8;
            }
            _ => {
                // Repeat a nesting token many times (deep-nesting inputs).
                let tok = ["(", "[", "{", "-", "!", "fn() { ", "[1, "][rng.below(7)];
                let n = 1 + rng.below(300);
                out.splice(at..at, tok.repeat(n).into_bytes());
            }
        }
    }
    out
}

fn printable(data: &[u8]) -> String {
    let s = String::from_utf8_lossy(data);
    if s.chars().count() > 4000 {
        let head: String = s.chars().take(4000).collect();
        format!("{}… ({} bytes)", head, data.len())
    } else {
        s.into_owned()
    }
}

/// Run `check` on `input`, turning a panic into a test failure that shows
/// the input.
fn run_case(target: &str, label: &str, input: &[u8], check: fn(&[u8])) {
    if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(|| check(input))) {
        let msg = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default();
        panic!(
            "fuzz target `{target}` failed on {label}: {msg}\n--- input ({} bytes) ---\n{}\n--- bytes ---\n{:?}",
            input.len(),
            printable(input),
            input
        );
    }
}

fn replay_regressions(target: &str, check: fn(&[u8])) -> usize {
    let dir = repo().join("fuzz/regressions").join(target);
    let Ok(entries) = fs::read_dir(&dir) else {
        return 0;
    };
    let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    files.sort();
    let mut n = 0;
    for f in files.iter().filter(|f| f.is_file()) {
        if f.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        let data = fs::read(f).expect("read regression input");
        run_case(target, &f.display().to_string(), &data, check);
        n += 1;
    }
    n
}

fn mutational(target: &str, salt: u64, seeds: &[Vec<u8>], check: fn(&[u8])) {
    let replayed = replay_regressions(target, check);
    let mut rng = Rng::new(salt);
    for (i, seed) in seeds.iter().enumerate() {
        run_case(target, &format!("seed #{i}"), seed, check);
    }
    for i in 0..iterations() {
        let base = &seeds[rng.below(seeds.len())];
        let input = mutate(&mut rng, base, seeds);
        run_case(target, &format!("mutation #{i}"), &input, check);
    }
    eprintln!(
        "fuzz_smoke {target}: {replayed} regressions, {} seeds, {} mutations",
        seeds.len(),
        iterations()
    );
}

#[test]
fn fuzz_parse() {
    mutational("parse", 1, &seeds(), harness::check_parse);
}

#[test]
fn fuzz_compile() {
    mutational("compile", 2, &seeds(), harness::check_compile);
}

/// Serialized bytecode of the small parity fixtures: well-formed inputs
/// that the byte mutations turn into near-miss bytecode.
fn bytecode_seeds() -> Vec<Vec<u8>> {
    let mut files = Vec::new();
    fg_files(&repo().join("tests/parity/supported"), &mut files);
    let mut out = Vec::new();
    for f in files {
        let Ok(src) = fs::read_to_string(&f) else {
            continue;
        };
        let Some(program) = harness::parse(&src) else {
            continue;
        };
        if let Ok(chunk) = forge_lang::vm::compiler::compile(&program) {
            out.push(forge_lang::vm::serialize::serialize_chunk(&chunk).expect("serialize"));
        }
    }
    assert!(
        out.len() > 5,
        "expected compiled parity fixtures as bytecode seeds"
    );
    out
}

#[test]
fn fuzz_bytecode() {
    let seeds = bytecode_seeds();
    let replayed = replay_regressions("bytecode", harness::check_bytecode);
    let mut rng = Rng::new(3);
    for i in 0..iterations() {
        let base = &seeds[rng.below(seeds.len())];
        let mut input = base.clone();
        // Mostly small corruptions of valid bytecode, sometimes raw noise.
        match rng.below(8) {
            0 => {
                let len = rng.below(256);
                input = rng.bytes(len);
            }
            1 => input.truncate(rng.below(input.len() + 1)),
            _ => {
                for _ in 0..1 + rng.below(4) {
                    if input.is_empty() {
                        break;
                    }
                    let at = rng.below(input.len());
                    input[at] = match rng.below(4) {
                        0 => 0,
                        1 => 0xFF,
                        2 => input[at].wrapping_add(1),
                        _ => rng.next() as u8,
                    };
                }
            }
        }
        run_case(
            "bytecode",
            &format!("mutation #{i}"),
            &input,
            harness::check_bytecode,
        );
    }
    eprintln!(
        "fuzz_smoke bytecode: {replayed} regressions, {} mutations",
        iterations()
    );
}

#[test]
fn fuzz_differential() {
    let replayed = replay_regressions("differential", harness::check_differential);
    let mut rng = Rng::new(4);
    let n = (iterations() / 4).max(1);
    for i in 0..n {
        let len = 16 + rng.below(1024);
        let data = rng.bytes(len);
        if let Err(payload) =
            panic::catch_unwind(AssertUnwindSafe(|| harness::check_differential(&data)))
        {
            // Exploration mode: collect every failure instead of stopping.
            if let Ok(dir) = std::env::var("FORGE_FUZZ_FAILURES_DIR") {
                let _ = fs::create_dir_all(&dir);
                let _ = fs::write(Path::new(&dir).join(format!("diff-{i}.bin")), &data);
                continue;
            }
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .unwrap_or_default();
            panic!("differential program #{i} failed:\n{msg}");
        }
    }
    eprintln!("fuzz_smoke differential: {replayed} regressions, {n} programs");
}

#[test]
fn generator_produces_parseable_programs() {
    let mut rng = Rng::new(5);
    for _ in 0..200 {
        let data = rng.bytes(512);
        let mut u = arbitrary::Unstructured::new(&data);
        let src = gen::program(&mut u).expect("generation never fails");
        if let Err(e) = harness::parse_result(&src) {
            panic!("generated program does not parse: {e}\n{src}");
        }
    }
}

/// Triage helper, a no-op unless `FORGE_FUZZ_TRIAGE_DIR` is set: for every
/// `.bin` input in that directory (collected with `FORGE_FUZZ_FAILURES_DIR`),
/// write the generated program next to it as `<name>.fg` and print both
/// engines' outcomes.
#[test]
fn triage_differential_failures() {
    let Ok(dir) = std::env::var("FORGE_FUZZ_TRIAGE_DIR") else {
        return;
    };
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("triage dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "bin"))
        .collect();
    files.sort();
    for f in files {
        let data = fs::read(&f).expect("read");
        let mut u = arbitrary::Unstructured::new(&data);
        let Ok(src) = gen::program(&mut u) else {
            continue;
        };
        fs::write(f.with_extension("fg"), &src).expect("write");
        let program = harness::parse_result(&src).expect("parse");
        let i = harness::run_interpreter(&program);
        let v = harness::run_vm(&program);
        let short = |o: &harness::Outcome| {
            let s = format!("{o:?}");
            s.chars().take(160).collect::<String>()
        };
        if let (harness::Outcome::Value(a), harness::Outcome::Value(b)) = (&i, &v) {
            // Show both values around the first difference.
            let at = a
                .chars()
                .zip(b.chars())
                .take_while(|(x, y)| x == y)
                .count()
                .saturating_sub(30);
            let window = |s: &str| s.chars().skip(at).take(90).collect::<String>();
            println!(
                "{}\n  interp: …{}\n  vm:     …{}",
                f.display(),
                window(a),
                window(b)
            );
            continue;
        }
        println!(
            "{}\n  interp: {}\n  vm:     {}",
            f.display(),
            short(&i),
            short(&v)
        );
    }
}
