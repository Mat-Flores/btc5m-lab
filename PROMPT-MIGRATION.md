# Prompt de migration — btc5m-lag-sim → updown5m-lab

Ce fichier contient la spécification complète que l'agent orchestrateur doit exécuter. Il n'est pas modifié pendant l'exécution.

<role>
Tu es l'ORCHESTRATEUR (Opus). Tu es un ingénieur senior en trading quantitatif et en infrastructure (Rust, Node, Linux).
- Tu conçois, découpes, délègues, valides et intègres.
- Tout ce qui peut s'exécuter en parallèle est délégué à des subagents lancés dans le même message.
- Tu ne fais toi-même que la synthèse, les décisions d'architecture, la validation et les commits.
</role>

<context>
Dépôt source (lecture seule) : `C:\dev\REA Projects\btc5m-lag-sim`. Il sert à trader les marchés Polymarket « Up or Down » 5 min sur BTC. Son contexte est trop gros et mélangé :
- engine Rust `engine/` (core, strategy, adapters okx/polymarket/chainlink, app, replay, maker-bt, redeem) ;
- simulateur Node `src/` ;
- ancienne UI `ui/` + `src/server/` ;
- déploiement `deploy/` (setup-ubuntu.sh, push-node.sh, collect.sh, systemd) et `.github/workflows/` ;
- recherche éparpillée (`engine/00-*.md` à `11-*.md`, `JOURNAL.md`, `research_notes/`, `engine/reports/`, `docs/`, `reports/`, `SPEC.md`, `PRODUCT.md`, `TODO.md`, `DESIGN.md`) ;
- `.claude/` et `CLAUDE.md`.

Nœuds AWS (accès décrits dans `deploy/README.md` du dépôt source, clés dans `~/.ssh/`) :
- `dublin-dry-run` : enregistre aujourd'hui des sessions de l'ancienne app. Il devient le serveur de tracing.
- `dublin-live` : à nettoyer, puis plus rien n'y tourne. NEVER y déployer quoi que ce soit.

Dépôt cible : `C:\dev\REA Projects\updown5m-lab`. Il existe déjà : `git init` est fait (branche `main`, aucun commit), et il ne contient que ce fichier `PROMPT-MIGRATION.md`.

Objectif : construire ce dépôt, recentré sur deux choses :
1. l'enregistrement continu de flux ;
2. un backtest ultra réaliste des marchés 5 min,
afin de trouver une stratégie performante en ajustant les paramètres et les sources, seules ou croisées. Ensuite, déployer le tracing sur dublin-dry-run et le laisser tourner en continu.
</context>

<target_structure>
updown5m-lab/
├── README.md         → objectif, rôle de chaque outil, enchaînement source-tracing → backtest → engine
├── CLAUDE.md         → copie IDENTIQUE à l'octet du CLAUDE.md source
├── .claude/          → copie IDENTIQUE de `skills/` et `settings.json` (sans `scheduled_tasks.lock`)
├── engine/           → engine : sources observées → stratégies → décisions d'ordre (mode live + mode test)
├── source-tracing/   → enregistreur continu
│   └── trace/<source>/<AAAA>/<MM>/<AAAA-MM-JJ>[.old].<ext>   (1 fichier par jour UTC ; trace/ gitignoré)
├── backtest/         → backtest qui pilote l'engine en mode test
├── ui/               → UI simplifiée (contrôle et état du tracing)
├── deploy/           → déploiement simplifié vers dublin-dry-run + rapatriement des traces
└── docs/             → synthèse de toute la recherche, 1 fichier par sujet
Chaque dossier de premier niveau a son README.md. Tous les fichiers .md sont en français.
</target_structure>

<requirements>
GÉNÉRICITÉ (par structure, sans fonctionnalité spéculative)
- Ajouter une source d'observation = ajouter 1 adapter, sans toucher au cœur.
- Une stratégie peut croiser N sources pour décider d'un ordre.
- L'actif et l'horizon du marché sont des paramètres de config. BTC 5 min est le seul marché configuré et testé. ETH doit être possible par la config, mais n'est PAS implémenté.
- Stratégies, paramètres et sources se déclarent dans des fichiers de config de backtest, qu'on peut balayer (sweep) sans recompiler.

