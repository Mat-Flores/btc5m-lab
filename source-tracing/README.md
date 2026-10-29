# source-tracing

Enregistreur continu des flux publics utiles aux marchés « BTC Up or Down 5 min » de Polymarket. Il tourne sur l'instance EC2 « Up&Down-Tracing » (Dublin) ; le backtest relit ses traces. Binaire Rust autonome, qui réutilise les adapters de l'engine (`../engine`, par chemins).

- Déploiement et pilotage (start, stop, pull) : [`COMMANDES.md`](COMMANDES.md), scripts dans `deploy/`.
- Conversion des anciennes sessions en traces `.old` : [`convert/MAPPING.md`](convert/MAPPING.md).

## Commandes

| Commande | Rôle |
|---|---|
| `source-tracing [--config config.json]` | enregistre (défaut : `config.json` du dossier courant ; modèle : `config.example.json`) |
| `cargo test` (dans WSL `Ubuntu-24.04`, `CARGO_TARGET_DIR=$HOME/st-target`) | tests de l'enregistreur |
| `node --test convert/test/` | tests du convertisseur |
| `node convert/convert.mjs --out trace <sessions>…` | conversion des anciennes sessions |

## Configuration (`config.example.json`)

| Clé | Rôle |
|---|---|
| `host_id` | nom de la machine dans les manifestes |
| `root` | racine des traces (`trace`, ignorée par git) |
| `market` | marché suivi (`venue`, `asset`, `horizon_s`) : slugs des fenêtres et tokens à abonner |
| `gamma_poll_ms`, `windows_ahead`, `gamma_prev_grace_s` | interrogation de gamma : période, fenêtres à l'avance, fenêtre précédente gardée |
| `clob_time_poll_ms` | période de `GET /time` du CLOB |
| `disk_min_free_gb`, `disk_resume_free_gb` | pause de l'écriture sous ce disque libre, reprise au-dessus |
| `sources[]` | `name` (dossier de trace), `adapter`, `topics`, `connections`, `enabled` |

## Format de trace (Contrat 1)

Un fichier par source et par jour UTC : `trace/<source>/<AAAA>/<MM>/<AAAA-MM-JJ>.jsonl.gz` et son manifeste `<AAAA-MM-JJ>.manifest.json`. Pendant la journée, l'enregistreur écrit des morceaux ; après minuit, il les **scelle** : réparation, concaténation, sha256, manifeste, renommage. Le manifeste écrit en dernier valide le jour. Les anciennes sessions converties portent le suffixe `.old` (et `.<hôte>.old` pour un hôte secondaire du même jour).

Chaque ligne est un objet JSON, champs dans cet ordre :

| Champ | Valeur |
|---|---|
| `seq` | numéro de ligne dans le fichier |
| `t_recv` | heure de réception, µs Unix (horloge locale synchronisée) |
| `t_src` | heure de la source, µs, quand elle en donne une ; sinon `null` |
| `seq_src` | numéro de séquence de la source, sinon `null` |
| `conn` | numéro de connexion (une source peut en ouvrir plusieurs) |
| `kind` | type de ligne (ci-dessous) |
| `raw` | la trame reçue, texte exact ; `null` pour les lignes de cycle de vie |

| `kind` | Sens |
|---|---|
| `header` | début de fichier (schéma `updown5m-trace` v1, source, date) |
| `connect`, `sent`, `disconnect` | connexion ouverte (`url`), abonnement écrit (`raw`), connexion fermée (`reason`, `detail`) |
| `quote`, `snapshot`, `delta`, `tick`, `trade`, `oracle`, `twap` | une trame reçue, nommée par ce que l'adapter en décode ; `n_ev` = nombre d'événements |
| `msg` | trame sans événement, ou mêlant plusieurs types |
| `http` | réponse HTTP (gamma, `GET /time`) : `method`, `url`, `status`, `t_sent`, corps dans `raw` |
| `decode_error` | trame illisible (`error`), gardée telle quelle |
| `gap` | trou détecté (`reason`, bornes) |
| `end` | fin de fichier (`reason`) |

La valeur est toujours dans `raw` : le backtest redécode chaque trame avec l'adapter de sa source, au `t_recv` enregistré. Dans les `.old`, `raw` vaut `null` et la valeur est dans `legacy` (la ligne de l'ancien format).

Le backtest ne lit que les jours scellés (`<jour>.jsonl.gz`) : le jour en cours n'est lisible qu'après son scellement.
