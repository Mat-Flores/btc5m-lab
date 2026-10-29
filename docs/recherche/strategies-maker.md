# Stratégies maker : C4 et backtest des 20 finalistes (22/10/2026)

**Verdict.** Aucune stratégie maker n'est recommandée. Aucun des 20 finalistes n'a un PnL positif en borne prudente hors échantillon avec un IC95 qui exclut 0. Le meilleur candidat mesuré, G2-A, fait +1,71 $ par fenêtre [−0,72 ; +4,13] hors échantillon, porté par le seul 21/10 (`engine/11-maker-polymarket.md:11-15`). L'hypothèse « deux côtés, puis annulation du côté perdant dans la dernière minute » (`engine/11-maker-polymarket.md:45`) est **réfutée** (`:47-58`).

**Attention : ces chiffres sont historiques.** Ils ont été produits avec des bornes de fill ensuite reconnues biaisées (prudente : −97 % de PnL sur ordres réels ; voir [`simulateur-maker.md`](../simulateur-maker.md)), **sans rebates** et avec une retenue de 0,15 s appliquée par défaut aux poses (`engine/maker-bt/WALLETS.md:20-24`, `engine/11-maker-polymarket.md:106,125`). Le verdict « Aucune stratégie n'est recommandée » (`engine/11-maker-polymarket.md:11`) est conservé ; il a été confirmé ensuite avec le moteur corrigé (voir [`exploration-maker-b01-b10.md`](exploration-maker-b01-b10.md) et [`wallets-arcsine-certova.md`](wallets-arcsine-certova.md)). Les chiffres ci-dessous servent de référence de méthode et d'ordres de grandeur, pas de PnL attendu.

Conventions. Chemins relatifs au dépôt source `btc5m-lag-sim` (lecture seule). **Tout est simulé ; aucun ordre maker n'a jamais été envoyé** (`engine/11-maker-polymarket.md:3,119`). `[à vérifier]` : non établi.

Voir aussi : [`strategies-preneuses.md`](strategies-preneuses.md), [`ordres-et-appariement.md`](ordres-et-appariement.md), [`contraintes-marche.md`](contraintes-marche.md), [`architecture-backtest.md`](../architecture-backtest.md), [`journal-decisions.md`](../journal-decisions.md).

---

## 1. Pourquoi le maker : ce que la retenue change

L'ordre non marketable (post-only) échappe à la retenue des preneurs ; il est posé dans le carnet sans frais preneur, avec 20 % de remise maker ; ses annulations ne sont pas retenues. La retenue joue alors **pour** le maker : il peut retirer une cotation devenue fausse avant que les preneurs, retenus, ne la touchent (`engine/10-delai-appariement.md:125-129`). Cette affirmation n'a jamais été mesurée par un ordre maker réel `[à vérifier]`. C'est la candidate C4 de la phase 8, restée sans verdict faute de trades enregistrés (`:128-129`).

Contexte public (phase 0) : seule stratégie pour laquelle une mesure on-chain chiffrée existe : en une semaine de février 2026 (15 min, 2 688 marchés), les makers ont collectivement pris +728 501 $ aux preneurs, avant frais et rebates ; le plus gros perdant de la semaine (−139 971 $) est aussi un maker. Le risque est la sélection adverse (`engine/research_notes/Engine réel Polymarket BTC 5m/axe_C_strategies.md:144,152`). Rebate maker : 20 % des frais preneur, au prorata du `fee_equivalent` du maker dans le même marché, payé chaque jour, « at the sole discretion of Polymarket » (formulation de la source, en anglais) (`axe_C_strategies.md:147`). Détail : [`contraintes-marche.md`](contraintes-marche.md).

## 2. C4 en phase 8 : deux bornes sur des carnets sans trades

Modèle de `replay/src/maker.rs` (`engine/08-strategies.md:147-154`) : un bid sur chaque côté à juste valeur − demi-écart, arrondi au pas, jamais marketable ; un changement de cote prend effet un aller plus tard (12,2 ms) ; 10 parts au plus par côté et par fenêtre, aucune cote dans les 10 premières ni les 10 dernières secondes ; frais maker nuls, rebates non comptés. Deux paires Up + Down dans une même fenêtre valent 1 au règlement : c'est C2 en maker.

