# engine

Le moteur de trading en Rust : sources observées → stratégies → décisions d'ordre. Le même cœur tourne en live (ce dossier, binaire `engine`) et en test (`../backtest`, qui substitue l'horloge, les flux et l'exchange). Conception : [`docs/engine-conception.md`](../docs/engine-conception.md), [`docs/architecture-backtest.md`](../docs/architecture-backtest.md).

## Crates

| Dossier | Rôle |
|---|---|
| `core/` | horloge, événements, carnets, contrat des flux (`feed::Adapter`, runner, supervision), ordres, compte, garde-fous, registre de faits (ledger) |
| `adapters/{polymarket,chainlink,okx,binance,coinbase}/` | un adapter par source : URL, abonnements, décodage des trames ; Polymarket aussi : fenêtres (gamma), signature, envoi d'ordres, appariement simulé du mode papier |
| `strategy/` | stratégies pures : `reference` (preneuse, ancienne app), `maker` (portée du simulateur `sim.mjs`), modèle de juste valeur |
| `app/` | `Engine` (synchrone, ne lit aucune horloge), registre des adapters et stratégies, coquille live (`main.rs`), modes papier et réel |
| `replay/` | rejoue une ancienne session (format de session JSONL) dans la stratégie de référence : parité de décisions (hors workspace) |
| `redeem/` | réclamation automatique des marchés gagnés (Node) |
| `bench/`, `fixtures/` | mesure de pertes de carnet ; trames, sessions et cas de test |

Ajouter une source = un adapter et une ligne dans `app/src/registry.rs` ; ajouter une stratégie = une ligne dans `strategy`.

## Commandes (WSL `Ubuntu-24.04`)

```
CARGO_TARGET_DIR=$HOME/eng-target ~/.cargo/bin/cargo test --workspace     # tests
CARGO_TARGET_DIR=$HOME/eng-target ~/.cargo/bin/cargo build --release       # binaire engine
engine <config.json>                                                         # live ou papier
zcat <session>.jsonl.gz | replay '{}' decisions                              # parité de la référence
```

`config.example.json` liste chaque clé de configuration avec sa valeur par défaut (marché BTC 5 min). Le mode réel n'envoie que des achats FAK : ni post-only ni annulation tant que cette décision ne change pas ([`docs/securite-et-mode-reel.md`](../docs/securite-et-mode-reel.md)).

## Vérifications

- Parité de la stratégie de référence : `replay` donne les décisions de l'ancien moteur (0 écart sur 22 sessions) et celles de l'engine en direct (captures p9 et p10 rejouées : ordres, fills et clôtures identiques, 2026-10-28).
- Stratégie maker : non-régression à 0 écart avec le simulateur Node, voir [`../backtest/README.md`](../backtest/README.md).
