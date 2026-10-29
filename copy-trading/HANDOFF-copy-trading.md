# HANDOFF — copy-trading

**Branche** : `main` (chantier dans `copy-trading/` ; spec `copy-trading/SPEC.md`, immuable). Une autre session travaille en parallèle sur main (backtest/, docs/, engine/app/src/engine.rs) : ne jamais toucher ni commiter ses fichiers. RIEN n'est commité (accord requis).

**Objectif** : outil Rust autonome qui suit un wallet public Polymarket et recopie ses trades au plus vite, mises par paliers, paper/live, 3 comptes de PnL, latences par segment.

**Fait (étapes 1 à 4)**
- Détection : logs OrderFilled on-chain (1,77 s après le match, médiane, Dublin) + secours/contrôle data-api /activity. Ordre suivi daté par le match CLOB (websocket market) : 761 ms médiane jusqu'à l'observation.
- Code + 14 tests verts (copy-trading) ; tests engine verts (dont encrypt_v1 ↔ decrypt_v1). Aucune crate nouvelle.
- Hors copy-trading (accord) : `engine/core/src/key.rs` (+encrypt_v1), `engine/app/src/arm.rs` (arm/wallet_line déplacés), `engine/app/src/lib.rs`, `engine/app/src/live.rs` (ré-export).
- Paper 34 min (règle K=100, Dublin) : 88 fills, 88/88 concordants data-api (achats et ventes), 1 copié. Run barème E en cours/terminé dans `~/ct-paper/` (ubuntu) — voir sa sortie.
- 2026-10-29 : nouveau run `paper` démarré sur dublin-dry-run avec `paper_cash_usd=100` ; service actif, aucun run live. Plafond d'ordre conservé à 20 $. La config locale d'exemple est revenue à 20 $ : un prochain `start` la redéploiera avec cette valeur par défaut.
- `report` adopte la nouvelle maquette dashboard : cadre fixe, lignes TRADES/SOURCE/ordres, tendances PnL, distributions de latence, état paper/live et valeurs absentes en `NULL`. 20 tests release verts, dont alignement, ANSI et journal hostile. Binaire arm64 remplacé atomiquement sur dublin-dry-run avec SHA-256 vérifié, sans redémarrage ; rapport distant confirmé, paper actif, capital configuré à 100 $, aucun PID ni verrou live.
- deploy/{start,stop,report,lib.sh,setup.sh,build-wsl.sh,copy-trading.service,.env.example} + COMMANDES.md. Testés en réel sur dublin-dry-run : start paper, report, stop OK. Chemin live testé avec clé jetable (0 $ de solde, aucun ordre possible) : phrase par stdin OK, ARMED, verrou → halt au relancement, SIGTERM → stop + verrou retiré.
- Graphify non actualisé lors de cette mise à jour : commande `graphify` absente de WSL (`command not found`).

**Décisions utilisateur (2026-10-28)**
- Wallet suivi `0xf0318c32136c2db7fec88b84869aee6a1106c80c` (COPY_WALLET dans deploy/.env).
- Barème E (config `tiers`) : p < 0,001 % rien | ≥ 0,001 % 3 % | ≥ 0,1 % 5 % | ≥ 1 % 10 %, relevé au minimum d'ordre ; UNE entrée par token. Capital de test 20 $.
- Serveur : dublin-dry-run `ubuntu@54.76.142.4`, `~/.ssh/dublin-dry-run.pem`, aarch64. Cross arm64 installé dans WSL (rustup target + gcc-aarch64).

**Reste à faire**
1. À la prochaine demande de bilan, livrer `bash copy-trading/deploy/report` ; ce rapport couvre tout le journal du serveur.
2. Livrer aussi la sortie du paper barème E (`ssh … 'cd ~/ct-paper && ./copy-trading report journal.jsonl'`).
3. Accord utilisateur pour commiter (uniquement copy-trading/ + les 4 fichiers engine, `git commit -- <chemins>`).
4. Optionnel (accord requis) : signature neg-risk dans sign.rs (marchés neg-risk ignorés en live) ; nettoyer `~/ct-paper/` sur le serveur.

**Décisions et pièges**
- Réseau local : filtre TLS sur les RPC Polygon → tout test réel se fait sur Dublin.
- Horodatage de bloc Polygon postérieur à la réception du log → inutilisable pour dater l'ordre.
- Polymarket donne des identifiants API à toute clé (derive-api-key) : un live avec une clé quelconque s'arme ; seul le solde (0 $) empêche les ordres.
- sh non interactif donne /dev/null au process en arrière-plan : `exec 3<&0` puis `<&3` (deploy/start).
- systemd crée run.log en root : start le rend à copyt.

**Fichiers clés**
- `copy-trading/src/main.rs` : run/encrypt/report, boucle, ordres live. `src/{chain,book,sizing,pnl,report}.rs`.
- `copy-trading/deploy/start` : menu, wallet chiffré, déploiement, armement live, vérification.
- `copy-trading/COMMANDES.md` : mode d'emploi.

**Commandes utiles**
- Tests : WSL `CARGO_TARGET_DIR=$HOME/ct-target cargo test --release` dans copy-trading/ ; `bash copy-trading/deploy/{start,stop,report}`.

**Prochaine action** : lire la sortie du paper barème E (tâche de fond ou commande ci-dessus) et la livrer ; puis demander l'accord de commit.
