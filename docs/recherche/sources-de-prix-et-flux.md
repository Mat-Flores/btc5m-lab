# Sources de prix et flux

**Objet.** Ce que l'on sait des flux qui alimentent le moteur : canal `market` de Polymarket, Chainlink (RTDS, PolyBolt, Data Streams), OKX, Coinbase, Binance, Bitstamp, Hyperliquid ; cadences, âges mesurés, limites de connexion, seuils de silence, mesure de l'avance (lead-lag), et flux OKX « creux » de l'enregistrement. Les chiffres de marché (frais, tick, fenêtres) sont dans [contraintes-marche.md](contraintes-marche.md).

**Sources couvertes.** `engine/00-recherche.md` (§2.1, §4, §5.7), `engine/01-audit.md` (§3.3-3.4, §5, §7.1), `engine/04-flux.md`, `engine/05-okx.md`, `engine/09-endurance.md` (§2-3), `research_notes/…/axe_A_sources.md`, `research_notes/…/delais_sources.md` (§3-4, §7), `engine/maker-bt/EXPLORATION.md` (lead-lag), `engine/maker-bt/WALLETS.md` (§0 bis), `engine/JOURNAL.md` (mesures, décisions flux).

**Lecture des citations.** Chemins relatifs au dépôt source `btc5m-lag-sim` (lecture seule). `E/` = `engine/` ; `RN/` = `engine/research_notes/Engine réel Polymarket BTC 5m/` ; `MB/` = `engine/maker-bt/`. `fichier:N-M` désigne des lignes. `[à vérifier]` : non établi ou contradictoire.

**Renvois.** [ordres-et-appariement.md](ordres-et-appariement.md) · [donnees-publiques.md](donnees-publiques.md) · [simulateur-maker.md](../simulateur-maker.md) · [exploration-maker-b01-b10.md](exploration-maker-b01-b10.md) · [engine-mesures-phases.md](engine-mesures-phases.md) · [architecture-backtest.md](../architecture-backtest.md).

---

## 1. Vue d'ensemble : âges mesurés depuis Dublin

Mesures du nœud `dublin-live`, chrony ≈ 1 µs, écart NTP 0,7 ms (`E/01-audit.md:165`, `:201-202`). L'âge est le délai entre l'horodatage source et la réception.

| Source | Messages/s | Âge p50 / p99 / max (ms) | Écart p99 − p50 | Intervalle p50 / p99 / max (ms) | Source |
|---|---|---|---|---|---|
| Polymarket `market` (4 tokens, 1 connexion) | 253,1 | 11,5 / 276 / 588 | 265 | 0,4 / 24 / 14 125 | `E/01-audit.md:206` |
| Coinbase `ticker` | 4,9 | 52,8 / 151 / 265 | 99 | 94 / 1 106 / 1 681 | `E/01-audit.md:207` |
| Binance `aggTrade` | 12,3 | 106,8 / 153 / 317 | 46 | 0,2 / 1 537 / 6 572 | `E/01-audit.md:208` |
| Binance `bookTicker` (sans horodatage) | 186,1 | — | — | 0,2 / 109 / 1 271 | `E/01-audit.md:209` |
| OKX `bbo-tbt` BTC-USDT-SWAP | 35,5 | 117,9 / 119,7 / 174 | 1,8 | 11 / 170 / 700 | `E/01-audit.md:210` |
| OKX `bbo-tbt` BTC-USDT (spot) | 13,8 | 117,9 / 119,7 / 174 | 1,8 | 30 / 560 / 2 400 | `E/01-audit.md:211` |
| RTDS Chainlink | 0,97 | 1 411 / 2 196 / 2 814 | 785 | 959 / 2 018 / 8 696 | `E/01-audit.md:212` |

Lectures : OKX arrive avec 118 ms de retard très régulier (trajet Tokyo → Dublin) ; Coinbase arrive à 53 ms en médiane, soit ~65 ms plus tôt qu'OKX, avec la réserve que le `ts` d'OKX est l'heure du moteur et celui de Coinbase l'heure du trade (`E/01-audit.md:216-219`). Polymarket : 11,5 ms en médiane mais 276 ms au p99 (`E/01-audit.md:220-221`). Chainlink : « le flux de règlement n'est donc jamais un signal rapide » (`E/01-audit.md:144-145`).

