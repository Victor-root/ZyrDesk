# Jalon M6 : le relais et la bascule automatique

Ce document se déroule sur les deux mêmes PC Windows et le même serveur qu'au jalon M5. Il suppose ce jalon-là fait : les deux PC rattachés au compte, le serveur joignable, une session par le rendez-vous déjà tenue au moins une fois ([M5-PROTOCOLE.md](M5-PROTOCOLE.md)).

Vocabulaire : **PC hôte** = celui qu'on contrôle. **PC client** = celui depuis lequel on se connecte. **Serveur** = le conteneur. **Direct** = la session passe d'un PC à l'autre. **Relais** = elle passe par le serveur.

**Ce que ce jalon fait.** Quand deux ordinateurs ne peuvent pas se joindre en direct, le serveur porte leurs paquets sans pouvoir les lire, et la session s'ouvre quand même. Le relais n'est jamais un choix : il sert le temps qu'aucun chemin direct ne réponde, et la session repasse en direct toute seule, sans coupure, dès qu'un chemin direct est validé. Chaque appareil reçoit du serveur, avec le ticket de session, un **laissez-passer** qui ne le laisse joindre qu'un seul autre appareil, pour une seule session, pendant cinq minutes.

**Ce qu'il ne fait pas, et qu'il ne faut donc pas chercher.** Demander à la box d'ouvrir un port toute seule (UPnP, NAT-PMP, PCP) : c'est le complément qui viendra ensuite, et c'est le relais qui tient ce cas en attendant. Un repli sur TCP quand un réseau coupe tout UDP : hors périmètre, et un tel réseau coupe déjà le tunnel lui-même. Le banc de mesure relayé, qui chiffrera le surcoût : il reste à faire.

---

## Où on en est

**Règle de tenue :** un essai ne passe en « confirmé » que quand il a été essayé et dit tel quel. Rien n'y monte parce que le code a l'air juste ou parce que les tests automatiques passent.

### À vérifier maintenant

| Essai | Ce qu'il vérifie |
|---|---|
| **R1**, **R2** | Le serveur se met à jour, se fait son certificat de relais, et `check` dit à quelle adresse les appareils y sont envoyés |
| **R3** | **Le cas dur, et celui pour lequel le relais existe** : le client sur un partage de connexion 4G ou 5G, l'hôte à la maison |
| **R4**, **R5** | Le direct coupé au pare-feu : la session s'ouvre quand même, et repasse en direct dès qu'on rouvre, sans coupure |
| **R6** | Ce que la fenêtre affiche du chemin, et ce que les trois journaux en disent |
| **R7** | Les octets comptés : rien en direct, quelque chose en relais |
| **R8**, **R9** | Le relais débrayé, et les plafonds |

### Confirmé

| Essai | Ce qui a été dit |
|---|---|

---

## Avant de commencer

### Mettre à jour les deux PC

Sur **les deux PC**, dans une fenêtre PowerShell **administrateur** placée dans le dossier du projet :

```
taskkill /IM ZyrDesk.exe /F 2>$null; .\target\release\zyrdeskd stop; git pull && cargo build --release && .\target\release\zyrdeskd start
```

**Les deux ordinateurs doivent être à jour.** Un ordinateur en retard n'ouvrira pas sa branche de relais, et la session restera bloquée sur le direct.

### Mettre à jour le serveur

Sur le conteneur, relancer le script d'installation et choisir **mettre à jour** dans le menu :

```
bash <(curl -fsSL https://raw.githubusercontent.com/Victor-root/ZyrDesk/develop/server/install.sh)
```

Rien à reconfigurer : le fichier de configuration existant est lu tel quel, et les nouveaux réglages du relais prennent leur valeur par défaut. Au premier démarrage, le serveur se fait son certificat de relais dans `/var/lib/zyrdesk-server/keys/relay.crt` et `relay.key`.

### Le port UDP doit être renvoyé sur la box

C'est le seul prérequis réseau, et il est déjà rempli si le miroir a répondu au jalon M5 : le relais écoute sur **le même port UDP que le miroir**. Rien de plus à ouvrir.

---

## R1. Le serveur redémarre avec son relais

Sur le conteneur :

```
systemctl restart zyrdesk-server
journalctl -u zyrdesk-server -n 20 --no-pager
```

**Attendu.** Une ligne de la forme :

```
listening on 0.0.0.0:8443, TLS, mirror and relay on UDP 0.0.0.0:8443
```

Le mot qui compte est **`mirror and relay`**. S'il n'y a que `mirror`, le relais est débrayé dans la configuration (`[relay] enabled = false`) ou son certificat n'a pas pu être écrit ; la ligne juste avant dit laquelle des deux.

Puis :

```
ls -l /var/lib/zyrdesk-server/keys/
```

**Attendu.** `signing.key`, `relay.crt` et `relay.key`, tous appartenant à `zyrdesk`.

