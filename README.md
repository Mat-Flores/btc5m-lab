# updown5m-lab

Laboratoire pour les marchés « Bitcoin Up or Down — 5 minutes » de Polymarket. Deux objectifs :

1. **enregistrer en continu** les flux qui comptent (carnets Polymarket, Chainlink, OKX, Binance, Coinbase, fenêtres gamma) ;
2. **backtester de façon réaliste** les stratégies sur ces enregistrements, en ajustant paramètres et sources (seules ou croisées), pour trouver une stratégie performante avant de la passer en réel.

## Les outils et leur enchaînement

```
source-tracing ──traces──▶ backtest ──stratégie retenue──▶ engine (live)
 (enregistreur)            (engine en mode test +          (même code, flux live,
                            exchange simulé)                 exchange réel ou papier)
```

| Dossier | Rôle |
|---|---|
| [`source-tracing/`](source-tracing/README.md) | enregistreur continu (Rust), format de trace, déploiement sur l'instance « Up&Down-Tracing » (Dublin), conversion des anciennes sessions |
| [`backtest/`](backtest/README.md) | rejoue les traces dans le code même de l'engine, avec un exchange Polymarket simulé ; configuration JSON + un fichier TOML commenté par stratégie ; sweeps sans recompiler |
| [`engine/`](engine/README.md) | moteur de trading (Rust) : sources → stratégies → ordres ; live, papier et test |
| [`docs/`](docs/README.md) | synthèse de la recherche : marché, sources, simulateur, stratégies, décisions |

Le dossier `copy-trading/` est un projet isolé, géré à part : il n'est pas décrit ici.

## Démarrer

- Rust ne tourne que dans WSL `Ubuntu-24.04` (`~/.cargo/bin/cargo`, `CARGO_TARGET_DIR` sous `$HOME`) ; Node 24 sous Windows pour les outils `.mjs`.
- Tests : `backtest/scripts/test-wsl.sh` (engine puis backtest).
- Récupérer les traces du serveur : `source-tracing/deploy/pull` ([`COMMANDES.md`](source-tracing/COMMANDES.md)).
- Lancer un backtest : `backtest run --config config/default.json --override <surcharge.json>` ([`backtest/README.md`](backtest/README.md)).

Le dépôt d'origine `btc5m-lag-sim` reste en lecture seule : le backtest y lit encore les packs et les fills du 15 au 22/10.
