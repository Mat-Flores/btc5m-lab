# Exploration maker : critères R1-R7, lots b01 à b10, confirmations, lots x01 à x05

**Verdict.** L'exploration de 10 lots de stratégies maker (et 5 lots de re-test) n'a produit **aucune stratégie fiable**. Tous les résultats des lots b01 à b10 sont **à revoir** en borne `exact`, retenue nulle, avec rebates (`engine/maker-bt/EXPLORATION.md:243`). Le candidat principal LT-REF a été invalidé comme résultat robuste, **sans conclusion définitive** : son signe dépend de la source de prix (négatif avec Binance spot, légèrement positif avec le composite Binance perp + Coinbase, trop peu de fills pour conclure) (`engine/maker-bt/WALLETS.md:194`). Les makers en cotation continue (x03, x05) sont tous perdants, de −10 à −87 % sur la semaine.

Conventions. Chemins relatifs au dépôt source `btc5m-lag-sim` (lecture seule). `EXPLORATION.md` désigne `engine/maker-bt/EXPLORATION.md` ; `WALLETS.md` désigne `engine/maker-bt/WALLETS.md`. Tout est simulé, aucun ordre n'a été envoyé. `[à vérifier]` : non établi. Le tâtonnement (prompt de reprise, compteur détaillé, tableaux par lot) n'est pas recopiée : une ligne par lot et les candidats nommés.

Voir aussi : [`strategies-maker.md`](strategies-maker.md) (G1-G4), [`simulateur-maker.md`](../simulateur-maker.md) (bornes de fill), [`wallets-arcsine-certova.md`](wallets-arcsine-certova.md), [`contraintes-marche.md`](contraintes-marche.md), [`sources-de-prix-et-flux.md`](sources-de-prix-et-flux.md), [`architecture-backtest.md`](../architecture-backtest.md).

---

## 1. Cadre de l'exploration

**But** : trouver une stratégie maker (bids post-only) **fiable** sur les marchés BTC Up/Down 5 min, en backtest uniquement (`EXPLORATION.md:5-8`).

**Données.** Carnets enregistrés (jeu « B ») : 15 sessions de `dublin-dry-run`, du 15/10 00:20 au 22/10 19:20 UTC, soit 2 090 fenêtres, empaquetées dans `engine/maker-bt/packs/B/` (≈ 12 Go, hors git). Trades publics et gamma (jeu « A », 30 jours) : `data/polymarket/`, empaquetés ; le mois n'est pas simulé (décision de l'utilisateur) (`EXPLORATION.md:20-26`).

**Capital et périodes.** Départ 1 000 $, mise = f × capital libre, 50/50 Up/Down, gain rendu à la fin de la fenêtre + `claim` (0 s). `sel` = avant le 2026-10-20 00:00 UTC ; `test` = après (hors échantillon, ≈ 808 fenêtres) ; `confirm` = après le 22/10 19:20 UTC (`EXPLORATION.md:47-49` ; `engine/maker-bt/run.mjs:18-21`).

**Critères de fiabilité** (en borne de décision, mise et paramètres figés) (`EXPLORATION.md:74-85`) :

| # | Critère |
|---|---|
| R1 | PnL moyen par fenêtre > 0 sur `sel` **et** sur `test`, et IC95 sur `test` entièrement > 0 |
| R2 | Gain non concentré : ≥ 5 jours positifs sur 8, et aucun jour ne pèse plus de 50 % du gain de la semaine |
| R3 | Drawdown maximal de la semaine ≤ 20 % |
| R4 | Borne calibrée > 0 sur `test` |
| R5 | (indicatif) PnL ≥ 0 en prudente sur `test` avec une retenue de 1,5 s |
| R6 | **Confirmation** sur des jours enregistrés après le 22/10, jamais vus (`explore.mjs --confirm`) : l'IC95 de la borne « traversée » > 0 **et** l'EV basse (R7) > 0 (`EXPLORATION.md:337`) |
| R7 | Assez de fills : sur les fenêtres **remplies**, l'espérance au bas de l'IC95 de Wilson du taux de gain est > 0 (perte non observée = toute la dépense). Ajouté le 23/10 car une stratégie rarement remplie peut avoir un IC95 serré sur les fenêtres jouées sans rien prouver |

