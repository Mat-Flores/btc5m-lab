# Outils — IA de décision et sources de données

Légende :
- **[vérifié 30/10]** : lu dans une source le 30/10/2026 (lien donné) ;
- **[connu]** : connaissance antérieure, à revérifier avant usage.

## 1. IA de décision : JEV, Von et alternatives

| Outil | Ce que c'est | Licence / coût | Chiffres | Statut |
| --- | --- | --- | --- | --- |
| **JEV** (TypeSafe AI) | Modèle « System One » : choisit parmi des options et donne une probabilité pour chacune. N'écrit pas, ne résume pas | Fermé, API payante (directe ou via Vercel AI Gateway) | JevBench : composite 63,3, calibration 76,3, p50 0,65 s | [vérifié 30/10] ([HF blog](https://huggingface.co/blog/dddffgassa/jev-ai), [README Von](https://github.com/wfzyx/von)) |
| **Von** (wfzyx) | Alternative libre à JEV : encodeur ModernBERT de 395 M paramètres. Trois types de question : choix parmi K options, probabilité qu'une condition soit vraie, score sur une échelle | Apache-2.0, gratuit, CPU possible (image Docker CPU) | Composite 27,5, calibration 75,7 (88,4 avec chaînes) ; p50 0,34 s (0,096 s brut sur 4 vCPU, 0,023 s sur GPU A10G) | **Archivé, poids figés** [vérifié 30/10] ([github](https://github.com/wfzyx/von)) |
| **LiquidAI d1-omni-600M** | Successeur recommandé par Von. Mêmes types de questions, plus image et audio, contexte de 16 k tokens | Licence « lfm1.0 » (termes à lire) | Pas de latence publiée (version de recherche) | [vérifié 30/10] ([HF](https://huggingface.co/LiquidAI/d1-omni-600M)) |
| **JevK5** | Alternative ouverte à JEV | Apache-2.0 | 2e sur 76 au JevBench, selon le projet lui-même | [vérifié 30/10, chiffre non audité] ([github](https://github.com/allebee/jevk5)) |
| Autres (Laya, Nimble, opendecider, tinyjev…) | Liste de la communauté | variable | non audités | ([DataCamp](https://www.datacamp.com/blog/top-open-source-jev-alternatives), [écosystème](https://github.com/VoltAgent/typesafe-jev-ecosystem)) |
| **LLM généralistes** (Claude, etc.) avec recherche web | Lire un communiqué, extraire un chiffre, relier une info à un marché | API payante | Les agents avec recherche battent les agents sans recherche ; un ensemble de 3 à 7 exécutions bat un agent seul (synthèse) | [vérifié 30/10] ([EA Forum](https://forum.effectivealtruism.org/posts/Spyz3wESZu2eeqhDj/ai-forecasting-in-2026-what-11-analyses-say)) |

**Comment les utiliser.** Un modèle de décision (Von, d1, JEV) sert de **filtre rapide et local**. Exemples de questions :
- « Ce titre de dépêche concerne-t-il le marché X ? »
- « Le résultat est-il officiellement final ? »
- « Lequel de ces 5 marchés est touché ? »

Un LLM extrait ensuite le détail ; la décision de trader vient d'une **règle chiffrée** (prix contre probabilité calculée sur la donnée), jamais d'un avis de l'IA. Aucune de ces IA ne prévoit mieux qu'un marché liquide : voir le benchmark Coupe du monde dans README.md, et [ForecastBench](https://www.emergentmind.com/topics/forecastbench).

**Choix conseillé** :
- Von en local (gratuit, CPU, licence claire) pour prototyper le filtre ;
- d1-omni-600M si sa licence convient ;
- JEV seulement si la latence et la qualité de Von bloquent.

## 2. Sources de données par domaine

### Météo (marchés « température max à NYC le … »)

- **Source de résolution** : selon le marché, Weather Underground (tableau « Daily Observations », station LaGuardia KLGA) ou NOAA avec Weather Underground en secours. À lire marché par marché [vérifié 30/10] ([marché 28/09](https://polymarket.com/event/highest-temperature-in-nyc-on-september-28-2026)).
- **Relevés en temps réel** : METAR de KLGA (aviationweather.gov, gratuit), observations de l'API NWS (`api.weather.gov`, gratuite, sans clé) [connu].
- **Prévisions probabilistes** : ensembles via Open-Meteo (gratuit en usage non commercial), NBM du NWS (percentiles de température) [connu].
- **Marge** : après 15-17 h, la max du jour est presque acquise. Le relevé horaire donne la tranche gagnante avant que le marché ne la cote à 0,99.

### Sport

- **Flux officiel Polymarket** : topic `sports` (`sport_result`) du websocket RTDS, gratuit [vérifié 30/10] ([docs RTDS](https://docs.polymarket.com/developers/RTDS/RTDS-overview)).
- **Lignes de référence** : Pinnacle, réputé le plus fin (accès API restreint aux partenaires, à revérifier) [connu] ; The Odds API, agrégateur avec palier gratuit limité [connu]. Usage : comparer le prix Polymarket à la ligne sans marge du bookmaker ([exemple](https://clawarbs.com/blog/best-polymarket-trading-bot/)).
- **Données par sport** (gratuites, non officielles) [connu] :
  - NBA : endpoints stats.nba.com et scoreboards ;
  - MLB : StatsAPI ;
  - football : football-data.org, API-Football (freemium) ;
  - tennis et e-sport : flux des organisateurs.
- **Flux pro payants** : Sportradar, Genius Sports, Opta. Chers, réservés aux contrats [connu].
- **Contraintes** : ordres preneurs retardés de 1 à 3 s ; ordres limites annulés au début du match [vérifié 30/10] ([docs](https://docs.polymarket.com/concepts/order-lifecycle)). Le direct se joue donc en maker, ou sur des écarts qui durent plus de 3 s.

### Politique et élections

- Résultats officiels : sites des États et comtés (gratuits, formats hétérogènes) [connu].
- Appels de course : AP Elections API et Decision Desk HQ (payants) [connu].
- Comparaison inter-plateformes : Kalshi (probablement inaccessible depuis la France) [connu].
- Marge type : l'écart entre l'appel d'une course et la cotation à 0,99, et les règles de résolution (« certifié » ou « projeté »).

### Finance et économie

- Indicateurs : FRED (gratuit, clé), BLS et BEA avec heures de publication fixes, calendrier de la Fed [connu].
- Prix : le dépôt a déjà des adaptateurs Binance, OKX, Coinbase et Chainlink (`engine/adapters/`).
- Marchés concernés : chiffres de l'IPC, décisions de taux, seuils de prix. La donnée tombe à heure fixe, la vitesse de lecture fait la marge.

### Actualité générale

- GDELT (gratuit, mis à jour toutes les 15 min), flux RSS des agences et des institutions, X ou Bluesky (API payante ou limitée) [connu].
- Le filtre Von / d1 trie le flux. Un LLM extrait le fait.

## 3. Ce qui existe déjà en bots IA pour Polymarket

- Polystrat (Olas), l'agent BlockRun et les SDK Simmer : chiffres annoncés par leurs auteurs, non vérifiés ([CoinDesk](https://www.coindesk.com/tech/2026/03/15/ai-agents-are-quietly-rewriting-prediction-market-trading), [BlockRun](https://github.com/BlockRunAI/polymarket-agent)).
- Le framework d'agents officiel de Polymarket serait archivé depuis mai 2026, selon une seule source ([Turbine](https://www.turbinefi.com/blog/best-ai-algorithmic-trading-platforms-prediction-markets-2026)).
- Sécurité : des scripts tiers ont déjà vidé des wallets, 2,5 M$ tracés ([BleepingComputer](https://www.bleepingcomputer.com/news/security/polymarket-halts-trading-after-crypto-draining-script-breach-25m-traced/)). On garde notre propre chaîne de signature (clé chiffrée, jamais sur disque en clair).
