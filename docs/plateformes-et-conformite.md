# Plateformes concurrentes et conformité

**Objet.** Les plateformes comparées à Polymarket pour le BTC 5 min (Limitless, Kalshi, Overtime, PRDT, Myriad), la matrice plateforme × pays, le géoblocage de Polymarket et ses CGU, la co-location, et les décisions d'hébergement (Dublin, Londres). Ce document ne donne aucun avis juridique : l'éligibilité décrite suit les règles des plateformes, la légalité locale demande un avis juridique que le projet ne remplace pas (`E/JOURNAL.md:73`).

**Sources couvertes.** `engine/00-recherche.md` (§2.7-2.8, §5.1, §5.3), `engine/01-audit.md` (§6), `SPEC.md` (§10), `deploy/README.md` (introduction), `docs/real-trading.md` (§4), `research_notes/…/axe_B_ordres.md` (§6), `research_notes/…/delais_sources.md` (§6), `engine/JOURNAL.md` (décisions pays et plateformes).

**Lecture des citations.** Chemins relatifs au dépôt source `btc5m-lag-sim` (lecture seule). `E/` = `engine/` ; `RN/` = `engine/research_notes/Engine réel Polymarket BTC 5m/`. `fichier:N-M` désigne des lignes. `[à vérifier]` : non établi ou contradictoire.

**Renvois.** [contraintes-marche.md](contraintes-marche.md) · [ordres-et-appariement.md](ordres-et-appariement.md) · [securite-et-mode-reel.md](securite-et-mode-reel.md) · [exploitation-deploiement.md](exploitation-deploiement.md) · [journal-decisions.md](journal-decisions.md).

---

## 1. Plateformes concurrentes

Question posée (2026-10-18) : existe-t-il un concurrent de Polymarket sur le BTC 5 min **sans taker delay** ? Recherche sans compte ni requête authentifiée. Source : `E/00-recherche.md:216-217`.

| Plateforme | Ce que c'est | Taker delay | Règlement | Frais | Liquidité observée | Verdict | Source |
|---|---|---|---|---|---|---|---|
| **Polymarket** (référence) | BTC 5 min, moteur eu-west-2 (Londres) | 150 ms (changelog) ; `itode: true` | TWAP Chainlink 60 s | 0,07 × p(1−p) par part | ≈ 128 M$ en août sur le BTC 5 min (Bitquery) | — | `E/01-audit.md:278-285` |
| **Limitless Exchange** (Base) | Marchés récurrents BTC Up or Down **5 min** (`btc-up-or-down-5-min-<start>`), aussi 15 min et 1 h ; CLOB, API REST + WebSocket, heartbeat maker ; marché adossé à Polymarket (`externalProvider: polymarket`) ; `rebateRate 0.3` ; `minSize 50000000` (unité et rôle non vérifiés) | **250 ms** (`takerDelayMs`), **plus long** que Polymarket | Même TWAP Chainlink 60 s (`btc-usd-twap-60s-streams`) | 0,40 % à 3,00 % | Marché observé : volume 0, bid 0,256, ask 0,475, 20 parts | **Écarté** (accord de l'utilisateur du 2026-10-18) : la recherche initiale « sans délai, règlement ponctuel » était **fausse** ; CGU du 2026-09-15 : trading interdit aux États-Unis, en Chine, en Ontario et en Alberta, France non listée | `E/00-recherche.md:221`, `:319-321` ; `E/01-audit.md:278-285`, `:298` ; `E/JOURNAL.md:47`, `:51` |
| **Kalshi** | Bourse régulée par la CFTC ; BTC **15 min** (`KXBTC15M`) et horaire ; le 5 min n'est affirmé que par des sites d'affiliation ; règlement sur la moyenne 60 s de l'indice CF Benchmarks BRTI ; REST, WebSocket, FIX ; limites dès le palier de base : 200 lectures et 100 écritures par seconde ; moteur probablement en AWS us-east-2 (Ohio) | **Aucun champ de délai** dans les données de marché, aucun documenté ; une demande de délai déposée à la CFTC fin 2025, sans suite connue (simple affirmation) | Moyenne 60 s BRTI | `fee_type: quadratic`, multiplicateur 1 (≈ arrondi au cent de 0,07 × C × P(1−P) selon des blogs `[non vérifié]`) | Fenêtre en cours : 397 007 contrats échangés ; ≈ 2 M de contrats au carnet | **Écarté tant que l'utilisateur n'est pas éligible** : France et Canada restreints ; éligibilité fondée sur la résidence déclarée au KYC. Seule plateforme sans taker delay connu, mais sans 5 min | `E/00-recherche.md:222` ; `E/01-audit.md:278-285`, `:298`, `:300-302` ; `E/JOURNAL.md:127` |
| **Overtime / Thales « Speed Markets »** | UP/DOWN contre un pool de liquidité, au prix Pyth du moment de l'achat, gain ≈ 2× moins les frais | — | Pyth au moment de l'achat | — | — | **Écarté** : pas de carnet, donc pas de prix en retard à exploiter ; durée minimale signalée de 10 min (simple affirmation) | `E/00-recherche.md:223` |
| **PRDT Finance** et paris mutuels | Mises bloquées avant le tour | — | — | — | — | **Écarté** : la latence n'apporte rien (simple affirmation `[non vérifié]`) | `E/00-recherche.md:224` |
| **Myriad** | — | — | — | — | — | **Écarté** : aucun marché BTC 5 min trouvé | `E/00-recherche.md:225` |

- Les mesures de latence de Kalshi (5,6 ms chaud) et Limitless (5,9 ms) viennent d'un cache CDN et ne mesurent pas le moteur. Source : `E/01-audit.md:190-191`.
- Le choix de plateformes de la décision du 2026-10-18 : « Plateforme et pays ouverts » ; les phases 1 et 2 comparent Polymarket, Kalshi et Limitless ; le choix final du pays revient à l'utilisateur. Source : `E/JOURNAL.md:73`.
- **Conclusion de la phase 1** : Kalshi est la seule plateforme sans taker delay connu, mais elle n'a pas de 5 min et la France comme le Canada y sont restreints ; Polymarket reste la seule à combiner 5 min et liquidité. La liste exacte des pays éligibles à Kalshi doit être lue dans son contrat membre (erreur 429 jusqu'ici). Source : `E/01-audit.md:300-302`.
- Kalshi est resté non éligible pour un résident français (pays de résidence déclaré au KYC), même physiquement à Dublin ; à rouvrir seulement si l'utilisateur change réellement de résidence pour un pays éligible. Les tests Kalshi n'ont pas été lancés. Source : `E/JOURNAL.md:127` ; `E/01-audit.md:332-333`.