**Bornes de décision.** Prudente jusqu'au lot b04 ; « traversée » dès b05 (prudente + trades publics strictement à travers notre prix, « uniquement des preuves certaines ») ; `exact` à partir de x01 (`EXPLORATION.md:87-91, 703`). Définitions : [`simulateur-maker.md`](../simulateur-maker.md).

**Pièges connus** (`EXPLORATION.md:65-70`) : multiplier les essais sur les mêmes ~808 fenêtres de `test` produit des faux positifs ; une stratégie retenue doit être confirmée sur des données postérieures au 22/10 ; une moyenne par fenêtre positive peut coexister avec la ruine en capital composé ; le 21/10 domine plusieurs résultats.

**Compteur d'évaluations sur `test`** : 32 (grilles G1-G4 et H, 22/10) → 100 à la fin de b10 (23/10) → ~110 (réplicas de wallets, 24/10) → ~134 (réplicas d'arcsine en `exact`, 25/10) → ~136 (x01 à x05, 26/10) (`EXPLORATION.md:126-143`).

**Format des lots.** Un lot est un JSON `{name, note, base, candidates[{id, desc, ...}]}` dans `engine/maker-bt/batches/` (clés pointées pour les paramètres imbriqués) ; 21 fichiers : `b01-leviers`, `b02-fin-de-fenetre`, `b03-fin-combinee`, `b04-fin-desaccord`, `b05-traversee-penny`, `b06-traversee-affinage`, `b07-retrait-rapide`, `b08-lt-mise`, `b09-fin-precoce-sure`, `b10-outsider-fin`, `diag-c4`, `r6-candidats-1` à `5`, `x01-retest-exact`, `x02-binance-spot`, `x03-binance-maker`, `x04-mix-frozen`, `x05-mix-maker`. Les candidats figés pour la confirmation R6 ne sont jamais modifiés : toute variante est un nouveau candidat (`EXPLORATION.md:339-340`). Ces lots doivent tourner dans le backtest de la migration (voir [`architecture-backtest.md`](../architecture-backtest.md)).

## 2. Les lots b01 à b10 (borne prudente puis traversée)

Chaque ligne : contenu, résultat d'origine, état. Nombre de stratégies et de promues : ligne « Base » de chaque section du §5 de `EXPLORATION.md`.