Les trades n'étaient pas enregistrés, donc un remplissage est encadré, jamais affirmé (`:156-160`) : borne **prudente** (rempli seulement quand un ask enregistré atteint notre prix, dans la limite de sa taille) ; borne **optimiste** (on est aussi premier dans la file, toute taille retirée d'un meilleur bid à notre prix ou en dessous compte comme une vente qui nous touche).

Couloir OKX, 441 fenêtres réglées au plus (`:162-169`) :

| Demi-écart, borne | Remplissages | Fenêtres | Paires | PnL par fenêtre [IC95] | Écart au remplissage ($/part) | Dérive à 10 s ($/part) [IC95] |
|---|---|---|---|---|---|---|
| 0,02, prudente | 1 046 | 437 | 355 | −0,28 [−0,46 ; −0,10] | 0,025 | −0,022 [−0,026 ; −0,017] |
| 0,02, optimiste | 1 371 | 441 | 399 | +0,04 [−0,10 ; +0,17] | 0,025 | −0,012 [−0,016 ; −0,008] |
| 0,05, prudente | 983 | 436 | 301 | −0,00 [−0,24 ; +0,24] | 0,055 | −0,035 [−0,040 ; −0,030] |
| 0,05, optimiste | 1 349 | 441 | 352 | +0,26 [+0,06 ; +0,47] | 0,055 | −0,026 [−0,030 ; −0,022] |

Lecture (`:171-183`) : la **sélection adverse est mesurée** : dans les 10 s qui suivent un remplissage, la juste valeur se déplace contre nous de 0,012 à 0,035 $ par part, soit la moitié ou plus de l'écart capté. Résultat nul ou très faible (−0,28 à +0,26 $ par fenêtre pour 10 parts par côté). Rebates inconnus (le `fee_equivalent` des autres makers n'est pas public). **Edge non démontré.**

## 3. Le backtest de la phase 11 (maker-bt)

Rejeu à la milliseconde des carnets enregistrés sur `dublin-dry-run` : 2 090 fenêtres de 5 min, du 15/10 00:20 au 22/10 19:20 UTC, avec les trades publics de Polymarket sur les mêmes fenêtres. Départ 1 000 $, capital composé (`engine/11-maker-polymarket.md:3-5`). Code : `engine/maker-bt/` (voir [`simulateur-maker.md`](../simulateur-maker.md)).

Découpage : `sel` = fenêtres avant le 2026-10-20 (paramètres choisis en borne prudente), `test` = à partir du 2026-10-20 (808 fenêtres, hors échantillon, sans retouche), `semaine` = un seul run composé sur les 2 090 fenêtres (`engine/maker-bt/results/G2/synthese.md:5-8`). Les 20 stratégies sont réparties en 4 groupes de risque ; 358 combinaisons × 3 bornes ont été évaluées sur `sel` (`engine/11-maker-polymarket.md:62`).

### 3.1 Les 20 finalistes (paramètres choisis sur `sel`)

