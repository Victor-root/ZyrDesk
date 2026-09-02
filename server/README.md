# Le serveur ZyrDesk

Le serveur est facultatif. Sans lui, ZyrDesk se joint sur un réseau local, par un VPN ou par une adresse publique dont le port UDP 47000 est renvoyé, sans compte et sans rien d'autre. Avec lui, une personne a un compte, y rattache ses ordinateurs, les voit en ligne ou non depuis chacun d'eux, et s'y connecte en un clic. Il met en relation et ne regarde jamais passer l'image : en fonctionnement normal, rien d'une session ne passe par lui, et il ne connaît aucune clé de session. Le relais de secours, qui transportera des paquets chiffrés sans les ouvrir quand aucun chemin direct n'existe, vient au jalon M6. La conception complète est dans [docs/SERVER.md](../docs/SERVER.md).

Un seul programme, `zyrdesk-server`, sous [AGPLv3](LICENSE), et un script, `install.sh`, qui l'installe, le met à jour, le reconfigure ou le retire.

## Ce qu'il faut

- Un Debian 12 ou 13 avec systemd, dans un conteneur LXC non privilégié de Proxmox ou sur une machine ordinaire. Le binaire publié est en x86_64 ; une autre architecture, un Raspberry Pi par exemple, compile sur place (`--from-source`). Le serveur est léger : un conteneur ordinaire suffit.
- Le compte root le temps de l'installation.
- Pour être joint depuis Internet : un nom de domaine ou l'adresse publique de la box, et deux ports renvoyés vers le conteneur, le TCP de l'API et l'UDP du miroir, celui qui dit à un ordinateur son adresse vue de l'extérieur et rend le direct possible. Le script ne touche pas à la box et l'écrit dans son résumé, avec les numéros. Une adresse publique en 100.64.0.0/10 (CGNAT) ne se renvoie pas ; le script le dit quand il la voit.

Ni base de données à installer, ni certificat à acheter.

## Installer

### 1. Obtenir le programme

Dans le conteneur, en root, une seule ligne :

```
bash <(curl -fsSL https://raw.githubusercontent.com/Victor-root/ZyrDesk/develop/server/install.sh)
```

Le script télécharge le programme de la dernière version publiée du dépôt et vérifie son empreinte, puis pose ses questions. `--version vX.Y.Z` en choisit une autre que la dernière.

**Tant qu'aucune version n'est publiée**, ce téléchargement n'a rien à prendre, et il y a deux façons de lui donner le programme.

**Compiler sur place**, avec la même ligne, une fois un compilateur C et Rust posés dans le conteneur :

```
apt-get update && apt-get install -y git curl build-essential
curl -fsSL https://sh.rustup.rs | sh -s -- -y && . "$HOME/.cargo/env"
bash <(curl -fsSL https://raw.githubusercontent.com/Victor-root/ZyrDesk/develop/server/install.sh) --from-source --branch develop
```

La compilation prend quelques minutes, et rien du reste du produit n'est compilé : seulement le serveur et ce dont il a besoin.

**Le binaire compilé par l'intégration continue**, pour ne rien compiler dans le conteneur. À chaque changement du serveur, le flux de travail « Serveur » du dépôt (onglet Actions sur GitHub) produit un artefact `zyrdesk-server-x86_64`. Le télécharger depuis le PC et le décompresser : deux fichiers, le programme `zyrdesk-server-x86_64-linux-musl` et son empreinte `.sha256`. Les déposer dans le conteneur avec `server/install.sh`, par exemple depuis l'hôte Proxmox :

```
pct push <numéro du conteneur> zyrdesk-server-x86_64-linux-musl /root/zyrdesk-server-x86_64-linux-musl
pct push <numéro du conteneur> zyrdesk-server-x86_64-linux-musl.sha256 /root/zyrdesk-server-x86_64-linux-musl.sha256
pct push <numéro du conteneur> install.sh /root/install.sh
```

Puis, dans le conteneur, en root :

```
sha256sum -c zyrdesk-server-x86_64-linux-musl.sha256
bash install.sh --binary ./zyrdesk-server-x86_64-linux-musl
```

### 2. Répondre aux questions

Le script se lance en root, dans un terminal, en français quand la machine l'est et en anglais sinon (`--lang fr` ou `--lang en` pour choisir). Il vérifie la machine, puis pose ses questions, chacune avec en défaut ce qu'il a détecté ; Entrée garde le défaut.

