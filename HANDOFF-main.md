# HANDOFF — main

**Branche** : `main` (branche unique, pas de remote).

**Objectif** : exécuter `PROMPT-MIGRATION.md` (spécification, ne jamais la modifier ; les décisions utilisateur du 2026-10-28 ci-dessous la PRIMENT) : migrer `C:\dev\REA Projects\btc5m-lag-sim` (LECTURE SEULE) vers ce dépôt. L'orchestrateur délègue, valide lui-même (relance les vérifs) et fait tous les commits.

## Fait
- Phase 1 : contrats et décisions D1-D9, E1-E2 dans `docs/contrats-migration.md` (À LIRE EN PREMIER, « Règles communes » à recopier dans chaque prompt de subagent).
- Commités : `.claude/`+`CLAUDE.md`, `deploy/`, `source-tracing/`+`convert/`, `docs/`, `engine/` (285fe6a, 5e6197f : 131 tests, parité replay identique, `runtime` en config), test source-tracing durci (25c8791).
- 2026-10-28 : `trading-gap.service` arrêté + désactivé sur dublin-dry-run et dublin-live (seul service applicatif, aucun docker, aucun cron utilisateur).

## En cours
- Rapatriement FINI et vérifié (2026-10-28) : dry-run 10 012 fichiers, live 888 + /tmp + p9/p10 (24 fichiers, sans wallet) dans `updown5m-staging/dublin-live/home`. Les 2 nœuds sont NETTOYÉS (service, /opt, /var/lib/tgap, tgap, node, rust, p9/p10, /tmp) ; l'utilisateur les stoppe dans la console.
- Déploiement tracing commité : build WSL (`deploy/build-wsl.sh`) + envoi du binaire seul, garde disque 30 Go, tests 32 PASS. Jamais lancé en réel. Conversion des traces en cours (log `updown5m-staging/convert.log`).

## Reste à faire (dans l'ordre)
1. Finir la recopie + `/tmp` live ; puis l'UTILISATEUR arrête les 2 EC2 (stop, pas terminate) dans la console AWS (pas d'aws CLI ici ; `shutdown` via SSH risqué si le comportement d'arrêt = terminate).
2. Conversion de tout (staging + `data/` local du source) : `node source-tracing/convert/convert.mjs --out source-tracing/trace <entrées>` + vérif ; les traces vont DANS le projet (`source-tracing/trace/`, gitignoré, jamais commité).
3. source-tracing : supprimer l'idée d'UI (contrat 3 caduc) ; `source-tracing/COMMANDES.md` + 3 commandes interactives et visuelles (couleurs, barres de progression) : lancer le tracing sur le serveur en choisissant les sources ; désactiver des sources choisies ; rapatrier les dernières sessions. Cible : NOUVELLE instance « Up&Down-Tracing » à Dublin (créée par l'utilisateur), plus dublin-dry-run.
4. backtest/ : config globale JSON (`backtest/config/default.json`, E2) + UN fichier de config par stratégie en TOML (commentaires = description détaillée de la stratégie et de chaque paramètre) ; constantes de stratégie de l'engine (KEEP_NS, END/T_END/K_TAPE, WINDOW/FLOOR/PRIOR, HORIZON_S, capital0, claim) y passent, défauts inchangés. Signal par défaut binance-spot.
5. README (8 + racine), phase 4 vérifs, `graphify update .`, rapport final.

## Décisions et pièges
- 2026-10-28 utilisateur : historique git laissé tel quel ; garder SA version de `CLAUDE.md` (noter l'écart au rapport) ; signal backtest binance-spot ms σ×1,3 configurable, moyenne Binance+Coinbase = option ; OKX reste enregistré.
- 2026-10-28 utilisateur, PRIME la spec : couper entièrement les 2 instances (apps arrêtées, puis EC2 stop) ; wallets sans capital, aucun risque, ne pas s'en soucier ; plus d'UI ; plus de déploiement sur dublin-dry-run ; format config stratégie laissé à mon choix → TOML (ajout de la crate `toml`).
- Gratuit (D3) : trace Polymarket ≈ 4 Go/jour → `pull-traces` quotidien + garde disque.
- Rust seulement dans WSL `Ubuntu-24.04` (`~/.cargo/bin/cargo`, `CARGO_TARGET_DIR=$HOME/...`). Depuis Git Bash vers WSL : `MSYS_NO_PATHCONV=1` et passer par un script (les guillemets cassent).
- Le `.jsonl.gz` de `2026-10-16T01-17-20_dublin` (source) est tronqué.
- Max 6 subagents ; 1 relance par livrable refusé, au 2e échec STOP.

## Fichiers clés
- `docs/contrats-migration.md` ; `docs/architecture-backtest.md` (cible backtest) ; `source-tracing/convert/MAPPING.md` ; `deploy/pull-traces`, `deploy/updown5m@.service`.
- Instance tracing « Up&Down-Tracing » : `ubuntu@34.247.53.227`, clé `C:\Users\mathe\.ssh\Up&Down.pem` ; Ubuntu 26.04, 2 vCPU, 1,8 Gio RAM, disque 61 Go.
- SERVEURS : dry-run `ubuntu@108.130.16.251` (`~/.ssh/dublin-dry-run.pem`), live `ubuntu@3.253.60.148` (`~/.ssh/dublin-live.pem`) ; données dans `/var/lib/tgap/data` (sudo).

## Commandes utiles
- Tests engine : `wsl.exe -d Ubuntu-24.04 -- bash -c 'cd "/mnt/c/dev/REA Projects/updown5m-lab/engine" && CARGO_TARGET_DIR=$HOME/eng-target ~/.cargo/bin/cargo test --workspace'` (idem `source-tracing/`).
- Parité : `updown5m-staging/engine-baseline/replay.sh after` puis `cmp` avec `replay-before-*`.

## Prochaine action
Vérifier la sortie de la recopie (`todo.sha256` recopiés, `sha256sum -c` OK sur les 2 nœuds), recomparer `/tmp` de live, puis demander à l'utilisateur d'arrêter les 2 EC2 et lancer la conversion (étape 2).
