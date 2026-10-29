# Conception de l'engine

Ce fichier synthétise la mission, les règles de conception, la carte de l'ancien chemin d'ordre, l'architecture Rust retenue, ses contrats, le registre et les cibles chiffrées de l'engine du dépôt source `btc5m-lag-sim`. Les mesures de chaque phase sont dans [engine-mesures-phases.md](recherche/engine-mesures-phases.md) ; la sécurité dans [securite-et-mode-reel.md](securite-et-mode-reel.md) ; les décisions datées dans [journal-decisions.md](journal-decisions.md).

Convention : chemins relatifs au dépôt source `btc5m-lag-sim` (lecture seule), cités `fichier:ligne`. `[à vérifier]` marque ce qui n'est pas établi. Le refactor de la migration (séparation horloge / flux / exchange pour le backtest) est décrit dans `architecture-backtest.md`, pas ici.

## 1. Mission et objectifs

Construire, dans un dossier indépendant, un engine de trading **réel** pour les marchés Polymarket « Bitcoin Up or Down » de 5 minutes, avec le moins de temps possible entre l'information et l'ordre, et aucun écart entre ce que l'engine affirme et ce qui s'est passé (`TODO.md:10-12`).

Ce que fait l'engine, et rien d'autre : voir (état frais du marché et des sources de prix, avec âge et intégrité de chaque donnée), demander (une stratégie pure et déterministe répond « rien » ou « cet ordre »), agir (envoi sans délai évitable, sous les garde-fous, sans jamais perdre la trace d'un ordre) et rendre compte (registre en ajout seul, seule source de tous les chiffres) (`TODO.md:89-100`).

| # | Objectif | Exigence | Source |
| --- | --- | --- | --- |
| 1 | Flux Polymarket à latence minimale | Carnets de la fenêtre en cours et de la suivante abonnés avant l'ouverture ; aucune requête HTTP sur le chemin chaud à l'ouverture ; intégrité du carnet contrôlée | `TODO.md:110` |
| 2 | Sources sans perte ni latence évitable | Pertes détectées (séquence si la source en fournit, sinon silence anormal) ; source invalide jusqu'à resynchronisation ; reconnexion immédiate | `TODO.md:111` |
| 3 | Envoi instantané | De la décision au premier octet écrit : connexion chaude, métadonnées en cache, aucune autre E/S réseau | `TODO.md:112` |
| 4 | Mesurer, ne pas interpréter | Chaque chiffre vient d'une réponse de Polymarket ou de la chaîne ; seule la valeur latente (§4.2 de la mission) est calculée | `TODO.md:113` |

L'engine ne dessine rien, ne simule rien sur le chemin réel et ne devine rien ; il expose en lecture son état et son registre et accepte quelques commandes (armer, désarmer, arrêt d'urgence, limites) (`TODO.md:102-104`).

**Valeur en direct et défenses.** Une position n'est valorisée qu'une fois son exécution connue de Polymarket, au prix et à la quantité que Polymarket renvoie, jamais au prix limite. Le calcul est unique : coût = montant payé + frais ; valeur de sortie = vente des parts sur les bids observés niveau par niveau, moins les frais preneur ; gain latent = valeur de sortie − coût ; valeur du compte = solde pUSD mesuré + valeur de sortie des positions ouvertes. Chaque valeur est étiquetée « latent » avec l'âge du carnet ; latent et réalisé ne se mélangent jamais (`TODO.md:120-142`). Départ de la valorisation décidé le 2026-10-19 : dès `MATCHED` (position « appariée, non confirmée »), « confirmée » au `CONFIRMED`, retirée au `FAILED` (`engine/02-conception.md:264-268, :338`).

## 2. Principes de conception

