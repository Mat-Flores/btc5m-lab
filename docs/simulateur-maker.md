# Simulateur maker : bornes de fill, validation sur ordres réels, corrections, rebates, calibrations

**Verdict.** Le simulateur maker (`engine/maker-bt/sim.mjs`) a été **corrigé et validé sur des ordres réels** les 24-25/10/2026. Avec la borne `exact`, une retenue nulle et les rebates comptés :

- sur 59 695 ordres horodatés d'autres makers (35 fenêtres) : PnL prédit **16 581 $** pour 17 331 $ réels (**−4,3 %**), volume ×1,044, ordres exacts 84,1 % ;
- sur 29 579 ordres réels d'arcsine : PnL prédit **16 645 $** pour 15 360 $ réels (**+8,4 %**), volume ×0,958, ordres exacts 71,0 % (`engine/maker-bt/WALLETS.md:14-19, 52-71`).

Ces valeurs sont la **référence de non-régression** de la migration (re-mesurée le 2026-10-27, §6). Les résultats antérieurs de la phase 11 et des lots b01-b10 ont été produits avec des bornes biaisées : voir [`strategies-maker.md`](strategies-maker.md) et [`exploration-maker-b01-b10.md`](exploration-maker-b01-b10.md).

Conventions. Chemins relatifs au dépôt source `btc5m-lag-sim` (lecture seule) ; `WALLETS.md` = `engine/maker-bt/WALLETS.md`, `sim.mjs` = `engine/maker-bt/sim.mjs`. Rien n'a jamais été envoyé en ordre maker réel (`engine/11-maker-polymarket.md:119`). `[à vérifier]` : non établi.

Voir aussi : [`architecture-backtest.md`](architecture-backtest.md) (portage Rust, `SimVenue`, plan E1), [`wallets-arcsine-certova.md`](wallets-arcsine-certova.md), [`donnees-publiques.md`](donnees-publiques.md), [`contraintes-marche.md`](contraintes-marche.md), [`ordres-et-appariement.md`](ordres-et-appariement.md), [`sources-de-prix-et-flux.md`](sources-de-prix-et-flux.md), [`journal-decisions.md`](journal-decisions.md).

---

## 1. Ce que fait `sim.mjs`

Rejeu événementiel **à la milliseconde** des carnets enregistrés (`dublin-dry-run`) : bids post-only sur Up et Down des fenêtres BTC 5 min, décidés à chaque événement comme l'engine, capital composé. Aucun ordre n'est jamais envoyé (`sim.mjs:1-3`). La fonction `simWindow(w, d, strat, opts, budget)` mêle trois choses (`sim.mjs:164-462`) :

- **la stratégie** (`decide`, l. 311-400) ;
- **l'exchange** : `post`/`liveAt` (222-234), `onBook` (251-290), `exactFill` (148-156), sorties preneuses par lecture anticipée `bookAt` (402-406) ;
- **la juste valeur** (193-217).

**Juste valeur** = celle de l'ancien moteur (`src/core/engine.js`) : mid OKX moins une base de type Binance contre Chainlink (EMA, 1/30 par impression, comparée à l'instant d'observation de l'impression), strike = moyenne Chainlink des 60 s avant l'ouverture, σ = volatilité réalisée à 1 s du spot sur 300 s (poussée chaque seconde) × `sigmaMult`, chemin Chainlink connu sur les 60 dernières secondes. `vwap` = prix moyen des trades preneurs publics des 5 dernières secondes (`sim.mjs:4-7`).

**Timing (contrainte 1)** : nos poses et annulations prennent effet `ONE_WAY` ms après la décision et ne sont pas retenues. Un changement de carnet daté `ts` (serveur) qui nous remplit vient d'un ordre preneur soumis à `ts − delay` : il ne peut remplir notre bid que si celui-ci était vivant à cet instant. Nos sorties TP/SL sont des ventes preneuses : retenue `hold` (tirée), puis FAK contre les bids existant après la retenue. Carnet silencieux plus de 3 s : aucun fill, aucune annulation, aucun stop (contrainte 7) (`sim.mjs:9-12`).

**Rebate** sur chaque fill : `rebateRate` × frais preneur du match, vérifié au cent près sur les rebates quotidiens d'un wallet (`sim.mjs:30` ; `WALLETS.md:42`).

