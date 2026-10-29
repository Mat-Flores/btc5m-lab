# HANDOFF — main

**Branche** : `main` (branche unique, pas de remote).

**Objectif** : migration `btc5m-lag-sim` → `updown5m-lab` (spec `PROMPT-MIGRATION.md`, décisions utilisateur des 27-28/10 qui la priment) : TERMINÉE le 2026-10-28, hors redéploiement du traceur (décision utilisateur).

## Fait (vérifié le 2026-10-28)
- engine, source-tracing, backtest, docs, README (racine, engine, source-tracing, backtest, docs) commités.
- Backtest : entrée `pack` (validée : `validate` + lots x01/x04 à 0 écart avec l'oracle Node) et entrée `trace` (`backtest/src/run.rs` : Engine + TraceVenue ; parité avec `replay` hors décalage d'horloge des anciennes sessions).
- E1 : correction 1 `venue.cancel_ahead_factor` (défaut 1, 0,5 recommandé) ; correction 2 non retenue (erreur de mesure de `validate`).
- p9/p10 rejoués identiques au live ; staging `updown5m-staging/` entièrement supprimé.
- Tracing actif sur « Up&Down-Tracing » (+5,5 Mo/min) ; `pull` réel du 28/10 OK (8 fichiers, sha256 vérifiés).
- docs/ restructuré : racine = projet, `recherche/`, `migration/`, `pistes-ecartees.md`, 0 lien cassé.

## Reste à faire
1. Vérifier qu'un `pull` du 29/10 ramène des lignes `http` gamma avec `"closed":true` (traceur redéployé le 28/10 avec `gamma_prev_grace_s` 900).
2. `pull` quotidien (≈ 4 Go/jour, 25 Go libres sur le serveur).
3. Idée non décidée : PnL des fenêtres « choc » avec/sans `no_knife`, `max_diverge`, `max_imbalance`, `Move`, puis éventuelle pause sur calendrier macro, validée en walk-forward + Monte Carlo.

FAIT aussi le 28/10 : engine suit le tick du marché (`follow_tick`) ; traceur redéployé (900 s) ; convert-report et deploy/ vide supprimés.
FAIT le 29/10 (limites) : note d'unification papier/TraceVenue en tête de `engine/app/src/paper.rs` (à faire quand une stratégie maker passera en papier) ; `last_trade_price` validé contre data-api (`backtest/scripts/ltp-check.mjs` : 99,5 % des tx, niveaux 90/92 %) + crédit au niveau miroir ; TP/SL mesuré (G2-C 961,55 vs 950,86 $, G4-E ruiné des deux côtés) après correction du refus de vente au-delà du solde ; arcsine copié (`backtest/data/wallets/`, gitignoré) et `validate --maker` à 0 écart avec l'oracle ; `cancel_ahead_factor` revenu à 1 (arcsine contredit 0,5).

## Décisions et pièges
- `copy-trading/` : projet isolé de l'utilisateur, ne jamais le lire ni le commiter ; commiter avec `git commit -- <chemins>` (l'utilisateur stage lui-même). Exclu de graphify (`graphify-out/.graphify_build.json`).
- Rust seulement dans WSL `Ubuntu-24.04` ; depuis Git Bash : `MSYS_NO_PATHCONV=1` et passer par un script (les `$` sont sinon développés par Git Bash).
- Binance n'est enregistré que 14→17/10 et 26→28/10 : le signal par défaut n'existe pas les autres jours.
- Les anciennes traces Node n'ont ni trades ni conditionId : borne `exact` sans fills de trade, marché = slug.
- Le `.jsonl.gz` de `2026-10-16T01-17-20_dublin` (source) est tronqué.

## Fichiers clés
- `backtest/README.md` (usage, paramètres, limites) ; `backtest/src/run.rs` (mode trace) ; `backtest/src/venue.rs` (exchange validé).
- `docs/README.md` (index) ; `docs/exploitation.md` (serveur, routine) ; `docs/pistes-ecartees.md`.
- `source-tracing/COMMANDES.md` ; `source-tracing/deploy/{start,stop,pull}`.

## Commandes utiles
- Tests : `MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash "/mnt/c/dev/REA Projects/updown5m-lab/backtest/scripts/test-wsl.sh"`.
- Non-régression : `backtest/scripts/validate-wsl.sh` (≈ 11 min), `batch-wsl.sh x01-retest-exact`.
- Backtest sur traces : `backtest run --config config/default.json --override <f.json>` (dans WSL, depuis `backtest/`).

## Prochaine action
Demander à l'utilisateur le feu vert pour `source-tracing/deploy/start` (redéploiement du traceur avec 900 s de suivi gamma), puis vérifier le lendemain qu'un `pull` ramène des lignes `http` gamma avec `"closed":true`.
