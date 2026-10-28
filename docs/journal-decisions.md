# Journal des décisions et verdicts

Ce fichier regroupe les décisions datées et les verdicts (y compris les impasses) de la recherche menée dans `btc5m-lag-sim`, puis les décisions de la migration vers `updown5m-lab`. Il ne garde que ce qui engage la suite ; le tâtonnement est supprimé.

Convention de citation : tous les chemins sont relatifs au dépôt source `btc5m-lag-sim` (lecture seule), sous la forme `fichier:ligne`. `[à vérifier]` marque ce qui n'est pas établi. « Décision de l'utilisateur » = décision écrite dans le journal source ; « décision utilisateur du 2026-10-27 » = décision de la migration, consignée dans les contrats figés de la migration ([contrats-migration.md](contrats-migration.md), décisions D1 à D9 aux lignes 181-189, exigences E1 et E2 aux lignes 192-197) et dans `PROMPT-MIGRATION.md` du dépôt cible. La conception engine live/test de la migration et la référence de non-régression mesurée le 2026-10-27 sont dans [conception-engine-migration.md](conception-engine-migration.md).

Voir aussi : [engine-conception.md](engine-conception.md), [engine-mesures-phases.md](engine-mesures-phases.md), [securite-et-mode-reel.md](securite-et-mode-reel.md), [exploitation-deploiement.md](exploitation-deploiement.md).

## 1. Décisions de la migration (2026-10-27)

Toutes validées par l'utilisateur le 2026-10-27, sauf D3 qui a été modifiée par lui (rester gratuit).

