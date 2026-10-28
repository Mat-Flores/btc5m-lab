# Données publiques Polymarket

**Objet.** Les sources de données publiques exploitables pour reconstituer le marché : Data API v2, gamma, événement `last_trade_price` du WebSocket, on-chain, jointure des fills aux carnets enregistrés, horodatage par bloc, comptes dominants, méthode de mesure de l'avance. Ce qui a été appris sur la microstructure en rejouant les carnets (apprentissages « Polymarket » de l'étude des wallets) est repris ici.

**Sources couvertes.** `engine/00-recherche.md` (§2.5), `research_notes/…/trades_publics.md`, `engine/maker-bt/WALLETS.md` (§0, apprentissages Polymarket), `engine/11-maker-polymarket.md` (annexe données).

**Lecture des citations.** Chemins relatifs au dépôt source `btc5m-lag-sim` (lecture seule). `E/` = `engine/` ; `RN/` = `engine/research_notes/Engine réel Polymarket BTC 5m/` ; `MB/` = `engine/maker-bt/`. `fichier:N-M` désigne des lignes. `[à vérifier]` : non établi ou contradictoire. Les adresses de wallets sont celles des sources (publiques on-chain), tronquées comme dans les notes.

**Renvois.** [contraintes-marche.md](contraintes-marche.md) · [sources-de-prix-et-flux.md](sources-de-prix-et-flux.md) · [wallets-arcsine-certova.md](wallets-arcsine-certova.md) · [simulateur-maker.md](simulateur-maker.md) · [strategies-maker.md](strategies-maker.md).

---

## 1. Les sources publiques

| Source | Contenu et limites | Source | Recommandation |
|---|---|---|---|
| **Data API v2 `/v2/trades`** (`https://data-api.polymarket.com/v2/…`, sans authentification) | Fills par marché (`condition`), événement ou wallet (`user`) : `proxy_wallet`, `side` (du point de vue du wallet), `token_id`, `condition_id`, `size`, `price`, `timestamp`, `slug`, `outcome`, `transaction_hash`. **Aucun champ maker/taker, aucun champ frais.** **Horodatage = heure du bloc Polygon, en secondes.** | `RN/trades_publics.md:17-19`, `:24` ; `E/00-recherche.md:176` | Utiliser |
| | Rôle maker/taker reconstitué par différence : `taker_only=true` (défaut) sert chaque fill une fois côté taker ; `taker_only=false` ajoute les lignes maker (6 marchés : 6 416 lignes taker, 13 589 lignes maker, 20 005 « all »). | `RN/trades_publics.md:21` | |
| | Pagination par curseur keyset `(block_timestamp, sequence_id)`, `limit` ≤ 1000 ; requêtes par `condition` sur une fenêtre fixe de 3 ans, par `user` avec `start`/`end`. Débit non publié : `429` + `Retry-After`, `Cache-Control: public, max-age=300`. | `RN/trades_publics.md:22-23`, `:72` | |
| | Volume d'une fenêtre de 5 min : 2 704 à 4 121 lignes « all », 858 à 1 284 lignes taker par marché. | `RN/trades_publics.md:25` | |
| | Écarts mesurés entre horodatages distincts sur 6 marchés : 1 s (471 fois), 2 s (470), 3 s (90) ; un hash ne porte qu'un horodatage. La doc est incohérente sur l'unité (le type TS annonce des ms). | `RN/trades_publics.md:20` ; `E/00-recherche.md:176` | |
| **Data API `/v2/activity`, `/positions`, `/holders`, `/user-stats`, `/user-pnl`, `/leaderboard`** | `activity` : flux d'un wallet (TRADE, SPLIT, MERGE, REDEEM) ; `holders?include_pnl=true` : top détenteurs (≤ 100 lignes, une condition) ; `user-stats` : décomposition du PnL (`realized_market_pnl`, `maker_rebate`, `fees`, `fees_paid`, `fees_refunded`, `trade_pnl`, `economic_pnl`, `volume_usdc`) ; `leaderboard` : `time_period`, `category` (ex. `crypto`), `sort_by` PNL/VOLUME, **non filtré par série de marchés** (pas de leaderboard « btc-updown-5m »). PnL multi-marchés. | `RN/trades_publics.md:29-32` ; `E/00-recherche.md:177` | Utiliser pour qualifier les comptes |
| **Data API `/v2/prices-history`** | Grain de 60 s conservé ≥ 7 jours, 300 s ≥ 60 jours, fenêtre explicite plafonnée à 15 jours : trop grossier pour une avance à la seconde. | `RN/trades_publics.md:35-36` | Non prioritaire |
| **Gamma `/events?slug=`** | Résout `btc-updown-5m-<start>` en `conditionId`, `clobTokenIds`, `outcomePrices`, `closedTime`, `volume`, `resolutionSource`, `feeSchedule` (rate 0,07, exponent 1, takerOnly, rebateRate 0,2). Exemple : volume 28 359 USD, `closedTime` 18:10:53 (≈ +53 s). `/markets?slug=` renvoie une liste vide pour un marché fermé. Règlement sur `btc-usd-twap-60s-streams`. | `RN/trades_publics.md:39-40` ; `E/00-recherche.md:178` | Utiliser pour indexer les marchés |
| **WS `market`, événement `last_trade_price`** | `market`, `asset_id`, `price`, `size`, `fee_rate_bps`, `side`, `timestamp` (13 chiffres : **ms**), `transaction_hash`. **Pas d'adresse de trader.** Temps réel seulement (pas d'historique). Seul moyen public de dater un fill à la ms puis de le rattacher à un wallet via `transaction_hash` → Data API. | `RN/trades_publics.md:44-46` ; `E/00-recherche.md:180` | **Ajouter à l'enregistrement** |
| **Endpoints REST CLOB publics de trades par wallet** | Aucun trouvé (`clob-openapi.yaml` non lu en entier). | `RN/trades_publics.md:49` | Sans objet (`[non vérifié]`) |
| **On-chain Polygon** | Les exchanges émettent `OrderFilled`, `OrdersMatched`, `FeeCharged` ; deux domaines EIP-712 (version « 2 » CTF, « 3 » Protocol V2). Temps de bloc moyen de Polygon PoS : 2 s → 1,75 s en mai 2026 (presse). Utile pour le numéro de bloc ou le `logIndex` (ordre intra-bloc). | `RN/trades_publics.md:53-55` ; `E/00-recherche.md:182` | Facultatif |
| **Goldsky** | Depuis le 2026-04-28 (contrats V2) les subgraphs publics renvoient des données « incomplete or incorrect » ; datasets v2 payants (`order_filled` avec `fee`, `order_hash`…). | `RN/trades_publics.md:58-60` | Ne pas utiliser |
| **Dune / Allium** | Tables `polymarket_polygon.market_trades`, etc. ; rafraîchissement ≈ 1 h ; aucun dashboard public dédié aux top traders 5/15 min. | `RN/trades_publics.md:63-65` ; `E/00-recherche.md:182` | Facultatif (agrégats multi-mois) |

