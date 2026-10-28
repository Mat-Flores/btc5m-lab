#!/usr/bin/env bash
# Tests locaux de start / stop / pull : AUCUN ssh, aucun serveur (PT_TEST_NOSSH=1).
# Le "serveur" est un dossier temporaire ; systemctl est un faux (etat dans un fichier) qui lance un faux tracer
# (boucle shell qui relit la config a chaud, ecrit les fichiers du jour et status.json). Ce n'est PAS le vrai tracer :
# le comportement reel (relecture a chaud <= 1 s) est celui lu dans src/run.rs, pas teste ici.
# Usage : bash tests/test-deploy.sh
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; D="$(dirname "$HERE")"
T=$(mktemp -d); trap 'kill $(cat "$T/srv/pid" 2>/dev/null) 2>/dev/null; rm -rf "$T"' EXIT
export PT_TEST_NOSSH=1 ST_TEST_DIR="$T/srv" NO_COLOR=1
mkdir -p "$ST_TEST_DIR/trace" "$ST_TEST_DIR/opt/bin" "$T/bin"
head -c 2500000 /dev/urandom > "$T/fakebin"; export ST_TEST_BIN="$T/fakebin"   # simule la compilation WSL
PASS=0; FAIL=0
t() { if "$@"; then PASS=$((PASS+1)); echo "  PASS  $DESC"; else FAIL=$((FAIL+1)); echo "  FAIL  $DESC"; fi; }
d() { DESC="$1"; shift; t "$@"; }

# ---- faux tracer + faux systemctl ----------------------------------------------------------------------
cat > "$T/bin/faketracer" <<'EOF'
#!/bin/sh
S=$1; C=$S/config.json; R=$S/trace
while :; do
  day=$(date -u +%F); y=${day%%-*}; m=$(echo $day | cut -d- -f2); st=""
  for n in $(sed -nE 's/.*"name" *: *"([A-Za-z0-9_-]+)".*"enabled" *: *true.*/\1/p' "$C"); do
    mkdir -p "$R/$n/$y/$m"; head -c 700 /dev/zero | tr '\0' 'x' >> "$R/$n/$y/$m/$day.jsonl.gz.part-1"
  done
  for l in $(sed -nE 's/.*"name" *: *"([A-Za-z0-9_-]+)".*"enabled" *: *(true|false).*/\1:\2/p' "$C"); do
    st="$st{\"name\":\"${l%%:*}\",\"enabled\":${l##*:},\"connected\":true},"
  done
  printf '{"schema_version":1,"sources":[%s]}' "${st%,}" > "$R/status.json.tmp" && mv "$R/status.json.tmp" "$R/status.json"
  sleep 0.3
done
EOF
cat > "$T/bin/systemctl" <<'EOF'
#!/bin/sh
S=$ST_TEST_DIR; case "$1" in
  is-active) cat "$S/svc" 2>/dev/null || echo inactive ;;
  start|restart) [ -f "$S/pid" ] && kill $(cat "$S/pid") 2>/dev/null; echo "$1" >> "$S/calls"; echo active > "$S/svc"
                 nohup "$(dirname "$0")/faketracer" "$S" >/dev/null 2>&1 & echo $! > "$S/pid" ;;
  stop) echo stop >> "$S/calls"; [ -f "$S/pid" ] && kill $(cat "$S/pid") 2>/dev/null; echo inactive > "$S/svc" ;;
  enable) echo enable >> "$S/calls" ;;
esac
EOF
printf '#!/bin/sh\necho "(journal simule)"\n' > "$T/bin/journalctl"
chmod +x "$T/bin/"*; export PATH="$T/bin:$PATH"

mkman() { # mkman <chemin donnees .jsonl.gz> [sha-force] : cree donnees + manifeste compact
  local f="$ST_TEST_DIR/trace/$1" sha; mkdir -p "$(dirname "$f")"; head -c "${2:-5000}" /dev/urandom > "$f"
  sha=${3:-$(sha256sum "$f" | cut -d' ' -f1)}
  printf '{"schema_version":1,"bytes":%s,"sha256":"%s","lines":3}' "$(wc -c < "$f" | tr -d ' ')" "$sha" > "${f%.jsonl.gz}.manifest.json"
}

