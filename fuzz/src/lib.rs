//! Shared code for the Forge fuzz targets (see `fuzz/README.md`).
//!
//! `tests/fuzz_smoke.rs` in the main crate includes these same modules with
//! `#[path]`, so the stable smoke harness and cargo-fuzz exercise identical
//! target bodies.

pub mod gen;
pub mod harness;
