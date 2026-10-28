# Sécurité et mode réel

Ce fichier synthétise la sécurité de l'ancien engine (clé, garde réseau, garde-fous), les revues menées, les décisions sur le mode réel, l'incident du 2026-10-22 et le constat du 2026-10-27 sur `dublin-live`.

Convention : chemins relatifs au dépôt source `btc5m-lag-sim` (lecture seule), cités `fichier:ligne`. `[à vérifier]` marque ce qui n'est pas établi. Voir aussi [engine-conception.md](engine-conception.md) (garde-fous dans la conception), [exploitation-deploiement.md](exploitation-deploiement.md) (serveurs), [journal-decisions.md](journal-decisions.md).

## 1. Règles absolues de la mission

| Règle | Contenu | Source |
| --- | --- | --- |
| 1 | Aucun ordre réel sans le « go » explicite de l'utilisateur dans la conversation ; seule exception : un essai de mesure à mise minimale s'il l'accepte | `TODO.md:31-32` |
| 5 | Un ordre sans réponse est « inconnu » : jamais compté rempli, jamais renvoyé à l'aveugle (risque de double ordre) ; on interroge Polymarket jusqu'à connaître son sort | `TODO.md:40-41` |
| 6 | La clé privée et la phrase secrète ne sont jamais écrites, journalisées ni affichées | `TODO.md:42` |
| 7 | Demander avant de créer un compte, payer, installer un skill/plugin/MCP, analyser une plateforme avec REA, lancer quoi que ce soit sur un nœud distant, ajouter une dépendance, supprimer un fichier | `TODO.md:43-50` |
| Principe 2 | « Dans le doute, on n'ouvre pas » : donnée périmée, source en trou, position incertaine, désaccord de réconciliation, horloge non synchronisée → aucun nouvel ordre d'ouverture ; sorties, règlements et réclamations continuent | `TODO.md:184-186` |

Garde-fous exigés par l'utilisateur dès la mission : plafond par ordre, seuil de perte qui arrête la session sans retour automatique en papier, arrêt d'urgence qui bloque les nouveaux ordres (les positions vont au règlement), clé déchiffrée en mémoire seulement, garde réseau qui bloque toute écriture vers Polymarket non prévue, contrôle géographique avant d'armer (`TODO.md:76-82`).

## 2. Garde-fous de la conception (engine Rust)

| Garde-fou | Règle | Source |
| --- | --- | --- |
| Plafond par ordre | Un ordre au-dessus est refusé (pas réduit) | `engine/02-conception.md:250` |
| Seuil de perte de session | Atteint : plus aucune ouverture ; les positions vont au règlement et sont réclamées | `engine/02-conception.md:251` |
| Arrêt d'urgence | Drapeau atomique lu par le fil chaud avant chaque envoi ; sorties, règlements, réclamations continuent | `engine/02-conception.md:59-60, :252` |
| Pas d'ouverture si… | source utilisée invalide, ordre `inconnu`, réconciliation en désaccord, horloge chrony non synchronisée, marché non préparé | `engine/02-conception.md:253` |
| Garde réseau | Une seule fonction écrit vers Polymarket ; liste blanche (`POST /order`, annulation, `auth`, relayer) ; toute écriture refusée quand l'engine n'est pas armé | `engine/02-conception.md:254` |
| Clé | Déchiffrée à l'armement, effacée au désarmement (`zeroize`), jamais écrite ni journalisée | `engine/02-conception.md:255` |
| Contrôle géographique | `GET polymarket.com/api/geoblock` avant d'armer ; refus si le pays est bloqué pour l'API | `engine/02-conception.md:256` |
| Limites de débit | Respect de `Retry-After` sur 425/429/503 ; jamais de boucle REST | `engine/02-conception.md:257` |

Valeurs par défaut décidées le 2026-10-19 : stop loss si la valeur latente de sortie tombe sous −50 % du coût, hors dernière minute ; plafonds de 5 $ par ordre et 25 $ de perte par session ; seuil de session = réalisé + latent ; tous ces paramètres sont modifiables sans redémarrage par la commande « limites », chaque changement étant un fait `command` du registre (`engine/02-conception.md:335-342`).