| # | Principe | Source |
| --- | --- | --- |
| 1 | Mesure avant calcul : un chiffre sans fait mesuré derrière lui est un bug | `TODO.md:183` |
| 2 | Dans le doute, on n'ouvre pas | `TODO.md:184-186` |
| 3 | Le chemin chaud passe avant tout : réception → état → stratégie → envoi ; ni écriture disque bloquante, ni compression, ni HTTP de service, ni diffusion vers une interface sur ce fil | `TODO.md:187-189` |
| 4 | Deux horloges jamais confondues : monotone (durées), murale synchronisée par chrony (comparaison aux bourses) ; les deux sont enregistrées | `TODO.md:190-191` |
| 5 | Minimal : chaque module, dépendance, processus justifie sa présence par un des quatre objectifs | `TODO.md:192-193` |
| 6 | Redémarrage sûr : relire le registre, interroger Polymarket (ordres ouverts, positions, solde) et réconcilier avant le premier ordre | `TODO.md:194-195` |
| 7 | Faits externes vérifiés dans la documentation officielle, le SDK et des messages réels, jamais de mémoire | `TODO.md:196-198` |
| 8 | Toute règle de fraîcheur s'appuie sur des sessions enregistrées (une règle `STALE_MS` a été ajoutée puis retirée car les données contredisaient la cause supposée) | `TODO.md:199-201` |

Architecture imposée : le cœur ne connaît aucune source (aucun nom, URL, format ; un test le vérifie) ; tout flux entre par un adaptateur ; l'adaptateur ne fournit que ce qui est propre à sa source (connexion, décodage, séquences et horodatages natifs, heartbeat, déclaration de capacités) ; le cœur fait le reste une seule fois (reconnexion, trous et silences, validité, âge, registre) ; brancher une source = un adaptateur + une ligne de configuration (`TODO.md:146-163`).

## 3. Carte de l'ancien chemin d'ordre (audit de la phase 1)

L'ancienne application tournait dans **un seul processus Node, sur un seul fil** (`engine/01-audit.md:11-13`). Les diagnostics de départ ont tous été confirmés (`engine/01-audit.md:60-68`).

| Saut | Constat | Source |
| --- | --- | --- |
| Réception | `JSON.parse` du message entier pour chaque message de chaque source ; événement `px` émis seulement si le mid change ; numéros de séquence OKX non lus | `engine/01-audit.md:19-21` |
| Carnet Polymarket | Seuls `book` et `price_change` utilisés ; `last_trade_price`, `tick_size_change`, `best_bid_ask` et `hash` ignorés | `engine/01-audit.md:22` |
| Enregistrement | `recorder.write(ev)` (stringify + gzip) **avant** le moteur, sur le chemin chaud | `engine/01-audit.md:23` |
| Ordre réel | Miroir de l'ordre simulé ; signature au moment de l'envoi ; préchauffage seulement en dry-run ; première commande d'un token = appels réseau cachés du SDK | `engine/01-audit.md:31-33` |
| Résultat | Remplissage déduit de la réponse du POST ; pas de canal `user` ni de `CONFIRMED` ; gamma sondé toutes les 15 s ; règlement calculé localement ; `pnl()` compte les positions ouvertes au coût, frais non isolés | `engine/01-audit.md:39-42` |
| Même fil que le chemin chaud | Diffusion SSE (250 ms), `GET /time` (2 s), fenêtres (1 s), résolutions (15 s), horloge via 5 requêtes Binance (30 s), serveur HTTP | `engine/01-audit.md:48-54` |
| Délai d'un ordre | Estimé : signature 3 ms + RTT de `GET /time` + matching 10 ms ; sans les 150 ms du taker delay | `engine/01-audit.md:64` |
| OKX | Le marché suivi est le perpétuel `BTC-USDT-SWAP` | `engine/01-audit.md:72` |

## 4. Architecture retenue

### 4.1 Un processus, deux fils

