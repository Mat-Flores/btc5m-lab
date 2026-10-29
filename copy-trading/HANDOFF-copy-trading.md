# HANDOFF — copy-trading

**Branche** : `main` (chantier dans `copy-trading/` ; spec de référence `copy-trading/SPEC.md`, immuable). Une autre session travaille en parallèle sur main (backtest/, docs/, engine/app/src/engine.rs) : ne jamais toucher ni commiter ses fichiers.

**Objectif** : outil Rust autonome qui suit un wallet public Polymarket et recopie ses trades au plus vite, mises proportionnelles, modes paper/live, 3 comptes de PnL, latences par segment.

**Fait**
- Étape 1 : sources mesurées. Logs OrderFilled on-chain = source principale (1,77 s médiane après le match, mesuré depuis Dublin ; avant RTDS dans 96 % des cas ; 28/28 trades du wallet). Secours : data-api /activity (19 s). Tx en attente via RPC testée : 1,43 s, gain ~0,5 s seulement, écartée.
- Étape 2 : code + tests ✅ — 14 tests copy-trading verts, tests engine tous verts (dont encrypt→decrypt_v1 qui refait le fixture à l'octet). Aucune crate nouvelle (lock = celui de source-tracing).
- Hors copy-trading (accord utilisateur) : `engine/core/src/key.rs` + `encrypt_v1` ; `engine/app/src/arm.rs` (arm/wallet_line déplacés depuis live.rs, ré-exportés) ; `engine/app/src/lib.rs` + `pub mod arm`.

**Décisions utilisateur (2026-10-28)**
- Wallet suivi `0xf0318c32136c2db7fec88b84869aee6a1106c80c`, modifiable via `COPY_WALLET` dans deploy/.env (start l'écrit dans config.json).
- Règle de mise remplacée : m = min(100 %, K × p), K = 100, capital de test 20 $. Ce wallet vaut ~5,45 M$ (2,63 M$ pUSD + 2,82 M$ positions) : il doit miser ≳ 5 450 $ pour 20 % de mes 20 $.
- Serveur de test : `dublin-dry-run` = `ubuntu@54.76.142.4`, clé `~/.ssh/dublin-dry-run.pem`, Ubuntu 24.04 **aarch64**, 2 vCPU, 1,8 Go, vide (aucun service). À traiter comme définitif.

**Reste à faire**
1. Étape 3 : paper ≥ 30 min en WSL → sortie de `report`.
2. Étape 4 : deploy/{start,stop,report} + COMMANDES.md. Le binaire doit être arm64 : WSL n'a ni target aarch64 ni gcc-aarch64 → demander l'accord (outil nouveau) ou compiler sur le serveur.
3. `graphify update .` à la fin.

**Décisions et pièges**
- Réseau local : filtre TLS intermittent sur les RPC Polygon (curl Windows/WSL, node) ; data-api et CLOB passent. Mesures on-chain fiables seulement depuis Dublin.
- Python urllib → 403 Cloudflare sans User-Agent.
- wsl.exe casse les `$var` passées en ligne : toujours passer par un script.
- /book : niveaux du pire au meilleur (on trie). Achat log : data[2] = USDC, data[3] = parts ; vente : inversé `[non vérifié réel, contrôlé par les lignes check]`.
- Live : neg-risk (échange 0xe2222d…) non signé par sign.rs → ordre ignoré en live (journal) ; étendre Signer demande accord.
- Live : verrou `<journal>.live-lock` ; présent au démarrage → halt journalisé, pas de reprise seule.

**Fichiers clés**
- `copy-trading/src/main.rs` : run/encrypt/report, boucle, ordres live, valeur du wallet suivi.
- `copy-trading/src/{chain,book,sizing,pnl,report}.rs` : détection, carnet, mise, 3 comptes, synthèse.
- `copy-trading/config.example.json` : config (K, 20 $, slippage 10 %, RPC).

**Commandes utiles**
- Tests : script WSL `CARGO_TARGET_DIR=$HOME/ct-target cargo test --release` dans copy-trading/ (et engine/ `--workspace`).

**Étape 3 (2026-10-28 22:38–23:13 UTC+1, sur dublin-dry-run car le réseau local bloque Polygon)** : 88 fills vus (tous par la chaîne), 88/88 concordants avec data-api (achats ET ventes : décodage vente vérifié), 1 copié (achat 122 493 $ du wallet, p = 2,24 % → 20 $ à 0,53, 37,74 parts sur carnet réel), puis cash à 0 → 85 « sous le minimum ». PnL obs −2,12 / dem −2,45 / réel −2,45 $. Observation → décision 34 ms médiane. Binaire arm64 + journal dans `~/ct-paper/` sur le serveur (process terminé).
- PROBLÈME : « ordre suivi → observation » = −1 038 ms médiane : l'horodatage de bloc Polygon est postérieur à notre réception (déjà vu à l'étape 1) → il ne date pas l'ordre. 25/88 trades sans bloc connu. Proposé : dater par le match CLOB (websocket market, `last_trade_price.timestamp` ms + transaction_hash, mesuré 9 ms après le match).

**Prochaine action** : réponse de l'utilisateur sur l'horodatage source (match CLOB via websocket market, ou garder le bloc), puis étape 4 : deploy/{start,stop,report} + COMMANDES.md (build : `deploy/build-wsl.sh`, cross arm64 installé).