**Limite pour le timing.** La Data API donne la seconde de bloc (≈ 1,75 s de granularité réelle plus le délai entre le match hors chaîne et l'inclusion) : suffisant pour une avance de plusieurs secondes, insuffisant pour quelques centaines de ms. Sous 2 s il faut la ms (carnet ou `last_trade_price`). Source : `RN/trades_publics.md:68`, `:178` ; `E/00-recherche.md:181`.

## 2. Ce que la jointure fills / carnets a appris (mesuré)

Étude des wallets du 2026-10-24/25, sur les carnets enregistrés (jeu B, 15/10 → 22/10 19:20 UTC).

1. **Les heures data-api sont des heures de bloc.** Elles retardent de **2,5 s** en médiane (1,7 à 3,1 s) sur le match visible dans le carnet. Mesure : fills on-chain d'arcsine comparés à la baisse exacte qu'ils produisent dans les deltas. Le tape public utilisé par les bornes « traversée » et « calibrée » était donc mal daté. Source : `MB/WALLETS.md:33-35`. (Une estimation antérieure, §4, plaçait le match 3 s plus tôt avec ±1 s de bruit : `MB/WALLETS.md:316`, section déclarée périmée en `:131`.)
2. **Les trades apparaissent dans les deltas `price_change`**, pas dans les snapshots, et les sessions de l'ancienne application ne contiennent aucun trade. `fills.mjs` retrouve **97 % des trades et 96 % du volume**, contre 8,6 % de coïncidences avec un décalage témoin de +10 s ; chaque ordre taker (fills complets `takerOnly=false`, avec les lignes maker au prix exact du niveau) est associé à la baisse exacte de son volume. Source : `MB/WALLETS.md:36-38`.
3. **Le matching est FIFO** (ordre des `OrderFilled` on-chain). Quand arcsine est rempli, il est le seul maker du niveau dans 42 % des cas ; quand il y en a plusieurs, il est le dernier servi dans 58 % des cas. Source : `MB/WALLETS.md:39`.
4. **Annulations et file.** Une annulation réduit la file devant nous au prorata. Sur 66 273 ordres réels : volume prédit 1,04 fois le réel au prorata, 0,94 si les annulations sont toutes derrière nous, 1,09 si toutes devant ; la réalité est entre le prorata et « toutes derrière ». Source : `MB/WALLETS.md:40`.
5. **Le calldata `matchOrders` (exchange V2 `0xe11118…`) contient les ordres signés** : taille d'origine, prix, et un `timestamp` de création en ms (présent chez la plupart des clients, 0 chez arcsine). Décodage : `orders-onchain.mjs`. Source : `MB/WALLETS.md:41`.
6. **Rebates maker** : 0,2 × 0,07 × p(1−p) × parts par fill, versés le lendemain à 00:45 UTC ; vérifié au cent près sur les versements quotidiens (254,60 $ pour 254,44 $ ; 477,78 $ pour 477,72 $). Source : `MB/WALLETS.md:42`.
7. **La retenue de 0,15 s ne vaut pas pour la pose d'un ordre** (délai pose → premier fill d'arcsine : 140 ms en médiane) ; pour les annulations, nos données ne permettent pas de trancher. Source : `MB/WALLETS.md:43`.
8. **Limites** : on ne voit que les fills, jamais les ordres posés ni les annulations ; la position dans la file et la vitesse d'annulation ne sont pas mesurables par les fills seuls. Source : `MB/WALLETS.md:317`.