Un seul processus ; le **fil chaud** (un vCPU, boucle d'événements à un seul fil) possède seul l'état de marché, les ordres en vol et la connexion d'ordre ; il ne fait aucune E/S disque et pousse chaque fait dans un canal sans blocage vers le **fil froid** (registre, réconciliation, découverte des fenêtres et métadonnées, résolutions, API locale sur 127.0.0.1) (`engine/02-conception.md:35-62`). L'arrêt d'urgence est un drapeau atomique lu avant chaque envoi. Pas de processus supplémentaire : la t4g.small a 2 vCPU et un fil par vCPU suffit (`engine/02-conception.md:59-62`).

Frontière du chemin chaud : décodage → mise à jour de l'état → stratégie → garde-fous → construction de l'ordre → signature → en-têtes HMAC → écriture (`engine/02-conception.md:66-68`).

Démarrage et redémarrage : relire le registre, interroger Polymarket (ordres ouverts, trades récents, positions, solde pUSD), réconcilier (un écart bloque les ouvertures), seulement ensuite s'abonner aux flux puis accepter « armer » (`engine/02-conception.md:70-75`).

### 4.2 Contrats

| Contrat | Contenu | Source |
| --- | --- | --- |
| Adaptateur de flux | Connexion/abonnement/heartbeat ; décodage vers des événements normalisés (un message illisible est compté, jamais deviné) ; capacités déclarées (`sequence`, `event_time`, `book`, `silence_ms`) ; resynchronisation | `engine/02-conception.md:81-93` |
| Adaptateur d'exécution | `prepare(marché)` hors chemin chaud (pas, taille min, frais, `itode`, tokens, connexion chaude) ; `send` (construit, signe, écrit) ; `on_response`/`on_user_event` ; `query(ordre)` ; modèle de délai propre à l'adaptateur | `engine/02-conception.md:97-106` |
| Événement normalisé | `source` (nom de configuration), `kind` (`price`, `book_snapshot`, `book_delta`, `tick_change`, `trade`, `twap`, `heartbeat`), `recv_mono_ns`, `recv_wall_ns`, `event_time_ms`, `seq`, charge utile typée | `engine/02-conception.md:108-117` |
| Décision de stratégie | `decide(état, configuration) -> Rien | Intention`, fonction pure et déterministe ; l'état porte âge et validité de chaque donnée | `engine/02-conception.md:119-123` |
| Machine d'états d'un ordre | décidé → signé → envoyé → acquitté → apparié → confirmé ; issues refusé, tué, échoué, inconnu ; délai imparti avant `inconnu` de 300 ms au départ (30 + 150 + 120), réglé ensuite sur mesure | `engine/02-conception.md:125-145` |
| Argent et prix | Entiers en micro-unités ; prix en millièmes (ou centièmes) avec le pas courant ; flottants seulement pour la juste valeur ; frais calculés puis remplacés par ceux de Polymarket | `engine/02-conception.md:147-153` |

Le taker delay vit **uniquement** dans l'adaptateur d'exécution Polymarket (décision de l'utilisateur, `engine/JOURNAL.md:48`). Trois implémentations de l'exécution étaient prévues : Polymarket réel, Polymarket simulé (même modèle de délai, appariement contre le carnet enregistré), faux CLOB local pour les tests (`engine/02-conception.md:105-106`).

### 4.3 Registre en ajout seul

Une ligne JSON par fait, en ajout seul : `{seq, mono_ns, wall_ns, type, …}` ; types : `boot`, `config`, `source_state`, `order_*`, `fee`, `balance`, `position`, `settlement`, `redeem`, `reconcile`, `valuation`, `defense`, `command` (`engine/02-conception.md:156-162`). Un fait est « acquitté par le registre » une fois écrit **et** `fsync` fait (lots de 50 ms ou 64 faits) ; les faits non acquittés mais connus de Polymarket se retrouvent par la réconciliation (`engine/02-conception.md:164-166` ; `engine/03-registre.md:11, :16`).

Règles de relecture : une fin de fichier tronquée est ignorée (jamais acquittée) ; tout autre dommage ou trou de `seq` **refuse l'ouverture** (« dans le doute, l'engine ne démarre pas ») (`engine/03-registre.md:12, :17-19`). L'horloge monotone repart de zéro à chaque démarrage ; la murale ordonne les démarrages (`engine/03-registre.md:21-22`). Critère de sortie vérifié le 2026-10-19 : 20 arrêts brutaux par SIGKILL, 109 844 faits relus à l'identique (`engine/03-registre.md:31`). Limite : SIGKILL prouve la tenue face à un crash du processus, pas à une coupure de la machine (`engine/03-registre.md:39-40`).

### 4.4 Stack : Rust

Prototypes mesurés sur le nœud de Dublin (t4g.small), même travail des deux côtés (décodage d'un vrai message OKX, décision, ordre V2 signé EIP-712, HMAC, requête sur socket TCP chaude locale) :

| µs | Rust | Node 24 |
| --- | --- | --- |
| Total p50 | 159,7 – 160,5 | 673,5 – 685,1 |
| Total p99 | 185 – 253 | 1 238 – 1 273 |
| Total max | 325 – 2 116 | 5 167 – 8 787 |
| dont signature p50 | 142 | 625 – 636 |

Source : `engine/02-conception.md:172-186`. Pour mémoire, le chemin de l'ancien SDK Node coûtait 1 160 µs au p50 (`engine/02-conception.md:188`). Rust fait gagner ≈ 0,5 ms au p50, ≈ 1 ms au p99 et 2 à 7 ms dans la queue, et supprime les pauses de GC ; face au budget total (≈ 216 à 281 ms jusqu'à l'appariement) c'est peu en proportion, mais c'est la part que l'engine contrôle (`engine/02-conception.md:192-195`). Écartés : Node (mesuré), C++ (mêmes performances, moins de garanties mémoire), Go (GC, non mesuré), Python (`engine/02-conception.md:205-206`).

Le SDK Rust officiel (`rs-clob-client-v2`) sert de **référence** mais n'est pas embarqué (il tire alloy et reqwest, lourds à compiler sur 2 Go) ; la signature minimale (k256 + sha3) est vérifiée contre lui et contre le SDK TypeScript (`engine/02-conception.md:196-199`). Pas de busy-poll (crédits CPU) : le fil chaud attend sur epoll (`engine/02-conception.md:200-201`).

Dépendances proposées et validées : `tokio` (un fil), `tokio-tungstenite` + `rustls`, `httparse`, `serde`/`serde_json`, `k256`, `sha3`, `hmac`, `sha2`, `base64`, `hex`, `zeroize`, `scrypt`, `aes-gcm` ; en tests `proptest` ; `cargo-deny` accepté (`engine/02-conception.md:208-222, :331`).

Disposition du dossier : `core/` (contrats, état, machine d'états, registre, garde-fous ; aucun nom de source), `strategy/`, `adapters/{polymarket,chainlink,okx,coinbase,fakeclob}`, `app/`, `bench/`, `proto/`, `fixtures/` (`engine/02-conception.md:224-239`). Le test `core_sources_name_no_venue` échoue si `core/src` contient un nom, une URL ou un format de source (`engine/02-conception.md:241-242`). L'adaptateur Coinbase et le faux CLOB figuraient dans la disposition prévue ; l'état réel des adaptateurs est dans `engine/README.md:15-25` (polymarket, chainlink, okx seulement) `[à vérifier : adaptateurs prévus mais non construits]`.

Pour brancher une source : écrire son adaptateur, l'ajouter à `adapter()` dans `app/src/main.rs`, ajouter sa ligne de configuration ; une source absente de la configuration n'est ni chargée ni connectée (`engine/README.md:54-55`). Configuration minimale : sources `polymarket-market` (3 connexions), `chainlink-rtds` (`btc/usd`), `okx-bbo-tbt` (`BTC-USDT-SWAP`), registre `ledger` (`engine/README.md:42-49`).

## 5. Ce que la recherche externe a tranché

Verdicts seulement (le détail est dans les notes brutes `engine/research_notes/…`).

| Sujet | Verdict | Source |
| --- | --- | --- |
| Clients officiels v1 (`py-clob-client`, `rs-clob-client`, `clob-client`) | Archivés, « no longer functional » depuis CLOB V2 (2026-04-28) ; tout code qui signe `feeRateBps`/`nonce` est cassé | `engine/00-recherche.md:158` ; `engine/research_notes/Engine réel Polymarket BTC 5m/open_source.md:8-9` |
| `rs-clob-client-v2` | Référence de vérité V2, à s'inspirer ; reprendre si Rust | `engine/00-recherche.md:163` |
| NautilusTrader (adaptateur Polymarket V2) | Meilleur cahier des charges pour l'objectif 4 (états en vol, ordre non résolu gardé tel quel, « a missing position report is not evidence of a flat position ») ; s'en inspirer, ne pas l'adopter (LGPL, lourd) | `engine/00-recherche.md:165` |
| Machines d'états d'ordre (Hummingbot, barter-rs, poly-market-maker) | S'inspirer : états « en vol » distincts, état « inconnu » rattachable | `engine/00-recherche.md:166` |
| Détection de trous (cryptofeed, barter-data) | S'inspirer ; ne pas reprendre cryptofeed (passé en AGPL-3.0) | `engine/00-recherche.md:167` |
| Journal append-only hors chemin chaud | Reprendre le principe (écrivain unique) ; éviter de réécrire tout un `state.json` | `engine/00-recherche.md:168` |
| Bots communautaires « HFT », Freqtrade, CCXT, Aeron | À écarter ; leurs gains annoncés sont de simples affirmations | `engine/00-recherche.md:170` ; `engine/research_notes/Engine réel Polymarket BTC 5m/open_source.md:361-362` |
| Langage du chemin chaud | Le langage pèse quelques µs à quelques dizaines de µs par message ; l'écart se joue sur la signature et la gigue de GC ; Rust et C++ suppriment la gigue de GC | `engine/research_notes/Engine réel Polymarket BTC 5m/runtime_client.md:10-12, :138-139` |
| Signature secp256k1 | Aucun chiffre publié et à jour pour libsecp256k1, k256, go-ethereum ; il faut mesurer sur la cible | `engine/research_notes/Engine réel Polymarket BTC 5m/runtime_client.md:82-83` |
| Instance t4g.small | Ligne de base de 20 % par vCPU ; un busy-poll permanent dépasse la ligne de base ; pas de Time Sync « enhanced » sur T4g, donc chrony vers 169.254.169.123 | `engine/research_notes/Engine réel Polymarket BTC 5m/runtime_client.md:172-173, :222-223` |

Solutions écartées dès la conception : co-location (≤ 6 ms face à 150 ms, conditions inconnues), lots d'ordres, pré-signature (≤ 0,15 ms en Rust), Chainlink Data Streams direct (payant, passerelle aux États-Unis), OKX VIP4/SBE, Limitless (délai 250 ms, carnet vide), Kalshi (non éligible) (`engine/02-conception.md:26-29`). Les dépôts d'arnaque ou commerciaux repérés pendant la recherche sont à ne ni télécharger ni exécuter (`engine/00-recherche.md:206-212`).

## 6. Cibles chiffrées (machine de référence : t4g.small, Dublin)

| # | Objectif | Cible | Mesuré | Source |
| --- | --- | --- | --- | --- |
| 1 | Âge du carnet Polymarket à la réception | p50 ≤ 15 ms ; p99 ≤ 150 ms, validé avec 3 connexions | 10,4 / 91,3 ms (phase 4) | `engine/02-conception.md:290` |
| 1 | Carnet exploitable à l'ouverture | Snapshot avant l'ouverture, aucune requête HTTP | ≈ 299 s avant l'ouverture | `engine/02-conception.md:291` |
| 1 | Détection d'une coupure | ≤ 3 s de silence (révisé) | — | `engine/02-conception.md:292` |
| 1 | Carnets incohérents | 100 % détectés et resynchronisés | 100 %, réparation ≤ 195 ms | `engine/02-conception.md:293` |
| 2 | Latence ajoutée par l'engine, réception → état | p99 ≤ 250 µs (révisé : 20 µs ne valait que pour le décodage seul) | p50 22 µs, p99 165 µs | `engine/02-conception.md:294` |
| 2 | Trous de séquence hors coupure | 0 | OKX `bbo-tbt` sans séquence ; 0 faux trou | `engine/02-conception.md:295` |
| 3 | Décision → dernier octet écrit | p50 ≤ 0,25 ms ; p99 ≤ 0,5 ms ; p99,9 ≤ 1 ms ; max ≤ 3 ms | 0,223 / 0,288 / 0,585 / 2,38 ms | `engine/02-conception.md:297` |
| 3 | Envoi → acquittement | Aller-retour p99 ≤ 30 ms + délai du marché | 24,4 / 29,5 ms à chaud | `engine/02-conception.md:298` |
| 4 | Réconciliation registre ↔ Polymarket | Écart nul ; chaque chiffre remonte à un fait du registre | Traçabilité testée ; écart nul à prouver sur le premier ordre réel | `engine/02-conception.md:299` |

## 7. Plan de tests

| Domaine | Tests | Source |
| --- | --- | --- |
| Registre | Crash en pleine écriture puis relecture exacte ; fsync par lots | `engine/02-conception.md:307` |
| Cœur des flux | Suite de conformance commune (vrais messages dans `engine/fixtures/raw`, coupures, trous, doublons, désordre, messages tronqués, silences) ; test « le cœur ne contient aucune source » | `engine/02-conception.md:308` |
| Exécution | Faux CLOB : succès, partiel, FAK tué, 4xx, 5xx, délai dépassé, coupure pendant l'envoi, réponse après le délai, doublon de message `user` ; `inconnu` jamais renvoyé | `engine/02-conception.md:310` |
| Signature | Vecteurs du SDK TypeScript officiel revérifiés par notre code, et inversement | `engine/02-conception.md:311` |
| Argent et carnet | Tests par propriétés (`proptest`) : conservation des montants, arrondis, frais, valeur de sortie ; analyse dimensionnelle | `engine/02-conception.md:312` |
| Valeur et défenses | Traçabilité de chaque chiffre ; étiquette « latent » + âge ; position non confirmée jamais valorisée ; chute de carnet rejouée → stop loss au bon instant | `engine/02-conception.md:313` |
| Performance | Bancs versionnés, même machine avant/après | `engine/02-conception.md:314` |
| Stratégie | Parité de décisions avec le compte Réaliste de l'ancien moteur | `engine/02-conception.md:315` |
| Revues | `spec-to-code-compliance` contre la conception ; `claude-security`, `sharp-edges`, `zeroize-audit` sur la clé et la garde réseau ; revue adversariale | `engine/02-conception.md:316` |

## 8. Décisions de l'utilisateur sur la conception (2026-10-19)

Stack Rust et dépendances validées (+ `cargo-deny`) ; 1 stratégie, 1 source de prix, 1 wallet en réel ; mode papier oui (adaptateur Polymarket simulé, chiffres étiquetés « simulés ») ; capital par source partagé comme l'application actuelle ; stop loss paramétrable (défaut −50 % de la valeur latente, hors dernière minute) ; seuil de session = réalisé + latent ; plafonds 5 $ par ordre et 25 $ de perte par session ; valorisation dès `MATCHED` ; source de prix indifférente mais « brancher une source ne touche jamais au cœur » (`engine/02-conception.md:329-339`).

## 9. Points `[à vérifier]`

- Adaptateurs prévus en conception (Coinbase, faux CLOB) mais absents de l'organisation décrite par `engine/README.md:15-25` : construits ou non, selon la suite (le faux CLOB est cité en test dans `engine/06-execution.md:94`).
- « L'identifiant d'ordre est le hachage EIP-712 » : supposition non vérifiée (`engine/02-conception.md:143-144` ; `engine/06-execution.md:42-43`).
- Le temps de matching du moteur est supposé nul dans la stratégie (`engine/08-strategies.md:83`).
