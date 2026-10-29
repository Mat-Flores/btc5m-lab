# HANDOFF — copy-trading

**Branche** : `main` (chantier dans `copy-trading/` ; spec `copy-trading/SPEC.md`, immuable). Une autre session travaille en parallèle sur main (backtest/, docs/, HANDOFF-main.md, .claude/) : ne jamais toucher ni commiter ses fichiers. Commits autorisés par vagues (2026-10-29), toujours `git commit -- <chemins>`.

**Objectif** : outil Rust autonome qui suit un wallet public Polymarket et recopie ses actions au plus près (il est teneur de marché : je pose mes ordres à son prix), paper/live, 3 comptes de PnL, latences ; plus une UI web locale en lecture seule.

**Fait**
- Étapes 1 à 4 de la spec (détection chaîne + secours data-api, mises par paliers, paper/live, deploy/, COMMANDES.md).
- Vague 1 (bddbacf) : resync du flux chaîne quand data-api rattrape un fill (abonnement logs perdu 4 h 20 le 29), rapport terminal corrigé.
- Vague 2 (c3fd113) : stratégies interchangeables `strategy.rs` (exec maker|taker, achat cumulative|first_fill, vente lots|all, `cancel_after_min`), `maker.rs` (mes ordres posés + file d'attente paper ; ses ordres déduits + stats ; `stat` « annulation à tort »), engine : `body_typed` (GTC) et `cancel_request`. 28 tests copy-trading + tests adapter-polymarket verts.
- Run de test maker en parallèle : `~/ct-maker` sur dublin-dry-run (PID lancé 2026-10-29 17:11 UTC, journal `~/ct-maker/journal.jsonl`), le service paper principal n'a PAS été relancé (consigne : ne pas relancer sans nécessité). Premiers résultats OK (pose, remplissage immédiat quand le carnet croise, file de 52 512 parts devant un autre ordre).
- UI : `PRODUCT.md`, `UI-INVENTAIRE.md`. 1re série de maquettes (`.impeccable/mocks/decision/`) REJETÉE par l'utilisateur (« trop simpliste », dispositions mauvaises). Consigne : garder l'identité Polymarket, fusionner lui/moi, un graphique qui image les décalages. Nouvelle maquette `.impeccable/mocks/fusion.html` (capture `.impeccable/review/fusion.png`) : tokens Polymarket relevés sur leur CSS (bleu #3262FF = moi, orange #FF9900 = lui, gris #0C0E13/#1C1F27/#2D3037, vert #42C772, rouge #E23939, Inter, prix en ¢, pastilles Oui/Non, icônes gamma).

**Reste à faire**
1. Validation de `fusion.html` par l'utilisateur, puis écrire le contrat de direction (`impeccable surface-brief write`) et construire l'UI : `copy-trading/ui/` = `serve.py` (stdlib, `ssh tail -F` du journal + proxys data-api/gamma/CLOB, SSE) + `index.html` (vanilla, sans dépendance).
2. Après l'UI : revue impeccable (finish reviewer), DESIGN.md (documenter), commit.
3. Vérifier dans `~/ct-maker/journal.jsonl` : lignes `check` `trade_side` (sens du preneur), remplissages `order fill`, puis arrêter ce run (`pkill -f maker.json`) quand l'utilisateur décide de basculer le service sur la stratégie maker (redéploiement = nouveau run).
4. Live maker non vérifié sur ordre réel (GTC, annulation, `size_matched`) : premier test réel uniquement avec accord.
5. Plus tard : analyse statistique de ses ordres (lignes `trade.his_order`, `stat`) pour rétro-ingénierie ; vérification des news par IA (idée utilisateur).

**Décisions et pièges**
- Ses ordres ouverts ne sont pas publics : on les déduit de ses fills ; ses annulations sont invisibles → annulation après `cancel_after_min` (360) sans fill de lui, et trace des annulations démenties.
- Paper maker : file devant moi = taille du niveau au moment de la pose ; les annulations devant moi ne me font pas avancer (prudent). Le sens `side` des trades publics est supposé « preneur » : contrôlé par les lignes `check` `trade_side`.
- Ventes `lots` : en gain, au moins un lot par vente qu'il pose ; en perte, sa fraction, tout dès qu'il a vendu la moitié ; un reste < minimum part avec le lot.
- Réseau local : RPC Polygon filtrés → tout test réel sur Dublin. sh non interactif : `exec 3<&0` (deploy/start). systemd crée run.log en root.
- PRODUCT.md est dans `copy-trading/` : lancer les commandes impeccable depuis `copy-trading/`.

**Fichiers clés**
- `src/main.rs` (boucle, `maker`, `post`, `cancel`, `on_public_trade`, `expire`, `poll_rests`), `src/strategy.rs`, `src/maker.rs`, `src/{chain,book,sizing,pnl,report}.rs`.
- `deploy/` (start/stop/report), `COMMANDES.md`, `config.example.json` (bloc `strategy`).
- `PRODUCT.md`, `UI-INVENTAIRE.md`, `.impeccable/mocks/decision/`.

**Commandes utiles**
- Tests : WSL `CARGO_TARGET_DIR=$HOME/ct-target cargo test --release` dans copy-trading/. Build arm64 : `deploy/build-wsl.sh`.
- Journal test : `ssh -i ~/.ssh/dublin-dry-run.pem ubuntu@54.76.142.4 'cat ~/ct-maker/journal.jsonl'`.
- Capture d'une maquette : Edge headless `--screenshot` (voir session du 29/10).

**Prochaine action** : recueillir la validation (ou les retouches) de `.impeccable/mocks/fusion.html`, puis construire `copy-trading/ui/` sur cette disposition (données réelles du journal via ssh tail -F).
