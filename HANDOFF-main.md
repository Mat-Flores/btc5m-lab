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
1. Redéployer le traceur (`source-tracing/deploy/start`) pour `gamma_prev_grace_s` = 900 : sans cela les traces courantes n'ont aucun résultat officiel (backtest sans règlement).
2. `pull` quotidien (≈ 4 Go/jour, 25 Go libres sur le serveur).
3. Idée non décidée : PnL des fenêtres « choc » avec/sans `no_knife`, `max_diverge`, `max_imbalance`, `Move`, puis éventuelle pause sur calendrier macro (TOML `macro_calendar`…), validée en walk-forward + Monte Carlo.
4. Écart engine connu : tick 0,001 refusé (tick de la fenêtre figé, `engine/app/src/engine.rs` `amounts`) ; à décider avant tout backtest maker sur traces.
5. Décider : `cancel_ahead_factor` 0,5 par défaut ? `source-tracing/convert-report.json` à commiter ou ignorer ?

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
