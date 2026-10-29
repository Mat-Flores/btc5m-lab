# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Délégué : page statique HTML/CSS/JS sans framework, servie par un petit serveur local Python (bibliothèque standard, aucune dépendance) qui lit le journal du bot par ssh (`tail -F`). Choix : léger, rien à installer, aucune charge sur le serveur de trading.

## Users

Un seul utilisateur : l'opérateur du bot. Il suit en continu, sur son PC (grand écran, ≥ 1440 px), un wallet Polymarket public et la copie que le bot en fait, en paper puis en live. Il veut tout voir d'un coup d'œil sans lire le terminal.

## Product Purpose

Tableau de bord temps réel de `copy-trading` : voir côte à côte ce que fait le wallet suivi (à gauche) et ce que fait le bot (à droite) — trades, ordres posés, portefeuilles, profits — et repérer tout écart (latence, prix, ordres non remplis, annulations à tort).

## Positioning

Il ne montre que des faits observés (journal du bot, API publiques Polymarket) ; une valeur non mesurée s'affiche comme telle, jamais estimée.

## Operating Context

- Le bot tourne sur un serveur à Dublin et écrit `journal.jsonl` (lignes `trade`, `order`, `state`, `check`, `stat`, `feed`, `start`, `stop`, `halt`).
- L'opérateur lance l'UI sur son PC ; elle reste ouverte des heures à côté d'autres fenêtres.
- Le wallet suivi est un teneur de marché sur des marchés longs (élections, NBA) : ordres posés au repos, remplis par morceaux.

## Capabilities and Constraints

- Lecture seule : l'UI ne parle jamais au bot ; start/stop restent dans `copy-trading/deploy/`.
- Ne doit pas impacter le trading : aucun processus supplémentaire sur le serveur hormis la lecture ssh du journal.
- Ses ordres ouverts ne sont pas publics : ils sont déduits de ses remplissages.
- Vue mobile : secours seulement (colonnes empilées).

## Evidence on Hand

Journaux réels du run paper du 2026-10-29 (473 trades, 11 copies) et du run de test maker `~/ct-maker` sur le serveur.

## Product Principles

1. Lui à gauche, moi à droite : toute donnée a son jumeau en face.
2. Un fait observé ou rien : NULL plutôt qu'une estimation.
3. L'écart d'abord : ce qui diverge entre lui et moi doit sauter aux yeux.
4. Zéro impact sur le trading.
