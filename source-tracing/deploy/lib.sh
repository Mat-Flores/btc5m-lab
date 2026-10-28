# Bibliotheque commune de start / stop / pull (a sourcer, pas a executer).
# Aucune dependance cote client au-dela de bash + ssh (+ tar, sha256sum, sed, awk de Git Bash).

DEPLOY_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ST_DIR="$(cd "$DEPLOY_DIR/.." && pwd)"          # source-tracing/
REPO_DIR="$(cd "$ST_DIR/.." && pwd)"            # engine/ est requis pour compiler (dependances par chemin)

APP=/opt/updown5m
DATA=/var/lib/updown5m
UNIT=updown5m-tracing
RUSER="${REMOTE_USER:-tgap}"
R_ROOT="${REMOTE_TRACE_ROOT:-$DATA/trace}"
R_CONFIG="${REMOTE_CONFIG:-$DATA/config.json}"
# Mode test : PT_TEST_NOSSH=1 execute les commandes "serveur" en local (aucun ssh, aucun sudo).
# Le test fournit alors ST_TEST_DIR (faux serveur) et un faux systemctl dans le PATH.
if [ "${PT_TEST_NOSSH:-}" = 1 ]; then
  : "${ST_TEST_DIR:?PT_TEST_NOSSH=1 exige ST_TEST_DIR}"
  R_ROOT="$ST_TEST_DIR/trace"; R_CONFIG="$ST_TEST_DIR/config.json"; DATA="$ST_TEST_DIR"
fi

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
# bar <courant> <total> <largeur> -> [#####-----]  50%
bar() {
  local c=$1 t=$2 w=${3:-24} f p
  [ "$t" -gt 0 ] || t=1; [ "$c" -le "$t" ] || c=$t
  f=$(( c * w / t )); p=$(( c * 100 / t ))
  printf '[%s%s%s%s%s] %3d%%' "$GRN" "$(rep '#' "$f")" "$RST$DIM" "$(rep '-' "$((w-f))")" "$RST" "$p"
}
human() { awk -v b="$1" 'BEGIN{split("o Ko Mo Go To",u," ");i=1;while(b>=1024&&i<5){b/=1024;i++}; if(i==1) printf "%d %s", b, u[i]; else printf "%.1f %s", b, u[i]}'; }
# etape <i> <n> <texte> : barre de progression globale
etape() { printf '\n%s[%s/%s]%s %s\n  ' "$BLD" "$1" "$2" "$RST" "$3"; bar "$(( $1 - 1 ))" "$2" 30; printf '\n'; }

# ---- parametres de connexion (.env) ---------------------------------------------------------------------------
# Le .env n'est JAMAIS source (pas d'execution) : seules les 3 cles connues sont lues.
load_env() {
  local f="${ST_ENV_FILE:-$DEPLOY_DIR/.env}" k v r h
  if [ "${PT_TEST_NOSSH:-}" = 1 ]; then SSH_DEST=test@local; SSH_KEY=none; HOST_ID="${TRACING_HOST_ID:-tracing}"; return 0; fi
  [ -f "$f" ] || die "fichier $f absent : copier .env.example en .env et le remplir (voir COMMANDES.md)"
  while IFS='=' read -r k v; do
    v="${v%%#*}"; v="$(printf '%s' "$v" | sed -E 's/^[[:space:]]+|[[:space:]]+$//g; s/^"(.*)"$/\1/')"
    case "$k" in
      TRACING_SSH_DEST) SSH_DEST="$v" ;; TRACING_SSH_KEY) SSH_KEY="$v" ;; TRACING_HOST_ID) HOST_ID="$v" ;;
    esac
  done < <(grep -E '^[A-Z_]+=' "$f" | tr -d '\r')
  HOST_ID="${HOST_ID:-tracing}"
  [ -n "${SSH_DEST:-}" ] && [ -n "${SSH_KEY:-}" ] || die "TRACING_SSH_DEST et TRACING_SSH_KEY sont obligatoires dans $f"
  case "$SSH_DEST" in *'<'*|*'>'*) die "TRACING_SSH_DEST contient encore un modele ($SSH_DEST)" ;; esac
  SSH_KEY="${SSH_KEY/#\~/$HOME}"
  [ -f "$SSH_KEY" ] || die "cle SSH introuvable : $SSH_KEY"
  [[ "$HOST_ID" =~ ^[A-Za-z0-9][A-Za-z0-9_-]*$ ]] || die "TRACING_HOST_ID invalide : $HOST_ID"
  # GARDE historique : jamais sur dublin-live (ssh -G ne se connecte pas, il resout seulement la config)
  r=$(ssh -G "$SSH_DEST" 2>/dev/null | awk '$1=="hostname"{print $2}') || r=""
  for h in "$SSH_DEST" "$r"; do case "${h,,}" in *3.253.60.148*|*dublin-live*) die "REFUS : dublin-live interdit ($h)" ;; esac; done
}

