# Inventaire de l'UI copy-trading

Tout ce qui peut être affiché, d'où ça vient, et à quelle fréquence. L'UI est en lecture seule : elle tourne sur le PC, lit le journal du serveur par ssh (`tail -F`) et interroge elle-même les API publiques de Polymarket. Le bot n'est jamais sollicité.

## Sources

| Source | Contenu | Fréquence |
| --- | --- | --- |
| Journal du bot (`journal.jsonl`) | lignes `start`, `trade`, `order`, `state`, `check`, `stat`, `feed`, `warn`, `halt`, `stop` | temps réel (une ligne par événement), `state` chaque minute |
| data-api `/positions?user=` | ses positions : titre du marché, issue, parts, prix moyen, valeur, PnL $ et % | toutes les 30 s (côté PC) |
| data-api `/value`, `/activity` | valeur de son portefeuille, ses trades (contrôle) | 30 s |
| gamma `/markets?clob_token_ids=` | question du marché, issue (Yes/No…), date de fin, icône | une fois par token, en cache |
| CLOB `/book`, `/prices-history` | carnet d'un token (son niveau et le mien), historique du prix | à l'ouverture d'un marché dans l'UI |

## LUI (colonne gauche)

- **Portefeuille** : valeur totale (positions + cash) et sa courbe (journal `state.his_value`) ; nombre de positions.
- **Profits** : PnL latent par position et total (data-api `cashPnl`, `percentPnl`) ; PnL réalisé (`realizedPnl`).
- **Positions** : marché, issue, parts, prix moyen, prix actuel, valeur, PnL ; tri par valeur ou par PnL.
- **Ses ordres posés (déduits)** : token, sens, prix, nombre de fills, cumul $ et parts, âge, dernier fill, plus long silence (`trade.his_order`). Un ordre n'est connu qu'à son premier remplissage, ses ordres ouverts n'étant pas publics.
- **Fil de ses trades** : heure du match, sens, prix, $, parts, source (chaîne / secours API), part de son portefeuille `p` et `p_cum`.
- **Rythme** : fills par heure, taille médiane, côtés achat/vente par token (repérer le teneur de marché).

## MOI (colonne droite)

- **Trois comptes** : observé (mes tailles à ses prix), demandé (au meilleur prix à l'observation), réel (mes fills) ; valeur, PnL $ et %, cash, positions, non valorisées, et leurs courbes.
- **Positions** : parts, coût moyen, prix (milieu du carnet), valeur, PnL latent (`state.real_pos`, `state.marks`).
- **Mes ordres posés** : prix, taille, rempli, reste, file devant moi (paper), âge, compte à rebours avant annulation (`state.orders`, lignes `order`).
- **Fil de mes ordres** : pose, remplissage, déplacement, annulation (motif), rejet.
- **Décisions** : copié / ignoré avec motif (renfort, palier, engagement atteint, vente en attente d'un lot).
- **Latences** par segment : son match → observation → décision → envoi → accusé → fill, médiane / P90 / N ; `NULL` si non mesuré.

## ENTRE LES DEUX (le lien lui ↔ moi)

- Chaque trade de lui relié à mon ordre (même token, même prix) : écart de prix, de mise (ma mise en % de la sienne), délai.
- Glissements : demandé → réel, observé → réel, en $ et en % du capital.
- Annulations à tort : mes ordres annulés pour inactivité qu'il a remplis ensuite (`stat`), avec le silence qui a déclenché l'annulation.

## SANTÉ

- Mode (paper / live), stratégie active (exec, achat, vente, annulation), durée du run, dernier événement.
- Flux : chaîne et market (`feed` : valide, silence, déconnexion, resync), contrôles data-api (`check` ok / écarts), contrôle du sens des trades (`trade_side`), avertissements (`warn`, HTTP 429), arrêt (`halt`).

## DÉTAIL D'UN MARCHÉ (au clic)

- Question, issue, date de fin ; carnet en profondeur avec SON niveau et le MIEN marqués ; historique du prix avec ses fills et les miens posés dessus.
