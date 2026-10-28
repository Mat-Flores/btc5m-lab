#!/usr/bin/env bash
# Installation unique (idempotente) du noeud de tracing : Ubuntu (arm64 ou x86_64).
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
RUST_VERSION=1.99.1
UNIT_SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/updown5m-tracing.service"

[ "$(id -u)" = 0 ] || { echo "a lancer en root (sudo)"; exit 1; }
[ -f "$UNIT_SRC" ] || { echo "introuvable : $UNIT_SRC (doit etre a cote du script)"; exit 1; }
case "$(uname -m)" in
  aarch64) RUST_TRIPLE=aarch64-unknown-linux-gnu ;;
  x86_64)  RUST_TRIPLE=x86_64-unknown-linux-gnu ;;
  *) echo "architecture non geree : $(uname -m)"; exit 1 ;;
esac

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq ca-certificates curl build-essential pkg-config gzip chrony >/dev/null

# --- utilisateur, dossiers (code = tgap pour le build ; donnees = tgap)
id -u tgap >/dev/null 2>&1 || useradd --system --create-home --home-dir "$HOME_TGAP" --shell /usr/sbin/nologin tgap
mkdir -p "$APP" "$TRACE" "$HOME_TGAP"
chown tgap:tgap "$APP" "$TRACE" "$HOME_TGAP"
chown tgap:tgap "$DATA"; chmod 755 "$DATA"   # config.json y vit (ecrit par tgap)

# --- swap 2 Go si absente : cargo build -j1 sur 2 Go de RAM
if [ -z "$(swapon --show --noheadings)" ]; then
  [ -f /swapfile ] || { fallocate -l 2G /swapfile && chmod 600 /swapfile && mkswap /swapfile >/dev/null; }
  swapon /swapfile
  grep -q '^/swapfile ' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
fi


# --- Rust 1.99.1 pour tgap (rustup-init verifie par son .sha256)
if ! sudo -u tgap env HOME="$HOME_TGAP" "$HOME_TGAP/.cargo/bin/rustup" toolchain list 2>/dev/null | grep -q "^$RUST_VERSION"; then
  T=$(mktemp -d); B="https://static.rust-lang.org/rustup/dist/$RUST_TRIPLE"
  curl -fsSL "$B/rustup-init" -o "$T/rustup-init"
  EXPECTED=$(curl -fsSL "$B/rustup-init.sha256" | awk '{print $1}')
  echo "$EXPECTED  $T/rustup-init" | sha256sum -c -
  chmod 755 "$T" "$T/rustup-init"
  sudo -u tgap env HOME="$HOME_TGAP" "$T/rustup-init" -y --profile minimal --default-toolchain "$RUST_VERSION" --no-modify-path
  rm -rf "$T"
fi
sudo -u tgap env HOME="$HOME_TGAP" "$HOME_TGAP/.cargo/bin/rustc" --version

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
