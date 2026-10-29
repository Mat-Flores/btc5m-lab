# Fenêtres « choc » : garde-fous et pause sur calendrier macro

**Verdict (2026-10-29).** Aucun garde-fou (`noKnife`, `maxDiverge`, `maxImbalance`, `pullSide`, `pull`) ni la pause sur les annonces macro US n'améliore de façon significative les deux stratégies testées. Le maker continu ne perd pas dans les fenêtres « choc » : il y gagne, et perd tout le reste du temps. La piste est écartée ([pistes-ecartees.md](../pistes-ecartees.md)).

Étude faite dans ce dépôt (pas dans `btc5m-lag-sim`) : lot [`backtest/config/chocs.json`](../../backtest/config/chocs.json), analyse [`backtest/scripts/chocs.mjs`](../../backtest/scripts/chocs.mjs), sorties `backtest/out/chocs.txt` et `chocs-analyse.txt` (ignorées par git).

## 1. Protocole

**Données.** Packs du 15/10 00:20 au 22/10 19:15 UTC, 2 090 fenêtres de 5 min, entrée `pack` (le chemin validé à 0 écart avec l'oracle, fills réels). Exchange par défaut : borne `exact`, retenue 0, rebates, `cancel_ahead_factor` 1. Capital 1 000 $, mise 5 % du capital libre. Les traces courantes ne servent pas : celles du 28/10 n'ont aucun résultat réglé, et le 29/10 n'est pas encore clos.

**Deux bases**, toutes deux sur Binance spot à la ms, σ × 1,3 :
- **A** = BN-E04 (lot x03) : rejoint le meilleur bid des deux côtés pendant toute la fenêtre, si juste valeur − bid ≥ 0,04.
- **B** = LT-REF : à partir de 270 s, bid à juste valeur − 0,01 sur le côté dont la probabilité est entre 0,88 et 0,999.

**Variantes.** Chaque base, puis + un garde-fou, puis tous ensemble (`ALL`, sans `pull`). Les valeurs ont été fixées **avant** le premier run, à partir des exemples de `config/strategies/maker.toml`, et jamais ajustées :

| Garde-fou | Valeur | Effet |
| --- | --- | --- |
| `noKnife` (NK) | T 5 s, M 0,10 | pas d'offre sur un côté dont la juste valeur a chuté de plus de 0,10 en 5 s |
| `maxDiverge` (MD) | 0,08 | pas d'offre tant que \|juste valeur − milieu du carnet\| > 0,08 |
| `maxImbalance` (MI) | 20 parts | pas d'offre sur un côté qui détient 20 parts de plus que l'autre (50 dans le TOML, rarement atteint à f 5 %) |
| `pullSide` (PS, `Move`) | J 20 $, S 2 s | pas d'offre du côté contre lequel le BTC a bougé de plus de 20 $ en 2 s |
| `pull` (PL, `Move`) | J 20 $, S 2 s | retire les deux offres dans ce cas |

**Fenêtre « choc ».** Le plus grand mouvement du BTC (Binance spot, clôtures à la seconde) sur 10 s dans la fenêtre dépasse le 95e centile mesuré sur 20 160 fenêtres d'avril à septembre, donc hors de la semaine testée. Seuil : **100,08 $ en 10 s**. 78 fenêtres de la semaine sont des chocs (3,7 %).

**Pause macro.** Ne pas jouer de T − 5 min à T + 15 min autour des annonces US à fort impact de la période (8:30 ET = 12:30 UTC) : ventes au détail et inscriptions au chômage le 15/10, mises en chantier le 20/10, inscriptions au chômage le 22/10. Soit 12 fenêtres, dont 2 chocs. Calendrier tiré de calendriers en ligne ([investrade](https://investrade.com/weekly-event-calendar-10-19-2026-10-23-2026/), [thetrading.tools](https://www.thetrading.tools/economic-calendar/2026-10-15)), heure du 15/10 `[à vérifier]` (la page ne donne pas l'heure). La pause est appliquée a posteriori : le rendement des fenêtres en pause est mis à 0.

**Ce que mesurent les chiffres.**
- « Rendement » = PnL de la fenêtre / capital juste avant, en % ; les colonnes sont la **somme** de ces rendements, pas le capital final composé. Deux runs n'ont pas le même capital au même instant, et le rendement les rend comparables.
- « Δ » = somme des rendements de la variante − ceux de la base, sur les mêmes fenêtres.
- **Monte Carlo** : bootstrap par blocs d'une heure (10 000 tirages) de Δ. On tire des heures avec remise, ce qui garde la corrélation entre fenêtres voisines. IC95 = 2,5e et 97,5e centiles ; P(Δ>0) = part des tirages où la variante fait strictement mieux.
- **Walk-forward** : chaque jour, on prend la variante (base comprise) qui a le meilleur rendement cumulé sur les jours précédents, puis on la joue ce jour-là. Résultat sur les jours 2 à 8.
- « Δ sel » / « Δ test » : Δ avant et après le 20/10 00:00 UTC (la séparation habituelle).

## 2. Résultats

| id | jouées | Σ rendement | dont chocs | hors chocs | Δ total [IC95] | P(Δ>0) | Δ chocs [IC95] | Δ sel | Δ test |
| --- | ---: | ---: | ---: | ---: | --- | ---: | --- | ---: | ---: |
| A | 1 875 | −99,6 % | +31,3 % | −130,9 % | | | | | |
| A-NK | 1 878 | −91,5 % | +28,4 % | −119,9 % | +8,1 [−74,7 ; +124,2] | 0,51 | −2,9 [−12,0 ; +4,4] | +33,6 | −25,5 |
| A-MD | 1 881 | −75,9 % | +51,3 % | −127,1 % | +23,7 [−45,8 ; +111,3] | 0,72 | +20,0 [−3,7 ; +59,8] | +35,0 | −11,3 |
| A-MI | 1 873 | −104,9 % | +34,0 % | −138,9 % | −5,3 [−81,1 ; +53,1] | 0,48 | +2,7 [−4,2 ; +10,4] | −1,4 | −3,9 |
| A-PS | 1 873 | −99,1 % | +32,5 % | −131,6 % | +0,4 [−24,6 ; +24,9] | 0,52 | +1,2 [−3,1 ; +6,0] | +6,1 | −5,7 |
| A-PL | 1 871 | −110,7 % | +28,7 % | −139,4 % | −11,2 [−52,2 ; +27,4] | 0,30 | −2,7 [−9,9 ; +4,2] | −9,6 | −1,6 |
| A-ALL | 1 871 | −93,2 % | +31,7 % | −124,9 % | +6,4 [−113,8 ; +143,4] | 0,52 | +0,4 [−5,2 ; +6,5] | +17,4 | −11,0 |
| B | 464 | +0,20 % | +0,08 % | +0,13 % | | | | | |
| B-NK | 464 | identique à B | | | 0 | | 0 | 0 | 0 |
| B-MD | 288 | −0,39 % | +0,08 % | −0,46 % | −0,59 [−1,51 ; +0,10] | 0,05 | 0 | −0,32 | −0,27 |
| B-MI | 464 | +0,14 % | +0,08 % | +0,06 % | −0,06 [−0,18 ; 0,00] | 0,17 | 0 | −0,06 | 0,00 |
| B-PS | 463 | identique à B | | | 0 | | 0 | 0 | 0 |
| B-PL | 461 | +0,14 % | 0,00 % | +0,14 % | −0,06 [−0,25 ; +0,06] | 0,24 | −0,08 [−0,24 ; 0,00] | −0,01 | −0,05 |
| B-ALL | 287 | −0,45 % | +0,08 % | −0,53 % | −0,65 [−1,57 ; +0,05] | 0,04 | 0 | −0,38 | −0,27 |

Capital final (composé) : A de 220 à 298 $ selon la variante (−70 à −78 %) ; B de 994 à 1 001 $.

**Pause macro.** A perd −5,2 % dans les 12 fenêtres en pause ; les sauter rapporte +5,2 % [−6,0 ; +24,4], P(Δ>0) 0,63. B n'a rien joué dans ces fenêtres (Δ = 0).

**Walk-forward.** A : −122,6 % contre −136,1 % pour la base (choisit `MD` dès le 17/10) ; le maker continu reste ruiné. B : −0,69 % contre −0,62 %.

## 3. Lecture

1. **Le maker continu (A) ne perd pas dans les chocs.** Il gagne +31 % de rendement cumulé sur les 78 fenêtres choc et perd −131 % sur les 2 012 autres. Les garde-fous visent les chocs, mais la perte de A est une sélection adverse diffuse, déjà constatée en x03/x05 ([pistes-ecartees.md](../pistes-ecartees.md)). Aucun garde-fou ne la corrige : tous les IC95 contiennent 0.
2. **`maxDiverge` est le seul candidat qui aide sur A** (+23,7 %, P 0,72), surtout dans les chocs (+20 %, P 0,91), mais il ne passe pas le seuil de significativité. Son effet change de signe entre `sel` (+35 %) et `test` (−11 %), et le walk-forward qui le choisit reste à −123 %. Ce n'est pas un résultat robuste.
3. **LT-REF (B) n'est presque pas exposé aux chocs** : il ne cote qu'à partir de 270 s, sur un côté déjà sûr à ≥ 88 %. `noKnife` et `pullSide` ne s'y déclenchent jamais (runs identiques au bit près). `maxDiverge` et `ALL` lui retirent 176 fenêtres jouées et du gain (P(Δ>0) 0,04-0,05). Une seule fenêtre choc diffère (B-PL). Les garde-fous sont inutiles ou nuisibles sur B.
4. **Pause macro** : 12 fenêtres sur 2 090, 3 annonces. L'échantillon est trop petit pour rien conclure (IC95 de −6 à +24 %). Elle ne coûte rien à B, qui n'y joue pas.

## 4. Limites

- Une semaine (15→22/10), 78 chocs et 3 annonces macro : puissance faible, surtout pour la pause macro.
- Une seule valeur par garde-fou, fixée a priori (choix délibéré contre le surajustement) : une autre valeur pourrait se comporter autrement, mais la tester sur la même semaine ajouterait des essais sur `test` (~150 évaluations déjà).
- Rendements additionnés, pas composés ; la pause macro est appliquée a posteriori (en réel, le capital non engagé dans ces fenêtres changerait légèrement les mises suivantes).
- Seuil de choc défini sur Binance spot à la seconde ; un choc plus court qu'une seconde n'est pas vu.
- Pour retenter avec un fait nouveau : refaire tourner le lot sur des traces courantes réglées (à partir du 29/10, suivi gamma 900 s) quand plusieurs semaines seront rapatriées.
