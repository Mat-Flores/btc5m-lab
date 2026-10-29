# HANDOFF — copy-trading

**Branche** : `main` (chantier isolé dans `copy-trading/`, spec de référence : `copy-trading/SPEC.md`, ne pas la modifier).

**Objectif** : outil Rust autonome qui suit un wallet public Polymarket et recopie ses trades au plus vite, mises proportionnelles, modes paper/live, 3 comptes de PnL, latences par segment.

**Fait**
- Étape 1 : code réutilisable lu, sources mesurées (10 min, 2026-10-28 ~21h30), plan livré. EN ATTENTE DE L'ACCORD de l'utilisateur.
- Wallet à suivre (donné par l'utilisateur) : `0xf0318c32136c2db7fec88b84869aee6a1106c80c`, à rendre modifiable depuis `deploy/.env`.

**Mesures (étape 1)** — horloge Windows corrigée de +1,50 s (w32tm vs time.windows.com)
- Logs OrderFilled on-chain (WS publicnode, topic0 `0xd543adfd…`) : arrivent avant RTDS dans 96 % des cas, 580 ms d'avance en médiane (p10 973 ms, p90 155 ms ; n = 12 509 tx). Couvrent le wallet cible 28/28 (topic maker).
- RTDS `activity/trades` : 257 ms en médiane après son propre `timestamp` ; wallet cible vu seulement 14/28 fois (un seul côté par tx).
- data-api `/activity?user=` (sondage 1 s) : 19 s de retard en médiane (p90 32 s) → source de secours. `/trades` global : ~280 s, inutilisable.
- Timestamp de bloc : résolution 1 s, parfois après notre réception → segment « ordre suivi → observation » peu fiable sous la seconde.

**Reste à faire**
1. Accord sur le plan + réponses aux questions ouvertes (taille p minuscule, arm() dans main.rs, deps directes, .env).
2. Étape 2 : code + tests. Vérifier d'abord la disposition des données d'OrderFilled V2 (non décodée : 2 échecs TLS du RPC depuis Windows).
3. Étape 3 : paper ≥ 30 min. 4. Étape 4 : deploy + COMMANDES.md → stop.

**Décisions et pièges**
- curl Windows (schannel) échoue en TLS → utiliser node fetch ou WSL.
- `engine/app/src/live.rs` est un module du binaire (main.rs), pas de la lib : `arm`/`wallet_line` non importables.
- Le flux RTDS peut passer par `engine_core::feed::run_with(trace=true)` (trames brutes), sans toucher engine/.

**Fichiers clés**
- `copy-trading/SPEC.md` : spécification, immuable.
- Scripts de mesure : dans le scratchpad de session (non versionnés) : measure.mjs, analyze.mjs.

**Commandes utiles**
- cargo uniquement dans WSL Ubuntu-24.04, `CARGO_TARGET_DIR=$HOME/ct-target`.

**Prochaine action** : attendre l'accord de l'utilisateur sur le plan d'étape 1 (voir la conversation), puis commencer l'étape 2 par le décodage d'un log OrderFilled V2 du wallet cible.