| Groupe | Finaliste | Idée | Paramètres | Source |
|---|---|---|---|---|
| G1 très prudent (f 1-2 %) | G1-A | Deux côtés, annulation tôt du perdant | f 1 %, décalage 0,05, annulation X = 180 s / Y = 0,7 | `engine/maker-bt/results/G1/synthese.md:19` |
| | G1-B | Deux côtés + retrait sur saut BTC | f 1 %, décalage 0,05, retrait si BTC bouge de plus de 40 $ en 5 s | `:20` |
| | G1-C | Prix fixe figé (paire fusionnable) | f 1 %, prix fixe 0,45 | `:21` |
| | G1-D | Probabilité VWAP 5 s | f 1 %, décalage 0,03 | `:22` |
| | G1-E | Entrée retardée + stop serré | f 1 %, décalage 0,03, entrée dès 30 s, SL 0,05 | `:23` |
| G2 prudent (f 2-5 %) | G2-A | Deux côtés suivis + annulation du perdant | f 2 %, décalage 0,05, X = 120 s, Y = 0,8 | `engine/maker-bt/results/G2/synthese.md:20` |
| | G2-B | Deux côtés + retrait sur saut BTC | f 2 %, décalage 0,05, J = 30 $ (S = 5 s) | `:21` |
| | G2-C | Take profit + stop loss preneurs | f 2 %, décalage 0,03, TP 0,10, SL 0,20 | `:22` |
| | G2-D | Prix fixe figé | f 2 %, prix 0,40, X = 60 s | `:23` |
| | G2-E | Probabilité VWAP 5 s | f 2 %, décalage 0,02, X = 120 s | `:24` |
| G3 équilibré (f 5-10 %) | G3-A | Deux côtés + annulation | f 5 %, décalage 0,03, X = 60 s, Y = 0,8 | `engine/maker-bt/results/G3/synthese.md:22` |
| | G3-B | Deux côtés + retrait sur saut BTC | f 10 %, décalage 0,03, J = 30 $ / S = 5 s, annulation 60 s / 0,9 | `:23` |
| | G3-C | Stop loss | f 5 %, décalage 0,03, SL 0,20, annulation 60 s / 0,9 | `:24` |
| | G3-D | Prix fixe figé | f 10 %, prix 0,40, annulation 60 s / 0,9 | `:25` |
| | G3-E | Probabilité VWAP 5 s | f 5 %, décalage 0,01, annulation 60 s / 0,9 | `:26` |
| G4 agressif (f ≥ 10 %) | G4-A | Deux côtés, annulation très tardive | f 20 %, décalage 0,01, X = 15 s, Y = 0,9 | `engine/maker-bt/results/G4/synthese.md:13` |
| | G4-B | C4 composé sans annulation | f 20 %, décalage 0,01 | `:14` |
| | G4-C | Prix fixe figé | f 10 %, prix 0,49, X = 30 s | `:15` |
| | G4-D | Probabilité VWAP 5 s | f 20 %, décalage 0,03, X = 15 s | `:16` |
| | G4-E | Take profit sans stop | f 10 %, TP 0,10, décalage 0,03 | `:17` |

Fiches complètes de ces 20 finalistes (1 semaine, 1 jour, 1 h glissants, pire et meilleure période, fills par côté, sorties tuées) : `engine/maker-bt/results/fiches.md` (non recopiées ici : évaluées en bornes biaisées).

### 3.2 Récapitulatif par groupe (borne prudente, hors échantillon = 20 au 22/10)

| Groupe | Meilleur hors éch. | PnL / fenêtre hors éch. [IC95] | Calibrée [IC95] | Semaine | DD max | Pire fenêtre | Verdict |
|---|---|---|---|---|---|---|---|
| G1 | G1-A | −0,03 [−1,10 ; +1,04] | +0,38 [−0,30 ; +1,06] | −8,7 % (−87 $) | −9,9 % | −1,5 % | 5/5 éliminés |
| G2 | G2-A | +1,71 [−0,72 ; +4,13] | +1,20 [+0,25 ; +2,15] | +7,1 % (+71 $) | −27,8 % | −12,4 % | G2-A et G2-B gardés, IC95 prudent contient 0 ; 3 éliminés |
| G3 | G3-B | +3,40 [−3,20 ; +10,00] | +4,93 [−0,71 ; +10,58] | +78,4 % (+784 $) | −67,9 % | −20,0 % | G3-B gardé, IC95 contient 0 ; 4 éliminés |
| G4 | G4-E | −0,64 [−9,12 ; +7,84] | −0,00 [−2,88 ; +2,87] | −99,9 % | −99,9 % | −20,0 % | 5/5 éliminés ; f = 20 % ruine toute combinaison |

Source : `engine/11-maker-polymarket.md:19-26`. Par horizon en partant de 1 000 $ (G2-A) : 1 jour, médiane −2,9 % (pire −19,5 %, meilleur +58,6 %) ; 1 heure, médiane 0,0 % (pire −13,0 %, meilleure +13,3 %) ; le mois n'a pas été calculé, à la demande de l'utilisateur (`:14`). **Confiance faible** : l'IC95 contient 0, le gain hors échantillon vient du seul 21/10 (+50,8 %), et une retenue de 1,5 s le rend négatif (−6,42 $ par fenêtre) (`:15`). G2-A : 6 jours de perte d'affilée du 15 au 20/10 avant le +50,8 % du 21/10 (`:75`). G2-C (TP 0,10 + SL 0,20) : −3,23 $ par fenêtre, IC95 entièrement négatif (`engine/maker-bt/results/G2/synthese.md:56`).

