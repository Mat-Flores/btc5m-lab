# Contrats figés — migration btc5m-lag-sim → updown5m-lab (validés par l'utilisateur le 2026-10-27)

**Historique.** Contrats et décisions de la migration, figés les 27 et 28/10/2026. L'état actuel du projet est décrit à la racine de `docs/` ; le contrat 3 (UI) est caduc (D11).

Dépôt source (LECTURE SEULE) : `C:\dev\REA Projects\btc5m-lag-sim`. Dépôt cible : `C:\dev\REA Projects\updown5m-lab`.
Spécification complète : `C:\dev\REA Projects\updown5m-lab\PROMPT-MIGRATION.md` (ne jamais la modifier).

## Contrat 1 — Format de trace v1

Chemins :
- jour validé : `source-tracing/trace/<source>/<AAAA>/<MM>/<AAAA-MM-JJ>.jsonl.gz` + `<AAAA-MM-JJ>.manifest.json`
- ancien format converti : `<AAAA-MM-JJ>.old.jsonl.gz` + `<AAAA-MM-JJ>.old.manifest.json`
- plusieurs hôtes le même jour (anciennes traces seulement, décision D5) : l'hôte qui a le plus de lignes garde le nom canonique ;
  les autres : `<AAAA-MM-JJ>.<host>.old.jsonl.gz` + `<AAAA-MM-JJ>.<host>.old.manifest.json`
- en cours de journée : `<AAAA-MM-JJ>.jsonl.gz.part-<run>` (un membre gzip complet par lancement) ; scellement via `.tmp`
- `trace/` est gitignoré, sauf une petite fixture de test.

Encodage : JSONL gzip niveau 6, multi-membres. Écriture par le binaire `gzip` lancé en sous-processus (comme `engine/app/src/paper.rs:338`).

Enveloppe de chaque ligne (clés dans cet ordre) :
`{"seq":12345,"t_recv":1792359572098123,"t_src":1792359572098000,"seq_src":null,"conn":0,"kind":"delta","raw":"<trame texte exacte>"}`
- `seq` u64 : compteur local par lancement, contigu, header = 0
- `t_recv` i64 : microsecondes UTC (horloge murale hôte, `Stamp.wall_ns/1000`) ; pour les lignes de cycle de vie = instant de l'événement
- `t_src` i64 µs ou null : horodatage source (`event_ms*1000`)
- `seq_src` u64 ou null : séquence source (retour de `decode` ou `Event.seq`)
- `conn` u16 ou null : indice de connexion dans la source
- `raw` string ou null : texte EXACT de la trame reçue/envoyée (chaîne, pas objet) ; null en `.old`
- champs propres selon `kind` : `n_ev, dup, url, topics, attempt, reason, detail, from, to, expected, got, error, method, status, t_sent, offset_ms, legacy, inferred, derived, lines`

`kind` :
- `header` (seq 0 de chaque lancement) : `schema:"updown5m-trace", schema_version:1, time_unit:"us", source, adapter, topics, connections, date, host_id, hostname, recorder:"source-tracing", recorder_version, run, run_id, format:"current"|"legacy-btc5m-session-v1", prev_run_clean`
- `snapshot`, `delta`, `trade`, `quote`, `oracle`, `twap`, `tick` : classement du `Payload` décodé par l'adapter (`raw`, `n_ev`, `dup?`)
- `msg` : trame sans événement (ack, pong) ou mêlant plusieurs types
- `sent` : trame envoyée (abonnement/désabonnement ; pings non enregistrés)
- `http` : requête REST (gamma, clob-markets, `/time`) : `method, url, status, t_sent, raw` (corps)
- `connect` (`url, topics, attempt`) / `disconnect` (`reason` ∈ close, error, timeout, silence, seq_gap, decode, disabled, shutdown, disk_low ; `detail`)
- `gap` : `from, to` (µs), `reason` ∈ silence, seq_gap, reconnect, restart, disabled, disk_low, no_recording ; `expected?, got?, inferred?`
- `decode_error` : `raw` (trame complète), `error`
- `clock` : `offset_ms`
- `end` : `lines`, `reason` ∈ rotate, shutdown
- réservés aux `.old` : `recorder_start`, `mid`, `window`, `resolution`, `price_to_beat`, `rtt` (+ `legacy` = ligne d'origine en objet)

Rotation : date d'une ligne = date UTC de son `t_recv`. À 00:00:00.000 UTC (minuterie, même source muette) : `end{rotate}`, fermeture du membre gzip, ouverture de `<J+1>.jsonl.gz.part-<run>` avec un `header`.
Scellement (fil séparé, dans l'ordre) : 1) réparer un morceau mal fermé (lecture tolérante jusqu'à la dernière ligne complète, recompression de ce seul morceau) ; 2) concaténer les morceaux octet par octet + sha256 à la volée dans `.tmp`, fsync ; 3) `manifest.json.tmp`, fsync ; 4) renommer les données puis renommer le manifeste = POINT DE VALIDATION ; 5) supprimer les morceaux. Au démarrage : sceller tout `.part-*` d'un jour passé ; aujourd'hui → `part-<run+1>`. SIGTERM → `disconnect{shutdown}` + `end{shutdown}`.