| Question | Ce qu'elle décide |
|---|---|
| Nom affiché du serveur | Le nom que l'application montre dans la section Compte |
| Adresse publique | Ce que les appareils tapent : un nom de domaine, ou l'adresse publique de la box, détectée et proposée en défaut |
| Chiffrement de l'API | 1) un mandataire inverse déjà en place, avec son certificat : le serveur écoute en clair sur la boucle locale (port 8443 en défaut) et le script imprime les lignes exactes pour Caddy et nginx ; 2) un certificat auto-signé, généré par le script, valable dix ans, portant le nom et l'adresse saisis : l'application demandera de comparer son empreinte, une fois par appareil ; 3) des fichiers à vous, certificat (chaîne complète) et clé, vérifiés ensemble puis copiés sous `/etc/zyrdesk-server/tls/` |
| Port de l'API | TCP 443 en défaut, hors mandataire |
| Port UDP du miroir et du relais | UDP 443 en défaut. Le miroir y répond dès maintenant ; le relais vient au jalon M6, et la question qui suit, l'activer ou non, ne fait rien tant qu'il n'existe pas |
| Dossier des données | `/var/lib/zyrdesk-server` : la base et les clés du serveur |
| Inscriptions | Ouvertes ; sur invitation, le défaut, avec un code par compte à créer ; ou fermées, les comptes se créant alors sur le serveur par `user create` |
| Premier compte | Son nom et son mot de passe, douze caractères au moins, tapé deux fois sans écho |

Un récapitulatif précède l'installation, et rien n'est modifié avant le « oui » qui le suit. Chaque étape s'affiche derrière une roue, puis en `✓` ou en `✗` ; la sortie des commandes n'apparaît qu'en cas d'échec.

### 3. Lire le résumé

À la fin, un panneau vert donne ce qu'il faut garder :

- **l'adresse à taper dans l'application**, sur chaque ordinateur à rattacher ;
- **l'empreinte du serveur**, en certificat auto-signé : c'est elle que l'application montre au premier rattachement et demande de comparer ; `zyrdesk-server fingerprint` la réaffiche à tout moment ;
- **les ports à renvoyer sur la box** vers l'adresse du conteneur ;
- le code d'invitation pour un second compte, en politique d'invitation ;
- la ligne sur les clés : `/var/lib/zyrdesk-server/keys` fait l'identité du serveur, à sauvegarder.

Le script garde ses réponses dans `/etc/zyrdesk-server/install.env`, sans le mot de passe, et les propose en défaut quand on le relance.

## Rattacher un ordinateur

Sur chaque ordinateur, dans ZyrDesk : Réglages, section Compte, « Se connecter à un serveur ». L'adresse, le nom du compte, le mot de passe, et le nom sous lequel cet ordinateur apparaîtra ; « Créer un compte » crée le compte d'abord, avec le code d'invitation si le serveur en demande un. En certificat auto-signé, un panneau orange montre l'empreinte que le serveur présente : si c'est celle du résumé, « C'est bien lui, continuer » la retient, et un serveur qui en présenterait une autre serait refusé. Sans fenêtre, `zyr-cli account attach <adresse> --user <nom>` fait de même, et propose `--trust <empreinte>` quand il y a une empreinte à retenir.

L'application ne parle jamais en clair : une adresse tapée sans schéma est comprise en `https://`, et `http://` est refusé.

## Administrer

Tout se fait sur la machine, jamais par le réseau, avec le programme lui-même. Les commandes qui touchent à la base se lancent sous l'identité du service, comme le script le fait, pour que la base ne change jamais de propriétaire :

```
zyrdesk-server status                        # où il en est : comptes, appareils, inscriptions
zyrdesk-server check                         # le serveur se joint lui-même, comme un appareil le ferait
zyrdesk-server fingerprint                   # l'empreinte à comparer dans l'application
runuser -u zyrdesk -- zyrdesk-server user create <nom> --password-stdin
runuser -u zyrdesk -- zyrdesk-server user list
runuser -u zyrdesk -- zyrdesk-server user reset-password <nom> --password-stdin
runuser -u zyrdesk -- zyrdesk-server user delete <nom>
runuser -u zyrdesk -- zyrdesk-server invite new          # un code d'invitation
runuser -u zyrdesk -- zyrdesk-server invite list
runuser -u zyrdesk -- zyrdesk-server invite revoke <code>
runuser -u zyrdesk -- zyrdesk-server backup <dossier>    # base, configuration et clés, cohérents
```

Un mot de passe se donne sur l'entrée standard, jamais sur la ligne de commande : la commande le demande quand rien ne lui est donné, et `printf '%s\n' 'le mot de passe' | runuser -u zyrdesk -- zyrdesk-server user create victor --password-stdin` le passe depuis un script. Remettre un mot de passe déconnecte le compte partout ; supprimer un compte emporte ses appareils, ses contacts et ses partages.

