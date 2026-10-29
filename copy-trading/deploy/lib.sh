# Bibliotheque commune de start / stop / report (a sourcer, pas a executer). Modele : source-tracing/deploy/lib.sh.
# Aucune dependance cote client au-dela de bash + ssh (+ sha256sum, sed, awk de Git Bash) et de WSL pour compiler.

DEPLOY_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CT_DIR="$(cd "$DEPLOY_DIR/.." && pwd)"          # copy-trading/
APP=/opt/copy-trading
DATA=/var/lib/copy-trading
UNIT=copy-trading
RUSER=copyt
R_CONFIG=$DATA/config.json
R_JOURNAL=$DATA/journal.jsonl
R_WALLET=$DATA/wallet
R_LOCK=$R_JOURNAL.live-lock
R_RUNLOG=$DATA/run.log

# ---- couleurs (desactivees si pas de terminal ou NO_COLOR) -------------------------------------------------
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  RST=$'\033[0m'; BLD=$'\033[1m'; DIM=$'\033[2m'; RED=$'\033[31m'; GRN=$'\033[32m'; YEL=$'\033[33m'; CYA=$'\033[36m'
else RST=; BLD=; DIM=; RED=; GRN=; YEL=; CYA=; fi
title() { printf '\n%s== %s ==%s\n' "$BLD$CYA" "$*" "$RST"; }
info()  { printf '  %s\n' "$*"; }
ok()    { printf '  %sOK%s    %s\n' "$GRN" "$RST" "$*"; }
warn()  { printf '  %sATTENTION%s %s\n' "$YEL" "$RST" "$*"; }
err()   { printf '  %sECHEC%s %s\n' "$RED" "$RST" "$*" >&2; }
die()   { err "$*"; exit 1; }
rep() { local s=""; local i; for ((i=0;i<$2;i++)); do s+="$1"; done; printf '%s' "$s"; }
bar() {
  local c=$1 t=$2 w=${3:-24} f p
  [ "$t" -gt 0 ] || t=1; [ "$c" -le "$t" ] || c=$t
  f=$(( c * w / t )); p=$(( c * 100 / t ))
  printf '[%s%s%s%s%s] %3d%%' "$GRN" "$(rep '#' "$f")" "$RST$DIM" "$(rep '-' "$((w-f))")" "$RST" "$p"
}
etape() { printf '\n%s[%s/%s]%s %s\n  ' "$BLD" "$1" "$2" "$RST" "$3"; bar "$(( $1 - 1 ))" "$2" 30; printf '\n'; }

# ---- parametres (.env) -----------------------------------------------------------------------------------------
# Le .env n'est JAMAIS source (pas d'execution) : seules les 3 cles connues sont lues.
load_env() {
  local f="${CT_ENV_FILE:-$DEPLOY_DIR/.env}" k v r h
  [ -f "$f" ] || die "fichier $f absent : copier .env.example en .env et le remplir (voir COMMANDES.md)"
  while IFS='=' read -r k v; do
    v="${v%%#*}"; v="$(printf '%s' "$v" | sed -E 's/^[[:space:]]+|[[:space:]]+$//g; s/^"(.*)"$/\1/')"
    case "$k" in COPY_SSH_DEST) SSH_DEST="$v" ;; COPY_SSH_KEY) SSH_KEY="$v" ;; COPY_WALLET) WALLET="$v" ;; esac
  done < <(grep -E '^[A-Z_]+=' "$f" | tr -d '\r')
  [ -n "${SSH_DEST:-}" ] && [ -n "${SSH_KEY:-}" ] || die "COPY_SSH_DEST et COPY_SSH_KEY sont obligatoires dans $f"
  case "$SSH_DEST" in *'<'*|*'>'*) die "COPY_SSH_DEST contient encore un modele ($SSH_DEST)" ;; esac
  SSH_KEY="${SSH_KEY/#\~/$HOME}"
  [ -f "$SSH_KEY" ] || die "cle SSH introuvable : $SSH_KEY"
  WALLET="${WALLET:-$(sed -nE 's/.*"wallet" *: *"(0x[0-9a-fA-F]{40})".*/\1/p' "$CT_DIR/config.example.json")}"
  [[ "$WALLET" =~ ^0x[0-9a-fA-F]{40}$ ]] || die "COPY_WALLET invalide : $WALLET (0x + 40 hex attendu)"
  # GARDES : jamais dublin-live ni Up&Down-Tracing (ssh -G ne se connecte pas, il resout seulement la config)
  r=$(ssh -G "$SSH_DEST" 2>/dev/null | awk '$1=="hostname"{print $2}') || r=""
  for h in "$SSH_DEST" "$r"; do case "${h,,}" in
    *3.253.60.148*|*dublin-live*) die "REFUS : dublin-live interdit ($h)" ;;
    *34.247.53.227*|*tracing*) die "REFUS : Up&Down-Tracing interdit ($h)" ;;
  esac; done
}