| Id | Décision | Raison / portée |
| --- | --- | --- |
| D1 ([contrats-migration.md:181](contrats-migration.md)) | Rapatriement anticipé AUTORISÉ, en lecture seule, des sessions des 2 serveurs vers `C:\dev\REA Projects\updown5m-staging\<nœud>\`, sha256 vérifié ; les fichiers en cours d'écriture sont recopiés plus tard (étape « 3a ») | Les anciens serveurs purgent les sessions à 14 jours (`deploy/setup-ubuntu.sh:115-120`), la première purge étant attendue le 2026-10-31 (information de la session de migration, `[à vérifier]` dans le dépôt source). Voir [exploitation-deploiement.md](exploitation-deploiement.md) |
| D2 ([contrats-migration.md:182](contrats-migration.md)) | 3 nouveaux adapters `binance-spot`, `binance-perp`, `coinbase`, sans nouvelle crate | Logique de parsing reprise de `git show 7138fbc^:src/feeds/live.js` du dépôt source |
| D3 ([contrats-migration.md:183](contrats-migration.md)) | PAS d'agrandissement du disque EBS (solution gratuite) : `pull-traces` quotidien et garde disque dans le tracer (arrêt d'écriture si l'espace libre < 2 Go, reprise au-dessus de 3 Go) | Préférence de l'utilisateur : rester gratuit |
| D4 ([contrats-migration.md:184](contrats-migration.md)) | `sha2` déclarée en dépendance directe dans `source-tracing/` | Déjà présente dans `engine/Cargo.lock` |
| D5 ([contrats-migration.md:185](contrats-migration.md)) | Plusieurs hôtes le même jour (anciennes traces seulement) : `<AAAA-MM-JJ>.<host>.old.jsonl.gz` pour les hôtes secondaires | L'hôte qui a le plus de lignes garde le nom canonique |
| D6 ([contrats-migration.md:186](contrats-migration.md)) | SimVenue réaliste (fill livré à l'arrivée) ; non-régression bit à bit limitée aux configs sans TP/SL (x01, x04, validate) ; écart documenté | Voir `simulateur-maker.md` |
| D7 ([contrats-migration.md:187](contrats-migration.md)) | `.claude/skills/prompt-master/` copié SANS son `.git` ; `read-session` copié identique (inerte, documenté) | Voir [skills.md](skills.md) |
| D8 ([contrats-migration.md:188](contrats-migration.md)) | dublin-live : arrêt propre (sell-only, attendre les positions à plat) et confirmation nominative de l'utilisateur sur le wallet AVANT tout nettoyage (phase 3b) | Voir [securite-et-mode-reel.md](securite-et-mode-reel.md) §6 |
| D9 ([contrats-migration.md:189](contrats-migration.md)) | `data/wallets` NON copié | Secrets |
| E1 ([contrats-migration.md:192-196](contrats-migration.md)) | Au-delà du portage Rust du simulateur maker, essayer de corriger les décalages avec le réel (validate : volume ×1,044, PnL −4,3 %, 84,1 % d'ordres exacts ; arcsine : volume ×0,958, PnL +8,4 %, 71 %). Méthode : portage bit à bit d'abord ; chaque correction est ensuite une option NOUVELLE et paramétrée du modèle de l'exchange simulé (jamais de la logique d'une stratégie), de valeur par défaut reproduisant le comportement validé ; mesurée avant/après sur `validate` ; fondée sur une cause documentée ou mesurée ; fenêtres d'ajustement et de contrôle séparées ; résultat documenté dans `docs/` | Exigence ajoutée le 2026-10-27 |
| E2 ([contrats-migration.md:197](contrats-migration.md)) | UN fichier de config éditable qui stocke TOUS les paramètres du backtest : `backtest/config/default.json` (marché, sources, exchange simulé, capital, stratégies). Aucune constante cachée dans le code ; un fichier de lot ne contient que des surcharges (fusion profonde). Documenté dans `backtest/README.md` | Exigence ajoutée le 2026-10-27 |

Référence de non-régression du simulateur maker (tolérance zéro) : `validate.mjs --delay 0 --sim exact` donne 59 695 ordres, volume ×1,044, 84,1 % d'ordres exacts, PnL 16 581 $ prédit pour 17 331 $ réel ([contrats-migration.md:142-156](contrats-migration.md), décision utilisateur du 2026-10-27 ; chiffres aussi dans `engine/maker-bt/WALLETS.md:19`).

## 2. Décisions de l'ancien chantier d'interface (2026-10-15 à 2026-10-17)

| Date | Décision | Source |
| --- | --- | --- |
| 2026-10-15 | Identité visuelle A « Fidèle affinée », thème clair par défaut ; onglets Live, Stratégies (ajouté le 10-16), Backtest, Nœud et latence ; onglets Vue d'ensemble et Analyse supprimés | `PRODUCT.md:39-42, :71-72` ; `.impeccable/surfaces/ui-src-app-svelte.md:21-22` |
| 2026-10-16 | Un seul capital par source, commun à ses stratégies, premier signal premier servi ; désactivation d'une source possible hors session | `docs/sources-config-wallets.md:23, :8-10` |
| 2026-10-17 | Réel : connexion Magic.Link, signature pour le wallet proxy ; passage en réel source par source et à chaque session ; arrêt d'urgence qui bloque seulement les nouveaux ordres ; plafond par ordre de 100 $ (alors) ; jamais de retour automatique en papier ; seuil de perte fixé par l'utilisateur sinon le réel ne démarre pas | `docs/real-trading.md:8-16` |
| 2026-10-17 | Le contrôle géographique accepte les pays « Close-Only on Frontend » (IE, NL, JP, KR) car l'API n'y est pas restreinte | `docs/real-trading.md:69-73` ; `SPEC.md:327-333` |
| 2026-10-17 | Le collatéral des ordres est le pUSD (et non l'USDC.e) | `docs/sources-config-wallets.md:114-116` |

## 3. Décisions de l'engine Rust (2026-10-18 à 2026-10-22)

Source unique : `engine/JOURNAL.md` (tableau « Décisions », lignes 21 à 74). Les lignes sont citées ; les mesures associées sont dans [engine-mesures-phases.md](engine-mesures-phases.md).

### 2026-10-18 (phase 0 et 1)

| Décision | Pourquoi | Source |
| --- | --- | --- |
| Le dossier s'appelle `engine/` | Court, distinct de `src/` | `engine/JOURNAL.md:44` |
| Point d'arrêt après l'inventaire des skills ; installation de `dimensional-analysis` et `spec-to-code-compliance` (portée projet) ; désactivation de 5 plugins hors mission | Accord de l'utilisateur | `engine/JOURNAL.md:45-46` ; `engine/00-recherche.md:82-94` |
| Limitless ajouté à la recherche puis écarté entièrement | Délai de 250 ms et carnet quasi vide mesurés par l'API publique | `engine/JOURNAL.md:47, :51` |
| Le taker delay (et sa simulation) vit seulement dans l'adaptateur d'exécution Polymarket ; le cœur, la stratégie et les autres adaptateurs l'ignorent | Décision de l'utilisateur | `engine/JOURNAL.md:48` |
| Modèle d'un ordre preneur : appariement = aller + délai du marché ; le retour fixe seulement l'instant de connaissance | Remarque de l'utilisateur | `engine/JOURNAL.md:49` |
| Phase 0 validée | Validation explicite | `engine/JOURNAL.md:50` |
| Mesures de la phase 1 refaites depuis le nœud live de Dublin | Accord de l'utilisateur (trading à l'arrêt) | `engine/JOURNAL.md:72` |
| Plateforme et pays ouverts : comparer Polymarket, Kalshi, Limitless ; le choix du pays revient à l'utilisateur, la légalité locale demande un avis juridique | Décision de l'utilisateur | `engine/JOURNAL.md:73` |

### 2026-10-19 (phases 1 à 3)

| Décision | Pourquoi | Source |
| --- | --- | --- |
| Phase 1 validée ; Rust installé sur le nœud de Dublin (rustup, profil minimal) avec `gcc` | Feu vert de l'utilisateur | `engine/JOURNAL.md:70-71` |
| Phase 2 validée ; réponses aux questions : Rust (+ `cargo-deny`), 1 stratégie / 1 source / 1 wallet, mode papier oui, stop loss et plafonds paramétrables, seuil de session réalisé + latent, valorisation dès `MATCHED` | Réponses de l'utilisateur | `engine/JOURNAL.md:68-69` ; `engine/02-conception.md:327-339` |
| Pas de bot UI pour éviter le taker delay ; anticipation par limite sur le carnet (post-only) | Le délai est appliqué par le moteur quel que soit le canal | `engine/JOURNAL.md:74` |

### 2026-10-20 (phases 4 à 8)

| Décision | Pourquoi | Source |
| --- | --- | --- |
| OKX par `bbo-tbt` seul, `sequence: false` (pertes non détectables sur ce canal, déclaré) | « Meilleur compromis pour les faibles latences » ; `books` (100 ms) écarté | `engine/JOURNAL.md:61` ; `engine/05-okx.md:16-17, :25-29` |
| Instrument OKX par défaut `BTC-USDT-SWAP` | Même âge que le spot, aucune avance de l'un sur l'autre, 2,6 fois plus de messages | `engine/JOURNAL.md:62` ; `engine/05-okx.md:61-75` |
| Seuil de silence OKX 3 s | Maximum observé 1,46 s (SWAP) et 2,57 s (spot) ; à recalibrer sur 24 h | `engine/JOURNAL.md:63` ; `engine/05-okx.md:77-82` |
| Chainlink lu par RTDS en attendant des identifiants PolyBolt | `price.crypto` répond `auth_required` | `engine/JOURNAL.md:65` |
| 3 connexions Polymarket par défaut, chacune avec ses carnets ; l'engine utilise la plus fraîche | Le retard dépend du chemin (82 ms contre 9 ms au même instant) ; p99 de 558 à 91 ms | `engine/JOURNAL.md:66` ; `engine/04-flux.md:36` |
| Cibles révisées : silence Polymarket 3 s (au lieu de 2 s), latence de l'engine p99 ≤ 250 µs (au lieu de 20 µs) | Calibration sur 17 M messages | `engine/JOURNAL.md:67` ; `engine/04-flux.md:76-82` |
| Phase 4 terminée en autonomie | Demande de l'utilisateur | `engine/JOURNAL.md:64` |
| Signature de type 3 (Deposit Wallet, ERC-7739) seule | Le wallet de l'utilisateur est un Deposit Wallet déployé ; vérifié contre 80 vecteurs du SDK | `engine/JOURNAL.md:59` ; `engine/06-execution.md:25` |
| Un 5xx, un silence ou une coupure après l'écriture laissent l'ordre « inconnu » ; « non trouvé » ne vaut jamais « absent » | Règle 5 de `TODO.md` | `engine/JOURNAL.md:60` ; `TODO.md:40-41` |
| Frais d'entrée estimés depuis le barème, puis remplacés par `entry_fees_usdc` de data-api | Le CLOB ne donne qu'un taux par trade | `engine/JOURNAL.md:56` ; `engine/07-comptabilite.md:22` |
| Réclamation lue chez Polymarket (data-api `REDEEM`), pas exécutée ; l'écriture vers le relayer viendra avant la phase 10, sur accord | Décision de l'utilisateur | `engine/JOURNAL.md:55` |
| L'engine tourne avec une priorité au moins égale aux autres processus | Mesuré : sous `nice -n 10`, max décision → octet écrit 5,6 ms, contre 2,59 ms sans | `engine/JOURNAL.md:58` ; `engine/07-comptabilite.md:76-78` |
| Mesures par message en histogramme de taille fixe | La mesure ne doit pas peser (44 Mo et ~1 Go/jour en listes, ~6 Mo à plat) | `engine/JOURNAL.md:53` |
| Plusieurs connexions OKX seulement après mesure, et sans effet sur les performances générales | Décision de l'utilisateur | `engine/JOURNAL.md:52` |
| Phase 8 : parité de la référence vérifiée sur toutes les sources enregistrées sauf « fastest » ; rejeu dans un crate hors de l'espace de travail (`replay/`) | Choix de l'utilisateur ; lecture exacte des flottants pour la parité | `engine/JOURNAL.md:40-41` |
| C4 (maker) évalué entre deux bornes de remplissage | Les trades n'étaient pas enregistrés : la file maker ne pouvait pas être affirmée | `engine/JOURNAL.md:43` |

### 2026-10-21 (phase 9 et préparation du réel)

| Décision | Pourquoi | Source |
| --- | --- | --- |
| Phase 9 sur `dublin-live`, en papier (ordres signés avec une clé jetable, jamais envoyés), appariement simulé 12,2 ms + 150 ms | Demande de l'utilisateur ; aucun nœud Berlin n'existe | `engine/JOURNAL.md:34-35` |
| Phase 9 analysée sur 18,7 h au lieu de 24 h ; le run n'est pas arrêté | Demande de l'utilisateur ; l'engine n'intercepte aucun signal | `engine/JOURNAL.md:32` |
| Capture complète des flux au format des sessions de l'ancienne application | « Tout ce qui est exploitable pour un backtest » ; rejouée : décisions identiques au direct | `engine/JOURNAL.md:36` |
| Fenêtres cherchées 3 à l'avance (au lieu d'une) | Panne de gamma ≈ 15:30–15:37:23 UTC, fenêtre de 15:35 connue 143 s après son ouverture | `engine/JOURNAL.md:33` ; `engine/09-endurance.md:38-48` |
| Valorisations papier au plus une fois par seconde et par token | Mesuré : 73 451 en 8 min sinon (≈ 6 Go/jour) | `engine/JOURNAL.md:37` |
| Engine lancé en `nice -10` | Priorité de l'ancienne application | `engine/JOURNAL.md:38` |
| Mode réel : décisions de la référence, 5 pUSD, 1 ordre par run, achats seulement ; réconciliation limitée aux marchés de l'engine | Choix de l'utilisateur (nouvel engine, mise minimale) | `engine/JOURNAL.md:29` |
| Mode réel en continu : 5 pUSD tant que le dernier solde lu les couvre, arrêt à 10 h, perte de session 25 pUSD | Demande de l'utilisateur (remplace « 1 ordre par run ») | `engine/JOURNAL.md:28` |
| Identifiants d'API dérivés (`derive-api-key`), jamais créés | Le compte en a déjà | `engine/JOURNAL.md:30` |
| Blocage « pays » levé pour Polymarket : l'utilisateur est physiquement à Dublin (Irlande : API ouverte, restriction sur le site seulement) | Déclaration de l'utilisateur ; la légalité locale n'est pas étudiée | `engine/JOURNAL.md:31` |

### 2026-10-22 (verdicts)

| Verdict | Détail | Source |
| --- | --- | --- |
| Live coupé à 01:20 UTC | 3 ordres réels FAK de 5 pUSD, tous tués, aucun trade, solde intact (20,697166 pUSD). La retenue réelle de Polymarket rend la stratégie preneuse inopérante | `engine/JOURNAL.md:26` ; `engine/10-delai-appariement.md:3-12, :40-44` |
| Stratégie preneuse inopérante avec la retenue réelle | Réponse en 0,35 à 3,5 s, retenue d'au moins 0,19 à 0,58 s ; au-delà de ~1 s, plus rien de mesurable | `engine/10-delai-appariement.md:8-12, :99-100` |
| Maker (bids post-only des deux côtés) : aucune stratégie recommandée | 20 finalistes rejoués sur la semaine de carnets de `dublin-dry-run` ; aucun IC95 prudent hors échantillon n'exclut 0 ; annuler le côté perdant n'a aucun effet (perdant déjà exécuté dans 61 à 85 % des fenêtres). Périmètre réduit à la semaine enregistrée (décision de l'utilisateur) | `engine/JOURNAL.md:23` |
| Hyperliquid piloté par Binance écarté | Signal juste (95 à 100 % des sauts suivis ~0,9 s plus tard) mais 1,7 à 3,8 pb contre 9 pb de frais aller-retour ; aucun perpétuel BTC HIP-3 actif ; maker : 0 remplissage sur 21 signaux | `engine/JOURNAL.md:24` |
| Papertrade écarté | Prix lu à l'exécution (relayeur, blocs HyperEVM de 1 à 2 s, file) ; décote des petits gains | `engine/JOURNAL.md:25` |
| Nœud à Londres écarté | Royaume-Uni close-only sur le site et l'API ; Dublin est la région ouverte la plus proche | `engine/JOURNAL.md:27` |
| Kalshi non éligible pour un résident français | Pays de résidence déclaré au KYC, même physiquement à Dublin | `engine/JOURNAL.md:127` |
| Architecture d'intégration : le serveur Node pilote l'engine Rust ; garder seulement Polymarket, OKX, Chainlink ; purge de l'ancien moteur Node | Choix de l'utilisateur, passation du 2026-10-22 (remplacée par la migration vers `updown5m-lab`) | `docs/reprise-integration-engine.md:8-17` |

## 4. Verdicts de la recherche maker (2026-10-22 à 2026-10-26)

Le détail est dans `strategies-maker.md`, `exploration-maker-b01-b10.md`, `wallets-arcsine-certova.md` et `simulateur-maker.md`. Seuls les verdicts qui orientent la suite sont repris ici.

| Date | Verdict | Source |
| --- | --- | --- |
| 2026-10-25 | Le simulateur maker corrigé (borne `exact`, retenue 0, rebates) rejoue 29 579 ordres réels d'arcsine : PnL prédit 16 645 $ pour 15 360 $ réel (+8 %), volume ×0,96 | `engine/maker-bt/WALLETS.md:14-16` |
| 2026-10-25 | La retenue de 0,15 s appliquée aux poses fausse le simulateur : 7 943 $ prédits pour 16 383 $ réels (−52 %) | `engine/maker-bt/WALLETS.md:23` |
| 2026-10-25/26 | La stratégie d'arcsine ne se réplique pas (meilleur réplica −1,58 $/fenêtre contre +6,37 $ pour le wallet) ; CERTova ne peut pas être rejoué | `engine/maker-bt/WALLETS.md:26-29` |
| 2026-10-25/26 | Tous les résultats des lots b01 à b10 et des confirmations sont « à revoir » en borne `exact`, retenue nulle | `engine/maker-bt/EXPLORATION.md:243` |
| 2026-10-25/26 | Les candidats figés (LT-REF, S-E270-HI1…) deviennent tous négatifs avec un spot à jour (x02) ; en spot OKX et borne exacte (x01), espérance nulle | `engine/maker-bt/EXPLORATION.md:248` |
| 2026-10-26 | Le signe du résultat de la famille fin de fenêtre « dépend de la source de prix (négative en x02, positive en x04) : ce n'est pas encore un résultat robuste » | `engine/maker-bt/WALLETS.md:194` |

## 5. Constat du 2026-10-27 sur dublin-live

Un trader réel est armé sur dublin-live, source okx, dans la session `2026-10-18T23-09-09_live` (vérification de l'orchestrateur en lecture seule par SSH le 2026-10-27) : log « Session started: 2026-10-18T23-09-09_live, ordres réels sur okx » ; `records.jsonl` : 3 `order`, 3 `fill`, 3 `close`, 528 `skip` avec `"account":"real"`. Exécution réelle non confirmée on-chain (pas de hash de transaction) et lien avec les 3 ordres tués du 2026-10-22 non établi `[à vérifier]`. Détail, sources et statut de vérification : [securite-et-mode-reel.md](securite-et-mode-reel.md) §6. Ce constat a motivé la décision D8.

## 6. Impasses à ne pas rouvrir sans fait nouveau

| Impasse | Pourquoi | Source |
| --- | --- | --- |
| Stratégie preneuse sur signal OKX | Retenue réelle trop longue et trop variable | `engine/10-delai-appariement.md:99-100` |
| Rejoindre le meilleur bid en maker sur toute la fenêtre (lot x03) | Tous négatifs (−36 à −87 % sur la semaine) ; nos fills sont plus adverses que ceux du maker moyen | `engine/maker-bt/WALLETS.md:183`, `engine/maker-bt/EXPLORATION.md:249` |
| Hyperliquid piloté par Binance | Frais (9 pb) supérieurs au gain (1,7 à 3,8 pb) | `engine/JOURNAL.md:24` |
| Papertrade | Prix lu à l'exécution | `engine/JOURNAL.md:25` |
| Limitless, Kalshi, nœud de Londres | Voir §3 (2026-10-18 et 2026-10-22) | `engine/JOURNAL.md:27, :51, :127` |
| Annuler le côté perdant en fin de fenêtre | Perdant déjà exécuté dans 61 à 85 % des fenêtres | `engine/JOURNAL.md:23` |

## 7. Points `[à vérifier]`

- Date de la première purge des anciens serveurs (2026-10-31) : donnée de la session de migration ; le cron lui-même est bien dans `deploy/setup-ubuntu.sh:115-120` (`-mtime +14`).
- Légalité locale de l'opération depuis Dublin pour un résident français : non étudiée (`engine/JOURNAL.md:31`).