**Reclassement par risque mesuré** (seuils sur la semaine en prudente : G1 si DD ≤ 20 % et pire fenêtre ≥ −2 % ; G2 si DD ≤ 30 % et ≥ −10 % ; G3 si DD ≤ 60 % ; G4 au-delà) : G1-A à E restent G1 ; G2-A passe en **G3** (DD 27,8 %, pire fenêtre −12,4 %) ; G2-B reste G2 ; G2-C, D, E passent en G3 (DD 35 %) ; G3-A à E passent en **G4** ; G4 reste G4 (ruine). Le risque réel dépasse le risque prévu dès f = 2 %, à cause du dépassement de mise (§3.5) (`engine/11-maker-polymarket.md:32-43`).

### 3.3 Hypothèse H « deux côtés, puis annulation du côté perdant dans la dernière minute » (`engine/11-maker-polymarket.md:45`) : réfutée

Comparaison à décalage égal sur les mêmes fenêtres avec un témoin sans annulation (mise 5 %) (`engine/11-maker-polymarket.md:47-56` ; `engine/maker-bt/results/H/verdict.md`) :

| Mesure | Résultat |
|---|---|
| Gain de l'annulation (60 s / 90 %) | 0,000 $ par fenêtre en prudente et en optimiste ; ≤ +0,001 $ en calibrée |
| Variante 60 s / 80 % | au mieux +0,035 $ [−0,034 ; +0,104] |
| Annulation déclenchée | 97 à 99 % des fenêtres, presque toujours dès T − 60 s |
| **Côté perdant déjà exécuté avant l'annulation** | **61 à 65 % (décalage 0,02), 68 à 70 % (0,03), 80 à 85 % (0,05)** |
| Stratégie entière, prudente, hors échantillon | −0,139 $ par fenêtre [−2,76 ; +2,49], −48 % de capital |

Mécanisme (`:58`) : le côté perdant est acheté tôt pendant que son prix glisse (prix moyen 0,39) ; quand l'annulation tombe, il ne reste qu'un bid à 0,07 au plus à retirer (0,026 $ par fenêtre). Le côté gagnant est lui aussi acheté tôt (0,48-0,50) : on gagne ~0,52 $ par part sur le gagnant et on perd ~0,39 $ sur le perdant, résultat proche de zéro. Dans G3 et G4, X et Y n'ont aucun effet mesurable (identiques à la 4e décimale).

### 3.4 Effet de chaque paramètre (358 combinaisons × 3 bornes)

| Paramètre | Effet | Source |
|---|---|---|
| **f (mise)** | Dominant. Plus petit = mieux dans les 4 groupes. f = 20 % ruine toute combinaison ; f = 10 % perd 61 à 99,9 %. Une moyenne par fenêtre positive peut coexister avec la ruine en composé | `engine/11-maker-polymarket.md:66` |
| Décalage sous la probabilité | 0,05 meilleur en G1 et G2, 0,03 en G3 ; un décalage plus large augmente la part des deux côtés exécutés sans rendre rentable | `:67` |
| Juste valeur ou VWAP 5 s | La VWAP est la pire dans chaque groupe (suit le marché avec retard, nos bids sont pris quand le prix glisse) | `:68` |
| Re-cotation suivie ou figée | Suivie meilleure (écart 0,57 $ par fenêtre en calibrée, G1) ; les prix fixes figés sont tous éliminés | `:69` |
| Retrait sur saut BTC | Se déclenche rarement à 30-40 $ ; effet non établi (prudente et calibrée en sens opposés) | `:70` |
| Annulation du perdant | Aucun effet | `:71` |
| Stop loss | 85 à 91 % des stops ne vendent pas tout (pas de bid au prix limite après la retenue) ; 0,20 fait moins mal que 0,10 ; ne protège pas | `:72` |
| Take profit | G2-C −3,23 $ par fenêtre ; G4-E négatif dans toutes ses combinaisons | `:73` |
| Entrée retardée, filtres σ et volume | Pas d'effet net mesurable | `:74` |
| Stop journalier | Borne chaque jour, n'empêche pas les journées perdantes de s'enchaîner | `:75` |
| Retenue 1,5 s au lieu de 0,15 s | Fortement négatif : G1-A/B −5,5 $, G2-A/B −6,4 $, G4-A/B −166 $ par fenêtre | `:76` |
| Réclamation à 5 min au lieu de 0 | Effet négligeable avec f ≤ 5 % | `:77` |