SSH_OPTS=(-o BatchMode=yes -o IdentitiesOnly=yes -o StrictHostKeyChecking=accept-new -o ConnectTimeout=15 -o ServerAliveInterval=15)
# rsh <cmd sh> : en tant que copyt (donnees). La commande ne doit pas contenir d'apostrophe.
rsh()    { ssh -i "$SSH_KEY" "${SSH_OPTS[@]}" "$SSH_DEST" "sudo -n -u $RUSER sh -c '$1'"; }
# rroot <cmd sh> : en root (systemctl, setup)
rroot()  { ssh -i "$SSH_KEY" "${SSH_OPTS[@]}" "$SSH_DEST" "sudo -n sh -c '$1'"; }
# rplain <cmd> : utilisateur ssh simple (staging /tmp)
rplain() { ssh -i "$SSH_KEY" "${SSH_OPTS[@]}" "$SSH_DEST" "$1"; }
check_conn() {
  command -v ssh >/dev/null || die "ssh introuvable (installer Git for Windows / OpenSSH)"
  rplain true || die "connexion SSH impossible vers $SSH_DEST (adresse, cle, groupe de securite port 22 : voir COMMANDES.md)"
}

confirm() { # confirm <question> ; --yes (ASSUME_YES=1) saute la question
  [ "${ASSUME_YES:-0}" = 1 ] && return 0
  [ -t 0 ] || die "pas de terminal interactif : relancer avec --yes pour confirmer d'avance"
  local a; read -r -p "  $BLD$1$RST [o/N] " a; case "$a" in o|O|y|Y|oui) return 0 ;; *) return 1 ;; esac
}

# ---- etat du serveur ---------------------------------------------------------------------------------------------
installed()  { rplain "test -x $APP/bin/copy-trading && test -f /etc/systemd/system/$UNIT.service" 2>/dev/null; }
svc_state()  { rroot "systemctl is-active $UNIT 2>/dev/null; true" | head -1; }
# live_pid : pid du run live (lance hors systemd), vide sinon
live_pid()   { rroot "pgrep -u $RUSER -f \"^$APP/bin/copy-trading run\" | head -1; true" | grep -E '^[0-9]+$' | while read -r p; do
                 [ "$(svc_pid)" = "$p" ] || echo "$p"; done; }
svc_pid()    { rroot "systemctl show -p MainPID --value $UNIT 2>/dev/null; true" | head -1; }
lock_present() { rplain "sudo -n test -e $R_LOCK" 2>/dev/null; }
# state_line : "service <etat> | live <pid|-> | verrou <oui|non>"
state_line() {
  local s p l; s=$(svc_state); p=$(live_pid); lock_present && l=oui || l=non
  printf 'service %s | run live %s | verrou live %s' "${s:-inconnu}" "${p:--}" "$l"
}