Machine d'états d'ordre : décidé → signé → envoyé → acquitté → apparié → confirmé ; issues refusé, tué (FAK sans contrepartie), échoué, inconnu. Un ordre `inconnu` n'est jamais renvoyé et bloque les ouvertures (`engine/02-conception.md:125-145`).

## 3. Clé et phrase secrète : revue du 2026-10-21

Revue faite par l'agent lui-même, sans pipeline de skill (`engine/reports/10-revue-securite.md:3-6`).

| Étape | Protection | Verdict | Source |
| --- | --- | --- | --- |
| Fichier du wallet (`<adresse>:v1.<…>`, format de l'ancienne application) | Refusé s'il est lisible par le groupe ou les autres (`chmod 600` exigé, testé) ; contenu effacé en mémoire après lecture | OK | `engine/reports/10-revue-securite.md:12` |
| Phrase secrète | Lue sur l'entrée standard, jamais en argument ; tampon réservé d'avance puis effacé (une copie non effacée a été corrigée) | OK | `engine/reports/10-revue-securite.md:13` |
| Déchiffrement | scrypt N=2^17 + AES-256-GCM ; clé en `Zeroizing`, effacée après création du signataire | OK | `engine/reports/10-revue-securite.md:14` |
| Erreurs | Le message d'arrêt ne nomme que la catégorie (`Format`, `Passphrase`) | OK | `engine/reports/10-revue-securite.md:15` |
| Identifiants d'API | Dérivés (`GET /auth/derive-api-key`), effacés à leur destruction ; caractères de contrôle refusés (injection d'en-têtes) | OK | `engine/reports/10-revue-securite.md:16` |
| Copies non effacées | Le message d'abonnement du canal `user` contient les identifiants dans une chaîne ordinaire, recréée à chaque reconnexion | Accepté : mémoire du processus seulement | `engine/reports/10-revue-securite.md:17` |
| Registre | Aucun fait ne contient la clé, la phrase ou les identifiants | OK | `engine/reports/10-revue-securite.md:18` |