Garde disque (décision D3, solution gratuite) : si l'espace libre du volume de `root` < 2 Go → `disconnect{disk_low}` + `gap{disk_low}` et arrêt d'écriture des sources jusqu'à retour > 3 Go.

Manifeste :
`{"schema_version":1,"source":…,"date":"AAAA-MM-JJ","file":…,"format":"current"|"old","host_id":…,"bytes":…,"sha256":…,"lines":…,"lines_by_kind":{…},"first_t_recv":…,"last_t_recv":…,"runs":[{"run","host_id","recorder_version","first_t_recv","last_t_recv","lines","clean_end"}],"hours":[{"h":0..23,"lines","covered_s"}],"covered_s":…,"coverage":…,"gaps":[{"from","to","reason","origin":"recorded"|"inferred"}],"gaps_total_s":…,"disconnects":…,"decode_errors":…,"silence_threshold_ms":…,"legacy":null|{"sessions":[…],"truncated_input":bool},"validated_at":…,"validator_version":…}`
Trous = lignes `gap` + silences > seuil de la source (`Caps.silence` : OKX 3 s, Polymarket 3 s, Chainlink 10 s) + 00:00→1re ligne + dernière ligne→24:00 + intervalles entre lancements.
Un jour est VALIDÉ si son manifeste existe (mode strict : `bytes` + `sha256` vérifiés).

Sources de départ (noms = clés de config) :
| source | adapter | topics |
| `polymarket-clob` | `polymarket-market` (1 connexion) | tokens fenêtres courante + suivante(s) |
| `polymarket-gamma` | HTTP `windows.rs` | slugs du marché configuré |
| `polymarket-clob-time` | HTTP `GET /time` | — |
| `chainlink-rtds` | `chainlink-rtds` | `btc/usd` |
| `okx-btc-usdt-swap` | `okx-bbo-tbt` | `BTC-USDT-SWAP` |
| `binance-btcusdt` | `binance-spot` (NOUVEAU, D2) | `btcusdt` |
| `binance-perp-btcusdt` | `binance-perp` (NOUVEAU, D2) | `btcusdt` |
| `coinbase-btc-usd` | `coinbase` (NOUVEAU, D2) | `BTC-USD` |

Config tracer `source-tracing/config.json` : `{"host_id":…,"root":"…/trace","sources":[{"name","adapter","topics":[…],"connections":1,"enabled":true}]}`.

