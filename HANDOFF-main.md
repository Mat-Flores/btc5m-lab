# HANDOFF — main

**Branche** : `main` (branche unique, pas de remote).

**Objectif** : exécuter `PROMPT-MIGRATION.md` (spécification qui fait autorité, ne jamais la modifier) : migrer `C:\dev\REA Projects\btc5m-lag-sim` (LECTURE SEULE) vers ce dépôt, phase par phase. L'orchestrateur (Opus) délègue à des subagents, valide lui-même chaque livrable en relançant sa vérification, et fait tous les commits.

## Fait
- Phase 1 validée par l'utilisateur : contrats figés et décisions D1-D9, E1-E2 dans `docs/contrats-migration.md` (À LIRE EN PREMIER, section « Règles communes » à recopier dans chaque prompt de subagent).
- Validés + commités par l'orchestrateur : `.claude/`+`CLAUDE.md` (7851cee), `deploy/` (f5a392c), `source-tracing/`+`convert/` (0202009), `docs/` 21 fichiers (ad23760).
- Rapatriement D1 en lecture seule dans `C:\dev\REA Projects\updown5m-staging\{dublin-dry-run,dublin-live}\` : sha256 OK sauf les 2 sessions en cours d'écriture (`2026-10-26T00-16-17_dry-run.*`, `2026-10-18T23-09-09_live.*`) à recopier en 3a. `/tmp` de live : 2844/2844 OK.

## En cours, NON validé : `engine/` (agent arrêté proprement, son contexte est perdu)
- Fait par l'agent (commité NON validé dans 4a25075 « wip engine », voir `git show --stat HEAD`) : `core/src/market.rs` (MarketSpec), `trait Now`, `feed.rs` Frame/Connected/Sent, `http_get`, `app/src/{lib,engine,registry}.rs`, `strategies/{reference,maker,mod}.rs`, `strategy/src/maker.rs` (port op par op de `sim.mjs`) + `strategy/tests/maker_js.rs` + `fixtures/maker/` (valeurs attendues générées par le JS), adapters `adapters/{binance,coinbase}` + `fixtures/raw/*`.
- Reste (étapes 8-9 de sa tâche, voir `docs/contrats-migration.md` Contrat 2 et E2) : `engine/config.example.json` exhaustif ; vérifier `binance-spot`/`binance-perp`/`coinbase` au registre ; `cargo test --workspace` vert ; parité `replay decisions` = diff vide ; grep plus de `btc-updown-5m`/`LEN_S` en dur hors tests/fixtures/défauts.
- Référence avant refactor (hors dépôt) : `C:\dev\REA Projects\updown5m-staging\engine-baseline\` (107 tests OK dans `test-before.txt`, `replay-before-*.jsonl`, `replay.sh <tag>` qui rejoue 3 sessions).

## Reste à faire (dans l'ordre)
1. Finir et valider `engine/` (relancer un subagent opus/high avec ce qui précède), puis commit.
2. Resserrer le test `source-tracing` `example_config_is_valid_and_complete` (il tolère l'absence des 3 nouveaux adapters).
3. Vague 2b en parallèle : `backtest/` (opus/high : SimVenue, boucle déterministe, non-régression maker 0 écart, E1, `config/default.json` E2) ; `ui/` (sonnet/high via skill `impeccable:impeccable` ; activer `"impeccable@impeccable": true` dans `.claude/settings.local.json`, déjà gitignoré ; s'il ne se charge pas : STOP et demander) ; les 8 README + README racine (sonnet/medium).
4. Phase 3 : 3a recopie des 2 sessions en cours + copie du `data/` local + conversion (`node source-tracing/convert/convert.mjs --out source-tracing/trace <entrées>`) + vérif sha256 → **STOP 3b obligatoire** (bilan + ce qui sera arrêté/supprimé sur chaque nœud, attendre l'accord) → 3c nettoyage → 3d déploiement dry-run.
5. Phase 4 (vérifications), `graphify .`, rapport final.

## Décisions et pièges
- Questions posées à l'utilisateur, SANS réponse : (a) reconstruire l'historique « 1 commit par outil » (ses commits « . » 0feb6a2, cfe0f41) par `git reset --soft ac1841a` puis commits par outil ; (b) `CLAUDE.md` modifié par l'utilisateur (consigne HANDOFF, dad9e07) ≠ source → la vérif phase 4 `diff CLAUDE.md` échouera : garder sa version et le noter ? Ne rien faire sans réponse.
- dublin-live (3.253.60.148) : trader réel armé sur okx + wallet `/home/ubuntu/p10/wallet`. NEVER rien y déployer ; nettoyage seulement après STOP 3b, arrêt propre (sell-only, positions à plat) et accord nominatif sur le wallet (D8). NEVER éteindre une instance EC2.
- Purge cron 14 j sur les 2 serveurs (1re le 2026-10-31 ~06:25 UTC) : déjà sauvegardé (D1), seules les sessions en cours restent à recopier.
- Gratuit uniquement (D3) : trace Polymarket ≈ 4 Go/jour sur un disque de 30 Go → `pull-traces` quotidien + garde disque du tracer.
- Rust seulement dans WSL `Ubuntu-24.04` (`~/.cargo/bin/cargo`), avec `CARGO_TARGET_DIR=$HOME/...` (lenteur de `/mnt/c`).
- Limites d'API : un subagent coupé se reprend par SendMessage dans la même session ; dans une nouvelle session, relancer un agent neuf avec l'état ci-dessus.
- `.gitattributes` : `CLAUDE.md` et `.claude/**` en `-text`. Max 6 subagents simultanés ; 1 relance par livrable refusé, au 2e échec STOP.

## Fichiers clés
- `PROMPT-MIGRATION.md` : spécification. `docs/contrats-migration.md` : contrats, décisions, règles subagents.
- `docs/conception-engine-migration.md` : options live/test, choix, référence de non-régression maker.
- `docs/architecture-backtest.md` : cible du backtest (SimVenue, config, E1/E2, non-régression).
- `source-tracing/convert/MAPPING.md` : ancien → nouveau format. `deploy/pull-traces`, `deploy/updown5m@.service`.

## Commandes utiles
- Tests engine : `wsl.exe -d Ubuntu-24.04 -- bash -c 'cd "/mnt/c/dev/REA Projects/updown5m-lab/engine" && CARGO_TARGET_DIR=$HOME/eng-target ~/.cargo/bin/cargo test --workspace'` (idem dans `source-tracing/`).
- Parité : `wsl.exe -d Ubuntu-24.04 -- bash "/mnt/c/dev/REA Projects/updown5m-staging/engine-baseline/replay.sh" after` puis `diff` avec `replay-before-*`.
- Oracle maker (≈22 min, dans `btc5m-lag-sim`, lecture seule) : `node --max-old-space-size=8192 engine/maker-bt/validate.mjs --delay 0 --sim exact` → 59695 ordres, ratio 1.044, exacts 0.841, PnL 16581 $ / 17331 $.

## Prochaine action
Lancer un subagent opus/high sur `engine/` : finir `config.example.json` et la vérification finale (section « En cours » ci-dessus), avec les règles communes de `docs/contrats-migration.md` ; puis valider soi-même (cargo test + diff replay) et commiter `engine/`.