| Lot | Contenu | Résultat d'origine | État / pourquoi à revoir | Source |
|---|---|---|---|---|
| **b01-leviers** | 24 stratégies : filtres anti-sélection adverse, favori/outsider, VWAP, etc., f 2 %, cotation sur toute la fenêtre | 23 non promues. Les variantes sur toute la fenêtre font −0,62 à −1,79 $/fenêtre sur `sel` ; FAV −0,10, LATE240 −0,03, LATEBK −0,42. **1 promue : LATE270** (+0,02 $/fenêtre, 6 jours sur 8 positifs, drawdown ≈ 0, tient à 1,5 s) | À revoir. **Impasse robuste** : coter toute la fenêtre perd, quel que soit le filtre (confirmé en x03/x05) | `EXPLORATION.md:147-161, 344-376` |
| **b02-fin-de-fenetre** | Affinage de LATE270 : bid seulement sur le côté quasi gagnant (90-99,5 %) dès 270 s. 24 stratégies, 8 promues | R1 échoue de peu (borne basse hors échantillon −0,02 à 0,00 : manque de puissance, pas changement de signe). Leviers favorables : décalage 0,01, côté jusqu'à 0,999, pas de plafond de liquidité, coter jusqu'à la fin, entrée 260-275 s. Défavorables : entrée 240-255 s, côté ≥ 0,80-0,85, VWAP, σ × 1,0, prix figé, meilleur bid | À revoir (bornes). Dépend de la coupure du carnet, non documentée `[à vérifier]` | `:162-173, 377-409` |
| **b03-fin-combinee** | Combinaison des leviers de b02. 18 stratégies, 15 promues | La **réserve des ordres en vol est indispensable** (sans elle le gain sur `sel` tombe à 0,00). La mise ne change pas la significativité (f 20 % : +10 % sur la semaine, drawdown ≈ 0) | À revoir | `:165-173, 410-436` |
| **b04-fin-desaccord** | Seuil du côté (0,86-0,90) × désaccord juste valeur / milieu du carnet. 18 stratégies, 15 promues | Seuil 0,88-0,90 gagne, ≥ 0,85-0,86 perd : **surajustement du seuil**. X-P88-Dno : 17 fenêtres remplies par semaine, toutes gagnantes (calibrée : 70 remplies, 1 perte de −10 $ le 18/10). **Zéro fill** sur les 84 fenêtres de confirmation. D'où R7 | À revoir | `:177-185, 437-463` |
| **b05-traversee-penny** | Borne « traversée » ; compare juste valeur − décalage et « penny » (meilleur bid + 1 tick). 17 stratégies, 2 promues | **LT-REF** (= X-P88-Dno) : `sel` +0,13 [0,08 ; 0,19], `test` +0,19 [0,11 ; 0,28], 55 fenêtres remplies, 0 perte, +10,3 % à f 5 %, drawdown 0, 8 jours sur 8, calibrée +0,21, retenue 1,5 s +0,06. « Penny » : plus de fills mais des pertes ; toute la fenêtre −85 % | **LT-REF invalidé comme résultat robuste** (§4) | `:186-200, 474-499` |
| **b06-traversee-affinage** | Seuil × entrée × décalage autour de LT-REF. 20 stratégies, 20 promues | Plateau à 270 s (seuils 0,88/0,90, décalages 0,005/0,01, désaccord 0,15 passent tous R1). Entrer à 260-265 s ou descendre à ≥ 0,86 ajoute 1 à 3 pertes. R7 reste le point faible (gain ~0,06 $/part contre perte ~0,9 $) ; seuls LT-REF et T-E265-D15 passent R7 | À revoir | `:202-209, 510-538` |
| **b07-retrait-rapide** | Retrait des bids sur mouvement d'OKX (3-12 $ en 1-3 s), toute la fenêtre. 14 stratégies, 0 promue | −31 à −72 % sur la semaine, près de la moitié des fills perdants. L'avantage de vitesse (annulation en 12 ms contre ≥ 150 ms de retenue des preneurs) ne suffit qu'en fin de fenêtre | À revoir. **Impasse** (confirmée par x03/x05) | `:211-213, 539-561` |
| **b08-lt-mise** | LT-REF à mises croissantes. 4 stratégies, 4 promues | f 10 % : +20,7 %, `test` +0,39 [0,22 ; 0,56] ; f 20 % : +36,8 %, +0,61 [0,33 ; 0,89] ; f ≥ 35 % : +60 à +83 % mais R7 échoue (une perte coûterait trop) | À revoir | `:214-222, 562-574` |
| **b09-fin-precoce-sure** | Entrer plus tôt (230-260 s) si l'issue est très sûre. 12 stratégies, 3 promues | Entrée 230-260 s : 1 à 2 pertes, R7 échoue ; à 260 s avec ≥ 0,95-0,97 : 0 perte mais EV basse < 0. Seule amélioration : **S-E270-HI1** (LT-REF qui cote aussi au-delà de 0,999) : 61 fenêtres remplies, 0 perte, R1-R5 et R7 remplis, +21,3 % sur la semaine à f 10 %. Entrée à 270 s = optimum de la famille | À revoir | `:224-231, 631-651` |
| **b10-outsider-fin** | Bids sur l'outsider (3-30 %) en fin de fenêtre. 6 stratégies, 0 promue | 73 pertes sur 74 fills pour le meilleur cas ; −43 à −99 % sur la semaine. Miroir de LT-REF : à 270 s et plus, la juste valeur désigne le gagnant de façon très fiable et les vendeurs qui nous remplissent sur l'outsider sont informés | **Réfuté nettement**, probablement robuste ; à revoir en `exact` pour confirmation | `:233-235, 652-666` |