SSH_OPTS=(-o BatchMode=yes -o IdentitiesOnly=yes -o StrictHostKeyChecking=accept-new -o ConnectTimeout=15 -o ServerAliveInterval=15)
# rsh <cmd sh> : en tant que tgap (lecture/ecriture des donnees). La commande ne doit pas contenir d'apostrophe.
rsh() {
  if [ "${PT_TEST_NOSSH:-}" = 1 ]; then sh -c "$1"
  else ssh -i "$SSH_KEY" "${SSH_OPTS[@]}" "$SSH_DEST" "sudo -n -u $RUSER sh -c '$1'"; fi
}
# rroot <cmd sh> : en root (systemctl, journalctl, setup)
rroot() {
  if [ "${PT_TEST_NOSSH:-}" = 1 ]; then sh -c "$1"
  else ssh -i "$SSH_KEY" "${SSH_OPTS[@]}" "$SSH_DEST" "sudo -n sh -c '$1'"; fi
}
# rplain <cmd> : utilisateur ssh simple (staging /tmp)
rplain() {
  if [ "${PT_TEST_NOSSH:-}" = 1 ]; then sh -c "$1"
  else ssh -i "$SSH_KEY" "${SSH_OPTS[@]}" "$SSH_DEST" "$1"; fi
}
check_conn() {
  [ "${PT_TEST_NOSSH:-}" = 1 ] && return 0
  command -v ssh >/dev/null || die "ssh introuvable (installer Git for Windows / OpenSSH)"
  rplain true || die "connexion SSH impossible vers $SSH_DEST (adresse, cle, groupe de securite port 22 : voir COMMANDES.md)"
}

