# Pistes écartées

Ce qui a été essayé ou envisagé puis abandonné, pour ne pas le réexplorer sans **fait nouveau**. Une ligne par piste : quoi, pourquoi, quand, preuve. Les chemins en `code` sont relatifs au dépôt d'origine `btc5m-lag-sim` (lecture seule) ; les liens mènent au détail dans ce dossier.

## Stratégies

| Piste | Pourquoi écartée | Date | Preuve |
| --- | --- | --- | --- |
| Stratégie preneuse sur un signal de prix (OKX, Binance) | La retenue réelle des ordres preneurs dépasse nettement les 150 ms annoncés ; au-delà de ~1 s, plus rien de mesurable. 3 ordres réels, tous tués | 2026-10-22 | `engine/10-delai-appariement.md:99-100` ; [mesures des phases](recherche/engine-mesures-phases.md) §10 |
| Candidates preneuses C2 (complete-set en preneur), C3, C7 | C2 exige a + b < ~0,965 et la seconde jambe bouge pendant la retenue ; C3 sans edge (IC95 contient 0) ; C7 rejetée dès la phase 0 | 2026-10-18 à 10-20 | [stratégies preneuses](recherche/strategies-preneuses.md) |
| Maker continu, rejoindre le meilleur bid sur toute la fenêtre (x03, x05) | Tous perdants, −10 à −87 % sur la semaine ; nos fills sont plus adverses que ceux du maker moyen | 2026-10-25/26 | `engine/maker-bt/EXPLORATION.md:752-767` ; `engine/maker-bt/WALLETS.md:183` |
| Annuler le côté perdant en fin de fenêtre | Le perdant est déjà exécuté dans 61 à 85 % des fenêtres | 2026-10-22 | `engine/11-maker-polymarket.md:47-58` ; [stratégies maker](recherche/strategies-maker.md) §3.3 |
| Bids sur l'outsider en fin de fenêtre (b10) | 73 pertes sur 74 fills, −43 à −99 % : le vendeur qui nous remplit est informé | 2026-10-22/23 | [exploration b01-b10](recherche/exploration-maker-b01-b10.md) |
| Retrait rapide des bids sur mouvement d'OKX (b07) | 14 stratégies, 0 promue, −31 à −72 % sur la semaine | 2026-10-22/23 | [exploration b01-b10](recherche/exploration-maker-b01-b10.md) |
| Répliquer arcsine ; rejouer CERTova | Meilleur réplica d'arcsine −1,58 $/fenêtre (le wallet : +6,37) ; edge de CERTova éteint (avril +29,7 k$, mai −1,9 k$) | 2026-10-25/26 | `engine/maker-bt/WALLETS.md:25-29, :107-117, :209` ; [wallets](recherche/wallets-arcsine-certova.md) |
| Pause de cotation sur les news (VON, IA) | La news arrive plus lentement que le prix ; non backtestable | 2026-10-28 | décision utilisateur (la pause sur calendrier macro reste une idée non décidée) |

## Plateformes, infrastructure, exécution

| Piste | Pourquoi écartée | Date | Preuve |
| --- | --- | --- | --- |
| Hyperliquid piloté par Binance | Signal juste, mais 1,7 à 3,8 pb de gain contre 9 pb de frais aller-retour | 2026-10-22 | `engine/JOURNAL.md:24` |
| Papertrade | Prix lu à l'exécution (relayeur, blocs de 1 à 2 s) | 2026-10-22 | `engine/JOURNAL.md:25` |
| Limitless | Délai de 250 ms et carnet quasi vide | 2026-10-18 | `engine/JOURNAL.md:47, :51` |
| Kalshi | Non éligible pour un résident français (pays de résidence du KYC) | 2026-10-22 | `engine/JOURNAL.md:127` ; [plateformes](recherche/plateformes-et-conformite.md) |
| Nœud à Londres | Royaume-Uni close-only (site et API) ; Dublin est la région ouverte la plus proche | 2026-10-22 | `engine/JOURNAL.md:27` |
| Co-location, lots d'ordres, pré-signature, Chainlink Data Streams direct | Gains de quelques ms face à 150 ms de retenue ; Data Streams payant | 2026-10-18 | [conception de l'engine](engine-conception.md) |
| Passer par l'interface web pour éviter la retenue | La retenue est appliquée par le moteur d'appariement quel que soit le canal | 2026-10-22 | [ordres et appariement](recherche/ordres-et-appariement.md) |
| Canal OKX `books` (100 ms) | `bbo-tbt` seul, meilleur compromis pour la latence | 2026-10-20 | `engine/JOURNAL.md:61` |
| Skills Polymarket communautaires | Second ordre : la documentation officielle prime | 2026-10-18 | [skills](skills.md) |

## Simulateur et backtest

| Piste | Pourquoi écartée | Date | Preuve |
| --- | --- | --- | --- |
| Bornes de fill `prudent`, `traversée`, `calibrée`, `optimiste` comme référence | Sur ordres réels : PnL −97 %, −19 %, +12 %, −12 % ; la borne `exact` fait −4,3 % | 2026-10-24/25 | [simulateur maker](simulateur-maker.md) §3 |
| Retenue de 150 ms appliquée aux fills maker (`fill_lookback_ms`) | PnL prédit 7 943 $ pour 16 383 $ réels (−52 %) ; les poses ne sont pas retenues | 2026-10-25 | `engine/maker-bt/WALLETS.md:71, :123` |
| Corriger par une option du modèle la surprédiction des ordres sans file devant eux | Erreur de mesure de `validate` (pose mal retrouvée pour les tailles courantes), pas du modèle | 2026-10-28 | [simulateur maker](simulateur-maker.md) §10.3 |
| Ancien moteur de backtest Node (`sim.mjs`) comme outil courant | Porté en Rust à 0 écart ; le backtest exécute le code de l'engine | 2026-10-28 | [`backtest/README.md`](../backtest/README.md) |

## Projet et exploitation

| Piste | Pourquoi écartée | Date | Preuve |
| --- | --- | --- | --- |
| Interface web de pilotage du tracing (`ui/`, API d'état, contrat 3) | Remplacée par trois commandes interactives (start, stop, pull) | 2026-10-28 | décision utilisateur ; [`source-tracing/COMMANDES.md`](../source-tracing/COMMANDES.md) |
| Tracing sur `dublin-dry-run` ; garder `dublin-live` | Les deux instances sont arrêtées ; le tracing tourne sur la nouvelle instance « Up&Down-Tracing » | 2026-10-28 | décision utilisateur D10 ; [exploitation](exploitation.md) |
| Agrandir le disque EBS | Rester gratuit : rapatriement quotidien et garde disque | 2026-10-27 | décision utilisateur D3 |
| Ancienne application Node `trading-gap` (sessions, interface « Lagon ») | Remplacée par l'engine Rust et l'enregistreur ; service arrêté et supprimé des deux nœuds | 2026-10-28 | [ancien déploiement](recherche/exploitation-deploiement.md) |
