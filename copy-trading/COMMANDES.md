# Commandes de copy-trading

Trois commandes bash (Git Bash sous Windows, lancées depuis PowerShell ou Git Bash) pilotent le bot sur l'instance définie dans `copy-trading/deploy/.env` : `start`, `stop`, `report`. `start` et `stop` affichent l'état, demandent confirmation (`--yes` pour passer), agissent, puis vérifient le résultat.

```
bash copy-trading/deploy/start            # menu paper / live
bash copy-trading/deploy/stop
bash copy-trading/deploy/report
```

## Ce que fait le bot

Il suit UN wallet public Polymarket et recopie ses trades :

- **Détection** : logs `OrderFilled` de la chaîne Polygon (websocket gratuit publicnode), filtrés sur le wallet. C'est la source la plus rapide qui a été mesurée : 1,77 s après le match en médiane, depuis Dublin. Secours : data-api `/activity` toutes les 5 s (19 s de retard en médiane). Elle rattrape un trade manqué et contrôle chaque fill vu sur la chaîne (lignes `check` du journal).
- **Mise** : p = sa mise / la valeur totale de son portefeuille (positions au prix du marché + cash pUSD). Les paliers `tiers` de la config donnent m, la part de MON portefeuille. Par défaut :

  | sa mise p | ma mise m |
  | --- | --- |
  | < 0,001 % | rien (poussière) |
  | ≥ 0,001 % | 3 % |
  | ≥ 0,1 % | 5 % |
  | ≥ 1 % | 10 % |

  La mise est relevée au minimum d'ordre Polymarket (`min_order_size` du carnet, en général 5 parts) si mon cash le permet. Elle est plafonnée par mon cash et par `max_order_usd`.
- **Une seule entrée par token** : ses renforts sur un token où je suis déjà entré sont ignorés.
- **Ventes** : s'il vend X % de sa position, je vends X % de la mienne. Sous le minimum d'ordre, aucun ordre n'est envoyé et une ligne est écrite dans le journal.
- **Prix** : le prix « demandé » est le meilleur prix du carnet au moment de l'observation. Mon prix limite ne s'en écarte jamais de plus de `max_slippage` (10 %). Sans liquidité dans cette borne, rien n'est exécuté.
- **Trois comptes de PnL** :
  - *observé* : mes tailles à ses prix ;
  - *demandé* : mes tailles au meilleur prix observé ;
  - *réel* : mes fills en live, ou le remplissage calculé sur le vrai carnet en paper.

  La résolution des marchés est comptée dans les trois.
- **Latences** par trade : match CLOB de son ordre → observation → décision → envoi → accusé du CLOB → fill. Un segment non mesuré s'affiche `NULL` avec `N = 0`, jamais estimé.
  - Le match est daté par le websocket market de Polymarket. Le premier trade sur un token encore jamais suivi n'est pas daté.
  - En paper, les segments envoi, accusé et fill n'existent pas.

## Prérequis

- Git Bash (bash, ssh, sed, awk, sha256sum).
- WSL `Ubuntu-24.04` avec Rust (`~/.cargo/bin/cargo`) et la compilation croisée arm64, installée une fois :
  ```
  wsl -d Ubuntu-24.04 -- ~/.cargo/bin/rustup target add aarch64-unknown-linux-gnu
  wsl -d Ubuntu-24.04 -- sudo apt-get install -y gcc-aarch64-linux-gnu
  ```
  `start` compile dans WSL (`CARGO_TARGET_DIR=$HOME/ct-target`, script `deploy/build-wsl.sh`) et n'envoie que le binaire. `engine/` doit rester à côté de `copy-trading/` (dépendances par chemins). Le serveur n'a ni Rust ni compilateur.
- Une instance Ubuntu 24.04 (arm64 ; x86_64 demanderait de changer la cible dans `build-wsl.sh`), le port 22 ouvert depuis votre IP, l'utilisateur `ubuntu` avec `sudo` sans mot de passe, la clé `.pem`.
- Le réseau du serveur doit joindre Polygon : un réseau qui filtre les RPC crypto (cas du réseau local au 2026-10-28) coupe la détection et la valeur de son portefeuille.

## `deploy/.env`

```
cp copy-trading/deploy/.env.example copy-trading/deploy/.env
```

```
COPY_SSH_DEST=ubuntu@<IP>
COPY_SSH_KEY=~/.ssh/<cle>.pem
COPY_WALLET=0xf0318c32136c2db7fec88b84869aee6a1106c80c   # wallet suivi, modifiable ici
```