ENGINE + BACKTEST — mêmes conditions que la réalité
- backtest/ exécute le MÊME code de l'engine (stratégies, décision, gestion d'ordres) en mode test. Seuls l'horloge, l'entrée des flux (traces et flux publics rejoués) et le marché (exchange simulé) sont substitués.
- La séparation se conçoit selon les bonnes pratiques Rust (ex. interfaces Clock / Feed / Venue, avec une implémentation live et une simulée), avec le refactor minimal. Documente ce choix dans `docs/architecture-backtest.md` : options envisagées, choix retenu, raisons.
- L'exchange simulé reproduit les comportements réels documentés :
  - délais calibrés sur les mesures du projet (`engine/10-delai-appariement.md`, etc.) ;
  - un ordre n'est rempli que s'il y a de la liquidité en face dans le carnet à l'instant où il arrive. Sinon il est rejeté, partiellement rempli ou mis en attente, selon son type et les règles réelles de Polymarket ;
  - file d'attente maker ;
  - tick et taille minimale ;
  - fees et rebates ;
  - fin de fenêtre et résolution Chainlink.
- Aucun comportement inventé. Si une règle n'est pas établie, marque-la `[à vérifier]` dans docs et rends-la paramétrable.
- Non-régression : le simulateur maker déjà validé sur ordres réels (borne exacte, delay 0, rebates) MUST donner les mêmes résultats après la migration.

SOURCE-TRACING
- Autonome, déployable seul. Reconnexion automatique, redémarrage propre, rotation à minuit UTC.
- À chaque rotation, le fichier du jour est fermé et marqué « validé » : sha256 + manifest (nombre de lignes, trous, heures couvertes).
- Réutilise les adapters et l'enregistreur existants.
- Chaque trace contient :
  - payload brut ;
  - horodatage local de réception à haute résolution, et horodatage de la source ;
  - numéros de séquence ;
  - snapshots et deltas de carnet, trades ;
  - événements de connexion, de déconnexion et de trous ;
  - version du schéma et identifiant de l'hôte.
- Le format est documenté dans `source-tracing/README.md`.
- Activation et désactivation par source, via la config ou l'UI.

ANCIENNES TRACES
- Récupère toutes les données déjà enregistrées : dublin-dry-run, dublin-live s'il y en a, et le `data/` local du dépôt source (en copie).
- Convertis-les dans `source-tracing/trace/<source>/<AAAA>/<MM>/`, découpées par source et par jour UTC. Suffixe `.old` si le format diffère du nouveau.
- N'invente jamais un champ manquant. Documente le mapping ancien → nouveau.

UI (ui/)
- Design par le skill `impeccable:impeccable` et ses subagents. NEVER de maquette écrite à la main.
- Le plugin est désactivé dans settings.json : active-le uniquement dans `.claude/settings.local.json` du nouveau dépôt (`"impeccable@impeccable": true`, gitignoré). S'il ne se charge pas, arrête-toi et demande-moi.
- Impeccable établit `PRODUCT.md` (nouvel objectif) et `DESIGN.md` (repris du dépôt source).
- Périmètre :
  - la liste des sources, avec un interrupteur on/off pour le tracing ;
  - l'état de chaque source : connectée, âge du dernier message, messages/s, trous du jour, taille du fichier du jour ;
  - les jours validés présents sur le serveur.
  Rien d'autre.
- L'UI écoute sur 127.0.0.1 uniquement, et on y accède par tunnel SSH. NEVER d'exposition publique.

DEPLOY (deploy/)
- 1 script de setup, 1 script de déploiement, 1 unit systemd (`Restart=always`) pour source-tracing et l'UI. Pas de workflow GitHub.
- `deploy/pull-traces`, exécutable depuis Windows (Git Bash) :
  - il ne prend que les jours validés (jamais le jour UTC en cours) ;
  - il les télécharge dans `source-tracing/trace/` en respectant l'arborescence ;
  - il vérifie le sha256 en local ;
  - il ne supprime le fichier sur le serveur qu'après une vérification réussie ;
  - il est idempotent, reprend proprement après une interruption et propose une option `--dry-run`.

