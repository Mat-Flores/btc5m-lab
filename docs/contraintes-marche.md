# Contraintes de marché (Polymarket « Bitcoin Up or Down » 5 min)

**Objet.** Les vérités acquises sur le marché, avec leur niveau de preuve : frais et rebates, tick, taille minimale, types d'ordre, taker delay et délais d'appariement, fenêtres de 5 min, résolution Chainlink, limites des flux, calibrages validés du simulateur maker, résultats b01 à b10 « à revoir ». Les impasses sont gardées, le tâtonnement est supprimé.

**Sources couvertes.** `engine/10-delai-appariement.md`, `engine/11-maker-polymarket.md` (§5-6), `engine/01-audit.md` (§4), `engine/00-recherche.md` (§2.6, §4), `engine/06-execution.md`, `engine/07-comptabilite.md` (§2), `engine/04-flux.md`, `engine/05-okx.md`, `engine/09-endurance.md`, les notes `research_notes` (axe_A, axe_B, axe_C, delais_sources, trades_publics), `engine/maker-bt/WALLETS.md` (§0 et §0 bis), `engine/maker-bt/EXPLORATION.md` (§3, §4, §5), `engine/maker-bt/sim.mjs`, `SPEC.md`, `TODO.md`, `src/core/fees.js`.

**Lecture des citations.** Tous les chemins sont relatifs au dépôt source `btc5m-lag-sim` (lecture seule). Abréviations :
- `E/` = `engine/`
- `RN/` = `engine/research_notes/Engine réel Polymarket BTC 5m/`
- `MB/` = `engine/maker-bt/`

Une citation `fichier:N` ou `fichier:N-M` désigne les lignes du fichier. Les lignes ont été relues dans le dépôt source avant d'être figées.

**Statuts.** *Établi* : texte du marché ou documentation officielle. *Mesuré* : mesure interne. *Calcul* : dérivé d'une formule sourcée. *Décision* : choix du projet. `[à vérifier]` : non établi ou contradictoire, avec les deux sources citées.

**Renvois.** [ordres-et-appariement.md](ordres-et-appariement.md) (cycle de vie des ordres, taker delay en détail), [sources-de-prix-et-flux.md](sources-de-prix-et-flux.md) (flux), [donnees-publiques.md](donnees-publiques.md), [plateformes-et-conformite.md](plateformes-et-conformite.md), [simulateur-maker.md](simulateur-maker.md), [exploration-maker-b01-b10.md](exploration-maker-b01-b10.md), [strategies-maker.md](strategies-maker.md), [strategies-preneuses.md](strategies-preneuses.md), [wallets-arcsine-certova.md](wallets-arcsine-certova.md).

---

## 1. Frais taker et rebates maker (crypto 5 min)

| Fait | Valeur | Source | Statut |
|---|---|---|---|
| Formule taker | `fee = C × feeRate × p × (1−p)`, `feeRate` crypto = 0,07. Le SDK généralise en `rate × (p(1−p))^exposant`. Arrondi à 5 décimales, minimum 0,00001. Makers jamais facturés. Pic de 1,75 $ pour 100 parts à p = 0,50. | `RN/axe_B_ordres.md:146-148` ; `src/core/fees.js:1-4` et `:9-10` (code : arrondi à 1e-5) | Établi (doc officielle + code) |
| Valeurs par part | 0,0175 $ à p = 0,50 (3,5 % du notionnel) ; 0,0063 $ à 0,90 ; 0,0033 $ à 0,95 | `E/00-recherche.md:142` ; `RN/axe_C_strategies.md:34` | Calcul |
| Effet de la nouvelle courbe | À p = 0,9 : 0,0020 $ avec l'ancienne formule contre 0,0063 $ avec la nouvelle. Les entrées tardives à prix élevé sont plus taxées qu'en janvier. | `RN/axe_C_strategies.md:34` | Calcul |
| Paramètres lus sur un marché réel | CLOB : `fd {r 0.07, e 1, to true}` ; gamma : `feeSchedule {rate 0.07, exponent 1, takerOnly, rebateRate 0.2}` | `E/01-audit.md:236` ; `E/00-recherche.md:178` | Mesuré (exposant 1) |
| Exposant | 1 mesuré. L'exemple « 2 » du schéma `fd` ne prouve rien. | `RN/axe_B_ordres.md:147` ; `E/00-recherche.md:192` | Tranché par la mesure |
| Frais lus, pas codés en dur | L'engine lit `fd.r` / `fd.e` sur chaque marché. | `E/00-recherche.md:192` | Décision |
| Un trade du CLOB ne porte qu'un taux | `fee_rate_bps`, jamais un montant. Les frais d'entrée viennent de data-api `entry_fees_usdc` ; ceux de sortie restent estimés. | `E/07-comptabilite.md:22-23` et `:122-123` | Établi |
| Valeur de sortie | Chaque niveau de bid moins `taille × taux × p × (1−p)^e`. | `E/07-comptabilite.md:36-37` | Règle interne |
| Rebate maker | 20 % des frais preneur du marché. `fee_equivalent = C × feeRate × p(1−p)` par ordre maker rempli ; la part du pool est proportionnelle au `fee_equivalent` dans le même marché. Versé chaque jour en pUSD, minimum 1 $, « at the sole discretion of Polymarket ». | `RN/axe_C_strategies.md:147` ; `E/00-recherche.md:149` | Doc officielle |
| Rebate vérifié | 0,2 × 0,07 × p(1−p) × parts par fill, versé le lendemain à 00:45 UTC. 254,60 $ versés pour 254,44 $ calculés ; 477,78 $ pour 477,72 $. | `MB/WALLETS.md:42` | Mesuré (2 jours, un seul wallet) |
| Poids du rebate | Environ +25 % du gain d'arcsine. | `MB/WALLETS.md:24` | Mesuré |
| Rebates dans le simulateur | Comptés dans `rec.pnl` et dans le capital en fin de fenêtre depuis 2026-10-25 ; absents de b01-b10 et de G1-G4. En réalité ils sont versés le lendemain (raccourci). | `MB/WALLETS.md:122` et `:130` ; `MB/sim.mjs:185` et `:240` ; `E/11-maker-polymarket.md:125` | Mesuré |
| Récompenses de liquidité standard | Score `S(v,s) = ((v−s)/v)² · b`, échantillonnage chaque minute, époque d'un jour UTC. Programme spécial « Crypto TWAP Rewards » de 1 M$ en août 2026 : « This program has ended. » | `RN/axe_C_strategies.md:149-150` | Doc. Éligibilité des marchés crypto 5 min aux récompenses standard **[à vérifier]** : non établie dans le corpus. |

