#!/usr/bin/env bash
# A executer DANS WSL (Ubuntu 24.04) : compile copy-trading en release pour le serveur (arm64, aarch64-unknown-linux-gnu)
# et le copie dans copy-trading/target/wsl-bin/ (dossier ignore par git). glibc 2.39 : tourne sur Ubuntu 24.04+.
# Prerequis (une fois) : rustup target add aarch64-unknown-linux-gnu ; sudo apt-get install gcc-aarch64-linux-gnu
# Usage : build-wsl.sh <chemin WSL de copy-trading>
set -euo pipefail
SRC="${1:?chemin copy-trading}"
export CARGO_TARGET_DIR="$HOME/ct-target"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
cd "$SRC"
"$HOME/.cargo/bin/cargo" build --release --locked --target aarch64-unknown-linux-gnu 2>&1
mkdir -p "$SRC/target/wsl-bin"
cp -f "$CARGO_TARGET_DIR/aarch64-unknown-linux-gnu/release/copy-trading" "$SRC/target/wsl-bin/copy-trading.new"
mv -f "$SRC/target/wsl-bin/copy-trading.new" "$SRC/target/wsl-bin/copy-trading"
echo "BINAIRE-OK $(sha256sum "$SRC/target/wsl-bin/copy-trading" | cut -d' ' -f1)"