### Données collectées par cette étude

| Données | Volume | Commande | Source |
|---|---|---|---|
| Fills complets par marché (`data/polymarket/fills/`) | 2 357 fenêtres | `fetch-fills.mjs` | `MB/WALLETS.md:137` |
| Ordres on-chain, tous makers (`data/polymarket/orders/`) | 35 fenêtres (1 sur 60) | `orders-onchain.mjs --every 60` | `MB/WALLETS.md:138` |
| Fills on-chain d'arcsine (`data/wallets/…orders.jsonl`) | 78 527 fills (15/10 → 24/10) | `wallet-orders.mjs` | `MB/WALLETS.md:139` |
| Calldata d'arcsine | 64 157 transactions maker (complet) | `wallet-calldata.mjs` | `MB/WALLETS.md:140` |
| Cache gamma de CERTova (avril → septembre) | partiel | `wallet-windows.mjs` | `MB/WALLETS.md:141` |

Annexe de l'étude maker du 22/10 : trades publics 8 632 fenêtres (22/09 20:00 → 22/10 19:20, sans trou, 1 356 trades par fenêtre au p50, 93 fenêtres tronquées à 3 000+ trades) ; BTC 1 s Binance (22/09 → 22/10) ; carnets de 15 sessions de `dublin-dry-run` (2 090 fenêtres, 296 000 événements de carnet par fenêtre au p50). Le `side` des trades publics est le côté du preneur ; par défaut une ligne par transaction, `takerOnly=false` ajoute les lignes des makers ; le prix d'une ligne preneuse est la moyenne de son balayage. Source : `E/11-maker-polymarket.md:131-137`. Ces dossiers `data/` sont hors git (`MB/WALLETS.md:133`).

## 3. Comptes dominants des marchés 5 min

Échantillon de 30 min, 6 fenêtres consécutives (`btc-updown-5m-1792345200` à `…1792346700`, 2026-10-18), mesuré par nous.

