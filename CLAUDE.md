# CLAUDE.md

## Langue

**Écris toujours en français :** toutes tes réponses, et tous les fichiers Markdown (`.md`) que tu crées ou modifies (rapports, README, docs, notes).

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).

## Continuité de session (HANDOFF par branche)

Objectif : pouvoir couper une session à tout moment (limite d'usage, fin de journée) et reprendre dans une session neuve sans recharger l'historique.

### Nom du fichier
- Le fichier de passation s'appelle `HANDOFF-<branche>.md`, à la racine du projet.
- `<branche>` = résultat de `git branch --show-current`, avec les `/` remplacés par `-` (ex. `feature/auth` → `HANDOFF-feature-auth.md`).
- Si la branche est `main` ou `master`, utilise `HANDOFF-main.md`.
- Si le dépôt n'est pas un dépôt git ou si la branche est indéterminée (HEAD détachée), utilise `HANDOFF.md`.

### Au début d'une session
- Détermine la branche courante, puis lis `HANDOFF-<branche>.md` s'il existe, et reprends à « Prochaine action ». Ignore les `HANDOFF-*.md` des autres branches.
- Ne relis pas tout le code : ouvre seulement les fichiers listés dans « Fichiers clés ».

### Pendant la session
- Mets à jour `HANDOFF-<branche>.md` après chaque étape significative (fonctionnalité terminée, décision importante, bug résolu, changement de direction), sans attendre que je le demande.
- Réécris le fichier au lieu d'y ajouter à la suite : il doit rester sous ~60 lignes et refléter l'état actuel, pas l'historique.
- Quand je dis « handoff » ou « sauvegarde le contexte », mets à jour le fichier de la branche courante immédiatement, puis confirme en une ligne.
- Si je change de branche en cours de session, bascule sur le `HANDOFF-<nouvelle-branche>.md` correspondant.

### Format du fichier
- **Branche** : nom de la branche et branche de base (ex. `feature/auth`, depuis `main`).
- **Objectif** : la tâche en 1-2 phrases.
- **Fait** : ce qui est terminé et vérifié.
- **Reste à faire** : liste courte, ordonnée.
- **Décisions et pièges** : choix techniques et leur raison, erreurs à ne pas refaire, approches écartées.
- **Fichiers clés** : chemins, avec une ligne sur le rôle de chacun.
- **Commandes utiles** : build, tests, lancement, si spécifiques à la tâche.
- **Prochaine action** : l'étape suivante, assez précise pour être exécutée sans contexte.

### Règles
- Ne mets dans le fichier que ce qui n'est pas déjà dans le code ou dans `git log`.
- Pas de morceaux de code longs : référence le fichier et la ligne.
- Les règles permanentes du projet vont dans ce `CLAUDE.md`, pas dans les fichiers HANDOFF.
- Quand une branche est mergée, supprime son `HANDOFF-<branche>.md`.

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.


