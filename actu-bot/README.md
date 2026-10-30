# actu-bot — bot de trading sur l'actualité (recherche)

Recherche du 30/10/2026. Question posée : un bot maison (sport, politique, météo, finance), appuyé sur des IA de décision comme Von ou JEV, serait-il plus rentable que le copy-trading ? Et quelles stratégies « sûres » cumuler ?

- [OUTILS.md](OUTILS.md) : les IA de décision (JEV, Von et leurs alternatives) et les sources de données par domaine.
- [STRATEGIES.md](STRATEGIES.md) : les stratégies possibles, classées, avec leur source de marge, leur risque et la façon de les mesurer.

## Verdict

**À court terme, non : le copy-trading corrigé reste plus rentable par heure de travail.**

1. Une IA qui « juge » l'actualité ne bat pas le marché. Sur 104 matchs de la Coupe du monde 2026, aucun des quatre agents LLM testés n'a battu le Brier score du bookmaker. Leur ROI va de −18 % à +10 %, et parier à plat sur le favori du marché a rapporté plus que chacun d'eux ([arXiv 2607.17765](https://arxiv.org/abs/2607.17765)).
2. Les IA de décision comme JEV ou Von sont des **classifieurs rapides** : elles choisissent parmi des options et donnent une probabilité. Elles servent à router, vérifier ou classer une information. Elles ne savent rien que le marché ignore.
3. Sur Polymarket, la marge d'un bot d'actualité vient de trois choses :
   - **une donnée plus rapide** (relevé météo officiel, score, résultat publié) ;
   - **la maîtrise des règles de résolution** : par exemple, Polymarket a lu 29 °F à LaGuardia là où le relevé officiel du NWS donnait 30 °F, parce que la règle désignait le tableau horaire de Weather Underground ([Ryan Maue](https://x.com/RyanWeather/status/2000941328306463201)) ;
   - **l'exécution maker**, sans frais et avec rebates.

   Ce sont des briques d'ingénierie, pas de jugement.
4. Le wallet copié a fait +11 M$ en 9 mois avec une marge déjà prouvée. Rejouée à 1 000 $ en proportionnel, cette marge se récupère (voir `copy-trading/ANALYSE-2026-10-30.md`). Un bot maison part de zéro et doit d'abord prouver la sienne.

**Mais il vaut la peine d'en construire un, étroit et mesurable**, en complément et non en remplacement, pour deux raisons :
- **diversifier** : le copy-trading dépend d'un seul parieur très volatil (−30 % en 3 jours) ;
- **réutiliser l'existant** : le dépôt a déjà presque tout ce qu'il faut (engine Rust, signature et ordres GTC, carnets en websocket, journal avec 3 comptes de PnL, déploiement à Dublin).

## Plan proposé (du plus sûr au plus spéculatif)

| Phase | Stratégie (voir STRATEGIES.md) | Pourquoi d'abord | Critère pour passer à la suite |
| --- | --- | --- | --- |
| 1 | **S1 Résolu non réglé** : « victoires assurées » | Risque de modèle nul ; frais preneur ~0,15 % à 0,97 ; données gratuites | 2 semaines en paper, 0 perte hors litige, rendement ≥ 1 %/semaine du capital engagé |
| 2 | **S2 Météo** (températures max des villes) | Marchés quotidiens = retour rapide ; prévisions d'ensemble et relevés gratuits ; règles connues | Brier de notre modèle < Brier du prix du marché sur 30 jours |
| 3 | **S4 Maker + récompenses** sur nos marchés S1/S2 | Rebates et récompenses de liquidité (376 k$ pour le wallet copié) | PnL de tenue de marché ≥ 0 hors récompenses |
| 4 | **S3 Marché pas encore réagi** (actualité, sport en direct) | La plus grosse marge, mais la plus disputée et la plus chère à construire | Seulement si les phases 1-2 sont rentables |

On cumule les stratégies avec un capital séparé par stratégie, des plafonds par marché et par jour, un arrêt automatique sous un seuil de baisse, et le même journal à 3 comptes que le copy-trading (observé, demandé, réel).

## Points à vérifier avant de miser

- **Accès réglementaire depuis la France** : l'ANJ a bloqué Polymarket en 2024 (connaissance antérieure, non revérifiée aujourd'hui). Le bot tourne à Dublin, mais c'est le titulaire du compte qui compte.
- **Règles de résolution** : les lire marché par marché ; la source a déjà changé d'un marché météo à l'autre.
- **Frais réels** du marché visé : lire le champ `feeRateBps` du CLOB plutôt que les barèmes des sites tiers, qui se contredisent.
