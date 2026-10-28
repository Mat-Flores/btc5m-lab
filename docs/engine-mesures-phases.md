# Mesures et critères de sortie des phases de l'engine

Ce fichier garde, phase par phase (3 à 10), les mesures, les critères de sortie et l'état de l'engine du dépôt source `btc5m-lag-sim`. La conception est dans [engine-conception.md](engine-conception.md) ; les décisions datées dans [journal-decisions.md](journal-decisions.md).

Convention : chemins relatifs au dépôt source `btc5m-lag-sim` (lecture seule), cités `fichier:ligne`. `[à vérifier]` marque ce qui n'est pas établi. Lieux de mesure : sauf mention, nœud `dublin-live` (t4g.small, ARM64, chrony à ~1 µs).

## 1. État des phases (au 2026-10-22, dernier état écrit)

| Phase | État | Source |
| --- | --- | --- |
| 0 Outillage et état de l'art | Validée le 2026-10-18 | `engine/JOURNAL.md:7` |
| 1 Audit et mesure de base | Validée le 2026-10-19 | `engine/JOURNAL.md:8` |
| 2 Conception | Validée le 2026-10-19 | `engine/JOURNAL.md:9` |
| 3 Registre et horloges | Terminée le 2026-10-19 | `engine/JOURNAL.md:10` |
| 4 Cœur des flux, Polymarket, Chainlink | Terminée le 2026-10-20 | `engine/JOURNAL.md:11` |
| 5 Adaptateur OKX | Terminée le 2026-10-20 | `engine/JOURNAL.md:12` |
| 6 Exécution | Terminée le 2026-10-20, aucun ordre réel | `engine/JOURNAL.md:13` |
| 7 Comptabilité, valeur en direct, défenses | Terminée le 2026-10-20, aucun ordre réel | `engine/JOURNAL.md:14` |
| 8 Stratégies | Livrable présenté le 2026-10-20, en attente de la décision de l'utilisateur | `engine/JOURNAL.md:15` |
| 9 Endurance sans argent | Run papier de 18,7 h analysé ; latence interne de l'engine « en attente » du rapport de fin | `engine/JOURNAL.md:16` ; `engine/09-endurance.md:31` |
| 10 Premier ordre réel | Mode réel assemblé et revu le 2026-10-21 ; le 2026-10-22, 3 ordres FAK réels, tous tués (§10) ; la case reste ouverte dans le journal (en attente du fichier du wallet et du go de l'utilisateur), l'état n'a pas été mis à jour après le 2026-10-22 | `engine/JOURNAL.md:17, :26` |

Critères d'acceptation globaux de la mission (tous encore des cases non cochées dans `TODO.md`) : quatre objectifs aux cibles ; brancher/retirer une source sans toucher au cœur ; décisions de la référence identiques à l'ancien moteur ; 24 h d'endurance sans incident inexpliqué ; revues adversariale et de sécurité faites ; réconciliation exacte sur l'essai réel ; délais des sources documentés et mesurés ; valeur du compte et stop loss suivis en direct (`TODO.md:635-649`).

Chaque phase se termine par la vérification de son critère de sortie, sur du code qui a tourné : « Rien n'est terminé sans avoir tourné » (`TODO.md:425, :605`).

## 2. Phase 1 : mesures de base (2026-10-18)

Mesures du portable (Windows 11, Node v24.20.0) puis du nœud live de Dublin, qui fait référence ; l'accès réseau du portable domine tout (TCP seul vers Cloudflare 110 à 126 ms ; horloge en retard de 790 ms sur NTP) et seules les durées purement locales sont transposables (`engine/01-audit.md:86-92`).

| Mesure | Résultat | Source |
| --- | --- | --- |
| Premier `createMarketOrder` d'un token (SDK Node) | 1 044 ms au portable (2 GET en série) ; 82 ms à Dublin | `engine/01-audit.md:101` ; `engine/JOURNAL.md:99, :107` |
| `createMarketOrder` suivants | p50 0,53 ms, p99 1,42 ms (portable) ; p50 1,16 ms, p99 3,29 ms (Dublin) | `engine/01-audit.md:102` ; `engine/JOURNAL.md:107` |
| Aller-retour HTTP chaud vers le CLOB | p50 159 ms (portable) ; p50 24,4 ms, p99 29,5 ms (Dublin) ; froid 32,9 ms | `engine/JOURNAL.md:101, :106` |
| Âge des sources à Dublin (p50) | OKX 118 ms, Coinbase 53 ms, Polymarket 11,5 ms, Chainlink 1 411 ms | `engine/JOURNAL.md:108` |
| Prototype du chemin chaud (Dublin) | Rust 160 µs au p50 contre 680 µs pour Node (3 runs × 20 000) | `engine/JOURNAL.md:109` |
| Coupures du canal Polymarket | 1 par ~10 min, 14 s de silence avant | `engine/JOURNAL.md:120` |
| Changements de pas | `tick_size_change` : 8 en 10 min (0,01 → 0,001) | `engine/JOURNAL.md:103` |

## 3. Phase 3 : registre et horloges

Critère de sortie : un crash au milieu d'une écriture, puis une relecture exacte (`TODO.md:497`). Vérifié le 2026-10-19 (`cargo test -j1 --release`, nœud de Dublin, Rust 1.99.1).

| Test | Résultat | Source |
| --- | --- | --- |
| `killed_mid_write_then_exact_reread` | 20 arrêts brutaux (SIGKILL), 109 844 faits relus exactement, seq consécutifs | `engine/03-registre.md:31` |
| `roundtrip` (propriété, 256 cas) | OK | `engine/03-registre.md:32` |
| `torn_tail_at_any_byte` (256 cas) | Coupé à n'importe quel octet, le registre rouvre avec exactement les lignes complètes | `engine/03-registre.md:33` |
| Dommage au milieu / trou de seq | Refusent l'ouverture | `engine/03-registre.md:34-35` |

Reste pour plus tard : le contrôle « horloge non synchronisée → aucune ouverture » est branché avec les garde-fous (phase 6) (`engine/03-registre.md:48-49`).

## 4. Phase 4 : cœur des flux, adaptateurs Polymarket et Chainlink

Run final de 30 minutes (3 connexions Polymarket + Chainlink), 2026-10-20 (`engine/04-flux.md:45-47`) :

| Mesure | Résultat | Source |
| --- | --- | --- |
| Âge du carnet utilisé par l'engine (deltas) | p50 10,4 ms ; p99 91,3 ms ; p99,9 239 ms (max 1,88 s, blocage de source commun aux 3 connexions) | `engine/04-flux.md:52` |
| Âge par connexion (p50 / p99) | n°1 83,6 / 643 ms ; n°2 11,9 / 182 ms ; n°3 49,3 / 238 ms | `engine/04-flux.md:53` |
| Âge de Chainlink | p50 1 441 ms ; p99 2 225 ms (celui de la source) | `engine/04-flux.md:54` |
| Temps sans aucune connexion prête | 0,3 s sur 30 min | `engine/04-flux.md:55` |
| Coupures Polymarket | 18, toutes absorbées par les autres connexions ; reconnexion en ~0,85 s | `engine/04-flux.md:56` |
| Réparation d'un carnet | p50 160 ms ; max 195 ms (26 cas) | `engine/04-flux.md:60` |
| Carnet de la fenêtre suivante exploitable | ≈ 299 s avant l'ouverture (14 tokens, 7 fenêtres) | `engine/04-flux.md:61` |
| Latence de l'engine (réception → état) | p50 22,4 µs ; p99 164,7 µs ; p99,9 711 µs ; max 39,7 ms | `engine/04-flux.md:64` |
| Ressources | 6,5 % d'un vCPU, 16,6 Mo, 3 fils | `engine/04-flux.md:65` |

Évolution des runs de 30 min : 1 connexion → âge p50 81 ms / p99 558 ms ; 3 connexions → 10,2 / 122,8 ms ; final avec recyclage conditionnel → 10,4 / 91,3 ms et 0,3 s sans connexion prête (`engine/04-flux.md:69-74`).

Apprentissages des essais réels (aucun ne se voyait avec les fixtures seules) : le fournisseur crypto de `rustls` manquait ; RTDS envoie un historique à l'abonnement ; certains `book` n'ont pas de `tick_size` ; un événement Polymarket est découpé sur plusieurs trames de même horodatage (204 faux écarts en 40 s, corrigé : 0 écart sur 57 018 vérifications) ; le retard dépend du chemin de la connexion (jusqu'à 27-28 s sur certaines) d'où les 3 connexions ; les carnets sont remis à zéro dès qu'une connexion devient invalide ; seuils de silence calibrés sur ~17 M messages : Polymarket 3 s, Chainlink 10 s (`engine/04-flux.md:28-41`).

Cibles révisées : coupure détectée en ≤ 3 s (au lieu de 2 s, qui aurait sonné à tort) ; latence de l'engine p99 ≤ 250 µs (au lieu de 20 µs, qui ne valait que pour le décodage seul) (`engine/04-flux.md:78-82`).

Critères de sortie : conformance (42 tests OK dont troncature à chaque octet et injection de fautes), test `core_sources_name_no_venue`, carnet exploitable ≈ 299 s avant l'ouverture, 100 % des écarts réparés (195 ms au pire), règlement officiel et `priceToBeat` lus (`engine/04-flux.md:86-93`).

Points ouverts : rafales au déblocage de la source (latence jusqu'à 40 ms) ; bascules de connexion fréquentes (186 en 30 min, sans coût mesuré) ; adaptateur PolyBolt à écrire dès que des identifiants existent (`engine/04-flux.md:97-103`).

## 5. Phase 5 : adaptateur OKX

| Mesure (30 min) | `okx-swap` | `okx-spot` | Source |
| --- | --- | --- | --- |
| Messages | 61 498 (34,2/s) | 23 593 (13,1/s) | `engine/05-okx.md:44` |
| Âge à la réception p50 / p99 | 127,4 / 131,1 ms | 118,4 / 128,4 ms | `engine/05-okx.md:45` |
| Intervalle entre trames p50 / p99 / max | 11,8 / 192 / 1 460 ms | 30 / 640 / 2 571 ms | `engine/05-okx.md:46` |
| Trames illisibles, trous | 0, 0 (non détectables) | 0, 0 | `engine/05-okx.md:47-48` |

- Sur `bbo-tbt`, le `seqId` est la séquence du carnet et saute d'une trame à l'autre : une perte n'est pas détectable sur ce canal ; la source déclare `sequence: false`, et un test vérifie qu'aucun faux trou n'apparaît (`engine/05-okx.md:25-34`).
- SWAP ou spot : le spot n'arrive pas plus tôt (même âge, p50 126 ms, sur la même connexion) ; aucun des deux ne bouge en premier (corrélation croisée maximale à décalage nul) ; le SWAP publie 2,6 fois plus souvent. Défaut `BTC-USDT-SWAP` (`engine/05-okx.md:61-75`).
- Seuil de silence 3 s (deux fois le maximum du SWAP) ; serré pour le spot ; ce seuil n'a pas tourné en réel pendant le run (5 s utilisé), à recalibrer sur 24 h (`engine/05-okx.md:77-82`).
- L'engine n'est pas dégradé par OKX : latence réception → état p99 190,6 µs ; réparation d'un carnet p50 20 ms (`engine/05-okx.md:55-58`).
- Brancher OKX = le commit `76c7035` : membre du workspace, dépendance, une branche dans `adapter()`, adaptateur et fixtures ; `core/` intact (`engine/05-okx.md:86-103`).
- Critères de sortie : 49 tests OK (dont 7 OKX) ; 0 trou sur 85 091 trames réelles ; latence p99 190,6 µs ≤ 250 (`engine/05-okx.md:109-112`).
- Points ouverts : âge OKX dépendant du chemin (9 ms d'écart entre deux connexions) ; mémoire 44 Mo contre 16,6 Mo en phase 4 sans cause établie ; doublons non filtrés sur `bbo-tbt` (`engine/05-okx.md:116-122`).

## 6. Phase 6 : exécution

Banc décision → dernier octet écrit (30 min, 2026-10-20, ordre toutes les 100 ms au plus, sur un puits local en TCP simple) :

| µs | p50 | p99 | p99,9 | max | n |
| --- | --- | --- | --- | --- | --- |
| Décision → dernier octet écrit | 222,7 | 288,4 | 584,6 | 2 384,7 | 12 328 |
| dont construction et signature | 170,2 | 222,7 | 384,9 | 2 269,5 | 12 328 |
| Premier ordre d'une fenêtre | 248,4 | — | — | 725,2 | 7 |

Source : `engine/06-execution.md:58-62`. Les 4 cibles (0,25 / 0,5 / 1 / 3 ms) sont tenues (`engine/06-execution.md:66-69`). Réserves : TLS non mesuré (~1,4 ko) ; « décision » = instant après la mise à jour de l'état ; type 3 plus long que le type 0 du prototype (142 → 170 µs au p50) (`engine/06-execution.md:74-80`).

Faits appris de l'étude du SDK et du compte : wallet de l'utilisateur = Deposit Wallet déployé, donc signature de type 3 (ERC-7739) seule ; arrondi des montants vers le haut à 4 décimales (pas 0,01) ou 5 (pas 0,001), vérifié sur 80 vecteurs (l'arrondi au plus proche en rate 12, vers le bas 36) ; un FAK tué revient en HTTP 400 « no orders found to match with FAK order… » (un seul cas observé, réponse 3,6 s après la décision) ; sans wallet déployé, le SDK déploie lui-même le Deposit Wallet, ce que l'engine ne fait jamais (`engine/06-execution.md:25-31`). Règles d'exécution : un ordre n'est écrit qu'une fois ; tout ce qui ne dit pas le sort de l'ordre le laisse inconnu (délai 300 ms, coupure, 5xx, corps illisible) ; « rien trouvé » ne vaut jamais « absent » (`engine/06-execution.md:36-45`).

Critères de sortie : 80 vecteurs reproduits octet pour octet ; machine d'états (8 tests dont une propriété sur suites aléatoires) ; faux CLOB (6 tests sur socket réel : succès, partiel, FAK tué, 4xx, 5xx, délai dépassé, coupure, réponse tardive) ; revue adversariale (8 constats corrigés) ; 85 tests OK (`engine/06-execution.md:90-99`).

Non établi avant le premier ordre réel : réponse acceptée du CLOB et canal `user` sur message réel, `GET /data/order`, `orderID` = hachage ; TLS sur le chemin d'ordre (`engine/06-execution.md:106-110`).

## 7. Phase 7 : comptabilité mesurée, valeur en direct, défenses

Coût de la valeur en direct sur le chemin chaud (banc : une position de 10 parts valorisée à chaque mise à jour de son carnet, 2 runs de 30 min) :

| µs | p50 | p99 | p99,9 | max | n |
| --- | --- | --- | --- | --- | --- |
| Valorisation + règle du stop loss (run 1 / run 2) | 0,6 / 0,6 | 2,1 / 2,2 | 16,9 / 18,3 | 505 / 5 855 | 1 153 808 / 916 599 |

Source : `engine/07-comptabilite.md:62-67`. Soit environ 1 % de la cible de latence de l'engine (`engine/07-comptabilite.md:72-75`). Sous `nice -n 10`, le maximum décision → octet écrit monte à 5,6 ms (signature seule 5,5 ms) : l'engine doit tourner avec une priorité au moins égale (`engine/07-comptabilite.md:76-78`).

| Vérification | Résultat | Source |
| --- | --- | --- |
| Réconciliation contre une adresse publique tierce (registre sans trade) | 180 désaccords sur 180, chacun alerté, avec le token et les deux tailles | `engine/07-comptabilite.md:82-86` |
| Redémarrage sur le registre existant | Registre du run 1 (804 faits) relu en entier, compte reconstruit, premier fait du run 2 au `seq` 804 | `engine/07-comptabilite.md:90-92` |
| Rejeu d'une chute de carnet réelle (session live du 2026-10-17) | 3 216 événements ; bid 0,58 → 0,30 en 1,5 s ; stop loss à l'événement 3 184 (6,33 s après l'entrée à 0,56), coût 5,77248, valeur 2,853, latent −2,91948 : identiques au calcul indépendant (BigInt) | `engine/07-comptabilite.md:106` |
| Traçabilité | Chaque position et chaque réalisé reconstruit à l'identique à partir de ses seuls `seq` | `engine/07-comptabilite.md:103` |
| Tests | 101 tests OK | `engine/07-comptabilite.md:108` |

Règles de comptabilité : une position naît d'un trade de Polymarket (jamais d'un ordre envoyé ou acquitté) ; valorisée dès `MATCHED` (`confirmed: false` jusqu'au `CONFIRMED`) ; sans carnet utilisable, aucune valeur ; le réalisé n'existe qu'à la clôture d'un marché (réclamation constatée) ; stop loss : valeur latente ≤ −50 % du coût, hors dernière minute, une fois par position ; réconciliation toutes les 10 s, accord seulement si parts, ordres ouverts et solde concordent exactement (`engine/07-comptabilite.md:31-49`).

Points ouverts : la réclamation (redeem) est lue chez Polymarket, pas exécutée ; solde et ordres ouverts codés mais non vérifiés sur réponse réelle ; frais de vente restent estimés ; la réconciliation compare toutes les positions de l'adresse (un wallet qui détient aussi des positions de l'ancienne application serait en désaccord permanent) ; retard de data-api après un trade à mesurer (`engine/07-comptabilite.md:119-126`).

## 8. Phase 8 : stratégies (rappel)

Le détail est dans `strategies-preneuses.md`. Chiffres du journal : parité de la référence contre le compte Réaliste, 22 sessions × 7 sources, 0 écart (A 414 ordres / 1 228 enregistrements, B 1 182 / 2 758, C 4 975 / 128 831) ; TWAP recalculé contre le règlement officiel : 416 fenêtres complètes, |erreur| p50 0,49 $, p99 7,12 $, max 16,11 $, 3 issues contredites sur 422 ; référence simulée +79,2 $ par position de 100 $ [IC95 +38,0 ; +120,5], n = 65 (`engine/JOURNAL.md:116-118`).

## 9. Phase 9 : endurance sans argent (18,7 h, du 2026-10-21 03:37:45 UTC)

Cadre : `dublin-live` à côté de l'ancienne application en dry-run, engine en `nice -10`, stratégie de référence en papier (capital 1 000 $, 100 $ par entrée, `minEdge` 0,1), appariement simulé = aller 12,2 ms + taker delay 150 ms ; CPU 10 % d'un cœur, mémoire résidente 10,5 Mo stable, capture ≈ 1,5 Go/jour (`engine/09-endurance.md:10-18`).

| Cible | Mesuré sur 18,7 h | Verdict | Source |
| --- | --- | --- | --- |
| Âge du carnet p50 ≤ 15 ms, p99 ≤ 150 ms | Deltas (n = 41,5 M) : p50 10,5 / p90 21,5 / p99 93,8 / p99,9 269,6 / max 2 535 ms | Atteinte | `engine/09-endurance.md:27` |
| Carnet exploitable avant l'ouverture | 223 fenêtres sur 225 (p50 299,4 s avant) ; 2 après (143 s, incident de gamma) | Expliqué, corrigé | `engine/09-endurance.md:28` |
| Coupure détectée en ≤ 3 s | 12 silences signalés entre 3 002 et 3 096 ms | Atteinte | `engine/09-endurance.md:29` |
| Carnets incohérents réparés | 1 669 cassures (1 248 changements de pas, 421 désaccords de meilleur prix), toutes réparées ; durée p50 22,9 / p99 268,7 / max 541 ms | Atteinte | `engine/09-endurance.md:30` |
| Latence de l'engine (p99 ≤ 250 µs) | Dans le rapport de fin de run seulement | En attente | `engine/09-endurance.md:31` |
| Âge des sources de prix | OKX p50 126,3 / p99 139,9 ms ; Chainlink p50 1 362 / p99 2 159 ms | Atteinte | `engine/09-endurance.md:32` |
| Réconciliation | Non exercée en papier ; cohérence interne : réclamations simulées 5 008,86 $ − coût 2 606,44 $ = 2 402,42 $ = PnL de la stratégie | — | `engine/09-endurance.md:34` |

Incident : la fenêtre de 15:35 UTC a été connue 143 s après son ouverture (les 3 connexions se sont tues ensemble 124 s) ; cause : gamma n'a rien servi entre ≈ 15:30 et 15:37:23 (l'ancienne application, sur le même nœud, a reçu la fenêtre suivante au même instant) ; correction : recherche de 3 fenêtres à l'avance, gamma publiant au moins une heure avant (`engine/09-endurance.md:38-48`).

Coupures attendues : Polymarket 212 à 215 par connexion (≈ 1 toutes les 5 min, invalide p50 145 ms) ; 514 recyclages ; OKX 9 (invalide p50 1,06 s) ; Chainlink 11 dont 7 à xx:52:00 toutes les 2 h (coupure planifiée côté serveur) (`engine/09-endurance.md:54-57`).

Stratégie en papier (simulé) : 45 décisions, 34 remplies, 11 manquées ; 28 gagnantes / 6 perdantes ; PnL +2 402,42 $ pour 2 519,85 $ engagés et 86,59 $ de frais ; creux maximal −104,9 $ ; +70,2 $ par position de 100 $ [IC95 +19,8 ; +120,7], médiane +72,9, n = 34 ; sans la plus grosse position (+836 $, un tiers du résultat) +48,5 $ [+20,6 ; +76,4] (`engine/09-endurance.md:71-77, :83-87`). La file d'attente n'est pas modélisée ; un preneur plus rapide aurait pu prendre ces niveaux (`engine/09-endurance.md:89-91`).

Verdict : endurance réussie sur 18,7 h, aucun crash ni perte de fait, mémoire stable ; critère « 24 h d'endurance sans incident inexpliqué » tenu sur 18,7 h par choix de l'utilisateur (`engine/09-endurance.md:95-102`).

## 10. Phase 10 : premiers ordres réels (2026-10-22)

Trois ordres FAK de 5 pUSD envoyés par l'engine, tous tués. Verdict : le délai naît chez Polymarket pendant la retenue des ordres preneurs, et dépasse nettement les 150 ms annoncés (`engine/10-delai-appariement.md:3-12`).

| Mesure | Résultat | Source |
| --- | --- | --- |
| Décision → dernier octet écrit | 0,21 à 0,23 ms (3 ordres réels) | `engine/10-delai-appariement.md:19` |
| Trajet + validation d'un `POST /order` (clé jetable, solde nul) | p50 32 ms, p90 42 ms, max 573 ms (n = 46) ; `GET /time` 25 ms | `engine/10-delai-appariement.md:30-31` |
| Réponse des 3 ordres réels | 868, 3 499 et 348 ms ; volume sous la limite resté +507, +428, +193 ms | `engine/10-delai-appariement.md:42-44` |
| Pas de lots à horloge fixe | 16 014 trades d'une heure répartis uniformément dans la seconde | `engine/10-delai-appariement.md:52-53` |
| Stratégie de référence rejouée avec délai d'appariement variable | 150 ms : +66 $ / position de 100 $ ; 450 ms : +41 ; 600 ms : +34 ; 900 ms : +34 ; 1 500 ms : +2 ; 3 500 ms : −20 | `engine/10-delai-appariement.md:92-97` |

Conclusions : en réel, 0 ordre sur 3 rempli, correspondant à un délai d'au moins 450 ms ; au-delà d'environ 1 s la stratégie preneuse ne gagne plus rien de mesurable (`engine/10-delai-appariement.md:99-100`). Rien côté client ne raccourcit la retenue ; une source de prix plus rapide (Coinbase 53 ms contre OKX 127 ms) et un nœud à Londres (~10 ms) ne jouent que sur la place dans la course ; la seule exécution qui échappe à la retenue est l'ordre non marketable (post-only) — d'où l'étude maker (`engine/10-delai-appariement.md:114-129`). Correctifs : attente de réponse portée de 300 ms à 5 s, délai du papier réglable (600 ms), solde écrit au registre seulement s'il change (321 lignes de bruit par heure supprimées), sonde de latence de l'ordre (`engine/10-delai-appariement.md:104-109`).

Cause de la retenue observée (> 150 ms) : non identifiée ; ordre de passage non strictement FIFO sur n = 3 `[non vérifié]` (`engine/10-delai-appariement.md:46-48`). Voir `contraintes-marche.md` pour le traitement de la retenue dans le simulateur.

## 11. Points `[à vérifier]`

- Latence interne de l'engine en fin de run (phase 9) : « en attente » dans la source ; non retrouvée dans les documents lus.
- Seuil de silence OKX de 3 s : jamais exercé en réel (`engine/05-okx.md:80-82`).
- Mémoire 44 Mo contre 16,6 Mo (phase 5) : cause non établie (`engine/05-okx.md:122`).
- Cause de la retenue réelle supérieure aux 150 ms annoncés (`engine/10-delai-appariement.md:8-12, :48`).