RTT applicatifs (`rtt.mjs`, chaud p50 / p99) : CLOB 24,4 / 29,5 ms ; OKX 255,6 / 278,9 ms ; Coinbase 125,0 / 129,5 ms ; Binance 207,2 / 212,3 ms. Source : `E/01-audit.md:183-189`. Les RTT de Kalshi et Limitless mesurés (5-6 ms) viennent d'un cache CDN et « ne mesure pas le moteur » (`E/01-audit.md:190-191`).

## 2. Polymarket, canal `market` (carnet)

### 2.1 Ce que dit la doc

- URL `wss://ws-subscriptions-clob.polymarket.com/ws/market` ; abonnement `{"type":"market","assets_ids":[…]}` ; options `initial_dump`, `level` (1/2/3, **sens non documenté**), `custom_feature_enabled` (active `best_bid_ask`, `new_market`, `market_resolved`). Source : `RN/axe_A_sources.md:18-19`.
- Messages : `book` (snapshot agrégé, `timestamp`, `hash`), `price_change` (deltas, `size` = nouvelle taille agrégée, 0 = niveau supprimé), `last_trade_price`, `tick_size_change`. Horodatage `timestamp` en ms ; **aucun numéro de séquence**. Source : `RN/axe_A_sources.md:20-21`, `:14`.
- Heartbeat : `PING` texte toutes les 10 s. Source : `RN/axe_A_sources.md:22`.
- Aucune limite d'abonnement, de débit ou de taille indiquée ; le plafond de 100 jetons par abonnement a été supprimé le 2025-05-28. Source : `RN/axe_A_sources.md:23`.
- Aucun flux « pro », gRPC ou FIX documenté ; seule voie « institutionnelle » : la colocation eu-west-2 après KYC/KYB. Source : `RN/axe_A_sources.md:14`, `:30-31`.
- PolyBolt expose un canal public `price.polymarket` (BBO par jeton, `seq` dense et compteur `dropped`) : contrôle de cohérence possible, cadence non documentée. Source : `RN/axe_A_sources.md:28` ; `E/00-recherche.md:110`.
- Cadence et regroupement des messages : non documentés. Source : `RN/delais_sources.md:71`, `:106`.
- Mesure tierce (billet de blog, simple affirmation) : écart `timestamp` → réception p95 de 522 ms (1 connexion) à 175 ms (5) et 154 ms (10). Source : `RN/axe_A_sources.md:40` ; `E/00-recherche.md:109`.

### 2.2 Ce que les essais réels ont appris (phase 4)

| # | Observation mesurée | Correction | Source |
|---|---|---|---|
| 4 | Certains `book` n'ont pas de `tick_size` | Le pas connu est gardé, amorcé par les métadonnées de la fenêtre | `E/04-flux.md:33` |
| 5 | Un événement est découpé sur plusieurs trames de même horodatage, chacune annonçant les meilleurs prix de l'événement complet (204 faux écarts en 40 s) | Vérification d'un token à la fin de son événement ; 0 écart sur 57 018 vérifications | `E/04-flux.md:34` |
| 6 | Un côté vide est annoncé `0` (achat) et `1` (vente) | Paramètres du gestionnaire de carnets | `E/04-flux.md:35` |
| 7 | Le retard dépend du chemin de la connexion : 82 ms de médiane contre 9-10 ms au même instant ; jusqu'à 27-28 s de retard sur 2 connexions sur 3 en 30 min | 3 connexions par source, chacune avec ses carnets ; l'engine utilise la plus fraîche | `E/04-flux.md:36` |
| 8 | Une connexion lente qui déverse son arriéré retarde les autres | Recyclage d'une connexion dont un message a > 1 s de retard, seulement si une autre est fraîche (un retard commun vient de la source : 1,86 s mesuré sur les trois) | `E/04-flux.md:37` |
| 9 | Après un changement de pas, l'instantané spontané arrive en ~0,9 s | Resynchronisation forcée : 76 ms (p50) | `E/04-flux.md:38` |
| 10 | Après une coupure, des carnets restaient « utilisables » avec un contenu périmé | Remise à zéro dès qu'une connexion devient invalide | `E/04-flux.md:39` |
| 11 | L'âge d'un `book` mesure sa dernière modification, pas son envoi | Exclu des mesures d'âge | `E/04-flux.md:40` |