echo "=== pull ==="
TODAY=$(date -u +%F)
mkman okx/2026/10/2026-10-20.jsonl.gz
mkman okx/2026/10/2026-10-21.jsonl.gz 30000
mkman binance-btcusdt/2026/10/2026-10-21.jsonl.gz 4000 $(printf 'f%.0s' $(seq 64))   # sha faux
mkman okx/$(date -u +%Y)/$(date -u +%m)/$TODAY.jsonl.gz                                  # jour en cours : ignore
echo partiel > "$ST_TEST_DIR/trace/okx/2026/10/2026-10-22.jsonl.gz.part-1"               # sans manifeste : ignore
L="$T/local"
before=$(find "$ST_TEST_DIR/trace" -type f | sort | xargs sha256sum | sha256sum)
echo "-- dry-run"; bash "$D/pull" --dry-run --local "$L"; rc=$?
d "dry-run : code 0" test $rc = 0
d "dry-run : serveur intact" test "$before" = "$(find "$ST_TEST_DIR/trace" -type f | sort | xargs sha256sum | sha256sum)"
echo "-- pull reel avec un sha faux"
mkdir -p "$L/okx/2026/10"; echo orphelin > "$L/okx/2026/10/2026-10-20.jsonl.gz.tmp"
bash "$D/pull" --yes --local "$L"; rc=$?
d "echec global (code 1) a cause du sha faux" test $rc = 1
d "jour valide rapatrie" test -f "$L/okx/2026/10/2026-10-21.jsonl.gz"
d "jour valide supprime cote serveur" test ! -f "$ST_TEST_DIR/trace/okx/2026/10/2026-10-21.jsonl.gz"
d "jour au sha faux : NON copie, serveur intact" bash -c "test ! -f '$L/binance-btcusdt/2026/10/2026-10-21.jsonl.gz' && test -f '$ST_TEST_DIR/trace/binance-btcusdt/2026/10/2026-10-21.jsonl.gz'"
d "jour en cours et .part intacts cote serveur" bash -c "test -f '$ST_TEST_DIR/trace/okx/$(date -u +%Y)/$(date -u +%m)/$TODAY.jsonl.gz' && test -f '$ST_TEST_DIR/trace/okx/2026/10/2026-10-22.jsonl.gz.part-1'"
d ".tmp orphelin supprime" test ! -f "$L/okx/2026/10/2026-10-20.jsonl.gz.tmp"
echo "-- idempotence : 2e passage, fichier deja present localement mais encore sur le serveur"
mkman okx/2026/10/2026-10-23.jsonl.gz; cp "$ST_TEST_DIR/trace/okx/2026/10/2026-10-23.jsonl.gz" "$T/x.gz"
mkdir -p "$L/okx/2026/10"; cp "$T/x.gz" "$L/okx/2026/10/2026-10-23.jsonl.gz"
out=$(bash "$D/pull" --yes --local "$L" 2>&1); echo "$out" | sed -n '/plan/,$p'
d "deja present reconnu" bash -c "echo '$out' | grep -q 'deja present et verifie'"
d "puis supprime cote serveur" test ! -f "$ST_TEST_DIR/trace/okx/2026/10/2026-10-23.jsonl.gz"
echo "-- --keep"
mkman okx/2026/10/2026-10-24.jsonl.gz; bash "$D/pull" --yes --keep --local "$L" >/dev/null 2>&1
d "--keep : copie locale ET serveur conserve" bash -c "test -f '$L/okx/2026/10/2026-10-24.jsonl.gz' && test -f '$ST_TEST_DIR/trace/okx/2026/10/2026-10-24.jsonl.gz'"
d "copie locale = sha256 du manifeste" test "$(sha256sum "$L/okx/2026/10/2026-10-24.jsonl.gz" | cut -d' ' -f1)" = "$(sed -nE 's/.*"sha256":"([0-9a-f]{64})".*/\1/p' "$L/okx/2026/10/2026-10-24.manifest.json")"
d "lock libere" test ! -d "$L/.pull.lock"

