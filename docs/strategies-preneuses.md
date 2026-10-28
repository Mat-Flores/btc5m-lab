# Stratégies preneuses (taker) : référence, candidates C1/C3/C5/C6, effet de la retenue réelle

**Verdict en une phrase.** La stratégie preneuse de référence est positive en simulation avec la retenue annoncée de 150 ms (OKX : +63,6 $ par position de 100 $, IC95 [+28,8 ; +98,4], 58 fenêtres), mais **la retenue réelle mesurée sur nos ordres (0,19 à 3,5 s) la rend inopérante** : au-delà d'environ 1 s, elle ne gagne plus rien de mesurable, et les 3 ordres réels envoyés ont tous été tués (`engine/10-delai-appariement.md:99-100`, `engine/JOURNAL.md:26`).

Conventions. Chemins relatifs au dépôt source `btc5m-lag-sim` (lecture seule). **Tous les résultats de stratégie de ce document sont simulés** sur des sessions enregistrées, sauf les 3 ordres réels de §6 (`engine/08-strategies.md:3-4`). `[à vérifier]` marque ce qui n'est pas établi. L'utilisateur décide seul de toute mise en réel.

Voir aussi : [`strategies-maker.md`](strategies-maker.md) (C4 et les stratégies maker), [`ordres-et-appariement.md`](ordres-et-appariement.md) (taker delay), [`contraintes-marche.md`](contraintes-marche.md) (frais, tick, résolution), [`sources-de-prix-et-flux.md`](sources-de-prix-et-flux.md), [`donnees-publiques.md`](donnees-publiques.md), [`journal-decisions.md`](journal-decisions.md), [`engine-mesures-phases.md`](engine-mesures-phases.md), [`architecture-backtest.md`](architecture-backtest.md).

---

## 1. Les candidates évaluées (phase 0 → phase 8)

Une stratégie ne compte que **nette des frais actuels** (0,07 × p(1−p) par part : 0,0175 $ à p = 0,50, 0,0063 $ à 0,90, 0,0033 $ à 0,95, calcul) **et du taker delay de 150 ms** (`engine/00-recherche.md:142`). Les résultats publics antérieurs à 2026-08-07 (règlement sur un prix ponctuel), 2026-03-06 (frais sur tous les marchés crypto) ou 2026-09-04 (150 ms) ne valent pas pour le jeu actuel (même ligne).

| # | Stratégie | Constat de la phase 0 | Source |
|---|---|---|---|
| C1 | Arbitrage de latence en preneur : estimer P(Up) depuis le spot, prendre les cotes restées en retard. C'est « notre stratégie » (la référence) | Les gains publics (313 $ → 414-512 k$ en un mois) sont de simples affirmations de presse. Le portefeuille emblématique (0x8dxd) a été reclassé teneur de marché symétrique (94 % d'ordres symétriques) | `engine/00-recherche.md:146` ; `engine/research_notes/Engine réel Polymarket BTC 5m/axe_C_strategies.md:43-57` |
| C2 | Complete-set : acheter Up + Down quand a + b + frais < 1 | En preneur il faut a + b < ~0,965 près de 50/50 (calcul), et la seconde jambe bouge pendant les 150 ms. Rejeté en preneur ; variante maker rattachée à C4 | `engine/00-recherche.md:147,363-371` |
| C3 | Entrée tardive à haute probabilité (≥ 0,90 en fin de fenêtre) | À 0,95, il faut ~95,3 % de réussite pour être à l'équilibre (calcul) ; une perte efface ~20 gains | `engine/00-recherche.md:148` ; `axe_C_strategies.md:111-121` |
| C4 | Tenue de marché + rebates | Voir [`strategies-maker.md`](strategies-maker.md) | `engine/00-recherche.md:149` |
| C5 | Prévision du TWAP de règlement (moyenne déjà accumulée + reste prévu) | Intégrée à C6 ; erreur à mesurer face au règlement officiel sur ≥ 1 000 fenêtres | `engine/00-recherche.md:150` |
| C6 | Juste valeur par loi normale + volatilité réalisée (cœur de la référence) | Aucun backtest public net de frais ; à évaluer en walk-forward après 2026-09-04 | `engine/00-recherche.md:151` |
| C7 | Manipulation du spot au règlement | **Exclue** : risque légal et CGU. Ne jamais faire | `engine/00-recherche.md:152` |