**Historique des frais** (changelog officiel, repris par les notes) :
- 2026-01-05 : frais preneur sur les marchés crypto 15 min, « peak at 1.56% at 50% probability » (`RN/axe_B_ordres.md:150`, `RN/axe_C_strategies.md:17`).
- 2026-02-12 : lancement des marchés 5 min avec la même courbe (`RN/axe_B_ordres.md:151`, `RN/axe_C_strategies.md:20`).
- 2026-03-06 : extension à tous les marchés crypto (`RN/axe_B_ordres.md:152`, `RN/axe_C_strategies.md:22`).
- 2026-03-30 : « Fee Structure V2 » ; 2026-03-31 : calcul via l'objet `feeSchedule` du marché (`RN/axe_B_ordres.md:153-154`, `RN/axe_C_strategies.md:22`).

**Points à vérifier sur les frais.**
- **[à vérifier] Date des « frais fixés au moment du match ».** 2026-04-17 dans `RN/axe_C_strategies.md:22` et `RN/delais_sources.md:52` ; 2026-04-28 dans `RN/axe_B_ordres.md:155` (le 04-28 est aussi la date de CLOB V2, `RN/axe_A_sources.md:37`). Contradiction non tranchée.
- **[à vérifier] Passage de `0,25 × (p(1−p))²` à `0,07 × p(1−p)`.** Date absente du changelog, « probablement l'entrée du 30/03/2026 » (`RN/axe_C_strategies.md:23` et `:37`) ; `E/00-recherche.md:192` dit la même chose avec `[non vérifié]`.
- **[à vérifier] Formule du rebate** : vérifiée sur 2 jours d'un seul wallet (`MB/WALLETS.md:42`). Durabilité inconnue : taux « at the sole discretion of Polymarket » (`RN/axe_C_strategies.md:147`).

---

## 2. Tick et taille minimale

| Fait | Valeur | Source | Statut |
|---|---|---|---|
| Pas de prix | CLOB `mts: 0.001` ; gamma `orderPriceMinTickSize: 0.01`. Le CLOB fait foi (le code lit le CLOB). | `E/01-audit.md:238` ; `SPEC.md:88-89` | Contradiction, CLOB retenu. `[à confirmer par un ordre]` (`E/01-audit.md:238`). |
| Changements de pas en cours de fenêtre | `tick_size_change` : 8 en 10 min (0,01 → 0,001). 16 par connexion en 30 min. 1 248 changements de pas sur 1 669 cassures de carnet en 18,7 h. | `E/01-audit.md:146-148` ; `E/04-flux.md:58` ; `E/09-endurance.md:30` | Mesuré |
| Réparation | Après un changement de pas, l'instantané spontané arrive en ~0,9 s ; une resynchronisation forcée répare en 76 ms (p50). | `E/04-flux.md:38` | Mesuré |
| Seuil exact de bascule du pas | Non documenté. Le simulateur lit le pas dans le carnet (0,01, puis 0,001 dès qu'un niveau à 3 décimales apparaît). | `E/11-maker-polymarket.md:107` | **[à vérifier]** |
| Arrondi des montants signés | `makerAmount` = taille à 2 décimales ; `takerAmount` = taille ÷ prix (achat) ou × prix (vente), arrondi vers le haut à 4 décimales (pas 0,01) ou 5 (pas 0,001). | `E/06-execution.md:27` | Mesuré (80 vecteurs du SDK) |
| Taille minimale | `mos: 5` parts. Aucun bid sous 5 parts dans le simulateur. La constante `MIN_SIZE = 5` est commentée comme valeur par défaut de `live.js`, pas comme lecture du marché. | `E/01-audit.md:237` ; `SPEC.md:88` ; `E/11-maker-polymarket.md:107` ; `MB/sim.mjs:37` | Mesuré (une lecture) |
| Notionnel minimum d'un ordre | Non documenté dans le corpus. | (aucune source) | **[à vérifier]** |
| Autres champs du marché | `r {mi 50, ma 4.5, moas 30}` (récompenses), `mbf` / `tbf` 1000, `ao`, `v: v1`. Sens non établi. | `E/01-audit.md:240` | `[non vérifié]` |
| Taille maximale d'un ordre | `balance − Σ(openOrderSize − filledAmount)`. | `RN/axe_B_ordres.md:52` | Doc |
| Taille des ordres réels de l'engine | 5 pUSD tant que le dernier solde lu les couvre (montant en pUSD, à ne pas confondre avec les 5 parts de `mos`). | `E/JOURNAL.md:28` | Décision |
| Collatéral | pUSD ; contrat d'échange V2 `0xE111180000d2663C0091e4f400237545B87B996B`, chainId 137, depuis CLOB V2 du 2026-04-28. | `E/06-execution.md:26` ; `RN/axe_A_sources.md:37` | Établi |

---

## 3. Types d'ordre et comportement

| Fait | Source | Statut |
|---|---|---|
| Tous les ordres sont des limites ; un « ordre au marché » est une limite qui croise. Types : GTC, GTD, FOK, FAK, post-only. GTD : expiration au moins 3 minutes plus tard, l'ordre expire 1 minute avant l'heure fixée. Ordre au marché : `expiration` = `"0"`. Post-only : seulement GTC ou GTD. FAK : défaut du SDK pour les ordres au marché. | `RN/axe_B_ordres.md:14` et `:18` | Doc officielle |
| `POST /order` : `deferExec` vaut `false` par défaut, son sens n'est pas documenté. | `RN/axe_B_ordres.md:19` et `:70` | **[à vérifier]** |
| Comportement détaillé de **FOK** : non décrit dans le corpus au-delà de son nom et de l'absence de hash de transaction. | `RN/axe_B_ordres.md:27` | **[à vérifier]** |
| Post-only qui croiserait : **rejeté, pas retardé**. Aucun compte n'est dispensé du délai. | `RN/delais_sources.md:48` et `:56-58` | Doc officielle |
| Qu'un post-only « passe sans aucune attente » : déduction de la définition de « marketable », jamais confirmé explicitement. | `RN/delais_sources.md:61` ; `E/00-recherche.md:127` | **[à vérifier]** |
| Indice mesuré : des ordres d'arcsine posés 11 à 140 ms avant un trade sont remplis (délai pose → premier fill : 140 ms en médiane). | `MB/WALLETS.md:43` | Mesuré sur un tiers. **Aucun ordre maker n'a jamais été envoyé par nous** (`E/11-maker-polymarket.md:119`). |
| Annulation permise « except while a marketable order is in a pending delay window », c'est-à-dire que l'ordre preneur retenu ne s'annule pas. | `RN/delais_sources.md:49` ; `E/10-delai-appariement.md:62-63` | Doc officielle |
| Annulation d'un maker face à un preneur déjà retenu : le simulateur est prudent (« une annulation n'évite le fill que si elle a pris effet avant la soumission du preneur »). La doc dit que l'ordre retenu est revalidé à la fin. Nos données ne permettent pas de trancher. | `E/11-maker-polymarket.md:104` et `:116` ; `MB/WALLETS.md:43` | **[à vérifier]** |
| Statuts de réponse : `live`, `matched`, `delayed` (montants 0, listes vides), `unmatched` (API seulement). Champs : `success, orderID, status, makingAmount, takingAmount, tradeIDs, transactionsHashes, errorMsg`. | `RN/axe_B_ordres.md:20-26` | Doc officielle. `delayed` **jamais observé** chez nous (`E/10-delai-appariement.md:59-60`). |
| FAK tué : **HTTP 400** « no orders found to match with FAK order… ». Un seul cas observé avant l'engine : réponse 3,6 s après la décision. | `E/06-execution.md:30` | Mesuré |
| Depuis le 2026-07-24 04:00 UTC, FAK/FOK appariés renvoient `tradeIDs` et non plus de hash de transaction. | `RN/axe_B_ordres.md:27` ; `RN/delais_sources.md:27` et `:146` | Doc |
| Statuts de trade : `MATCHED` → `MINED` → `CONFIRMED` (terminal, succès) ; `RETRYING` ; `FAILED` (terminal). `FAILED` reste possible après `matched`. | `RN/axe_B_ordres.md:28-35` ; `E/01-audit.md:241` | Doc officielle |
| Durée `MATCHED` → `CONFIRMED` : non publiée. Polygon : blocs de 1-2 s, finalité de 2-5 s. Attente maximale par défaut du SDK : 30 s. | `RN/delais_sources.md:145-147` et `:154` | Doc. Durée **[à mesurer]**. |
| Heartbeat : sans battement valide sous 10 s, tous les ordres ouverts (GTC/GTD) sont annulés ; intervalle conseillé 5 s. | `RN/axe_B_ordres.md:195` ; `E/00-recherche.md:137` | Doc. Chemin `/v1/heartbeats` ou `/heartbeats` **[à vérifier]** (`E/00-recherche.md:200`). |
| Lots `POST /orders` de 1 à 15, sans clé d'idempotence ; `DELETE /orders` jusqu'à 1 000 ids (changelog du 2026-06-15) ou 3 000 (référence `cancel-multiple`). | `E/00-recherche.md:135` et `:200` ; `RN/delais_sources.md:118` | Plafond d'annulation **[à vérifier]** |
| Redémarrage du moteur : HTTP 425, puis post-only 2 min (503), puis cancel-only (503). | `RN/axe_B_ordres.md:45-51` | Doc |
| Limites de débit : file Cloudflare par IP (`POST /order` 5 000 / 10 s) ; seau par signataire, Standard 40 ordres/s (rafale 60) jusqu'à Elite 600/s ; 429 + `Retry-After`. | `RN/axe_B_ordres.md:174-183` ; `E/00-recherche.md:300-301` | Doc. Application réelle du seau : « will announce ». |