### 3.5 Limite majeure : le dépassement de mise

Après une re-cotation, l'ancien ordre reste exécutable pendant la retenue alors que le nouveau est dimensionné sur tout le budget restant. Dans un marché qui glisse, une fenêtre peut dépenser jusqu'à **4 fois sa mise** : 106 $ pour 17 $ prévus en G2-A, et −108,7 % du capital en G4-D (capital final négatif). La règle « une annulation n'évite le fill que si elle a pris effet avant la soumission du preneur » est volontairement prudente ; selon la documentation, l'ordre retenu est revalidé à l'appariement, donc l'annulation protégerait plus tôt `[non vérifié sur nos ordres]`. Un robot réel devrait déduire du budget les ordres encore exécutables ; les drawdowns publiés sont donc probablement pessimistes (`engine/11-maker-polymarket.md:116`). Le levier `reserveInFlight` du simulateur répond à ce point (`engine/maker-bt/sim.mjs:65,228,391-393`).

### 3.6 Calibration trades contre carnets (étape 3)

Sur les carnets de la semaine, 2 façons de compter les fills : carnet seul (bornes de `maker.rs`) et carnet complété par les trades publics (borne « calibrée »). Cotes de référence : C4, 10 parts par côté, 2 090 fenêtres (`engine/maker-bt/calibration.json`, `engine/11-maker-polymarket.md:81`).

| Décalage | Parts / fenêtre (prud. / calib. / optim.) | Deux côtés | PnL / fenêtre [IC95] (prud. / calib. / optim.) | Dérive 10 s ($/part) |
|---|---|---|---|---|
| 0,01 | 22,3 / 22,4 / 22,7 | 85 / 94 / 96 % | −0,44 [−0,62 ; −0,26] / +0,10 [−0,04 ; +0,23] / −0,16 [−0,32 ; +0,00] | −0,031 / −0,014 / −0,020 |
| 0,02 | 22,1 / 22,0 / 22,0 | 82 / 92 / 88 % | −0,36 [−0,55 ; −0,17] / +0,22 [+0,08 ; +0,36] / −0,13 [−0,29 ; +0,03] | −0,034 / −0,017 / −0,026 |
| 0,03 | 21,9 / 21,7 / 21,6 | 76 / 89 / 85 % | −0,25 [−0,47 ; −0,03] / +0,19 [+0,05 ; +0,33] / −0,07 [−0,23 ; +0,10] | −0,037 / −0,019 / −0,030 |
| 0,05 | 20,6 / 21,1 / 20,8 | 67 / 82 / 75 % | −0,20 [−0,41 ; +0,02] / +0,36 [+0,20 ; +0,53] / +0,10 [−0,10 ; +0,30] | −0,046 / −0,025 / −0,036 |

Source : `engine/11-maker-polymarket.md:83-88`.

