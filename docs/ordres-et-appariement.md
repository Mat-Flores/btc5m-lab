# Ordres et appariement (CLOB Polymarket)

**Objet.** Comment un ordre vit chez Polymarket et dans l'engine : cycle de vie et statuts, SDK et appels réseau cachés, signature type 3, machine d'états, taker delay (histoire, doc officielle, enquête de la phase 10, modèle retenu), limites de débit, heartbeats, redémarrages du moteur, délai `MATCHED` → `CONFIRMED`. Les chiffres de marché (frais, tick, taille) sont dans [contraintes-marche.md](contraintes-marche.md).

**Sources couvertes.** `engine/00-recherche.md` (§2.2, §2.6, §4, §5.9-5.10), `engine/01-audit.md` (§3.1, §4), `engine/06-execution.md` (§1-§4), `engine/07-comptabilite.md` (§2), `engine/10-delai-appariement.md`, `research_notes/…/axe_B_ordres.md`, `research_notes/…/delais_sources.md` (§1, 2, 4, 5), `engine/JOURNAL.md`.

**Lecture des citations.** Chemins relatifs au dépôt source `btc5m-lag-sim` (lecture seule). `E/` = `engine/` ; `RN/` = `engine/research_notes/Engine réel Polymarket BTC 5m/` ; `MB/` = `engine/maker-bt/`. Une citation `fichier:N-M` désigne les lignes du fichier. `[à vérifier]` : non établi ou contradictoire.

**Renvois.** [sources-de-prix-et-flux.md](sources-de-prix-et-flux.md) · [strategies-preneuses.md](strategies-preneuses.md) (effet du délai sur la référence) · [strategies-maker.md](strategies-maker.md) · [simulateur-maker.md](simulateur-maker.md) · [securite-et-mode-reel.md](securite-et-mode-reel.md) · [plateformes-et-conformite.md](plateformes-et-conformite.md) · [engine-conception.md](engine-conception.md).

---

## 1. Cycle de vie d'un ordre

- Un ordre est un message signé en EIP-712. Il part par `POST /order`, ou par `POST /orders` pour un lot de 1 à 15. L'appariement est hors chaîne, puis le règlement passe sur Polygon. La réponse HTTP (`matched` + `tradeIDs`) prouve l'appariement hors chaîne ; le seul état final de succès est le trade `CONFIRMED` ; `FAILED` reste possible après `matched`. Source : `RN/axe_B_ordres.md:11`.
- Tous les ordres sont des limites ; un « ordre au marché » est une limite dont le prix permet l'exécution immédiate. L'ordre contient token, sens, prix, taille, expiration et un `timestamp` en ms « utilisé pour l'unicité ». Source : `RN/axe_B_ordres.md:14`.
- L'opérateur du CLOB vérifie la signature, le solde, les autorisations et le pas de prix ; l'appariement est hors chaîne. Source : `RN/axe_B_ordres.md:15`.
- Types : GTC, GTD, FOK, FAK, post-only. GTD exige une expiration au moins 3 minutes plus tard et l'ordre expire 1 minute avant. FAK : défaut du SDK pour les ordres au marché. Ordre au marché : `expiration` = `"0"`. Post-only : seulement GTC ou GTD. Source : `RN/axe_B_ordres.md:18`.
- Corps de `POST /order` : `{deferExec, order:{salt, maker, signer, tokenId, makerAmount, takerAmount, side, signatureType, timestamp, expiration, metadata, builder, signature}, owner, orderType, postOnly}`. `expiration` est dans le corps mais pas dans la structure signée. `deferExec` : `false` par défaut, **sens non documenté** `[à vérifier]`. Source : `RN/axe_B_ordres.md:19`, `:70`.
- Taille maximale : `maxOrderSize = balance − Σ(openOrderSize − filledAmount)`. Source : `RN/axe_B_ordres.md:52`.

### Statuts

