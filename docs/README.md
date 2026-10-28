# Documentation de `updown5m-lab`

Synthèse de toute la recherche menée dans le dépôt `btc5m-lag-sim` (lecture seule), à raison d'un fichier par sujet. Le dépôt `updown5m-lab` est recentré sur deux choses : l'enregistrement continu de flux (`source-tracing/`) et un backtest ultra réaliste des marchés Polymarket « Up or Down » 5 minutes (`backtest/`), le tout piloté par le même code d'engine (`engine/`). Ces documents donnent les faits établis, les chiffres, les conclusions et les impasses ; le tâtonnement est supprimé.

## Conventions

- **Langue** : français. Tous les fichiers `.md` du dépôt sont en français (`CLAUDE.md`, section « Langue »).
- **Sources** : chaque affirmation cite son fichier d'origine dans `btc5m-lag-sim`, par un chemin relatif suivi d'une ligne ou d'une section, par exemple `engine/10-delai-appariement.md:99-100`. Le dépôt source est en lecture seule. Les chemins du dépôt cible sont signalés « dépôt cible ». Les citations ont été revérifiées avant d'être figées ; une ligne citée peut toutefois bouger si le source est modifié.
- **`[à vérifier]`** : marque ce qui n'est pas établi (contradiction entre sources, mesure sur un petit échantillon, règle non documentée). Aucune règle de marché n'est inventée : une règle non établie est marquée et rendue paramétrable.
- **Chiffres** : les mesures citent leur lieu (nœud `dublin-live`, portable), leur taille d'échantillon quand elle est connue et leur borne (IC95, p50 / p99).
- **Graphe de connaissances** : le projet a un graphe `graphify-out/` ; pour une question sur le code, lancer d'abord `graphify query "<question>"`, et `graphify update .` après une modification de code (`CLAUDE.md`, section « graphify »).
- **Décisions** : « décision de l'utilisateur » = écrite dans un journal source ; « décision utilisateur du 2026-10-27 » = décision de la migration (D1 à D9, E1, E2), voir [journal-decisions.md](journal-decisions.md).

## Index des documents

### Marché, flux, exécution

| Fichier | Contenu |
| --- | --- |
| [contraintes-marche.md](contraintes-marche.md) | Vérités acquises sur le marché : frais et rebates, tick, taille minimale, types d'ordre, taker delay, fenêtres de 5 min, résolution Chainlink, limites des flux, calibrages validés, résultats b01 à b10 à revoir (document obligatoire) |
| [ordres-et-appariement.md](ordres-et-appariement.md) | Cycle de vie d'un ordre, statuts, appels cachés du SDK, signature de type 3, taker delay (histoire et enquête de la phase 10), limites de débit, heartbeats, redémarrages du moteur |
| [sources-de-prix-et-flux.md](sources-de-prix-et-flux.md) | Canal `market` de Polymarket, Chainlink (RTDS, PolyBolt, Data Streams), OKX, Coinbase, Binance, Hyperliquid, lead-lag, seuils de silence, âges mesurés, flux OKX « creux » |
| [donnees-publiques.md](donnees-publiques.md) | Data API, gamma, `last_trade_price`, données on-chain, horodatage par bloc, jointure des fills, comptes dominants, méthode de mesure d'avance |
| [plateformes-et-conformite.md](plateformes-et-conformite.md) | Plateformes concurrentes, matrice plateforme × pays, géoblocage, conditions d'utilisation, co-location, choix Dublin / Londres |

### Stratégies et backtest

| Fichier | Contenu |
| --- | --- |
| [architecture-backtest.md](architecture-backtest.md) | Séparation horloge / flux / exchange pour que le backtest exécute le même code que le live : options envisagées, choix retenu, raisons (document obligatoire) |
| [strategies-preneuses.md](strategies-preneuses.md) | Stratégie de référence et candidates C1 à C6, parité à zéro écart, papier de 18,7 h, effet de la retenue réelle, Hyperliquid et Papertrade écartés |
| [strategies-maker.md](strategies-maker.md) | Backtest maker G1 à G4, hypothèse « annuler le côté perdant » réfutée, calibration trades / carnets, limites |
| [exploration-maker-b01-b10.md](exploration-maker-b01-b10.md) | Critères R1 à R7, lots b01 à b10, r6, x01 à x05, confirmations, état « à revoir » |
| [wallets-arcsine-certova.md](wallets-arcsine-certova.md) | Paramètres reconstruits des wallets arcsine et CERTova, présence sélective, irréplicabilité, extinction de l'edge de CERTova |
| [simulateur-maker.md](simulateur-maker.md) | Bornes de fill, validation sur ordres réels, corrections, rebates, calibrations de σ, constantes de `sim.mjs` ; référence de non-régression |