## 2. Matrice plateforme × pays

Éligibilité selon les règles des plateformes ; la légalité locale demande un avis juridique. Lue le 2026-10-18. Source : `E/01-audit.md:287-296`.

| Pays de résidence et de présence | Polymarket (API) | Kalshi | Limitless |
|---|---|---|---|
| France | Close-only | Restreint | Non listé (CGU) ; risque légal local non étudié |
| Royaume-Uni | Close-only | Restreint | Non listé |
| Irlande | **Ouvert** (restriction sur le site seulement) | Restreint selon le contrat membre `[non vérifié]` | Non listé |
| Canada : Ontario, Québec, Colombie-Britannique, Alberta | Close-only | Restreint | ON et AB interdits |
| Canada : autres provinces | **Ouvert** | Restreint | Ouvert |
| États-Unis | Close-only | **Ouvert** (bourse américaine, KYC) | Interdit |

Kalshi juge l'éligibilité sur le **pays de résidence déclaré au KYC** (aide Kalshi, 2026-03-20) ; Polymarket sur le **lieu physique** (IP). Source : `E/01-audit.md:298`.

## 3. Géoblocage de Polymarket

### 3.1 Ce que dit la doc officielle (page « Geographic Restrictions », lue le 2026-10-17/18)

- Serveurs primaires en **eu-west-2** (Londres) ; « Closest Non-Georestricted Region: eu-west-1 » (Irlande). Source : `RN/axe_B_ordres.md:213` ; `SPEC.md:318`.
- Zones bloquées totalement : Iran, Syrie, Cuba, Corée du Nord, Crimée, Donetsk, Louhansk. **Close-only sur le site et l'API** : notamment États-Unis, Royaume-Uni, **France**, Allemagne, Italie, Belgique, Pologne, Australie, Singapour, Taïwan, Thaïlande, quatre provinces du Canada. **Close-only sur le site seulement (API non restreinte)** : **Irlande**, Japon, Pays-Bas, Corée du Sud, Malte (sport seulement). Source : `RN/axe_B_ordres.md:214-217`.
- « Orders submitted from blocked regions will be rejected. » Contrôle : `GET https://polymarket.com/api/geoblock` (`blocked, ip, country, region`). Source : `RN/axe_B_ordres.md:219`.
- La localisation est déterminée par l'**IP de la requête**. Centre d'aide (mis à jour le 2026-08-14) : « Permissions are based on physical location rather than residency » ; « Polymarket strictly prohibits the use of VPNs or similar tools to bypass geographic restrictions ». Source : `E/00-recherche.md:332-333` ; `RN/delais_sources.md:163-164`.
- Texte d'acceptation de l'interface (relevé le 2026-10-18 dans le HTML de `polymarket.com/tos`) : « By trading, you agree to the [Terms of Use] and attest that you are not (i) a U.S. person; (ii) located in the United States, France or other restricted jurisdiction; and (iii) using a VPN or other measures to circumvent […]. Polymarket reserves the right to put your wallet in close-only mode if you do not comply with these Terms. » Source : `E/00-recherche.md:334`.
- Le texte complet des CGU (clause numérotée) n'a pas pu être lu (rendu côté client). Une « section 2.1.4 » est citée par des sources secondaires et le centre d'aide : numéro `[non vérifié sur le texte officiel]`. Source : `E/00-recherche.md:335` ; `RN/axe_B_ordres.md:221`.
- Contexte : l'ANJ a ordonné le 2026-07-16 aux fournisseurs d'accès de bloquer Polymarket en France (presse). Source : `E/00-recherche.md:336` ; `RN/delais_sources.md:165`.
- Les CGU d'OKX, Binance et Coinbase sur l'usage de leurs flux publics par un bot n'ont pas été vérifiées. Source : `RN/delais_sources.md:178`.

