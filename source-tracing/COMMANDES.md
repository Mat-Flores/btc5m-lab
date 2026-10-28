# Commandes de déploiement du tracing

Trois commandes bash (Git Bash sous Windows) pilotent l'instance EC2 « Up&Down-Tracing » (Dublin). Elles sont dans `source-tracing/deploy/` : `start`, `stop`, `pull`. Chacune affiche un récapitulatif, demande confirmation (`--yes` pour passer), puis vérifie le résultat.

## Prérequis

- Git Bash (fournit bash, ssh, tar, sed, awk, sha256sum) ; rien d'autre à installer côté client.
- Une instance Ubuntu avec le port 22 ouvert depuis votre IP, un utilisateur `ubuntu` disposant de `sudo` sans mot de passe, et la clé `.pem`.
- Internet côté serveur pour la première installation (apt : gzip, chrony).
- WSL avec la distribution `Ubuntu-24.04` et Rust (`~/.cargo/bin/cargo`) : `start` compile le tracer en release dans WSL (`CARGO_TARGET_DIR=$HOME/st-target`) puis n'envoie que le binaire. Le dossier `engine/` doit rester à côté de `source-tracing/` (dépendances par chemins), mais il n'est plus envoyé au serveur. Le serveur n'a ni Rust ni compilateur.

## Créer `source-tracing/deploy/.env`

```
cp source-tracing/deploy/.env.example source-tracing/deploy/.env
```

Contenu (`.env` est ignoré par git, ne jamais le commiter) :

```
TRACING_SSH_DEST=ubuntu@<IP>
TRACING_SSH_KEY=C:/Users/<vous>/.ssh/<cle>.pem
TRACING_HOST_ID=tracing        # optionnel
```

Le fichier est lu ligne à ligne, jamais exécuté : une `&` dans le nom de la clé (ex. `Up&Down.pem`) est donc sans danger dans le `.env` ; utiliser des `/` dans le chemin. Le `~` est accepté. Les scripts refusent `dublin-live` (nom ou IP).

## `start` : installer / mettre à jour et lancer

```
bash source-tracing/deploy/start [--sources a,b] [--yes] [--config-only] [--interval 15]
```

1. Se connecte, affiche l'état du service et le disque libre avec les jours restants avant la garde du tracer (refus sous 2 Go, avertissement sous 10 Go).
2. Menu à cases à cocher (flèches, espace, `a` tout, `n` rien, entrée) des sources de `config.example.json` ; préselection = état actuel du serveur.
3. Récapitulatif + confirmation.
4. Installation du serveur si besoin (`setup.sh` : apt gzip + chrony, utilisateur `tgap`, unit systemd ; ni Rust, ni compilateur, ni swap). Compilation du tracer dans WSL (barre = crates compilés). Envoi du seul binaire dans un fichier temporaire (barre par Mo), contrôle sha256 distant, puis `install` + `mv` (remplacement atomique de `/opt/updown5m/bin/source-tracing`). Écriture de `/var/lib/updown5m/config.json` (réécrite depuis `config.example.json`, `host_id` et `root` adaptés), démarrage ou redémarrage.
5. Vérification : service actif, puis taille des fichiers du jour (`.part-*`) mesurée à 2 instants (`--interval`, 15 s par défaut) ; échec si aucun ne grossit.

Activation des sources : le tracer relit `config.json` dès que son mtime change (≤ 1 s) et active/désactive les sources sans redémarrage (`src/run.rs`). En revanche un changement d'adapter, de topics ou d'un réglage global (`host_id`, périodes…) n'est pas pris à chaud : il exige un redémarrage. Donc :

- `start` normal : nouveau binaire, donc redémarrage ;
- `start --config-only` : ne touche pas au code ; si le service tourne et que seuls les `enabled` changent, relecture à chaud sans redémarrage ; sinon redémarrage.

Exemple de sortie (test local, sans serveur) :

```
[3/4] verification
  OK    service actif
  source                       mesure 1     mesure 2      delta
  okx-btc-usdt-swap                4200         7000      +2800
  coinbase-btc-usd                 4200         7000      +2800
  OK    les fichiers du jour grossissent (5.5 Ko en 2s)
```