Au-delà de 100 évaluations sur `test` (fin de b09), poursuivre l'affinage sur la même semaine « n'apporte plus que du risque de surajustement : la suite se joue sur `confirm` » (`EXPLORATION.md:230-231`).

**Candidats figés pour la confirmation (R6)** : `r6-candidats-1` = X-P88-Dno (= LT-REF), X-P88-D0.08, X-P88-E260-D08 ; `r6-candidats-2` = T-P90-E270-O0.01, T-REF-D15 ; `r6-candidats-3` = LT-F0.1, LT-F0.2 (LT-REF à 10 % et 20 %) ; `r6-candidats-4` = S-E270-HI1 ; `r6-candidats-5` = X-P88-D0.08, T-P90-E270-O0.01 (`engine/maker-bt/batches/r6-candidats-*.json`).

**LT-REF**, tel que défini : à partir de 270 s, bid à juste valeur − 0,01 sur le côté dont la juste valeur est entre 0,88 et 0,999, jusqu'à la fin de la fenêtre ; re-cotation suivie, réserve des ordres en vol, sans plafond de liquidité, mise de 10 à 20 % (`EXPLORATION.md:267-269`). Mécanisme supposé : la **vitesse d'annulation** (annuler en 12 ms quand la juste valeur tombe sous 0,88, alors que les preneurs vendeurs sont retenus ≥ 150 ms) et la part déjà connue de la TWAP de règlement dans la juste valeur (`:192-194`).

## 3. Confirmation R6 du 23/10 (265 fenêtres, 22/10 19:20 → 23/10 17:20)

Borne « traversée », capital de départ 1 000 $ (`EXPLORATION.md:275-319`).

| Candidat | Fenêtres jouées | Fills / pertes | $/fenêtre [IC95] | EV basse R7 | Retenue 1,5 s |
|---|---|---|---|---|---|
| LT-REF (f 5 %) | 90 | 4 / 0 | 0,07 [−0,01 ; 0,16] | −11,41 | −0,23 |
| LT-F0.1 | 90 | 4 / 0 | 0,15 [−0,03 ; 0,32] | −22,82 | −0,45 |
| LT-F0.2 | 90 | 4 / 0 | 0,29 [−0,05 ; 0,64] | −45,63 | −0,91 |
| S-E270-HI1 (f 10 %) | 257 | 5 / 0 | 0,06 [−0,01 ; 0,12] | −20,12 | −0,15 |
| T-P90-E270-O0.01 | 90 | 4 / 0 | 0,05 [−0,02 ; 0,12] | −9,19 | −0,19 |
| X-P88-E260-D08 | 44 | 3 / 0 | 0,16 [−0,06 ; 0,39] | −8,95 | −0,44 |
| X-P88-D0.08 | 26 | 1 / 0 | 0,13 [−0,13 ; 0,39] | −19,13 | +0,18 |
| T-REF-D15 | 31 | 1 / 0 | 0,11 [−0,11 ; 0,33] | −19,13 | +0,12 |

- **R6 n'est rempli par aucun candidat.** 1 à 5 fills, aucune perte ; ce qui manque, c'est R7 : un fill gagne ~1,8 $ (f 5 %) alors qu'une perte coûterait ~25 $, soit un rapport d'environ 14 ; il faut ≈ 55 fills sans perte. Pour LT-REF il manquait environ 50 fills (7 jours au rythme de la semaine, 12 jours au rythme observé de ~4,4 par jour) ; pour S-E270-HI1 environ 60 (`:290-300`).
- Écarts avec la semaine d'étude : moins de fills (1,5 % des fenêtres contre 2,6 %, non significatif : P(≤ 4 | 7) ≈ 0,17) ; gain par fill inchangé (~0,18 % du capital à f 5 %) ; ni perte ni drawdown ; calibrée 0,08 (contre 0,21 sur `test`) (`:301-313`).
- **Signal d'alerte** : la retenue de 1,5 s devient négative (LT-REF −0,23 contre +0,06 sur `test`). Cohérent avec le mécanisme supposé (l'edge tient à l'annulation en 12 ms) : toute latence réelle au-delà de quelques centaines de ms annulerait l'edge (`:309-312`).
- Verdict de l'époque : « la stratégie n'est pas encore fiable » ; rien ne la contredit mais 4 fills ne prouvent rien face à un ratio perte/gain d'environ 14 (`:314-317`).