### 2.3 Mesures de l'engine (3 connexions)

| Mesure | Résultat | Source |
|---|---|---|
| Âge du carnet utilisé, run de 30 min | p50 10,4 ms ; p99 91,3 ms ; p99,9 239 ms ; max 1,88 s (blocage commun aux 3 connexions) | `E/04-flux.md:52` |
| Âge par connexion | n°1 : 83,6 / 643 ms ; n°2 : 11,9 / 182 ms ; n°3 : 49,3 / 238 ms | `E/04-flux.md:53` |
| Run papier de 18,7 h | Deltas (n = 41,5 M) : p50 10,5 / p90 21,5 / p99 93,8 / p99,9 269,6 / max 2 535 ms ; snapshots p50 11,6 / p99 98,4 ms | `E/09-endurance.md:27` |
| Temps sans connexion prête | 0,3 s sur 30 min | `E/04-flux.md:55` |
| Coupures | 18 en 30 min, toutes absorbées par les autres connexions, reconnexion en ~0,85 s ; en 18,7 h : 212 à 215 par connexion (≈ 1 par 5 min), invalidité p50 145 ms, p99 ≈ 1,1 s | `E/04-flux.md:56` ; `E/09-endurance.md:54` |
| Une coupure par ~10 min dans les premières mesures, précédée de 14-15 s de silence | Détecter vite compte plus que se reconnecter vite | `E/01-audit.md:222-223` |
| Cassures de carnet | 1 669 en 18,7 h (1 248 changements de pas, 421 désaccords de meilleur prix), toutes réparées ; durée p50 22,9 / p99 268,7 / max 541 ms. Les 421 désaccords arrivent surtout par trois, au même instant : ils viennent des messages de Polymarket | `E/09-endurance.md:30`, `:63-64` |
| Carnet de la fenêtre suivante exploitable | ≈ 299 s avant l'ouverture (14/14 tokens) ; 223 fenêtres sur 225 sur 18,7 h | `E/04-flux.md:61` ; `E/09-endurance.md:28` |
| Bascules de connexion | 15 224 en 18,7 h (≈ 820 par heure), sans coût mesuré | `E/09-endurance.md:59` ; `E/04-flux.md:100-101` |
| Latence de l'engine (réception → état) | p50 22,4 µs ; p99 164,7 µs ; p99,9 711 µs ; max 39,7 ms (rafales au déblocage de la source) | `E/04-flux.md:64` |

Panne de gamma de 15:30 à 15:37:23 UTC le 2026-10-21 : détails en [contraintes-marche.md](contraintes-marche.md) §5.

## 3. Chainlink : RTDS, PolyBolt, Data Streams