Pourquoi compiler dans WSL : l'instance n'a que ~1,9 Gio de RAM, une compilation LTO y est risquée. Le binaire WSL (glibc 2.39, dépend seulement de libc et libgcc_s) tourne sur Ubuntu 26.04 (glibc 2.43) ; même architecture x86_64 que le serveur. Si le serveur change d'architecture (arm64), ce choix est à revoir `[à vérifier]`. Le swap n'est plus créé : il ne servait qu'à la compilation. Le binaire compilé reste dans `source-tracing/target/wsl-bin/` (ignoré par git).

## `stop` : désactiver des sources ou tout arrêter

```
bash source-tracing/deploy/stop [--sources a,b] [--all] [--yes]
```

Affiche les sources actives du serveur, laisse cocher celles à désactiver.

- Sélection partielle : la config est réécrite atomiquement, le tracer la relit à chaud (`disconnect{disabled}` + `gap{disabled}`), le service reste actif. Vérifié par relecture de la config puis par `status.json` du tracer (`"enabled":false`, attente jusqu'à 8 s).
- Tout cocher (ou `--all`) : `systemctl stop` (fermeture propre), vérification que le service est inactif et que les fichiers du jour ne grossissent plus. La config n'est pas modifiée.

## `pull` : rapatrier les traces

```
bash source-tracing/deploy/pull [--dry-run] [--keep] [--yes] [--local <dossier>]
```

Cible par défaut : `source-tracing/trace/` (ignoré par git). Ne rapatrie que les jours validés (manifeste présent), jamais le jour UTC en cours ni un `.part-*`. Pour chaque fichier : téléchargement dans un `.tmp`, contrôle de la taille et du sha256 du manifeste, renommage, puis suppression côté serveur (données, puis manifeste) seulement après vérification ; `--keep` désactive la suppression, `--dry-run` n'affiche que le plan. Idempotent : un fichier déjà présent et vérifié n'est pas retéléchargé, un `.tmp` orphelin est refait. Deux barres : fichier courant et total.

Disque de 30 Go (~26 Go libres) et ~4 Go/jour de Polymarket : le tracer s'arrête de lui-même sous 2 Go libres (`disk_min_free_gb`) et reprend à 3 Go, soit environ 6 jours d'autonomie. **Lancer `pull` au moins tous les 5 jours.**

## En cas d'échec

| Symptôme | Cause probable / action |
|---|---|
| `fichier .env absent`, `cle SSH introuvable` | Créer `.env` / corriger le chemin (utiliser des `/`). |
| `connexion SSH impossible` | IP changée (IP publique non élastique après arrêt), groupe de sécurité sans le port 22 pour votre IP, mauvaise clé. |
| `setup.sh a échoué` | Relancer `start` (idempotent) ; lire la sortie apt. |
| `compilation WSL échouée` | Journal indiqué ; vérifier `wsl.exe -l`, la distribution `Ubuntu-24.04` (variable `WSL_DISTRO`) et `~/.cargo/bin/cargo` dans WSL. |
| `sha256 distant différent` | Envoi corrompu : rien n'a été remplacé, relancer `start`. |
| `le service n'est pas actif` | Les 15 dernières lignes du journal sont affichées ; `config ignorée : …` indique une config invalide. |
| `aucune croissance des fichiers` | Relancer avec `--interval 60` (gzip bufferise) ; sinon regarder `journalctl -u updown5m-tracing` sur le serveur. |
| `moins de 2 Go libres` | Lancer `pull`, puis `start`. |
| `pull` : `sha256 différent du manifeste` | Rien n'est supprimé côté serveur ; relancer. Persistant : fichier corrompu côté serveur à examiner. |
| `un autre pull tourne` | Supprimer le dossier `.pull.lock` de la cible si aucun pull ne tourne. |

## Tests (sans serveur ni SSH)

```
bash source-tracing/deploy/tests/test-deploy.sh
```

`PT_TEST_NOSSH=1` exécute les commandes « serveur » en local dans un dossier temporaire, avec un faux `systemctl` et un faux tracer et un binaire simulé (`ST_TEST_BIN`, remplace la compilation WSL) ; ils ne valident pas le vrai tracer ni le vrai déploiement.
