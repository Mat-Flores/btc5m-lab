---
name: Copy-trading
description: Tableau de bord temps réel, lecture seule, où le wallet suivi et sa copie partagent chaque vue.
colors:
  bg: "#0C0E13"
  s1: "#15181F"
  s2: "#1C1F27"
  s3: "#242833"
  ln: "#2D3037"
  t1: "#F4F5F7"
  t2: "#B6BBC4"
  t3: "#9AA0A9"
  him: "#FF9900"
  him2: "#FFB547"
  me: "#3262FF"
  me2: "#5485FF"
  yes: "#42C772"
  no: "#E23939"
  no2: "#FF7A7A"
typography:
  figure-xl:
    fontFamily: "Inter, system-ui, -apple-system, Segoe UI, sans-serif"
    fontSize: "22px"
    fontWeight: 700
  figure:
    fontFamily: "Inter, system-ui, -apple-system, Segoe UI, sans-serif"
    fontSize: "20px"
    fontWeight: 600
    letterSpacing: "-0.02em"
  title:
    fontFamily: "Inter, system-ui, -apple-system, Segoe UI, sans-serif"
    fontSize: "16px"
    fontWeight: 600
    lineHeight: 1.25
  headline:
    fontFamily: "Inter, system-ui, -apple-system, Segoe UI, sans-serif"
    fontSize: "14px"
    fontWeight: 600
  body:
    fontFamily: "Inter, system-ui, -apple-system, Segoe UI, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.35
    fontFeature: "tnum"
  label:
    fontFamily: "Inter, system-ui, -apple-system, Segoe UI, sans-serif"
    fontSize: "12px"
    fontWeight: 600
  axis:
    fontFamily: "Inter, system-ui, -apple-system, Segoe UI, sans-serif"
    fontSize: "11px"
    fontWeight: 400
rounded:
  panel: "12px"
  control: "8px"
  pill: "6px"
  tag: "5px"
  bar: "2px"
spacing:
  xs: "6px"
  sm: "8px"
  md: "10px"
  lg: "14px"
  xl: "16px"
components:
  panel:
    backgroundColor: "{colors.s1}"
    rounded: "{rounded.panel}"
    padding: "12px 14px"
  chip:
    backgroundColor: "{colors.s2}"
    textColor: "{colors.t2}"
    rounded: "{rounded.control}"
    padding: "0 12px"
    height: "30px"
  chip-hover:
    backgroundColor: "{colors.s3}"
  segment:
    backgroundColor: "{colors.s2}"
    textColor: "{colors.t3}"
    typography: "{typography.label}"
    rounded: "{rounded.pill}"
    padding: "5px 11px"
  segment-active:
    backgroundColor: "{colors.ln}"
    textColor: "{colors.t1}"
  market-row:
    padding: "9px 14px"
  market-row-hover:
    backgroundColor: "{colors.s2}"
  market-row-selected:
    backgroundColor: "{colors.s3}"
    rounded: "{rounded.control}"
  pill-yes:
    textColor: "{colors.yes}"
    typography: "{typography.label}"
    rounded: "{rounded.pill}"
    padding: "3px 8px"
  pill-no:
    textColor: "{colors.no2}"
    typography: "{typography.label}"
    rounded: "{rounded.pill}"
    padding: "3px 8px"
  tag-him:
    textColor: "{colors.him2}"
    typography: "{typography.label}"
    rounded: "{rounded.tag}"
    padding: "2px 8px"
  tag-me:
    textColor: "{colors.me2}"
    typography: "{typography.label}"
    rounded: "{rounded.tag}"
    padding: "2px 8px"
---

# Design System: Copy-trading

## Overview

**Creative North Star: « L'écart comme forme »**

Un seul écran où le wallet suivi (lui) et le bot (moi) partagent chaque vue. L'écart entre eux ne se lit pas en comparant deux colonnes : il se voit, comme une ligne grisée entre deux courbes, un segment qui s'allonge dans la cascade ou une barre de file qui avance. Le système refuse le tableau de bord en deux moitiés miroir et le mur de cartes de chiffres.

Le monde visuel reprend l'identité sombre de Polymarket : fonds bleu-noir étagés, filets fins, Inter, prix en ¢, pastilles Oui/Non teintées, icônes de marché carrées arrondies. Il s'agit d'un poste de travail (mode Operate) pour un opérateur unique sur grand écran (≥ 1440 px), ouvert des heures : densité élevée, chiffres tabulaires, mouvement réduit à ce qui signale une arrivée ou un changement de valeur.

Un fait observé ou rien : une valeur non mesurée s'affiche grisée (« — », « non mesuré »), jamais estimée.

