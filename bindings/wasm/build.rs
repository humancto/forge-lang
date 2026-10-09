// The WebAssembly shadow stack (Rust's call stack in linear memory) defaults
// to 1 MiB, which deep Forge recursion exhausts long before the engines'
// depth limit. Reserve `STACK_SIZE` bytes instead; `src/lib.rs` registers
// the same size with Forge's native-stack guard so overflow becomes a
// catchable "maximum recursion depth" error rather than memory corruption.
//
// Keep in sync with `STACK_SIZE` in src/lib.rs.
const STACK_SIZE: usize = 8 * 1024 * 1024;

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.starts_with("wasm32") {
        println!("cargo:rustc-link-arg=-zstack-size={}", STACK_SIZE);
    }
    println!("cargo:rerun-if-changed=build.rs");
}
