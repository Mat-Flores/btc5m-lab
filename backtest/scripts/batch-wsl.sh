#!/bin/bash
# Non-régression maker sur un lot de l'ancien format (x01-retest-exact, x04-mix-frozen…) : `backtest run` sur les
# packs du dépôt source (lecture seule), borne exact, retenue 0, fenêtres avant CONFIRM, puis comparaison bit à bit,
# (candidat, fenêtre) par (candidat, fenêtre), avec l'export de l'oracle Node (oracle/batch-export.mjs).
#   depuis Git Bash : MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash "/mnt/c/dev/REA Projects/updown5m-lab/backtest/scripts/batch-wsl.sh" x01-retest-exact
# Oracle (≈ 6-7 min par lot, Node 24 sous Windows, depuis la racine du dépôt) :
#   node --max-old-space-size=6144 backtest/oracle/batch-export.mjs "C:/dev/REA Projects/btc5m-lag-sim" "C:/dev/REA Projects/btc5m-lag-sim/engine/maker-bt/batches/<lot>.json" exact 0 backtest/out/<lot>-oracle.jsonl
set -e
B="${1:?lot, ex. x01-retest-exact}"
BT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$BT"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/bt-target}"
~/.cargo/bin/cargo build --release --quiet
mkdir -p out
"$CARGO_TARGET_DIR/release/backtest" run --config config/default.json --override config/packs.json \
  --batch "/mnt/c/dev/REA Projects/btc5m-lag-sim/engine/maker-bt/batches/$B.json" --export "out/$B-rust.jsonl" | tee "out/$B-rust.txt"
[ -f "out/$B-oracle.jsonl" ] && "$CARGO_TARGET_DIR/release/backtest" compare "out/$B-oracle.jsonl" "out/$B-rust.jsonl"
