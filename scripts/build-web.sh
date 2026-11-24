#!/usr/bin/env bash
# Builds patchplay as a static site in ui/dist-web: the engine and session as WebAssembly
# (crates/kerf-web), then the UI with VITE_TARGET=web.
#
#   scripts/build-web.sh                    # served from /
#   BASE=/patchplay/ scripts/build-web.sh   # served from a sub-path (GitHub Pages project site)
#
# One-off setup:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.130   # must match the wasm-bindgen crate pinned in crates/kerf-web
#   brew install binaryen                              # optional: wasm-opt shrinks the module further
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

pkg="ui/src/web/pkg"
wasm="target/wasm32-unknown-unknown/web/kerf_web.wasm"

# `--profile web` is `release` tuned for size (opt-level s, LTO, panic=abort); see Cargo.toml.
cargo build --profile web --target wasm32-unknown-unknown -p kerf-web

rm -rf "$pkg"
wasm-bindgen --target web --no-typescript --remove-name-section --remove-producers-section \
  --out-dir "$pkg" --out-name kerf_web "$wasm"

if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals \
    "$pkg/kerf_web_bg.wasm" -o "$pkg/kerf_web_bg.wasm.opt"
  mv "$pkg/kerf_web_bg.wasm.opt" "$pkg/kerf_web_bg.wasm"
else
  echo "wasm-opt not found: skipping (brew install binaryen, or apt install binaryen)" >&2
fi

cd ui
[ -d node_modules ] || npm ci
VITE_TARGET=web BASE="${BASE:-/}" npm run build:web

size="$(wc -c <dist-web/assets/*.wasm | tr -d ' ')"
gz="$(gzip -9 -c dist-web/assets/*.wasm | wc -c | tr -d ' ')"
echo "ui/dist-web ready. wasm: ${size} bytes, ${gz} gzip."
