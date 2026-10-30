# Stratégies cumulables pour actu-bot

Frais Polymarket (docs du 30/10) :
- preneur : `fee = parts × taux × p × (1 − p)` ;
- taux : sport 0,05 · politique, finance, tech 0,04 · météo, économie, culture 0,05 · géopolitique 0 ;
- maker : jamais de frais, et 15 à 25 % des frais preneur reversés en rebate ([docs](https://docs.polymarket.com/polymarket-learn/trading/fees)).

Conséquence : **prendre le carnet ne coûte presque rien aux extrêmes** (à 0,97 : 0,05 × 0,97 × 0,03 = 0,15 % du prix) et **coûte cher au milieu** (à 0,50 : 2,5 % du prix).

## S1 — Résolu non réglé (« victoires assurées »)

- **Principe** : l'issue est déjà connue (match terminé, relevé officiel final, course appelée, chiffre publié), mais le marché cote encore 0,95-0,99 en attendant la résolution UMA. On achète, puis on récupère 1.
- **Marge** : 1 à 5 % par opération, immobilisée quelques heures. Frais minimes aux extrêmes.
- **Risque** : on perd tout sur une erreur de règle ou un litige UMA. À 0,97, une seule perte efface environ 32 gains ([guide](https://startpolymarket.com/strategies/bonding/)). D'après une étude citée de seconde main, les achats à ≥ 0,90 ne rapportent en moyenne que 0,83 ¢ par $ ([datawallet](https://www.datawallet.com/crypto/top-polymarket-trading-strategies)) : la marge vient uniquement de **savoir** l'issue, pas de « parier sur le favori ».
- **Règles** :
  - n'acheter que si la donnée **de la source de résolution exacte** confirme l'issue ;
  - un marché à la fois par source ;
  - plafond de 5 % du capital par marché ;
  - liste noire des marchés à règle ambiguë.
- **Outils** : flux `sport_result`, METAR et API NWS, sites électoraux ; Von pour vérifier que le texte affirme bien un résultat final.

## S2 — Météo (température max quotidienne)

- **Principe** : calculer la probabilité de chaque tranche de température à partir des prévisions d'ensemble et des relevés du jour, puis acheter les tranches sous-cotées et vendre les surcotées, en ordres maker.
- **Marge** : un modèle mieux calibré que la foule sur des marchés quotidiens peu liquides, plus la fin de journée (la max est presque acquise, voir S1).
- **Risque** : erreur de modèle ; écart entre la source de résolution et le relevé officiel (cas 29 °F / 30 °F à LaGuardia).
- **Mesure** : le Brier score du modèle contre celui du prix du marché, chaque jour pendant 30 jours, avant toute mise réelle.

## S3 — Marché qui n'a pas encore réagi (actualité, sport en direct)

- **Principe** : une information tombe (blessure, but, dépêche, chiffre macro) et on achète avant que le prix ne bouge.
- **Marge** : la plus grosse possible, mais la plus disputée. Un bot sport aurait gagné 8 M$ en exploitant le décalage, chiffre non vérifié ([Phemex](https://phemex.com/news/article/sports-bot-earns-8-million-on-polymarket-by-exploiting-time-lag-55871)). Les écarts entre plateformes se ferment en 15 à 30 s, selon une source.
- **Obstacles** :
  - ordres preneurs retardés de 1 à 3 s en sport ;
  - frais de 2,5 % au milieu de l'échelle ;
  - concurrence de flux payants (Sportradar et autres) ;
  - le flux vidéo a 45 s de retard sur la réalité, alors que le flux de données n'a que 3 s, d'où l'intérêt des données ([source](https://x.com/0xRicker/status/2008998834580427101)).
- **Version réaliste** : comparer en continu le prix Polymarket à la ligne d'un bookmaker fin (sans sa marge), et poser des ordres maker quand l'écart dépasse frais + 2 ticks.

## S4 — Maker, rebates et récompenses de liquidité

- **Principe** : poser des deux côtés sur des marchés que l'on comprend (ceux de S1 et S2), pour toucher l'écart, les rebates et les récompenses.
- **Marge** : le wallet copié a touché 376 k$ de rebates et 64 k$ de récompenses (`/v2/user-pnl`).
- **Risque** : se faire remplir juste avant une nouvelle (sélection adverse). S1 et S2 donnent justement l'information qui protège.
- **Briques existantes** : ordres GTC, annulation et file d'attente en paper (`copy-trading/src/maker.rs`, engine).

## S5 — Signaux de wallets (pont avec le copy-trading)

- **Principe** : utiliser ses gros ordres (≥ 10 k$, la classe gagnante : +14 à +17 % sur 30 jours) comme signal, combiné avec S3 (ligne du bookmaker) pour filtrer.
- **Marge** : sa marge prouvée, sans copier ses petits paris perdants.

## Cumul et garde-fous

- Un budget par stratégie : par exemple 50 % copy-trading, 20 % S1, 20 % S2, 10 % S4. S3 n'est lancé qu'après des preuves.
- Plafonds par marché (5 %) et par jour ; arrêt automatique d'une stratégie à −10 % de son budget.
- Paper d'abord, avec le même journal à 3 comptes (observé, demandé, réel) et les latences par segment que copy-trading.
- Revue hebdomadaire : PnL par stratégie, Brier score (S2), taux de litiges (S1), taux de remplissage maker (S4).