**Capital** (`run.mjs`) : départ 1 000 $, mise = f × capital libre ; le gain revient à la fin de la fenêtre + `claim` (0 s par défaut `[non vérifié]`) ; raccourci : les rebates, en réalité versés le lendemain à 00:45 UTC, sont comptés à la fin de la fenêtre (`engine/maker-bt/run.mjs:40,59-60` ; `WALLETS.md:122`). Périodes : `sel` avant 2026-10-20 00:00 UTC, `test` après, `confirm` à partir du 2026-10-22 19:20 UTC (`run.mjs:18-21`).

## 2. Les bornes de fill

La file maker est inconnue ; les bornes encadrent le **volume** de fills, **pas le PnL** (`sim.mjs:14,29`). Book de référence : token Up ; les bids Down sont les asks Up en miroir.

| Borne | Règle (`sim.mjs:14-29`) | Biais mesuré sur ordres réels (autres makers / arcsine) |
|---|---|---|
| `prudent` | rempli quand un ask enregistré atteint notre prix, dans la limite de sa taille (`replay/src/maker.rs`) | volume ×0,779, PnL 537 $ (**−97 %**) |
| `optimistic` | prudent + toute baisse de taille d'un meilleur bid inchangé à notre prix ou en dessous, premier dans la file | ×1,196, 15 183 $ (−12 %) ; compte les annulations au meilleur bid, les fills les plus adverses |
| `calibrated` | file réelle lue dans le carnet avec les lignes preneuses publiques (heures arrondies à la seconde) ; une ligne à travers q nous remplit ; une ligne touchant q consomme la file d'abord ; une baisse de q sans ligne à ±1 s est une annulation répartie sur la file ; un ask apparaissant à q ou en dessous nous remplit | ×0,961, 19 435 $ (+12 %) / arcsine : −15 % |
| `through` | prudent + lignes preneuses publiques strictement à travers q, sans hypothèse de file (la « prudente » de la spécification d'origine de l'utilisateur), ajoutée pour les cotes au sommet du carnet | ×0,902, 14 117 $ (−19 %) / arcsine : −39 % |
| **`exact`** | file réelle avec les **vrais trades** : les fills complets (`fills.mjs`) disent quelles baisses du carnet enregistré étaient des trades, à la ms, avec leur volume par niveau ; un trade à notre niveau consomme la file devant nous (FIFO) puis nous ; un trade à un niveau sous notre bid nous a rencontré d'abord ; une baisse qui n'est pas un trade est une annulation répartie **au prorata** de la file ; un ask apparaissant à q ou en dessous nous remplit | **×1,044, 16 581 $ (−4,3 %)** / arcsine **×0,958, 16 645 $ (+8,4 %)** |

Chiffres : `WALLETS.md:54-60, 66-71`. Pourquoi `exact` : les heures du tape data-api sont des heures de bloc (≈ 2,5 s de retard sur le match), ce qui date mal le tape des bornes `calibrated` et `through` (`sim.mjs:24-26`).

La règle de fill de la borne `exact` (`exactFill`, `sim.mjs:148-156`, partagée avec `validate.mjs`) :

- on parle d'un bid (cote du côté de notre ordre) ou d'un ask (un vendeur arrive à notre prix) ;
- un ask à notre prix ou en dessous qui **augmente** nous remplit de l'augmentation ;
- un trade qui vend sous notre bid nous rencontre d'abord (`return v`) ;
- à notre niveau, un trade consomme `min(o.qa, v)` de la file devant nous puis nous remplit du reste ;
- une baisse qui n'est pas un trade réduit la file devant nous au prorata : `o.qa -= (chOld - chSize) * o.qa / chOld`.

Le test Rust de non-régression porte ces règles à l'identique : voir [`architecture-backtest.md`](architecture-backtest.md) §7.

## 3. Validation sur ordres réels (`validate.mjs --sim`)

Chaque ordre réel est rejoué par `simWindow` lui-même (option `script`) : il entre dans le carnet à l'instant de sa pose réelle (retrouvée dans les deltas) avec la vraie file devant lui, en sort à son annulation (retrouvée aussi), puis on compare ses fills prédits aux réels et le PnL au règlement (`WALLETS.md:45-50`). Commande : `node engine/maker-bt/validate.mjs [--maker 0x...] [--delay 0] [--sim prudent,through,calibrated,optimistic,exact]` (`validate.mjs:10-11`).