| Niveau | Valeurs | Source |
|---|---|---|
| Réponse d'ordre | `live` (dans le carnet), `matched` (appariement immédiat), `delayed` (accepté, pas encore apparié ; montants 0 et listes vides), `unmatched` (API seulement : exécutable mais délai non applicable ; le placement a réussi) | `RN/axe_B_ordres.md:20-26` |
| Champs de réponse | `success, orderID, status, makingAmount, takingAmount, tradeIDs, transactionsHashes, errorMsg` | `RN/axe_B_ordres.md:26` |
| Trade | `MATCHED` (non terminal) → `MINED` (non terminal) → `CONFIRMED` (terminal, succès) ; `RETRYING` ; `FAILED` (terminal). Le canal `user` cite aussi `MATCHED_NOT_BROADCAST`. Pas strictement linéaire. | `RN/axe_B_ordres.md:28-35` ; `E/01-audit.md:241` |
| Canal WS `user` | `wss://ws-subscriptions-clob.polymarket.com/ws/user`, auth `apiKey/secret/passphrase`, événements `order` et `trade` (`fee_rate_bps`, `transaction_hash`, `maker_orders`…), `PING` toutes les 10 s. **Aucune garantie d'ordre, aucun rejeu** : resynchroniser par REST. Le statut `DELAYED` existe aussi sur ce canal. | `RN/axe_B_ordres.md:36-43` ; `RN/delais_sources.md:19` |

- Depuis le 2026-07-24 04:00 UTC, `POST /order` et `POST /orders` renvoient des `tradeIDs` et non plus de hash pour les FAK/FOK appariés ; il faut interroger les trades par `tradeID` jusqu'à obtenir un hash ou `FAILED`. Source : `RN/axe_B_ordres.md:27` ; `RN/delais_sources.md:27`, `:146`.
- Le dépôt d'origine supposait des hash dans la réponse ; c'est caduc depuis cette date. Source : `RN/axe_B_ordres.md:64`.
- **Comptabiliser sur `CONFIRMED` seulement** et ne jamais renvoyer un ordre après un timeout. Jusqu'à 24,3 % de `matchOrders()` revertés aux heures de pointe (2 037 304 cas analysés, prépublication arXiv:2606.16852), problème jugé partiellement corrigé. Source : `E/00-recherche.md:138`.
- `waitForOrderFillSettlement` du SDK interroge les trades de `tradeIds` jusqu'à `CONFIRMED` ou `FAILED` ; délai par défaut 30 000 ms puis `TimeoutError` ; il ne suit pas les exécutions ultérieures d'un reste posé dans le carnet. Source : `RN/axe_B_ordres.md:44`.

### `MATCHED` → `CONFIRMED`

- Polymarket documente les statuts et **ne publie aucune durée**. Polygon PoS : blocs de 1-2 s, finalité déterministe de 2 à 5 s. Ordre de grandeur attendu : quelques secondes ; **à mesurer** `[à vérifier]`. Source : `RN/delais_sources.md:143-147`, `:150`, `:154-155`.
- Conséquence : le délai pèse sur la disponibilité des fonds et des positions, pas sur le prix d'exécution, fixé dès `MATCHED`. Source : `RN/delais_sources.md:151`.
- `POST /submit` du relayer répond tout de suite sans `transactionHash` depuis le 2026-04-21. Source : `RN/delais_sources.md:146`.

## 2. Post-only, annulations, exemptions

| Fait | Source | Statut |
|---|---|---|
| Le taker delay ne vise que les ordres **marketables**. Un ordre non marketable (GTC/GTD hors du marché, ou `postOnly`) se place directement dans le carnet. | `RN/delais_sources.md:44`, `:47` | Doc officielle |
| Un post-only qui croiserait est **rejeté, pas retardé**. L'engine doit traiter ce rejet. | `RN/delais_sources.md:48`, `:57` | Doc officielle |
| Annulation permise « except while a marketable order is in a pending delay window ». | `RN/delais_sources.md:49` | Doc officielle |
| Aucun compte ni programme (market maker, rebates, builders, niveaux de volume) n'est dispensé du délai. Rien n'est dit sur un accord privé pour les market makers (la page renvoie au support). | `RN/delais_sources.md:44`, `:50`, `:58`, `:62` | Doc / absence constatée |
| Qu'un post-only « passe sans attente » : déduction de la définition de « marketable ». | `RN/delais_sources.md:61` ; `E/00-recherche.md:127` | **[à vérifier]** |
| Des ordres d'arcsine posés 11 à 140 ms avant un trade sont remplis ; les annulations sont indécidables sur nos données. | `MB/WALLETS.md:43` | Mesuré sur un tiers |
| **Aucun ordre maker n'a jamais été envoyé par nous.** | `E/11-maker-polymarket.md:119` | Établi |