Protocole de confirmation (`:321-339`) : copie incrémentale des octets nouveaux de la session en cours (préfixe vérifié par md5), `fetch.mjs --days 2`, `pack.mjs A`, `pack.mjs B --after 2026-10-22T19:20:00Z`, puis `explore.mjs --confirm`. Le script `confirm.sh` fait ces étapes ; il est lancé à la main par l'utilisateur. Les confirmations de x01, x04 et r6-candidats-5 (25-26/10) portent sur les mêmes 265 fenêtres (jusqu'au 23/10 17:20).

## 4. Pourquoi tout est « à revoir », et le cas LT-REF

**Raisons globales** (`EXPLORATION.md:237-248` ; `WALLETS.md:20-24`) :

1. Bornes biaisées, mesurées sur ordres réels : prudente −97 % de PnL, traversée −19 % (jusqu'à b04 : prudente ; dès b05 : traversée).
2. Rebates maker non comptés.
3. Retenue de 0,15 s appliquée par défaut aux poses à tort.
4. Juste valeur fondée sur un OKX enregistré creux (25 à 100 prix par fenêtre du 16/10 22:00 au 18/10) : voir [`sources-de-prix-et-flux.md`](sources-de-prix-et-flux.md).
5. ~100 évaluations sur les mêmes ~808 fenêtres : faux positifs (règle écrite en `EXPLORATION.md:65-70`).
6. Le 21/10 domine plusieurs résultats.

**Re-test avec le moteur corrigé** (borne `exact`, retenue 0, rebates) :

| Lot | Contenu | Résultat | Source |
|---|---|---|---|
| **x01-retest-exact** | Les 8 candidats figés, juste valeur OKX σ×1,5 comme à l'origine | 0 promu ; `sel` exact entre −0,19 et −0,01 $/fenêtre ; **une perte apparaît** (la borne traversée n'en voyait aucune) ; semaine de −2,1 à +4,5 % sur une trentaine de fenêtres remplies. Confirmation : 2 à 3 fills gagnants, R6 non rempli | `EXPLORATION.md:703-733` ; `WALLETS.md:181` |
| **x02-binance-spot** | Mêmes candidats, juste valeur sur trades Binance spot à la ms (latence supposée 120 ms) | **Tous négatifs** : −1,7 à −5,1 % sur la semaine, 3 à 6 pertes. Le résultat positif venait du spot OKX périmé. Les candidats gardent leur σ×1,5 d'origine (`EXPLORATION.md:263`), pas le σ×1,3 calibré pour Binance : cohérence à vérifier `[à vérifier]` | `:735-750, 248` ; `WALLETS.md:182` |
| **x03-binance-maker** | Rejoindre le meilleur bid si l'écart Binance (σ×1,3) dépasse 0,02/0,04/0,08, avec ou sans retrait directionnel ; mise 2 ou 5 % ; fin de fenêtre recalibrée | **Tous négatifs** : −36 à −87 % sur la semaine en cotation continue, −3 à −4 % en fin de fenêtre. Le maker moyen gagne pourtant ~1 cent par part dans ces tranches : nos fills sont plus adverses que la moyenne | `:752-767` ; `WALLETS.md:183` |
| **x04-mix-frozen** | Candidats figés, juste valeur composite Binance perp + Coinbase (arrivées +113 / +40 ms, σ×1,2) | **X-P88-D0.08 promu** : `sel` +0,11 [0,05 ; 0,17], `test` +0,14 [0,02 ; 0,27], 22 fenêtres remplies, 0 perte, 8 jours sur 8 ; R1-R5 remplis, **R7 échoue**. T-P90-E270-O0.01 échoue R1 (`test` 0,02 [−0,00 ; 0,05]). Confirmation : 2 fills, 0 perte, R6 non rempli | `:769-784, 804-817` ; `WALLETS.md:187-191` |
| **x05-mix-maker** | Makers sur le composite (σ×1,2) | **Tous perdants** (−10 à −87 %), sauf la fin de fenêtre (+0,8 et +1,2 %), non promue | `:786-802` ; `WALLETS.md:192` |

