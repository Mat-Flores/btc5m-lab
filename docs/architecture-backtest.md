# Architecture du backtest : engine en deux modes (live / test)

**Statut au 2026-10-28 : implémenté.** `engine/app/src/engine.rs` (cœur synchrone), `engine/strategy/src/maker.rs` (stratégie maker portée), `backtest/` (deux entrées : packs de l'ancien dépôt, traces de `source-tracing`). Ce document garde les options, le choix et ses raisons ; l'usage et les paramètres sont dans [`backtest/README.md`](../backtest/README.md).

Conventions. Les chemins cités sont relatifs au dépôt source `btc5m-lag-sim` (lecture seule), sauf mention « dépôt cible ». « Contrat 2 » désigne l'interface figée de l'engine validée par l'utilisateur le 2026-10-27 ([contrats-migration.md:79](migration/contrats-migration.md)). `[à vérifier]` marque tout ce qui n'est pas établi : la règle est alors un **paramètre** de la config, jamais une valeur codée en dur.

Voir aussi : [`engine-conception.md`](engine-conception.md), [`engine-mesures-phases.md`](recherche/engine-mesures-phases.md), [`simulateur-maker.md`](simulateur-maker.md), [`strategies-maker.md`](recherche/strategies-maker.md), [`contraintes-marche.md`](recherche/contraintes-marche.md), [`ordres-et-appariement.md`](recherche/ordres-et-appariement.md), [`journal-decisions.md`](journal-decisions.md).

---

## 1. Objectif et contraintes

Exigences de la spécification (`PROMPT-MIGRATION.md` du dépôt cible, section « ENGINE + BACKTEST », l. 55-65) :

- `backtest/` exécute **le même code** de l'engine (stratégies, décision, gestion d'ordres) en mode test. Seuls l'horloge, l'entrée des flux (traces et flux publics rejoués) et le marché (exchange simulé) sont substitués.
- Séparation selon les bonnes pratiques Rust (interfaces Clock / Feed / Venue, implémentation live et simulée), **refactor minimal**.
- L'exchange simulé reproduit les comportements réels **documentés** : délais calibrés sur les mesures du projet, ordre rempli seulement s'il y a de la liquidité en face à l'instant d'arrivée, file d'attente maker, tick et taille minimale, frais et rebates, fin de fenêtre et résolution Chainlink.
- Aucun comportement inventé : règle non établie = `[à vérifier]` + paramètre.
- Non-régression : le simulateur maker déjà validé sur ordres réels (borne exacte, retenue 0, rebates) doit donner les mêmes résultats après la migration.
- Interdits (contrat) : modifier la logique d'une stratégie ; ajouter une dépendance sans arrêt et demande.

## 2. Architecture d'origine (constat)

Source : [conception-engine-migration.md:5-11](migration/conception-engine-migration.md), numéros de ligne relatifs au dépôt source `engine/`.

- **Temps lu** directement : `core/src/clock.rs:13-22` (`Clock::now()` renvoie `Stamp{mono_ns, wall_ns}`) ; `tokio::time` dans `core/src/feed.rs:57-70,99,112` ; fils de `app/src/main.rs` (froid `:295,336`, chaud : échéance `:402`, intervalle de 50 ms `:406`, `clock.now()` en `:416,466,475,588,614,625,645`) ; `app/src/paper.rs:223` (`t = recv.wall_ns/1e6`) ; `app/src/live.rs:74,97,107,108` ; `adapters/polymarket/src/exec.rs:134,145,172,178`.
- **Déjà purs** (temps passé en paramètre) : la stratégie de référence (`schedule`/`advance`, `strategy/src/reference.rs:335,343`), `Books::delta` et `quiet` (`core/src/book.rs:113,131`), `Supervisor`, `guard::Context.now_ns`, `Order::apply` (`core/src/order.rs:59`), `Account::apply`.
- **Flux** : trait `feed::Adapter` (`core/src/feed.rs:16-29`), `feed::run` (`:49`), sorties `Out::{Event,Health,DecodeError}`. Le registre d'adapters est un `match` dans `app/src/main.rs:69-76`. Les fenêtres arrivent par un fil froid HTTP (`windows.rs:86`, `main.rs:288-339`) ; la boucle chaude est un `tokio::select!` sur 7 sources (`main.rs:407-679`).
- **Ordres** : la référence émet `Rec::Order` (`reference.rs:653,722`) vers `Paper::decide` (`paper.rs:153-169`), appariement simulé `due()` (`paper.rs:172-194`, aller 12,2 ms + retenue 150 ms). En réel : garde-fous, solde, signature, `live::ToSend` (`live.rs:87-119`), `Clob::post` (`exec.rs:152-193`). **Seuls des achats FAK** (`adapters/polymarket/src/sign.rs:116-119`, `app/src/main.rs:648` qui refuse les ventes) ; ni post-only ni annulation.
- **`replay`** : session JSONL vers `reference::Event` (`replay/src/main.rs:69-92`), modes `decisions` (parité 0 écart, `JOURNAL.md:116`), `fair`, `maker` (C4, remplacé par `maker-bt`).
- **`maker-bt` (Node)** : `simWindow` (`engine/maker-bt/sim.mjs:164-462`) mêle trois choses : la stratégie (`decide`, l. 311-400), l'exchange (`post`/`liveAt` 222-234, `onBook` 251-290, `exactFill` 148-156, sorties FAK par lecture anticipée `bookAt` 402-406) et la juste valeur (l. 193-217). **Les stratégies maker n'existent pas dans l'engine Rust.**