- Le moyen conforme d'échapper au taker delay est d'envoyer des ordres non marketables ; le prix est le risque de non-exécution et la sélection adverse, car le délai sert justement à protéger les makers. Source : `RN/delais_sources.md:56`, `:214` (synthèse).
- La retenue joue **pour** un teneur de marché rapide, qui peut retirer une cotation devenue fausse avant que les preneurs retenus la touchent. Source : `E/10-delai-appariement.md:125-128`.
- **[à vérifier]** `E/10-delai-appariement.md:125-127` écrit que les annulations du post-only ne sont pas retenues ; `MB/WALLETS.md:43` établit seulement que les poses ne le sont pas (sur arcsine) et que les annulations ne se tranchent pas sur nos données. Le simulateur applique une règle prudente (`E/11-maker-polymarket.md:104`, `:116`).

## 3. SDK officiel et appels réseau cachés

- SDK actuels : `@polymarket/client` (TS) et `polymarket-client` (Python), version 0.13. Le Rust unifié est « en développement » ; les anciens clients Rust ne fonctionnent plus (`rs-clob-client` archivé le 2026-05-11). Source : `RN/axe_B_ordres.md:110-112`.
- Dans le SDK TS 0.13, `createMarketOrder` **avec `maxPrice`** fait **deux GET en série** au premier ordre sur un token (`/markets-by-token/{assetId}` puis `/clob-markets/{conditionId}`), puis cache (10 min pour `{feeInfo, negRisk, tickSize}`). **Sans `maxPrice`/`minPrice`**, il fait un `GET /book` à chaque ordre. Source : `RN/axe_B_ordres.md:119-123` ; `E/00-recherche.md:128-129`.
- Mesuré à Dublin : premier ordre d'un token 82 ms (34 + 23 ms de GET), `createMarketOrder` à chaud p50 1,16 ms / p99 3,29 ms ; sans `maxPrice` 37 et 28 ms (`GET /book`). Au portable : 1 044 ms et p50 0,53 ms. Source : `E/01-audit.md:171-176`, `:245`.
- Chaque fenêtre est un nouveau marché : le **premier ordre d'une fenêtre** paie les deux GET, sauf cache préchauffé. L'engine ne passe par aucun appel caché (`Clob::warm`, métadonnées lues par le fil froid). Source : `RN/axe_B_ordres.md:132` ; `E/06-execution.md:90`.
- Le SDK 0.13 ne contient ni `itode`, ni `secondsDelay`, ni le statut `delayed` ; l'adaptateur gère ces cas lui-même. Source : `E/01-audit.md:243`.
- Transport : `ky` au-dessus de `fetch` (undici) ; le serveur négocie `h2` derrière Cloudflare (une requête, PoP `ARN`). Source : `RN/axe_B_ordres.md:126`, `:193-194`.
- Pré-signature spéculative : écartée (gain ≤ 1 ms face aux 150 ms). Source : `E/00-recherche.md:132`.

## 4. Signature, wallet et corps d'ordre

