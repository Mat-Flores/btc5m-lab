# Interface et produit : principes retenus

Ce fichier ne garde que les **principes** produit et d'interface de l'ancien dépôt `btc5m-lag-sim` qui restent valables pour l'interface du dépôt `updown5m-lab` (`ui/`). Il ne reprend ni la charte graphique complète, ni les composants, ni les maquettes de l'ancienne application « Lagon » (huit sources, comptes Instantané / Réaliste / Réaliste + 1 s, stratégies par source), qui sont périmés depuis la purge du 2026-10-22 et la migration.

Convention : chemins relatifs au dépôt source `btc5m-lag-sim` (lecture seule), cités `fichier:ligne` ; ceux marqués « dépôt cible » sont relatifs à `updown5m-lab`. `[à vérifier]` marque ce qui n'est pas établi. Voir aussi [journal-decisions.md](journal-decisions.md) et `ui/README.md` (API d'état du tracing).

## 1. Périmètre de la nouvelle interface (dépôt cible)

La nouvelle interface est volontairement réduite : la liste des sources avec un interrupteur on/off pour le tracing ; l'état de chaque source (connectée, âge du dernier message, messages/s, trous du jour, taille du fichier du jour) ; les jours validés présents sur le serveur. « Rien d'autre » (`PROMPT-MIGRATION.md:91-95`, dépôt cible).

Contraintes imposées par la migration : conception par le skill `impeccable:impeccable` et ses sous-agents, jamais de maquette écrite à la main ; le plugin est désactivé dans `settings.json` et n'est activé que dans `.claude/settings.local.json` (gitignoré) ; Impeccable établit `PRODUCT.md` (nouvel objectif) et `DESIGN.md` (repris de l'ancien dépôt) ; l'interface écoute sur `127.0.0.1` uniquement et s'atteint par tunnel SSH, jamais d'exposition publique (`PROMPT-MIGRATION.md:88-90, :96`, dépôt cible ; voir [skills.md](skills.md)).

## 2. Principes produit retenus

Les cinq principes de l'ancien `PRODUCT.md` (`PRODUCT.md:83-91`) :

| # | Principe | Valable pour la nouvelle interface |
| --- | --- | --- |
| 1 | Le live d'abord, toujours | Oui : l'interface ne doit jamais gêner l'enregistrement ; voir §3 |
| 2 | Un moteur, une vérité : la provenance de chaque chiffre est affichée (live, rejeu, aperçu, provisoire) et jamais mélangée en silence | Oui, adapté : distinguer ce qui vient du statut en direct, d'un manifeste de jour validé, ou d'une donnée absente (« stale ») |
| 3 | D'abord l'état en un coup d'œil, ensuite le détail | Oui |
| 4 | Tout chiffre est comparable (source contre source, jour contre jour) | Oui, au niveau sources et jours |
| 5 | Incertitude honnête : petits échantillons, résultats provisoires et backtests optimistes sont signalés | Oui |

Utilisateur : un seul opérateur, propriétaire, qui fait de la recherche personnelle, seul, sur écran de portable (~1440-1536 px), via tunnel SSH, sessions longues ; il connaît bien le domaine mais n'est pas développeur front-end, donc chaque état et chaque action possible doivent être évidents (`PRODUCT.md:9-15`). Positionnement : un instrument de recherche, ni terminal de trading ni outil de graphiques (`PRODUCT.md:32`). Desktop uniquement (`PRODUCT.md:58`).

## 3. Contraintes fermes

- **La performance du live est sacrée** : l'interface ne ralentit jamais la boucle d'enregistrement ou de trading ; aucun travail lourd sur le serveur pour l'interface, aucune grosse charge utile sur le flux, ne dessiner que ce qui est visible ; cette contrainte passe avant toute fonctionnalité (`PRODUCT.md:51-53` ; `SPEC.md:263-273`). Dans l'ancienne application : un instantané toutes les 250 ms pour la seule source affichée (`DESIGN.md:479-480`).
- **Mode sans équivoque** : l'interface ne laisse jamais de doute sur le mode ; en papier rien ne suggère de l'argent réel ; en réel, bandeau rouge et plaque « RÉEL » (`PRODUCT.md:73-75`). Sans objet pour le tracing (aucun ordre), mais le principe de ne jamais laisser de doute sur l'état s'applique (source désactivée, données figées).
- **Accès** : serveur sur `127.0.0.1`, tunnel `ssh -L` (`SPEC.md:277`). L'ancienne application ajoutait un jeton et un contrôle de l'en-tête `Host` (`SPEC.md:278-281`) ; la nouvelle interface du tracing choisit de ne pas authentifier (loopback + tunnel) `[à vérifier : décision propre au dépôt cible, voir ui/README.md]`.
- **Langue** : français, nombres au format fr-FR (`1 234,56 $`, `12,5 ¢`), signe moins typographique (`PRODUCT.md:67` ; `SPEC.md:414`).
- **Accessibilité minimale** : tout au clavier ; lisible en clair et en sombre ; la couleur ne porte jamais seule l'information de gain ou de perte (`PRODUCT.md:93-96`). Rôles et libellés sur les tableaux, focus visible, contrastes 4,5:1 pour le texte, boutons à icône seule avec libellé masqué (`DESIGN.md:486-491`).
- **Code lisible et modifiable par un humain** ; animations souhaitées mais côté navigateur, sans coût pour le serveur (`PRODUCT.md:54-55, :59-60`).

## 4. Charte « Lagon » : ce qui est retenu

La charte complète (jetons de couleur, typographie, composants) est dans `DESIGN.md` du dépôt source, v1.1 réconciliée avec le code livré le 2026-10-15 (`DESIGN.md:223-225`). Principes à reprendre :