### 3.2 Contradiction du point d'accès `geoblock`

- Vu de Dublin, `https://polymarket.com/api/geoblock` répond `blocked: true, country: IE` : il applique la règle **du site web**, plus stricte que la règle des ordres par API. Source : `SPEC.md:327-329` ; `docs/real-trading.md:61-68`.
- L'ancienne application traduit cette réponse en règle d'API (`geoVerdict`) : `blocked: false` autorisé ; `blocked: true` pour un pays « Close-Only on Frontend » (IE, NL, JP, KR) autorisé ; tout autre blocage (GB, FR, US…) refusé ; réponse illisible refusée. Source : `SPEC.md:330-333` ; `docs/real-trading.md:69-72`.
- Sources concordantes pour l'API ouverte depuis l'Irlande : la page officielle, et deux sites commerciaux d'hébergeurs de VPS (QuantVPS, TradoxVPS : l'Irlande est passée dans ce groupe en juillet 2026). Toutes rappellent que les règles suivent l'utilisateur, pas le serveur. Source : `SPEC.md:335-341`.
- Preuve définitive : un premier ordre réel depuis Dublin (`docs/real-trading.md:72-73`). Les 3 ordres réels de l'engine (2026-10-22, depuis `dublin-live`) ont reçu une réponse de l'API (ordres FAK « tous tués », réponses après 348 ms à 3,5 s) : `E/10-delai-appariement.md:3-4`, `:40-44`. Le corpus ne dit pas explicitement que cela vaut preuve que l'API est ouverte depuis l'Irlande `[à vérifier]`.

### 3.3 Point de conformité bloquant et sa levée