- Totaux : notionnel taker 81 674 USD (6 416 fills), notionnel maker 89 750 USD (13 589 lignes maker). Source : `RN/trades_publics.md:85`.
- Wallets les plus actifs (notionnel taker | maker en USD ; marchés sur 6) : `0xe29042…` « stepahead » 828 | 3 848 (6/6) ; `0x41e2e1…` « norm1e69 » 1 955 | 2 157 (6/6) ; `0x9d57c4…` « Polkadot-Frog » 497 | 3 513 (6/6) ; `0x21f4c1…` « CrouchPredictions » 265 | 2 818 (6/6) ; `0xc53375…` 1 232 | 752 (6/6) ; `0x18c18e…` maker seul 2 125 ; `0x56f609…` « arcsine-barrister » maker seul 1 499 (6/6) ; `0xee6568…` « 0x50f7 » maker seul 6 054 (3/6). Source : `RN/trades_publics.md:87-94`.
- Timing des fills taker dans la fenêtre (secondes après `start`, p10 / p50 / p90) : `0xc53375…` 11 / 62 / 173 ; `0x41e2e1…` 18 / 99 / 179 ; `0x9d57c4…` 50 / 152 / 200. Les trades commencent jusqu'à 280 s avant `start` et continuent jusqu'à 60 s après la fin : **filtrer sur `[start, start+300)`** pour toute analyse de timing. Source : `RN/trades_publics.md:95`, `:131`.
- Concentration sur le 5 min (1 000 derniers trades) : `0xe29042…` 1 000/1 000 ; `0xc53375…` 1 000/1 000 ; `0x9d57c4…` 893/1 000 ; `0x21f4c1…` 843 en 5 min et 157 en 15 min ; `0x41e2e1…` 751 / 133 / 60 SOL / 56 ETH. Source : `RN/trades_publics.md:103-108`.
- Classement hebdomadaire crypto du leaderboard : `0x41e2e1…` n°2 en volume (4,88 M parts), `0x21f4c1…` n°3, `0xe29042…` n°4, `0x9d57c4…` n°5, `0xc53375…` n°8 ; en PnL `0xc53375…` n°6 (+47 112), `0xe29042…` n°9, `0x56f609…` n°13 (+36 151). Le n°1 en volume (`0x06dc51…`) ne trade pas le 5 min. Source : `RN/trades_publics.md:99-100`.
- **Décomposition du PnL cumulé** (`/v2/user-stats`, tous marchés depuis l'inscription du wallet, pas seulement le 5 min) :

| Wallet | Trades | Volume (USDC) | `trade_pnl` avant frais | `fees_paid` | `maker_rebate` | `economic_pnl` |
|---|---|---|---|---|---|---|
| `0xc53375…` | 25 874 | 9,14 M | 325 984 | −215 954 | 7 557 | **151 545** |
| `0x41e2e1…` | 56 152 | 12,76 M | 307 036 | −252 993 | 0 | **69 495** |
| `0x21f4c1…` | 22 418 | 28,75 M | 135 199 | −147 529 | 44 370 | **54 351** |
| `0xe29042…` | 18 389 | 11,01 M | 62 116 | −52 586 (`fees_refunded` 330 708) | 32 987 | **42 520** |
| `0x9d57c4…` | 82 783 | 32,02 M | 190 338 | −307 417 | 59 228 | **29 419** |
| `0x56f609…` | 8 903 | 1,19 M | 27 813 | −5 941 | 365 | **22 237** |

Source : `RN/trades_publics.md:110-122`. Pour `0xe29042…`, `0x21f4c1…` et `0x9d57c4…`, le résultat net vient largement des rebates et des remboursements de frais : profil de **teneur de marché**, pas de preneur directionnel. Source : `RN/trades_publics.md:121`.

- Lecture : les comptes dominants en volume sont des makers multi-fenêtres ; les comptes plus taker (`0xc53375…`, `0x41e2e1…`) ont le meilleur `economic_pnl` et sont les premiers candidats à une mesure d'avance. Rien ici ne mesure leur latence. Source : `RN/trades_publics.md:130` ; `E/00-recherche.md:179`.
- Limites : échantillon de 30 min ; stabilité du classement non mesurée ; sens exact de `fees_refunded` non documenté ; `arcsine` utilise SPLIT, donc son PnL estimé est incomplet. Source : `RN/trades_publics.md:93`, `:96`, `:134-136`.
- Pour la stratégie d'arcsine et de CERTova (paramètres reconstruits, présence, extinction de l'edge), voir [wallets-arcsine-certova.md](wallets-arcsine-certova.md).

## 4. Analyses publiques : simples affirmations