- Le wallet de l'utilisateur est un **Deposit Wallet déployé** (relayer : `WALLET`, ni `SAFE` ni `PROXY` ; code présent à l'adresse sur Polygon). Signature **type 3** : l'ordre est enveloppé dans un `TypedDataSign` (ERC-7739) au nom du wallet. Seul ce type est implémenté. Source : `E/06-execution.md:11`, `:25` ; `E/JOURNAL.md:59`.
- Types de signature de la doc : `EOA=0, POLY_PROXY=1, GNOSIS_SAFE=2, DEPOSIT_WALLET=3` ; le schéma de `POST /order` ne liste que 0, 1 et 2 (incohérence de doc). Les wallets Magic/Google sont « legacy » ; Deposit Wallet pour les nouveaux comptes depuis le 2026-05-04. Source : `RN/axe_B_ordres.md:80-81` ; `E/00-recherche.md:193`.
- Contrat d'échange V2 `0xE111180000d2663C0091e4f400237545B87B996B`, chainId 137 ; collatéral pUSD ; CLOB V2 en production le 2026-04-28 (`nonce`, `feeRateBps`, `taker` supprimés ; `timestamp`, `metadata`, `builder` ajoutés ; domaine EIP-712 en version « 2 »). Source : `E/06-execution.md:26` ; `RN/axe_B_ordres.md:82-87`.
- Montants : `makerAmount` = taille (2 décimales) ; `takerAmount` = taille ÷ prix (achat) ou × prix (vente), **arrondi vers le haut** à 4 décimales (pas 0,01) ou 5 (pas 0,001). L'arrondi au plus proche en rate 12 vecteurs sur 80, l'arrondi vers le bas 36. Source : `E/06-execution.md:27`.
- Corps exact : champs triés, `salt` en nombre, `expiration` `"0"`, `deferExec: false` (requête capturée du SDK). Source : `E/06-execution.md:28`.
- Authentification : L1 = signature EIP-712 `ClobAuth` (création ou dérivation de clé API, une fois) ; L2 = HMAC-SHA256 de timestamp + méthode + chemin, sans les paramètres de requête. Source : `RN/axe_B_ordres.md:78-79`. La signature L2 d'un GET couvre le chemin ; la place de la query n'est pas établie. Source : `E/07-comptabilite.md:27`.
- Sans `wallet` déployé, le SDK déploierait lui-même le Deposit Wallet ; l'engine ne le fait jamais. Source : `E/06-execution.md:31`.
- Vérification : 80 vecteurs générés par le SDK officiel sur un réseau simulé, reproduits octet pour octet. Source : `E/06-execution.md:19`, `:91`.
- **[à vérifier]** Support complet de `POLY_PROXY` (compte Magic) en CLOB V2 : non confirmé (`RN/axe_B_ordres.md:97`).
- Latence locale mesurée (nœud Dublin, 30 min) : décision → dernier octet écrit p50 222,7 µs / p99 288,4 / p99,9 584,6 / max 2 384,7 µs (n = 12 328), dont construction et signature 170,2 µs au p50. Le puits du banc est en TCP simple : le TLS n'est pas mesuré. Source : `E/06-execution.md:58-62`, `:76`.

## 5. Machine d'états et règles d'exécution de l'engine

États : décidé → signé → envoyé → acquitté → apparié → confirmé ; refusé, tué, échoué, inconnu. Source : `E/06-execution.md:14`. Règles :
- **Un ordre n'est écrit qu'une fois** ; la machine n'a aucune transition de renvoi. Source : `E/06-execution.md:36-37`.
- **Ce qui ne dit pas le sort de l'ordre le laisse inconnu** : pas de réponse dans le délai, coupure après l'écriture, 5xx (une passerelle peut échouer après l'appariement), corps illisible, statut inconnu. Un ordre inconnu bloque toute ouverture. Source : `E/06-execution.md:38-40`.
- Il se règle par un fait : réponse tardive, trade du canal `user`, ou `GET /data/order/{id}` ; **rien trouvé ne vaut jamais « absent »**. L'id interrogé à défaut est le hachage EIP-712 de l'ordre `[non vérifié]`. Source : `E/06-execution.md:41-43`.
- Refusé seulement sur réponse explicite (4xx, `success: false`) ou si rien n'a été écrit ; 425 et 429 imposent l'attente de `Retry-After`. Source : `E/06-execution.md:44-45`.
- Seules les tailles des trades de Polymarket comptent, jamais la taille demandée. Source : `E/06-execution.md:46`.
- Un FAK tué revient en **HTTP 400** « no orders found to match with FAK order… » (lu comme `tué`) ; la forme `unmatched` aussi. Source : `E/06-execution.md:30`.
- Attente de la réponse portée de 300 ms à 5 s après l'enquête de la phase 10, pour qu'un ordre lent ne passe plus en « inconnu ». Source : `E/10-delai-appariement.md:106`.
- Comptabilité : la position naît d'un trade de Polymarket ; valorisée dès `MATCHED` (`confirmed: false` jusqu'à `CONFIRMED`) ; un trade `FAILED` disparaît des positions. Source : `E/07-comptabilite.md:31-34`. Le CLOB ne porte qu'un taux (`fee_rate_bps`) ; frais d'entrée lus dans data-api `entry_fees_usdc`, ceux de sortie estimés. Source : `E/07-comptabilite.md:22-23`, `:122-123`.
- Non vérifié sur message réel au moment de la phase 6 : réponse acceptée du CLOB, canal `user`, `GET /data/order`, `orderID` = hachage. Source : `E/06-execution.md:32`, `:106-107`.

## 6. Taker delay

### 6.1 Valeur et historique

