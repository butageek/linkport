#!/usr/bin/env bash
# Full release build of the Windows binary: frontend + Rust cross-compile.
#
# WSL prerequisites:
#   rustup target add x86_64-pc-windows-gnu
#   sudo apt install mingw-w64
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> Building frontend"
( cd frontend
  [ -f package-lock.json ] && npm ci || npm install
  npm run build
)

echo "==> Building linkport.exe (x86_64-pc-windows-gnu)"
cargo build --release --target x86_64-pc-windows-gnu

echo
echo "Done: target/x86_64-pc-windows-gnu/release/linkport.exe"