DOCS (docs/)
- Synthétise TOUTE la recherche : 1 fichier par sujet, chaque affirmation cite son fichier d'origine. Garde chiffres, conclusions et impasses ; supprime le tâtonnement.
- OBLIGATOIRES :
  - `contraintes-marche.md` : vérités acquises sur les fees et rebates, tick, taille minimale, résolution Chainlink, délais d'appariement, fenêtres de 5 min, limites des flux (ex. OKX enregistré creux, donc LT-REF invalidé), calibrages validés (simulateur maker, spot Binance à la ms avec σ×1,3), résultats à revoir (b01 à b10) ;
  - `architecture-backtest.md` ;
  - `skills.md` : un tableau « Skill / plugin | Origine | Rôle | Justification (1 ligne) » pour chaque skill de `.claude/skills/` et chaque plugin activé ;
  - `README.md` : l'index des docs.
</requirements>

<orchestration>
Règles de délégation
- Lance dans UN SEUL message toutes les tâches d'une même vague, au maximum 6 subagents simultanés.
- Une vague ne démarre qu'après la validation de la vague dont elle dépend.
- Chaque subagent reçoit :
  - sa tâche ;
  - les dossiers qu'il a le droit de toucher (1 seul dossier cible, aucun chevauchement) ;
  - les contrats figés dont il dépend (format de trace, interfaces engine, API d'état de l'UI) ;
  - les contraintes et stop_conditions de ce prompt ;
  - le format de retour.
- Format de retour obligatoire :
  - fichiers créés ou modifiés ;
  - commandes lancées, avec leur sortie réelle (extrait) ;
  - écarts par rapport à la tâche ;
  - points `[à vérifier]`.
- Les subagents ne commitent JAMAIS et ne touchent jamais à un serveur sans y être explicitement autorisés par leur tâche.

Choix du modèle et de l'effort
- opus / high : conception d'architecture, refactor de l'engine, exchange simulé, format de trace.
- sonnet / high : implémentation non triviale, synthèse documentaire, conversion de données.
- sonnet / medium : scripts, deploy, README, intégration UI.
- haiku / low : inventaires, commandes SSH en lecture seule, copies, vérifications mécaniques (diff, hash, status).

Validation par l'orchestrateur, avant d'accepter un livrable
- Relance toi-même la commande de vérification clé (build, test, diff, hash). Ne valide jamais sur la seule parole d'un subagent.
- Vérifie que le livrable respecte les contrats figés et le périmètre de dossier.
- Si un livrable est refusé : 1 relance du même subagent avec une correction précise. Au 2e échec, arrête-toi et demande-moi.
- Après validation, commit par l'orchestrateur, 1 commit par outil.
</orchestration>

<process>
Phase 1 — Inventaire et conception. 1 vague parallèle, aucune écriture, SSH en lecture seule.
- [haiku/low] inventaire de dublin-dry-run : services, crons, données, tailles.
- [haiku/low] inventaire de dublin-live : mêmes éléments.
- [sonnet/medium] tableau « source → destination → action » du dépôt source, et endroits où BTC ou 5 min sont codés en dur.
- [opus/high] design mode live / mode test de l'engine (Clock, Feed, Venue) et modèle de l'exchange simulé.
- [opus/high] format de trace + mapping ancien format → nouveau format, en lisant les sessions existantes.
- [sonnet/high] plan de docs/ + extraction exhaustive des contraintes de marché, avec leurs sources.
L'orchestrateur synthétise en un plan unique et fige 3 contrats : format de trace, interfaces engine, API d'état tracing ↔ UI.
STOP : attends ma validation.

Phase 2 — Construction locale.
Vague 2a, en parallèle :
- [opus/high] engine/ : migration + séparation live/test ;
- [sonnet/high] source-tracing/ ;
- [sonnet/high] convertisseur des anciennes traces ;
- [sonnet/high] docs/, découpé en 2 ou 3 agents par groupe de sujets sans chevauchement ;
- [sonnet/medium] deploy/ (scripts + pull-traces) ;
- [haiku/low] copie de `.claude/` et `CLAUDE.md`, suivie d'un diff.
Vague 2b, après la validation de engine/ et de source-tracing/, en parallèle :
- [opus/high] backtest/ + exchange simulé + test de non-régression maker ;
- [sonnet/high] ui/ via impeccable ;
- [sonnet/medium] les 8 README.
Après chaque validation, affiche : ✅ <outil> — commande de vérification relancée par l'orchestrateur et son résultat réel.

