# Design engine live/test — rapport de conception (phase 1, agent opus, 2026-10-27), synthèse

**Historique.** Conception de la migration (2026-10-27). L'état actuel : [architecture-backtest.md](../architecture-backtest.md).

Chemins relatifs au dépôt source `btc5m-lag-sim`.

## Architecture constatée
- Temps lu : `core/src/clock.rs:13-22` (`Clock::now()` → `Stamp{mono_ns, wall_ns}`) ; `tokio::time` dans `core/src/feed.rs:57-70,99,112` ; fil froid `app/src/main.rs:295,336` ; fil chaud échéance `main.rs:402`, intervalle 50 ms `main.rs:406`, `clock.now()` `main.rs:416,466,475,588,614,625,645` ; `paper.rs` temps stratégie `t = recv.wall_ns/1e6` (l.223) ; `live.rs:74,97,107,108` ; `exec.rs:134,145,172,178`.
- Déjà purs (temps passé en paramètre) : `strategy::reference` (`schedule`/`advance`, `reference.rs:335,343`), `Books::delta`/`quiet` (`book.rs:113,131`), `Supervisor`, `guard::Context.now_ns`, `Order::apply` (`order.rs:59`), `Account::apply`.
- Flux : trait `feed::Adapter` (`feed.rs:16-29`), `feed::run` (`feed.rs:49`) → `Out::{Event,Health,DecodeError}` ; registre = `match` dans `main.rs:69-76` ; fenêtres par fil froid HTTP (`windows.rs:86`, `main.rs:288-339`) ; résultats `Cold::Resolved` (`main.rs:474`) ; fills propres par canal `user` (`main.rs:364-371`, 517-520) ; boucle chaude `tokio::select!` sur 7 sources (`main.rs:407-679`).
- Ordres : `Rec::Order` (`reference.rs:653,722`) → `Paper::decide` (`paper.rs:153-169`) → appariement simulé `due()` (`paper.rs:172-194`, aller 12,2 ms + retenue 150 ms) ; réel : garde-fous, solde, signature, `live::ToSend`, `live.rs:87-119`, `Clob::post` (`exec.rs:152-193`). Seuls des achats FAK (`sign.rs:117`, `main.rs:648`). Ni post-only ni annulation.
- `replay` : session JSONL → `reference::Event` (`replay/main.rs:69-92`), modes `decisions` (parité 0 écart, `JOURNAL.md:116`), `fair`, `maker` (C4, remplacé par maker-bt).
- `maker-bt` (Node) : `simWindow` (`sim.mjs:164-462`) mêle stratégie (`decide` 311-400), exchange (`post`/`liveAt` 222-234, `onBook` 251-290, `exactFill` 148-156, sorties FAK par lecture anticipée `bookAt` 402-406) et juste valeur (193-217). Les stratégies maker n'existent pas dans l'engine Rust.

## Options envisagées
1. Traits statiques génériques `Engine<C: Clock, F: Feed, V: Venue>` async. + coût nul. − réécrire tout le `select!` ; temps virtuel tokio = feature `test-util` ; ordonnancement non déterministe → parité bit à bit menacée ; gros refactor.
2. Traits `dyn` + enum de mode. + choix à l'exécution. − async non `dyn`-compatible sans crate `async-trait` (nouvelle dépendance) ou `Pin<Box<Future>>` manuels ; même non-déterminisme.
3. RETENUE : cœur synchrone sans E/S piloté par les événements (`Engine::on(input, at, out)`, ne lit aucune horloge), deux coquilles : live (boucle tokio actuelle → tâche d'envoi existante) et test (boucle déterministe, tas `(instant, n° séquence)`, même mécanisme que `Reference::schedule`).
   Raisons : briques déjà pures ; E/S live déjà par canaux ; déterminisme → parité exacte (précédent 0 écart) ; aucune dépendance, pas d'async dans le cœur ; conforme au contrat « Polymarket réel / simulé / faux CLOB » (`02-conception.md:96-106`) ; le papier devient Engine + exchange simulé sur flux live (un seul exchange simulé). Coût : extraire de `main.rs` la décision (578-608, 641-678), refactor moyen limité à `app` ; un `Vec` par événement, négligeable face à p99 ≤ 250 µs.
   Règle de parité maker : sur un même événement de carnet, fills de l'exchange simulé AVANT décision de la stratégie (`sim.mjs:446-448`).

## Écarts / limites notés
- Le simulateur JS applique le produit des sorties TP/SL dès la décision en lisant le carnet en avance (`sim.mjs:330-343`) ; un exchange réaliste livre le fill à l'arrivée → non-régression bit à bit limitée aux configs sans TP/SL (décision D6).
- Points [à vérifier] : symétrie de l'aller simple ; délai de réponse d'un ordre accepté ; ordre de sortie des ordres retenus ; retenue des annulations maker ; sémantique FOK ; post-only vérifié à l'arrivée vs décision ; file mesurée à la décision vs arrivée ; impact de nos ordres sur le carnet ; annulations prorata vs « toutes derrière » ; `last_trade_price` vs `labelTrades` ; taille min côté serveur ; arrondi des frais (5 décimales vs entier exact) ; durabilité des rebates ; délai de réclamation ; ordres après la fin de fenêtre ; slug d'un autre actif ; taux de trades `FAILED` ; `bench/parity.sh` cité par `JOURNAL.md:116` absent.

## Référence de non-régression mesurée le 2026-10-27 (validate.mjs --delay 0 --sim exact, 21 min 42 s)
59 695 ordres, réel 1 089 572 parts / PnL 17 331 $ ; exact prédit 1 137 900 parts, ratio 1,044, ordres exacts 0,841, PnL 16 581 $ ; par jour identique à `WALLETS.md` §0 (l.62). Étape intermédiaire : 71 049 ordres, 66 273 arrivées trouvées, ratio 1,102 ; 63 152 de fin connue, ratio 1,042 ; exacts 0,812.
