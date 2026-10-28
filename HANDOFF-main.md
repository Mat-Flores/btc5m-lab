# HANDOFF — main

**Branche** : `main` (branche unique, pas de remote).

**Objectif** : exécuter `PROMPT-MIGRATION.md` (spécification, ne jamais la modifier) : migrer `btc5m-lag-sim` (lecture seule) vers ce dépôt, en phases, avec des subagents ; l'orchestrateur valide et commite.

## Fait
- Phase 1 validée par l'utilisateur (décisions D1-D9, E1-E2 : voir `docs/contrats-migration.md`).
- Commité et validé : `.claude/` + `CLAUDE.md` (7851cee), `deploy/` (f5a392c), `source-tracing/` + `convert/` (0202009).
- Commités par l'utilisateur : `engine/` partiel (commits « . » 0feb6a2, cfe0f41, NON validé), consigne HANDOFF dans `CLAUDE.md` (dad9e07).
- `docs/` validé (21 fichiers, citations et guillemets contrôlés à 0 échec) et commité.
- Rapatriement D1 (lecture seule) dans `C:\dev\REA Projects\updown5m-staging\` : dry-run 10009/10012 sha256 OK, live 886/888 + `/tmp` 2844/2844 ; seules les 2 sessions en cours d'écriture diffèrent (à recopier en 3a).
- Convertisseur validé sur une vraie session (conservation exacte des lignes par type).

## Reste à faire
1. Finir `engine/` (agent opus) : Engine/Strategy/Venue, port maker, adapters `binance-spot`/`binance-perp`/`coinbase`, config exhaustive ; valider (cargo test + diff `replay decisions` = vide).
2. (fait) corrections docs
3. Resserrer `source-tracing` test `example_config_is_valid_and_complete` une fois les 3 adapters au registre.
4. Vague 2b : `backtest/` (SimVenue, non-régression maker à 0 écart, E1, `config/default.json` E2), `ui/` via impeccable (activer `impeccable@impeccable` dans `.claude/settings.local.json`), les 8 README + README racine.
5. Phase 3 (3a conversion → STOP 3b → nettoyage → déploiement dry-run), phase 4, graphify, rapport final.

## Décisions et pièges
- Questions ouvertes à l'utilisateur : (a) reconstruire un historique « 1 commit par outil » (`git reset --soft ac1841a` puis commits par outil) à cause des commits « . » ; (b) `CLAUDE.md` ne sera plus identique à l'octet au source (consigne HANDOFF ajoutée) → la vérif phase 4 `diff CLAUDE.md` échouera : garder la version modifiée ?
- dublin-live : trader réel armé sur okx (3 ordres/fills « real » enregistrés, non confirmés on-chain) + wallet `/home/ubuntu/p10/wallet` : JAMAIS rien déployer, arrêt propre et accord nominatif avant nettoyage (D8).
- Gratuit uniquement (D3) : pas d'agrandissement EBS ; trace Polymarket ≈ 4 Go/jour → `pull-traces` quotidien.
- Rust uniquement dans WSL `Ubuntu-24.04` (`~/.cargo/bin/cargo`) ; `CARGO_TARGET_DIR=$HOME/...` évite la lenteur de `/mnt/c`.
- Les limites d'usage API coupent les subagents : les relancer par SendMessage (contexte conservé), pas en nouveaux agents.
- `.gitattributes` : `CLAUDE.md` et `.claude/**` en `-text` (pas de conversion CRLF).

## Fichiers clés
- `PROMPT-MIGRATION.md` : spécification qui fait autorité.
- `docs/contrats-migration.md` : contrats figés (trace, interfaces engine, API UI), décisions D1-D9, E1-E2, règles des subagents.
- `docs/conception-engine-migration.md` : options live/test, choix, référence de non-régression maker.
- `source-tracing/convert/MAPPING.md` : mapping ancien → nouveau format.
- `deploy/pull-traces`, `deploy/updown5m@.service` : rapatriement et service.

## Commandes utiles
- Tests engine : `wsl.exe -d Ubuntu-24.04 -- bash -c 'cd "/mnt/c/dev/REA Projects/updown5m-lab/engine" && CARGO_TARGET_DIR=$HOME/eng-target ~/.cargo/bin/cargo test --workspace'`
- Tests tracer : idem dans `source-tracing/` ; convertisseur : `node --test source-tracing/convert/test/`
- Oracle maker (≈22 min, lecture seule du source) : `node --max-old-space-size=8192 engine/maker-bt/validate.mjs --delay 0 --sim exact` lancé dans `btc5m-lag-sim` → 59695 ordres, ratio 1.044, 84.1 %, PnL 16581 $ / 17331 $.

## Prochaine action
Attendre le rapport de l'agent engine (relancé par SendMessage), puis valider `engine/` : cargo test --workspace + diff `replay decisions` vide (scripts `replay.sh` du scratchpad, référence replay-before-*.jsonl), puis commit et vague 2b.