- **150 ms depuis le 2026-09-04 14:00 UTC** ; 50 ms du 2026-08-17 11:00 au 2026-09-04 ; 250 ms avant. Source : `RN/delais_sources.md:13-14`, `:30`.
- Les pages « Order lifecycle » et `clob-markets` affichent encore 250 ms : la valeur de 150 ms vient de l'entrée datée la plus récente du changelog. Source : `RN/axe_B_ordres.md:17` ; `E/00-recherche.md:190`.
- Aucune entrée du changelog officiel ne crée le délai de 250 ms, ni le 500 ms, ni sa suppression. Source : `RN/delais_sources.md:15`.
- Historique non officiel (rumeurs, extraits de moteur de recherche) : 500 ms retiré en février 2026, 250 ms « à partir du 2026-02-25 », blocage des annulations « depuis le 2026-06-05 ». Source : `RN/delais_sources.md:20-25`. Statut `[non vérifié]`.
- Sports : 3 s sur les ordres marketables, 1 s en test NBA/MLB (hors périmètre). Source : `RN/delais_sources.md:26`.

### 6.2 Comportement documenté

- Ordres marketables des marchés crypto et finance « up/down » sélectionnés. L'ordre est retenu, la validation est refaite à la fin, puis il est apparié, placé ou rejeté. « The API waits for this hold and returns the final order result. » Pendant la retenue, il ne peut pas être annulé. Source : `RN/delais_sources.md:16`.
- Statut `delayed` : montants « 0 », trade IDs et hashes vides ; `unmatched` : ordre placé sur le carnet après expiration du délai sans appariement. Source : `RN/delais_sources.md:18`. **Jamais observé chez nous** : la réponse HTTP arrive après la retenue. Source : `E/10-delai-appariement.md:59-60`.
- **Détection par marché** : `itode: true` dans `GET /clob-markets/{condition_id}` (mesuré, `secondsDelay` absent de cette réponse ; la durée n'est pas dans le marché). Source : `E/01-audit.md:235`. `E/00-recherche.md:189` laissait la question ouverte ; la mesure la tranche.
- La retenue est un « speed bump » qui protège les makers. Source : `RN/delais_sources.md:32`.
- Les 150 ms ne sont pas un cadencement par lots (16 014 trades d'une heure répartis uniformément dans la seconde). Source : `E/10-delai-appariement.md:50-53`.
- Le délai est appliqué par le moteur d'appariement quel que soit le canal : un bot qui clique dans l'interface web ne l'évite pas (écarté ; une capture réseau via l'outil `rea` le confirmerait). Source : `E/00-recherche.md:436-440`.

### 6.3 Modèle retenu pour un ordre preneur

| Instant | Formule | Ce qu'il détermine |
|---|---|---|
| Appariement | t_décision + signature + **aller** (nœud → moteur, Cloudflare compris) + **délai du marché** + traitement du moteur | Le prix et la quantité exécutés : l'ordre s'apparie contre le carnet de cet instant, pas celui de la décision |
| Connaissance | t_appariement + **retour** | Le moment où l'engine apprend le résultat ; jusque-là l'ordre est « en vol » |
| Confirmation | t_appariement + règlement Polygon | Le passage en « réalisé » (un `FAILED` reste possible) |

Source : `E/00-recherche.md:415-432`. Le taker delay vit uniquement dans l'adaptateur d'exécution Polymarket (réel et simulé) ; le cœur, la stratégie et les autres adaptateurs l'ignorent. Source : `E/00-recherche.md:401-413` ; `E/JOURNAL.md:48`. L'ancien moteur simulait signature + aller-retour + 10 ms, sans les 150 ms : optimiste d'environ 150 ms moins un aller. Source : `E/00-recherche.md:426-428`.

Principes face au délai : seul le maker échappe à la retenue ; anticiper = décider sur le carnet de l'instant d'appariement (limite anticipée, modèle de réaction du carnet, probabilité d'exécution conditionnelle à l'edge pour tenir compte de la sélection adverse). Source : `E/00-recherche.md:442-462`.

### 6.4 Enquête de la phase 10 (3 ordres réels, 2026-10-22)

**Verdict.** Le délai ne vient ni du serveur ni du trajet de l'ordre. Il naît chez Polymarket, après la validation, pendant la retenue ; elle dépasse nettement les 150 ms annoncés (au moins 0,19 à 0,58 s selon l'ordre, réponse entre 0,35 et 3,5 s). Source : `E/10-delai-appariement.md:6-11`.

