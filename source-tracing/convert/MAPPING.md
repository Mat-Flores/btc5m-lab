# Convertisseur des anciennes sessions : mapping ancien → nouveau

Outil : `source-tracing/convert/convert.mjs` (Node 24, aucune dépendance). Contrat : « Contrat 1 — Format de trace v1 » (voir `PROMPT-MIGRATION.md`, section ANCIENNES TRACES).

```
node source-tracing/convert/convert.mjs --out <racine trace> [--dry-run] <dossier ou fichier de sessions>...
node --test source-tracing/convert/test/
```

Les dossiers donnés sont lus à plat (`*.jsonl.gz` seulement : ni `*.records.jsonl`, ni `*.dryrun.jsonl`, ni `*.pack/`). Les sessions sont lues en flux ; rien n'est gardé en mémoire hors petits compteurs.

## Sorties

`<out>/<source>/<AAAA>/<MM>/<AAAA-MM-JJ>[.<hôte>].old.jsonl.gz` + `<…>.old.manifest.json` (gzip niveau 6, **un membre gzip par session et par fichier**). Le jour est le jour UTC de `t_recv`. Écriture dans `….old.jsonl.gz.tmp` puis renommage ; le manifeste est écrit en dernier (`.tmp` puis renommage = point de validation). Rapport : `<out>/../convert-report.json` si `--out` s'appelle `trace`, sinon `<out>/convert-report.json` (rien n'est écrit avec `--dry-run`).

