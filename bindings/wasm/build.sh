#!/usr/bin/env bash
# Build the Forge WebAssembly module and its JS glue into docs/playground/pkg.
#
#   bindings/wasm/build.sh            # release build (what the playground serves)
#   bindings/wasm/build.sh --dev      # faster, unoptimised build
#
# Needs: rustup target add wasm32-unknown-unknown
#        cargo install wasm-bindgen-cli --version <the version pinned in Cargo.toml>
# Optional: FORGE_WASM_OPT=1 runs wasm-opt (binaryen) on the result.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
out="$repo/docs/playground/pkg"
profile=release
cargo_flags=(--release)
if [[ "${1:-}" == "--dev" ]]; then
  profile=debug
  cargo_flags=()
fi

pinned="$(sed -n 's/^wasm-bindgen = "=\(.*\)"/\1/p' "$here/Cargo.toml")"
installed="$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)"
if [[ "$installed" != "$pinned" ]]; then
  echo "error: wasm-bindgen CLI ${installed:-not found}, need $pinned:" >&2
  echo "  cargo install wasm-bindgen-cli --version $pinned --locked" >&2
  exit 1
fi

target_dir="${CARGO_TARGET_DIR:-$here/target}"
(cd "$here" && cargo build "${cargo_flags[@]}" --target wasm32-unknown-unknown)

rm -rf "$out"
wasm-bindgen "$target_dir/wasm32-unknown-unknown/$profile/forge_wasm.wasm" \
  --target web --no-typescript --out-dir "$out"

wasm="$out/forge_wasm_bg.wasm"
# Opt-in post-link optimisation with binaryen (FORGE_WASM_OPT=1). Off by
# default: rustc's wasm32 output uses newer proposals (reference types,
# multi-value) that older wasm-opt releases reject.
if [[ "$profile" == release && "${FORGE_WASM_OPT:-0}" == 1 ]]; then
  wasm-opt -O3 --all-features -o "$wasm.opt" "$wasm"
  mv "$wasm.opt" "$wasm"
fi

bytes=$(wc -c <"$wasm")
gz=$(gzip -9 -c "$wasm" | wc -c)
echo "built $wasm: $((bytes / 1024)) KiB ($((gz / 1024)) KiB gzipped)"