echo; echo "=== start / stop ==="
cp "$D/../config.example.json" "$T/ex.json"
N=$(grep -c '"adapter"' "$D/../config.example.json")
echo "-- start : 2 sources, service arrete -> demarrage"
bash "$D/start" --yes --config-only --sources okx-btc-usdt-swap,coinbase-btc-usd --interval 2; rc=$?
d "start : code 0" test $rc = 0
d "service actif" test "$(cat "$ST_TEST_DIR/svc")" = active
d "config : 2 sources actives exactement" test "$(grep -c '"enabled": *true' "$ST_TEST_DIR/config.json")" = 2
d "config : host_id et root remplaces" bash -c "grep -q '\"host_id\": \"tracing\"' '$ST_TEST_DIR/config.json' && grep -q '\"root\": \"$ST_TEST_DIR/trace\"' '$ST_TEST_DIR/config.json'"
d "systemctl start appele (pas restart)" bash -c "grep -q '^start' '$ST_TEST_DIR/calls' && ! grep -q '^restart' '$ST_TEST_DIR/calls'"
echo "-- start --config-only sur service actif, seul enabled change -> relecture a chaud (pas de restart)"
: > "$ST_TEST_DIR/calls"
bash "$D/start" --yes --config-only --sources okx-btc-usdt-swap,coinbase-btc-usd,binance-btcusdt --interval 2 | sed -n '/recapitulatif/,/Lancer/p;$p'; rc=${PIPESTATUS[0]}
d "start (hot) : code 0" test $rc = 0
d "aucun start/restart/stop appele" bash -c "! grep -Eq 'start|restart|stop' '$ST_TEST_DIR/calls'"
d "config : 3 sources actives" test "$(grep -c '"enabled": *true' "$ST_TEST_DIR/config.json")" = 3
echo "-- stop partiel : desactive coinbase-btc-usd a chaud"
bash "$D/stop" --yes --sources coinbase-btc-usd; rc=$?
d "stop partiel : code 0" test $rc = 0
d "coinbase desactivee, 2 autres actives" bash -c "grep '\"coinbase-btc-usd\"' '$ST_TEST_DIR/config.json' | grep -q 'false' && test \$(grep -c '\"enabled\": *true' '$ST_TEST_DIR/config.json') = 2"
d "service toujours actif, sans restart" bash -c "test \"\$(cat '$ST_TEST_DIR/svc')\" = active && ! grep -Eq 'stop|restart' '$ST_TEST_DIR/calls'"
d "stop : source inconnue refusee" bash -c "! bash '$D/stop' --yes --sources nimporte-quoi >/dev/null 2>&1"
echo "-- stop total (--all) = arret du service"
bash "$D/stop" --yes --all; rc=$?
d "stop --all : code 0" test $rc = 0
d "service inactive" test "$(cat "$ST_TEST_DIR/svc")" = inactive
echo "-- start complet de nouveau (sans --config-only : binaire simule, envoi, remplacement atomique) : redemarre, 8 sources"
bash "$D/start" --yes --sources "$(sed -nE 's/.*"name": *"([^"]+)".*/\1/p' "$D/../config.example.json" | paste -sd, -)" --interval 2 >/dev/null; rc=$?
d "start : code 0, $N sources actives" bash -c "test $rc = 0 && test \$(grep -c '\"enabled\": *true' '$ST_TEST_DIR/config.json') = $N"
same_bin() { [ "$(sha256sum "$ST_TEST_BIN" | cut -d" " -f1)" = "$(sha256sum "$ST_TEST_DIR/opt/bin/source-tracing" | cut -d" " -f1)" ] && [ ! -e "$ST_TEST_DIR/opt/bin/source-tracing.new" ] && ! ls /tmp/st-bin.* >/dev/null 2>&1; }
d "binaire installe = binaire local (sha256), sans .new ni temporaire" same_bin
d "service redemarre apres remplacement du binaire" grep -q "^start" "$ST_TEST_DIR/calls"
echo "-- erreur : aucune source / fichier qui ne grossit pas"
d "start sans source : refuse" bash -c "! bash '$D/start' --yes --sources '' >/dev/null 2>&1"
bash "$D/stop" --yes --all >/dev/null 2>&1
printf '#!/bin/sh\nwhile :; do sleep 1; done\n' > "$T/bin/faketracer"   # tracer vivant mais muet
bash "$D/start" --yes --config-only --sources okx-btc-usdt-swap --interval 2 >/dev/null 2>&1; rc=$?
d "tracer muet (status.json perime) : start echoue (code 1)" test $rc = 1
bash "$D/stop" --yes --all >/dev/null 2>&1

echo; echo "=== bilan : $PASS PASS, $FAIL FAIL ==="
[ "$FAIL" = 0 ]
