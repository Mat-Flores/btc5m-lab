#!/usr/bin/env bash
# Usage : deploy/deploy.sh <ssh-dest> <cle-ssh>      (depuis Git Bash ; ex. ubuntu@<ip> ~/.ssh/cle.pem)
# Envoie engine/, source-tracing/, ui/ (sans target/ trace/ node_modules/ .git), compile sur le serveur,
# cree config.json seulement s'il n'existe pas, redemarre updown5m@tracing et updown5m@ui.
set -euo pipefail

DEST="${1:-}"; KEY="${2:-}"
[ -n "$DEST" ] && [ -n "$KEY" ] || { echo "usage: $0 <ssh-dest> <cle-ssh>"; exit 2; }

# GARDE : JAMAIS de deploiement sur dublin-live (destination brute ET hote resolu par ~/.ssh/config).
RESOLVED=$(ssh -G "$DEST" 2>/dev/null | awk '$1=="hostname"{print $2}') || RESOLVED=""
for h in "$DEST" "$RESOLVED"; do
  case "${h,,}" in
    *3.253.60.148*|*dublin-live*) echo "REFUS : deploiement sur dublin-live interdit ($h)"; exit 3 ;;
  esac
done

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP=/opt/updown5m
SSH=(ssh -i "$KEY" -o BatchMode=yes -o IdentitiesOnly=yes)
RUN_AS="sudo -n -u tgap env HOME=/var/lib/updown5m/home"
DIRS=()
for d in engine source-tracing ui; do
  [ -d "$REPO/$d" ] || { echo "dossier absent : $d"; exit 1; }
  DIRS+=("$d")
done

echo "== envoi du code vers $DEST"
tar -C "$REPO" --exclude=target --exclude=node_modules --exclude=.git \
    --exclude=source-tracing/trace --exclude=source-tracing/config.json -cf - "${DIRS[@]}" |
  "${SSH[@]}" "$DEST" "$RUN_AS tar -xf - -C $APP"

echo "== config (creee seulement si absente)"
HOST_ID="${DEPLOY_HOST_ID:-dry-run}"
CFG=$(node -e '
  const c = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
  c.host_id = process.argv[2]; c.root = "/var/lib/updown5m/trace";
  process.stdout.write(JSON.stringify(c, null, 2) + "\n");' "$REPO/source-tracing/config.json" "$HOST_ID")
printf '%s' "$CFG" | "${SSH[@]}" "$DEST" \
  "$RUN_AS sh -c 'f=/var/lib/updown5m/config.json; if [ -e \$f ]; then echo config-existante-conservee; else cat > \$f && echo config-creee; fi'"

echo "== build (cargo build --release -j1)"
"${SSH[@]}" "$DEST" "$RUN_AS sh -c '
  set -e; export PATH=/var/lib/updown5m/home/.cargo/bin:\$PATH
  if [ -f $APP/source-tracing/Cargo.toml ]; then cd $APP/source-tracing && cargo build --release -j1; B=$APP/source-tracing/target/release/source-tracing
  else cd $APP/engine && cargo build --release -j1 -p source-tracing; B=$APP/engine/target/release/source-tracing; fi
  mkdir -p $APP/bin && install -m 755 \$B $APP/bin/source-tracing.new && mv -f $APP/bin/source-tracing.new $APP/bin/source-tracing'"

echo "== redemarrage"
"${SSH[@]}" "$DEST" "sudo -n systemctl enable updown5m@tracing updown5m@ui >/dev/null 2>&1; sudo -n systemctl restart updown5m@tracing updown5m@ui; sleep 2; systemctl --no-pager is-active updown5m@tracing updown5m@ui"
