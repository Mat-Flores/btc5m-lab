#!/usr/bin/env bash
# Installation unique (idempotente) du noeud copy-trading : Ubuntu arm64 ou x86_64. Pas de compilateur ici : le binaire
# est compile cote client (WSL) et envoye par deploy/start.   sudo bash setup.sh   (envoye et lance par deploy/start)
set -euo pipefail

# GARDES : jamais dublin-live ni Up&Down-Tracing (IP publique IMDSv2 ; pas de reponse IMDS = non bloquant)
[ "$(hostname)" != ip-172-31-28-36 ] || { echo "REFUS : dublin-live"; exit 3; }
TOK=$(curl -s -m 2 -X PUT -H "X-aws-ec2-metadata-token-ttl-seconds: 60" http://169.254.169.254/latest/api/token || true)
PUB=$(curl -s -m 2 -H "X-aws-ec2-metadata-token: $TOK" http://169.254.169.254/latest/meta-data/public-ipv4 || true)
case "$PUB" in 3.253.60.148) echo "REFUS : dublin-live (IP publique)"; exit 3 ;; 34.247.53.227) echo "REFUS : Up&Down-Tracing (IP publique)"; exit 3 ;; esac

APP=/opt/copy-trading
DATA=/var/lib/copy-trading
UNIT_SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/copy-trading.service"
[ "$(id -u)" = 0 ] || { echo "a lancer en root (sudo)"; exit 1; }
[ -f "$UNIT_SRC" ] || { echo "introuvable : $UNIT_SRC (doit etre a cote du script)"; exit 1; }

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq ca-certificates chrony >/dev/null

id -u copyt >/dev/null 2>&1 || useradd --system --no-create-home --home-dir "$DATA" --shell /usr/sbin/nologin copyt
mkdir -p "$APP/bin" "$DATA"
chown copyt:copyt "$APP/bin" "$DATA"; chmod 700 "$DATA"   # journal, config et wallet chiffre : copyt seul

# horloge : service de temps AWS d'abord (les latences comparent notre horloge a celle du CLOB)
cat > /etc/chrony/conf.d/copy-trading.conf <<'CONF'
server 169.254.169.123 prefer iburst minpoll 4 maxpoll 4
makestep 0.1 3
CONF
systemctl restart chrony
sleep 3; chronyc tracking | grep -E "Reference|System time" || true

# unit du mode paper ; NI activee NI demarree ici (deploy/start choisit le mode)
install -m 644 "$UNIT_SRC" /etc/systemd/system/copy-trading.service
systemctl daemon-reload
echo "OK. Suite : deploy/start depuis le poste de travail."