Priorités fixées en phase 0 pour la phase 8 : (1) C4 avec C2 en variante maker ; (2) C6 + C5 ; (3) C3 ; (4) C1 ; rejetés : C2 en preneur, C7 (`engine/00-recherche.md:363-371`).

Principes retenus face au taker delay (discussion du 2026-10-19, `engine/00-recherche.md:434-462`) : pas de contournement par l'interface Polymarket (le délai est appliqué par le moteur d'appariement, `itode: true`, quel que soit le canal) ; le seul vrai contournement est l'ordre non marketable (maker) ; décider sur le carnet de l'instant d'appariement, pas sur celui de la décision (limite anticipée, modèle de réaction du carnet, probabilité d'exécution conditionnelle). Non vérifié : qu'une capture réseau de l'ordre passé par le front confirme le même point d'entrée (`engine/00-recherche.md:434-440`).

## 2. La référence en Rust : parité à zéro écart

**Forme.** `Reference::process(&Event)` est une fonction déterministe de (état, événement) : ni horloge, ni E/S, ni hasard. Elle désigne sa source par son nom de config (`"source": "okx"`). La décision suit l'ancien moteur : `evaluate` (juste valeur, `minEdge`, `minFair`, `jump`, temporisation), `enter` (risque, montant, prix limite), `check_exits` (sortie `converge`). Le remplissage simulé est celui du compte « Réaliste » : FAK sur le carnet enregistré, après `signMs` + RTT p50 + `matchMs`. Chaque opération flottante garde l'ordre d'évaluation de JavaScript (`Math.max/min` propagent NaN, `Math.round` arrondit les égalités vers +∞, les montants passent par `toFixed`) (`engine/08-strategies.md:17-30`).

**Parité.** L'ancien moteur (`src/core`, non modifié) et le portage Rust lisent les mêmes 22 sessions. Comparaison champ par champ, sans tolérance : ordres (instant, fenêtre, côté, prix limite, `makerAmount`, `takerAmount`, latence), remplissages (parts, montant, frais), ratés, refus de risque, clôtures (`engine/08-strategies.md:47-55`).

| Variante | Configuration | Sessions | Ordres | Enregistrements | Écarts |
|---|---|---|---|---|---|
| A | celle de la session (`minEdge` 0,10, `jump` 0,20, `minFair` 0,60, `hold`) | 22 | 414 | 1 228 | **0** |
| B | A + sortie `converge` | 22 | 1 182 | 2 758 | **0** |
| C | dense : `jump` 0, `minFair` 0, `minEdge` 0,05, 3 entrées par fenêtre, `converge` | 22 | 4 975 (4 855 remplis) | 128 831 | **0** |

Source : `engine/08-strategies.md:57-61`. Les trois variantes passent deux fois (binaire du premier rejeu, puis binaire final avec `rttShare`) : 0 écart à chaque fois (`:63-65`). Un premier passage de la plus grande session (753 Mo) avait donné une sortie tronquée ; relancée seule, elle lit 39 968 329 événements, autant que l'ancien moteur, 0 écart : la cause était l'interruption du rejeu, pas le portage (`:72-74`). Le couloir « fastest » de l'ancienne application n'est pas une source de config et a été écarté ; les fenêtres encore ouvertes en fin d'enregistrement ne sont pas réglées par gamma (`:67-70`).

Dans la migration, `strategy/src/reference.rs` reste **inchangé** et le registre refuse la référence si `horizon_s` ≠ 300 (voir [`architecture-backtest.md`](architecture-backtest.md)).

## 3. Délais utilisés pour les candidates

| Maillon | Valeur | Source | Statut |
|---|---|---|---|
| Décision → dernier octet écrit | 0,223 ms | phase 6, p50 | mesuré |
| Aller nœud → CLOB | RTT p50 enregistré ÷ 2 (≈ 12 ms depuis Dublin) | `engine/08-strategies.md:81` | symétrie `[non vérifié]` |
| Taker delay du marché | 150 ms | changelog Polymarket | **non mesuré sur nos ordres** à l'époque (phase 10, voir §6) |
| Traitement du moteur d'appariement | 0 | `engine/08-strategies.md:83` | inconnu `[non vérifié]` |
| Âge des flux | celui de la machine qui a enregistré | `:84` | 10 sessions du nœud de Dublin, 5 du portable (RTT p50 ≈ 35 ms contre ≈ 140 ms) |

