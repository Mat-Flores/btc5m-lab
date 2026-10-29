# SPEC — copy-trading

Spécification de l'outil `copy-trading/`. Ce fichier ne se modifie pas pendant le développement : l'avancement va dans `copy-trading/HANDOFF-copy-trading.md`.

```xml
<contexte>
Dépôt : C:\dev\REA Projects\updown5m-lab (branche main). Lis d'abord CLAUDE.md, puis copy-trading/HANDOFF-copy-trading.md s'il existe. Écris en français (réponses et .md).
Règles établies (à respecter) :
- Rust compilé UNIQUEMENT dans WSL Ubuntu-24.04 (~/.cargo/bin/cargo, CARGO_TARGET_DIR=$HOME/ct-target). Depuis Git Bash : MSYS_NO_PATHCONV=1, passer par un script.
- Infra gratuite si possible ; aucune dépendance nouvelle sans accord.
- Briques Polymarket existantes à RÉUTILISER, sans les dupliquer :
  engine/adapters/polymarket/src/{exec.rs (Clob, Creds, order_request), sign.rs (Signer, amounts), account.rs (read_balance, read_holdings), market.rs, user.rs},
  engine/core/src/key.rs (decrypt_v1), engine/app/src/live.rs (armement : fichier wallet chmod 600 « <adresse>:<payload v1 chiffré> », phrase secrète lue sur stdin, clé gardée en mémoire seulement, zeroize).
- Modèle de déploiement à copier : source-tracing/deploy/{start,stop,pull} + COMMANDES.md (scripts bash Git Bash lancés depuis PowerShell, menus interactifs, .env lu ligne à ligne et jamais exécuté, binaire compilé dans WSL et envoyé seul, sha256, systemd, vérification après action).
</contexte>

<tache>
Crée un outil autonome `copy-trading/` (workspace Rust avec dépendances par chemin vers engine/, comme source-tracing/). Il suit UN portefeuille public Polymarket et recopie ses trades le plus vite possible, avec des mises proportionnelles.

1. Config locale `copy-trading/config.json` (versionner seulement un config.example.json) : adresse publique du wallet suivi (proxy wallet 0x…), mode `paper` | `live`, chemin du fichier wallet chiffré, plafonds.
2. Détection la plus rapide : mesure les sources publiques disponibles (websocket RTDS des trades, data-api /activity, logs on-chain OrderFilled sur Polygon) et choisis celle qui a la meilleure latence mesurée. Garde une source de secours pour réconcilier les trades manqués. Note les mesures dans le rapport, avec la source de chaque chiffre.
3. Taille de mise. p = mise du wallet suivi / valeur totale de SON portefeuille (cash USDC + positions au prix du marché). m = ma mise / valeur totale de MON portefeuille, plafonnée par mon cash disponible :
   - 0 < p ≤ 5 % → m = 10 × p   (0,5 % → 5 % ; 1 % → 10 % ; 5 % → 50 %)
   - 5 % < p ≤ 10 % → m = 50 % + 2 × (p − 5 %)   (interpolation linéaire 50 → 60 %)
   - p > 10 % → m = min(100 %, 60 % + (p − 10 %))
   Ventes : s'il vend X % de sa position sur un token, je vends X % de la mienne. Sous le minimum d'ordre Polymarket → pas d'ordre, ligne de journal.
   Ajoute un test unitaire de cette fonction couvrant 0,5 / 1 / 5 / 7,5 / 10 / 15 / 60 %.
4. Prix. Le prix « demandé » est le meilleur prix du carnet au moment où le trade est observé. Le prix limite de mon ordre ne s'écarte jamais de plus de 10 % de ce prix instantané. S'il n'y a pas de liquidité dans cette borne, rien n'est exécuté.
5. Modes :
   - `paper` (défaut) : aucune exécution inventée. Le remplissage se calcule en remontant le VRAI carnet observé à l'instant où l'ordre serait parti. Pas de liquidité, pas de fill. Aucune estimation : chaque résultat vient d'une observation.
   - `live` : chemin d'ordre réel complet et testé jusqu'à la signature. L'utilisateur l'active en changeant le mode au start. Le code est le même que celui de paper, seul l'envoi diffère.
6. Trois comptes de PnL tenus en parallèle, dans les deux modes :
   - observé = mes tailles aux prix d'entrée et de sortie du wallet suivi ;
   - demandé = mes tailles aux prix demandés au moment de l'observation ;
   - réel = mes fills réels (live) ou mes fills sur carnet réel (paper).
   La résolution des marchés est comptée dans les trois comptes.
7. Latences, horodatées par trade : ordre du wallet suivi (horodatage de la source) → observation → décision → envoi → accusé du CLOB → fill. En paper, un segment qui n'a pas été mesuré s'affiche « non mesuré » ; il n'est jamais estimé.
8. Clé privée : jamais en clair sur disque, dans argv, dans l'environnement ni dans les logs. Réutilise le format v1 et decrypt_v1. Si aucune commande de chiffrement compatible n'existe, ajoute une sous-commande locale `encrypt`, testée par un aller-retour encrypt → decrypt_v1. La phrase secrète est demandée à CHAQUE start live (saisie masquée) et transmise au process par stdin via ssh. Après un crash ou un redémarrage en live, le bot ne repart PAS seul en live : il s'arrête et le journal le dit.
9. Commandes interactives dans `copy-trading/deploy/`, sur le modèle de source-tracing/deploy/. Elles visent une NOUVELLE instance définie dans `copy-trading/deploy/.env` (COPY_SSH_DEST, COPY_SSH_KEY ; refuser dublin-live et Up&Down-Tracing) :
   - `start` : choix paper/live, phrase secrète si live, récapitulatif, confirmation, déploiement, puis vérification que le bot suit bien le wallet ;
   - `stop` : arrêt propre puis vérification ;
   - `report` : synthèse TRÈS courte (≤ 15 lignes) des 3 comptes de PnL et des latences médiane/p90 par segment, en paper comme en live.
   Documente tout dans `copy-trading/COMMANDES.md`.
</tache>

<suivi_avancement>
Tiens à jour `copy-trading/HANDOFF-copy-trading.md` (ce fichier, et non HANDOFF-main.md, est le handoff de ce chantier).
- Crée-le dès le début, puis mets-le à jour après chaque étape significative (étape terminée, décision, bug résolu, changement de direction), sans attendre qu'on te le demande.
- Réécris-le au lieu d'ajouter à la suite : moins de ~60 lignes, il reflète l'état actuel et non l'historique.
- Structure obligatoire (celle de CLAUDE.md) : **Branche** ; **Objectif** ; **Fait** ; **Reste à faire** ; **Décisions et pièges** ; **Fichiers clés** ; **Commandes utiles** ; **Prochaine action**. La prochaine action doit être assez précise pour être exécutée sans autre contexte.
- N'y mets que ce qui n'est ni dans le code ni dans git log ; pas de longs extraits de code, seulement des références fichier:ligne.
- Au début de chaque session, lis-le et reprends à « Prochaine action ».
</suivi_avancement>

<arrets_obligatoires>
Arrête-toi et demande avant :
- d'ajouter une dépendance (crate ou outil) ;
- tout ssh ou toute action sur un serveur (je te donnerai l'IP de la nouvelle instance) ;
- tout ordre réel, même de test ;
- tout commit ;
- de modifier un fichier hors de copy-trading/ (sauf ajout d'un membre de workspace si c'est indispensable : demande d'abord).
Si une API Polymarket ne se comporte pas comme prévu, deux tentatives au plus, puis stop avec les preuves.
</arrets_obligatoires>

<etapes>
1. Lecture du code réutilisable + mesure des sources de détection → livre un plan court (fichiers, source choisie avec ses latences mesurées, format du journal) et ATTENDS mon accord.
2. Code + tests (cargo test dans WSL) → ✅ résultats des tests.
3. Paper en local (WSL) pendant au moins 30 min sur le wallet de config.example.json → ✅ sortie de `report`, avec les chiffres réels observés.
4. Scripts de déploiement + COMMANDES.md → stop : j'ouvre l'instance et je te donne l'IP.
Après chaque étape : mets à jour copy-trading/HANDOFF-copy-trading.md et donne une ligne « ✅ fait / preuve ».
</etapes>

<fini_quand>
- Tests verts, dont la fonction de mise et l'aller-retour de chiffrement.
- Un run paper réel produit un report avec les 3 comptes de PnL et les latences par segment, sans aucune valeur estimée.
- Le mode live s'active par le seul choix au start, plus la phrase secrète.
- copy-trading/HANDOFF-copy-trading.md est à jour.
- `graphify update .` a été lancé.
</fini_quand>
```