## 3. Options envisagées, choix, raisons

Source : [conception-engine-migration.md:13-18](migration/conception-engine-migration.md), section « Options envisagées ».

| # | Option | Pour | Contre |
|---|---|---|---|
| 1 | Traits statiques génériques `Engine<C: Clock, F: Feed, V: Venue>`, async | Coût d'abstraction nul | Il faut réécrire tout le `select!`. Le temps virtuel de tokio exige la feature `test-util`. L'ordonnancement reste non déterministe, ce qui menace la parité bit à bit. Gros refactor. |
| 2 | Traits `dyn` + enum de mode | Choix à l'exécution | L'async n'est pas `dyn`-compatible sans la crate `async-trait` (nouvelle dépendance, interdite sans accord) ou des `Pin<Box<Future>>` manuels. Même non-déterminisme. |
| **3 (retenue)** | **Cœur synchrone sans E/S, piloté par les événements** (`Engine::on(input, at, out)`, ne lit aucune horloge), **deux coquilles** : live (boucle tokio actuelle, avec une `LiveVenue` enveloppant la tâche d'envoi existante) et test (boucle déterministe, tas trié `(instant, n° de séquence)`, même mécanisme que `Reference::schedule`) | Voir ci-dessous | Extraire de `main.rs` la décision (l. 578-608 et 641-678) ; refactor moyen limité à `app` ; un `Vec` alloué par événement |

**Raisons du choix 3** ([conception-engine-migration.md:17](migration/conception-engine-migration.md)) :

1. Les briques à temps paramétré existent déjà (§2).
2. Les E/S live passent déjà par des canaux.
3. Le déterminisme donne la parité exacte : le portage de la référence a déjà atteint **0 écart** sur 22 sessions (`engine/08-strategies.md:57-61`).
4. Aucune dépendance, aucun async dans le cœur.
5. Conforme au contrat des trois implémentations prévues (Polymarket réel, Polymarket simulé, faux CLOB local) de `engine/02-conception.md:95-106` (§3.2, « Adaptateur d'exécution »).
6. Le papier devient « Engine + exchange simulé sur flux live » ([conception-engine-migration.md:17](migration/conception-engine-migration.md)) : **un seul** exchange simulé pour le papier et le backtest.

Coût accepté : un `Vec` par événement, jugé négligeable face à la cible de latence interne p99 ≤ 250 µs (`engine/JOURNAL.md:67`).

**Règle de parité maker.** Sur un même événement de carnet, `SimVenue` traite les **fills avant** la décision de la stratégie (parité `engine/maker-bt/sim.mjs:446-448` : `onBook(jb)` puis `decide`).

## 4. Interfaces (Contrat 2)

État au 2026-10-28 (dépôt cible) :

| Élément | Où |
|---|---|
| `trait Now`, `MarketSpec` | `engine/core/src/clock.rs`, `engine/core/src/market.rs` |
| `Out::Frame`, `Out::Connected`, `Out::Sent`, `registry::feed_adapter` | `engine/core/src/feed.rs`, `engine/app/src/registry.rs` |
| `Engine`, `EngineInput`, `Venue`, `Strategy`, `Decision`, `Command`, `VenueEvent` | `engine/app/src/engine.rs` ; deux ajouts optionnels au contrat : `on_tick`, `placed` |
| Stratégies derrière `Strategy` | `engine/app/src/strategies/{reference,maker}.rs` |
| Exchanges simulés | `backtest/src/venue.rs` (`WindowVenue`, packs, validé à 0 écart) ; `backtest/src/run.rs` (`TraceVenue`, traces, implémente `Venue`) |

Contrat figé, extrait fidèle de [contrats-migration.md:79-137](migration/contrats-migration.md) :

```rust
// core/src/clock.rs : la struct Clock existante implémente le trait
pub trait Now { fn now(&self) -> Stamp; }
// core/src/market.rs : remplace windows.rs:8 LEN_S, :39 slug, :40 window_start
pub struct MarketSpec { pub venue: String, pub asset: String, pub horizon_s: u64 }
//   slug(start) = "{asset}-updown-{h/60}m-{start}" ; window_start(unix_s) ; end(start)
pub struct FeeSchedule { pub rate: Micro, pub exponent: u32, pub rebate_rate: Micro }
pub struct Window { start, end, slug, market, tokens: [String; 2] /*[up,down]*/, tick, min_size,
                    fee: FeeSchedule, taker_delay: bool, twap_s: Option<u64>,
                    price_to_beat: Option<Micro>, winner: Option<usize> }
// app/src/engine.rs : app devient lib + bin
pub enum EngineInput { Feed(Event), Window(Window), Resolved{market, winner_token, up_won, price_to_beat}, Venue(VenueEvent), Tick }
pub enum Tif { Fak, Fok, Gtc, Gtd { expire_s: u64 } }
pub enum Amount { Usd(Micro), Shares(Micro) }
pub struct Intent { client_id, token, side: Dir, limit: Micro, amount: Amount, tif: Tif, post_only: bool, reason: &'static str }
pub enum Decision { Place(Intent), Cancel { client_id: u64 } }
pub enum Command { Place { n: u32, strategy: u16, intent: Intent, maker: u64, taker: u64 }, Cancel { n: u32 } }  // après garde-fous
pub enum VenueEvent { Answer{n, input: order::Input}, Fill{n, trade, status, size, price, maker: bool, fee, rebate: Option<Micro>}, Closed{n} }
pub trait Venue { fn submit(&mut self, cmd: Command, at: Stamp); fn poll(&mut self, until: Stamp, out: &mut Vec<(Stamp, VenueEvent)>); }
pub trait Strategy {
  fn sources(&self) -> Vec<String>;   // noms de sources de la config ; croiser N sources = déclarer N noms ; vérifiés au démarrage
  fn on_window(&mut self, w: &Window, at: Stamp, out: &mut Vec<Decision>);
  fn on_event(&mut self, source: &str, ev: &Event, books: Option<&Books>, out: &mut Vec<Decision>);
  fn on_resolved(&mut self, market: &str, up_won: bool, ptb: Option<Micro>, at: Stamp, out: &mut Vec<Decision>);
  fn on_venue(&mut self, ev: &VenueEvent, out: &mut Vec<Decision>);
  fn stats(&self) -> Vec<(&'static str, f64)>;
}
impl Engine { pub fn on(&mut self, input: EngineInput, at: Stamp, out: &mut Vec<Command>); }  // synchrone, ne lit aucune horloge
// app/src/registry.rs : feed_adapter(kind) et strategy(kind, params, &MarketSpec) -> Result<Box<dyn Strategy>, String> ("reference", "maker")
```

Règles associées :

- **Coquille live** = boucle tokio actuelle de `main.rs` + `LiveVenue`. Elle refuse tout sauf FAK, comme aujourd'hui (`app/src/main.rs:648`, `adapters/polymarket/src/sign.rs:116-119`). Le live n'envoie donc ni post-only ni annulation tant que la décision de l'utilisateur ne change pas (voir [`securite-et-mode-reel.md`](securite-et-mode-reel.md)).
- **Coquille test** (`backtest/`) = boucle déterministe. Elle fusionne les traces par `t_recv` (égalité : ordre des sources dans la config) et les événements de `SimVenue` dans un tas `(instant, n° de séquence)`. Les trames `raw` sont redécodées par le **même** `Adapter::decode` avec le `t_recv` enregistré ; les traces `.old` sont mappées directement en `Event`. Format des traces : voir `source-tracing/README.md` (dépôt cible) et [`donnees-publiques.md`](recherche/donnees-publiques.md) pour l'origine des données publiques.
- **Stratégies** : `strategy/src/reference.rs` est **inchangé** ; le registre refuse `reference` si `horizon_s` ≠ 300. La stratégie maker est portée **op par op** depuis `engine/maker-bt/sim.mjs` (`decide` + juste valeur) dans `strategy/src/maker.rs`, logique inchangée, avec le même refus tant que des constantes 300 s y subsistent.
- **L'asset et l'horizon sont des paramètres** (`MarketSpec`). BTC 5 min est le seul marché configuré et testé. ETH est possible par la config mais **n'est pas implémenté**. Le slug d'un autre actif est `[à vérifier]` ([conception-engine-migration.md:22](migration/conception-engine-migration.md)).

## 5. Exchange simulé (`SimVenue`)

Le modèle repose sur l'exchange de `engine/maker-bt/sim.mjs`, avec la décision D6 (§9). Il existe en deux exemplaires, de mêmes règles : `WindowVenue` (`backtest/src/venue.rs`) rejoue une fenêtre des packs, et c'est lui que valide la non-régression ; `TraceVenue` (`backtest/src/run.rs`) implémente le trait `Venue` de l'engine sur les traces. Écart à la cible : le mode papier du live garde son propre appariement (`adapter_polymarket::paper`), il n'y a donc pas encore un seul exchange simulé pour le papier et le backtest.

### 5.1 Ce que le modèle reproduit

- **Aller simple** : poses et annulations prennent effet `one_way_ms` après la décision (`sim.mjs:231`, `hist[s].push([t + ONE_WAY, o])`).
- **Un ordre n'est rempli que s'il y a de la liquidité en face** dans le carnet à l'instant d'arrivée ; la file maker est une file FIFO (le matching FIFO est établi sur les `OrderFilled` on-chain : `engine/maker-bt/WALLETS.md:39`).
- **Bornes de fill** (§5.2) : la borne `exact` utilise les trades réels retrouvés dans les deltas `price_change` (`fills.mjs`, `labelTrades`) ; elle exige les fills complets de data-api (`fetch-fills.mjs`) ; une fenêtre sans fills sort avec `why: 'no-fills'` (`WALLETS.md:121`).
- **Rebates** : 0,2 × 0,07 × p(1−p) × parts, comptés à chaque fill maker (`sim.mjs:185,240`) ; formule vérifiée à moins de 1 $ près sur deux versements quotidiens d'un seul wallet (`WALLETS.md:42`). Éligibilité et durabilité : `[à vérifier]`.
- **Frais taker** : `matchFee` avec le barème lu du marché (`fd.r`, `fd.e`) ; défaut `CRYPTO_FEE` (`sim.mjs:167`). Arrondi : 5 décimales ou entier exact `[à vérifier]` ([conception-engine-migration.md:22](migration/conception-engine-migration.md)).
- **Tick** lu du marché (0,01 puis 0,001 dès qu'un niveau à 3 décimales apparaît : `sim.mjs:117,123,249`) ; **taille minimale 5 parts** (`sim.mjs:37,395`). Aucun bid sous 5 parts.
- **Post-only** : jamais marketable, vérifié **à la décision** dans `sim.mjs:377-378`.
- **Carnet silencieux** plus de 3 s : aucun fill, aucune annulation, aucun stop (`sim.mjs:253,258,313`).
- **Résolution** : gagnant officiel de gamma, jamais une TWAP recalculée (`engine/11-maker-polymarket.md:109`).
- **Fin de fenêtre** : les événements au-delà de fin + 2 s sont ignorés (`sim.mjs:258`) et la boucle s'arrête à fin + 2,5 s (`sim.mjs:410`). **Aucune règle de coupure du carnet n'est documentée ni mesurée** dans le corpus : `[à vérifier]`, critique pour toute stratégie de fin de fenêtre (famille LT-REF, voir [`exploration-maker-b01-b10.md`](recherche/exploration-maker-b01-b10.md)).

### 5.2 Bornes de fill (`fill_bound`)

Définies en tête de `sim.mjs:14-29`. Elles bornent le **volume**, pas le PnL (`sim.mjs:29`).

| Borne | Règle | Statut |
|---|---|---|
| `prudent` | rempli quand un ask enregistré atteint notre prix, dans la limite de sa taille (règle de `replay/src/maker.rs`) | biaisée : PnL −97 % sur ordres réels (`WALLETS.md:56`) |
| `optimistic` | prudent + toute baisse d'un meilleur bid inchangé ≤ notre prix, premier dans la file | compte les annulations comme des fills ; volume ×1,196, PnL −12 % |
| `calibrated` | file réelle lue dans le carnet, trades publics par seconde (heures de bloc) | mal datée (retard 2,5 s) ; PnL +12 % |
| `through` | prudent + trades publics strictement à travers notre prix | biaisée : −19 % |
| **`exact`** | file réelle et trades réels retrouvés à la ms dans les deltas ; FIFO ; annulations au prorata de la file | **référence** : −4,3 % (autres makers), +8,4 % (arcsine) |

Chiffres : `engine/maker-bt/WALLETS.md:54-60,66-71`. Détail : [`simulateur-maker.md`](simulateur-maker.md).

### 5.3 Paramètres (défauts, sources, statut)

Tous les paramètres sont lus du fichier de config (§6) ; aucun n'est codé en dur.

| Paramètre | Défaut | Source de la valeur | Statut |
|---|---|---|---|
| `one_way_ms` | 12,2 ms | moitié du RTT chaud Dublin → CLOB p50 24,4 ms (`engine/JOURNAL.md:106`) ; `sim.mjs:36` ; `engine/08-strategies.md:81` | symétrie de l'aller simple `[à vérifier]` |
| `ack_ms` | 0 (comportement de `sim.mjs`, qui ne modélise aucun délai de réponse) | indicatif : `POST /order` validé puis refusé p50 32 ms, p90 42 ms, n = 46 (`engine/10-delai-appariement.md:31`) | délai de réponse d'un ordre accepté `[à vérifier]` ; valeur 32 ms non appliquée, mesurée sur un ordre refusé |
| `taker_hold` | quantiles_s `[[0,0.15],[0.1,0.19],[0.5,0.36],[0.9,1.5],[1,3.5]]`, `seed` 7 (ou valeur fixe) | `sim.mjs:39-40,168` ; dérivé du tableau du premier trade après un saut d'OKX et des 3 ordres réels (`engine/10-delai-appariement.md:70-75`, `engine/11-maker-polymarket.md:105`) | **pas une mesure directe** de la retenue (n = 3 ordres) `[à vérifier]` ; ne sert qu'aux sorties preneuses TP/SL (`sim.mjs:328`) |
| `fill_bound` | `exact` | recommandation `WALLETS.md:123` | choix de modèle validé sur ordres réels |
| `fill_lookback_ms` | **0** (obligatoire) | le défaut de `sim.mjs:166` est 150 ms (`opts.delay ?? 0.15`) ; `WALLETS.md:123` recommande 0 avec la borne `exact` ; avec 150 ms le PnL prédit tombe à 7 943 $ pour 16 383 $ réels (−52 %) (`WALLETS.md:71`) | établi sur arcsine (poses non retenues) ; annulations indécidables `[à vérifier]` |
| `cancel_queue` | `pro_rata` ; option `behind` (toutes les annulations derrière nous) | `sim.mjs:154` ; volume prédit 1,04 (prorata) / 0,94 (derrière) / 1,09 (devant) du réel (`WALLETS.md:40`) | réalité entre prorata et « derrière » : voir `cancel_ahead_factor` |
| `cancel_ahead_factor` | 0,5 (défaut depuis le 2026-10-28) ; 1 = prorata entier, l'ancien simulateur (non-régression : `config/nonreg.json`) | E1, mesuré le 2026-10-28 : volume de contrôle 1,052 → 1,006 avec 0,5 ([`simulateur-maker.md`](simulateur-maker.md) §10.2) | part de la réduction prorata appliquée à la file devant nous ; sans effet avec `behind` |
| `post_only_cross_check` | `decision` ; option `arrival` | `sim.mjs:377-378` ; un post-only qui croiserait est rejeté, pas retardé (doc officielle, voir [`ordres-et-appariement.md`](recherche/ordres-et-appariement.md)) | vérification à l'arrivée vs à la décision `[à vérifier]` |
| `silence_ms` | 3000 | `sim.mjs:38` ; seuil de silence Polymarket 3 s (`engine/JOURNAL.md:67`) | calibré sur 17 M de messages |
| `trading_cutoff_ms_after_end` | 2000 | `sim.mjs:258` | **aucune règle documentée** `[à vérifier]` |
| `rebates` | `true` ; 0,2 × 0,07 × p(1−p) × parts | `sim.mjs:185,240` ; `WALLETS.md:42` | vérifié sur 2 jours, 1 wallet |
| `claim_s` | 0 | `engine/maker-bt/run.mjs:40,60` | délai de réclamation inconnu `[à vérifier]` (`engine/11-maker-polymarket.md:111`) |
| `trade_fail_rate` | 0 | — | des trades `FAILED` restent possibles après `matched` `[à vérifier]` |
| `consume_own_liquidity` | `false` (notre ordre ne modifie pas le carnet rejoué) | `engine/08-strategies.md:88-89` (« la réaction du marché à notre ordre n'est pas modélisée ») | impact de nos ordres sur le carnet `[à vérifier]` |
| `min_size_rule` | taille minimale 5 parts ; tick lu du marché | `sim.mjs:37,395,249` ; `mos: 5` lu une fois (`engine/01-audit.md:237`) | règle en notionnel inconnue `[à vérifier]` |
| ordres FOK, GTD | refusés tant que non nécessaires | `Tif` du contrat | comportement de FOK non décrit dans le corpus `[à vérifier]` |

Ces paramètres sont détaillés dans [`contraintes-marche.md`](recherche/contraintes-marche.md) pour la partie « marché ».

## 6. Config JSON, sweep, `backtest/config/default.json` (exigence E2, [contrats-migration.md:197](migration/contrats-migration.md))

Un seul fichier édité à la main, `backtest/config/default.json`, contient **tous** les paramètres du backtest avec leur valeur par défaut : marché, sources (retards de réception inclus), exchange simulé (tous ceux du §5.3 plus chaque constante de `sim.mjs`), capital, stratégies. **Aucune constante cachée dans le code : le code lit le fichier.** Il est documenté dans [`backtest/README.md`](../backtest/README.md) : pour chaque paramètre, unité, source de la valeur, `[à vérifier]`. Chaque stratégie a en plus son fichier TOML commenté (`backtest/config/strategies/`), qui porte ses constantes (fin de fenêtre, boucle, tape, volatilité, horizon, conservation des prix, capital), défauts inchangés (D13).

Un fichier de lot ou de sweep ne contient que des **surcharges**, fusionnées en profondeur sur `default.json` : objets clé par clé, tableaux remplacés.

Les autres outils gardent leur propre fichier de config, eux aussi exhaustifs : `source-tracing/config.json`, config de l'engine.

### 6.1 Blocs

```
name
market   { venue, asset, horizon_s }
period   { from, to, split }
capital  { start_usd, compound, claim_s }
sources  [ { name, adapter, topics?, input: { trace | archive | pack, recv_lag_ms? } } ]
venue    { one_way_ms, ack_ms, taker_hold, fill_bound, fill_lookback_ms, cancel_queue, post_only_cross_check,
           silence_ms, trading_cutoff_ms_after_end, rebates, claim_s, trade_fail_rate, consume_own_liquidity, min_size_rule }
variants [ { id, "venue.x": ... } ]
strategy { kind, sources { role: nom | [noms] }, base { ... }, candidates [ { id, ... } ], grid { "clé.pointée": [ ... ] } }
```

### 6.2 Syntaxe de sweep

Reprise de `engine/maker-bt/explore.mjs` et `grid.mjs` : clés pointées (`"price.offset"`, `"cancelLoser.X"`), produit cartésien des valeurs de `grid` (`grid.mjs` `combos`), `candidates` = surcharges de `base` (`explore.mjs` `build`, `setPath`). Les noms de paramètres maker sont ceux de `engine/maker-bt/sim.mjs:42-77` (`f`, `shares`, `prob`, `sigmaMult`, `price`, `requote`, `pull`, `pullSide`, `spot`, `spotLatency`, `cancelLoser`, `tp`, `sl`, `exitSlip`, `mergeHold`, `entryFrom`, `quoteEnd`, `volMin`, `volMax`, `minPrevVolume`, `dailyStop`, `liqCap`, `reserveInFlight`, `join`, `sideProb`, `maxDiverge`, `noKnife`, `maxImbalance`, `pairEdge`, `orderShares`, `life`, `script`). **Les `batches/*.json` existants (21 fichiers : b01 à b10, diag-c4, r6-candidats-1 à 5, x01 à x05) doivent tourner.**

Exemple de lot actuel (`engine/maker-bt/batches/x04-mix-frozen.json`) : `{"name", "note", "base": {}, "candidates": [{"id":"X-P88-D0.08", "f":0.05, "prob":"fair", "price":{"mode":"offset","offset":0.01}, "sideProb":[0.88,0.999], "maxDiverge":0.08, "spot":"perp+coinbase", ...}]}`. Exemple de grille (`engine/maker-bt/results/G1/grid.json`) : `{"strategies":[{"name":"G1-A", "grid":{"f":[0.01,0.02], "price.offset":[0.02,0.03,0.05], "cancelLoser.X":[180,120]}}]}`.

### 6.3 Contenu de départ de `default.json` (conception ; le fichier réel fait foi)

```json
{
  "name": "default",
  "market":  { "venue": "polymarket", "asset": "btc", "horizon_s": 300 },
  "period":  { "from": null, "to": null, "split": "2026-10-20T00:00:00Z" },
  "capital": { "start_usd": 1000, "compound": true, "claim_s": 0 },
  "sources": [
    { "name": "polymarket-clob",      "adapter": "polymarket-market", "input": { "trace": "polymarket-clob" } },
    { "name": "chainlink-rtds",       "adapter": "chainlink-rtds",    "input": { "trace": "chainlink-rtds" } },
    { "name": "okx-btc-usdt-swap",    "adapter": "okx-bbo-tbt",       "input": { "trace": "okx-btc-usdt-swap" } },
    { "name": "binance-btcusdt",      "adapter": "binance-spot",      "input": { "trace": "binance-btcusdt",      "recv_lag_ms": 120 } },
    { "name": "binance-perp-btcusdt", "adapter": "binance-perp",      "input": { "trace": "binance-perp-btcusdt", "recv_lag_ms": 113 } },
    { "name": "coinbase-btc-usd",     "adapter": "coinbase",          "input": { "trace": "coinbase-btc-usd",     "recv_lag_ms": 40 } }
  ],
  "venue": {
    "one_way_ms": 12.2, "ack_ms": 0,
    "taker_hold": { "quantiles_s": [[0,0.15],[0.1,0.19],[0.5,0.36],[0.9,1.5],[1,3.5]], "seed": 7 },
    "fill_bound": "exact", "fill_lookback_ms": 0, "cancel_queue": "pro_rata", "cancel_ahead_factor": 0.5,
    "post_only_cross_check": "decision", "silence_ms": 3000, "trading_cutoff_ms_after_end": 2000,
    "rebates": true, "trade_fail_rate": 0, "consume_own_liquidity": false,
    "min_size_rule": { "min_shares": 5 }
  },
  "strategy": { "kind": "maker", "sources": {}, "base": { "...": "défauts de sim.mjs:42-77" }, "candidates": [], "grid": {} }
}
```

Dans `default.json` réel, tous les `recv_lag_ms` valent 0 ; les retards ci-dessous se posent en surcharge. Origine des valeurs de `sources` : `recv_lag_ms` Binance spot 120 ms = latence mesurée pour OKX vers Dublin, **non vérifiée pour Binance** (`WALLETS.md:154-155`) ; perp +113 ms = p95 de l'utilisateur ; Coinbase +40 ms **non vérifié** (`engine/maker-bt/EXPLORATION.md:252`). Noms des sources : contrat du format de trace (`source-tracing/README.md`). L'ancien backtest lisait la valeur par défaut `sigmaMult` = 1,5 (`sim.mjs:48`) ; la calibration dépend de la source de prix (×1,0 OKX, ×1,2 composite, ×1,3 Binance : `WALLETS.md:155-157,187`), à porter dans `strategy.base` ou les `candidates`, jamais dans le code.

### 6.4 Constantes de `sim.mjs` à sortir du code (E2)

Inventaire des constantes aujourd'hui codées en dur, qui doivent devenir des clés de `default.json` :

| Constante | Valeur | Ligne dans `sim.mjs` |
|---|---|---|
| `ONE_WAY` | 12,2 ms | 36 |
| `MIN_SIZE` | 5 parts | 37 |
| `SILENCE_MS` | 3000 | 38 |
| `HOLD_Q` | voir §5.3 | 40 |
| défaut de retenue `delay` | 0,15 s (à remplacer par `fill_lookback_ms` 0) | 166 |
| graine `seed` | 7 | 168 |
| fin de fenêtre `END` | 300 000 ms (dérivée de `horizon_s`) | 173 |
| coupure des événements après la fin | 2000 ms | 258 |
| fin de boucle `T_END` | fin + 2500 ms | 410 |
| marge des ordres en vol (`reserveInFlight`) | retenue + 1000 ms | 228 |
| fenêtre de volatilité `Vol(300)` | 300 s | 195 |
| EMA de la base vers Chainlink | 1/30 par impression ; OKX périmé au-delà de 5000 ms | 428 |
| TWAP de règlement | 60 s | 214, 216 |
| profondeur du plafond de liquidité (`liqCap`) | [q − 0,05 ; q] | 387 |
| précision des tailles `floor2` | 2 décimales | 82 |
| tentatives de sortie | 1 par côté et par seconde | 323, 327 |

Tout paramètre de stratégie maker déjà dans `DEFAULTS` (`sim.mjs:42-77`) reste dans `strategy.base`.

## 7. Plan de non-régression maker (tolérance ZÉRO, f64 bit à bit)

**Résultat au 2026-10-28 : 0 écart** sur `validate` (59 695 jumeaux) et sur les lots `x01-retest-exact` et `x04-mix-frozen` (16 720 lignes chacun), comparés bit à bit à l'oracle Node (`backtest/scripts/validate-wsl.sh`, `batch-wsl.sh`) ; `replay decisions` identique avant et après le refactor sur 3 sessions. Référence **figée** le 2026-10-27, re-mesurée avec `node --max-old-space-size=8192 engine/maker-bt/validate.mjs --delay 0 --sim exact` (21 min 42 s) :

```
simWindow on 59695 twins after the open with a known end (real 1089572 sh, PnL 17331 $):
  exact      predicted 1137900 sh, ratio 1.044, orders exact 0.841, PnL 16581 $
  10-15 real 4684 $ | exact 4535 $     10-16 real 567 $ | exact 710 $
  10-17 real -6496 $ | exact -6557 $   10-18 real -1264 $ | exact -1550 $
  10-19 real 6225 $ | exact 5671 $     10-20 real 5162 $ | exact 5306 $
  10-21 real 8874 $ | exact 8911 $     10-22 real -422 $ | exact -444 $
```

Les valeurs par jour sont identiques à `engine/maker-bt/WALLETS.md:62`. Étape intermédiaire notée : 71 049 ordres, 66 273 arrivées trouvées, ratio 1,102 ; 63 152 ordres de fin connue, ratio 1,042, exacts 0,812 ([conception-engine-migration.md:25](migration/conception-engine-migration.md)).

Quatre contrôles, tous avec **diff vide** :

1. **Tests unitaires** de `engine/maker-bt/test/sim.test.mjs` portés en Rust. Le fichier contient 9 tests (`WALLETS.md:119` en annonçait 8/8 au 2026-10-25) : capital inchangé si f = 0 ; ask à notre prix remplit seulement une fois l'ordre vivant ; un trade antérieur à la pose ne remplit jamais ; une annulation effective avant la soumission du preneur évite le fill ; un stop pendant un carnet silencieux n'est pas exécuté ; `reserveInFlight` ; parité C4 sur les carnets enregistrés (≈ 4 min, ignorée sans les packs) ; borne `exact` (FIFO, annulations au prorata, trade sous notre bid) ; une fenêtre sans trades du spot demandé est ignorée au lieu de retomber sur OKX.
2. **Référence `validate.mjs`** ci-dessus.
3. **Lots `x01-retest-exact` et `x04-mix-frozen`** en `--bound exact --delay 0` : sorties de référence par (candidat, variante, fenêtre), exportées depuis le JS en lecture seule du dépôt source. **Ne pas lancer `explore.mjs`**, qui écrit dans le dépôt source (`EXPLORATION.md`). Comparaison avec la sortie Rust.
4. **`replay decisions`** avant/après refactor : diff vide. Précédent : la parité de la référence a donné 0 écart sur les variantes A, B, C (`engine/08-strategies.md:57-65`). Note : `JOURNAL.md:116` cite un script `bench/parity.sh` **absent** du dépôt source (`engine/bench/` ne contient que `book-drop.mjs`) `[à vérifier]` : le script de parité est à reconstruire.

Aucune fixture réduite n'est commitée (`backtest/fixtures/` est vide) : la non-régression lit les packs du dépôt source. Le run complet lit les packs et données du dépôt source en lecture seule (≈ 12 Go de `packs/B`, hors git : `EXPLORATION.md:21-23`). Les données `data/wallets` ne sont pas copiées (décision D9) : la non-régression sur les ordres d'arcsine n'est donc pas couverte ici `[à vérifier]` (seuls les ordres des autres makers, 35 fenêtres, le sont).

## 8. Correction des décalages avec le réel (exigence E1, [contrats-migration.md:192-196](migration/contrats-migration.md))

Décalages de départ (`engine/maker-bt/WALLETS.md:52-71`) :

| Jeu | Volume prédit / réel | Ordres exacts | PnL prédit / réel |
|---|---|---|---|
| Autres makers (`validate`, 59 695 ordres, 35 fenêtres) | 1,044 | 84,1 % | 16 581 $ / 17 331 $ (−4,3 %) |
| arcsine (29 579 ordres) | 0,958 | 71,0 % | 16 645 $ / 15 360 $ (+8,4 %) |

Les écarts sont de **signe opposé** (volume sur-prédit d'un côté, sous-prédit de l'autre) : une correction globale unique ne peut pas réduire les deux. À garder en tête avant toute modification.

### 8.1 Méthode imposée

1. **D'abord le portage bit à bit** : zéro écart avec les valeurs validées (§7).
2. **Ensuite, chaque correction = une option NOUVELLE et paramétrée du modèle de l'exchange simulé**, jamais de la logique d'une stratégie. Sa valeur par défaut reproduit le comportement validé, donc la non-régression reste vraie.
3. **Chaque correction est mesurée sur `validate`** (makers tiers, et arcsine si les données sont disponibles sans `data/wallets` ; sinon makers tiers seulement), avant et après, avec les chiffres réels, et **fondée sur une cause documentée ou mesurée**. Aucun réglage aveugle sur le résultat : fenêtres d'ajustement et fenêtres de contrôle séparées par un **split temporel**.
4. **Résultat documenté** dans [`simulateur-maker.md`](simulateur-maker.md) (gain, risque de surajustement, valeurs retenues) et dans [`journal-decisions.md`](journal-decisions.md).

### 8.2 Causes candidates (examinées le 2026-10-28)

| Cause | Fondement | Option envisagée (nom provisoire) |
|---|---|---|
| Annulations : la réalité est entre « prorata » et « toutes derrière nous » | volume prédit 1,04 / 0,94 / 1,09 selon l'hypothèse (`WALLETS.md:40`) | FAIT : `cancel_ahead_factor` (§6, [`simulateur-maker.md`](simulateur-maker.md) §10.2) |
| File prise à la décision vs à l'arrivée | `sim.mjs:396-397` lit la file au moment de la décision (`qa = book.bidSize(...)`), alors que `validate.mjs` (en-tête, l. 1-12) prend l'arrivée comme l'augmentation du carnet de exactement sa taille (traduction libre) et la file juste avant | non mesurable par `validate`, qui pose déjà l'ordre à l'arrivée ; reste `[à vérifier]` |
| Ordres de 5 parts mal retrouvés dans les deltas | « moins sûr pour les ordres de 5 parts et ceux de ~150 parts » (`WALLETS.md:82`) | confirmé le 2026-10-28 (part des fills précoces 49 % à 5 parts, 3 % au-delà de 100) : erreur de mesure, [`simulateur-maker.md`](simulateur-maker.md) §10.3 |
| 4 % du volume d'arcsine sans fill prédit | pose mal retrouvée, file aberrante (`WALLETS.md:83`) | idem : erreur de mesure à séparer de l'erreur de modèle |
| Seuls les ordres ayant reçu au moins un fill sont visibles | on ne mesure pas les fills que le simulateur inventerait à des ordres jamais remplis (`WALLETS.md:81`) | limite de la validation, pas corrigeable par une option |

### 8.3 Split temporel retenu

`SPLIT` = 2026-10-20 00:00 UTC (`engine/maker-bt/run.mjs:18`, `period.split` de la config) : ajustement 15→19/10 (38 218 jumeaux), contrôle 20→22/10 (21 477 jumeaux). Outil : `backtest/scripts/split.mjs`.

**Résultat (2026-10-28)** : correction 1 mesurée, `cancel_ahead_factor` (volume de contrôle 1,052 → 1,006 avec 0,5 ; ordres exacts −0,6 point) : [`simulateur-maker.md`](simulateur-maker.md) §10. La cause « file à la décision vs à l'arrivée » n'est pas mesurable par `validate` : un ordre scripté prend déjà sa file à l'arrivée (`backtest/src/venue.rs`, `script`).

## 9. Limite connue : sorties TP/SL (décision D6)

Le simulateur JS applique le produit des sorties preneuses (take profit, stop loss) **dès la décision**, en lisant le carnet en avance : `sim.mjs:328-343` (`hold` tiré, `m = t + ONE_WAY + hold`, `bookAt(m)` clone le carnet et rejoue les événements jusqu'à `m`, puis `rec.proceeds`, la position et l'annulation du bid sont mis à jour à l'instant `t` de la décision). Un exchange réaliste livre le fill **à l'arrivée**.

Décision D6 ([contrats-migration.md:138](migration/contrats-migration.md), [:186](migration/contrats-migration.md)) : `SimVenue` est réaliste (fill livré à l'arrivée). La non-régression bit à bit est donc **limitée aux configs sans TP/SL** : `x01-retest-exact`, `x04-mix-frozen`, `validate`. L'écart pour les configs avec TP/SL (par exemple G2-C, G4-E de [`strategies-maker.md`](recherche/strategies-maker.md)) est **documenté, non corrigé** ; son ampleur n'est pas mesurée `[à mesurer]`.

## 10. Points à vérifier

Limites du mode trace (mesurées le 2026-10-28, détail dans [`backtest/README.md`](../backtest/README.md)) : décalage d'horloge des anciennes sessions non transmis à l'engine ; borne `exact` sans trades sur les anciennes traces ; traces courantes du 28/10 sans résultat officiel (traceur corrigé et redéployé le jour même).

Points `[à vérifier]` d'architecture ([conception-engine-migration.md:22](migration/conception-engine-migration.md)) : symétrie de l'aller simple ; délai de réponse d'un ordre accepté ; ordre de sortie des ordres retenus ; retenue des annulations maker ; sémantique FOK ; post-only vérifié à l'arrivée vs à la décision ; file mesurée à la décision vs à l'arrivée ; impact de nos ordres sur le carnet ; annulations prorata vs « toutes derrière » ; `last_trade_price` vs `labelTrades` ; taille minimale côté serveur ; arrondi des frais (5 décimales vs entier exact) ; durabilité des rebates ; délai de réclamation ; ordres après la fin de fenêtre ; slug d'un autre actif ; taux de trades `FAILED` ; `bench/parity.sh` absent.
