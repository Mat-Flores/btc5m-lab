#!/bin/bash
# Tests de l'engine (workspace) puis du backtest, dans WSL Ubuntu-24.04.
#   depuis Git Bash : MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash "/mnt/c/dev/REA Projects/updown5m-lab/backtest/scripts/test-wsl.sh" [engine|backtest]
set -e
LAB="$(cd "$(dirname "$0")/../.." && pwd)"
if [ "${1:-all}" != backtest ]; then
  cd "$LAB/engine" && CARGO_TARGET_DIR="$HOME/eng-target" ~/.cargo/bin/cargo test --workspace 2>&1 | grep -E "^test result|FAILED|panicked|error(\[|:)|warning: unused" || true
  CARGO_TARGET_DIR="$HOME/eng-target" ~/.cargo/bin/cargo test --workspace --quiet >/dev/null 2>&1 && echo "engine: cargo test --workspace OK" || { echo "engine: cargo test --workspace FAILED"; exit 1; }
fi
if [ "${1:-all}" != engine ]; then
  cd "$LAB/backtest" && CARGO_TARGET_DIR="$HOME/bt-target" ~/.cargo/bin/cargo test --release 2>&1 | grep -E "^test result|FAILED|panicked|error(\[|:)|warning" || true
  CARGO_TARGET_DIR="$HOME/bt-target" ~/.cargo/bin/cargo test --release --quiet >/dev/null 2>&1 && echo "backtest: cargo test OK" || { echo "backtest: cargo test FAILED"; exit 1; }
fi