Le fichier est lu ligne à ligne, jamais exécuté. Il est ignoré par git. Les scripts refusent `dublin-live` et `Up&Down-Tracing` (nom ou IP), côté client comme côté serveur. Pour suivre un autre wallet : changer `COPY_WALLET`, puis relancer `start`. Les autres réglages (paliers, capital paper, ordre max, écart de prix, RPC) sont dans `copy-trading/config.example.json`, recopié dans la config du serveur à chaque `start`.

## `start` : choisir le mode, déployer, lancer, vérifier

```
bash copy-trading/deploy/start [--mode paper|live] [--yes]
```

1. Connexion. `start` affiche l'état (service paper, run live, verrou live), puis le menu : `1) paper` (par défaut) ou `2) live`.
2. **Live seulement** :
   - Si le run live précédent s'est arrêté sans arrêt propre (crash, redémarrage du serveur), le verrou est encore là. `start` montre ce que dit le journal et demande confirmation avant de relancer.
   - Si le serveur n'a pas de wallet chiffré, `start` le crée :
     - il demande l'adresse du Deposit Wallet, la clé privée et la phrase secrète (deux fois), toutes en saisie masquée ;
     - la clé est chiffrée **dans WSL**, par la sous-commande `copy-trading encrypt` (format v1 : scrypt + AES-256-GCM, relu par `decrypt_v1`). Elle passe par stdin, jamais en argument ni en variable d'environnement ;
     - seule la ligne `<adresse>:v1.…` est envoyée au serveur, dans `/var/lib/copy-trading/wallet`, en chmod 600.
   - Sinon, `start` demande la phrase secrète, à **chaque** start live.
3. Récapitulatif (serveur, mode, wallet suivi, paliers, capital, plafonds), puis confirmation.
4. Installation au premier passage (`setup.sh`) : chrony sur le service de temps AWS, utilisateur `copyt`, `/opt/copy-trading` et `/var/lib/copy-trading` (chmod 700), unité systemd du mode paper.
5. Compilation arm64 dans WSL, puis envoi du binaire : fichier temporaire, contrôle sha256 distant, remplacement atomique.
6. Écriture de la config, relue pour vérification. Arrêt du run en cours, quel qu'il soit.
7. Démarrage :
   - **paper** : service systemd `copy-trading`, relancé automatiquement après un crash ou un redémarrage ;
   - **live** : process lancé par SSH, **hors systemd**. La phrase secrète passe par le stdin de SSH. `start` attend que le bot écrive `ARMED`. La clé reste en mémoire seulement. Après un crash ou un redémarrage, rien ne le relance, et un relancement manuel est refusé tant que le verrou est là : le journal l'indique par une ligne `halt`.
8. Vérification : nouvelle ligne `start` dans le journal, avec le bon wallet et le bon mode, la valeur de son portefeuille, et les flux chaîne et market valides.

## `stop`

```
bash copy-trading/deploy/stop [--yes]
```

Arrête le service paper (et le désactive) ou envoie SIGTERM au run live. Vérifie ensuite qu'il ne reste aucun process, que le journal a reçu sa ligne `stop`, et que le verrou live a été retiré par le bot.

## `report`

```
bash copy-trading/deploy/report
```

Tableau de bord du journal serveur (`/var/lib/copy-trading/journal.jsonl`) : cadre à largeur fixe, lignes TRADES/SOURCE/ordres, tableaux COMPTES et LATENCE avec tendances PnL et barres de distribution, comparaison « COPIES · LUI vs MOI », puis état du service. La comparaison montre les six copies les plus récentes, le prix et la mise effectivement remplis, l'écart de prix et la marge à l'achat ; les valeurs absentes restent `NULL`. Les caractères de contrôle issus du journal sont neutralisés et les valeurs trop longues tronquées pour préserver l'alignement. Les couleurs sont désactivées si la sortie n'est pas un terminal ou si `NO_COLOR` est défini.

- volume copié/ignoré/indéterminé, motifs principaux, source et contrôles data-api ;
- ordres envoyés, acquittés et remplis ; annulations/rejets à `NULL` si le journal ne les qualifie pas ;
- PnL, rendement relatif au capital initial, positions non valorisées et tendance des trois comptes ;
- glissements entre les comptes, latences médiane/P90 et distribution par segment ;
- comparaison des prix/mises du wallet suivi et des fills effectifs du bot, avec moyennes et comptes de copies non remplies ;
- état du service, run paper/live, verrou live, dernier trade et arrêt de sécurité éventuel.

