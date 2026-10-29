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
Rien d'ouvert sur `main`.

FAIT le 29/10 : étude complète des fenêtres « choc » → piste écartée (`docs/recherche/fenetres-choc.md`, ligne dans `docs/pistes-ecartees.md`). Lot `backtest/config/chocs.json`, analyse `backtest/scripts/chocs.mjs`.
(Le `pull` quotidien est fait à la main par l'utilisateur, hors HANDOFF.)

FAIT le 29/10 : suivi gamma 900 s vérifié sur le `.part-2` du 29/10 (traceur redéployé le 29/10 à 02:11 UTC, pas le 28 ; le fichier du 28/10 est donc encore à 120 s) : 171/171 marchés suivis passent `"closed":true`, de 136 à 723 s après la fin (médiane 305 s, p95 445 s).
FAIT aussi le 28/10 : engine suit le tick du marché (`follow_tick`) ; traceur redéployé (900 s, effectif le 29/10 02:11 UTC) ; convert-report et deploy/ vide supprimés.
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
Aucune. Fait nouveau possible : relancer `chocs.json` sur des traces courantes réglées (à partir du 29/10) quand plusieurs semaines seront rapatriées.