**Méthode d'identification** (`validate.mjs:1-9`) : les ordres viennent du calldata `matchOrders` (taille d'origine, prix, champ `timestamp` de création en ms présent chez la plupart des clients, 0 chez arcsine) ; l'arrivée d'un ordre est l'augmentation du carnet de **exactement sa taille** à son niveau, à partir de sa création, avec la file juste avant ; un ordre partiellement rempli finit à la baisse exacte de son reste qui n'est pas un trade. Pour arcsine (`--maker`), les matches viennent du calldata du wallet, chaque ordre identifié par son hash on-chain, l'arrivée étant la dernière augmentation de sa taille exacte avant son premier fill.

**Ordres des autres makers** (59 695 ordres horodatés, 35 fenêtres réparties sur la semaine, retenue nulle) (`WALLETS.md:52-62`) :

| Borne | Volume prédit / réel | Ordres exacts | PnL (réel 17 331 $) |
|---|---|---|---|
| prudente | 0,779 | 77,2 % | 537 $ (−97 %) |
| traversée | 0,902 | 80,4 % | 14 117 $ (−19 %) |
| calibrée | 0,961 | 81,8 % | 19 435 $ (+12 %) |
| optimiste | 1,196 | 82,3 % | 15 183 $ (−12 %) |
| **exacte** | **1,044** | **84,1 %** | **16 581 $ (−4,3 %)** |

Par jour, réel / exacte (`WALLETS.md:62`) : 15/10 4 684 / 4 535 ; 16/10 567 / 710 ; 17/10 −6 496 / −6 557 ; 18/10 −1 264 / −1 550 ; 19/10 6 225 / 5 671 ; 20/10 5 162 / 5 306 ; 21/10 8 874 / 8 911 ; 22/10 −422 / −444.

**Ordres d'arcsine sur toute la semaine** (64 157 transactions maker, 29 579 ordres après l'ouverture dont la fin est connue) (`WALLETS.md:64-78`) :

| Borne | Volume prédit / réel | Ordres exacts | PnL (réel 15 360 $) |
|---|---|---|---|
| traversée | 0,746 | 62,7 % | 9 384 $ (−39 %) |
| calibrée | 0,801 | 65,0 % | 13 080 $ (−15 %) |
| **exacte** | **0,958** | **71,0 %** | **16 645 $ (+8,4 %)** |
| exacte avec retenue de 0,15 s | — | — | 7 943 $ pour 16 383 $ réels (−52 %) |

Par jour en borne exacte, réel / prédit : 15/10 356 / −93 ; 16/10 535 / 1 262 ; 17/10 325 / 502 ; 18/10 4 510 / 4 640 ; 19/10 2 470 / 2 990 ; 20/10 2 518 / 1 689 ; 21/10 2 146 / 2 376 ; 22/10 3 187 / 3 094 ; 23/10 −688 / 186 (`WALLETS.md:75-78`).

**Limites de la validation** (`WALLETS.md:80-83, 119`) :

- seuls les ordres ayant reçu **au moins un fill** sont visibles : on ne mesure pas les fills que le simulateur inventerait à des ordres jamais remplis ;
- la pose des ordres d'arcsine est retrouvée par leur taille, moins sûr pour les ordres de 5 parts et de ~150 parts ;
- 4 % du volume d'arcsine n'a aucun fill prédit (pose mal retrouvée, file aberrante) ;
- les tests `sim.test.mjs` passaient 8/8 à l'époque (le fichier compte 9 tests aujourd'hui).

## 4. Les quatre causes du décalage corrigées, et ce qu'on a appris de Polymarket

**Pourquoi le simulateur ne reproduisait pas les wallets** (`WALLETS.md:20-24`) : (1) le tape data-api daté en heure de bloc (2,5 s de retard sur le match) ; (2) les bornes de fill qui en découlaient (sur ordres réels : prudente −97 %, traversée −19 à −39 %, calibrée −15 à +12 %) ; (3) la **retenue du preneur de 0,15 s** appliquée à la pose : sur les ordres d'arcsine elle fait passer le PnL prédit de 16 383 $ réels à 7 943 $ (−52 %), car des ordres posés moins de 150 ms avant un trade sont bien remplis en réalité ; (4) les **rebates maker** non comptés (≈ +25 % du gain d'arcsine).

