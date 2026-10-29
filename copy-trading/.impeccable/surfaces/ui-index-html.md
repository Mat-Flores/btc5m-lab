---
version: 1
slug: "ui-index-html"
primary_target: "ui/index.html"
related_targets: []
---

# Surface : tableau de bord copy-trading (copy-trading/ui)

Mode : Operate. Un seul opérateur, PC grand écran (≥ 1440 px), page ouverte des heures, lecture seule.

Tâche : voir en temps réel ce que fait le wallet suivi et ce que fait le bot, et surtout les décalages entre les deux (temps, prix, taille, file, performance). Source : journal du bot (ssh tail -F) + API publiques Polymarket via le serveur local.

Direction : fixée par l'utilisateur (identité Polymarket, informations fusionnées, graphique des décalages) ; maquette validée `.impeccable/mocks/fusion.html`. Réactivité et associations entre composants exigées.

## Direction contract

THESIS : un seul écran où lui et moi partagent chaque vue, et où l'écart entre nous se voit comme une forme (ligne, segment, file) au lieu d'être lu dans deux colonnes ; refuse le tableau de bord en deux moitiés miroir et le mur de cartes de chiffres.

OWN-WORLD : identité Polymarket sombre : fond #0C0E13, panneaux #15181F / #1C1F27, filets #2D3037, Inter, prix en ¢, pastilles Oui/Non vertes et rouges teintées, icônes de marché carrées arrondies ; orange #FF9900 = lui, bleu #3262FF = moi, partout et seulement pour ça ; rouge = coût ou perte.

STORY : l'opérateur voit d'un coup d'œil si la copie suit (rendement, délai, prix, exécution), choisit un marché, comprend où il a posé, où j'ai posé, ce qui a été rempli et ce qui attend, et repère chaque décalage dans la cascade.

FIRST VIEWPORT : barre (wallet, mode, stratégie, santé, période) ; rangée haute : performance lui/moi avec l'écart grisé, quatre tuiles lui | moi | écart ; rangée basse : liste des marchés (280 px), centre (marché sélectionné : prix, ses fills, mes ordres, file ; cascade des décalages), droite (positions comparées, carnet avec nos niveaux, ma place dans la file).

FORM : maquette validée par l'utilisateur, direction épinglée (pas de tirage retenu) ; seed: none (unavailable).

Signature : la cascade des décalages (échelle log 0 → 6 h, segments son match → ma détection → mon ordre → mon fill) liée par survol et clic au graphique de prix et à la liste.

FINISH : unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
