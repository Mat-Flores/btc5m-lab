# HANDOFF — copy-trading

**Branche** : `main` (chantier dans `copy-trading/` ; spec `copy-trading/SPEC.md`, immuable). Une autre session travaille en parallèle sur main (backtest/, docs/, engine/app/src/engine.rs) : ne jamais toucher ni commiter ses fichiers. RIEN n'est commité (accord requis).

**Objectif** : outil Rust autonome qui suit un wallet public Polymarket et recopie ses trades au plus vite, mises par paliers, paper/live, 3 comptes de PnL, latences par segment.

**Fait (étapes 1 à 4)**
- Détection : logs OrderFilled on-chain (1,77 s après le match, médiane, Dublin) + secours/contrôle data-api /activity. Ordre suivi daté par le match CLOB (websocket market) : 761 ms médiane jusqu'à l'observation.
- Code + 14 tests verts (copy-trading) ; tests engine verts (dont encrypt_v1 ↔ decrypt_v1). Aucune crate nouvelle.
- Hors copy-trading (accord) : `engine/core/src/key.rs` (+encrypt_v1), `engine/app/src/arm.rs` (arm/wallet_line déplacés), `engine/app/src/lib.rs`, `engine/app/src/live.rs` (ré-export).
- Paper 34 min (règle K=100, Dublin) : 88 fills, 88/88 concordants data-api (achats et ventes), 1 copié. Run barème E en cours/terminé dans `~/ct-paper/` (ubuntu) — voir sa sortie.
- 2026-10-29 : nouveau run `paper` démarré sur dublin-dry-run avec `paper_cash_usd=100` ; service actif, aucun run live. Plafond d'ordre conservé à 20 $. La config locale d'exemple est revenue à 20 $ : un prochain `start` la redéploiera avec cette valeur par défaut.
- `report` comprend aussi `COPIES · LUI vs MOI` : six copies les plus récentes, prix moyen réellement rempli, mises, écart, marge d'achat et agrégats ; `NULL` si non rempli, marges hautes compactées. 22 tests release verts, format du module vérifié, graphe actualisé. Binaire arm64 remplacé atomiquement sur dublin-dry-run avec SHA-256 vérifié, sans redémarrage ; rapport distant confirmé, paper actif à 100 $, aucun PID ni verrou live.
- deploy/{start,stop,report,lib.sh,setup.sh,build-wsl.sh,copy-trading.service,.env.example} + COMMANDES.md. Testés en réel sur dublin-dry-run : start paper, report, stop OK. Chemin live testé avec clé jetable (0 $ de solde, aucun ordre possible) : phrase par stdin OK, ARMED, verrou → halt au relancement, SIGTERM → stop + verrou retiré.

**Décisions utilisateur (2026-10-28)**
- Wallet suivi `0xf0318c32136c2db7fec88b84869aee6a1106c80c` (COPY_WALLET dans deploy/.env).
- Barème E (config `tiers`) : p < 0,001 % rien | ≥ 0,001 % 3 % | ≥ 0,1 % 5 % | ≥ 1 % 10 %, relevé au minimum d'ordre ; UNE entrée par token. Capital de test 20 $.
- Serveur : dublin-dry-run `ubuntu@54.76.142.4`, `~/.ssh/dublin-dry-run.pem`, aarch64. Cross arm64 installé dans WSL (rustup target + gcc-aarch64).

- 2026-10-29 soir, analyse du paper 100 $ (11 h 29, 473 trades) : flux on-chain muet de 12:09 à 16:27 alors que « Valid » (abonnement logs perdu par publicnode, en-têtes de blocs OK) → 145 trades rattrapés par data-api à ~24 s. Corrigé : `Cmd::Resync` du flux chaîne dès qu'un fill est rattrapé (≤ 1/min, ligne `feed` « resync ») + anti-doublon (`recovered`). Rapport corrigé : run courant seulement ; « copiés » = exécutés (vente sous le minimum → ignorés) ; glissement en % du capital ; « MARGE » → « GAIN SI GAGNÉ », pondéré par mes mises. 23 tests verts. NON déployé.

**Reste à faire**
1. Accord utilisateur pour redéployer (`bash copy-trading/deploy/start --mode paper`) : redémarre le paper (nouveau run, comptes à 100 $).
2. Après déploiement : vérifier dans le journal qu'aucune longue série `src: data-api` ne réapparaît, ou qu'elle est suivie d'une ligne `feed … resync`.
3. Accord utilisateur pour commiter (uniquement copy-trading/ + les 4 fichiers engine, `git commit -- <chemins>`).
4. Optionnel (accord requis) : signature neg-risk dans sign.rs ; nettoyer `~/ct-paper/` sur le serveur.

**Décisions et pièges**
- Réseau local : filtre TLS sur les RPC Polygon → tout test réel se fait sur Dublin.
- Horodatage de bloc Polygon postérieur à la réception du log → inutilisable pour dater l'ordre.
- Polymarket donne des identifiants API à toute clé (derive-api-key) : un live avec une clé quelconque s'arme ; seul le solde (0 $) empêche les ordres.
- sh non interactif donne /dev/null au process en arrière-plan : `exec 3<&0` puis `<&3` (deploy/start).
- systemd crée run.log en root : start le rend à copyt.
- Le wallet suivi est un teneur de marché : il achète au bid (ordres au repos remplis par morceaux, d'où les « renforts »), je copie en preneur au ask → 1 tick perdu par copie, structurel (observé +1,08 $ vs réel −0,95 $ sur le run du 29). Pas un bug.
- Ses marchés sont longs (élections 3 nov. 2026, NBA 2027) : aucune résolution avant plusieurs jours.

**Fichiers clés**
- `copy-trading/src/main.rs` : run/encrypt/report, boucle, ordres live. `src/{chain,book,sizing,pnl,report}.rs`.
- `copy-trading/deploy/start` : menu, wallet chiffré, déploiement, armement live, vérification.
- `copy-trading/COMMANDES.md` : mode d'emploi.

**Commandes utiles**
- Tests : WSL `CARGO_TARGET_DIR=$HOME/ct-target cargo test --release` dans copy-trading/ ; `bash copy-trading/deploy/{start,stop,report}`.

**Prochaine action** : demander l'accord de redéploiement du binaire corrigé (étape 1), puis l'accord de commit.
