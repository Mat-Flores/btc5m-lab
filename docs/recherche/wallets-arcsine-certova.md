# Wallets arcsine-barrister et CERTova : paramètres reconstruits, présence sélective, irréplicabilité

**Verdict.** (1) **arcsine ne se réplique pas** à partir des données publiques : le meilleur réplica fondé sur son comportement observé fait −1,58 $ par fenêtre avec un volume comparable, pendant que le wallet fait +6,37 $ par fenêtre hors rebates (`engine/maker-bt/WALLETS.md:25-28`). Son avantage est une **présence sélective** que ni notre juste valeur, ni les mouvements d'OKX, ni Binance/Coinbase à la milliseconde n'expliquent (`:85-105`, `:172-175`). (2) **CERTova ne peut pas être rejoué** et son edge sur le BTC 5 min s'est **éteint** de lui-même (`:29`, `:209`). Ces deux wallets ne sont donc pas des stratégies reproductibles pour le projet.

Conventions. Chemins relatifs au dépôt source `btc5m-lag-sim` (lecture seule) ; `WALLETS.md` désigne `engine/maker-bt/WALLETS.md`. Les sections §1 à §5 et la « Conclusion » de `WALLETS.md` sont déclarées **périmées par leur auteur** (`WALLETS.md:131`) : leurs réplicas tournaient en bornes traversée ou calibrée, sans rebates. Ce document garde les **paramètres reconstruits** (observations sur fills réels, indépendantes de la borne de fill) et signale chaque chiffre dépendant d'une borne biaisée. Les données brutes (`data/wallets/`) **ne sont pas copiées** dans le dépôt cible (décision D9) : les commandes de reproduction (§7) ne sont pas rejouables sans elles. `[à vérifier]` : non établi.

Les adresses de wallets ci-dessous sont des adresses publiques de la blockchain (`WALLETS.md:5-6`).

