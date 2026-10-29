# Documentation de `updown5m-lab`

À la racine de ce dossier : ce qui décrit le projet tel qu'il est (architecture, simulateur, engine, exploitation, décisions). Dans `recherche/` : la synthèse de la recherche menée dans le dépôt `btc5m-lag-sim` (lecture seule), qui fonde ces choix. Dans `migration/` : les contrats et la conception de la migration vers ce dépôt, à titre historique. Vue d'ensemble du dépôt : [`../README.md`](../README.md).

## Conventions

- **Langue** : français. Tous les fichiers `.md` du dépôt sont en français (`CLAUDE.md`, section « Langue »).
- **Sources** : chaque affirmation cite son fichier d'origine dans `btc5m-lag-sim`, par un chemin relatif suivi d'une ligne ou d'une section, par exemple `engine/10-delai-appariement.md:99-100`. Le dépôt source est en lecture seule. Les chemins du dépôt cible sont signalés « dépôt cible ». Les citations ont été revérifiées avant d'être figées ; une ligne citée peut toutefois bouger si le source est modifié.
- **`[à vérifier]`** : marque ce qui n'est pas établi (contradiction entre sources, mesure sur un petit échantillon, règle non documentée). Aucune règle de marché n'est inventée : une règle non établie est marquée et rendue paramétrable.
- **Chiffres** : les mesures citent leur lieu (nœud `dublin-live`, portable), leur taille d'échantillon quand elle est connue et leur borne (IC95, p50 / p99).
- **Graphe de connaissances** : le projet a un graphe `graphify-out/` ; pour une question sur le code, lancer d'abord `graphify query "<question>"`, et `graphify update .` après une modification de code (`CLAUDE.md`, section « graphify »).
- **Décisions** : « décision de l'utilisateur » = écrite dans un journal source ; « décision utilisateur du 2026-10-27 / 28 » = décision de la migration (D1 à D13, E1, E2), voir [journal-decisions.md](journal-decisions.md).

## Le projet

| Fichier | Contenu |
| --- | --- |
| [architecture-backtest.md](architecture-backtest.md) | Séparation horloge / flux / exchange pour que le backtest exécute le même code que le live : options, choix, raisons, non-régression, corrections E1 |
| [simulateur-maker.md](simulateur-maker.md) | Modèle de fill, validation sur ordres réels, rebates, calibrations de σ, référence de non-régression, corrections E1 (§10) |
| [engine-conception.md](engine-conception.md) | Mission, règles de conception, architecture Rust (fil chaud / froid), contrats, registre, stack, plan de tests |
| [securite-et-mode-reel.md](securite-et-mode-reel.md) | Clé, garde réseau, garde-fous, décisions du mode réel, incident du 2026-10-22 |
| [exploitation.md](exploitation.md) | Instance de tracing, commandes, volumétrie mesurée, routine de rapatriement, points ouverts |
| [journal-decisions.md](journal-decisions.md) | Décisions datées et verdicts, dont D1 à D13, E1 et E2 |
| [pistes-ecartees.md](pistes-ecartees.md) | Tout ce qui a été écarté, pourquoi, quand, preuve : à lire avant de rouvrir une piste |
| [skills.md](skills.md) | Skills du projet et plugins activés : origine, rôle, justification |

## `recherche/` : ce qui fonde le projet

| Fichier | Contenu |
| --- | --- |
| [contraintes-marche.md](recherche/contraintes-marche.md) | Vérités acquises sur le marché : frais et rebates, tick, taille minimale, types d'ordre, taker delay, fenêtres de 5 min, résolution Chainlink, limites des flux |
| [ordres-et-appariement.md](recherche/ordres-et-appariement.md) | Cycle de vie d'un ordre, statuts, signature, taker delay (enquête de la phase 10), limites de débit, heartbeats |
| [sources-de-prix-et-flux.md](recherche/sources-de-prix-et-flux.md) | Canal `market` de Polymarket, Chainlink, OKX, Coinbase, Binance, Hyperliquid, lead-lag, seuils de silence, âges mesurés |
| [donnees-publiques.md](recherche/donnees-publiques.md) | Data API, gamma, `last_trade_price`, on-chain, horodatage par bloc, jointure des fills |
| [plateformes-et-conformite.md](recherche/plateformes-et-conformite.md) | Plateformes concurrentes, matrice plateforme × pays, géoblocage, CGU, co-location, choix de Dublin |
| [strategies-preneuses.md](recherche/strategies-preneuses.md) | Stratégie de référence et candidates C1 à C6, parité à zéro écart, papier de 18,7 h, effet de la retenue réelle |
| [strategies-maker.md](recherche/strategies-maker.md) | Backtest maker G1 à G4, hypothèse « annuler le côté perdant » réfutée, calibration trades / carnets |
| [exploration-maker-b01-b10.md](recherche/exploration-maker-b01-b10.md) | Critères R1 à R7, lots b01 à b10, r6, x01 à x05, confirmations, état « à revoir » |
| [wallets-arcsine-certova.md](recherche/wallets-arcsine-certova.md) | Wallets arcsine et CERTova : paramètres reconstruits, présence sélective, irréplicabilité |
| [engine-mesures-phases.md](recherche/engine-mesures-phases.md) | Mesures et critères de sortie des phases 3 à 10 de l'engine |
| [exploitation-deploiement.md](recherche/exploitation-deploiement.md) | Ancien déploiement : nœuds `dublin-*`, application `trading-gap`, wallets chiffrés, passation du 2026-10-22 |

## `migration/` : historique

| Fichier | Contenu |
| --- | --- |
| [contrats-migration.md](migration/contrats-migration.md) | Contrats figés de la migration (format de trace v1, interfaces de l'engine, API d'état de l'UI, caduque) et décisions D1 à D13, E1, E2 |
| [conception-engine-migration.md](migration/conception-engine-migration.md) | Conception engine live / test et référence de non-régression mesurée le 2026-10-27 |

## Par où commencer

| Question | Document |
| --- | --- |
| Comment lancer un backtest, avec quels paramètres ? | [`../backtest/README.md`](../backtest/README.md) |
| Que vaut le simulateur par rapport au réel ? | [simulateur-maker.md](simulateur-maker.md) |
| Quelles règles du marché le simulateur respecte-t-il, et lesquelles ne sont pas établies ? | [recherche/contraintes-marche.md](recherche/contraintes-marche.md) |
| Cette idée a-t-elle déjà été essayée ? | [pistes-ecartees.md](pistes-ecartees.md) |
| Pourquoi la stratégie preneuse est-elle abandonnée ? | [recherche/engine-mesures-phases.md](recherche/engine-mesures-phases.md) §10 |
| Comment piloter le tracing et rapatrier les traces ? | [exploitation.md](exploitation.md), [`../source-tracing/COMMANDES.md`](../source-tracing/COMMANDES.md) |
| Que s'est-il passé avec les ordres réels ? | [securite-et-mode-reel.md](securite-et-mode-reel.md) §6 |

Chaque outil a son `README.md` : [`engine/`](../engine/README.md), [`source-tracing/`](../source-tracing/README.md) (format de trace), [`backtest/`](../backtest/README.md) (paramètres). `PROMPT-MIGRATION.md` à la racine est la spécification d'origine de la migration ; les décisions de l'utilisateur des 27 et 28/10 la priment.