Mapping ancien → nouveau (sessions `data/sessions/*.jsonl.gz`, format `{"k":…,"t":…}`) : toujours `.old`. Découpage par source puis jour UTC de `t` puis hôte (`nodeName` sinon hostname : `ip-172-31-19-166`→`dublin`, `ip-172-31-28-37`→`dry-run`, `ip-172-31-28-36`→`live`, `mf-laptop`→`local`). Sessions d'un même hôte/jour concaténées par temps, chacune précédée de `recorder_start{legacy: en-tête}` et séparées par `gap{reason:no_recording, inferred:true}`. `seq` = numéro de ligne dans la session d'origine ; `raw`=null ; `seq_src`=null ; `conn`=0 (Node) ou null (capture engine) ; `legacy` = ligne d'origine en objet.
| ancien | source | kind | t_recv | t_src |
| `session` | toutes sources de la session | `recorder_start` | `Date.parse(id)` à la s, `inferred:"id"` | null |
| `px` src=binance/binanceFut/coinbase/bybit/okx/kraken/bitstamp | `binance-btcusdt`/`binance-perp-btcusdt`/`coinbase-btc-usd`/`bybit-btcusdt-linear`/`okx-btc-usdt-swap`/`kraken-btc-usd`/`bitstamp-btcusd` | `mid` | round(t*1000) | `et*1000` ou null |
| `cl` | `chainlink-rtds` | `oracle` | idem | `ts*1000` |
| `book` | `polymarket-clob` | `snapshot` | idem | `ts*1000` (ts==0 → null ; book synthétisé sans ts → `derived:true`) |
| `delta` | `polymarket-clob` | `delta` | idem | idem |
| `trade` (capture engine) | `polymarket-clob` | `trade` | idem | `ts*1000` |
| `meta`/`res`/`ptb` | `polymarket-gamma` | `window`/`resolution`/`price_to_beat` | idem | null |
| `rtt` | `polymarket-clob-time` | `rtt` | idem | null |
| `clock` | copié dans chaque fichier de la session et du jour | `clock` | idem | null (t_recv NON corrigé) |
| `cfg`,`lane`,`real` | — | NON convertis | | |
Exclus : `2026-10-16T09-00-00_sim` (synthétique), `2026-10-15T20-37-05` (vide). `2026-10-17T20-20-48_live` : jusqu'à la dernière ligne complète, `legacy.truncated_input:true`. Même `id` de session présent à plusieurs endroits (local, serveurs) → une seule fois (le fichier le plus long/complet). Ne jamais inventer un champ manquant.
Non convertis (traces) : `*.records.jsonl`, `*.dryrun.jsonl`, `*.pack/`, `data/backtests`, `data/live`, `data/binance/**`, `data/polymarket/**`, probes, strategies.json.

## Contrat 2 — Interfaces engine (cœur sans E/S, deux coquilles)

