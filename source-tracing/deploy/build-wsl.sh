#!/usr/bin/env bash
# A executer DANS WSL (Ubuntu 24.04) par deploy/start : compile le tracer en release et le copie dans
# source-tracing/target/wsl-bin/ (dossier ignore par git). glibc 2.39 : le binaire tourne sur un Ubuntu plus recent.
# Usage : build-wsl.sh <chemin WSL de source-tracing>
set -euo pipefail
SRC="${1:?chemin source-tracing}"
export CARGO_TARGET_DIR="$HOME/st-target"
cd "$SRC"
"$HOME/.cargo/bin/cargo" build --release --locked -p source-tracing 2>&1
mkdir -p "$SRC/target/wsl-bin"
cp -f "$CARGO_TARGET_DIR/release/source-tracing" "$SRC/target/wsl-bin/source-tracing.new"
mv -f "$SRC/target/wsl-bin/source-tracing.new" "$SRC/target/wsl-bin/source-tracing"
echo "BINAIRE-OK $(sha256sum "$SRC/target/wsl-bin/source-tracing" | cut -d' ' -f1)"