Détail du cycle de vie, des signatures et des garde-fous : [ordres-et-appariement.md](ordres-et-appariement.md).

---

## 4. Taker delay et délais d'appariement mesurés

**Doc officielle.**
- Retenue des ordres preneurs crypto : **150 ms depuis le 2026-09-04 14:00 UTC**. Avant : 50 ms du 2026-08-17 11:00 au 2026-09-04, et 250 ms avant. L'ordre est non annulable pendant la retenue, ses contrôles sont refaits à la fin, il peut être rejeté. Source : `RN/delais_sources.md:13-16`.
- Les pages « Order lifecycle » et `clob-markets` affichent encore 250 ms. La valeur de 150 ms vient de l'entrée datée la plus récente du changelog. Source : `RN/axe_B_ordres.md:17` ; `E/00-recherche.md:190`.
- Historique 500 ms (retiré en février 2026), 250 ms mis en place le 2026-02-25 (miroir) et blocage des annulations « depuis le 2026-06-05 » : rumeurs ou sources secondaires, aucune entrée du changelog officiel. Source : `RN/delais_sources.md:20-25` et `:36`. Statut : `[non vérifié]`.
- Champ de détection : `itode: true` dans `GET /clob-markets/{conditionId}`, **mesuré** ; aucun `secondsDelay` dans cette réponse, et la durée n'est pas dans le marché. Source : `E/01-audit.md:235`. Cela tranche la contradiction restée ouverte en `E/00-recherche.md:189`.
- Pendant la retenue, « The API waits for this hold and returns the final order result » ; `delayed` n'est jamais apparu. Source : `E/00-recherche.md:191` ; `E/10-delai-appariement.md:59-60`.
- Les 150 ms ne sont pas un cadencement par lots : 16 014 trades d'une heure sont répartis uniformément dans la seconde. Source : `E/10-delai-appariement.md:50-53`.
- Aucune exemption de compte ou de programme documentée. Source : `RN/delais_sources.md:44` et `:58`.

**Mesures sur nos ordres (nœud `dublin-live`).**

| Mesure | Résultat | Source |
|---|---|---|
| Décision → dernier octet écrit | 0,21 à 0,23 ms (3 ordres réels). Banc : p50 0,223 / p99 0,288 / p99,9 0,585 / max 2,38 ms (n = 12 328). | `E/10-delai-appariement.md:19` ; `E/06-execution.md:58-62` |
| Trajet + validation d'un `POST /order` | p50 32 ms, p90 42 ms, max 573 ms (n = 46, clé jetable à solde nul). `GET /time` : 25 ms (n = 5). | `E/10-delai-appariement.md:28-33` |
| RTT Dublin → CLOB | Chaud p50 24,4 / p99 29,5 ms ; froid 32,9 / 37,7 ms. Portable : 159 ms chaud. | `E/01-audit.md:185` ; `E/JOURNAL.md:101` |
| Premier `createMarketOrder` d'un token | 82 ms à Dublin (2 GET en série), 1 044 ms au portable ; ensuite p50 1,16 ms / p99 3,29 ms. | `E/01-audit.md:173-174` |
| Les 3 ordres réels (FAK 5 pUSD, tous tués, 2026-10-22) | Réponse après **868 ms**, **3 499 ms** et **348 ms**. Volume sous la limite resté **+507 ms**, **+428 ms**, **+193 ms** : la retenue est donc d'au moins 0,19 à 0,58 s. | `E/10-delai-appariement.md:40-44` |
| Ordre de passage | Non strictement FIFO : un acheteur est passé devant l'ordre C alors qu'il avait pu envoyer le sien après. | `E/10-delai-appariement.md:46-48` ; `[non vérifié : 3 ordres]` |
| Premier trade après un saut d'OKX > 20 $ en 300 ms (270 sauts / 18,7 h) | Tout trade : p10 135, p50 251, p90 1 254 ms. Achat agressif sur le favori : min 100, p10 **194**, p50 **358**, p90 **1 475** ms. | `E/10-delai-appariement.md:67-73` |
| Traîne | Une fois sur dix, la première réaction attend plus de 1,4 s. | `E/10-delai-appariement.md:77-80` |
| Papier : appariement simulé | 12,2 ms (aller) + 150 ms, soit 162,4 ms (p50). | `E/09-endurance.md:16` et `:78` |
| Budget total jusqu'à l'appariement | ≈ 281 ms (source OKX) ou 216 ms (Coinbase) avec 150 ms de retenue. Réel : au moins 450 ms. | `E/01-audit.md:322` ; `E/10-delai-appariement.md:99` |
| Effet sur la stratégie preneuse (18,7 h rejouées) | 150 ms : +66 $ par position de 100 $ [+18 ; +114] ; 450 ms : +41 [+1 ; +80] ; 600 ms : +34 [−8 ; +76] ; 900 ms : +34 [−17 ; +85] ; 1 500 ms : +2 [−78 ; +83] ; 3 500 ms : −20 [−104 ; +65]. | `E/10-delai-appariement.md:90-97` |