- **File devant nous** à la pose : médiane 0 part (77 % des poses arrivent sur un niveau vide, souvent au-dessus du meilleur bid), p90 406 parts (`:90` ; `calibration.json` : `queueAtJoin`).
- **Baisses de notre niveau** : seules 12 à 23 % sont des trades ; le reste, des annulations. La borne optimiste, qui compte toute baisse du meilleur bid comme un fill, borne donc le **volume** et non le PnL : les annulations au meilleur bid précèdent les baisses de prix, ce sont les fills les plus adverses (`:91`).
- La calibrée a la sélection adverse la plus faible (−0,014 à −0,025 $/part) et le meilleur PnL, mais n'était pas validée contre de vrais fills à l'époque. La recommandation restait donc fondée sur la prudente (`:92`). La validation sur ordres réels a eu lieu ensuite (calibrée : PnL +12 % sur les ordres des autres makers ; `exact` : −4,3 %) : voir [`simulateur-maker.md`](../simulateur-maker.md).
- Contrôle du simulateur (C4, 1 888 fenêtres, horloge de `maker.rs`, sans retenue) : demi-écart 0,02 → −0,217 $ (prudente) / +0,071 $ (optimiste) ; 0,05 → −0,093 / +0,181 : les 4 valeurs sont dans les fourchettes de `08-strategies.md` §4.2 (`:93-100`).

### 3.7 Contraintes appliquées au simulateur

(`engine/11-maker-polymarket.md:104-112`.) 1) Poses et annulations prennent effet après 12,2 ms ; un changement de carnet daté `ts` ne nous remplit que si notre ordre était vivant à `ts − 0,15 s` (variante pessimiste 1,5 s) ; 2) sorties preneuses TP/SL avec retenue tirée de la distribution mesurée (p10 0,19 / p50 0,36 / p90 1,5 / max 3,5 s) puis FAK après la retenue ; 3) frais maker 0, **sans compter la remise** ; 4) tick lu dans le carnet (0,01, puis 0,001), aucun bid sous 5 parts, jamais marketable ; 5) point de départ C4 reproduit dans les fourchettes ; 6) règlement par le gagnant officiel de gamma (jamais une TWAP recalculée) ; 7) carnet silencieux plus de 3 s = aucun fill, aucune annulation, aucun stop (93 fenêtres concernées ; Chainlink muet plus de 5 s dans 605 fenêtres sur 2 090, plus de 10 s dans 3 : signalées, pas exclues) ; 8) mise = f × capital **libre**, gain revenu après le délai de réclamation (0 s par défaut `[non vérifié]`, 5 min en variante) ; 9) IC95 publié pour chaque borne et période.

### 3.8 Autres limites

(`engine/11-maker-polymarket.md:117-127`.) Une seule semaine avec des trous (73 fenêtres le 16/10, 42 le 17/10) ; période hors échantillon de 2,8 jours dominée par le 21/10 ; les bornes ne sont pas emboîtées en PnL ; la calibrée n'est pas validée contre de vrais fills maker ; juste valeur de l'ancien moteur (OKX moins base glissante, strike = TWAP Chainlink 60 s ; 123 fenêtres sans prix OKX non jouées ; mid OKX enregistré seulement s'il change) ; première session du 15/10 à retard de réception élevé (p90 475 ms), probablement enregistrée hors de Dublin ; deux corrections faites après les grilles (borne calibrée qui comptait des annulations comme des fills, recalculée ; décisions prises au milieu d'un instantané, effet d'au plus +0,02 $ par fenêtre en prudente/optimiste, nul en calibrée) ; remise maker non comptée ; plafond de liquidité 23 à 94 % des fenêtres jouées selon la stratégie ; le mois de trades publics (8 632 fenêtres) téléchargé mais non simulé, à la demande de l'utilisateur.

Jeux de données : voir l'annexe `engine/11-maker-polymarket.md:131-135` (trades publics 8 632 fenêtres du 22/09 au 22/10 ; carnets 15 sessions, 2 090 fenêtres, 641 M d'événements).

## 4. Ce qui a suivi (renvois)

- Exploration de 10 lots de stratégies de fin de fenêtre et critères de fiabilité R1-R7 : [`exploration-maker-b01-b10.md`](exploration-maker-b01-b10.md).
- Correction du simulateur (borne `exact`, retenue 0, rebates) et réplicas de wallets : [`simulateur-maker.md`](../simulateur-maker.md), [`wallets-arcsine-certova.md`](wallets-arcsine-certova.md).
- Verdict du JOURNAL, 2026-10-22 : les stratégies maker sont « aucune recommandée » et annuler le côté perdant en fin de fenêtre « n'a aucun effet (perdant déjà exécuté dans 61 à 85 % des fenêtres) » (`engine/JOURNAL.md:23`).