Le journal est celui de systemd : `journalctl -u zyrdesk-server -f`. Une ligne par événement, jamais de secret.

### Mettre à jour, reconfigurer, retirer

Relancer `bash install.sh` là où le serveur est installé ouvre un menu : mettre à jour (le programme nouveau, que `--binary` et `--from-source` désignent ici aussi ; la base migre au démarrage suivant), reconfigurer (les mêmes questions, avec les réponses d'avant), afficher l'état, désinstaller. La désinstallation a deux paliers : le service et le programme d'abord ; puis, sur un « oui » tapé en entier, les données, les clés et la configuration, après quoi les appareils rattachés perdent leur compte.

### Sauvegarder

`zyrdesk-server backup <dossier>` écrit une copie cohérente de la base, la configuration et les clés. Les clés de `/var/lib/zyrdesk-server/keys` sont l'identité du serveur : sans elles, un serveur réinstallé est un autre serveur aux yeux des appareils, qui devront se rattacher à nouveau. Proxmox sauvegarde de toute façon le conteneur entier.

## Derrière un mandataire inverse

Avec le premier mode de chiffrement, le serveur écoute en clair sur `127.0.0.1:8443`, et c'est le mandataire qui porte TLS et le certificat, Let's Encrypt en général. Le canal vivant est un WebSocket : le mandataire doit transmettre `Upgrade` et `Connection`, et laisser une connexion ouverte longtemps. Le script imprime les lignes exactes ; les voici pour mémoire.

Caddy, dans le Caddyfile :

```
zyr.exemple.fr {
    reverse_proxy 127.0.0.1:8443
}
```

nginx, dans le bloc `server` du nom :

```
location / {
    proxy_pass http://127.0.0.1:8443;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $remote_addr;
    proxy_read_timeout 3600s;
}
```

Le serveur refuse d'écouter en clair ailleurs que sur une adresse de boucle locale : `listen = "0.0.0.0:8080"` sans certificat l'empêche de démarrer, avec la phrase qui l'explique.

## Ce qui est écrit où

| Quoi | Où |
|---|---|
| Programme | `/usr/local/bin/zyrdesk-server` |
| Configuration | `/etc/zyrdesk-server/server.toml`, lisible par root et par le service |
| Certificat et clé TLS | `/etc/zyrdesk-server/tls/` |
| Base et clés du serveur | `/var/lib/zyrdesk-server/`, au service seul |
| Réponses du script | `/etc/zyrdesk-server/install.env` |
| Service | `zyrdesk-server.service`, sous l'utilisateur système `zyrdesk`, relancé s'il tombe |

Hors conteneur, l'unité reçoit en plus un durcissement fondé sur les montages (`ProtectSystem`, `PrivateTmp` et les autres) ; dans un LXC non privilégié, Proxmox le refuse et le script ne l'écrit pas.

## Si quelque chose ne va pas

- `zyrdesk-server check` dit ce que voit un appareil : le serveur répond ou non, en TLS ou en clair, avec quelle empreinte, et si son miroir répond sur son port UDP. C'est la première commande à lancer quand une application dit que le serveur refuse.
- Deux ordinateurs du compte se voient en ligne mais la session ne s'ouvre pas depuis l'extérieur : lire `service.log` de celui qui se connecte. `answered through …` dit par où c'est passé ; sans lui au bout de quinze secondes, aucune adresse annoncée n'a répondu, ce qui arrive avec deux box qui changent de port à chaque destination. Renvoyer UDP 47000 vers l'ordinateur regardé sur sa box règle le cas ; le port UDP du serveur doit lui aussi être renvoyé, sans quoi personne n'apprend son adresse vue de l'extérieur.
- `systemctl status zyrdesk-server` et `journalctl -u zyrdesk-server -n 50` disent pourquoi il ne démarre pas ; une configuration fautive est expliquée en une phrase.
- L'application dit que le serveur présente un certificat que personne ne garantit : c'est le mode auto-signé, et c'est attendu une fois par appareil. Comparer l'empreinte avec `zyrdesk-server fingerprint`. Si elle diffère, quelqu'un est entre les deux, ou le serveur a été réinstallé sans ses clés.
- L'application dit que le serveur a changé de clé : le certificat a été refait sur une autre clé. Se détacher, puis se rattacher en comparant l'empreinte nouvelle.
- Depuis Internet rien ne répond mais tout marche depuis le réseau local : le port TCP de l'API n'est pas renvoyé sur la box, ou l'adresse publique est en CGNAT.
