#!/usr/bin/env bash
# Installation unique (idempotente) du noeud de tracing : Ubuntu x86_64 ou arm64. Pas de compilateur ici :
# le binaire est compile cote client (WSL) et envoye par deploy/start ; pas de swap non plus (RAM suffisante a l execution).
#   sudo bash setup.sh   (envoye et lance par deploy/start au premier usage)
# Ne supprime JAMAIS de donnees : pas de retention automatique (voir deploy/pull).
set -euo pipefail

# GARDE : jamais sur dublin-live (hostname ou IP publique IMDSv2 ; pas de reponse IMDS = non bloquant)
[ "$(hostname)" != ip-172-31-28-36 ] || { echo "REFUS : dublin-live"; exit 3; }
TOK=$(curl -s -m 2 -X PUT -H "X-aws-ec2-metadata-token-ttl-seconds: 60" http://169.254.169.254/latest/api/token || true)
PUB=$(curl -s -m 2 -H "X-aws-ec2-metadata-token: $TOK" http://169.254.169.254/latest/meta-data/public-ipv4 || true)
[ "$PUB" != 3.253.60.148 ] || { echo "REFUS : dublin-live (IP publique)"; exit 3; }

APP=/opt/updown5m
DATA=/var/lib/updown5m
TRACE=$DATA/trace
HOME_TGAP=$DATA/home
UNIT_SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/updown5m-tracing.service"

[ "$(id -u)" = 0 ] || { echo "a lancer en root (sudo)"; exit 1; }
[ -f "$UNIT_SRC" ] || { echo "introuvable : $UNIT_SRC (doit etre a cote du script)"; exit 1; }

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq ca-certificates curl gzip chrony >/dev/null

# --- utilisateur, dossiers (code = tgap pour le build ; donnees = tgap)
id -u tgap >/dev/null 2>&1 || useradd --system --create-home --home-dir "$HOME_TGAP" --shell /usr/sbin/nologin tgap
mkdir -p "$APP/bin" "$TRACE" "$HOME_TGAP"
chown tgap:tgap "$APP" "$APP/bin" "$TRACE" "$HOME_TGAP"
chown tgap:tgap "$DATA"; chmod 755 "$DATA"   # config.json y vit (ecrit par tgap)

# --- horloge : service de temps AWS d'abord (les horodatages t_recv en dependent)
cat > /etc/chrony/conf.d/updown5m.conf <<'CONF'
server 169.254.169.123 prefer iburst minpoll 4 maxpoll 4
makestep 0.1 3
CONF
systemctl restart chrony
sleep 3; chronyc tracking | grep -E "Reference|System time|RMS offset" || true

# --- reseau : BBR + fq
cat > /etc/sysctl.d/90-updown5m.conf <<'CONF'
net.core.default_qdisc = fq
net.ipv4.tcp_congestion_control = bbr
net.ipv4.tcp_fastopen = 3
net.ipv4.tcp_slow_start_after_idle = 0
CONF
sysctl -q --system

# --- unit modele ; activee mais pas demarree (le binaire arrive avec deploy/start)
install -m 644 "$UNIT_SRC" /etc/systemd/system/updown5m-tracing.service
systemctl daemon-reload
systemctl enable updown5m-tracing

df -h "$TRACE" | tail -1
echo "OK. Suite : deploy/start depuis le poste de travail."