```rust
// core/src/clock.rs — NOUVEAU trait ; la struct Clock existante l'implémente
pub trait Now { fn now(&self) -> Stamp; }
// core/src/market.rs — NOUVEAU ; remplace windows.rs:8 LEN_S, :39 slug, :40 window_start
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)] #[serde(deny_unknown_fields)]
pub struct MarketSpec { pub venue: String, pub asset: String, pub horizon_s: u64 }
impl MarketSpec { pub fn slug(&self, start: u64) -> String; /* "{asset}-updown-{h/60}m-{start}" ; BTC 5m inchangé */
                  pub fn window_start(&self, unix_s: u64) -> u64; pub fn end(&self, start: u64) -> u64; }
pub struct FeeSchedule { pub rate: Micro, pub exponent: u32, pub rebate_rate: Micro }   // rebate_rate : parse de feeSchedule.rebateRate
pub struct Window { pub start: u64, pub end: u64, pub slug: String, pub market: String, pub tokens: [String; 2] /*[up,down]*/,
  pub tick: Micro, pub min_size: Micro, pub fee: FeeSchedule, pub taker_delay: bool, pub twap_s: Option<u64>,
  pub price_to_beat: Option<Micro>, pub winner: Option<usize> }   // déplacée depuis adapters/polymarket/src/windows.rs:17-35
// core/src/feed.rs — 3 sorties AJOUTÉES pour le tracing, émises seulement si demandé (flag au lancement de run) :
//   Out::Frame { source, conn: u16, at: Stamp, text: String }      // toute trame texte reçue, avant decode
//   Out::Connected { source, conn: u16, at: Stamp, url: String }
//   Out::Sent { source, conn: u16, at: Stamp, text: String }       // abonnements/désabonnements
//   (Out::Health et Out::DecodeError existants inchangés ; DecodeError doit pouvoir porter la trame complète quand le flag est actif)
// app/src/engine.rs — NOUVEAU (app devient lib + bin)
pub enum EngineInput { Feed(engine_core::event::Event), Window(Window),
  Resolved { market: String, winner_token: String, up_won: bool, price_to_beat: Option<Micro> }, Venue(VenueEvent), Tick }
pub enum Tif { Fak, Fok, Gtc, Gtd { expire_s: u64 } }
pub enum Amount { Usd(Micro), Shares(Micro) }
pub struct Intent { pub client_id: u64, pub token: String, pub side: engine_core::account::Dir, pub limit: Micro,
  pub amount: Amount, pub tif: Tif, pub post_only: bool, pub reason: &'static str }
pub enum Decision { Place(Intent), Cancel { client_id: u64 } }
pub enum Command { Place { n: u32, strategy: u16, intent: Intent, maker: u64, taker: u64 }, Cancel { n: u32 } } // après garde-fous
pub enum VenueEvent { Answer { n: u32, input: engine_core::order::Input },
  Fill { n: u32, trade: String, status: TradeStatus, size: Micro, price: Micro, maker: bool, fee: Micro, rebate: Option<Micro> },
  Closed { n: u32 } }
pub trait Venue { fn submit(&mut self, cmd: Command, at: Stamp); fn poll(&mut self, until: Stamp, out: &mut Vec<(Stamp, VenueEvent)>); }
pub trait Strategy {
  fn sources(&self) -> Vec<String>;   // noms de sources de la config lues ; croiser N sources = déclarer N noms ; vérifiés au démarrage
  fn on_window(&mut self, w: &Window, at: Stamp, out: &mut Vec<Decision>);
  fn on_event(&mut self, source: &str, ev: &Event, books: Option<&Books>, out: &mut Vec<Decision>);
  fn on_resolved(&mut self, market: &str, up_won: bool, ptb: Option<Micro>, at: Stamp, out: &mut Vec<Decision>);
  fn on_venue(&mut self, ev: &VenueEvent, out: &mut Vec<Decision>);
  fn stats(&self) -> Vec<(&'static str, f64)>;
}
pub struct Engine { /* strategies: Vec<Box<dyn Strategy>>, orders, guards, account, books… */ }
impl Engine { pub fn on(&mut self, input: EngineInput, at: Stamp, out: &mut Vec<Command>); }  // synchrone, ne lit aucune horloge
// app/src/registry.rs — NOUVEAU (déplace main.rs:69-76)
pub fn feed_adapter(kind: &str) -> Option<(Box<dyn Adapter>, bool)>;  // ajouter une source = 1 crate adapter + 1 ligne ici
pub fn strategy(kind: &str, params: serde_json::Value, m: &MarketSpec) -> Result<Box<dyn Strategy>, String>; // "reference", "maker"
```
- Ajouts pour source-tracing (fournis par `engine/`) :
  - `adapter_polymarket::http_get(url: &str, timeout: Duration) -> impl Future<Output = Result<(u16 /*status*/, String /*corps brut*/), String>>` public, réutilisant le client HTTP existant (`exec.rs` dial/Conn, tokio-rustls) ; `windows::fetch` l'utilise.
  - `source-tracing` choisit ses adapters via `engine_app::registry::feed_adapter(kind)` (crate `engine-app` exposé en lib) : un seul endroit à modifier pour ajouter une source.
  - Kinds du registre : `polymarket-market`, `chainlink-rtds`, `okx-bbo-tbt`, `binance-spot`, `binance-perp`, `coinbase` (crates `adapters/binance` (spot+perp) et `adapters/coinbase`, logique de parsing reprise de `git show 7138fbc^:src/feeds/live.js` dans le dépôt source, lecture seule).
- Toolchain : Rust 1.99.1 dans WSL `Ubuntu-24.04` (pas de cargo sous Windows). Depuis Git Bash :
  `wsl.exe -d Ubuntu-24.04 -- bash -c 'cd "/mnt/c/dev/REA Projects/updown5m-lab/<outil>" && ~/.cargo/bin/cargo test'`
  Node 24 est disponible sous Windows (`node`).
