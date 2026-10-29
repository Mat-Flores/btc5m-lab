# backtest

Rejoue les stratégies de l'engine sur des données enregistrées, avec un exchange Polymarket simulé. Le code des stratégies est celui de l'engine (`engine/strategy`, `engine/app`), inchangé. Architecture et choix : [`docs/architecture-backtest.md`](../docs/architecture-backtest.md). Modèle de fill et validation : [`docs/simulateur-maker.md`](../docs/simulateur-maker.md).

## Commandes

Rust ne tourne que dans WSL `Ubuntu-24.04`. Depuis Git Bash, préfixer les appels de `MSYS_NO_PATHCONV=1 wsl.exe -d Ubuntu-24.04 -- bash`.

| Commande | Rôle |
|---|---|
| `scripts/test-wsl.sh [engine\|backtest]` | tests de l'engine (workspace) puis du backtest |
| `scripts/validate-wsl.sh` | non-régression du simulateur maker (surcharge `config/nonreg.json` : `cancel_ahead_factor` 1, le comportement validé) : `backtest validate` sur les ordres réels, comparé bit à bit à l'oracle Node (`oracle/validate-export.mjs`) |
| `scripts/batch-wsl.sh <lot>` | non-régression sur un lot de l'ancien dépôt (`x01-retest-exact`, `x04-mix-frozen`…), comparé à l'oracle Node (`oracle/batch-export.mjs`) |
| `backtest run --config config/default.json [--override f.json]... [--batch lot.json] [--windows a,b] [--export x.jsonl]` | un backtest |
| `backtest validate [--override …] [--export x.jsonl]` | rejoue les ordres réels des makers tiers (validation du modèle de fill) |
| `backtest compare <oracle.jsonl> <nous.jsonl>` | comparaison ligne à ligne, bit à bit |
| `node scripts/split.mjs <export validate>` | chiffres de `validate` séparés en fenêtres d'ajustement et de contrôle (E1) |

Les sorties (`out/`) et `target/` sont ignorés par git.

## Deux entrées

- **`input: "pack"`** (surcharge `config/packs.json`) : les packs de l'ancien dépôt `btc5m-lag-sim` (lecture seule), 15→22/10, avec les fills réels (`data/polymarket/fills`). C'est le chemin validé : maker, borne `exact`, 0 écart avec l'ancien simulateur Node.
- **`input: "trace"`** (défaut) : les traces de `source-tracing/trace/`, fusionnées par heure de réception, passées dans l'`Engine` lui-même (mêmes stratégies, même numérotation des ordres, mêmes carnets que le live) et l'exchange simulé `TraceVenue` (`src/run.rs`). Traces `.old` (anciennes sessions converties) : lues dans `legacy`. Traces courantes : chaque trame redécodée par l'adapter de sa source, gamma par `windows::parse`.

Vérifications du mode trace :
- p9 (24 h) et p10 (10 h, ordres réels) : le rejeu des captures donne les mêmes décisions que l'engine en direct (54 ordres, 94 fills et clôtures identiques ; 3 ordres et 6 lignes identiques).
- Ancienne session `2026-10-14T21-54-19` : l'engine sur traces donne les décisions de `replay` à une différence près, expliquée ci-dessous (horloge).

Limites du mode trace (mesurées le 2026-10-28) :
- **Horloge des anciennes sessions** : l'ancien moteur corrigeait ses instants par le décalage d'horloge enregistré (`clock`, jusqu'à −1,3 s sur le portable). L'engine n'a pas cette entrée (horloge synchronisée en live). Sur la session ci-dessus, `replay` donne 3 ordres, l'engine 2 ; `replay` sans les lignes `clock` donne exactement les 2 de l'engine.
- **Borne `exact`** : elle a besoin des trades. Les anciens enregistrements Node n'en ont pas : seuls les asks qui croisent notre bid remplissent. Les traces courantes ont les `last_trade_price` ; leur appariement aux deltas est `[à vérifier]`.
- **Tick du marché** : l'engine suit le tick annoncé par le flux (`tick_size_change`, tick d'un instantané) ; sur les anciennes traces qui ne le gardaient pas, un niveau hors de la grille fait passer la fenêtre au tick de 0,001 (le marché acceptait ce prix ; règle de `sim.mjs`).
- **Couverture** : Binance n'est enregistré que les 14→17/10 et 26→28/10. Le signal par défaut (binance-spot) n'existe pas les autres jours : la stratégie ne cote pas.
- **Résultats officiels** : les traces courantes du 28/10 n'en ont aucun (le traceur cessait d'interroger gamma 120 s après la fin d'une fenêtre, le résultat arrive ≈ 5 min après) : aucun marché n'y est réglé. Traceur corrigé et redéployé le 2026-10-28 (900 s).
- **Gamma interrogé toutes les 10 s** : chaque fenêtre et chaque résultat ne sont donnés qu'une fois à l'engine.
- **Fichiers d'un hôte secondaire** (`<jour>.<hôte>.old.jsonl.gz`, D5) : ignorés, seul le fichier canonique du jour est lu.

## Configuration

`config/default.json` contient **tous** les paramètres avec leur défaut (E2). Une surcharge (`--override`) ne contient que ce qui change ; elle est fusionnée en profondeur (objets clé par clé, tableaux remplacés). Chaque stratégie a son fichier TOML commenté (`config/strategies/maker.toml`, `reference.toml`) : chaque paramètre y est décrit, avec les défauts du code.