| Mesure | Résultat | Source |
|---|---|---|
| Notre côté | Décision → dernier octet écrit 0,21 à 0,23 ms ; `TCP_NODELAY`, connexion TLS gardée chaude | `E/10-delai-appariement.md:15-20` |
| Trajet + validation | `POST /order` p50 32 ms, p90 42, max 573 (n = 46, clé jetable à solde nul) ; `GET /time` 25 ms (n = 5) | `E/10-delai-appariement.md:22-33` |
| Ordre A (23:57:57) | Limite 0,81, 6,7 parts ; volume sous la limite resté **+507 ms**, aucun trade pendant l'attente ; réponse **868 ms** | `E/10-delai-appariement.md:42` |
| Ordre B (00:11:20) | Limite 0,56, 8,9 parts ; volume resté +428 ms ; trades continus ; réponse **3 499 ms** | `E/10-delai-appariement.md:43` |
| Ordre C (00:49:36) | Limite 0,77, 6,9 parts ; volume resté **+193 ms** ; un acheteur rafle tout jusqu'à 0,80 à +197 ms ; réponse **348 ms** | `E/10-delai-appariement.md:44` |
| Papier | L'appariement simulé (+162 ms) a rempli A et B et aurait rempli C ; en réel ce volume a été pris par d'autres. Retenue ni fixe ni strictement dans l'ordre d'arrivée `[non vérifié : 3 ordres]` | `E/10-delai-appariement.md:46-48` |
| Premier trade après un saut d'OKX > 20 $ en 300 ms (270 sauts / 18,7 h) | Tout trade : p10 135, p50 251, p90 1 254 ms. Achat agressif sur le favori : min 100, p10 194, p50 358, p90 1 475 ms | `E/10-delai-appariement.md:67-73` |
| Traîne | Une fois sur dix, la première réaction attend plus de 1,4 s ; les preneurs d'un même mouvement sont retenus ensemble puis appariés à la file (ordre A : 1,57 s sans trade, puis 37 trades en 285 ms) | `E/10-delai-appariement.md:77-80` |
| Course | OKX nous parvient 127 ms après son horodatage ; notre ordre sort de la retenue vers +290 ms au mieux ; les meilleurs concurrents ont ~100 ms d'avance ; volume visé mince (6 à 40 parts) | `E/10-delai-appariement.md:81-83` |

**Valeur de la stratégie de référence avec le délai réel (simulé sur les 18,7 h).**

| Délai | Remplis | PnL simulé | Par position de 100 $ [IC95] | Source |
|---|---|---|---|---|
| 150 ms (annoncé) | 37 / 49 (76 %) | +2 447 $ | +66 [+18 ; +114] | `E/10-delai-appariement.md:92` |
| 450 ms | 16 / 49 (33 %) | +652 $ | +41 [+1 ; +80] | `E/10-delai-appariement.md:93` |
| 600 ms | 14 / 49 (29 %) | +475 $ | +34 [−8 ; +76] | `E/10-delai-appariement.md:94` |
| 900 ms | 11 / 50 (22 %) | +374 $ | +34 [−17 ; +85] | `E/10-delai-appariement.md:95` |
| 1 500 ms | 6 / 50 (12 %) | +15 $ | +2 [−78 ; +83] | `E/10-delai-appariement.md:96` |
| 3 500 ms | 3 / 50 (6 %) | −59 $ | −20 [−104 ; +65] | `E/10-delai-appariement.md:97` |

En réel, 0 ordre sur 3 a été rempli, ce qui correspond à un délai d'au moins 450 ms. Au-delà d'environ 1 s, la stratégie preneuse ne gagne plus rien de mesurable. Source : `E/10-delai-appariement.md:99-100`.

**Correctifs appliqués** : attente de réponse portée de 300 ms à 5 s ; délai du papier réglable (`paper.taker_delay_ms`, 600 ms) ; solde écrit au registre seulement s'il change ; sonde de latence de l'ordre. Source : `E/10-delai-appariement.md:102-109`.

**Ce qui peut régler le problème.** Rien côté client ne raccourcit la retenue ni sa traîne ; la place dans la course se gagne par une source de prix plus rapide (Coinbase arrive à Dublin en 53 ms contre 127 ms pour OKX) et un nœud à Londres (~10 ms), mais Londres est exclu (voir [plateformes-et-conformite.md](plateformes-et-conformite.md)). Même premiers, les ordres restent exposés à la traîne. Source : `E/10-delai-appariement.md:114-123`.

