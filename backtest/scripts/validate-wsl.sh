#!/bin/bash
# Non-régression maker, run complet : `backtest validate` (surcharge config/nonreg.json : comportement validé, cancel_ahead_factor 1) (équivalent Rust de
# `node engine/maker-bt/validate.mjs --delay 0 --sim exact`) sur les packs et données du dépôt source (lecture seule),
# puis comparaison bit à bit, jumeau par jumeau, avec l'export de l'oracle Node (oracle/validate-export.mjs).
#   depuis Git Bash : MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash "/mnt/c/dev/REA Projects/updown5m-lab/backtest/scripts/validate-wsl.sh"
# Oracle (≈ 20 min, Node 24 sous Windows, depuis la racine du dépôt) :
#   node --max-old-space-size=8192 backtest/oracle/validate-export.mjs "C:/dev/REA Projects/btc5m-lag-sim" backtest/out/validate-oracle.jsonl > backtest/out/validate-oracle.txt
set -e
BT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$BT"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/bt-target}"
~/.cargo/bin/cargo build --release --quiet
mkdir -p out
"$CARGO_TARGET_DIR/release/backtest" validate --config config/default.json --override config/packs.json --override config/nonreg.json --export out/validate-rust.jsonl "$@" | tee out/validate-rust.txt
if [ -f out/validate-oracle.jsonl ] && [ $# -eq 0 ]; then
  "$CARGO_TARGET_DIR/release/backtest" compare out/validate-oracle.jsonl out/validate-rust.jsonl
  diff <(grep -v '^Wed\|^real\|^user\|^sys\|^exit\|^$' out/validate-oracle.txt | grep -v '^(') <(grep -v '^(' out/validate-rust.txt) && echo "résumé : identique à l'oracle Node"
fi