| Clé | Défaut | Unité | Rôle, origine |
|---|---|---|---|
| `name` | `default` | — | nom du run |
| `market.venue`, `.asset`, `.horizon_s` | `polymarket`, `btc`, 300 | s | marché ; un horizon autre que celui de la stratégie (`horizonS`) est refusé |
| `period.from`, `.to` | null | `AAAA-MM-JJTHH:MM:SSZ` | bornes du run (null : tout) |
| `period.split` | 2026-10-20T00:00:00Z | date | séparation ajustement / contrôle (E1, `run.mjs` SPLIT) |
| `capital.start_usd` | 1000 | $ | capital de départ (`capital0` du maker, `bankroll` de la référence) |
| `capital.compound` | true | — | mise = f × capital libre ; `false` non implémenté |
| `capital.claim_s` | 0 | s | délai entre la fin d'une fenêtre et la disponibilité du gain `[à vérifier]` |
| `input` | `trace` | — | `trace` ou `pack` |
| `trace.root` | `../../source-tracing/trace` | chemin | racine des traces (relative à `config/`) |
| `trace.formats` | `current`, `old` | — | formats de trace lus |
| `pack.source_root`, `.data_dir`, `.packs_dir` | dépôt `btc5m-lag-sim` | chemins | packs et données de l'ancien dépôt (lecture seule) |
| `pack.mix_latency_ms.perp`, `.coinbase` | 113, 40 | ms | retards du spot composite des packs (`EXPLORATION.md:252`, Coinbase `[non vérifié]`) |
| `sources[].name`, `.adapter` | 7 sources | — | nom de la source dans l'engine, adapter du registre (`polymarket-gamma` : fenêtres) |
| `sources[].input.trace` | nom du dossier | — | dossier de la source sous `trace.root` |
| `sources[].input.recv_lag_ms` | 0 | ms | retard ajouté à l'heure de réception enregistrée |
| `signal.name` | `signal` | — | source synthétique lue par les stratégies |
| `signal.sources` | `binance-btcusdt` | — | sources moyennées (D13 : binance-spot ; Binance + Coinbase en option) |
| `signal.field` | `trade` | — | `mid` ou `trade` (prix du dernier trade) |
| `signal.combine` | `mean` | — | seule règle implémentée |
| `venue.one_way_ms` | 12,2 | ms | aller simple vers le CLOB (moitié du RTT p50 Dublin) `[symétrie à vérifier]` |
| `venue.ack_ms` | 0 | ms | délai de réponse ; ≠ 0 non implémenté |
| `venue.taker_hold.quantiles_s`, `.seed` | `[[0,0.15],…,[1,3.5]]`, 7 | s | retenue des ordres preneurs (sorties maker, FAK du mode trace) ; pas une mesure directe `[à vérifier]` |
| `venue.fill_bound` | `exact` | — | `exact`, `prudent`, `optimistic` ([`simulateur-maker.md`](../docs/simulateur-maker.md) §2) |
| `venue.fill_lookback_ms` | 0 | ms | un fill daté `ts` vient d'un ordre preneur envoyé à `ts −` ceci ; 0 recommandé (`WALLETS.md:123`) |
| `venue.cancel_queue` | `pro_rata` | — | une annulation au niveau amincit la file devant nous au prorata ; `behind` : jamais |
| `venue.cancel_ahead_factor` | 0,5 | 0 à 1 | part du prorata appliquée (E1) : 0,5 est la valeur mesurée la plus proche du réel (volume de contrôle 1,052 → 1,006) ; 1 = comportement de l'ancien simulateur (`config/nonreg.json`) |
| `venue.post_only_cross_check` | `decision` | — | un post-only qui croiserait est refusé à la décision ; `arrival` non implémenté |
| `venue.silence_ms` | 3000 | ms | carnet silencieux plus longtemps : aucun fill |
| `venue.trading_cutoff_ms_after_end` | 2000 | ms | aucun fill au-delà de la fin + ceci `[aucune règle documentée]` |
| `venue.rebates` | true | — | rebate maker = taux de rebate × frais du fill (0,2 × 0,07 × p(1−p) × parts) |
| `venue.trade_fail_rate` | 0 | — | trades `FAILED` ; ≠ 0 non implémenté |
| `venue.consume_own_liquidity` | false | — | nos ordres ne modifient pas le carnet rejoué ; `true` non implémenté |
| `venue.min_size_rule.min_shares` | 5 | parts | taille minimale d'un ordre |
| `variants[]` | [] | — | variantes de l'exchange : `{ "id", "venue.<clé>": valeur }` |
| `engine.book_quiet_ms` | 200 | ms | un changement de carnet non confirmé aussi longtemps est vérifié (`Books::quiet`) |
| `engine.windows_kept_s` | 3600 | s | durée de conservation d'une fenêtre finie |
| `engine.tick_ms` | 50 | ms | pas du temps sans événement (`on_tick`), comme la minuterie du live |
| `strategy.kind`, `.file` | `maker`, `strategies/maker.toml` | — | stratégie du registre et son fichier TOML |
| `strategy.sources` | price `signal`, oracle `chainlink-rtds`, book `polymarket-clob` | — | sources lues, par rôle |
| `strategy.base` | `spot: "spot"`, `sigmaMult: 1.3` | — | surcharge du fichier de stratégie (calibration Binance σ×1,3) |
| `strategy.candidates`, `.grid` | [], {} | — | candidats et grille de sweep (syntaxe de `explore.mjs` / `grid.mjs` : clés pointées, produit cartésien) |
| `out` | `../out` | chemin | dossier des sorties |

Les détails et sources de chaque valeur de `venue` sont dans [`docs/architecture-backtest.md`](../docs/architecture-backtest.md) §5.3.