**Verdict de l'auteur** : le signe du résultat de la famille « fin de fenêtre » dépend de la source de prix, et « ce n'est pas encore un résultat robuste » (`WALLETS.md:194`). Il faut juger sur des données nouvelles, pas sur cette semaine déjà très exploitée (`EXPLORATION.md:263`). **Question ouverte** : le flux OKX enregistré est-il vraiment périmé, ou le filtrage (« la capture n'écrit un prix qu'au changement du milieu », `engine/09-endurance.md:66`) explique-t-il les 25-100 prix par fenêtre ? Un mid inchangé n'est pas périmé. Le flux brut OKX est dense (20-35 msg/s) (`engine/01-audit.md:153-157`, `engine/09-endurance.md:66-67`). La cause réelle de la rareté du 16-18/10 n'est pas établie `[à vérifier]` ; elle détermine si LT-REF est invalidé par un spot périmé ou par autre chose.

**Dépendance à la coupure du carnet.** `sim.mjs` ignore les événements au-delà de fin + 2 s et boucle jusqu'à fin + 2,5 s ; les stratégies de fin de fenêtre cotent jusqu'à la fin (« coter jusqu'à la fin », `EXPLORATION.md:165`). Aucune règle de coupure du carnet ni de dernier ordre accepté n'est documentée ni mesurée dans le corpus : tous les résultats de la famille LT-REF sont fragiles de ce fait `[à vérifier]` (`engine/maker-bt/sim.mjs:258,410`).

## 5. Enseignements retenus

- **Coter toute la fenêtre perd** en toute borne et avec tout filtre testé (b01, b07, x03, x05). La perte vient de la sélection adverse (dérive −0,02 à −0,06 $/part dans les 10 s après un fill) et, historiquement, du dépassement de mise (`EXPLORATION.md:112-118`). Diagnostic C4 (décalage 0,03) : gain −0,008 $/part en prudente, +0,012 en calibrée, pour un edge attendu de ~0,03 : la sélection adverse mange presque tout ; 53 % des fills ont lieu dans les 30 premières secondes (`:151-155`).
- **La fin de fenêtre sur le côté quasi gagnant** est la seule famille à rester proche de l'équilibre, mais sa rentabilité dépend de la source de prix et d'une latence d'annulation de quelques dizaines de ms ; R7 n'est jamais rempli.
- **Le côté outsider en fin de fenêtre est réfuté** (b10).
- **Lead-lag** (`leadlag.mjs`, 26/10) : le carnet Polymarket n'intègre que 7-9 % d'un mouvement Binance/Coinbase à son arrivée, ~30 % à +100 ms, la moitié vers 200 ms, > 90 % à 500 ms, soit ~150-200 ms d'avance réelle ; Coinbase de peu devant Binance perp, devant Binance spot ; Bitstamp suit de 1 à 3 s et ne change pas de prix 91 % des secondes ; Chainlink n'est pas suivi à court terme (`:251-257`). Latences supposées : Binance +113 ms (p95 de l'utilisateur), Coinbase +40 ms `[non vérifié]`.
- **Calibration de σ par source** : OKX enregistré ×1,0 (×1,5 sous-confiante), composite perp+Coinbase ×1,2, Binance ×1,3 (`WALLETS.md:155-157,187,303-312`). Les seuils `sideProb` des anciens candidats (« ≥ 0,88 » à σ×1,5) correspondent à environ 0,95 réel `[à vérifier]` avant de comparer des stratégies entre sources.

## 6. Reste à faire (hors périmètre de ce document)

Rejouer b01 à b10 et les confirmations dans le backtest de la migration, en `exact`, retenue 0, rebates, avec la juste valeur appropriée à chaque source ; confirmer sur des jours postérieurs au 22/10 avec ≥ 50 fills (R6/R7) ; lever la question de la coupure du carnet. Voir [`architecture-backtest.md`](../architecture-backtest.md) §7 et §10.