### Engine, exploitation, produit, outillage

| Fichier | Contenu |
| --- | --- |
| [engine-conception.md](engine-conception.md) | Mission, règles de conception, carte de l'ancien chemin d'ordre, architecture Rust (fil chaud / froid), contrats, registre, stack, cibles, plan de tests |
| [engine-mesures-phases.md](engine-mesures-phases.md) | Mesures et critères de sortie des phases 1 à 10 de l'engine, état des phases |
| [securite-et-mode-reel.md](securite-et-mode-reel.md) | Clé, garde réseau, garde-fous, revues de sécurité, décisions du mode réel, incident du 2026-10-22, constat du 2026-10-27 sur `dublin-live` |
| [exploitation-deploiement.md](exploitation-deploiement.md) | Nœuds AWS, ancien et nouveau déploiement (`deploy/`), rétention de 14 jours des anciens serveurs, rapatriement D1, solution gratuite D3, `pull-traces` |
| [interface-produit.md](interface-produit.md) | Principes produit et d'interface retenus (périmètre de la nouvelle UI, performance, accessibilité, charte) |
| [journal-decisions.md](journal-decisions.md) | Décisions datées et verdicts, dont D1 à D9, E1 et E2 du 2026-10-27, et les impasses à ne pas rouvrir |
| [contrats-migration.md](contrats-migration.md) | Contrats figés de la migration (validés le 2026-10-27) : format de trace v1, interfaces de l'engine, API d'état tracing / UI, décisions D1 à D9 et exigences E1, E2 |
| [conception-engine-migration.md](conception-engine-migration.md) | Synthèse de la conception engine live/test (séparation horloge / flux / exchange) et référence de non-régression du simulateur maker mesurée le 2026-10-27 |
| [skills.md](skills.md) | Tableau des skills du projet et des plugins activés : origine, rôle, justification (document obligatoire) |
| `README.md` (ce fichier) | Index des documents et conventions (document obligatoire) |

## Par où commencer

| Question | Document |
| --- | --- |
| Quelles règles du marché le simulateur doit-il respecter, et lesquelles ne sont pas établies ? | [contraintes-marche.md](contraintes-marche.md) |
| Pourquoi la stratégie preneuse est-elle abandonnée ? | [engine-mesures-phases.md](engine-mesures-phases.md) §10, [strategies-preneuses.md](strategies-preneuses.md) |
| Que vaut le simulateur maker par rapport au réel, et que doit-il reproduire bit à bit ? | [simulateur-maker.md](simulateur-maker.md), [journal-decisions.md](journal-decisions.md) §1 |
| Comment le backtest exécute-t-il le même code que le live ? | [architecture-backtest.md](architecture-backtest.md), [engine-conception.md](engine-conception.md) |
| Comment déployer le tracing et rapatrier les traces ? | [exploitation-deploiement.md](exploitation-deploiement.md) §6 et §7 |
| Que s'est-il passé avec les ordres réels ? | [securite-et-mode-reel.md](securite-et-mode-reel.md) §6 |
| Quels skills et plugins sont actifs, et pourquoi ? | [skills.md](skills.md) |

## Autres documents du dépôt (hors `docs/`)

Chaque dossier de premier niveau a son propre `README.md` : `engine/`, `source-tracing/` (format de trace v1, conversion des anciennes traces), `backtest/` (configuration `backtest/config/default.json`), `ui/`, `deploy/`. Le fichier `PROMPT-MIGRATION.md` à la racine est la spécification de la migration et n'est pas modifié.