**Key Characteristics:**
- Deux couleurs d'acteur réservées : orange = lui, bleu = moi, partout et seulement pour ça.
- Aplats tonaux sans ombre ; la profondeur vient de l'étagement bg → s1 → s2 → s3.
- Chiffres tabulaires partout (`font-variant-numeric: tabular-nums`).
- Associations : survoler ou cliquer un élément allume ses jumeaux (liste, graphique, cascade) et atténue le reste.

## Colors

Une base bleu-noir quasi neutre, deux accents d'acteur et un couple vert/rouge réservé au sens (Oui/Non, gain/perte).

### Primary
- **Orange Lui** (#FF9900) : tout ce qui appartient au wallet suivi : sa courbe, ses fills, son segment de détection dans la cascade, l'éclair d'arrivée d'un nouveau trade. Variante claire **Orange Lui clair** (#FFB547) pour le texte sur fond teinté (puce paper, étiquette « lui »).

### Secondary
- **Bleu Moi** (#3262FF) : tout ce qui appartient au bot : sa courbe, ses ordres, ses fills, la barre de file, l'anneau de sélection, la sélection de texte. Variante claire **Bleu Moi clair** (#5485FF) pour le texte et l'anneau de focus.

### Tertiary
- **Vert Oui** (#42C772) : Oui, gain, santé du flux (point vert).
- **Rouge Non** (#E23939) : Non, perte, coût, alerte, secours API dans la cascade, annulation démentie. **Rouge Non clair** (#FF7A7A) pour le texte sur fond teinté.

### Neutral
- **Nuit** (#0C0E13) : fond de page et de la barre collante.
- **Panneau** (#15181F) : surface des panneaux.
- **Panneau relevé** (#1C1F27) : puces, segments, survol de ligne.
- **Panneau sélectionné** (#242833) : ligne sélectionnée, icône vide, piste neutre.
- **Filet** (#2D3037) : bordures, séparateurs, segment actif, pistes de barres.
- **Texte** (#F4F5F7), **Texte secondaire** (#B6BBC4), **Texte discret** (#9AA0A9) : valeurs, libellés, axes et valeurs nulles.

### Named Rules
**La règle des deux voix.** L'orange et le bleu désignent un acteur, jamais une catégorie, un état ou une décoration. Un élément qui n'est ni à lui ni à moi reste neutre.

**La règle du rouge qui coûte.** Le rouge signifie coût, perte, Non ou alerte, rien d'autre. Un écart défavorable passe en rouge, un écart favorable en vert, un écart neutre en gris.

Les teintes de fond des pastilles sont l'accent à 14–20 % d'opacité (`rgba(…, .14)` à `.2`), jamais un aplat plein.

## Typography

**Display Font:** Inter (avec system-ui, -apple-system, Segoe UI)
**Body Font:** Inter
**Label/Mono Font:** Inter avec chiffres tabulaires (pas de police mono)

**Character:** une seule famille, celle de Polymarket ; la hiérarchie passe par la graisse et la taille, à pas serrés, pas par un contraste de familles.

### Hierarchy
- **Chiffre XL** (700, 22 px) : cote du marché sélectionné.
- **Chiffre** (600, 20 px, −0.02em) : valeurs des tuiles lui | moi.
- **Title** (600, 16 px, 1.25) : question du marché sélectionné, logo.
- **Headline** (600, 14 px) : titres de panneau.
- **Body** (400–500, 13 px, 1.35) : lignes, carnet, valeurs courantes.
- **Label** (600, 12 px) : en-têtes de tuile en Texte discret, puces, segments, pastilles, étiquettes.
- **Axe** (400, 11 px) : graduations de la cascade.

### Named Rules
**La règle des colonnes qui ne bougent pas.** Toute valeur numérique est en chiffres tabulaires pour que les colonnes restent alignées quand les valeurs changent en direct.

## Layout

Grille fixe pensée pour ≥ 1440 px (`min-width: 1280px` sur le corps). Barre collante de 60 px. Rangée haute : performance (minmax(380px, 34 %)) puis quatre tuiles égales. Rangée principale à la hauteur de la fenêtre (`calc(100vh − 60px − 238px)`, min 640 px) : liste des marchés 280 px | centre fluide | colonne droite 312 px. Gouttière et marge de page : 16 px ; marge interne des panneaux : 14 px horizontalement, 12 px verticalement.

Les zones longues défilent à l'intérieur de leur panneau (liste, cascade, carnet), jamais la page. Sous 1280 px tout s'empile (secours, rien n'est coupé) ; sous 640 px la cascade perd son tracé et ne garde que les textes.

## Elevation & Depth

Système plat. Aucune ombre portée : la profondeur vient uniquement de l'étagement des fonds (Nuit → Panneau → Panneau relevé → Panneau sélectionné) et de filets de 1 px. La seule `box-shadow` est un anneau intérieur de 1 px Bleu Moi sur la ligne de marché sélectionnée : c'est un état, pas une élévation.

### Named Rules
**La règle du plat.** Rien ne flotte. Un état (survol, sélection, focus) monte d'un cran de fond ou reçoit un anneau, jamais une ombre.

## Shapes

Coins arrondis à pas réguliers : panneaux 12 px, contrôles et icônes de marché 8 px, pastilles et segments 6 px, étiquettes 5 px, barres 2 px (file : 4 px). Icônes de marché carrées arrondies (36 px en liste, 40 px en tête, 20 px dans la cascade), jamais rondes ; seul l'avatar du wallet est rond.

## Components

### Buttons
Pas de bouton d'action : l'UI est en lecture seule. Les seuls contrôles sont des sélecteurs.
- **Segments (période, filtres)** : piste Panneau relevé, boutons 12 px/600 en Texte discret ; actif = fond Filet et Texte (`aria-pressed=true`) ; survol = Texte. Transition de fond et couleur 150 ms.
- **Lien « Suivre le direct »** : texte Bleu Moi clair, survol fond Panneau sélectionné.
- **Focus** : contour 2 px Bleu Moi clair, décalé de 2 px, rayon 6 px.

### Chips
- **Style** : 30 px de haut, rayon 8 px, fond Panneau relevé, texte secondaire, valeur en gras Texte.
- **États** : paper = fond orange 14 %, texte Orange Lui clair ; live et alerte = fond rouge 18 %, texte Rouge Non clair. Point d'état 7 px : vert (sain), orange (attention), rouge (panne), gris (arrêt).

### Cards / Containers
- **Corner Style** : 12 px.
- **Background** : Panneau, filet 1 px.
- **Shadow Strategy** : aucune (voir Elevation & Depth).
- **Internal Padding** : 12 px × 14 px.
- **Tuile lui | moi** : en-tête Label discret, deux lignes « lui » / « moi » (étiquette d'acteur 12 px colorée, Chiffre, détail discret à droite) séparées par un filet, puis la pastille d'écart en bas (fond rouge, vert ou neutre à 14 %).

### Navigation
Barre collante de 60 px sur fond Nuit, filet bas : logo, puces (wallet, mode, stratégie, santé), espace, segments de période, horloge 12 px discrète.

### Ligne de marché
Grille icône 36 px | question sur 2 lignes + deux barres de part (orange lui, bleu moi, 4 px) | pastille Oui/Non et détail. Survol ou jumeau allumé = Panneau relevé ; sélection = Panneau sélectionné, anneau Bleu Moi, rayon 8 px.

### Carnet
Niveaux de 27 px : prix | profondeur en barre de fond | taille ; mes niveaux sur fond bleu à 8 % ; étiquettes « lui » / « moi » à côté du prix ; ligne de spread entre filets. Sous le carnet, la barre de file (8 px, piste Filet, remplissage Bleu Moi animé en 400 ms) montre ma place.

### Cascade des décalages (signature)
Une ligne par trade de lui : heure | son action | tracé | ma réaction. Le tracé, en SVG, suit une échelle logarithmique de 0,1 s à 6 h : segment orange de son match à ma détection (rouge si détecté par le secours API, pointillé gris si le match n'est pas daté), segment gris de décision, puis mon ordre en trait de 3 px (bleu plein si rempli, pointillé sinon) terminé par un disque bleu (rempli), un anneau (en file, qui s'allonge à chaque seconde) ou une croix (annulé ; rouge si l'annulation est démentie). Un nouveau trade arrive avec un éclair orange de 1,6 s. Survoler une ligne allume son point sur le graphique et l'entrée de la liste, et atténue le reste à 45 %.

## Do's and Don'ts

### Do:
- **Do** réserver #FF9900 à lui et #3262FF à moi, dans chaque vue.
- **Do** afficher une valeur absente comme telle (« — », texte discret #9AA0A9), jamais une estimation.
- **Do** donner un jumeau en face à chaque donnée et les relier par survol et clic (classe `hl`, conteneur `dimming`).
- **Do** utiliser les chiffres tabulaires et les prix en ¢.
- **Do** teinter les pastilles à 14–20 % de leur couleur, texte dans la variante claire.
- **Do** couper toute animation sous `prefers-reduced-motion: reduce`.

### Don't:
- **Don't** construire deux moitiés miroir lui / moi ni un mur de cartes de chiffres.
- **Don't** utiliser l'orange ou le bleu pour une catégorie, un état ou une décoration.
- **Don't** ajouter d'ombre portée ni de dégradé décoratif (seul l'avatar du wallet porte un dégradé).
- **Don't** utiliser le rouge pour autre chose qu'un coût, une perte, Non ou une alerte.
- **Don't** ajouter de bouton qui agit sur le bot : l'UI est en lecture seule.