- Coquille live = boucle tokio actuelle de `main.rs` + `LiveVenue` (enveloppe de la tâche d'envoi existante ; refuse tout sauf FAK, comme aujourd'hui).
- Coquille test (`backtest/`) = boucle déterministe : fusion des traces (par `t_recv`, égalité → ordre des sources dans la config) et des événements de `SimVenue` dans un tas trié `(instant, n° séquence)`. Les trames `raw` sont redécodées par le MÊME `Adapter::decode` avec le `t_recv` enregistré ; les `.old` sont mappées directement en `Event`.
- Sur un même événement de carnet, `SimVenue` traite les fills AVANT la décision de la stratégie (parité `sim.mjs:446-448`).
- `strategy/src/reference.rs` INCHANGÉ ; le registre refuse Reference si `horizon_s` ≠ 300. Stratégie maker portée op par op depuis `engine/maker-bt/sim.mjs` (decide + juste valeur) dans `strategy/src/maker.rs`, logique inchangée ; mêmes refus si horizon ≠ 300 tant que les constantes y sont.
- Interdit : modifier la logique d'une stratégie ; ajouter une dépendance (stop et demander).

SimVenue (backtest/, vague 2b) — paramètres (défauts) : `one_way_ms` 12.2 ; `ack_ms` ; `taker_hold` {quantiles_s [[0,0.15],[0.1,0.19],[0.5,0.36],[0.9,1.5],[1,3.5]], seed} ou fixe ; `fill_bound` exact|prudent|through|calibrated|optimistic ; `fill_lookback_ms` 0 (obligatoire) ; `cancel_queue` pro_rata|behind ; `post_only_cross_check` decision|arrival ; `silence_ms` 3000 ; `trading_cutoff_ms_after_end` 2000 [à vérifier] ; `rebates` true (0.2×0.07×p(1−p)×parts) ; `claim_s` 0 ; `trade_fail_rate` 0 ; `consume_own_liquidity` ; `min_size_rule` ; tick lu du marché (0.01/0.001), taille min 5 ; FOK et GTD refusés tant que non nécessaires [à vérifier]. Décision D6 : SimVenue réaliste (fill livré à l'arrivée) ; non-régression bit à bit limitée aux configs sans TP/SL (x01, x04, validate) ; écart documenté.

Config backtest : JSON. Blocs `name, market{venue,asset,horizon_s}, period{from,to,split}, capital{start_usd,compound,claim_s}, sources[{name,adapter,topics?,input:{trace|archive|pack,recv_lag_ms?}}], venue{…}, variants[{id, "venue.x": …}], strategy{kind, sources{role: nom|[noms]}, base{…}, candidates[{id,…}], grid{"clé.pointée":[…]}}`. Syntaxe de sweep = celle de `engine/maker-bt/explore.mjs` / `grid.mjs` ; noms de paramètres maker = ceux de `sim.mjs:42-77` (les `batches/*.json` existants doivent tourner).

Non-régression maker (tolérance ZÉRO, f64 bit à bit) :
1. 8-9 tests de `engine/maker-bt/test/sim.test.mjs` portés en Rust ;
2. référence figée : `node --max-old-space-size=8192 engine/maker-bt/validate.mjs --delay 0 --sim exact` (≈22 min) donne :
```
simWindow on 59695 twins after the open with a known end (real 1089572 sh, PnL 17331 $):
  exact      predicted 1137900 sh, ratio 1.044, orders exact 0.841, PnL 16581 $
  10-15 real 4684 $ | exact 4535 $
  10-16 real 567 $ | exact 710 $
  10-17 real -6496 $ | exact -6557 $
  10-18 real -1264 $ | exact -1550 $
  10-19 real 6225 $ | exact 5671 $
  10-20 real 5162 $ | exact 5306 $
  10-21 real 8874 $ | exact 8911 $
  10-22 real -422 $ | exact -444 $
```
3. lots `x01-retest-exact` et `x04-mix-frozen` en `--bound exact --delay 0` : sorties de référence par (candidat, variante, fenêtre) exportées depuis le JS (lecture seule du source ; NE PAS lancer `explore.mjs` qui écrit dans le source), diff vide ;
4. `replay decisions` avant/après refactor : diff vide.
Fixture commitée réduite (~3 fenêtres, < 20 Mo) ; run complet en lisant les packs/data du dépôt source en lecture seule.

## Contrat 3 — API d'état tracing ↔ UI

- Le tracer lit `source-tracing/config.json` (Contrat 1), relu dès que son mtime change (≤ 1 s). Désactivation → `disconnect{disabled}` + `gap{disabled}` ; réactivation → `connect`.
- Le tracer écrit `<root>/status.json` toutes les 1 s (écriture `.tmp` + rename) :
```json
{"schema_version":1,"host_id":"dry-run","recorder_version":"…","now_us":…,"started_at_us":…,
 "sources":[{"name":"okx-btc-usdt-swap","enabled":true,"connected":true,"last_msg_age_ms":12,"msgs_per_s":34.1,
             "gaps_today":2,"gap_seconds_today":5.3,"file_today_bytes":123456}]}
```
  (`last_msg_age_ms` null si aucun message ; `msgs_per_s` sur les 10 dernières secondes ; `file_today_bytes` = somme des `.part-*` du jour)
- Serveur UI (`ui/`, Node 24, `node:http` seul, AUCUNE dépendance, pas de build), écoute 127.0.0.1:8787 UNIQUEMENT (jamais 0.0.0.0), accès par tunnel SSH :
  - `GET /api/status` → contenu de `status.json` + `"stale": true|false` (stale si now − now_us > 5 s ou fichier absent)
  - `PUT /api/sources/:name` body `{"enabled":bool}` → réécrit atomiquement `config.json` (seul le champ `enabled` change) → 200 `{"name","enabled"}` ; 404 source inconnue ; 400 body invalide
  - `GET /api/days` → `[{"source","date","format","host_id","bytes","sha256","lines","coverage","gaps_total_s"}]` lu des manifestes
  - `GET /` → page statique
- Pas d'authentification (shortcut : toute personne avec un shell sur le serveur peut basculer une source ; acceptable car loopback + tunnel).
- Chemins passés à l'UI par variables d'environnement : `TRACE_CONFIG` (config.json), `TRACE_ROOT` (racine trace/).
- Sur le serveur : config d'état `/var/lib/updown5m/config.json`, traces `/var/lib/updown5m/trace`, code `/opt/updown5m` (lecture seule pour le service). Le tracer se lance par `source-tracing --config <chemin config.json>` (la racine est lue dans le champ `root` de la config). En local, `source-tracing/config.json` reste le défaut.

## Décisions validées
- D1 : rapatriement anticipé AUTORISÉ en lecture seule des sessions des 2 serveurs vers `C:\dev\REA Projects\updown5m-staging\<nœud>\`, sha256 vérifié ; fichiers en cours d'écriture recopiés en 3a.
- D2 : 3 nouveaux adapters `binance-spot`, `binance-perp`, `coinbase` (sans nouvelle crate).
- D3 : PAS d'agrandissement EBS (gratuit) → `pull-traces` quotidien + garde disque < 2 Go dans le tracer.
- D4 : `sha2` déclarée en dépendance directe dans `source-tracing/` (déjà dans `engine/Cargo.lock`).
- D5 : multi-hôtes même jour (anciennes traces) → `<AAAA-MM-JJ>.<host>.old.jsonl.gz` pour les hôtes secondaires.
- D6 : SimVenue réaliste ; non-régression bit à bit sur configs sans TP/SL.
- D7 : `.claude/skills/prompt-master/` copié SANS son `.git` ; vérif `diff -r -x .git`. `read-session` copié identique (inerte, documenté).
- D8 : dublin-live : arrêt propre (sell-only, attendre positions à plat) et confirmation nominative de l'utilisateur sur le wallet AVANT tout nettoyage (phase 3b).
- D9 : `data/wallets` NON copié.

## Exigences ajoutées par l'utilisateur (2026-10-27)
- E1. Au-delà du portage Rust du simulateur maker, ESSAYER DE CORRIGER LES DÉCALAGES AVEC LE RÉEL (validate : volume ×1,044, PnL −4,3 %, 84,1 % d'ordres exacts ; arcsine : volume ×0,958, PnL +8,4 %, 71 %). Méthode imposée :
  1) d'abord le portage bit à bit (non-régression à zéro écart avec les valeurs validées) ;
  2) ensuite chaque correction = une option NOUVELLE et paramétrée du modèle de l'exchange simulé (jamais de la logique d'une stratégie), dont la valeur par défaut reproduit le comportement validé, pour que la non-régression reste vraie ;
  3) chaque correction est mesurée sur `validate` (makers tiers ET arcsine si possible sans `data/wallets` ; sinon makers tiers seulement) avant/après, avec ses chiffres réels, et fondée sur une cause documentée ou mesurée (ex. annulations entre prorata et « toutes derrière », file prise à la décision vs à l'arrivée, ordres de 5 parts mal retrouvés). Aucun réglage aveugle sur le résultat : séparer les fenêtres d'ajustement et de contrôle (split temporel) ;
  4) résultat documenté dans `docs/` (gain, risque de surajustement, valeurs retenues).
- E2. UN fichier de config éditable qui stocke TOUS les paramètres du backtest : `backtest/config/default.json`. Il contient chaque paramètre du marché, des sources (retards de réception…), de l'exchange simulé (tous ceux du Contrat 2, plus toute constante de `sim.mjs` comme `MIN_SIZE`, `HOLD_Q`, `SILENCE`, aller simple…), du capital et des stratégies, avec sa valeur par défaut. Aucune constante cachée dans le code : le code lit le fichier. Un fichier de lot/sweep ne contient que les surcharges (fusion profonde sur `default.json`). Le fichier est documenté (`backtest/README.md` : chaque paramètre, unité, source de la valeur, `[à vérifier]`). Les autres outils gardent chacun leur fichier de config (`source-tracing/config.json`, config de l'engine), eux aussi exhaustifs.

## Décisions utilisateur du 2026-10-28 (priment sur la spécification)
- D10. Les 2 instances (dublin-dry-run, dublin-live) sont coupées entièrement : services arrêtés, puis EC2 arrêtées (stop). Wallets sans capital : aucune précaution particulière.
- D11. Plus d'UI : le Contrat 3 est caduc. `source-tracing/` fournit à la place 3 commandes interactives et visuelles (lancer le tracing en choisissant les sources, désactiver des sources, rapatrier les dernières sessions), répertoriées dans `source-tracing/COMMANDES.md`. Il sera déployé seul sur une nouvelle instance « Up&Down-Tracing » (Dublin).
- D12. Les traces converties sont rangées dans le projet (`source-tracing/trace/`, non commitées).
- D13. Backtest : config globale `backtest/config/default.json` (E2) + un fichier de config par stratégie en TOML, commenté (description de la stratégie et de chaque paramètre). Signal par défaut : binance-spot ms, `sigmaMult` 1,3, configurable ; moyenne Binance+Coinbase en option.

## Règles communes à tous les subagents
- Réponds en français. Tous les .md en français.
- NEVER modifier le dépôt source (lecture et copie seulement ; pas de build, pas de npm install, pas d'`explore.mjs`, rien n'écrit dans son `target/`, `packs/`, `results/`).
- NEVER copier ou commiter `.env`, clés, wallets, secrets ; fournir des `.env.example`. NEVER copier `node_modules/`, `target/`, `graphify-out/`, l'ancienne `ui/`, `src/server/`.
- Ne jamais commiter. Ne toucher à aucun serveur sauf autorisation explicite de ta tâche. NEVER éteindre/terminer une instance EC2. NEVER rien déployer sur dublin-live.
- Ne toucher QUE ton dossier cible. Aucune dépendance ni abstraction au-delà de ce que la tâche exige.
- STOP et rends la main (rapport) avant : d'ajouter une dépendance non listée ici ; de modifier la logique d'une stratégie ; de supprimer quoi que ce soit hors de ton dossier ; après le 2e échec d'une même commande (montre l'erreur exacte).
- Aucun comportement inventé : règle non établie → `[à vérifier]` en doc + paramètre.
- FORMAT DE RETOUR OBLIGATOIRE : fichiers créés ou modifiés ; commandes lancées avec extrait réel de sortie ; écarts par rapport à la tâche ; points `[à vérifier]`.