- **Flux de règlement** : `btc-usd-twap-60s-streams` (TWAP 60 s) ; règle et historique des fenêtres TWAP en [contraintes-marche.md](contraintes-marche.md) §6.
- **RTDS** : `wss://ws-live-data.polymarket.com`, topics `crypto_prices` (Binance `btcusdt`) et `crypto_prices_chainlink` (`btc/usd`), public, `PING` toutes les 5 s. « Legacy » depuis le 2026-09-15 ; retrait des topics de prix « one month after the 0.11.0 release » sans date calendaire. Source : `RN/delais_sources.md:75-76` ; `RN/axe_A_sources.md:105-106`.
- **PolyBolt** : `wss://ws-live-v2.polymarket.com/ws`, canaux `price.crypto` (Chainlink par défaut depuis le 2026-10-02, « about one update per second per symbol »), `price.crypto.twap` (60 s), `price.polymarket` (public). Authentification obligatoire (identifiants CLOB L2 ou ticket `POST /api/ws-ticket`). Enveloppe `{"v":1,"channel","seq","ts","snapshot"?,"dropped"?,"payload"}`. Limites : 64 abonnements, 20 trames d'abonnement/s, 64 Ko par trame, 8 tentatives d'auth, ping serveur toutes les 25 s, code 4008 en cas de dépassement. Source : `RN/axe_A_sources.md:104-111`. Il n'existe pas d'équivalent PolyBolt au TWAP 30 s. Source : `RN/axe_A_sources.md:106`.
- **Pas d'adaptateur PolyBolt** : `price.crypto` et `price.crypto.twap` répondent `auth_required` sans identifiants ; l'enveloppe est vérifiée sur de vrais messages (`fixtures/raw/polybolt-*`). Chainlink reste lu par RTDS en attendant. Source : `E/JOURNAL.md:65` ; `E/04-flux.md:102-103`.
- **Mesures** : ~0,97 msg/s ; âge p50 1 411 / p99 2 196 / max 2 814 ms (phase 1) ; 1 441 / 2 225 ms sur 30 min ; 1 362 / 2 159 ms sur 18,7 h. Source : `E/01-audit.md:212` ; `E/04-flux.md:54` ; `E/09-endurance.md:32`. Le retard vient de la source et du relais, pas du portable (~60 ms d'aller simple). Source : `E/01-audit.md:143-145`.
- **Historique à l'abonnement** : RTDS envoie un historique (`payload.data[]`, sans `full_accuracy_value`) qui a fait échouer le décodage en boucle ; le sujet est désormais lu avant la charge utile. Source : `E/04-flux.md:32`.
- **Coupures** : coupure planifiée à xx:52:00 pile toutes les 2 h (7 coupures sur 11) ; 68 trous > 3 s, max 8,8 s, sous le seuil de silence de 10 s. Source : `E/09-endurance.md:57`, `:65`.
- **Silences dans les enregistrements** : > 5 s dans 605 fenêtres sur 2 090 (écart maximal p90 = 8 s), > 10 s dans 3. Source : `E/11-maker-polymarket.md:110`. TWAP recalculé figé jusqu'à 60 s dans 45 fenêtres. Source : `E/08-strategies.md:217`.
- **Data Streams direct** : WebSocket authentifié HMAC, rapports d'environ 1 s livrés « up to 1-2 seconds later », « sub-second » sans chiffre ; ≥ 150 $/mois par flux, aucun palier gratuit ; passerelle aux États-Unis (≈ +35 ms d'aller simple depuis Dublin) ; redistribution interdite ; 20 WS simultanés, 10 req/s REST par endpoint. **Écarté** sauf mesure montrant que PolyBolt retarde le TWAP. Source : `RN/axe_A_sources.md:115-118` ; `RN/delais_sources.md:83-85`, `:122` ; `E/00-recherche.md:112`, `:351`.
- **[à mesurer]** Cadence réelle du BTC/USD Chainlink sur PolyBolt et retard PolyBolt contre Data Streams direct : aucune mesure publique. Source : `RN/axe_A_sources.md:152` ; `RN/delais_sources.md:105`.

## 4. OKX