**Réserves et contradictions.**
- **[à vérifier]** La distribution de retenue utilisée par le simulateur maker pour les sorties preneuses (p10 0,19 / p50 0,36 / p90 1,5 / max 3,5 s) est dérivée du tableau du premier trade après un saut d'OKX (`E/10-delai-appariement.md:70-73`) et des 3 ordres (`E/11-maker-polymarket.md:105`, `MB/sim.mjs:40`, quantiles `[[0,0.15],[0.1,0.19],[0.5,0.36],[0.9,1.5],[1,3.5]]`). Ce n'est pas une mesure directe de la retenue ; n = 3 ordres.
- La retenue observée (0,19 à 3,5 s) est très supérieure aux 150 ms annoncés. Cause non identifiée : file de vidange après un mouvement, ordre de passage, ou autre (`E/10-delai-appariement.md:6-11`, `:65-83`).
- Le « traitement du moteur d'appariement » est supposé nul : `[non vérifié]` (`E/08-strategies.md:83`).
- **[à vérifier]** Annulations et poses des makers. `E/10-delai-appariement.md:125-127` écrit que les annulations du post-only ne sont pas retenues non plus, alors que `MB/WALLETS.md:43` établit seulement que les *poses* ne sont pas retenues (sur arcsine) et que les annulations sont indécidables sur nos données.
- **[à vérifier]** `E/10-delai-appariement.md:62-63` cite l'annonce du 2026-06-05 (« l'ordre retenu ne peut plus être annulé ») comme fait ; `RN/delais_sources.md:24` la classe en rumeur (extrait de moteur de recherche). La doc officielle « Order lifecycle » dit la même chose (`RN/delais_sources.md:16` et `:49`).
- Sorties preneuses des stratégies maker : 85 à 91 % des stops ne vendent pas tout (`E/11-maker-polymarket.md:72`).

---

## 5. Fenêtres de 5 minutes

