#!/usr/bin/env bash
# A executer DANS WSL (Ubuntu 24.04), lance par deploy/start.
#   build-wsl.sh <chemin WSL de copy-trading>                  compile pour le serveur (arm64) -> target/wsl-bin/copy-trading
#   build-wsl.sh <chemin WSL de copy-trading> encrypt <adresse> chiffre localement (binaire x86 de WSL) : cle + phrase sur
#                                                               stdin, ligne "<adresse>:v1..." sur stdout ; rien sur disque
# Prerequis (une fois) : rustup target add aarch64-unknown-linux-gnu ; sudo apt-get install gcc-aarch64-linux-gnu
set -euo pipefail
SRC="${1:?chemin copy-trading}"
export CARGO_TARGET_DIR="$HOME/ct-target"
cd "$SRC"
if [ "${2:-}" = encrypt ]; then
  "$HOME/.cargo/bin/cargo" build --release --locked -q >&2
  exec "$CARGO_TARGET_DIR/release/copy-trading" encrypt "${3:?adresse}"
fi
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
"$HOME/.cargo/bin/cargo" build --release --locked --target aarch64-unknown-linux-gnu 2>&1
mkdir -p "$SRC/target/wsl-bin"
cp -f "$CARGO_TARGET_DIR/aarch64-unknown-linux-gnu/release/copy-trading" "$SRC/target/wsl-bin/copy-trading.new"
mv -f "$SRC/target/wsl-bin/copy-trading.new" "$SRC/target/wsl-bin/copy-trading"
echo "BINAIRE-OK $(sha256sum "$SRC/target/wsl-bin/copy-trading" | cut -d' ' -f1)"