- **Emplacement** : « All latency-sensitive services will be migrated from Hong Kong to Tokyo » (annonce 2026-05-14, migration fin juillet 2026) ; fournisseur et région non précisés. La page Glassnode indiquant « AWS ap-east-1 (Hong Kong) » n'est probablement pas à jour. RTT Dublin → OKX 255,6 ms. Source : `RN/axe_A_sources.md:164-166` ; `E/01-audit.md:187`, `:198-199`.
- **Canaux publics (JSON)** : `bbo-tbt` (top 1, poussé toutes les 10 ms si changement, `ts` = heure du moteur de matching, sans restriction VIP) ; `books-l2-tbt` / `books50-l2-tbt` 10 ms réservés au VIP4 avec login (ancien guide : VIP5 ; VIP4 retenu d'après la page actuelle) ; `books` et `books5` 100 ms ; `tickers` 100 ms. Source : `RN/axe_A_sources.md:168-174` ; `E/00-recherche.md:113-114`, `:195-196`.
- **Séquence** : `bbo-tbt` porte un `seqId` mais **pas de `prevSeqId`** (réservé aux canaux `books*`) ; le `seqId` saute d'une trame à l'autre ; une perte n'est donc pas détectable, la source déclare `sequence: false`. Source : `E/01-audit.md:153-154` ; `E/05-okx.md:27-29` ; `RN/axe_A_sources.md:176`.
- **Limites** : 3 connexions/s par IP ; 480 subscribe/unsubscribe/login par heure et par connexion ; 30 connexions WS par canal et par sous-compte ; déconnexion après 30 s sans données (ping avant 30 s). Source : `RN/axe_A_sources.md:178` ; `RN/delais_sources.md:123` ; `E/05-okx.md:22-23`.
- **Choix de l'adaptateur** : `bbo-tbt` seul (« le meilleur compromis pour les faibles latences », décision de l'utilisateur) ; un instrument par ligne de configuration ; heartbeat `ping` toutes les 20 s. Source : `E/05-okx.md:14-23` ; `E/JOURNAL.md:61`.
- **SWAP ou spot** : sur la même connexion, mêmes âges (p50 126 ms, p99 129 ms) ; corrélation croisée maximale à décalage nul : aucun ne devance l'autre ; le SWAP publie 2,6 fois plus (34,2 contre 13,1 msg/s) et reste moins longtemps silencieux (p99 192 contre 640 ms). Défaut : `BTC-USDT-SWAP`. Source : `E/05-okx.md:42-46`, `:63-75` ; `E/JOURNAL.md:62`.
- **Mesures dans l'engine (30 min)** : SWAP âge p50 127,4 / p99 131,1 ms, silence max 1,46 s ; spot 118,4 / 128,4 ms, silence max 2,57 s ; 1 fermeture serveur chacun (~1,07 s). Sur 18,7 h : p50 126,3 / p99 139,9 ms (n = 41 359 changements du milieu) ; 9 coupures, aucune après 08:40. Source : `E/05-okx.md:44-49` ; `E/09-endurance.md:32`, `:56`.
- **Âge dépendant du chemin** : 9 ms d'écart médian entre deux connexions au même instant. Plusieurs connexions OKX : seulement après mesure et sans effet sur les performances générales (décision de l'utilisateur). Source : `E/05-okx.md:116-118` ; `E/JOURNAL.md:52`.
- **Seuil de silence 3 s** (deux fois le maximum du SWAP ; serré pour le spot) ; à recalibrer sur 24 h. Le run de 30 min utilisait 5 s sans aucun silence déclenché ; l'engine n'a signalé aucun silence OKX en 18,7 h. Source : `E/05-okx.md:79-82` ; `E/09-endurance.md:66-67`.
- **Brancher / retirer OKX** : une ligne de configuration, aucun changement du cœur (test `core_sources_name_no_venue`). Source : `E/05-okx.md:97-103`.

## 5. Coinbase, Binance, Bitstamp, autres