| Fait | Valeur | Source | Statut |
|---|---|---|---|
| Fenêtre | `btc-updown-5m-<début en s>`, 300 s, slug déterministe. | `SPEC.md:81` ; `TODO.md:209-210` | Établi |
| Création des marchés | Environ 24 h avant (créé le 2026-10-17 18:18 UTC pour une fenêtre finissant le 10-18 18:15). Gamma publie au moins 1 h avant, jusqu'à +12 fenêtres. | `RN/axe_A_sources.md:99` ; `E/09-endurance.md:46-48` | Mesuré |
| Panne de gamma | ≈ 15:30 à 15:37:23 UTC le 2026-10-21 : la fenêtre de 15:35 est connue 143 s après son ouverture. L'engine cherche désormais 3 fenêtres à l'avance. | `E/09-endurance.md:38-48` ; `E/JOURNAL.md:33` | Mesuré |
| Carnet exploitable avant l'ouverture | ≈ 299 s avant (14/14 tokens ; 223 fenêtres sur 225, 2 incidents gamma). | `E/04-flux.md:61` ; `E/09-endurance.md:28` | Mesuré |
| `closedTime` | ≈ +53 s après la fin (un marché : `closedTime` 18:10:53 pour une fin à 18:10). | `E/00-recherche.md:178` et `:202` ; `RN/trades_publics.md:39` | **[à vérifier]** : 60 à 120 s selon une spécification tierce (`E/00-recherche.md:202`, `RN/trades_publics.md:147`). Échantillons minuscules des deux côtés. |
| **Heure de coupure du carnet / dernier ordre accepté** | **Aucune règle documentée ni mesurée dans le corpus** (recherche négative lors de l'inventaire). | — | **[à vérifier]** |
| Conséquence pour le simulateur | `sim.mjs` ignore les événements au-delà de END + 2 s (ligne 258) et boucle jusqu'à END + 2,5 s (ligne 410). Les stratégies de fin de fenêtre (`quoteEnd: 0`) cotent « jusqu'à la fin ». Aucune source ne dit si le carnet accepte des ordres jusqu'à T+300. | `MB/sim.mjs:258` et `:410` ; `MB/EXPLORATION.md:267-269` | **[à vérifier]**. Critique pour b02-b10 et les candidats LT-REF. |
| Conventions de cotation | Maker de référence (C4) : aucune cote dans les 10 premières ni les 10 dernières secondes. Stop loss : hors dernière minute. | `E/08-strategies.md:151` ; `E/02-conception.md:335` ; `E/07-comptabilite.md:42` | Choix de modèle, pas une règle de marché |
| Strike | Le `priceToBeat` officiel quand il est connu ; sinon TWAP Chainlink de 60 s terminé à l'ouverture. | `SPEC.md:82-84` | Établi (modèle) |
| Publication du `priceToBeat` | « publié après l'ouverture » (`SPEC.md:83` ; `src/core/engine.js:15`) contre « publié après la fin de la fenêtre » (`TODO.md:211`). | cités | **[à vérifier]** (contradiction) |
| Prix final officiel | Le `priceToBeat` de la fenêtre suivante. | `E/08-strategies.md:209-210` | Mesuré |

---

## 6. Résolution Chainlink

| Fait | Valeur | Source | Statut |
|---|---|---|---|
| Flux de règlement | `resolutionSource` = `data.chain.link/streams/btc-usd-twap-60s-streams` ; `cryptoMarketConfig {id btc-5m-twap-60, twapLookbackSeconds 60}`. | `RN/axe_A_sources.md:99` ; `E/01-audit.md:239` | Mesuré (gamma) |
| Règle | Up si le TWAP Chainlink de la plage est « greater than or equal to the price at the beginning of that range ». **L'égalité donne Up.** | `RN/axe_A_sources.md:99` (texte gamma) ; `SPEC.md:85` ; `TODO.md:213` | Établi (texte du marché) |
| Fenêtre TWAP | 30 s du 2026-08-07 au 08-14 ; **60 s depuis le 2026-08-14 00:00 UTC** pour le 5 min. Avant le 08-07 : prix ponctuel. | `RN/axe_A_sources.md:100` ; `RN/axe_C_strategies.md:27-28` | Doc (changelog) |
| Méthode d'échantillonnage du TWAP | Non publiée (bornes, pondération, arrondi, entrées manquantes). | `RN/axe_C_strategies.md:186` ; `RN/axe_A_sources.md:102` et `:150` | Manque. Le déconseil de recalcul vient d'un vendeur : `[non vérifié]`. |
| Définition du « prix au début » | TWAP 60 s avant le début ou point unique ? La règle gamma ne le précise pas. | `RN/axe_A_sources.md:151` | **[à vérifier]** |
| Erreur de notre TWAP recalculé | 416 fenêtres complètes : erreur absolue p50 0,49 $ / p90 2,00 / p99 7,12 / max 16,11 $ ; **3 issues contredites sur 422**. Avec un silence Chainlink > 5 s : jusqu'à 370 $, 10 issues sur 41. | `E/08-strategies.md:212-215` ; `E/JOURNAL.md:117` | Mesuré (n sous les 1 000 visés) |
| Marchés 1 h | Règlement sur la bougie 1H Binance, pas sur le TWAP. Hors périmètre. | `E/00-recherche.md:154` ; `RN/axe_C_strategies.md:29` | `[non vérifié sur page officielle]` |
| Manipulation du spot au règlement | Exclue pour nous (risque légal et CGU). Étude Dai, Jia & Yu (arXiv:2606.31675), ~8,2 M$ capturés, chiffre de seconde main. | `E/00-recherche.md:152` ; `RN/axe_C_strategies.md:188` | Éthique / CGU |
| Utilité d'un signal rapide | Bornée par la moyenne 60 s : un saut à T−5 s ne pèse qu'environ 1/12 de l'issue. | `E/00-recherche.md:146` | Calcul |

---

## 7. Limites des flux et latences

Détail des flux : [sources-de-prix-et-flux.md](sources-de-prix-et-flux.md).

| Source | Fait | Source | Statut |
|---|---|---|---|
| **Polymarket WS `market`** | Pas de numéro de séquence ; `hash` ; un événement peut être découpé sur plusieurs trames de même horodatage ; `PING` toutes les 10 s. | `RN/axe_A_sources.md:20-22` ; `E/04-flux.md:34` | Doc + mesuré |
| | Âge à Dublin (une connexion) : p50 11,5 / p99 276 / max 588 ms. **3 connexions, la plus fraîche** : p50 10,4 / p99 91,3 / p99,9 239 ms ; sur 18,7 h : p50 10,5 / p99 93,8 / p99,9 269,6 / max 2 535 ms. | `E/01-audit.md:206` ; `E/04-flux.md:52` ; `E/09-endurance.md:27` | Mesuré |
| | Le retard dépend du chemin de la connexion : 82 ms contre 9 ms au même instant, jusqu'à 27-28 s de retard sur certaines. | `E/04-flux.md:36` | Mesuré |
| | Une coupure par ~10 min, précédée de 14-15 s de silence ; 212 à 215 coupures par connexion en 18,7 h (≈ 1 par 5 min) ; seuil de silence 3 s ; réparation d'un carnet p50 22,9 / p99 268,7 ms. | `E/01-audit.md:222-223` ; `E/09-endurance.md:30` et `:54` ; `E/04-flux.md:41` | Mesuré |
| **Chainlink (RTDS)** | ~0,97 msg/s ; âge p50 1 411 / p99 2 196 / max 2 814 ms à Dublin ; 1 362 / 2 159 ms sur 18,7 h. Jamais un signal rapide. Historique envoyé à l'abonnement. Seuil de silence 10 s. | `E/01-audit.md:212` ; `E/09-endurance.md:32` ; `E/04-flux.md:32` et `:41` | Mesuré |
| | Coupure planifiée à xx:52:00 toutes les 2 h (7 coupures sur 11) ; 68 trous > 3 s, max 8,8 s en 18,7 h. | `E/09-endurance.md:57` et `:65` | Mesuré |
| | Dans les enregistrements, silence > 5 s dans 605 fenêtres sur 2 090 (> 10 s dans 3) ; notre TWAP figé jusqu'à 60 s dans 45 fenêtres. | `E/11-maker-polymarket.md:110` ; `E/08-strategies.md:217` | Mesuré |
| | RTDS « legacy » depuis le 2026-09-15, retrait « one month after the 0.11.0 release », sans date. PolyBolt : identifiants CLOB L2 obligatoires, `price.crypto` ≈ 1 mise à jour/s (Chainlink par défaut depuis le 2026-10-02), `price.crypto.twap` 60 s. Pas d'adaptateur PolyBolt. | `RN/axe_A_sources.md:105-109` ; `E/04-flux.md:102` | Doc. Cadence du TWAP PolyBolt **[à mesurer]** (`RN/axe_A_sources.md:152`). |
| | Chainlink Data Streams direct : ≥ 150 $/mois/flux, passerelle aux États-Unis. Écarté. | `E/00-recherche.md:112` ; `RN/axe_A_sources.md:116-118` | Décision |
| **OKX `bbo-tbt`** | Tokyo (annonce du 2026-05-14, migration fin juillet 2026). Fournisseur et région non publiés. RTT Dublin → OKX 255,6 / 278,9 ms (chaud). | `RN/axe_A_sources.md:164` ; `E/01-audit.md:187` | Établi. La page Glassnode « AWS ap-east-1 » est probablement périmée (`RN/axe_A_sources.md:166`). |
| | Âge à la réception p50 118 à 127 ms, écart p99 − p50 ≈ 1,8 à 4 ms ; sur 18,7 h : p50 126,3 / p99 139,9 ms. | `E/01-audit.md:210` ; `E/05-okx.md:45` ; `E/09-endurance.md:32` | Mesuré |
| | SWAP : 34,2 msg/s, silence max 1,46 s. Spot : 13,1 msg/s, silence max 2,57 s. Aucun des deux ne devance l'autre. Seuil de silence retenu 3 s (à recalibrer). | `E/05-okx.md:44-46`, `:68-70`, `:79-82` | Mesuré |
| | `seqId` sans `prevSeqId` : aucune perte détectable sur ce canal. Limites : 3 connexions/s par IP, 480 subscribe/h, déconnexion après 30 s de silence. | `E/01-audit.md:153-154` ; `E/05-okx.md:27-29` ; `E/00-recherche.md:315` | Établi |
| **Coinbase / Binance (Dublin)** | Coinbase `ticker` p50 52,8 / p99 151 ms (RTT 125 ms). Binance `aggTrade` p50 106,8 ms (RTT 207 ms). | `E/01-audit.md:188-189` et `:207-208` | Mesuré |
| **Avance sur Polymarket (lead-lag, 26/10)** | Le carnet n'a intégré que 7 à 9 % d'un mouvement Binance/Coinbase à l'arrivée, ~30 % à +100 ms, la moitié vers 200 ms, > 90 % à 500 ms : **~150-200 ms d'avance réelle**. Ordre d'arrivée : Coinbase, de peu devant Binance perp, devant Binance spot. Bitstamp suit de 1 à 3 s. Chainlink n'est pas suivi à court terme. | `MB/EXPLORATION.md:251-256` | Mesuré. Latences supposées : Binance +113 ms (p95 de l'utilisateur), Coinbase +40 ms `[non vérifié]` (`MB/EXPLORATION.md:252`). |
| **Hyperliquid piloté par Binance** | Signal juste (95-100 % des sauts suivis ~0,9 s plus tard) mais 1,7 à 3,8 pb de gain contre 9 pb de frais aller-retour. Aucun perpétuel BTC HIP-3 actif. Maker : 0 remplissage sur 21 signaux. | `E/JOURNAL.md:24` | Mesuré, impasse |

### Flux OKX « creux » : invalidation de LT-REF, et nuance contradictoire

**Version « creux ».**
- Entre 25 et 1 330 prix OKX par fenêtre ; de 25 à 100 du 16/10 22:00 au 18/10, soit un prix toutes les 7 à 27 s. La session `2026-10-17T20-40-17_dry-run` donne 33 prix/min contre 938 pour celle du 15/10 ; écart q90 3,3 s, q99 26 s. Source : `MB/WALLETS.md:146-150`.
- **Conséquence retenue** : toute juste valeur fondée sur l'OKX enregistré était périmée de plusieurs secondes sur une bonne partie de la semaine. Sources : `MB/WALLETS.md:150` ; `MB/EXPLORATION.md:246-248`.
- Corollaire : le résultat positif de LT-REF venait du spot périmé (voir §10).

**Version « dense ».**
- Le flux brut OKX est dense : 20,7 msg/s sur le SWAP en phase 1, jamais plus de 0,82 s sans message (`E/01-audit.md:155-157`) ; 34,2 msg/s dans l'engine, silence max 1,46 s (`E/05-okx.md:44-46`).
- Les sessions n'écrivent un `px` que quand le mid change (`E/01-audit.md:252-253`) ; les trous OKX de la capture (jusqu'à 128 s) « ne sont pas des silences » (`E/09-endurance.md:66-67`) ; le mid OKX n'est enregistré que lorsqu'il change (`E/11-maker-polymarket.md:120`).
- Mesure du 2026-10-20 : en 30 s, 781 messages `bbo-tbt` pour 6 changements du milieu, les autres ne changeant que la taille au meilleur prix. L'anomalie des 231 `px` en 51 min est « expliquée » (`E/JOURNAL.md:124` ; `E/01-audit.md:271-272`).
- Pendant les silences d'enregistrement, 29 des 30 silences OKX > 30 s de la session live tombent pendant un mouvement Chainlink < 20 $ (`E/01-audit.md:265-267`).

**[à vérifier] Cause réelle de la rareté du 16-18/10.** Un mid inchangé n'est pas périmé : si le filtrage au changement de mid explique les silences, la juste valeur OKX n'était pas obsolète, et LT-REF ne serait pas invalidé par un spot périmé. Les deux séries de sources ne sont pas réconciliées dans le corpus. Indice indirect : remplacer OKX par Binance à la ms a changé le signe des résultats (x01 puis x02, §10), mais cela mélange changement de source, de latence et de σ. Non tranché.

**[à vérifier] Engine en direct.** `MB/WALLETS.md:151` demande si l'engine en direct lit le même flux et trade sur le même retard. Les mesures de l'engine (`E/05-okx.md:44-46`, `E/09-endurance.md:32`) portent sur un flux brut dense ; le corpus ne contient pas de test explicite de ce point.

---

## 8. Géoblocage et exploitation (contrainte de marché)

Le détail est dans [plateformes-et-conformite.md](plateformes-et-conformite.md). Faits retenus :
- La localisation est jugée sur le lieu physique (IP), pas sur la résidence. Source : `E/00-recherche.md:333`.
- Serveurs primaires en eu-west-2 (Londres) ; eu-west-1 (Dublin) est la région non restreinte la plus proche. Source : `SPEC.md:311-313` et `:318`.
- GB, FR et US : close-only sur le site et l'API. L'Irlande : close-only sur le site seulement, l'API est ouverte. VPN et contournement interdits. Source : `E/00-recherche.md:332-334`.
- L'avis juridique sur la légalité locale n'est pas étudié. Source : `E/JOURNAL.md:31`, `:73`.
- Nœud de Londres écarté (close-only) : `E/JOURNAL.md:27`.

---

## 9. Calibrages validés

### 9.1 Simulateur maker : borne `exact`, retenue 0, rebates

| Contrôle | Résultat | Source |
|---|---|---|
| 29 579 ordres réels d'arcsine rejoués | PnL prédit **16 645 $** pour un réel de **15 360 $** (+8,4 %). Volume prédit 0,958 fois le réel ; ordres exacts 71,0 %. Accord jour par jour. | `MB/WALLETS.md:14-17`, `:64-78` |
| 59 695 ordres horodatés d'autres makers (35 fenêtres) | 16 581 $ prédits pour 17 331 $ réels (**−4,3 %**), ordres exacts 84,1 %, volume prédit / réel 1,044. | `MB/WALLETS.md:19`, `:52-62` |
| Autres bornes sur ces ordres réels | Prudente −97 % (537 $), traversée −19 % (14 117 $), calibrée +12 % (19 435 $), optimiste −12 % (15 183 $). | `MB/WALLETS.md:56-59` |
| Autres bornes sur arcsine | Traversée −39 % (9 384 $), calibrée −15 % (13 080 $). | `MB/WALLETS.md:66-71` |
| Avec retenue de 0,15 s | 7 943 $ prédits pour 16 383 $ réels (−52 %). La retenue ne vaut pas pour la pose. **Recommandation : `delay: 0` avec la borne `exact`** ; le défaut du code reste 0,15 s (contrainte de l'utilisateur). | `MB/WALLETS.md:23`, `:71`, `:123` ; `MB/EXPLORATION.md:241` |
| Heures data-api | Heures de bloc, retard de 2,5 s en médiane (1,7 à 3,1 s). | `MB/WALLETS.md:33-35` |
| Trades retrouvés dans les deltas `price_change` | 97 % des trades et 96 % du volume, contre 8,6 % de coïncidences avec un décalage témoin de +10 s. | `MB/WALLETS.md:36-38` |
| Matching FIFO ; annulations au prorata | Volume prédit 1,04 fois le réel au prorata, 0,94 si toutes derrière, 1,09 si toutes devant ; la réalité est entre prorata et « toutes derrière ». | `MB/WALLETS.md:39-40` |
| Limites de la validation | Seuls les ordres avec au moins un fill sont visibles ; pose retrouvée par la taille, moins sûr pour les ordres de 5 parts et ~150 parts ; 4 % du volume sans fill prédit ; tests `sim.test.mjs` 8/8. | `MB/WALLETS.md:80-83`, `:119` |
| Constantes du simulateur | Aller 12,2 ms (`ONE_WAY`), silence 3 000 ms, taille min 5, `HOLD_Q` ci-dessus. | `MB/sim.mjs:36-38`, `:40` |
| Contrôle C4 sur 1 888 fenêtres | 4 valeurs dans les plages de `E/08-strategies.md` §4.2. | `E/11-maker-polymarket.md:93-100` |
| Calibration trades / carnets | File devant nous : médiane 0 part, 77 % des poses sur un niveau vide, p90 406 ; 12 à 23 % des baisses de notre niveau sont des trades. | `E/11-maker-polymarket.md:90-91` ; `MB/calibration.json` (`queueAtJoin`) |

**[à vérifier] PnL réel d'arcsine.** 15 360 $ (`MB/WALLETS.md:15` et `:66`) contre 16 383 $ « réels » dans la ligne sur la retenue (`MB/WALLETS.md:23` et `:71`). Les deux lignes ne sont pas réconciliées dans la source (jeux d'ordres différents ?).

**Re-mesure du 2026-10-27 (phase de migration).** `validate.mjs --delay 0 --sim exact` (21 min 42 s) retrouve exactement `MB/WALLETS.md` §0 : 59 695 ordres, réel 1 089 572 parts / PnL 17 331 $ ; exact prédit 1 137 900 parts, ratio 1,044, ordres exacts 0,841, PnL 16 581 $ ; PnL par jour identique à `MB/WALLETS.md:62`. Étape intermédiaire : 71 049 ordres, 66 273 arrivées trouvées, ratio 1,102 ; 63 152 de fin connue, ratio 1,042 ; exacts 0,812. Source : [conception-engine-migration.md:24-25](conception-engine-migration.md) (rapport de conception de la phase 1 de la migration) ; référence figée dans [contrats-migration.md:144-156](contrats-migration.md). Cette valeur est la référence de non-régression (tolérance zéro) du portage Rust, voir [simulateur-maker.md](simulateur-maker.md).

### 9.2 Spot Binance à la milliseconde, σ×1,3

- Les trades Binance BTCUSDT (spot et perpétuel, archives publiques à la ms) remplacent OKX pour la juste valeur, avec une réception supposée 120 ms après la bourse. Source : `MB/WALLETS.md:153-154`.
- Calibration à **σ×1,3** : probabilité 0,93 donne 93 % observé ; 0,48 donne 47-48 % ; 0,02 donne 3-4 %, en début et en milieu de fenêtre. σ×1,0 est trop confiante, σ×1,6 pas assez. Source : `MB/WALLETS.md:155-156`.
- **[à vérifier]** La latence de 120 ms est mesurée pour OKX vers Dublin, non vérifiée pour Binance. Source : `MB/WALLETS.md:154`.
- Composite perpétuel Binance + Coinbase (arrivées à Dublin +113 / +40 ms) calibré à σ×1,2. Source : `MB/WALLETS.md:187` ; `MB/EXPLORATION.md:258`.
- Sur l'OKX enregistré : σ×1,5 (défaut de `sim.mjs`) est fortement sous-confiante (un token coté 0,18 ne gagne que 9 % du temps), σ×1,0 est presque calibrée. Le milieu du carnet est bien calibré, avec un léger biais favori/outsider de 2 à 3 points. Source : `MB/WALLETS.md:303-312`.
- Brier OKX : σ×1,0 test 0,1306, σ×1,5 test 0,1329 ; la validation glissante choisit 1,5 sur l'apprentissage et 1,0 sur le test ; échantillon effectif proche de 450 fenêtres. Source : `E/08-strategies.md:191-201`.
- Le σ optimal dépend donc de la source de prix (OKX enregistré ×1,0, Binance ×1,3, composite ×1,2). Les seuils `sideProb` des anciens candidats (« ≥ 0,88 » à σ×1,5) correspondent à environ 0,95 réel. Source : `MB/WALLETS.md:312`.
- **[à vérifier]** σ du lot x02 : `MB/EXPLORATION.md:247` et `MB/WALLETS.md:155` disent σ×1,3 ; `MB/EXPLORATION.md:263` écrit « Binance spot seul, σ×1,5 : tous perdants ». À réconcilier avant toute comparaison de stratégies entre sources.

---

## 10. Résultats b01 à b10 : tous « à revoir », et pourquoi

**Raison globale.** Source : `MB/EXPLORATION.md:237-243`, `:246-248` ; `MB/WALLETS.md:20-24`.
1. Les bornes utilisées (prudente jusqu'à b04, traversée dès b05) étaient biaisées, mesurées sur ordres réels : −97 % et −19 %.
2. Les rebates maker n'étaient pas comptés.
3. La retenue de 0,15 s était appliquée par défaut, à tort pour les poses.
4. La juste valeur reposait sur un OKX enregistré creux (voir §7, avec la nuance contradictoire).
5. Environ 100 évaluations sur les mêmes ~808 fenêtres hors échantillon produisent des faux positifs (compteur `MB/EXPLORATION.md:126-139` ; règle `MB/EXPLORATION.md:66-68`).
6. Le 21/10 domine plusieurs résultats (`MB/EXPLORATION.md:70` ; `E/11-maker-polymarket.md:15`).

Citation : « tous les résultats des lots b01 à b10 et des confirmations sont à revoir en borne `exact`, retenue nulle » (`MB/EXPLORATION.md:243`).

**Critères de fiabilité (rappel).** R1 PnL moyen par fenêtre > 0 sur `sel` et `test` avec IC95 sur `test` > 0 ; R2 gain non concentré (≥ 5 jours sur 8, aucun jour > 50 %) ; R3 drawdown ≤ 20 % ; R4 borne calibrée > 0 ; R5 indicatif (retenue 1,5 s) ; R7 espérance au bas de l'IC95 de Wilson du taux de gain > 0 ; R6 confirmation sur jours postérieurs au 22/10. Source : `MB/EXPLORATION.md:72-85` (tableau R1-R7, hors R6 défini en `:85`).

| Lot | Contenu | Résultat d'origine | État / pourquoi à revoir | Source |
|---|---|---|---|---|
| **b01-leviers** | 24 stratégies (filtres, favori/outsider, VWAP…) à f 2 %, cotation sur toute la fenêtre. | 23 non promues, −0,6 à −1,8 $/fenêtre. 1 promue : LATE270 (+0,02 $/fen., 6/8 jours, drawdown ≈ 0, tient à 1,5 s). | À revoir. **Impasse robuste** : coter toute la fenêtre perd (confirmé en x03/x05). | `MB/EXPLORATION.md:147-161`, `:348` |
| **b02-fin-de-fenetre** | Affinage de LATE270. | 24 stratégies, 8 promues. R1 échoue de peu (borne basse −0,02 à 0,00). Leviers favorables : décalage 0,01, côté jusqu'à 0,999, entrée 260-275 s. | À revoir (bornes). Dépend de la coupure du carnet (§5, **[à vérifier]**). | `MB/EXPLORATION.md:162-173`, `:381` |
| **b03-fin-combinee** | Combinaison des leviers de b02. | 18 stratégies, 15 promues. La réserve des ordres en vol est indispensable. | À revoir | `MB/EXPLORATION.md:162-173`, `:414` |
| **b04-fin-desaccord** | Seuil du côté × désaccord juste valeur / milieu. | 18 stratégies, 15 promues. Seuil 0,88-0,90 gagne, ≥ 0,85-0,86 perd : **surajustement du seuil**. X-P88-Dno : 17 fenêtres remplies par semaine, toutes gagnantes (calibrée : 70 remplies, 1 perte de −10 $). Aucun fill sur les 84 fenêtres de confirmation. D'où le critère R7. | À revoir | `MB/EXPLORATION.md:177-185`, `:441` |
| **b05-traversee-penny** | Borne « traversée » ; apparition de **LT-REF** (= X-P88-Dno). | 17 stratégies, 2 promues. LT-REF : `sel` +0,13 $ [0,08 ; 0,19], `test` +0,19 [0,11 ; 0,28], 55 fenêtres remplies, 0 perte, +10,3 % à f 5 %. « Penny » en fin de fenêtre : plus de fills mais des pertes. | **LT-REF invalidé** (voir ci-dessous) | `MB/EXPLORATION.md:186-200`, `:478` |
| **b06-traversee-affinage** | Seuil × entrée × décalage. | 20 stratégies, 20 promues. Plateau à 270 s. Entrer à 260-265 s ou ≥ 0,86 ajoute 1 à 3 pertes. R7 reste le point faible (gain ~0,06 $/part contre perte ~0,9 $). | À revoir | `MB/EXPLORATION.md:202-209`, `:514` |
| **b07-retrait-rapide** | Retrait des bids sur mouvement d'OKX (3-12 $ en 1-3 s), toute la fenêtre. | 14 stratégies, 0 promue, −31 à −72 % sur la semaine. L'avantage de vitesse ne suffit qu'en fin de fenêtre. | À revoir. **Impasse** (confirmée par x03/x05). | `MB/EXPLORATION.md:211-213`, `:543` |
| **b08-lt-mise** | LT-REF à mises croissantes. | f 10 % : +20,7 %, `test` +0,39 [0,22 ; 0,56] ; f 20 % : +36,8 % ; f ≥ 35 % : R7 échoue. | À revoir | `MB/EXPLORATION.md:214-222`, `:566` |
| **b09-fin-precoce-sure** | Entrer plus tôt (230-260 s) si l'issue est très sûre. | 12 stratégies, 3 promues. Entrée 230-260 s : 1 à 2 pertes ou EV basse < 0. Seule amélioration : **S-E270-HI1** (61 remplies, 0 perte, +21,3 % à f 10 %). Entrée à 270 s = optimum. | À revoir | `MB/EXPLORATION.md:224-231`, `:635` |
| **b10-outsider-fin** | Bids sur l'outsider (3-30 %) en fin de fenêtre. | 6 stratégies, 0 promue. 73 pertes sur 74 fills, −43 à −99 %. Miroir de LT-REF : le vendeur qui nous remplit est informé. | **Réfuté nettement**, probablement robuste ; à refaire en `exact` pour confirmation. | `MB/EXPLORATION.md:233-235`, `:656` |

**LT-REF invalidé, mais sans conclusion définitive.**
- x01 (moteur corrigé, OKX enregistré, σ×1,5) : tous non promus ; `sel` exact entre −0,19 et −0,01 $/fen. ; une perte apparaît (la borne traversée n'en voyait aucune). Source : `MB/EXPLORATION.md:703-718` ; `MB/WALLETS.md:181`.
- x02 (Binance spot à la ms) : **tous négatifs** (−1,7 à −5,1 % sur la semaine, 3 à 6 pertes). Le résultat positif venait donc du spot périmé. Source : `MB/EXPLORATION.md:735-750`, `:248` ; `MB/WALLETS.md:182`.
- x04 (composite perpétuel + Coinbase, σ×1,2) : X-P88-D0.08 promu (`sel` +0,11 [0,05 ; 0,17], `test` +0,14 [0,02 ; 0,27], 22 remplies, 0 perte), mais **R7 échoue** ; T-P90-E270-O0.01 échoue R1. Source : `MB/EXPLORATION.md:769-784`, `:258-263` ; `MB/WALLETS.md:187-191`.
- Verdict de l'auteur : « Le signe du résultat de la famille "fin de fenêtre" dépend de la source de prix (négative en x02, positive en x04) : ce n'est pas encore un résultat robuste. » Source : `MB/WALLETS.md:194`.
- Confirmation R6 (265 fenêtres, 22/10 19:20 → 23/10 17:20) : 3 à 4 fills sans perte, **R6 non rempli** pour tous les candidats. Il manque environ 50 fills (7 à 12 jours). La retenue à 1,5 s devient négative : toute latence réelle de quelques centaines de ms annulerait l'edge. Source : `MB/EXPLORATION.md:275-319` ; `MB/EXPLORATION.md:804-826`.
- Makers continus (x03, x05) : tous perdants, −10 à −87 %. Nos fills sont plus adverses que ceux du maker moyen. Source : `MB/EXPLORATION.md:752-767`, `:786-802` ; `MB/WALLETS.md:183`, `:192`.

**Verdicts globaux à conserver (impasses).**
- Stratégie preneuse : inopérante avec la retenue réelle (au-delà de ~1 s, plus rien de mesurable). Source : `E/10-delai-appariement.md:99-100`.
- Maker du 22/10 : aucune stratégie recommandée ; annulation du perdant sans effet (perdant déjà exécuté dans 61 à 85 % des fenêtres). Source : `E/11-maker-polymarket.md:11-15`, `:47-58`.
- arcsine ne se réplique pas (meilleur réplica −1,58 $/fenêtre contre +6,37 pour le wallet). CERTova : edge éteint (avril +29,7 k$, mai −1,9 k$), non rejouable. Source : `MB/WALLETS.md:25-29`, `:107-117`, `:209`.