## R2. Le serveur dit où il envoie les appareils

Sur le conteneur :

```
zyrdesk-server check
```

**Attendu.** Les lignes du jalon M5, plus celle-ci :

```
  Relais : les appareils y sont envoyés sur zyrdesk.vroot.fr:8443
```

L'adresse est celle de `api.public_url` avec le port UDP du relais. **Si le nom ne mène nulle part depuis le conteneur**, la commande le dit en toutes lettres et donne le réglage à corriger : c'est la seule faute de configuration qui rendrait le relais inutilisable sans que rien d'autre ne le montre.

Puis :

```
zyrdesk-server status
```

**Attendu.** Une ligne `Relais : UDP 0.0.0.0:8443, 0 session relayée et 0 Mo portés en 30 jours`. Ce compteur est celui de R7.

## R3. Le client en 4G ou en 5G, l'hôte à la maison

**C'est l'essai principal du jalon.** Le PC client en partage de connexion depuis un téléphone, **VPN coupé** ; le PC hôte chez Victor, sur sa fibre. Ouvrir la session depuis « Mes ordinateurs », sur la **carte du compte** (celle qui porte le nom de l'ordinateur distant), et non sur une carte d'« Ordinateurs vus ».

**Attendu.** La session s'ouvre. Deux issues sont bonnes, et il faut noter laquelle :

- **En direct.** La perforation a suffi, même en 4G : c'est ce qui arrive quand l'opérateur garde le numéro de port en sortie. Le menu dit « par <adresse>:47000 ». Le relais n'a rien porté, et R7 le confirmera.
- **Par le relais.** L'opérateur change de port à chaque destination, ou partage une adresse entre des milliers d'abonnés (CGNAT) : aucun chemin direct n'existe. Le menu dit « par le relais <adresse>:8443 ». **C'est le cas pour lequel ce jalon a été écrit.**

Ce qu'il ne faut **pas** voir : la session qui ne s'ouvre pas. Si c'est le cas, aller à R6 et lire les trois journaux.

Tenir la session **dix minutes** et regarder l'image : le relais ajoute un aller-retour vers le serveur et rien d'autre. Une image qui se fige, qui saccade ou qui gèle est un vrai défaut, à rapporter avec les trois journaux.

## R4. Le direct coupé exprès : la session s'ouvre quand même

Cet essai se fait **sur le réseau local**, avec les deux PC côte à côte : c'est celui qui prouve le relais sans dépendre d'un opérateur.

Sur le **PC client**, dans PowerShell **administrateur** :

```
New-NetFirewallRule -DisplayName "ZyrDesk direct coupe" -Direction Outbound -Protocol UDP -RemotePort 47000 -Action Block
```

Cette règle coupe tout ce qui va vers le port du tunnel de l'autre ordinateur, dans les deux sens de fait, et ne touche ni le serveur ni le relais, qui vivent sur un autre port.

Ouvrir la session depuis « Mes ordinateurs », sur la **carte du compte**.

**Attendu.** La session s'ouvre, et le menu de la session dit « par le relais <adresse>:8443 ». L'aller-retour affiché est celui du chemin relayé, donc plus grand que le direct de la veille.

**Si la session ne s'ouvre pas**, le refus le dit lui-même : « n'a répondu ni en direct ni par le relais en 15 secondes », et le journal dit ce que chaque chemin a donné.

## R5. On rouvre : la session repasse en direct sans coupure

**La session de R4 restant ouverte et l'image à l'écran**, sur le **PC client**, dans la même fenêtre PowerShell :

```
Remove-NetFirewallRule -DisplayName "ZyrDesk direct coupe"
```

**Attendu.** Dans les cinq secondes, sans que l'image bouge, sans clic et sans rien relancer :

- le menu de la session passe de « par le relais <adresse>:8443 » à « par 192.168.x.x:47000 », et l'aller-retour tombe ;
- le journal du PC client écrit une ligne de la forme :

```
card 240.x.x.x: now through 192.168.1.20:47000, 1 ms, instead of the relay at 82.64.12.7:8443, which carried it for 34210 ms
```

**Ce qu'il ne faut pas voir** : l'image qui se coupe, la fenêtre qui se referme, ou un message d'erreur. La bascule est une ligne écrite dans une table ; la session, elle, ne sait même pas qu'elle a changé de route.

Remettre ensuite la règle et la retirer une seconde fois pour vérifier le retour : le direct coupé en pleine session doit rendre la session au relais au bout de trois sondes sans écho, soit environ six secondes, et l'image reprend.

## R6. Ce que les journaux disent

Trois journaux à lire, et ils racontent la même histoire de trois points de vue.

**Sur les deux PC**, dans « Journal » depuis l'accueil, chercher :

```
the relay at 82.64.12.7:8443 took the pass after 128 ms, 12 ms to it
card 240.x.x.x: reached through the relay at 82.64.12.7:8443, 38 ms
```

La première ligne dit que le serveur a bien donné un laissez-passer et que le relais l'a pris. Si elle manque, chercher plutôt `no relay:` : la ligne qui suit dit pourquoi (adresse qui ne mène nulle part, laissez-passer refusé, chemin qui ne porte pas 1200 octets).

**Sur le serveur** :

```
journalctl -u zyrdesk-server -f
```

**Attendu** pendant une session relayée, deux lignes comme :

```
relay: 0829cc7e… is in for session AbCd…, from 82.64.12.7:53211
relay: f145a3b2… is in for session AbCd…, from 176.153.4.9:41022
```

et, à la fin de la session, l'une de ces deux :

```
session AbCd…: relayed, 48213 kB carried, 0 still on the relay
session AbCd…: the relay held a road and carried nothing, 0 still on the relay
```

La seconde est le cas ordinaire : le relais était prêt, le direct a répondu le premier, et rien n'est passé par là.

**Ce que le serveur n'écrit jamais** : le contenu d'un paquet, une adresse de candidat, ou quoi que ce soit du flux. Il compte des octets, c'est tout.

## R7. Les octets comptés : rien en direct, quelque chose en relais

Après une session **en direct** (R5 après la bascule, ou une session sur le réseau local sans règle de pare-feu), sur le conteneur :

```
zyrdesk-server status
```

**Attendu.** La ligne `Relais` compte **0 session relayée** si aucune session n'est jamais passée par le relais. C'est le critère du jalon M5 qui tient toujours : en direct, le serveur ne porte rien.

Une précision qui compte pour lire ce chiffre : les deux ordinateurs ouvrent leur branche de relais **à chaque session**, direct compris, et le journal du serveur écrit donc `relay: … is in for session …` même quand la session finit en direct. Ce que le relais porte alors, ce sont les sondes qui mesurent le chemin relayé, et elles ne comptent pas comme du trafic. La ligne de fin le dit en toutes lettres : `the relay held a road and carried nothing`.

Après une session **relayée** (R4), la même commande compte une session de plus et les mégaoctets portés. Une session qui a commencé par le relais puis est passée en direct compte les octets du début, et c'est juste : ils sont bien passés par là.

## R8. Le relais débrayé

Sur le conteneur, mettre `enabled = false` dans la section `[relay]` de `/etc/zyrdesk-server/server.toml`, puis `systemctl restart zyrdesk-server`.

**Attendu.** Le journal du serveur dit `mirror on UDP …` sans le mot `relay`. `zyrdesk-server check` dit `Relais : aucun, les sessions sans chemin direct n'aboutiront pas`. Une session sur le réseau local marche toujours, et le miroir répond toujours : c'est lui qui rend le direct possible, et il ne se débraye pas.

Remettre la règle de pare-feu de R4 et essayer d'ouvrir une session : le refus doit dire, en toutes lettres, qu'aucun chemin direct n'a été trouvé **et que ce serveur n'a pas de relais**. C'est la phrase qui distingue un réseau difficile d'un serveur mal réglé.

Remettre ensuite `enabled = true` et redémarrer.

## R9. Les plafonds

Trois réglages, dans `[relay]` :

- `max_sessions` : le nombre de sessions relayées en même temps. Le mettre à `1`, redémarrer, ouvrir deux sessions relayées depuis deux ordinateurs différents : la seconde est refusée avec « ce relais porte déjà autant de sessions qu'il en accepte », dans le journal de l'ordinateur refusé.
- `max_kbps_per_session` : le débit qu'une session relayée a le droit de prendre, 60 000 par défaut (60 Mb/s). Le baisser à `5000` et ouvrir une session relayée : l'image reste regardable mais perd en qualité, sans que rien ne se coupe. Ce qui dépasse est jeté comme un réseau le ferait.
- `connections_per_minute` : le nombre de connexions qu'une même adresse a le droit d'ouvrir au relais en une minute, 60 par défaut. Il n'a pas à être essayé à la main ; il est là pour qu'un inconnu ne puisse pas occuper le port du relais, qui est la seule chose du serveur que n'importe qui peut atteindre sans compte.

Remettre les valeurs d'origine et redémarrer.

---

## Ce qu'il faut envoyer si un essai échoue

Les trois journaux, pris **après** l'essai raté, sans redémarrer quoi que ce soit :

- sur les deux PC : « Journal » depuis l'accueil, bouton **Copier**, ou les fichiers de `%LOCALAPPDATA%\ZyrDesk\logs\` ;
- sur le serveur : `journalctl -u zyrdesk-server -n 300 --no-pager`.

Dire, en une phrase, ce qui était branché où : quel PC était le client, sur quel réseau, avec ou sans la règle de pare-feu, avec ou sans VPN.