Risque résiduel : copies de secrets non effacées dans des tampons de bibliothèques (corps de la réponse `/auth/api-key`, trame d'abonnement `user`) ; à traiter avant la phase 10, y compris en désactivant les core dumps du processus à l'armement (`engine/reports/06-revue-adversariale.md:56`). Le traitement effectif de ce risque n'est pas établi dans les sources lues `[à vérifier]`.

## 4. Garde réseau et garde-fous : constats de la revue du 2026-10-21

| Point | Constat | Source |
| --- | --- | --- |
| Écritures possibles | `POST /order` seulement en pratique ; `DELETE /order` et `POST /auth/api-key` listés mais jamais appelés | `engine/reports/10-revue-securite.md:24` |
| Lectures | Connexion à part ; `Clob::get` refuse tout ce qui n'est pas un `GET` | `engine/reports/10-revue-securite.md:25` |
| Envoi unique | L'ordre passe par valeur ; `post` n'écrit qu'un ordre `Signed` ; aucun renvoi ; un ordre inconnu est interrogé (2 s × 30), jamais renvoyé | `engine/reports/10-revue-securite.md:26` |
| Nombre d'ordres | **Défaut trouvé et corrigé** : `"orders": 0` laissait passer un premier ordre ; désormais 0 bloque toute ouverture dès l'armement | `engine/reports/10-revue-securite.md:27` |
| Plafond | Un ordre au-dessus de `max_order` est refusé ; la mise doit donc être ≤ 5 pUSD | `engine/reports/10-revue-securite.md:28` |
| Réconciliation | Ouverture seulement sur « accord » ; ce qui ne se lit pas reste « en attente » donc bloque | `engine/reports/10-revue-securite.md:29` |
| Horloge | `timedatectl` : synchronisée (écart chrony 3 µs) ; sinon refus | `engine/reports/10-revue-securite.md:30` |
| Ventes | Non envoyées (la référence tient jusqu'au règlement) | `engine/reports/10-revue-securite.md:31` |
| Perte de session | Réalisé seulement, sans le latent (accepté pour un seul ordre de 5 pUSD) | `engine/reports/10-revue-securite.md:32` |

**Incident de méthode.** Pendant un essai d'armement avec la clé jetable des tests (solde nul), une décision de la stratégie (≈ 1 % de chances sur 20 s) aurait envoyé un ordre réel sans le « go » de l'utilisateur ; le registre confirme que rien n'est parti. Cause : le défaut `"orders": 0` ci-dessus, corrigé ; les essais se font désormais avec `"orders": 0` (`engine/reports/10-revue-securite.md:40-43`).

## 5. Revue adversariale de la phase 6 (2026-10-20)

Méthode : skill `differential-review`, 15 fichiers et 1 401 lignes ajoutées ; 8 constats, tous corrigés et testés (`engine/reports/06-revue-adversariale.md:3-5, :26-35, :64`).

| # | Gravité | Constat | Correctif |
| --- | --- | --- | --- |
| 1 | Élevée | `Clob::post` pouvait écrire deux fois le même ordre (double ordre réel) | `post` ne sort rien si l'ordre n'est plus `Signed` (test `an_order_is_never_written_twice`) |
| 2 | Élevée | `Clob::get` écrivait n'importe quels octets (contournement de la garde) | `get` refuse tout ce qui ne commence pas par `GET ` |
| 3 | Moyenne | Liste blanche par préfixe : `/order` autorisait `/orders` | Chemin exact ; `/auth/` réduit à `/auth/api-key` |
| 4 | Moyenne | Identifiants d'API copiés dans les en-têtes sans contrôle | Refus des identifiants vides ou à caractères de contrôle |
| 5 | Moyenne | `read_order` adoptait l'ordre renvoyé sans vérifier son `id` | `read_order(r, id)` ignore tout autre `id` |
| 6 | Moyenne | Identifiants de token non validés (débordement uint256) | Refus de tout ce qui ne tient pas dans un uint256 |
| 7 | Moyenne | Tailles HTTP du réseau non bornées (panique, déni de service) | Corps limité à 1 Mio, contrôle avant tout calcul de décalage |
| 8 | Faible | `Clob::warm` sans délai | Délai de 2 s |

Source des 8 lignes : `engine/reports/06-revue-adversariale.md:28-35`. Vérifié sans constat : ordre compté rempli à tort, renvoi, signature (80 vecteurs du SDK reproduits octet pour octet), clé en mémoire, paniques sur entrée réseau (`engine/reports/06-revue-adversariale.md:39-50`). Risques résiduels non corrigés à ce stade : copies de secrets, hypothèse `orderID` = hachage EIP-712, formats du canal `user` et de `GET /data/order` jamais vus sur un vrai message, coût du TLS sur l'écriture d'un ordre (`engine/reports/06-revue-adversariale.md:54-59`).

## 6. Décisions du mode réel et incidents

### 6.1 Décisions

| Date | Décision | Source |
| --- | --- | --- |
| 2026-10-17 | Ancienne application : réel source par source et à chaque session (phrase secrète + confirmation en tapant le nom de chaque source) ; arrêt d'urgence ne bloquant que les nouveaux ordres ; jamais de retour automatique en papier ; seuil de perte fixé par l'utilisateur sinon le réel ne démarre pas ; plafond par ordre par défaut 100 $ | `docs/real-trading.md:8-15` |
| 2026-10-17 | Ancienne garde réseau (`src/exec/guard.js`) : toute écriture vers `*.polymarket.com` refusée sauf trois cas accordés par le module réel (`POST /order` d'une source armée, création des identifiants d'API pendant le déverrouillage, relayer pendant une réclamation). Armer exigeait nœud live + wallet à clé chiffrée + phrase secrète + seuil de perte + absence de blocage géographique | `SPEC.md:246-259` |
| 2026-10-17 | Le contrôle géographique de l'ancienne application accepte les pays « Close-Only on Frontend » (IE, NL, JP, KR) ; tout autre blocage (GB, FR, US…) ou réponse illisible est refusé | `SPEC.md:327-333` ; `docs/real-trading.md:69-73` |
| 2026-10-21 | Mode réel de l'engine : décisions de la référence, 5 pUSD, achats seulement, 1 ordre par run, puis en continu (5 pUSD tant que le dernier solde lu les couvre, arrêt à 10 h, perte de session 25 pUSD) | `engine/JOURNAL.md:28-29` |
| 2026-10-21 | Identifiants d'API dérivés, jamais créés ; blocage « pays » levé car l'utilisateur est physiquement à Dublin ; la légalité locale n'est pas étudiée | `engine/JOURNAL.md:30-31` |
| 2026-10-22 | Live coupé à 01:20 UTC, solde intact | `engine/JOURNAL.md:26` |

Géoblocage (fait externe, vérifié le 2026-10-17) : la localisation est jugée sur le lieu physique ; GB et d'autres pays sont en close-only sur le site et l'API ; l'Irlande est en close-only sur le site seulement, l'API est ouverte ; VPN et contournement interdits (`SPEC.md:315-325` ; `engine/00-recherche.md:332-336`). Voir `plateformes-et-conformite.md` pour la matrice complète.

### 6.2 Incident du 2026-10-22 : trois ordres réels tués

Trois ordres FAK de 5 pUSD envoyés par l'engine sur le wallet de l'utilisateur, tous tués ; aucun trade, solde intact à 20,697166 pUSD (`engine/10-delai-appariement.md:3-4` ; `engine/JOURNAL.md:26` ; `docs/reprise-integration-engine.md:113`). L'enquête montre que le délai naît chez Polymarket pendant la retenue des ordres preneurs (réponses en 868, 3 499 et 348 ms) et non dans le trajet de l'ordre (0,21 à 0,23 ms de la décision au dernier octet écrit) (`engine/10-delai-appariement.md:6-12, :17, :40-44`). Conséquences pour la sécurité : le délai d'attente de réponse a été porté de 300 ms à 5 s pour qu'un ordre lent ne passe plus en « inconnu » et ne bloque pas à tort les ouvertures (`engine/10-delai-appariement.md:106`).

### 6.3 Constat du 2026-10-27 : un trader réel est armé sur dublin-live

**Fait constaté le 2026-10-27** : sur `dublin-live`, la session `2026-10-18T23-09-09_live` a un trader réel armé sur la source `okx`. Trois ordres réels de 5 $ y ont été exécutés ; des refus pour risque sont ensuite apparus.

Sources indiquées par la demande de rédaction : `src/server/app.js:108-138` de la version déployée, et le fichier `records.jsonl` de la session. **Statut de vérification** : ces deux sources se trouvent sur le serveur, auquel le rédacteur n'a pas accès (interdit par la tâche) ; le constat est repris tel quel et n'a pas été revérifié `[à vérifier]`. Éléments de contexte vérifiables dans le dépôt source :

- Le `src/server/app.js` actuel du dépôt (HEAD) ne contient plus ce comportement : aux lignes 101-125, `startRun` lance le binaire de l'engine Rust (`src/server/app.js:101-125`, constaté par lecture). La version déployée est donc plus ancienne que le HEAD `[à vérifier : quelle version exacte]`.
- Dans la version antérieure à la purge (commit `7138fbc^`), `src/server/app.js:105-140` contient `startSession`, qui crée un `RealTrader`, l'arme (`t.arm({ sources, wallet, passphrase, limits })`) et écrit un événement `{k:'real', type:'armed'}` dans la session. Cette lecture (`git show 7138fbc^:src/server/app.js`) est cohérente avec un « trader réel armé » mais ne prouve pas à elle seule la version déployée.
- Le principe de l'ancien trader réel : miroir du compte simulé « Réaliste », ordre au marché FAK, montant plafonné (`engine/01-audit.md:29-33` ; `docs/real-trading.md:26-31`).
- Ce constat contredit les phrases « Aucun ordre réel n'a jamais été envoyé » (`TODO.md:227`) et « Aucun ordre réel n'a encore été essayé » (`docs/real-trading.md:76`), antérieures ou propres à d'autres périodes. Il ne se confond pas avec l'incident du 2026-10-22 (3 ordres FAK tués, session de l'engine Rust, trois jours plus tard). Le rapport entre les deux n'est pas établi `[à vérifier]`.
- Le wallet de l'ancienne application est celui de l'utilisateur (`0x515A…610c`) ; le fichier du wallet chiffré se trouve sur le serveur (chemin `~/p10/wallet` mentionné par la session de migration). `data/wallets` n'est pas copié dans le dépôt cible (décision D9, [contrats-migration.md:189](contrats-migration.md)).

**Conséquence retenue** (décision D8, 2026-10-27, [contrats-migration.md:188](contrats-migration.md)) : sur dublin-live, arrêt propre (sell-only, attendre que les positions soient à plat) et confirmation nominative de l'utilisateur sur le wallet AVANT tout nettoyage. Aucun déploiement n'est autorisé sur dublin-live (voir §7).

## 7. Garde-fous du dépôt cible `updown5m-lab`

| Mesure | Détail | Source (dépôt cible) |
| --- | --- | --- |
| Jamais de déploiement sur dublin-live | `deploy/deploy.sh` refuse si la destination brute ou l'hôte résolu par `ssh -G` contient l'IP publique ou le nom de dublin-live | `deploy/deploy.sh:10-16` |
| Jamais d'installation sur dublin-live | `deploy/setup.sh` refuse selon le nom d'hôte, puis selon l'IP publique lue par IMDSv2 (pas de réponse IMDS = non bloquant) | `deploy/setup.sh:7-11` |
| Service durci et local | `NoNewPrivileges`, `ProtectSystem=strict`, écriture seulement dans `/var/lib/updown5m`, UI sur `127.0.0.1:8787` | `deploy/updown5m@.service:16-17, :24-33` |
| Pas d'ordre depuis le tracing | L'objectif du tracer est l'enregistrement de flux ; aucune clé ni wallet n'est copié (D9) | `PROMPT-MIGRATION.md` (sections DOCS et contraintes) ; décision D9 |
| Accès à l'UI par tunnel SSH | Aucune exposition publique | `PROMPT-MIGRATION.md:96` |

L'authentification de l'UI du tracing est absente par choix (shortcut documenté dans les contrats : loopback + tunnel SSH, toute personne ayant un shell sur le serveur peut basculer une source) ; c'est acceptable pour un outil qui ne passe aucun ordre.

## 8. Points d'attention opérationnels

- Le mode automatique de Claude Code a parfois refusé des actions touchant au réel (copie de `start.sh` vers le nœud, `git commit`, lecture de l'environnement du service) ; ne pas les contourner, demander à l'utilisateur (`docs/reprise-integration-engine.md:111`).
- `pkill -f` sur un nœud peut tuer la session SSH elle-même quand le motif figure dans la commande ; viser le numéro de processus (`docs/reprise-integration-engine.md:112`).
- Ne jamais lancer le réel sans le « go » explicite de l'utilisateur (`docs/reprise-integration-engine.md:107`).

## 9. Points `[à vérifier]`

- Constat du 2026-10-27 (§6.3) : version exacte de `src/server/app.js` déployée, contenu de `records.jsonl`, rapport avec l'incident du 2026-10-22.
- Traitement des copies de secrets non effacées (§3) avant la phase 10 : non établi.
- Formats du canal `user` et de `GET /data/order`, hypothèse `orderID` = hachage EIP-712 : « non vérifié avant le premier ordre réel » (`engine/reports/10-revue-securite.md:45-49`).
- Légalité locale de l'opération depuis Dublin pour un résident français : non étudiée (`engine/JOURNAL.md:31`).
