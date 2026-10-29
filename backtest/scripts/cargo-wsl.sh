#!/bin/bash
# Lance cargo dans WSL Ubuntu-24.04 sur le paquet backtest/ (Rust n'existe que là).
#   depuis Git Bash : MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash "/mnt/c/dev/REA Projects/updown5m-lab/backtest/scripts/cargo-wsl.sh" test
set -e
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/bt-target}"
exec ~/.cargo/bin/cargo "$@"
