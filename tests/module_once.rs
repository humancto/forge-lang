//! A module's top level runs at most once per program, also when spawned
//! tasks import it (`runtime::imports::ModuleRegistry`; VM:
//! `vm::module_share`). Every check runs on both engines.

use std::path::{Path, PathBuf};
use std::process::Command;

const FORGE: &str = env!("CARGO_BIN_EXE_forge");
const ENGINES: [&[&str]; 2] = [&[], &["--interp"]];

fn project(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "forge_module_once_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("mkdir");
    for (name, source) in files {
        std::fs::write(dir.join(name), source).expect("write module");
    }
    dir
}

#[track_caller]
fn expect_output(dir: &Path, expected: &str) {
    for engine in ENGINES {
        let out = Command::new(FORGE)
            .args(engine)
            .arg("run")
            .arg(dir.join("main.fg"))
            .current_dir(dir)
            .output()
            .expect("spawn forge");
        assert!(
            out.status.success(),
            "{:?}: {}",
            engine,
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            expected,
            "engine {:?}",
            engine
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

const COUNTER_MODULE: &str = "println(\"loading m\")\n\
    let value = 42\n\
    let mut hits = 0\n\
    struct Point { x: Int, y: Int }\n\
    impl Point {\n    fn sum(self) { return self.x + self.y }\n}\n\
    fn get() { return value }\n\
    fn bump() {\n    hits += 1\n    return hits\n}\n";

#[test]
fn a_module_first_imported_in_a_task_does_not_run_again_in_the_parent() {
    let dir = project(
        "task_then_parent",
        &[
            ("m.fg", COUNTER_MODULE),
            (
                "main.fg",
                "let t = spawn {\n\
                     import { get, Point } from \"m.fg\"\n\
                     let p = Point { x: 1, y: 2 }\n\
                     return get() + p.sum()\n\
                 }\n\
                 println(\"task {await t}\")\n\
                 import { get, bump, Point } from \"m.fg\"\n\
                 let p = Point { x: 10, y: 20 }\n\
                 println(get() + p.sum())\n\
                 println(bump())\n",
            ),
        ],
    );
    expect_output(&dir, "loading m\ntask 45\n72\n1\n");
}

#[test]
fn tasks_importing_a_module_at_once_run_it_once() {
    let dir = project(
        "concurrent",
        &[
            (
                "slow.fg",
                "println(\"loading slow\")\nwait(0.3)\nlet value = 21\nfn get() { return value }\n",
            ),
            (
                "main.fg",
                "let tasks = [\n\
                     spawn { import { get } from \"slow.fg\"\n return get() },\n\
                     spawn { import { get } from \"slow.fg\"\n return get() },\n\
                     spawn { import { get } from \"slow.fg\"\n return get() }\n\
                 ]\n\
                 let mut total = 0\n\
                 for t in tasks { total += await t }\n\
                 println(total)\n",
            ),
        ],
    );
    expect_output(&dir, "loading slow\n63\n");
}

#[test]
fn a_module_imported_from_another_task_brings_its_own_imports() {
    // `k.fg` imports `n.fg`; the parent gets `k` from the task's run, and
    // `k`'s functions still reach `n`'s globals.
    let dir = project(
        "deps",
        &[
            (
                "n.fg",
                "println(\"loading n\")\nlet factor = 2\n\
                 fn scale(x) { return x * factor }\n\
                 fn helper(x) { return scale(x) + 1000 }\n",
            ),
            (
                "k.fg",
                "println(\"loading k\")\nimport { helper } from \"n.fg\"\n\
                 fn twice(x) { return helper(x) }\n",
            ),
            (
                "main.fg",
                "let t = spawn {\n\
                     import { helper } from \"n.fg\"\n\
                     import { twice } from \"k.fg\"\n\
                     return twice(5) + helper(1)\n\
                 }\n\
                 println(\"task {await t}\")\n\
                 import { twice } from \"k.fg\"\n\
                 println(twice(7))\n\
                 import { helper } from \"n.fg\"\n\
                 println(helper(2))\n",
            ),
        ],
    );
    expect_output(&dir, "loading n\nloading k\ntask 2012\n1014\n1004\n");
}

#[test]
fn a_reexporting_module_run_in_a_task_exports_its_imports() {
    // `lib.fg` re-exports what it imports from `core.fg` (a function, a
    // struct, a type and an interface). A task runs both modules first; the
    // parent then imports the re-exports from the task's run.
    let dir = project(
        "reexport",
        &[
            (
                "core.fg",
                "println(\"loading core\")\n\
                 fn twice(x) { return x * 2 }\n\
                 struct Pt { x: Int }\n\
                 type Tok = Num(Int) | End\n\
                 interface Named {\n    fn name() -> String\n}\n",
            ),
            (
                "lib.fg",
                "println(\"loading lib\")\n\
                 import { twice, Pt, Tok, Num, Named } from \"core.fg\"\n\
                 fn quad(x) { return twice(twice(x)) }\n",
            ),
            (
                "main.fg",
                "let t = spawn {\n\
                     import { quad } from \"lib.fg\"\n\
                     return quad(1)\n\
                 }\n\
                 println(\"task {await t}\")\n\
                 import { twice, Pt, Tok, Num, Named } from \"lib.fg\"\n\
                 import \"lib.fg\" as lib\n\
                 println(twice(3))\n\
                 println(Pt { x: 4 }.x)\n\
                 println(Tok.name)\n\
                 println(Num(2))\n\
                 println(Named.name)\n\
                 println(lib.quad(2))\n",
            ),
        ],
    );
    expect_output(
        &dir,
        "loading lib\nloading core\ntask 4\n6\n4\nTok\n{ \"__type__\": \"Tok\", \"__variant__\": \"Num\", \"_0\": 2 }\nNamed\n8\n",
    );
}

#[test]
fn an_import_cycle_across_tasks_is_reported_not_a_deadlock() {
    // Task a runs ca.fg, which imports cb.fg; task b runs cb.fg, which
    // imports ca.fg. Each would wait for the other forever.
    let dir = project(
        "cycle",
        &[
            (
                "ca.fg",
                "wait(0.2)\nimport { fb } from \"cb.fg\"\nfn fa() { return 1 }\n",
            ),
            (
                "cb.fg",
                "wait(0.2)\nimport { fa } from \"ca.fg\"\nfn fb() { return 2 }\n",
            ),
            (
                "main.fg",
                "let a = spawn {\n    import { fa } from \"ca.fg\"\n    return fa()\n}\n\
                 let b = spawn {\n    import { fb } from \"cb.fg\"\n    return fb()\n}\n\
                 try { println(await a) } catch e { println(\"a: {e.message}\") }\n\
                 try { println(await b) } catch e { println(\"b: {e.message}\") }\n",
            ),
        ],
    );
    expect_output(
        &dir,
        "a: task error: circular import: ca.fg -> cb.fg -> ca.fg\n\
         b: task error: circular import: cb.fg -> ca.fg -> cb.fg\n",
    );
}
