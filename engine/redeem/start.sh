#!/bin/bash
# phase 10 live run (copied to ~/p10/start.sh on the node): real orders of 5 pUSD at most while the wallet can pay, and
# the auto-redemption of the engine's won markets. The passphrase is typed here and handed to both on stdin: never on
# a command line, never written. Both run detached (setsid). The engine stops after 10 h and writes its report;
# timeout kills it at 10 h 05 whatever happens. The redemption runs 15 min longer, so the last windows are claimed.
cd /home/ubuntu/p10 || exit 1
[ -f wallet ] || { echo "fichier wallet absent"; exit 1; }
read -rsp "Phrase secrète du wallet : " P; echo
printf '%s\n' "$P" | setsid timeout -k 60 36300 ./engine p10.json > report.json 2> err.log &
ENGINE=$!
printf '%s\n' "$P" | setsid timeout -k 60 37200 node redeem.mjs engine.jsonl wallet redeem.log 37000 > redeem.err 2>&1 &
REDEEM=$!
unset P
sleep 10
if kill -0 $ENGINE 2>/dev/null; then
  sudo renice -n -10 -p $ENGINE $(pgrep -P $ENGINE) >/dev/null
  echo "engine armé et lancé (pid $ENGINE)"
else
  echo "engine : arrêt au démarrage :"; cat err.log
fi
if kill -0 $REDEEM 2>/dev/null && grep -q '"armed"' redeem.log 2>/dev/null; then
  echo "réclamation automatique armée (pid $REDEEM)"
else
  echo "réclamation : pas armée :"; cat redeem.err redeem.log 2>/dev/null
fi
