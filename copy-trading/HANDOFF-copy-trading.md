# HANDOFF — copy-trading

**Branche** : `main` (chantier dans `copy-trading/` ; spec `copy-trading/SPEC.md`, immuable). Une autre session travaille en parallèle sur main (backtest/, docs/, HANDOFF-main.md, .claude/) : ne jamais toucher ni commiter ses fichiers. Commits autorisés par vagues (2026-10-29), toujours `git commit -- <chemins>`.

**Objectif** : outil Rust autonome qui suit un wallet public Polymarket et recopie ses actions au plus près (il est teneur de marché : je pose mes ordres à son prix), paper/live, 3 comptes de PnL, latences ; plus une UI web locale en lecture seule.

**Fait**
- Étapes 1 à 4 de la spec (détection chaîne + secours data-api, mises par paliers, paper/live, deploy/, COMMANDES.md).
- Vague 1 (bddbacf) : resync du flux chaîne quand data-api rattrape un fill ; rapport terminal corrigé.
- Vague 2 (c3fd113) : stratégies interchangeables `strategy.rs` + ordres posés `maker.rs` ; engine `body_typed` (GTC) et `cancel_request`.
- Vagues 3-4 (366f8a4, 93c7e39, a13ab3f) : inventaire `UI-INVENTAIRE.md`, `PRODUCT.md`, maquettes ; 1re série rejetée, `fusion.html` validée (identité Polymarket, décalages).
- Vague 5 (361b567) : UI `copy-trading/ui/` (serve.py stdlib, index.html, app.css, app.js, model.js + test node) ; ligne `start` du journal porte la stratégie (pris en compte au prochain déploiement). COMMANDES.md : sections UI et stratégies.
- UI vérifiée sur données réelles (journal taker du 29 et run maker en direct) par pilotage DevTools : survol → jumeaux allumés liste/graphique/cascade ; clic → sélection, carnet, « Suivre le direct ». 28 tests Rust + 2 tests modèle verts.
- Revue finale impeccable appliquée (01a633a, 3be03e9 : DNS rebinding, rendu, focus, carnet…) ; `DESIGN.md` + `.impeccable/design.json` écrits depuis l'UI livrée (North Star « L'écart comme forme »). UI terminée.
- 2026-10-30 (non commité) : `ANALYSE-2026-10-30.md` (profil du wallet, rejeu 1 000 $ sur 30 j, 10 bugs, goulots, choix de wallet) ; `buy: proportional` (+ `leverage`, `max_frac`), sortie au point mort s'il sort gagnant et nous perdant (`exit_price`), annulation des ordres posés à la résolution ; config.example à 1 000 $ / proportional ; scripts `analyse/` ; 30 tests verts. Recherche bot d'actualité dans `actu-bot/` (racine).
- Run de test maker `~/ct-maker` toujours en cours sur dublin-dry-run (lancé 2026-10-29 17:11 UTC) ; service paper principal NON relancé (consigne).

**Reste à faire**
0. Lire les journaux en cours (service + `~/ct-maker`) : bugs 3-6 de l'ANALYSE (annulation au coup d'envoi, sens preneur, dédoublonnage par tx, « déplacé ») ; puis corriger le bug 3 (gameStartTime Gamma) avant tout run sport.
1. Basculer le service sur la stratégie maker = redéploiement (nouveau run) : seulement sur accord ; puis arrêter `~/ct-maker` (`pkill -f maker.json`).
2. Lire dans `~/ct-maker/journal.jsonl` les `check` `trade_side` (sens du preneur) et les `order fill`.
3. Live maker non vérifié sur ordre réel (GTC, annulation, `size_matched`) : premier test réel uniquement avec accord.
4. Plus tard : analyse statistique de ses ordres (`trade.his_order`, `stat`) ; vérification des news par IA (idée utilisateur).

**Décisions et pièges**
- Ses ordres ouverts ne sont pas publics : déduits de ses fills ; annulation après `cancel_after_min` sans fill de lui, et trace des annulations démenties.
- Paper maker : file devant moi = taille du niveau à la pose ; les annulations devant moi ne me font pas avancer (prudent).
- UI : ne pas capturer avec `msedge --headless --timeout` (capture au chargement, requêtes coupées) ni `--virtual-time-budget` (bloqué par le SSE) ; piloter Edge par DevTools (script node, `--remote-debugging-port`). Le navigateur ne tient que 6 connexions par hôte (dont le SSE) : les titres de marchés partent groupés (`/api/markets`).
- Police Inter = identité Polymarket : exception au détecteur impeccable (`.impeccable/config.json`, ignoré par git car `config.json` est dans .gitignore).
- Réseau local : RPC Polygon filtrés → tests réels sur Dublin. PRODUCT.md est dans `copy-trading/` : lancer impeccable depuis `copy-trading/`.

**Fichiers clés**
- `src/main.rs` (boucle, `maker`, `post`, `cancel`, `on_public_trade`, `expire`, `poll_rests`), `src/strategy.rs`, `src/maker.rs`.
- `ui/serve.py` (ssh tail -F, SSE, proxys cache), `ui/model.js` (dérivation testée), `ui/app.js` (rendu, associations), `ui/app.css`.
- `deploy/`, `COMMANDES.md`, `config.example.json`, `.impeccable/surfaces/ui-index-html.md` (contrat de direction), `DESIGN.md` (système visuel de l'UI : à suivre pour tout nouvel écran).

**Commandes utiles**
- Tests : WSL `CARGO_TARGET_DIR=$HOME/ct-target cargo test --release` dans copy-trading/ ; `node --test copy-trading/ui/model.test.mjs`.
- UI : `python copy-trading/ui/serve.py` (service) ou `--journal /home/ubuntu/ct-maker/journal.jsonl --no-sudo --port 8767` (test), puis http://127.0.0.1:8765/.

**Prochaine action** : obtenir l'accès en lecture aux journaux (refusé le 30/10 : lecture de deploy/.env), les analyser contre ANALYSE-2026-10-30.md §5, puis demander l'accord pour commit et redéploiement en `proportional`. Ancienne action : lire `~/ct-maker/journal.jsonl` sur dublin-dry-run (lecture seule) : `check` `trade_side` et `order fill` ; en faire le bilan, puis demander l'accord pour basculer le service sur maker.