`--dry-run` lit réellement les entrées (comptage exact, pas d'estimation) mais n'écrit rien ; il affiche les fichiers qui seraient produits et leurs nombres de lignes.

## Enveloppe et champs

Chaque ligne : `{"seq","t_recv","t_src","seq_src","conn","kind","raw", …champs propres}` dans cet ordre.

| Champ | Valeur |
|---|---|
| `seq` | numéro de ligne (base 0) dans la session d'origine ; l'en-tête est la ligne 0. Les lignes `gap` ajoutées par le convertisseur n'ont pas de ligne d'origine : `seq:null` [à vérifier : le validateur doit accepter `null`] |
| `t_recv` | `round(t × 1000)` en µs (`t` ancien = ms flottantes, horloge locale) |
| `t_src` | voir tableau ; `null` si absent ou égal à 0 |
| `seq_src` | toujours `null` (l'ancien format n'enregistre aucune séquence source) |
| `conn` | `0` (enregistreur Node) ; `null` pour une capture de l'engine Rust (en-tête avec champ `engine`) et pour toute ligne `trade` |
| `raw` | toujours `null` (la trame d'origine n'a pas été conservée) |
| `legacy` | la ligne d'origine, texte copié tel quel (objet JSON) : **toutes les valeurs (prix, tailles, carnets…) ne sont que là** |

## Table ancien → nouveau

| Ancien | Source | `kind` | `t_src` | Champs ajoutés |
|---|---|---|---|---|
| `session` (en-tête) | toutes les sources du fichier | `recorder_start` | `null` | `legacy` = en-tête ; `inferred:"id"` (`t_recv` = `Date.parse(id)` à la seconde) ou `inferred:"rotate"` (fichier d'un jour suivant : `t_recv` = 00:00:00 UTC) |
| `px` binance / binanceFut / coinbase / bybit / okx / kraken / bitstamp | `binance-btcusdt` / `binance-perp-btcusdt` / `coinbase-btc-usd` / `bybit-btcusdt-linear` / `okx-btc-usdt-swap` / `kraken-btc-usd` / `bitstamp-btcusd` | `mid` | `round(et × 1000)` ou `null` (binance spot n'a pas d'`et`) | `legacy` |
| `cl` | `chainlink-rtds` | `oracle` | `ts × 1000` | `legacy` |
| `book` | `polymarket-clob` | `snapshot` | `ts × 1000` ; `ts==0` → `null` | `legacy` ; `derived:true` si `ts` absent |
| `delta` | `polymarket-clob` | `delta` | idem | idem |
| `trade` (capture engine) | `polymarket-clob` | `trade` | `ts × 1000` | `legacy` |
| `meta` / `res` / `ptb` | `polymarket-gamma` | `window` / `resolution` / `price_to_beat` | `null` | `legacy` |
| `rtt` | `polymarket-clob-time` | `rtt` | `null` | `legacy` |
| `clock` | **copiée dans le fichier de chaque source** de la session et du jour | `clock` | `null` (`t_recv` non corrigé) | `offset_ms` = `offset` (ms, local − échange) ; `legacy` |
| *(trou entre deux sessions du même hôte et du même jour)* | fichier concerné | `gap` | `null` | `reason:"no_recording"`, `from`, `to` (µs), `inferred:true` ; `t_recv` = `to` |

Non convertis (comptés et signalés dans le rapport) : `cfg`, `lane`, `real`. Ignorés et comptés : `px` d'une source inconnue (ex. `fastest`), `k` inconnu, en-tête `session` répété. Lignes illisibles (JSON invalide, `k` ou `t` absent/non numérique) : comptées, numéros des 10 premières donnés dans le rapport, jamais devinées.

## Ce qui est absent, et pourquoi (jamais inventé)

- `raw` : la trame d'origine n'a pas été conservée → `null`. Les valeurs ne sont lisibles que dans `legacy` (le backtest mappe les `.old` directement en `Event`, Contrat 2).
- `seq_src`, `n_ev`, `dup`, `topics`, `url`… : absents de l'ancien format. `n_ev` en particulier n'est pas déduit du nombre de changements d'un `delta`.
- `t_src` des `mid` de binance spot ; des `book`/`delta` à `ts==0` ou sans `ts` (`derived:true` : carnet synthétisé, pas de `ts` source).
- `recorder_version` des runs : `null` (l'ancien enregistreur n'a pas de version ; `engineVersion` de l'en-tête est un autre objet, il reste dans `legacy`).
- Aucun `connect`/`disconnect`/`header`/`end`/`decode_error` : l'ancien format ne les enregistre pas. `disconnects` et `decode_errors` du manifeste valent donc 0 (aucune ligne de ce type dans le fichier).
- Les `clock` ne servent pas à corriger `t_recv`.

## Découpage, hôtes, D5, doublons

- **Hôte** : `nodeName` de l'en-tête, sinon hostname via la table `ip-172-31-19-166`→`dublin`, `ip-172-31-28-37`→`dry-run`, `ip-172-31-28-36`→`live`, `mf-laptop`→`local`. Hôte inconnu : hostname nettoyé, avertissement dans le rapport.
- **Sessions d'un même hôte et du même jour** : concaténées dans l'ordre (début de session `Date.parse(id)`, puis id), chacune précédée de `recorder_start` ; si la session suivante commence après la fin de la précédente (dans ce fichier), une ligne `gap{no_recording, inferred:true}` les sépare. Chevauchement : pas de `gap`.
- **Session à cheval sur minuit** : les lignes vont dans le fichier du jour de leur `t_recv` ; chaque fichier de jour suivant commence par un `recorder_start` `inferred:"rotate"` à 00:00:00 UTC.
- **D5 (plusieurs hôtes le même jour)** : pour un couple (source, jour), l'hôte qui a le plus de lignes dans ce fichier garde le nom canonique `<jour>.old.*` (égalité : ordre alphabétique du nom d'hôte) ; les autres : `<jour>.<hôte>.old.*`. Ce choix se fait à la fin du traitement : les fichiers sont écrits sous `<jour>.<hôte>.old.jsonl.gz.tmp` puis renommés. Un hôte seul sur un couple (source, jour) garde toujours le nom canonique. Chaque cas D5 est listé dans les avertissements du rapport.
- **Doublons** : une même `id` de session présente à plusieurs endroits (local, copies de serveurs) n'est convertie qu'une fois, depuis le fichier **le plus gros** (le plus complet), à égalité le premier par chemin. Chaque doublon écarté est listé dans le rapport (`identical: true` si octets identiques, vérifié par sha256 quand les tailles sont égales ; `null` si les tailles diffèrent).
- **Exclusions** : `2026-10-16T09-00-00_sim` (synthétique) et `2026-10-15T20-37-05` (vide, contrat) ; tout fichier de 0 octet ou sans en-tête `session` lisible est écarté et signalé.
- **Fin tronquée** (enregistrement interrompu : pas de trailer gzip) : conversion jusqu'à la dernière ligne complète ; une dernière ligne sans `\n` est abandonnée (`dropped_partial_tail`). Les manifestes concernés ont `legacy.truncated_input:true` et `runs[].clean_end:false`. La troncature est détectée en comparant le trailer du fichier (CRC32 et taille) au flux décompressé ; un fichier gzip à plusieurs membres serait signalé tronqué à tort (non rencontré).

## Manifeste (`.old.manifest.json`)

Champs du Contrat 1 : `format:"old"`, `host_id`, `bytes`/`sha256` du fichier de données, `lines`, `lines_by_kind` (y compris `recorder_start`, `gap`, `clock`), `first_t_recv`/`last_t_recv` (toutes lignes du fichier), `runs` (un par session : `run` = id de session, `recorder_version:null`, `clean_end` = fin de flux propre), `hours` (24 entrées), `covered_s`/`coverage`, `gaps`, `gaps_total_s`, `disconnects:0`, `decode_errors:0`, `silence_threshold_ms`, `legacy:{sessions:[ids],truncated_input}`, `validated_at` (µs ; réutilisé tel quel si le sha256 du fichier est inchangé, pour que la relance produise le même manifeste), `validator_version:"convert-1"`.

Trous (`origin:"inferred"` pour tous : rien n'est « enregistré » dans l'ancien format) :
- `silence` : écart entre deux lignes de marché consécutives d'une même session et d'un même fichier, au-delà du seuil de la source ; seuils : **60 s** pour `mid` (toutes les sources `px`), **10 s** pour `oracle` (Chainlink), **3 s** pour `snapshot`/`delta`/`trade` du CLOB Polymarket [à vérifier : seuil Polymarket appliqué à un carnet qui peut légitimement être calme], **aucun** pour `polymarket-gamma` et `polymarket-clob-time` (HTTP ; `silence_threshold_ms:null`). Un silence à cheval sur une frontière de session ou de jour n'est pas détecté comme tel (il apparaît comme trou `no_recording`).
- `no_recording` : entre deux sessions du même fichier (ligne `gap` aussi présente dans les données) ; de 00:00 à la première ligne ; de la dernière ligne à 24:00 (tout écart > 0, même minuscule).
- `covered_s = 86400 − union des trous` ; `coverage = covered_s / 86400` ; `hours[h].covered_s` = 3600 − recouvrement des trous avec l'heure.

## Rapport de conversion (`convert-report.json`)

Pour chaque session d'entrée : `lines_read`, `read_by_kind` (anciens `k`), `converted_by_kind`, `written_by_kind` (nouveaux `kind` de données), `not_converted`, `ignored`, `unreadable{count, lines}`, `clock{read, copies_written}`, `lifecycle_written{recorder_start, gap}`, `truncated_input`, `dropped_partial_tail`, `files[]`, `duplicates`, `check{mapped_in, data_out, ok}`. Totaux : `conservation_ok` vérifie (1) lignes de marché lues convertibles = lignes de données écrites, (2) lignes de tous les fichiers = données + `recorder_start` + `gap` + copies de `clock`. Le code de sortie est 1 si la conservation échoue.

Calcul de conservation à la main : entrée = 1 en-tête + lignes `cfg`/`lane`/`real` + `clock` + lignes de marché ; sortie = lignes de marché + `recorder_start` (un par fichier) + `gap` + `clock` × (nombre de fichiers source du jour).

## Limites connues

- Lecture de bout en bout : `--dry-run` coûte autant de temps de lecture que la conversion.
- Pas de nettoyage des anciens fichiers `.old` si l'attribution D5 change entre deux exécutions sur des entrées différentes.
- `conn` : toutes les sessions présentes à ce jour sont enregistrées par l'enregistreur Node (`conn:0`), y compris celles produites via l'engine Rust par `runner.js` (leurs en-têtes n'ont pas de champ `engine`) [à vérifier].