**Apprentissages sur Polymarket, mesurés** (`WALLETS.md:33-43`) :

1. **Les heures data-api sont des heures de bloc**, en retard de **2,5 s** en médiane (1,7-3,1 s) sur le match visible dans le carnet (mesure : fills on-chain d'arcsine comparés à la baisse exacte qu'ils produisent dans nos deltas).
2. **Les trades apparaissent dans les deltas `price_change`**, pas dans les snapshots ; nos sessions ne contiennent aucun trade. `fills.mjs` les retrouve en associant chaque ordre taker (fills complets `takerOnly=false`, lignes maker au prix exact du niveau) à la baisse exacte de son volume : **97 % des trades et 96 % du volume**, contre 8,6 % de coïncidences avec un décalage témoin de +10 s.
3. **Matching FIFO** (ordre des `OrderFilled` on-chain). Quand arcsine est rempli, il est le seul maker du niveau dans 42 % des cas ; s'il y en a plusieurs, il est le dernier servi dans 58 % des cas.
4. **Annulations et file** : une annulation réduit la file devant nous **au prorata**. Sur 66 273 ordres réels, le volume prédit vaut 1,04 fois le réel au prorata, 0,94 si les annulations sont toutes derrière nous, 1,09 si elles sont toutes devant. **La réalité est entre le prorata et « toutes derrière nous » (`WALLETS.md:40`).**
5. Le calldata `matchOrders` (exchange V2 `0xe11118…`) contient les ordres signés : taille d'origine, prix, champ `timestamp` de création en ms (0 chez arcsine).
6. **Rebates maker** : 0,2 × 0,07 × p(1−p) × parts par fill, versés le lendemain à 00:45 UTC. Vérifié au cent près sur les versements quotidiens d'arcsine (254,60 $ versés pour 254,44 $ calculés ; 477,78 $ pour 477,72 $) : **deux jours, un seul wallet** `[à vérifier]` ailleurs.
7. **La retenue de 0,15 s ne vaut pas pour la pose** : un ordre posé 11 à 140 ms avant un trade est rempli en réalité (délai pose → premier fill d'arcsine : 140 ms en médiane). Pour les **annulations**, les données ne permettent pas de trancher `[à vérifier]`.

**Recommandation retenue** : `delay: 0` avec la borne `exact` (le défaut du code reste 0,15 s, contrainte de l'utilisateur, à ne changer que sur sa décision) (`WALLETS.md:123`). Dans la migration, ce choix est le paramètre `fill_lookback_ms` = 0 (obligatoire) : voir [`architecture-backtest.md`](architecture-backtest.md) §5.3.

**Leviers ajoutés au simulateur, tous désactivés par défaut** (`WALLETS.md:124-129`) : `orderShares` (taille de chaque ordre sous le plafond `shares`), `prob: 'mid'` (milieu du carnet), `life` (durée de vie d'un ordre), `pullSide` (retrait directionnel sur mouvement d'OKX), `script` (un ordre imposé, pour la validation). Les résultats antérieurs changent du montant des rebates (C4 reste dans ses plages) (`:130`).

## 5. Rebates

| Fait | Valeur | Source | Statut |
|---|---|---|---|
| Formule | `rebate = rebateRate × rate × (p(1−p))^exponent × parts`, avec `rebateRate` 0,2, `rate` 0,07, exposant 1 lus du barème du marché | `sim.mjs:185,240` ; `engine/01-audit.md:236` | mesuré |
| Versement | le lendemain à 00:45 UTC ; compté à la fin de la fenêtre dans le simulateur (raccourci) | `WALLETS.md:42,122` | mesuré |
| Vérification | 254,60 $ versés pour 254,44 $ calculés ; 477,78 $ pour 477,72 $ | `WALLETS.md:42` | 2 jours, 1 wallet |
| Poids | ≈ +25 % du gain d'arcsine | `WALLETS.md:24` | mesuré |
| Comptés dans les PnL | depuis 2026-10-25 ; **absents de b01-b10 et G1-G4** | `WALLETS.md:24,130` ; `engine/11-maker-polymarket.md:106` | établi |
| Éligibilité aux récompenses de liquidité (ancienneté 3,5 s) pour le crypto | non vérifiée | `engine/research_notes/Engine réel Polymarket BTC 5m/axe_C_strategies.md:151` | `[à vérifier]` |

Détail des règles officielles : [`contraintes-marche.md`](contraintes-marche.md).

## 6. Re-mesure du 2026-10-27 (référence de non-régression figée)

Commande : `node --max-old-space-size=8192 engine/maker-bt/validate.mjs --delay 0 --sim exact`, durée 21 min 42 s ([conception-engine-migration.md:24-25](conception-engine-migration.md) ; bloc de sortie aussi dans [contrats-migration.md:144-156](contrats-migration.md)). Sortie de référence :

```
simWindow on 59695 twins after the open with a known end (real 1089572 sh, PnL 17331 $):
  exact      predicted 1137900 sh, ratio 1.044, orders exact 0.841, PnL 16581 $
  10-15 real 4684 $ | exact 4535 $     10-16 real 567 $ | exact 710 $
  10-17 real -6496 $ | exact -6557 $   10-18 real -1264 $ | exact -1550 $
  10-19 real 6225 $ | exact 5671 $     10-20 real 5162 $ | exact 5306 $
  10-21 real 8874 $ | exact 8911 $     10-22 real -422 $ | exact -444 $
```

La re-mesure **reproduit exactement** les valeurs de `WALLETS.md:52-62` (par jour identique à la l. 62). Étape intermédiaire notée : 71 049 ordres, 66 273 arrivées trouvées, ratio 1,102 ; 63 152 ordres de fin connue, ratio 1,042, ordres exacts 0,812. Le chiffre de 66 273 ordres se retrouve dans `WALLETS.md:40`. Le plan de non-régression (tolérance zéro, f64 bit à bit) est dans [`architecture-backtest.md`](architecture-backtest.md) §7.

## 7. Calibrations de la juste valeur validées

### 7.1 Spot Binance à la milliseconde, σ×1,3

- Les trades Binance BTCUSDT (spot et perpétuel, archives publiques `aggTrades`) remplacent OKX pour la juste valeur, avec une réception supposée **120 ms** après la bourse (`WALLETS.md:153-155` ; `binance-trades.mjs:1-4`).
- Calibrée à **σ×1,3** : une probabilité de 0,93 donne 93 % observé, 0,48 donne 47-48 %, 0,02 donne 3-4 %, en début et milieu de fenêtre ; σ×1,0 est trop confiante, σ×1,6 pas assez (`WALLETS.md:155-157`).
- La latence de 120 ms est **mesurée pour OKX vers Dublin, non vérifiée pour Binance** `[à vérifier]` (`:155`).
- Raison du changement : le flux OKX enregistré était très creux (25 à 1 330 prix par fenêtre ; 25 à 100 du 16/10 22:00 au 18/10, un prix toutes les 7 à 27 s), donc la juste valeur fondée sur lui était périmée de plusieurs secondes (`:146-151`). Nuance : le flux brut OKX est dense (20-35 msg/s) et les sessions n'écrivent un `px` que quand le mid change ; la cause réelle de la rareté n'est pas établie `[à vérifier]` (voir [`sources-de-prix-et-flux.md`](sources-de-prix-et-flux.md)).

### 7.2 Autres calibrations

| Source de prix | σ calibré | Mesure | Source |
|---|---|---|---|
| OKX enregistré | ×1,0 presque calibré ; ×1,5 sous-confiant (un token coté 0,18 ne gagne que 9 % du temps) | Brier test 0,1306 (×1,0) contre 0,1329 (×1,5) ; la validation glissante choisit 1,5 à l'apprentissage et 1,0 au test | `WALLETS.md:303-312` ; `engine/08-strategies.md:191-205` |
| Binance spot ou perp | ×1,3 | voir §7.1 | `WALLETS.md:155-157` |
| Composite Binance perp + Coinbase (arrivées +113 / +40 ms) | ×1,2 | calibré | `WALLETS.md:187` ; `EXPLORATION.md:258` |
| Milieu du carnet | bien calibré, léger biais favori/outsider de 2 à 3 points en début de fenêtre | 2 090 fenêtres, un point toutes les 10 s | `WALLETS.md:301-311` |

Le σ optimal dépend donc de la source de prix. Les seuils `sideProb` des anciens candidats (« ≥ 0,88 » à σ×1,5) valent environ 0,95 réel : `[à vérifier]` avant de comparer des stratégies entre sources (`WALLETS.md:312`). Latence Coinbase +40 ms non vérifiée (`EXPLORATION.md:252`).

## 8. Constantes de `sim.mjs`

| Constante | Valeur | Origine | Ligne |
|---|---|---|---|
| `ONE_WAY` | 12,2 ms | moitié du RTT p50 Dublin → CLOB ; symétrie `[non vérifié]` | 36 |
| `MIN_SIZE` | 5 parts | `orderMinSize` (`src/feeds/live.js`) ; `mos: 5` lu une fois | 37 |
| `SILENCE_MS` | 3 000 | seuil de silence Polymarket | 38 |
| `HOLD_Q` | [[0, 0,15], [0,1, 0,19], [0,5, 0,36], [0,9, 1,5], [1, 3,5]] s | `engine/10-delai-appariement.md` ; **pas une mesure directe** (n = 3 ordres + tableau du premier trade après un saut d'OKX) `[à vérifier]` | 39-40 |
| retenue par défaut `delay` | 0,15 s (recommandé : 0 avec `exact`) | contrainte de l'utilisateur | 166 |
| `exitSlip` | 0,05 | limite FAK = probabilité − exitSlip | 56 |
| `quoteEnd`, `entryFrom` | 10 s, 10 s | stratégie | 59-60 |
| coupure après la fin | événements > fin + 2 000 ms ignorés ; boucle jusqu'à fin + 2 500 ms | aucune règle documentée `[à vérifier]` | 258, 410 |
| `sigmaMult` | 1,5 | défaut historique, sous-confiant avec OKX | 47 |
| `spotLatency` | 120 ms | Binance, non vérifié `[à vérifier]` | 54 |

Voir [`architecture-backtest.md`](architecture-backtest.md) §6.4 pour la liste complète à sortir du code.

## 9. Tests et outils

`engine/maker-bt/test/sim.test.mjs` : 9 tests (capital inchangé à f = 0 ; ask à notre prix ; trade antérieur à la pose ; annulation avant la soumission du preneur ; stop pendant un carnet silencieux ; `reserveInFlight` ; parité C4 sur les carnets enregistrés, ≈ 4 min ; borne `exact` ; spot absent). Outils de données du dépôt source : `fetch-fills.mjs` (tous les fills maker et taker, `takerOnly=false`, de data-api, mis en cache), `fills.mjs` (`labelTrades` : volume tradé porté par chaque enregistrement du carnet), `binance-trades.mjs`, `leadlag.mjs` (quelle source bouge avant le carnet), `calibrate.mjs` (produit `calibration.json`), `pack.mjs` (empaquette les jeux A et B), `validate.mjs`. Lecture seule ; **ne pas lancer `explore.mjs`**, qui écrit dans `EXPLORATION.md`.

## 10. Corrections E1 des décalages avec le réel

Écarts à réduire : volume ×1,044 / PnL −4,3 % / ordres exacts 84,1 % (autres makers) ; volume ×0,958 / PnL +8,4 % / 71 % (arcsine). **Aucune correction n'est encore mesurée.** La méthode (options nouvelles et paramétrées, défaut = comportement validé, mesure avant/après sur `validate` avec split temporel, cause documentée) est décrite dans [`architecture-backtest.md`](architecture-backtest.md) §8. Ce document recevra le résultat (gain, risque de surajustement, valeurs retenues) : **à compléter après les mesures**.

## 11. Points à vérifier

Symétrie de l'aller simple ; retenue des annulations maker (indécidable sur nos données) ; annulations prorata contre « toutes derrière nous » (`WALLETS.md:40`) ; file mesurée à la décision contre à l'arrivée ; impact de nos ordres sur le carnet ; ordres de 5 parts mal retrouvés ; durabilité et éligibilité des rebates ; délai de réclamation ; coupure du carnet en fin de fenêtre ; distribution de retenue des sorties preneuses ; latences de réception de Binance et Coinbase ; σ par source.