**[à vérifier] Cause de la retenue réelle supérieure à 150 ms.** File de vidange après un mouvement, ordre de passage, ou autre : non identifiée (n = 3 ordres). Source : `E/10-delai-appariement.md:6-11`, `:65-83`.

## 7. Limites de débit, heartbeats, redémarrages

| Sujet | Fait | Source |
|---|---|---|
| Couche Cloudflare par IP | Mise en file « delayed/queued », pas de rejet : global CLOB 9 000 / 10 s ; `POST /order` 5 000 / 10 s et 120 000 / 10 min ; `POST /orders` 2 000 / 10 s et 21 000 / 10 min ; `/book` 1 500 / 10 s ; tick size 200 / 10 s ; clés API 100 / 10 s. Durée réelle de la file : non publiée. | `RN/axe_B_ordres.md:174-182`, `:204` ; `RN/delais_sources.md:116`, `:132` |
| Couche par signataire | Un jeton par ordre (y compris dans un lot) ; Standard 40 ordres/s (rafale 60), 80 annulations/s (rafale 120) ; jusqu'à Elite 600/s selon le volume maker sur 30 jours ; 429 + `Retry-After` ; avertissement depuis le 2026-07-24, application réelle « à annoncer ». Un lot est refusé en entier si le seau ne le couvre pas. | `RN/axe_B_ordres.md:183` ; `RN/delais_sources.md:117` ; `E/00-recherche.md:301` |
| Contradiction des pages | « Rate Limits » décrit de la mise en file, « Trading Rate Limits » des 429 : les deux couches coexistent sans doute. | `RN/axe_B_ordres.md:192` ; `E/00-recherche.md:198` |
| Pertinence | Quelques ordres par fenêtre : très loin des seuils ; gérer 425 / 429 / 503 avec `Retry-After`, ne jamais boucler en REST. | `RN/axe_B_ordres.md:199` ; `E/00-recherche.md:136` |
| Lots | `POST /orders` de 1 à 15 ordres, sans clé d'idempotence (un lot ne se réessaie pas) ; utile seulement pour coter deux côtés. `DELETE /orders` jusqu'à 1 000 ids (changelog 2026-06-15) ou 3 000 (référence). | `E/00-recherche.md:135`, `:200` ; `RN/delais_sources.md:118` |
| Heartbeat | Sans battement valide sous 10 s, tous les ordres ouverts (GTC/GTD) sont annulés ; intervalle conseillé 5 s ; concerne seulement les ordres posés. Endpoint : `/v1/heartbeats` (guide) ou `/heartbeats` (référence) **[à vérifier]**. | `RN/axe_B_ordres.md:195` ; `E/00-recherche.md:137`, `:200` |
| Redémarrages du moteur | HTTP 425 pendant le redémarrage, puis post-only 2 min (503 `post_only_mode` avec `Retry-After`), puis 503 en cancel-only ; préavis visé ~2 jours (Telegram, Discord). | `RN/axe_B_ordres.md:45-51` ; `E/00-recherche.md:302` |
| Connexion chaude | Une seule instance de client, keep-alive, `GET /time` périodique ; délai d'inactivité Cloudflare inconnu ; `/time` compte dans le plafond global de 9 000 / 10 s. | `RN/axe_B_ordres.md:200`, `:205` ; `E/00-recherche.md:130` |

## 8. Points [à vérifier]

1. Comportement de **FOK**, sens de `deferExec`, statut `delayed` jamais observé (`RN/axe_B_ordres.md:27`, `:70`).
2. Retenue réelle > 150 ms : cause inconnue ; ordre de passage non FIFO sur n = 3 (`E/10-delai-appariement.md:46-48`, `:65-83`).
3. Annulations de makers face à un preneur retenu ; poses non retenues établies sur arcsine seulement (`MB/WALLETS.md:43` ; `E/10-delai-appariement.md:125-127`).
4. Chemin d'endpoint du heartbeat ; plafond de `DELETE /orders` (`E/00-recherche.md:200`).
5. Durée `MATCHED` → `CONFIRMED` non mesurée (`RN/delais_sources.md:154`) ; taux de trades `FAILED` non mesuré.
6. Symétrie de l'aller (aller = aller-retour / 2) `[non vérifié]` (`E/01-audit.md:193-194`, `:320`).
7. Support de `POLY_PROXY` en CLOB V2 (`RN/axe_B_ordres.md:97`).