Un compteur mesuré à zéro s'affiche `0` ; une donnée absente ou pas encore mesurée s'affiche `NULL`, jamais comme un faux zéro.
Le journal s'allonge d'un run à l'autre : le rapport ne compte que le dernier run (ses comptes repartent de zéro). L'état du service et le verrou live sont lus en direct sur le serveur.

## UI temps réel (sur le PC)

```
python copy-trading/ui/serve.py                                                 # journal du service
python copy-trading/ui/serve.py --journal /home/ubuntu/ct-maker/journal.jsonl --no-sudo --port 8767   # un run de test
python copy-trading/ui/serve.py --file journal.jsonl                            # un journal local, sans ssh
```

Puis ouvrir http://127.0.0.1:8765/ (ou le port choisi). Python 3 seul, aucune dépendance. Le serveur lit `deploy/.env` (`COPY_SSH_DEST`, `COPY_SSH_KEY`, `COPY_WALLET`), suit le journal par `ssh … tail -F` et relaie avec un cache les API publiques de Polymarket. Il n'écoute que sur 127.0.0.1. L'UI est en lecture seule : elle ne parle jamais au bot, et sur le serveur seule tourne la commande `tail`. Si ssh coupe, il se reconnecte seul et la page se recharge à partir du journal complet.

- **Bandeau** : wallet suivi, mode, stratégie, santé des flux chaîne / market, contrôles, période (1H, 6H, 1J, Run).
- **Comparaison** : performance lui / moi (écart grisé), rendement, délai de copie, prix d'entrée, exécution.
- **Marché sélectionné** : prix, ses fills (orange, taille selon le montant), mes ordres posés (bleu : plein si rempli, pointillé en file, × si annulé), file devant mes ordres.
- **Cascade des décalages** : un trade de lui par ligne ; son match → ma détection (orange, rouge si secours API) → mon ordre → mon fill (bleu), échelle log de 0 à 6 h.
- **Droite** : nos deux positions, carnet avec son niveau (Lui) et le mien (Moi), ma place dans la file et l'heure d'annulation automatique.
- **Liens** : survoler une ligne allume son marché dans la liste et son point dans le graphique ; cliquer un marché (liste, cascade, graphique) le sélectionne partout ; « Suivre le direct » revient au dernier marché actif ; ↑/↓ dans la liste.

## Stratégies (`strategy` dans la config)

| Clé | Valeurs | Défaut |
| --- | --- | --- |
| `exec` | `maker` : je pose un ordre limite à son prix · `taker` : je prends le carnet (FAK) | `maker` |
| `buy` | `cumulative` : mon engagement suit son cumul sur le token (barème) · `first_fill` : une entrée, sur son 1er remplissage | `cumulative` |
| `sell` | `lots` : sa fraction vendue, par lots vendables, au moins un lot en gain · `all` : toute ma position à son prix | `lots` |
| `cancel_after_min` | annulation de mon ordre posé s'il n'a plus rempli à ce prix depuis ce délai | `360` |

Ses ordres ouverts ne sont pas publics : ils sont déduits de ses remplissages. Une annulation de mon côté démentie par un fill de lui au même prix est tracée (`stat` « annulation à tort »).

## Fichiers sur le serveur

| Chemin | Rôle |
| --- | --- |
| `/opt/copy-trading/bin/copy-trading` | le binaire |
| `/var/lib/copy-trading/config.json` | config du run (écrite par `start`) |
| `/var/lib/copy-trading/journal.jsonl` | journal : une ligne JSON par événement (`start`, `trade`, `check`, `state`, `resolution`, `feed`, `warn`, `halt`, `stop`) |
| `/var/lib/copy-trading/wallet` | `<adresse>:v1.…` chiffré, chmod 600 (live) |
| `/var/lib/copy-trading/journal.jsonl.live-lock` | verrou d'un run live en cours |
| `/var/lib/copy-trading/run.log` | sortie du process (`ARMED`, messages d'erreur) |

## Limites connues

- Live : les marchés **neg-risk** (échange `0xe2222d…`) sont ignorés, avec une ligne dans le journal. La signature (`engine/adapters/polymarket/src/sign.rs`) ne connaît que l'échange standard.
- Live : les montants remplis viennent de `makingAmount` / `takingAmount` de la réponse du CLOB. Ce point n'a pas encore été vérifié sur un ordre réel de copy-trading.
- Les positions déjà présentes sur le compte avant le démarrage du bot ne sont pas comptées dans le PnL.