| Source | Fait | Source |
|---|---|---|
| Coinbase | Origine AWS us-east-1 (zone unique use1-az4) ; `ticker` à chaque match (regroupé en cascade) ; `ticker_batch` 5 s ; `market_trades` regroupés sur 250 ms ; `level2_batch` public par lots de 50 ms ; `level2`/`full` authentifiés. RTT eu-west-1 → us-east-1 ≈ 70 ms (agrégateur, méthode non publiée). | `RN/axe_A_sources.md:225-226`, `:271` ; `RN/delais_sources.md:97-98` ; `E/00-recherche.md:115` |
| Binance spot | `@trade`, `@aggTrade`, `@bookTicker` « real time » ; `@ticker` 1 000 ms ; `@depth` 100 ou 1 000 ms ; SBE `@depth` à 20 ms (clé Ed25519) ; origine AWS ap-northeast-1 (Tokyo) selon Glassnode ; limites : 5 messages entrants/s, 1 024 flux/connexion, 300 connexions / 5 min par IP. | `RN/delais_sources.md:94`, `:124` ; `RN/axe_A_sources.md:223-224` ; `E/00-recherche.md:116`, `:316` |
| Mesures à Dublin | Coinbase `ticker` p50 52,8 / p99 151 ms (RTT 125 ms) ; Binance `aggTrade` p50 106,8 / p99 153 ms (RTT 207 ms) ; `bookTicker` 186 msg/s. | `E/01-audit.md:188-189`, `:207-209` |
| Bitstamp, Kraken, Bybit | Présents dans les anciennes sessions enregistrées. Bitstamp (Francfort) suit Binance de 1 à 3 s et ne change pas de prix 91 % des secondes : « inutile ». | `E/01-audit.md:259-261` ; `MB/EXPLORATION.md:255` |
| Hyperliquid piloté par Binance | Signal juste (95 à 100 % des sauts suivis ~0,9 s plus tard) mais 1,7 à 3,8 pb de gain contre 9 pb de frais aller-retour ; aucun perpétuel BTC HIP-3 actif ; maker : 0 remplissage sur 21 signaux, contrôle au hasard perdant (−3,6 pb). **Écarté.** | `E/JOURNAL.md:24` |
| Papertrade (prix lu à l'exécution) | Écarté : prix lu au moment de l'exécution (relayeur, blocs HyperEVM de 1 à 2 s, file) ; décote des petits gains. | `E/JOURNAL.md:25` |

Choix de la source de prix (phase 0, décision laissée à l'utilisateur) : à découverte du prix égale, Coinbase arrive ~65 ms plus tôt à Dublin qu'OKX ; l'inconnue « quelle bourse découvre le prix en premier » n'était pas tranchée. Le règlement est un prix consensus Chainlink moyenné sur 60 s, ce qui borne la valeur de tout signal ultra-rapide. Source : `E/00-recherche.md:381-390`.

### Avance sur le carnet Polymarket (lead-lag, 26/10)

- Chaque source est placée à son heure d'arrivée à Dublin (Binance +113 ms, p95 de l'utilisateur ; Coinbase +40 ms, non vérifié ; OKX et Chainlink à leur réception enregistrée). Source : `MB/EXPLORATION.md:252`.
- Le carnet n'a intégré que 7 à 9 % d'un mouvement Binance ou Coinbase à son arrivée, environ 30 % 100 ms plus tard, la moitié vers 200 ms et plus de 90 % à 500 ms : **~150 à 200 ms d'avance réelle**. Source : `MB/EXPLORATION.md:253`.
- Ordre d'arrivée : Coinbase, de peu devant Binance perp, lui-même devant Binance spot. Source : `MB/EXPLORATION.md:254`.
- Chainlink n'est pas suivi à court terme par le carnet : source de règlement (TWAP 60 s), pas source d'avance. Source : `MB/EXPLORATION.md:256`.
- **[à vérifier]** Latences d'arrivée supposées (Binance +113 ms, Coinbase +40 ms, réception de Binance supposée à 120 ms dans `MB/WALLETS.md:154`) : mesurées seulement pour OKX.

## 6. Seuils de silence et santé des sources

| Source | Seuil | Justification | Source |
|---|---|---|---|
| Polymarket `market` | **3 s** | Calibré sur 4 sessions (~17 M messages) : p99,99 ≤ 405 ms, max hors coupure 2,8 s. Une cible de 2 s aurait sonné à tort plusieurs fois par session. 12 silences en 18,7 h, tous signalés entre 3 002 et 3 096 ms | `E/04-flux.md:41`, `:80` ; `E/09-endurance.md:29` |
| Chainlink | **10 s** | Max hors coupure 8,9 s ; 68 trous > 3 s (max 8,8 s) en 18,7 h. Un trou de 5 à 10 s passe encore : seuil « à revoir en phase 9 » | `E/04-flux.md:41` ; `E/09-endurance.md:65` ; `E/08-strategies.md:217-219` |
| OKX | **3 s** | Deux fois le maximum du SWAP (1,46 s) ; serré pour le spot (2,57 s) ; à recalibrer | `E/05-okx.md:79-82` |

Le tracer de la migration reprend ces seuils comme `Caps.silence` (OKX 3 s, Polymarket 3 s, Chainlink 10 s). Source : [contrats-migration.md:48](../migration/contrats-migration.md) (contrat de trace validé le 2026-10-27).

Principe : l'engine doit dater le **dernier message reçu** de chaque source, pas seulement le dernier changement de prix, sinon une coupure silencieuse ressemble à un marché calme. Source : `E/01-audit.md:268-270`.

## 7. Flux OKX « creux » de l'enregistrement

Synthèse, avec les deux séries de sources ; l'analyse complète est en [contraintes-marche.md](contraintes-marche.md) §7.

- **Constat de la recherche maker** : entre 25 et 1 330 prix OKX par fenêtre, 25 à 100 du 16/10 22:00 au 18/10, soit un prix toutes les 7 à 27 s ; la session du 17/10 donne 33 prix/min contre 938 pour celle du 15/10. Les trades Binance à la ms (option `spot`, σ×1,3) les ont remplacés pour la juste valeur. Source : `MB/WALLETS.md:146-155` ; `MB/EXPLORATION.md:246-247`.
- **Constat de l'engine** : le flux brut est dense (20,7 msg/s sur le SWAP, jamais plus de 0,82 s sans message ; 34,2 msg/s dans l'engine) ; les sessions n'écrivent un `px` que si le mid change ; en 30 s, 781 messages pour 6 changements du milieu. Source : `E/01-audit.md:155-157`, `:252-253` ; `E/05-okx.md:44` ; `E/JOURNAL.md:124`.
- **[à vérifier]** Un mid inchangé n'est pas périmé : la cause réelle de la rareté du 16-18/10 (filtrage au changement de mid contre vrais trous) n'est pas établie ; elle détermine si LT-REF est invalidé par un spot périmé ou par autre chose. Source : `MB/WALLETS.md:146-151` contre `E/09-endurance.md:66-67`, `E/11-maker-polymarket.md:120`.