Phase 3 — Serveurs, dans cet ordre strict.
a) En parallèle : [haiku/low] rapatriement de dublin-dry-run, [haiku/low] rapatriement de dublin-live, [haiku/low] copie du `data/` local. Ensuite, [sonnet/medium] conversion dans trace/ (`.old` si besoin). L'orchestrateur vérifie les sha256.
b) STOP : montre-moi le bilan (fichiers, tailles, hashes OK) et ce qui sera arrêté et supprimé sur chaque nœud. Attends mon accord.
c) En parallèle : [sonnet/medium] nettoyage de dublin-dry-run, [sonnet/medium] nettoyage de dublin-live. Chacun arrête et désactive tous les services et crons de l'ancienne app, puis supprime ses fichiers. NEVER éteindre ou terminer une instance EC2.
d) [sonnet/medium] déploiement de source-tracing + UI sur dublin-dry-run uniquement, puis lancement du tracing.

Phase 4 — Vérification. Vague parallèle [haiku/low], chaque résultat étant revérifié par l'orchestrateur :
- `cargo build && cargo test` et les tests de backtest/ ;
- non-régression du simulateur maker ;
- 1 backtest BTC de référence sur une fixture, avec ses métriques ;
- sur dublin-dry-run : `systemctl status` actif, fichier de trace du jour qui grossit (taille mesurée 2 fois à 60 s d'écart), UI joignable par tunnel ;
- `deploy/pull-traces --dry-run` liste les jours validés ;
- dublin-live : aucun service applicatif actif ;
- `diff -r .claude/skills` et `diff CLAUDE.md` avec le dépôt source : les deux doivent être vides ;
- `git status` du dépôt source : il doit être inchangé.
Ensuite, l'orchestrateur lance `graphify .` à la racine du nouveau dépôt et vérifie que `graphify-out/graph.json` existe.
</process>

<constraints>
- NEVER modifier le dépôt source : uniquement lire et copier.
- NEVER copier ou commiter `.env`, des clés, des wallets ou des secrets. Fournis des `.env.example`.
- NEVER copier `node_modules/`, `target/`, `graphify-out/`, l'ancienne `ui/`, `src/server/`.
- Aucune dépendance ni abstraction au-delà de ce qu'exigent ces requirements.
- Historique Git vierge, 1 commit par outil. Pas de remote, pas de push.
- `source-tracing/trace/` est gitignoré, sauf une petite fixture de test.
</constraints>

<stop_conditions>
Arrête-toi et demande-moi avant de :
- ajouter une dépendance ;
- modifier la logique d'une stratégie ;
- supprimer quoi que ce soit, en local ou sur un serveur (hors suppression par pull-traces après vérification du hash) ;
- toute action sur dublin-live autre que la lecture et le nettoyage validé en 3b ;
- continuer après le 2e échec d'un même livrable ou d'une même commande. Dans ce cas, montre l'erreur exacte.
</stop_conditions>

<done>
Le travail est terminé quand :
- les 7 dossiers et les 8 README existent ;
- chaque outil se build et se teste seul ;
- le backtest exécute le code de l'engine en mode test ;
- la non-régression du simulateur maker passe ;
- les anciennes traces sont en local dans trace/ (`.old` si besoin), avec leur hash vérifié ;
- les 2 nœuds sont nettoyés ;
- le tracing tourne en continu sur dublin-dry-run ;
- pull-traces fonctionne ;
- docs/ est complet ;
- CLAUDE.md et les skills sont identiques à l'octet ;
- graphify est généré ;
- le dépôt source est intact.

Rapport final :
- arborescence de niveau 2 ;
- tableau des vérifications avec leur résultat réel ;
- tableau des subagents lancés (tâche, modèle, effort, validé du 1er coup ou relancé) ;
- état de chaque nœud ;
- ce qui n'a pas été migré et pourquoi.
</done>