| # | Principe | Source |
| --- | --- | --- |
| 1 | Le chiffre d'abord : l'écran est un instrument, chiffres grands et tabulaires, étiquettes discrètes | `DESIGN.md:234` |
| 2 | La couleur porte un sens, jamais une décoration ; rien d'autre n'est coloré que le sens (modèle / Polymarket / gain-perte) | `DESIGN.md:235-236` |
| 3 | La couleur ne porte jamais seule l'information : signe `+`/`−`, libellé ou forme l'accompagnent | `DESIGN.md:237` |
| 4 | La provenance est visible | `DESIGN.md:238` |
| 5 | Le mouvement explique un changement, il n'orne pas ; il se fait dans le navigateur, sans coût pour le serveur | `DESIGN.md:239-240` |

Thème clair par défaut, sombre équivalent complet (choisi par l'opérateur, mémorisé dans le navigateur, aucun thème ne suit le système) (`DESIGN.md:244-245`). Les couleurs, espacements, rayons et durées sont des jetons CSS ; le code ne réécrit jamais une couleur de sens en dur (`DESIGN.md:226-228`). Barres de défilement maison, superposées, visibles seulement quand un défilement est possible (`PRODUCT.md:56-57`).

États à prévoir (formes) : chargement par squelette à la forme du contenu attendu ; vide par une phrase courte qui dit quoi faire ; « données figées depuis n s » au-delà de 3 s ; « serveur injoignable » (`DESIGN.md:465-469`). Ces états sont directement transposables aux notions de source muette et de statut périmé (`stale`) du tracing. Graphiques en SVG, au plus ~300 points par série ; historiques gardés dans le navigateur ; onglet caché = on ne dessine rien (`DESIGN.md:482-484`).

L'identité de départ garde l'ADN : bleu « modèle » `#2a78d6`, orange Polymarket `#eb6834`, vert gain `#0b7a0b`, rouge perte `#c42f2f`, chiffres tabulaires, Segoe UI Variable (`PRODUCT.md:68-71` ; `.impeccable/surfaces/ui-src-app-svelte.md:48-50`). Pour le tracing, la sémantique « gain / perte » n'a pas de sens ; seul le sens « connecté / coupé / désactivé » reste utile `[à vérifier : à trancher par Impeccable pour la nouvelle UI]`.

## 5. Décisions de conception de l'ancienne interface qui restent pertinentes

| Décision | Contenu | Source |
| --- | --- | --- |
| Stack | Svelte 5 + Vite avec build ; icônes `@lucide/svelte` ; serveur Node pur sans framework ; l'interface est construite sur le poste puis envoyée en fichiers statiques | `SPEC.md:369-373` ; `deploy/push-node.sh:13-14` |
| Contrainte de la nouvelle UI | Serveur `node:http` seul, aucune dépendance, pas de build (contrat de la migration, `ui/README.md`) | contrats de migration (dépôt cible) |
| Un onglet = une tâche | Live, Stratégies, Backtest, Nœud et latence ; Vue d'ensemble et Analyse supprimés ; plus de bouton « Paramètres » global : les seuils affichés sont les paramètres | `.impeccable/surfaces/ui-src-app-svelte.md:14-15, :23-25` ; `PRODUCT.md:39-42` |
| Un axe de temps unique pour la fenêtre de 5 min | Graphique, signaux, ordres et règlement s'alignent sur le même axe ; refus de la pile de panneaux où santé, décision et résultat sont au même niveau | `.impeccable/surfaces/ui-src-app-svelte.md:45-47` |
| Pouls du nœud | 4 tuiles vitales avec courbe récente tenue dans le navigateur, une carte par flux ; détail du serveur servi par un point d'API ponctuel, jamais par le flux temps réel | `.impeccable/surfaces/ui-src-app-svelte.md:39-42` |
| Raccourcis | `l`, `b`, `n`, `T` (thème), `?` et `Ctrl K` (palette de commandes), `Échap` | `DESIGN.md:488-489` ; `SPEC.md:399-402` |
| Vocabulaire | fenêtre, strike, Chainlink, source, nœud, session, rejeu, parité, etc. | `PRODUCT.md:61-63` |

## 6. Ce qui est périmé et retiré

- Huit sources de prix simulées côte à côte, trois comptes (Instantané, Réaliste, Réaliste + 1 s), bibliothèque de stratégies (jusqu'à 3 par source), rejeu piloté, comparaison de nœuds : retirés avec la purge de l'ancien moteur Node du 2026-10-22 (`docs/reprise-integration-engine.md:12-15`).
- Fonctionnalités notées « non fait » dans le tableau de l'ancienne interface : comparateur de nœuds dans l'interface, comparaison de deux backtests (`SPEC.md:430, :434`).
- L'interface du backtest (Live alimenté par le rejeu, frise des sessions) n'a pas d'équivalent dans le périmètre actuel : le backtest du dépôt cible est un outil en ligne de commande (`PROMPT-MIGRATION.md:56`, dépôt cible) `[à vérifier : interface du backtest hors périmètre, à confirmer avec l'utilisateur]`.
- La section 11 de `SPEC.md` est à confirmer avec l'utilisateur, car l'interface est en cours de réécriture (plan de documentation de la migration).

## 7. Points `[à vérifier]`

- Authentification de la nouvelle interface (aucune, loopback + tunnel) : contrat de la migration, non issu de l'ancien dépôt.
- Palette et sémantique de couleur pour le tracing (pas de gain/perte) : à définir par Impeccable.
- Présence de `ui/README.md` et de l'API d'état au moment de la lecture de ce fichier : rédigé en parallèle par un autre agent.