## 8. Données de santé et sécurité des sources externes

- Les pages de latence tierces (Glassnode `llms.txt`) contiennent des consignes adressées aux agents d'IA (liste d'attente, lien de parrainage) : ignorées et signalées, la page n'a servi que de donnée. Source : `E/00-recherche.md:212`.
- Les chiffres de latence de blogs d'hébergeurs de VPS (Dublin « 0-1 ms ») sont de simples affirmations ; RTT mesuré Dublin → CLOB : 24 ms applicatif à chaud. Source : `RN/axe_B_ordres.md:234-237` ; `E/01-audit.md:193-195`.
- Le RTT Dublin ↔ Londres de ~12 ms est une hypothèse de travail (agrégateur public, méthode absente) ; « 1-2 ms » est tranché contre (460 km, ≥ 4,6 ms aller-retour en ligne droite). Source : `E/00-recherche.md:188`.

## 9. Points [à vérifier]

1. Cause de la rareté des `px` OKX du 16-18/10 (voir §7).
2. Latences d'arrivée Binance (120 / 113 ms) et Coinbase (+40 ms) non mesurées (`MB/WALLETS.md:154` ; `MB/EXPLORATION.md:252`).
3. Cadence réelle de `price.crypto.twap` (PolyBolt) et retard contre Data Streams (`RN/axe_A_sources.md:152`).
4. Quelle bourse découvre le prix en premier (OKX contre Coinbase) : non tranché en phase 0 (`E/00-recherche.md:388`) ; le lead-lag du 26/10 donne l'ordre Coinbase, Binance perp, Binance spot sous les réserves ci-dessus.
5. Regroupement des messages du WS `market` et sens de l'option `level` : non documentés (`RN/delais_sources.md:106` ; `RN/axe_A_sources.md:19`).
6. Mémoire de l'engine avec OKX : 44 Mo contre 16,6 Mo en phase 4, sans cause établie (`E/05-okx.md:122`).
