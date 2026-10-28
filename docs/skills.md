# Skills et plugins

Inventaire des skills du projet (`.claude/skills/`) et des plugins Claude Code activés dans `.claude/settings.json`, avec l'origine, le rôle et la justification de chacun. Le dossier `.claude/` du dépôt cible est une copie identique de celui du dépôt source `btc5m-lag-sim` (`skills/` et `settings.json`), sans `scheduled_tasks.lock` (`PROMPT-MIGRATION.md:37`, dépôt cible).

Convention : chemins relatifs au dépôt source `btc5m-lag-sim` (lecture seule), cités `fichier:ligne`, sauf mention « dépôt cible ». `[à vérifier]` marque ce qui n'est pas établi. Voir aussi [journal-decisions.md](journal-decisions.md) (décision D7) et [interface-produit.md](interface-produit.md) (Impeccable).

## 1. Tableau principal

| Skill / plugin | Origine | Rôle | Justification (1 ligne) |
| --- | --- | --- | --- |
| `prompt-master` (skill du projet, v1.8.2) | Skill tiers, dépôt `https://github.com/nidhinjs/prompt-master.git` (lu dans `.claude/skills/prompt-master/.git/config` du dépôt source) ; copié sans son `.git` (D7) | Génère des prompts optimisés pour des outils d'IA, uniquement sur demande explicite (`.claude/skills/prompt-master/SKILL.md:3-4`) | Hors mission de trading ; conservé parce qu'il n'est pas désactivable par réglage (il faudrait supprimer le dossier) et que la copie doit rester identique (`engine/00-recherche.md:93-94`) |
| `read-session` (skill du projet) | Écrit pour `btc5m-lag-sim` (`.claude/skills/read-session/SKILL.md:2-3`) ; copié identique (D7) | Analyse une session enregistrée `data/sessions/<id>.jsonl.gz` pour répondre à un objectif de trading libre (stop loss, réglage de `minEdge`, comparaison de sources ou de comptes) ; pose d'abord ≥ 10 questions de clarification puis produit un rapport chiffré (`.claude/skills/read-session/SKILL.md:3, :10`) | **Inerte dans `updown5m-lab`** : son outil `node scripts/sessions/read-session.mjs` (déclaré dans `allowed-tools`, `.claude/skills/read-session/SKILL.md:5`) n'est pas migré, et les sessions `data/sessions/*.jsonl.gz` sont converties en traces `.old` ; conservé tel quel pour garder `.claude/` identique (D7), à réécrire si besoin |
| `claude-security@claude-plugins-official` | Marketplace officielle `claude-plugins-official` | Scan de sécurité du code ou d'un diff, patchs vérifiés | Revue de sécurité prévue avant l'argent réel (clé, garde réseau) (`engine/00-recherche.md:31` ; `engine/02-conception.md:316`) ; usage précis non journalisé `[à vérifier]` |
| `differential-review@trailofbits` | Marketplace `trailofbits` (dépôt `trailofbits/skills`, `engine/00-recherche.md:59`) | Revue de sécurité différentielle d'un diff, calcul du rayon d'impact | Utilisé pour la revue adversariale de la phase 6 (8 constats corrigés) (`engine/JOURNAL.md:86` ; `engine/reports/06-revue-adversariale.md:3`) |
| `insecure-defaults@trailofbits` | Marketplace `trailofbits` | Audit des défauts dangereux (secrets de repli, identifiants par défaut, options « fail-open ») | Pertinent pour une config de service exposée par tunnel (`engine/00-recherche.md:31`) ; usage non journalisé `[à vérifier]` |
| `property-based-testing@trailofbits` | Marketplace `trailofbits` | Tests par propriétés (proptest, fast-check…) | Utilisé en phases 3, 4, 6, 7 (registre, carnet, montants, machine d'états) (`engine/JOURNAL.md:85-86, :88-89`) ; non utilisé en phase 8 car la parité exacte sur 132 817 enregistrements couvre l'arithmétique portée (`engine/JOURNAL.md:84`) |
| `security-guidance@claude-plugins-official` | Marketplace officielle `claude-plugins-official` | Garde-fous de sécurité pendant l'écriture du code | Activé d'emblée, utile aux phases d'écriture (`engine/00-recherche.md:36`) |
| `sharp-edges@trailofbits` | Marketplace `trailofbits` | Repère les API et configurations piégeuses (footguns) | Prévu pour les contrats et les commandes (phases 2 et 6) (`engine/00-recherche.md:30` ; `engine/02-conception.md:316`) ; usage non journalisé `[à vérifier]` |
| `supply-chain-risk-auditor@trailofbits` | Marketplace `trailofbits` | Audit des dépendances (npm, PyPI, Go) | **Ne couvre pas crates.io** : les dépendances Rust se vérifient à la main (licences, mainteneurs) et par `cargo-deny` (`engine/02-conception.md:221-222`) ; ne s'appliquerait qu'aux paquets npm du dépôt cible (convert, ui) `[à vérifier]` |
| `dimensional-analysis@trailofbits` | Marketplace `trailofbits`, installé en portée projet le 2026-10-18 avec l'accord de l'utilisateur (`engine/00-recherche.md:84-85`) | Traque les erreurs d'unités et d'échelle décimale (pUSD, parts, prix au tick, frais) | Couvre le risque « argent » ; utilisé à la main en phase 7 (formules cohérentes, trois points de précision) car le pipeline annote les fichiers et écrit à la racine du dépôt (`engine/00-recherche.md:59` ; `engine/JOURNAL.md:57` ; `engine/07-comptabilite.md:110-115`) |
| `spec-to-code-compliance@trailofbits` | Marketplace `trailofbits`, installé en portée projet le 2026-10-18 avec l'accord de l'utilisateur (`engine/00-recherche.md:84-85`) | Confronte le code au document de conception, une exigence à la fois | Prévu contre `02-conception.md` (phases 6, 7, avant 10) (`engine/02-conception.md:316`) ; limité par l'absence de l'outil `Workflow`, à piloter agent par agent (`engine/00-recherche.md:42-44`) ; usage effectif non journalisé `[à vérifier]` |
| `impeccable@impeccable` | Marketplace `impeccable` ; **désactivé** dans `.claude/settings.json`, à activer **uniquement** dans `.claude/settings.local.json` (gitignoré) du dépôt cible : `"impeccable@impeccable": true` | Skill `impeccable:impeccable` et ses sous-agents : conception de l'interface, établit `PRODUCT.md` et `DESIGN.md` (`PROMPT-MIGRATION.md:88-90`, dépôt cible) | Seul moyen autorisé pour concevoir l'UI (jamais de maquette écrite à la main) ; s'il ne se charge pas, la migration s'arrête et demande à l'utilisateur (`PROMPT-MIGRATION.md:89`, dépôt cible) ; avait servi à l'ancienne interface (`.impeccable/surfaces/ui-src-app-svelte.md`) |

Le fichier `.claude/settings.json` actif contient exactement : `claude-security`, `differential-review`, `insecure-defaults`, `property-based-testing`, `security-guidance`, `sharp-edges`, `supply-chain-risk-auditor`, `dimensional-analysis`, `spec-to-code-compliance` à `true` (dépôt cible, `.claude/settings.json`). L'ancien `.claude/settings.local.json` du dépôt source ne contenait que 5 règles de permission Bash et aucun plugin ; il n'est pas migré (plan de documentation de la migration).

## 2. Plugins présents mais désactivés dans `.claude/settings.json`

| Plugin | Raison documentée | Source |
| --- | --- | --- |
| `agentic-actions-auditor@trailofbits`, `chrome-devtools-mcp@claude-plugins-official`, `frontend-design@claude-plugins-official`, `skill-creator@claude-plugins-official` | Désactivés le 2026-10-18 parce qu'ils ne servent pas la mission ; réactivables par `claude plugin enable <plugin> --scope project` | `engine/00-recherche.md:89-92` |
| `impeccable@impeccable` | Idem, mais réactivé pour l'UI via `settings.local.json` (ligne du tableau principal) | `engine/00-recherche.md:89-92` ; `PROMPT-MIGRATION.md:89` (dépôt cible) |
| `static-analysis@trailofbits` | Plus tard (CodeQL et Semgrep non installés) | `engine/00-recherche.md:50` |
| `context7@claude-plugins-official` | Non pour l'instant : les faits Polymarket doivent venir de la documentation officielle | `engine/00-recherche.md:51` |
| `quantitative-trading@claude-code-workflows` | Non : agent générique ; les stratégies se rejouent sur nos sessions | `engine/00-recherche.md:52` |
| `modern-python@trailofbits` | Seulement si la stack retenue est Python (elle est en Rust) | `engine/00-recherche.md:53` ; `engine/02-conception.md:170` |
| `n8n-mcp-skills@n8n-mcp-skills`, `pg@aiguide` | Aucune raison trouvée dans la documentation lue `[à vérifier]` | `.claude/settings.json` (dépôt cible) |

## 3. Hors tableau : skills utilisateur et outils mentionnés

- `graphify` : skill utilisateur (`~/.claude/skills/graphify`), imposé par `CLAUDE.md` (requêtes `graphify query`, mise à jour `graphify update .` après modification de code) ; utilisé en phases 1 à 8 (`engine/JOURNAL.md:81, :84, :87-88` ; `engine/00-recherche.md:27`).
- `anthropic-skills:deep-research` : a coordonné 7 agents de recherche en phase 0 (`engine/JOURNAL.md:91`).
- `developing-with-streamlit` : skill utilisateur sans usage pour la mission, non désactivable par réglage de projet (`engine/00-recherche.md:38-40, :93-94`).
- Aucun skill de mesure de latence ni de HFT n'existe : les mesures sont faites par des scripts maison versionnés (`engine/bench/`) (`engine/00-recherche.md:77-78` ; `engine/JOURNAL.md:82`).
- Skills Polymarket communautaires : écartés (second ordre, la règle impose la documentation officielle) (`engine/00-recherche.md:70-73`).

## 4. Points `[à vérifier]`

- Origine précise (marketplace et dépôt) de `impeccable@impeccable` : seul le nom `impeccable@impeccable` est connu dans les sources lues.
- Usage effectif de `claude-security`, `insecure-defaults`, `sharp-edges`, `spec-to-code-compliance` : prévus, mais non journalisés dans `engine/JOURNAL.md:76-91`.
- `.claude/settings.local.json` gitignoré : le fichier `.gitignore` du dépôt cible n'existait pas à la date de cette rédaction.