Aucune analyse publique reproductible, wallet par wallet, des marchés 5/15 min n'a été trouvée ; les articles reprennent des posts X non sourcés et des blogs de vendeurs de bots. Source : `RN/trades_publics.md:141`.
- « Bot latence 15 min, 313 $ → 414 000 $ en un mois, 98 % de réussite » (captures de posts X, aucune adresse). Source : `RN/trades_publics.md:144`.
- Retard Polymarket contre Binance : « 30-90 s » (vendeur) ou « 12,3 s (2024) → 2,7 s (fin 2025) » : contradictoire, `[non vérifié]`. Source : `RN/trades_publics.md:145`.
- Étude Telonex (15 min, 02-08/02/2026, 2 688 marchés, 15,3 M fills, 46 945 portefeuilles) : 36,8 % de portefeuilles profitables ; makers +728 501 $ net, **avant frais et rebates** ; mesure partielle (une semaine, février 2026, avant le délai et le TWAP). Source : `RN/axe_C_strategies.md:152` ; `E/00-recherche.md:149`.
- Un spec tiers observe un règlement 60-120 s après `endDate` ; mesure interne : `closedTime` ≈ +53 s (un marché) : **[à vérifier]**. Source : `RN/trades_publics.md:147` ; `E/00-recherche.md:202`.
- Les résultats publics antérieurs au 2026-03-06 (frais), 2026-08-07 (règlement ponctuel) ou 2026-09-04 (150 ms) ne valent pas pour le jeu actuel. Source : `E/00-recherche.md:142`.

## 5. Méthode de mesure de l'avance des meilleurs comptes (proposée, non exécutée)

| Niveau | Résolution | Principe | Source |
|---|---|---|---|
| (a) | Seconde de bloc | Pour chaque fill taker d'un wallet candidat, comparer le rendement bourse sur [ts−Δ, ts] et [ts, ts+Δ] (Δ = 1, 2, 5, 10 s) à une population témoin (tous les autres takers, et des instants tirés au hasard dans la même fenêtre). Réalisable tout de suite sur les sessions passées ; ne voit que des avances de plusieurs secondes. | `RN/trades_publics.md:167`, `:178` |
| (b) | ms par le carnet | Retrouver dans les `delta` le retrait de liquidité au prix du fill (même token, même prix, baisse ≈ `size`) dans les ~5 s qui précèdent le `ts` de bloc ; le `ts` serveur du delta donne l'heure du match. Ambiguïté possible quand plusieurs fills ou annulations tombent au même prix : mesurer un taux d'appariement. | `RN/trades_publics.md:168` |
| (c) | ms exacte | Enregistrer `last_trade_price` (ms + `transaction_hash`) puis joindre le `transaction_hash` de la Data API pour obtenir le wallet ; jointure exacte, pour les sessions futures seulement. | `RN/trades_publics.md:169` ; `E/00-recherche.md:180-181` |
| Makers | Autre mesure | L'« avance » d'un maker se mesure par le délai entre un mouvement bourse et l'annulation ou le déplacement de ses ordres dans les `delta`, ou par la sélection adverse subie (rendement bourse après ses fills). | `RN/trades_publics.md:170` |

- Toujours comparer à une population témoin (en tendance, tous les takers « suivent » le marché), et filtrer sur `[start, start+300)`. Source : `RN/trades_publics.md:131`, `:175`.
- Limites connues : l'horodatage on-chain est postérieur au match ; l'ordre intra-bloc n'est pas exposé par la Data API (`logIndex` nécessaire on-chain) ; les PnL `user-stats` sont multi-marchés ; les wallets peuvent être scindés en plusieurs proxys. Source : `RN/trades_publics.md:172-174`.
- Données manquantes à l'époque : `last_trade_price` enregistré, flux bourse denses à la ms sur toutes les sessions, numéro de bloc et `logIndex` par fill, dérive d'horloge du `ts` serveur Polymarket. Source : `RN/trades_publics.md:181`.
- Depuis, l'étude des wallets a réalisé le niveau (b) (97 % des trades retrouvés, §2) ; le niveau (c) reste à faire. L'engine de la phase 9 enregistre les trades `last_trade_price` dans sa capture. Source : `E/JOURNAL.md:36`.
- Le `last_trade_price` n'était pas enregistré par l'ancienne application (aucune occurrence dans le dépôt d'origine). Source : `RN/trades_publics.md:45`.

## 6. Points [à vérifier]

1. `closedTime` ≈ +53 s (un marché) contre 60-120 s (spec tierce).
2. Débit autorisé de la Data API : non publié ; absence d'un endpoint REST CLOB public de trades par wallet (`clob-openapi.yaml` non lu en entier).
3. Émission de `last_trade_price` : une ligne par fill (maker) ou par match taker ? Délai entre son `timestamp` et le bloc ? (`RN/trades_publics.md:74`).
4. Unité de `block_timestamp` chez Goldsky et adresses des contrats V2 (`RN/trades_publics.md:73`).
5. Sens de `fees_refunded` et lien avec un programme de remboursement taker (`RN/trades_publics.md:134`).
6. Stabilité du classement des comptes dominants sur plusieurs jours (`RN/trades_publics.md:136`).