Voir aussi : [`simulateur-maker.md`](../simulateur-maker.md) (correction du simulateur sur les ordres réels d'arcsine), [`donnees-publiques.md`](donnees-publiques.md) (Data API, jointure des fills, heures de bloc), [`strategies-maker.md`](strategies-maker.md), [`exploration-maker-b01-b10.md`](exploration-maker-b01-b10.md), [`contraintes-marche.md`](contraintes-marche.md).

---

## 1. Les deux wallets étudiés

| Wallet | Adresse | Style | Marchés |
|---|---|---|---|
| `arcsine-barrister` | `0x56f609c609975454bf851f0d91079fbc4a5fe251` | Market maker des deux côtés, à partir d'un inventaire splitté | BTC 5 min uniquement |
| `CERTova` | `0x8d1d5d1c6041b13fc708b5d9f668070e1724ed4a` | Directionnel : achète le favori avec un seul ordre limite par fenêtre | BTC 5 min d'avril à juillet, puis 15 min BTC, ETH, SOL et XRP depuis septembre |

Source : `WALLETS.md:3-6, 199-202`. Étude du 2026-10-24 ; le rejeu porte sur nos carnets enregistrés (jeu B, 15/10 → 22/10 19:20 UTC). **Aucun ordre n'a été envoyé** (`:8`).

## 2. arcsine-barrister

### 2.1 Données et paramètres reconstruits

230 746 lignes d'activité du 25/09 au 24/10 (et un essai le 27/08) ; 164 151 fills dont 146 451 en maker (1,25 M$) et 17 700 en taker (0,34 M$) ; aucun achat, uniquement des `SPLIT`, des `SELL` et des `REDEEM` (`WALLETS.md:217-219`).

| Paramètre | Valeur observée | Comment on le voit |
|---|---|---|
| Inventaire | `SPLIT` de **1 500 parts** (2 000 au q90) par fenêtre, ≈ 300 s avant l'ouverture | ligne `SPLIT` de chaque fenêtre |
| Côté | Vend les **deux** tokens ; vendre Up à p revient à poser un bid Down à 1 − p | aucun `BUY` |
| Prix | **Au meilleur ask** du token : 73 % des fills pile au sommet pour les tokens bon marché, 44-55 % autour de 0,4-0,5 ; spread de 1 cent dans 88 % des cas | fill recalé à −3 s, comparé au carnet enregistré |
| Taille d'ordre | **~150 parts par niveau** (le total vendu par fenêtre, token, prix culmine à 150, dont 756 niveaux à 149,99 exactement) | histogramme des totaux par niveau |
| Période | de −60 s à 300 s, sur toute la fenêtre (médiane du temps des fills : 176 s) | — |
| Volume | ~730 parts vendues par fenêtre (q90 1 600) ; les ~1 000 paires invendues sont rendues au règlement (`REDEEM`) | — |
| Part du flux | 0,1 à 3,7 % du volume taker public ; maximum sur les bids à 0,7-0,9 (vente des outsiders), minimum sur les bids à 0,2-0,4 | jeu A comparé aux fills |
| Exposition finale | **long du gagnant dans 77 % des fenêtres** (+194 parts en moyenne) | inventaire de fin de fenêtre |
| Taker | 11 % des fills, à ~0,63 ; proche de 0 dans la fenêtre | liste `takerOnly` |

Source : `WALLETS.md:221-233`. Réserve : la taille de 150 parts est « déduite d'un pic de distribution, non certaine » (`:318`).

**Source du gain** : il vend aux acheteurs d'outsiders (« billets de loterie ») au sommet du carnet ; le PnL réalisé est positif dans tous les segments de notre semaine ; dans les 30 s qui suivent une vente d'outsider, la juste valeur de ce token baisse encore de 0,03 à 0,04 : ses fills ne subissent **pas** de sélection adverse (`:235`).

**PnL observé** : +23,9 k$ sur un mois (3,17 $/fenêtre) ; +6,37 $/fenêtre hors rebates sur notre semaine (`:203`). Les rebates maker représentent environ **+25 %** de son gain (`:24`).

### 2.2 Présence sélective (ce qui explique l'écart)

Aux trades où arcsine est présent, son PnL par part dépasse celui de n'importe quel maker présent au même trade (`WALLETS.md:87-94`) :

| Niveau de prix | Niveau touché : arcsine / makers présents | Niveau vidé : arcsine / makers présents |
|---|---|---|
| prix moyen (0,4-0,6) | +0,090 / −0,004 | +0,064 / −0,025 |
| favori | +0,003 / −0,009 | −0,004 / −0,034 |
| outsider | +0,025 / +0,006 | +0,032 / −0,002 |

- Il n'évite pas les balayages (il y est même un peu plus présent) (`:95`).
- **Pas de stock périmé** : le maker moyen est rempli après qu'OKX a bougé contre lui (−1,56 $ dans les 2 s précédentes aux niveaux touchés, −3,31 $ aux niveaux vidés) ; arcsine non (+0,75 $ et +0,02 $). Sa présence passe d'environ 1,4 % à 0,55 % des trades après un mouvement adverse de plus de 0,5 $ (`:96-99`).
- **Indépendance de notre juste valeur** : à écart égal (juste valeur − prix), il gagne 2 à 5 cents de plus par part ; sa présence ne croît pas avec notre écart, signe d'un signal que nous n'avons pas (`:100`).
- **Comportement observable** : il rejoint le meilleur prix (87 %) ; petits ordres (20 parts en médiane) sur un budget d'environ 150 parts par niveau ; il recharge dans les 100 ms qui suivent un trade (52 % des poses contre 25 % attendu par hasard) ; il cote surtout le favori (bids ≥ 0,6) et, en fin de fenêtre, les prix proches de 1 sur les deux côtés (`:101-105`).

**Mesure de présence** (`presence.mjs`, 361 790 trades dont 54 806 avec arcsine) : sur les niveaux **vidés** par un trade, tous les ordres présents sont servis, d'où sa présence pure (`WALLETS.md:158-160`).

| Ce qu'on a mesuré | Résultat | Source |
|---|---|---|
| Où il est | Toujours au meilleur prix ; près des extrêmes (bids ≥ 0,9 et ≤ 0,1 : 15 à 25 % des niveaux vidés) ; avant l'ouverture 0,6-0,8 ; presque absent des prix intermédiaires (0,2-2 %) | `:164` |
| Prévisibilité avec les données de marché (gradient boosting, test sur les 30 % de fenêtres suivantes) | AUC 0,914 ; les 10 % d'états les mieux notés couvrent 69 % de ses présences, précision 19 % | `:165` |
| Avec son propre état (parts vendues par niveau et côté, fills récents) | AUC 0,948 : pas de règle de budget déterministe | `:166` |
| Avec Binance spot et perp à la ms et la juste valeur Binance | AUC inchangée (0,914) | `:167` |
| Plafond | Même dans ses meilleurs états, présent à ~45-50 % des niveaux vidés ; cycle irrégulier : durée de vie de 60 ms à 3,4 s (médiane 574 ms), nouvelle pose de 14 ms à 32 s après la fin du précédent | `:168` |
| Son avantage | À chaque tranche d'écart (juste valeur Binance − prix), son PnL par part est positif et supérieur au maker moyen (bid au-dessus de la juste valeur de plus de 0,05 : arcsine +0,0135, maker moyen −0,0132) | `:169` |
| Se placer là où il est probablement présent | Le maker moyen n'y gagne qu'entre −0,2 et +0,6 cent par part | `:170` |

**Conclusion de l'auteur** : reproduire chacun de ses ordres à près de 100 % n'est pas atteignable avec des données publiques. Le meilleur modèle reproduit sa **couverture** (AUC 0,91) mais ni son **intermittence** ni sa **sélection**. Son information n'est dans aucune source de prix disponible (OKX, Binance spot et perp à la ms, Chainlink) ; ce serait plutôt de la vitesse d'exécution ou du flux d'ordres non public (`:172-175`). Hypothèse non testée `[à vérifier]`.

### 2.3 Réplicas valides (borne `exact`, retenue nulle, rebates compris, semaine complète)

2 342 fenêtres ; wallet : +14 911 $ hors rebates, 1,34 M parts (`WALLETS.md:107`).

| Réplica | PnL | $/fenêtre | Parts |
|---|---|---|---|
| ARC-R2 : rejoint le meilleur bid du favori (σ×1 ≥ 0,6), ordres de 20, plafond 1 500 | −21 409 $ | −9,14 | 3,64 M |
| ARC-R4 : R2 + retrait du côté qu'OKX attaque (> 1 $ en 2 s) | −12 114 $ | −5,17 | 3,21 M |
| ARC-R4 plafonné à 264 parts par côté (exposition médiane du wallet) | −2 684 $ | −1,15 | 0,82 M |
| ARC-R4 plafonné à 522 parts (q90 de son exposition) | −3 698 $ | −1,58 | 1,54 M |

Source : `WALLETS.md:109-114`. Un échantillon d'une fenêtre sur trois a donné R4 à +5 085 $ ; les deux autres tiers donnent −5 689 $ et +639 $ : **ne jamais juger un réplica maker sur un échantillon de fenêtres** (`:116`). Ces réplicas ont été évalués sur la semaine entière, période `test` comprise, et s'ajoutent au compteur d'évaluations (`:117`).

### 2.4 Anciens réplicas (périmés, à ne pas citer comme résultat)

Évalués en bornes traversée et calibrée, sans rebates, avant la correction du simulateur (`WALLETS.md:241-253`). Meilleur : ARC-fav80 (favori ≥ 0,8, σ×1,0, ordres de 150, plafond 1 500) = −11,4 $/fenêtre en traversée, −7,35 en calibrée, 3,7 M parts, corrélation de PnL avec le wallet 0,43 ; les autres vont de −5,1 à −164 $/fenêtre. Diagnostic de l'époque : le réplica épuise ses 1 500 parts dès la première minute, alors que le wallet étale ses fills sur toute la fenêtre ; sur les mêmes segments il perd là où le wallet gagne (`:251-253`). Paramètres de ARC-fav80 : `{"shares":1500,"orderShares":150,"price":{"mode":"book","offset":-1},"join":0,"entryFrom":0,"quoteEnd":0,"liqCap":false,"sigmaMult":1,"sideProb":[0.8,0.99]}` (`:341`).

## 3. CERTova

### 3.1 Données

549 377 lignes du 06/04 au 24/10 ; aucun `SPLIT` : achats, ventes occasionnelles (`WALLETS.md:259-260`). PnL par famille de marchés :

| Famille | Fenêtres | PnL |
|---|---|---|
| BTC 5 min | 17 194 | **+49,0 k$** (2,85 $/fenêtre) |
| BTC 15 min | 11 349 | +16,4 k$ |
| ETH 15 min | 8 772 | −16,2 k$ |
| SOL / XRP 15 min | 5 130 / 5 246 | +5,7 / +6,2 k$ |
| ETH / SOL / XRP 5 min | — | +2,8 / −4,2 / −10,7 k$ |

Source : `WALLETS.md:264-270`. Calendrier : BTC 5 min à pleine taille du 06/04 au 16/07, puis à très petite taille jusqu'au 07/09 ; depuis septembre, uniquement des marchés 15 min (`:274-275`). **Aucun fill sur nos fenêtres enregistrées** : sa position dans le carnet n'est pas observable (`:276`) ; il n'a pas tradé le BTC 5 min pendant notre semaine (`:29`) ; ses fenêtres d'avril à juillet n'ont pas de carnet enregistré.

### 3.2 Paramètres reconstruits (échantillon d'avril-mai, 4 310 fenêtres avec résultat gamma)

| Jambe | Paramètres observés | PnL réalisé |
|---|---|---|
| **Favori** (2 922 fenêtres) | **Un seul ordre limite BUY de 400 parts** par fenêtre (800 au q90), **à un seul prix** (jamais re-coté) ; côté favori dans 85 à 99 % des cas avant 150 s ; premier fill à 114 s en médiane (q10 42 s, q90 211 s) ; prix médian ~0,8, de 0,4 à 0,98 (suit le marché, pas une grille) | avril **+29,7 k$** (+0,029 $/part) ; mai **−1,9 k$** |
| **Loterie** (1 022 fenêtres) | 400 parts à **0,01-0,03** sur l'outsider, à partir de ~230 s | avril +3,8 k$, mai +0,3 k$ |
| Taker occasionnel | Balayages de 600 parts sur le favori (0,84 à 0,93 vers 46-70 s) | proche de 0 |

Source : `WALLETS.md:280-286`. Réserve : sa juste valeur hors de notre semaine est calculée sur Binance 1 s avec la base au prix d'ouverture officiel ; elle est trop confiante (edge apparent +0,14 contre +0,02 réalisé), seul le PnL réalisé (gain gamma − prix) est fiable (`:319`). L'échantillon est partiel : la règle est tirée d'avril-mai, les résultats gamma de juin-juillet étaient en cours de téléchargement (`:320`).

### 3.3 Réplicas sur nos carnets (semaine, 400 parts fixes par côté ; bornes périmées)

| Réplica | Traversée $/fenêtre [IC95] | Calibrée $/fenêtre [IC95] |
|---|---|---|
| CER-fav-follow : rejoint le meilleur bid du favori (σ×1,0 ≥ 0,6), 30 à 180 s, re-cotation suivie | −18,84 [−33,8 ; −3,9] | −14,64 [−26,1 ; −3,2] |
| **CER-fav-freeze** : idem, prix figé | −0,05 [−6,7 ; +6,6] | **+1,61 [−4,9 ; +8,1]** |
| CER-lot-01 : bid à 0,01 sur l'outsider (≤ 0,1) dès 210 s | −1,79 | −1,22 |
| CER-lot-02 : bid à 0,02 | −2,80 | −2,06 |

Source : `WALLETS.md:290-295`. Le prix figé (un ordre unique, comme le wallet) bat largement la re-cotation suivie mais reste neutre, ce qui cadre avec l'abandon du marché par le wallet. La loterie perd sur notre semaine, cohérent avec le lot b10 « outsider en fin de fenêtre : réfuté » (`:297` ; voir [`exploration-maker-b01-b10.md`](exploration-maker-b01-b10.md)). Ces chiffres sont en bornes biaisées ; ils ne sont pas rejoués en `exact` `[à vérifier]`.

### 3.4 Extinction de l'edge

L'edge sur le BTC 5 min **s'est éteint** de lui-même : de +29,7 k$ en avril à −1,9 k$ en mai, puis le wallet quitte ce marché en septembre ; rejoué sur notre semaine, il est neutre (`WALLETS.md:209`). Remarque : la table de conclusion (`:203`) donne aussi « −1,9 k$ » pour le BTC 15 min « sur le dernier mois », avec un libellé différent ; la coïncidence du chiffre n'est pas expliquée `[à vérifier]`.

## 4. Ce que ces wallets ont appris sur Polymarket (mesuré)

Ces apprentissages (heures data-api en heures de bloc, trades dans les deltas `price_change`, matching FIFO, annulations au prorata, calldata `matchOrders`, rebates, retenue ne valant pas pour la pose) sont détaillés dans [`simulateur-maker.md`](../simulateur-maker.md) et [`donnees-publiques.md`](donnees-publiques.md) (`WALLETS.md:31-43`). Points propres aux wallets : quand arcsine est rempli, il est le seul maker du niveau dans 42 % des cas ; quand il y en a plusieurs, il est le dernier servi dans 58 % des cas (le taker s'épuise sur lui) (`:39`).

## 5. Calibration de la juste valeur, mesurée en chemin

2 090 fenêtres, un point toutes les 10 s, spread ≤ 2 cents ; les points d'une même fenêtre sont corrélés (effectifs surestimant la précision) (`WALLETS.md:301`).

| Prob. Up | Milieu du carnet, début de fenêtre (gain observé) | Juste valeur σ×1,5, milieu de fenêtre | Juste valeur σ×1,0, milieu de fenêtre |
|---|---|---|---|
| 0,08 | 0,05 | 0,03 | 0,09 |
| 0,18 | 0,16 | 0,09 | 0,17 |
| 0,28 | 0,28 | 0,19 | 0,26 |
| 0,73 | 0,75 | 0,80 | 0,72 |
| 0,88 | 0,89 | 0,95 | 0,86 |

Source : `WALLETS.md:303-309`. Le milieu du carnet est bien calibré, avec un léger biais favori/outsider de 2 à 3 points en début de fenêtre (« le terrain d'arcsine »). La juste valeur à σ×1,5 est fortement sous-confiante (un token coté 0,18 ne gagne que 9 % du temps) ; à σ×1,0 elle est presque calibrée ; les seuils « ≥ 0,88 » à σ×1,5 valent environ 0,95 réel (`:311-312`).

## 6. Limites

(`WALLETS.md:314-321`.) Les heures data-api sont celles du bloc Polygon ; le match est estimé 3 s plus tôt (accord prix du fill / carnet 63-68 % contre 45 % sans décalage), ±1 s de bruit. On ne voit que les fills, jamais les ordres posés ni les annulations : la position dans la file et la vitesse d'annulation ne sont pas mesurables. Taille de 150 parts déduite d'un pic. Juste valeur de CERTova trop confiante hors semaine. Échantillon CERTova partiel. Les 10 réplicas ont été évalués sur toute la semaine, période `test` comprise.

## 7. Reproduire (nécessite des données non copiées)

Commandes du dépôt source (`WALLETS.md:325-331`) : `wallets.mjs <wallet>` (toute l'activité vers `data/wallets/<wallet>.jsonl`), `wallet-stats.mjs`, `wallet-join.mjs` (fills × carnets enregistrés), `wallet-windows.mjs` (cache gamma + Binance hors semaine, ~3 req/s), `wallet-join-bn.mjs --sigma 1`, `wallet-replica.mjs <wallet> --strategy '<json>' --bounds through,calibrated`. Elles lisent `data/wallets/`, qui n'est **pas** copié dans le dépôt cible (décision D9). Données de la validation sur ordres réels : fills complets `data/polymarket/fills/` (2 357 fenêtres), ordres on-chain (35 fenêtres, 1 sur 60), fills on-chain d'arcsine (78 527 fills, 15/10 → 24/10), calldata d'arcsine (64 157 transactions maker) (`WALLETS.md:135-141`). La validation d'arcsine ne peut donc être refaite sans re-télécharger ces données ; voir [`architecture-backtest.md`](../architecture-backtest.md) §7 et §8 pour l'impact sur la non-régression.