# ---- confirmation et menus ---------------------------------------------------------------------------------------
confirm() { # confirm <question> ; --yes (ASSUME_YES=1) saute la question
  [ "${ASSUME_YES:-0}" = 1 ] && return 0
  [ -t 0 ] || die "pas de terminal interactif : relancer avec --yes pour confirmer d'avance"
  local a; read -r -p "  $BLD$1$RST [o/N] " a; case "$a" in o|O|y|Y|oui) return 0 ;; *) return 1 ;; esac
}
# select_multi : ITEMS[] (libelles) et SEL[] (0/1) en entree ; SEL modifie en sortie. Retour 1 = annule.
select_multi() {
  [ -t 0 ] && [ -t 1 ] || die "pas de terminal interactif : utiliser --sources nom1,nom2"
  local n=${#ITEMS[@]} cur=0 first=1 i k k2 mark ptr
  printf '\033[?25l'
  while :; do
    [ $first = 1 ] || printf '\033[%dA' "$((n+1))"; first=0
    for ((i=0;i<n;i++)); do
      if [ "${SEL[i]}" = 1 ]; then mark="${GRN}[x]${RST}"; else mark="[ ]"; fi
      if [ $i = $cur ]; then ptr="${CYA}>${RST}"; else ptr=" "; fi
      printf '\033[K  %s %s %s\n' "$ptr" "$mark" "${ITEMS[i]}"
    done
    printf '\033[K  %s\n' "${DIM}haut/bas : deplacer | espace : cocher | a : tout | n : rien | entree : valider | q : annuler${RST}"
    IFS= read -rsn1 k || { printf '\033[?25h'; return 1; }
    if [ "$k" = $'\033' ]; then read -rsn2 -t 0.2 k2 || true; k="$k$k2"; fi
    case "$k" in
      $'\033[A'|k) cur=$(( (cur-1+n) % n )) ;;
      $'\033[B'|j) cur=$(( (cur+1) % n )) ;;
      ' ') SEL[cur]=$(( 1 - SEL[cur] )) ;;
      a) for ((i=0;i<n;i++)); do SEL[i]=1; done ;;
      n) for ((i=0;i<n;i++)); do SEL[i]=0; done ;;
      q) printf '\033[?25h'; return 1 ;;
      '') printf '\033[?25h'; return 0 ;;
    esac
  done
}
# apply_names <a,b,c> : coche exactement ces noms dans SEL (erreur si nom inconnu)
apply_names() {
  local w i found; local -a W; IFS=, read -ra W <<< "$1"
  for ((i=0;i<${#ITEMS[@]};i++)); do SEL[i]=0; done
  for w in "${W[@]}"; do
    found=0; for ((i=0;i<${#ITEMS[@]};i++)); do [ "${ITEMS[i]}" = "$w" ] && { SEL[i]=1; found=1; }; done
    [ $found = 1 ] || die "source inconnue : $w (disponibles : ${ITEMS[*]})"
  done
}

# ---- config.json : une source par ligne (format de config.example.json) ----------------------------------------
# cfg_list <texte> -> lignes "nom true|false" ; echoue si le format n'est pas celui attendu
cfg_list() {
  local out n a
  out=$(printf '%s\n' "$1" | sed -nE 's/.*"name" *: *"([A-Za-z0-9_-]+)".*"enabled" *: *(true|false).*/\1 \2/p')
  n=$(printf '%s\n' "$out" | grep -c .); a=$(printf '%s\n' "$1" | grep -c '"adapter"')
  [ "$n" -gt 0 ] && [ "$n" = "$a" ] || return 1
  printf '%s\n' "$out"
}
# cfg_set <texte> <nom> <true|false> -> texte modifie (seul le champ enabled de cette source)
cfg_set() { printf '%s\n' "$1" | sed -E "/\"name\" *: *\"$2\"/ s/(\"enabled\" *: *)(true|false)/\1$3/"; }
# cfg_shape <texte> : texte sans les valeurs enabled (si seul enabled differe, la relecture a chaud suffit)
cfg_shape() { printf '%s\n' "$1" | sed -E 's/("enabled" *: *)(true|false)/\1X/'; }
# cfg_push <texte> : ecriture atomique cote serveur (tmp + rename => le mtime change, le tracer relit en <= 1 s)
cfg_push() { printf '%s\n' "$1" | rsh "cat > $R_CONFIG.tmp && mv -f $R_CONFIG.tmp $R_CONFIG"; }

# ---- etat du serveur ---------------------------------------------------------------------------------------------
svc_state() { rroot "systemctl is-active $UNIT 2>/dev/null; true" | head -1; }
today_utc() { rsh "date -u +%F"; }
# today_sizes : lignes "octets source" (somme des .part-* du jour UTC, par source)
today_sizes() {
  local d; d=$(today_utc) || return 1
  rsh "find $R_ROOT -type f -name \"$d.jsonl.gz.part-*\" -exec wc -c {} +" 2>/dev/null |
    awk -v root="$R_ROOT/" '$2!="total"{p=$2; sub(root,"",p); split(p,a,"/"); s[a[1]]+=$1} END{for(k in s) print s[k], k}' | sort -k2
}
free_bytes() { rroot "df --output=avail -B1 $DATA 2>/dev/null || df --output=avail -B1 /" | tail -1 | tr -d ' '; }