Le modèle s'écrit dans la config de la référence : `signMs` 0,223, `rttShare` 0,5 (nouveau, 1 par défaut donc l'ancien moteur à l'identique), `matchMs` 150. L'ordre s'apparie contre le carnet enregistré à cet instant : les makers qui ont réellement retiré leurs cotes pendant le délai sont donc dans les données ; **notre propre ordre n'y est pas** (sa réaction n'est pas modélisée) (`engine/08-strategies.md:86-89`).

## 4. Résultats simulés de la phase 8

Échantillon : 21 sessions réelles du 2026-10-14 au 2026-10-17, toutes postérieures au 2026-09-04 ; la session `2026-10-16T09-00-00_sim` est synthétique (parité seulement) (`engine/08-strategies.md:96-97`). PnL réalisé par position fermée, en $, pour 100 $ de mise, frais compris ; IC95 = moyenne ± 1,96 écart-type / √n (`:99-102`). Le couloir OKX est celui qui compte ; « Toutes » réunit 7 couloirs aux observations non indépendantes (`:104-105`).

### 4.1 Référence, C1 (taker delay), C3 (entrée tardive)

| Stratégie | Couloir | Ordres | Ratés | Positions fermées | Gagnées | Frais | PnL moyen [IC95] | PnL total |
|---|---|---|---|---|---|---|---|---|
| Référence, délais de l'ancien moteur | OKX | 72 | 5 | 65 | 48 | 229,54 | **+79,2** [+38,0 ; +120,5] | +5 149 |
| Référence | Toutes | 400 | 38 | 340 | 225 | 1 083,11 | +30,1 [+17,7 ; +42,5] | +10 229 |
| **C1** = référence + aller + 150 ms | OKX | 74 | 14 | 58 | 43 | 185,64 | **+63,6** [+28,8 ; +98,4] | +3 688 |
| C1 | Toutes | 414 | 110 | 282 | 177 | 893,24 | +18,6 [+6,5 ; +30,8] | +5 257 |
| C3a : `maxTau` 60, `minFair` 0,90, `jump` 0, `minEdge` 0,01, délais C1 | OKX | 40 | 7 | 33 | 27 | 62,37 | +1,6 [−19,3 ; +22,6] | +54 |
| C3a | Toutes | 354 | 56 | 295 | 236 | 484,31 | −5,8 [−12,0 ; +0,4] | −1 706 |
| C3b : comme C3a, `minEdge` 0,03 | OKX | 27 | 3 | 24 | 19 | 60,82 | +5,9 [−21,8 ; +33,6] | +142 |
| C3b | Toutes | 262 | 40 | 220 | 164 | 465,42 | −6,8 [−15,0 ; +1,4] | −1 495 |

Source : `engine/08-strategies.md:109-118`. Sessions du nœud de Dublin, couloir OKX : référence +81,3 [+36,4 ; +126,3] (58 positions), C1 +63,6 [+26,4 ; +100,8] (52 positions) (`:120-125`).

Lecture (`engine/08-strategies.md:127-143`) :

- Le taker delay coûte environ **un cinquième** du PnL de la référence sur OKX (+79 → +64 $ par position) et multiplie les ratés par 2,8 (5 → 14). La référence reste positive et son IC95 ne contient pas 0.
- **Concentration** : la session `2026-10-15T00-20-43` (≈ 11 h) porte 25 des 65 positions de la référence et 43 % de son PnL ; sans elle, il reste 40 positions.
- **Échantillon** : 58 à 65 fenêtres en 4 jours de marché ne suffisent pas à exclure un régime favorable. Rien n'établit que l'edge tient sur une autre semaine.
- **C3 n'a pas d'edge mesurable** : IC95 contient 0 sur OKX, PnL négatif toutes sources. Prix payé médian 0,879 ; 38 des 298 remplissages se font sous 0,50 alors que le modèle voit au moins 0,90 (le marché avait raison, ces positions perdent). Les 4 remplissages à 0,003 (fenêtre `1792022100`) suivent un flux Chainlink enregistré figé 60 s ; le lien avec le strike reste `[non vérifié]`. **Rejetée sous cette forme.**

### 4.2 C6 : juste valeur, calibrage, validation glissante

Score de Brier de la juste valeur de Up, une fois par seconde, face au gagnant officiel ; 450 fenêtres réglées, couloir OKX ; coupe par date (première moitié = apprentissage, seconde = test). Une prévision constante à 0,5 donnerait 0,25 (`engine/08-strategies.md:187-189`).

| `sigmaMult` | Apprentissage | Test | 0-60 s | 60-120 s | 120-240 s | 240-300 s |
|---|---|---|---|---|---|---|
| 1,0 | 0,1431 | **0,1306** | 0,0271 | 0,1076 | 0,1683 | 0,2124 |
| 1,5 (actuel) | 0,1429 | 0,1329 | 0,0293 | 0,1073 | 0,1689 | 0,2149 |
| 2,0 | 0,1465 | 0,1381 | 0,0323 | 0,1119 | 0,1742 | 0,2190 |

Source : `engine/08-strategies.md:191-195`. La validation glissante choisit 1,5 (écart de 0,0002 avec 1,0 à l'apprentissage) ; sur le test, 1,0 fait mieux. L'échantillon effectif est proche de 450 fenêtres, pas de 131 327 secondes. Le modèle est trop prudent au centre (σ × 1,5 : prédit 0,15 → observé 0,08 ; 0,35 → 0,28 ; 0,65 → 0,73). Aucun changement proposé sur 225 fenêtres de test (`:199-205`). Calibrages ultérieurs sur d'autres sources de prix : [`simulateur-maker.md`](simulateur-maker.md).

### 4.3 C5 : TWAP de règlement recalculé face au chiffre officiel

On compare notre TWAP des impressions Chainlink sur les 60 dernières secondes au prix final officiel (le `priceToBeat` de la fenêtre suivante) (`engine/08-strategies.md:209-210`).

| Fenêtres | Nombre | \|Erreur\| p50 / p90 / p99 / max | Issues contredites |
|---|---|---|---|
| Flux enregistré complet (aucun silence > 5 s) | 416 | 0,49 / 2,00 / 7,12 / 16,11 $ | 3 sur 422 (écarts à l'ouverture 0,94, 0,29, 0,05 $) |
| Silence Chainlink > 5 s | 40 | jusqu'à 370 $ | 10 sur 41 |

Source : `engine/08-strategies.md:212-215`. Les gros écarts mesurent l'enregistrement (TWAP figé jusqu'à 60 s sans impression, 45 fenêtres), pas le calcul. L'engine invalide Chainlink après 10 s de silence ; un trou de 5 à 10 s passe encore (`:217-220`). Échantillon (461 fenêtres) sous les 1 000 demandés (`:222`). Voir [`contraintes-marche.md`](contraintes-marche.md) pour la résolution Chainlink.

### 4.4 Données qui manquaient (à l'époque)

Pas de trades enregistrés (donc pas de file maker ni de sélection adverse réelle), délai réel de nos ordres, pool de rebates, continuité de Chainlink (45 fenêtres avec silence > 5 s), volume (461 fenêtres en 4 jours) (`engine/08-strategies.md:224-235`). Les trades ont été enregistrés dès la phase 9 (`engine/JOURNAL.md:36`).

## 5. Papier sur 18,7 h (phase 9)

Run papier sur `dublin-live` : référence, source OKX, capital 1 000 $, 100 $ par entrée, `minEdge` 0,1 ; appariement simulé après aller (12,2 ms) + 150 ms sur le carnet observé (`engine/09-endurance.md:8-17`). Résultats simulés :

| Mesure | Valeur |
|---|---|
| Décisions | 45 : 34 remplies, 11 manquées (prix passé au-delà de la limite pendant les 162 ms) |
| Positions réglées | 34 : 28 gagnantes, 6 perdantes ; 18 Up, 16 Down |
| PnL simulé | **+2 402,42 $** pour 2 519,85 $ engagés et 86,59 $ de frais ; creux maximal −104,9 $ |
| Par position de 100 $ | +70,2 $ [IC95 +19,8 ; +120,7], médiane +72,9, n = 34 |
| Sans la plus grosse position | +48,5 $ [+20,6 ; +76,4], n = 33 |
| Appariement | 162,4 ms après la décision (p50) ; âge du carnet p50 28 / p99 1 963 ms |

Source : `engine/09-endurance.md:71-78`. Comparaison avec la phase 8 : C1 +63,6 $ [+28,8 ; +98,4], n = 58 ; « le papier en direct est du même ordre » (`:80-81`). La plus grosse position (+836 $, 21:39:03) pèse un tiers du résultat : OKX monte avant que Polymarket rattrape ; l'appariement simulé (03,568) tombe avant le rattrapage (`:83-87`). Limites inchangées : file d'attente non modélisée, un preneur plus rapide aurait pu prendre ces niveaux (le taker delay s'applique à tous), 6 positions de moins de 20 $ faute de profondeur (`:89-91`).

## 6. La retenue réelle : pourquoi la stratégie preneuse est inopérante

Le 2026-10-22, trois ordres FAK de 5 pUSD ont été envoyés par l'engine sur le wallet de l'utilisateur, **tous tués**. Mesures depuis `dublin-live` (`engine/10-delai-appariement.md:3-4`).

**Verdict.** Le délai ne vient ni du serveur ni du trajet : il naît chez Polymarket, après la validation, pendant la retenue des ordres preneurs. Elle dépasse nettement les 150 ms annoncés : au moins 0,19 à 0,58 s selon l'ordre, réponse entre 0,35 et 3,5 s. D'autres preneurs passent avant nous et prennent le volume visé (`:6-11`).

| Mesure | Résultat | Source |
|---|---|---|
| Décision → dernier octet écrit | 0,21 à 0,23 ms (3 ordres réels) | `:19` |
| `GET /time` (aller-retour) | p50 25 ms (n = 5) | `:30` |
| `POST /order`, dernier octet → réponse (ordre refusé, solde nul) | p50 32 ms, p90 42 ms, max 573 ms (n = 46) | `:31` |
| Ordre A (limite 0,81, 6,7 parts) | volume sous la limite resté +507 ms ; aucun trade pendant l'attente ; réponse 868 ms | `:42` |
| Ordre B (limite 0,56, 8,9 parts) | +428 ms ; trades continus ; réponse 3 499 ms | `:43` |
| Ordre C (limite 0,77, 6,9 parts) | +193 ms ; un acheteur rafle tout jusqu'à 0,80 à +197 ms ; réponse 348 ms | `:44` |

Le papier (+162 ms) aurait rempli A, B et C ; en réel ce volume a été pris par d'autres. Pour C, l'acheteur passé devant pouvait avoir envoyé son ordre après le nôtre : la retenue n'est ni fixe ni strictement FIFO `[non vérifié : 3 ordres]` (`:46-48`). Il n'y a pas de lots à horloge fixe : 16 014 trades d'une heure, horodatages uniformément répartis dans la seconde (`:52-53`). Documentation officielle : 150 ms depuis 2026-09-04, ordre revalidé puis apparié ou rejeté, ordre retenu non annulable, ordre de passage non documenté (`:57-63`).

**Pourquoi la réalité est plus lente que l'annonce** : sur 18,7 h, 270 sauts d'OKX de plus de 20 $ en 300 ms. Premier trade après le saut : tout trade, p10 135 ms, p50 251 ms, p90 1 254 ms (n = 256) ; achat agressif sur le token favorisé, min 100, p10 **194**, p50 **358**, p90 **1 475** ms (n = 198) (`:67-73`). La retenue de base correspond à l'annonce pour les plus rapides (194 ms au p10), mais la **traîne** n'est pas documentée : une fois sur dix, la première réaction attend plus de 1,4 s ; tous les preneurs qui réagissent au même mouvement sont retenus ensemble puis appariés à la file (`:75-80`). Nous arrivons tard : OKX nous parvient 127 ms après son horodatage, notre ordre sort de la retenue vers +290 ms au mieux, alors que les meilleurs concurrents ont ~100 ms d'avance (`:81-83`). Cause exacte de la traîne : non identifiée `[à vérifier]`.

### Effet sur la stratégie (simulé, capture de 18,7 h rejouée à l'identique, seul le délai d'appariement change)

| Délai | Ordres | Remplis | Gagnés | PnL simulé | Par position, IC95 |
|---|---|---|---|---|---|
| 150 ms (annoncé) | 49 | 37 (76 %) | 31 | +2 447 $ | +66 [+18 ; +114] |
| 450 ms | 49 | 16 (33 %) | 12 | +652 $ | +41 [+1 ; +80] |
| 600 ms | 49 | 14 (29 %) | 10 | +475 $ | +34 [−8 ; +76] |
| 900 ms | 50 | 11 (22 %) | 8 | +374 $ | +34 [−17 ; +85] |
| 1 500 ms | 50 | 6 (12 %) | 3 | +15 $ | +2 [−78 ; +83] |
| 3 500 ms | 50 | 3 (6 %) | 2 | −59 $ | −20 [−104 ; +65] |

Source : `engine/10-delai-appariement.md:90-97`. En réel, 0 ordre sur 3 rempli, ce qui correspond à un délai d'au moins 450 ms. **Au-delà d'environ 1 s, la stratégie preneuse ne gagne plus rien de mesurable** (`:99-100`).

**Ce qui pourrait régler le problème** (non fait) : une source de prix plus rapide (Coinbase arrive à Dublin en 53 ms contre 127 ms pour OKX, ~70 ms gagnés), ou un nœud à Londres (~10 ms, mais Londres est en close-only : voir [`plateformes-et-conformite.md`](plateformes-et-conformite.md)). Même premiers, les ordres restent exposés à la traîne (`engine/10-delai-appariement.md:116-123`). Le test de lead-lag du 2026-10-26 a mesuré ~150-200 ms d'avance réelle sur le carnet pour Binance/Coinbase (voir [`sources-de-prix-et-flux.md`](sources-de-prix-et-flux.md)).

**La seule exécution qui échappe à la retenue est l'ordre non marketable (post-only)** : posé directement dans le carnet, sans frais preneur, avec 20 % de remise maker ; ses annulations ne sont pas retenues non plus (`engine/10-delai-appariement.md:125-129`). **Cette affirmation n'a jamais été mesurée par un ordre maker réel** : aucun ordre maker n'a jamais été envoyé (`engine/11-maker-polymarket.md:119`) `[à vérifier]`. Suite : [`strategies-maker.md`](strategies-maker.md).

## 7. Décisions et impasses

| Date | Décision | Pourquoi | Source |
|---|---|---|---|
| 2026-10-22 | Live coupé à 01:20 UTC (engine et réclamation) : 3 ordres réels, tous tués, aucun trade, solde intact (20,697166 pUSD) | Demande de l'utilisateur : la retenue réelle rend la stratégie preneuse inopérante | `engine/JOURNAL.md:26` |
| 2026-10-22 | **Hyperliquid piloté par Binance écarté** : signal juste (95 à 100 % des sauts suivis ~0,9 s plus tard) mais 1,7 à 3,8 pb de gain contre 9 pb de frais preneur aller-retour ; aucun perpétuel BTC HIP-3 actif ; en maker, 0 remplissage sur 21 signaux, témoin au hasard perdant (−3,6 pb, 0 % de gagnants) | mesures du 2026-10-22 depuis Dublin | `engine/JOURNAL.md:24` |
| 2026-10-22 | **Papertrade écarté** | prix lu au moment de l'exécution (relayeur, blocs HyperEVM de 1 à 2 s, file) ; décote des petits gains | `engine/JOURNAL.md:25` |
| 2026-10-22 | Stratégies maker : aucune recommandée | voir [`strategies-maker.md`](strategies-maker.md) | `engine/JOURNAL.md:23` |

Recommandation de la phase 8 (jamais appliquée au réel) : garder la référence comme stratégie de la phase 9 en papier, enregistrer les trades pendant la phase 9 pour rejuger C4, aucune mise en réel avant la phase 10 (`engine/08-strategies.md:245-247`). La phase 10 a ensuite montré le point de §6.

## 8. Comptes dominants (contexte)

Sur 6 fenêtres consécutives (2026-10-18, 13:40 à 14:10 ET), les plus gros volumes viennent d'une poignée de wallets présents dans chaque fenêtre ; ils sont surtout **makers**, leur PnL cumulé tout compris va de 22 k à 152 k USD ; les frais taker absorbent la majorité de leur PnL brut, les rebates maker en forment une part notable ; rien dans ces données ne mesure leur latence (`engine/research_notes/Engine réel Polymarket BTC 5m/trades_publics.md:78-79`). Méthode de mesure de leur avance : `trades_publics.md:157-180` (proposée, non menée). Voir [`donnees-publiques.md`](donnees-publiques.md).

## 9. Ce qui reste ouvert

- Aucune stratégie preneuse n'a été confirmée sur une autre semaine ni sur des ordres réels remplis.
- La distribution de retenue (p10 0,19 / p50 0,36 / p90 1,5 / max 3,5 s) vient de 3 ordres et du « premier trade après un saut », pas d'une mesure directe `[à vérifier]` (`engine/11-maker-polymarket.md:105`).
- Le « traitement du moteur d'appariement » supposé nul (`engine/08-strategies.md:83`) et la symétrie de l'aller simple restent `[non vérifié]`.
- Dans la migration, la référence est simulée par `SimVenue` avec une retenue paramétrable (`taker_hold`) : voir [`architecture-backtest.md`](architecture-backtest.md) §5.