- **Conséquence initiale (2026-10-18)** : si la personne qui opère le compte se trouve en France (ou dans une autre juridiction close-only sur l'API), faire passer les ordres par un serveur à Dublin peut constituer un contournement interdit ; le risque est un wallet mis en close-only et des fonds bloqués. Le rapport ne recommande **aucun** contournement. Toute la partie réseau suppose ce point réglé. Source : `E/00-recherche.md:338-340`.
- Moyens non conformes (à ne jamais recommander) : VPN ou VPS servant à contourner le close-only FR/GB ; envoi massif d'ordres marketables puis annulations pour sonder le carnet ; redistribution d'un flux Chainlink Data Streams. Les sites de VPS à Dublin « pour se rapprocher de Londres » ne sont pas à suivre. Source : `RN/delais_sources.md:166`, `:172` ; `E/00-recherche.md:211`.
- **Levée du 2026-10-21** : l'utilisateur déclare être physiquement à Dublin, où est hébergé le nœud ; blocage « pays » levé pour Polymarket (Irlande : API ouverte, restriction sur le site seulement ; règle fondée sur le lieu physique). La légalité locale n'est pas étudiée (avis juridique). Source : `E/JOURNAL.md:31`, `:126`.
- Moyens conformes : ordres post-only ou non marketables ; co-location eu-west-2 après KYC/KYB pour un compte propre ; serveur dans une région autorisée **si l'opérateur s'y trouve aussi légalement** ; respect des seaux et des seuils ; flux de bourse publics. Source : `RN/delais_sources.md:171`.

## 4. Co-location et choix d'hébergement

| Sujet | Fait | Source |
|---|---|---|
| Co-location eu-west-2 | Possible sur formulaire KYC/KYB, réservée aux « market makers and individual traders operating on their own account » ; exclue pour les participants du Builder Program et les applications tierces. Contenu du formulaire, conditions et coût non publiés. | `RN/axe_B_ordres.md:213`, `:228` ; `RN/axe_A_sources.md:31` ; `RN/delais_sources.md:53`, `:177` |
| Décision sur la co-location | Ne pas poursuivre : ≤ ~6 ms d'aller simple, le taker delay reste ; le Royaume-Uni est close-only sur l'API (on ne sait pas comment un nœud colocalisé y échappe `[non vérifié]`). | `E/00-recherche.md:134`, `:355` |
| KYC | En mai 2026, le VP ingénierie de Polymarket a précisé que le KYC ne concernait qu'un produit bêta. | `RN/axe_B_ordres.md:220` |
| Dublin (eu-west-1) | Région non restreinte la plus proche du moteur Polymarket ; c'est là que tournent les nœuds `live` et `dry-run`. RTT applicatif mesuré vers le CLOB : 24,4 ms à chaud (p99 29,5). | `SPEC.md:311-313` ; `deploy/README.md:8` ; `E/01-audit.md:185` |
| Londres (eu-west-2) | Moteur de Polymarket. Le Royaume-Uni est restreint pour les ordres (site et API) : un nœud à Londres sert **uniquement à l'observation** (Oracle Cloud Always Free, nœud C). | `SPEC.md:311-312` ; `deploy/README.md:9`, `:11-13` |
| Londres écarté | « Nœud à Londres écarté : Royaume-Uni close-only sur le site et l'API ; Dublin est la région ouverte la plus proche. » Gain d'un nœud à Londres estimé à ~10 ms. | `E/JOURNAL.md:27` ; `E/10-delai-appariement.md:121` |
| RTT Dublin ↔ Londres | ~12 ms (agrégateur public, méthode absente) ; « 0-2 ms » tranché contre (460 km, ≥ 4,6 ms aller-retour en ligne droite). Hypothèse de travail. | `E/00-recherche.md:188` ; `RN/axe_C_strategies.md:269` |
| Règles qui suivent l'utilisateur | « Ces règles visent aussi l'utilisateur (le pays où il réside), pas seulement le serveur. » | `SPEC.md:325` ; `deploy/README.md:25` |
| Autres nœuds | Nœud A (machine de l'utilisateur, sortie réseau Montréal) en référence ; nœud B AWS Dublin ; nœud C Oracle Londres (observation). Aucun nœud Berlin n'existe. | `deploy/README.md:5-9` ; `E/JOURNAL.md:34` |

## 5. Autres points de conformité

- **Dépôts suspects** : un dépôt « arbitrage bot » (531 étoiles) au README npm mais au contenu C# avec DLL précompilées (fort signal d'arnaque) ; un dépôt de binaires précompilés manipulant une clé privée ; plusieurs dépôts promotionnels. Consigne : ne rien télécharger ni exécuter. Source : `E/00-recherche.md:208-210`.
- **Injection** : la page `latency.glassnode.com/llms.txt` contient des consignes adressées aux agents d'IA (liste d'attente, lien de parrainage) : ignorées et signalées. Source : `E/00-recherche.md:212`.
- **Manipulation du spot au règlement** : exclue (risque légal et CGU). Source : `E/00-recherche.md:152`.
- **Sondage de liquidité** par envoi puis annulation : c'est ce que le blocage des annulations pendant la retenue chercherait à empêcher (source : article syndiqué, rumeur). Source : `RN/delais_sources.md:168`.
- **Redistribution Data Streams** : interdite (chaque consommateur s'authentifie avec ses propres identifiants). Source : `RN/delais_sources.md:167`.

## 6. Points [à vérifier]

1. Légalité locale de l'opération depuis Dublin pour un résident français : non étudiée (`E/JOURNAL.md:31`, `:73`).
2. Texte officiel complet des CGU (clause 2.1.4, trading automatisé, API) : non lu (`E/00-recherche.md:335` ; `RN/delais_sources.md:176`).
3. Liste des pays éligibles à Kalshi dans le contrat membre (`E/01-audit.md:302`) ; éligibilité de l'Irlande `[non vérifié]` (`E/01-audit.md:293`).
4. Support de `POLY_PROXY` (compte Magic) en CLOB V2 et comportement de l'API depuis l'Irlande tant qu'un ordre accepté n'a pas été observé (`RN/axe_B_ordres.md:97` ; `docs/real-trading.md:72-73`).
5. Conditions et coût de la co-location eu-west-2 (`RN/delais_sources.md:177`).
