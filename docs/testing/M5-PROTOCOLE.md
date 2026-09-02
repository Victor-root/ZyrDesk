# Jalon M5, tranche 1 : le serveur, le compte et « Mes ordinateurs »

Ce document se déroule sur les deux mêmes PC Windows que les jalons précédents, plus une troisième machine : **le serveur**, un conteneur Debian sur Proxmox, installé en suivant [server/README.md](../../server/README.md). Il ne demande aucune ligne de commande sur les PC en dehors de la mise à jour ; sur le serveur, quelques commandes, toutes écrites ici.

Vocabulaire : **PC hôte** = celui qu'on contrôle. **PC client** = celui depuis lequel on se connecte. **Serveur** = le conteneur. Les deux PC jouent tour à tour les deux rôles : un compte ne distingue pas ses ordinateurs.

**Ce que ces deux tranches font.** Un compte sur un serveur à soi ; chaque PC s'y rattache une fois, sous son nom, et prouve sa clé à chaque connexion ; chacun voit les autres dans « Mes ordinateurs », en ligne ou non, prêt ou non ; un appareil se renomme, se révoque ou se détache depuis n'importe lequel des autres ; le serveur présente deux ordinateurs l'un à l'autre par un ticket signé, l'hôte laisse entrer l'ordinateur présenté sans que le réseau local ait eu à le lui montrer, et les deux se cherchent à travers Internet : chacun dit à l'autre, par le serveur, ses adresses locales et celle que le miroir du serveur lui a renvoyée, chacun sonde toutes celles de l'autre avec des datagrammes signés, et la première qui répond porte la session, la plus courte ensuite.

**Ce qu'elles ne font pas encore, et qu'il ne faut donc pas chercher.** Demander à la box d'ouvrir un port toute seule (UPnP) : deux box qui changent de port à chaque destination, ce qui est le cas de beaucoup de réseaux mobiles, ne se joignent qu'avec un port renvoyé à la main chez l'hôte, ou par le relais du jalon M6. Les contacts et le partage d'une machine à quelqu'un d'autre : tranche 3. Le relais : jalon M6. Sur le même réseau local, les deux PC se voyaient déjà sans serveur ; ce que le compte y ajoute se lit sur la carte, et la session prend le même chemin qu'avant.

---

## Où on en est

Ce tableau de bord est la seule chose à lire pour savoir quoi essayer. Le reste du document est la référence, à ouvrir quand un essai échoue ou qu'on veut le détail d'un attendu.

**Règle de tenue :** un essai ne passe en « confirmé » que quand il a été essayé et dit tel quel. Rien n'y monte parce que le code a l'air juste, parce que les tests automatiques passent, ou parce que ça marchait la semaine d'avant. Un essai qu'un changement touche redescend dans « à vérifier ».

### À vérifier maintenant

Tout est nouveau : c'est la première livraison du serveur.

| Essai | Ce qu'il vérifie |
|---|---|
| **V1** à **V6** | Le serveur : l'installation jusqu'au panneau vert, `status`, `check`, le refus du clair, le journal, le menu du script relancé |
| **C1** à **C6** | Le rattachement des deux PC, avec l'empreinte à comparer, les refus qui parlent, et un compte créé depuis la fenêtre |
| **C7** à **C11** | La section Compte : le lien, les appareils, la présence qui suit l'accès distant, l'hôte qui s'éteint, le serveur qui redémarre ou s'arrête |
| **C12** à **C15** | « Mes ordinateurs » : la carte du compte, la session par le rendez-vous, le refus quand l'hôte n'est pas prêt, le journal à distance |
| **C16** à **C20** | Renommer, révoquer en moins d'une minute, se rattacher de nouveau, se détacher, et la ligne de commande |
| **I1** à **I4** | **Internet, le vrai test** : le client sur un partage de connexion 4G, l'hôte à la maison, avec puis sans port renvoyé sur la box ; ce que le journal dit du chemin ; la carte qui dit par où passe la session |
| **V7** à **V9** | Le serveur, la suite : mise à jour, sauvegarde, désinstallation en deux paliers |

### Confirmé

Ce qui a été essayé sur les vraies machines et dit tel quel. La colonne de droite reprend ce qui a été dit, pour qu'on puisse juger de la force de la confirmation.

| Essai | Ce qui a été dit |
|---|---|
| | |

---

## Avant de commencer

### Mettre à jour les deux PC

Sur **les deux PC**, la même ligne qu'au jalon M4, dans une fenêtre PowerShell **administrateur** placée dans le dossier du projet :

```
taskkill /IM ZyrDesk.exe /F 2>$null; .\target\release\zyrdeskd stop; git pull && cargo build --release && pwsh -NoProfile -ExecutionPolicy Bypass -File .\packaging\engines\fetch-engines.ps1 && .\target\release\zyrdeskd start
```

**Les deux ordinateurs doivent être à jour** : la fenêtre et le service parlent un dialecte nouveau, et un ordinateur en retard ne saurait ni rattacher ni montrer le compte. Les moteurs ne changent pas à cette tranche.

### Installer le serveur

Sur le conteneur, en suivant [server/README.md](../../server/README.md), avec le binaire du flux de travail « Serveur » (`--binary`) ou en compilant sur place (`--from-source`). Répondre **auto-signé** au chiffrement, pour que l'épinglage soit essayé, et **sur invitation** aux inscriptions. Noter, du panneau vert : l'**adresse à taper**, l'**empreinte du serveur**, le **code d'invitation**.

Rien à ouvrir sur les PC : le port UDP 47000 du tunnel l'est déjà depuis M3, et le compte ne passe que par des connexions sortantes vers le serveur. Rien à renvoyer sur la box tant que les deux PC et le serveur sont sur le même réseau ; pour la partie Internet (I1 à I4), le panneau vert dit les deux ports du serveur à renvoyer vers le conteneur, TCP pour l'API et UDP pour le miroir.

### Lancer l'application

Sur **les deux PC** : double-clic sur `target\release\ZyrDesk.exe`, jamais depuis la fenêtre administrateur.

---

## Partie 1 : le serveur, sur le conteneur

### V1. L'installation va jusqu'au panneau vert

`bash install.sh` pose ses questions avec des défauts entre crochets, montre un récapitulatif, demande « Lancer l'installation maintenant ? », puis déroule ses étapes, chacune réécrite en `✓`. À la fin, un panneau vert « Serveur ZyrDesk installé » avec l'adresse à taper, l'empreinte en huit groupes de huit caractères, les ports à renvoyer, la configuration, les données, le code d'invitation et la ligne sur les clés.

**Attendu :** aucune étape en `✗`. Si l'une échoue, la sortie de la commande fautive s'affiche sous elle, et c'est elle qu'il faut recopier. Les ports à renvoyer sont deux : TCP pour l'API, UDP pour le miroir, même avec le relais répondu non.

### V2. `zyrdesk-server status`

```
zyrdesk-server status
```

**Attendu :** la ligne de version, le nom et l'adresse du serveur, « en marche, l'API répond », un compte, zéro appareil, les inscriptions « sur invitation », le dossier des données.

### V3. `zyrdesk-server check`

```
zyrdesk-server check
```

Le serveur se joint lui-même comme un appareil le ferait, avec la même vérification du certificat.

**Attendu :** « Le serveur répond sur … , en TLS. », son nom, sa version et son dialecte, les inscriptions, l'**empreinte du serveur**, identique à celle du panneau vert et à celle de `zyrdesk-server fingerprint`, et « Miroir : répond sur UDP 443, cette question venait de 127.0.0.1:… ». Un miroir absent est dit tel quel, avec le journal à lire : c'est un port UDP déjà pris par un autre programme.

### V4. Le clair est refusé

À ne faire que si l'on est à l'aise avec un éditeur de texte ; sinon passer, c'est vérifié par un test automatique.

```
cp /etc/zyrdesk-server/server.toml /root/server.toml.avant
```

Dans `/etc/zyrdesk-server/server.toml`, sous `[api]`, remplacer la ligne `listen` par `listen = "0.0.0.0:8080"` et retirer les deux lignes `tls_cert` et `tls_key`. Puis :

```
systemctl restart zyrdesk-server ; sleep 2 ; systemctl status zyrdesk-server --no-pager ; journalctl -u zyrdesk-server -n 5 --no-pager
cp /root/server.toml.avant /etc/zyrdesk-server/server.toml && systemctl restart zyrdesk-server && zyrdesk-server check
```

**Attendu :** le service ne démarre pas, et le journal dit en une phrase qui commence par « api.listen = 0.0.0.0:8080 sans certificat TLS : le serveur ne parle jamais en clair » pourquoi. Après la remise en place, `check` répond de nouveau.

### V5. Le journal

Dans une fenêtre du conteneur laissée ouverte pour toute la suite :

```
journalctl -u zyrdesk-server -f
```

**Attendu :** une ligne par événement, jamais de mot de passe, de jeton ni d'adresse de candidat. Au fil du protocole on y lira `device attached: … on account …`, `device … of … is online, build …`, `session …: … towards …`, `device revoked: …`.

### V6. Le script relancé

```
bash install.sh
```

**Attendu :** un panneau « Un serveur ZyrDesk est déjà installé » avec la version, la configuration et l'état du service, puis un menu : mettre à jour, reconfigurer, afficher l'état, désinstaller, ne rien faire. Choisir « Reconfigurer » : chaque question propose en défaut la réponse de la première fois ; tout valider par Entrée, sauf le mot de passe qui est redemandé, et le serveur redémarre à l'identique, `check` compris. Ne pas choisir « Désinstaller » avant la fin du protocole.

---

## Partie 2 : rattacher les deux PC

### C1. La section Compte sans lien

Sur le **PC hôte**, Réglages, section **Compte**, sous « Démarrer avec Windows ».

**Attendu :** « Aucun compte » et un bouton **Se connecter à un serveur**. Si le service est arrêté, la section dit à la place « Le service ne répond pas : le compte se lit quand il tourne. »

### C2. La fenêtre de rattachement

Cliquer **Se connecter à un serveur**.

**Attendu :** une fenêtre « Se connecter à un serveur ZyrDesk », deux segments **J'ai un compte** et **Créer un compte**, et les champs **Serveur**, **Utilisateur**, **Mot de passe** (masqué) et **Nom de cet ordinateur**, pré-rempli du nom que Windows donne à la machine. Avec **Créer un compte**, deux champs de plus : **Courriel** et **Code d'invitation (si le serveur en demande un)**. Le bouton **Se connecter** reste éteint tant que les trois premiers champs ne sont pas remplis. Tab passe d'un champ au suivant, Entrée vaut le bouton principal, Échap ferme.

### C3. Les refus parlent

Dans la fenêtre : l'adresse du serveur, le nom du premier compte, un mot de passe **faux**, **Se connecter**.

**Attendu :** un bandeau rouge sous les boutons : « nom d'utilisateur ou mot de passe incorrect ». Puis l'adresse écrite avec `http://` devant : « un serveur ZyrDesk ne se joint qu'en https:// : l'adresse en http:// est refusée ». La fenêtre reste ouverte, les champs gardent leur contenu.

### C4. L'empreinte à comparer, puis le rattachement

Le bon mot de passe, l'adresse sans schéma, **Se connecter**.

**Attendu :** le bouton dit « Connexion… » un instant, puis un panneau orange apparaît dans la fenêtre : « Ce serveur présente un certificat que personne ne garantit. Comparez cette empreinte avec celle que son installation a affichée. Si c'est bien la même, continuez : elle sera retenue, et un serveur qui en présenterait une autre serait refusé. » Dessous, l'empreinte en caractères à chasse fixe ; le serveur l'affiche en huit groupes, la fenêtre d'un seul tenant, ce sont les mêmes soixante-quatre caractères. Le bouton principal devient **C'est bien lui, continuer**. Le cliquer : la fenêtre se ferme et la ligne d'annonce dit « Cet ordinateur est rattaché au compte. »

Dans `service.log` de l'hôte, dans l'ordre : `the server at … presents a key nobody vouches for (…), the person is asked`, puis `this computer is attached to … as … under the name « … »`, puis `account link with … as …, this computer is device … there`. Sur le serveur (V5) : `device attached: …` puis `device … is online`.

### C5. Le second PC, même compte

Sur le **PC client**, C1 à C4 avec le même compte.

**Attendu :** le panneau orange revient, parce que chaque appareil compare et retient l'empreinte pour son propre compte ; puis « Cet ordinateur est rattaché au compte. »

### C6. Un compte créé depuis la fenêtre

À faire une fois, sur le **PC client** après s'être détaché (C19), ou sur un troisième ordinateur. Dans la fenêtre, **Créer un compte** : un nom nouveau, un mot de passe de douze caractères au moins, le **code d'invitation** du panneau vert, **Créer le compte**.

**Attendu :** le compte est créé et l'ordinateur rattaché dans le même geste. Le même code une seconde fois : « ce code d'invitation n'est pas valable ». Un mot de passe trop court : « le mot de passe doit faire douze caractères au moins ». Sur le serveur, `zyrdesk-server user list` montre les deux comptes. Pour la suite du protocole, se détacher et se rattacher au premier compte.

---

## Partie 3 : la section Compte

### C7. Le lien et les appareils

Sur les **deux PC**, Réglages, section Compte.

**Attendu :** « *utilisateur* sur *nom du serveur* », dessous une pastille verte et « *adresse* · relié », et à droite le bouton **Se détacher**. Puis **Appareils du compte** : les deux ordinateurs, chacun avec sa pastille et sa ligne de présence, celui-ci suivi de « · cet ordinateur » et sans bouton Révoquer, l'autre avec **Renommer** et **Révoquer**. La ligne de présence de l'autre dit « En ligne · prêt à être contrôlé » si son accès distant est allumé, « En ligne · accès distant désactivé » sinon.

### C8. La présence suit l'accès distant

Sur le **PC hôte**, éteindre **Accès distant** dans les réglages. Regarder la section Compte du **PC client**.

**Attendu :** en deux secondes au plus, la ligne de l'hôte passe à « En ligne · accès distant désactivé » et sa pastille en orange. Rallumer : « En ligne · prêt à être contrôlé », pastille verte. Dans `service.log` de l'hôte : `the server is told this computer's remote access is Off`, puis `… is Ready`. Pendant que le moteur hôte démarre, la ligne dit « démarrage en cours » ; sans moteur, « moteur hôte absent ».

### C9. L'hôte s'éteint

Sur le **PC hôte**, arrêter le service : `.\target\release\zyrdeskd stop` dans une fenêtre administrateur, ou redémarrer la machine.

**Attendu :** sur le **PC client**, la ligne de l'hôte passe en quelques secondes à « Hors ligne · vu il y a moins d'une minute », pastille grise. Remettre le service (`zyrdeskd start`) : « En ligne » revient de lui-même, sans rien toucher, dès que le service a rouvert son canal. Si c'est le câble réseau qui est débranché plutôt que le service arrêté, le serveur met jusqu'à une minute et demie à le constater : c'est le silence qu'il tolère avant de fermer un canal muet.

### C10. Le serveur redémarre

Sur le conteneur :

```
systemctl restart zyrdesk-server
```

**Attendu :** sur les deux PC, la pastille du lien passe en orange avec « injoignable : … » et la raison, puis revient à « relié » sans rien faire, en quelques secondes la première fois, chaque nouvel essai attendant un peu plus longtemps que le précédent, jusqu'à deux minutes au plus si le serveur restait longtemps absent.

### C11. Le serveur arrêté ne casse rien

```
systemctl stop zyrdesk-server
```

Sur le **PC client**, ouvrir une session vers le PC hôte par sa carte de « Mes ordinateurs », comme au jalon M4.

**Attendu :** le lien dit « injoignable », et la session s'ouvre exactement comme avant : sur un réseau local, rien ne dépend du serveur. Le refermer, puis `systemctl start zyrdesk-server` ; les deux liens reviennent à « relié ».

---

## Partie 4 : « Mes ordinateurs »

### C12. La carte du compte

Sur le **PC client**, l'accueil.

**Attendu, sur le même réseau local :** la carte du PC hôte est celle de toujours, avec sous son nom « *adresse* · compte » : vu par le réseau **et** connu du compte, une seule carte, jamais deux. Sa pastille suit le réseau local.

**Attendu, quand le réseau local ne montre pas l'hôte** (le PC client sur un autre sous-réseau, ou dehors et relié à la maison par un VPN qui ne porte pas la découverte) : la carte vient du compte seul, et dit sous son nom « en ligne · prêt à être contrôlé · compte », pastille verte ; « en ligne · accès distant désactivé · compte » en orange ; « hors ligne · compte » en gris. Un appareil hors ligne reste sur l'accueil : c'est le compte qui le montre, pas le réseau.

### C13. La session par le rendez-vous

Dans la situation où le réseau local ne montre pas l'hôte (C12, second attendu) : cliquer la carte. Sur deux sous-réseaux de la maison, ou par un VPN, c'est une adresse locale de l'hôte qui répondra ; depuis Internet, c'est la partie I qui le dit.

**Attendu :** la session s'ouvre, une seconde ou deux de plus qu'en réseau local, le temps de demander au serveur et de sonder. Dans `service.log` du client : `asking the server for a session towards … (…)`, `session … matched with … (…), which will say where it answers`, `opening a way to … through session …, probing what both sides name`, puis `card 240.…: reached through …, … ms` et `… answered through … after … ms`. Dans `service.log` de l'hôte : `… (…) is presented by the server for session …, and may come in for as long as the ticket lives`, puis `card 240.…: reached through …` et `session open with 240.… through …, round trip … ms`. Sur le serveur : `session …: … (…) towards … (…)`. À la fermeture, côté client : `its way closed, the server is told session … is over`.

Si aucune adresse annoncée n'a répondu au bout de quinze secondes, l'ouverture échoue avec « … n'a répondu par aucune des adresses annoncées en 15 secondes », et rien ne reste ouvert.

### C14. Le refus quand l'hôte n'est pas prêt

Accès distant **éteint** sur le PC hôte ; sur le PC client, cliquer sa carte de compte (carte du second attendu de C12).

**Attendu :** l'ouverture est refusée tout de suite, sans rien demander au serveur, avec « *nom de l'hôte* n'accepte pas l'accès distant en ce moment : accès distant désactivé » ; « … : moteur hôte absent » si l'hôte n'a pas de moteur ; et « *nom de l'hôte* n'est pas connecté au serveur en ce moment » si l'hôte est éteint. Sur le même réseau local, la carte est celle du réseau et le refus est celui de toujours.

### C15. Le journal à distance

Sur la carte de compte du PC hôte, le bouton **Journal**.

**Attendu :** la même fenêtre qu'au jalon M4, remplie de ce que l'hôte a écrit, avec **Vider** ; la demande passe par le rendez-vous quand la carte vient du compte seul, et par le réseau local sinon.

---

## Partie 5 : Internet, le vrai test

Le **PC client** quitte la maison : un portable sur le partage de connexion d'un téléphone (4G ou 5G), sans VPN. Le **PC hôte** reste à la maison, derrière la box, et le **serveur** aussi, avec ses deux ports renvoyés vers le conteneur. Les deux PC restent rattachés au même compte.

### I1. L'hôte se voit depuis dehors

Sur le portable, l'accueil et la section Compte.

**Attendu :** le lien dit « relié » par l'adresse publique du serveur ; la carte de l'hôte dit « en ligne · prêt à être contrôlé · compte », en vert. C'est la présence par le serveur, sans aucun chemin direct encore.

### I2. La session avec un port renvoyé chez l'hôte

Sur la box de la maison, renvoyer **UDP 47000** vers le PC hôte, comme au jalon M3 pour l'accès par adresse. Sur le portable, cliquer la carte de l'hôte.

**Attendu :** la session s'ouvre en quelques secondes. Dans `service.log` du portable, `… answered through <adresse publique de la maison>:47000 after … ms, round trip … ms`. Dans `service.log` de l'hôte, `card 240.…: reached through <adresse du téléphone>:…`. Pendant la session, l'aller-retour dans le menu de la session est celui d'un chemin 4G, quelques dizaines de millisecondes. Sur le serveur, `journalctl -u zyrdesk-server -f` ne montre que l'ouverture et la fin de la session : aucun octet de la session n'y passe, et le débit du conteneur reste nul pendant qu'on regarde l'image.

### I3. La session sans rien renvoyer chez l'hôte

Retirer le renvoi de UDP 47000 sur la box, redémarrer le service de l'hôte (`zyrdeskd stop` puis `start`, pour que la box oublie ses traductions), puis cliquer de nouveau la carte depuis le portable.

**Attendu, avec une box ordinaire à la maison :** la session s'ouvre quand même, par la perforation : l'hôte a sondé l'adresse du portable que le serveur lui a passée, la box de la maison a laissé revenir la réponse, et `service.log` du portable dit `… answered through <adresse publique de la maison>:<un port qui n'est pas 47000>`. **Attendu, si ça échoue :** « … n'a répondu par aucune des adresses annoncées en 15 secondes » au bout d'un quart de minute, et `service.log` du portable montre des `card 240.…` sans `reached`. C'est le cas de deux réseaux qui changent de port à chaque destination, fréquent en 4G ; il est écrit comme une limite de la tranche, et c'est le relais du jalon M6 qui le couvrira. Noter lequel des deux s'est produit, avec l'opérateur du téléphone et le modèle de box : c'est la mesure qui décidera de la priorité du mappage de port.

### I4. La carte dit par où passe la session

Pendant une session ouverte (I2 ou I3), sur le portable, l'accueil.

**Attendu :** la carte verte « Session en cours vers … » dit « Ouverte depuis …, par <adresse publique de la maison>:… en … ms. Fermer la fenêtre termine la session. » Sur une session de réseau local, la même ligne dit l'adresse locale et une ou deux millisecondes.

---

## Partie 6 : renommer, révoquer, se détacher

### C16. Renommer depuis l'autre ordinateur

Sur le **PC hôte**, Réglages, Compte, ligne du PC client, **Renommer** : une fenêtre « Renommer « *nom* » », le champ, **Renommer**.

**Attendu :** le nom change dans la liste des deux PC en quelques secondes, et sur la carte de l'accueil quand elle vient du compte. Sur le PC renommé, la section Compte le montre aussi, sans rien avoir fait dessus. Dans `service.log` de celui qui a renommé : `device … of the account renamed « … »`.

### C17. Révoquer en moins d'une minute

Sur le **PC hôte**, ligne du PC client, **Révoquer** : le bouton devient **Confirmer** pendant quatre secondes ; le cliquer une seconde fois. Chronomètre en main.

**Attendu, sur le PC client, en moins d'une minute :** la section Compte revient à « Aucun compte », les cartes qui venaient du compte disparaissent de l'accueil, et `service.log` dit `this device was revoked from the account, the link is forgotten`. Sur le PC hôte, la ligne du client disparaît de la liste. Sur le serveur : `device revoked: …`. Une session déjà ouverte n'est pas coupée par la révocation : couper une session en cours vient avec les partages, à la tranche 3.

### C18. Se rattacher de nouveau

Sur le **PC client**, C4 de nouveau.

**Attendu :** le panneau orange revient, l'empreinte ayant été oubliée avec le lien ; puis « Cet ordinateur est rattaché au compte. » L'appareil réapparaît dans la liste des deux PC, sous un identifiant nouveau : `zyr-cli account status` le montre.

### C19. Se détacher

Sur le **PC client**, Réglages, Compte, **Se détacher**, puis **Confirmer** dans les quatre secondes.

**Attendu :** « Aucun compte », les cartes du compte s'en vont, et sur le PC hôte l'appareil disparaît de la liste : se détacher, c'est se révoquer soi-même, et le serveur l'écrit comme une révocation. Se rattacher ensuite (C4) pour la suite.

### C20. La ligne de commande

Sur l'un des PC, dans une fenêtre de commandes ordinaire :

```
.\target\release\zyr-cli account status
.\target\release\zyr-cli account devices
```

**Attendu :** « Compte : *utilisateur* sur *nom du serveur* (*adresse*) », l'identifiant de cet appareil, « Canal vivant : relié » ; puis la liste des appareils, identifiant, nom, présence, « (cet ordinateur) » sur le bon. Sans lien : « Aucun compte : cet ordinateur ne connaît aucun serveur. » Ce sont les mêmes réponses du service que celles que la fenêtre montre.

---

## Partie 7 : le serveur, la suite

### V7. Mettre à jour

Avec un binaire plus récent du flux de travail « Serveur », déposé sur le conteneur :

```
bash install.sh --binary ./zyrdesk-server-x86_64-linux-musl
```

Choisir « Mettre à jour ».

**Attendu :** arrêt, installation, redémarrage, « Le serveur répond », puis « Mis à jour en *version* ». Les comptes et les appareils sont toujours là (`status`), et les deux PC reviennent à « relié » seuls (C10).

### V8. Sauvegarder

```
runuser -u zyrdesk -- zyrdesk-server backup /var/lib/zyrdesk-server/sauvegarde
ls -l /var/lib/zyrdesk-server/sauvegarde /var/lib/zyrdesk-server/sauvegarde/keys
```

**Attendu :** « Sauvegarde écrite dans … », avec `zyrdesk.db`, `server.toml` et le dossier `keys` contenant `signing.key`. Le serveur n'a pas cessé de répondre pendant ce temps.

### V9. Désinstaller, en deux paliers

Tout à la fin, et seulement si l'on veut repartir de zéro :

```
bash install.sh
```

Choisir « Désinstaller ».

**Attendu :** un panneau orange qui dit ce que fait le premier palier ; « Retirer le service et le programme ? », dont le défaut est non ; puis un second panneau qui dit que les données, les clés et la configuration seront effacées et que les appareils rattachés perdront leur compte, et une confirmation à taper en toutes lettres. Après le premier palier seul, les données restent et relancer le script réinstalle le programme par-dessus. Après le second, les deux PC disent « injoignable », et il faut **Se détacher** sur chacun.

---

## Si quelque chose ne va pas

- **« Le service ne répond pas »** dans la section Compte : le service n'est pas démarré sur ce PC. `zyrdeskd start` en administrateur, ou le bouton de la fenêtre.
- **« injoignable : … »** sous le lien : la raison est écrite après les deux points. Sur le serveur, `zyrdesk-server check` dit si c'est lui ; sinon c'est le chemin entre les deux, adresse, DNS ou box.
- **L'empreinte du panneau orange n'est pas celle du serveur** : ne pas continuer. Quelqu'un est entre les deux, ou le serveur a été réinstallé sans ses clés (`zyrdesk-server fingerprint` fait foi).
- **La carte du compte ne s'ouvre pas** alors que l'hôte est « en ligne · prêt » : lire `service.log` du client. `matched with …` suivi de « n'a répondu par aucune des adresses annoncées » veut dire que les sondes n'ont trouvé aucun chemin entre les deux box : renvoyer UDP 47000 vers l'hôte sur sa box (I2), et vérifier que le port UDP du serveur est bien renvoyé, sans quoi ni l'un ni l'autre n'apprend son adresse vue de l'extérieur. Sans `matched` au bout de dix secondes, c'est l'hôte qui n'a pas répondu au serveur : son `service.log` le dit.
- **Les délais à connaître :** la présence change en deux secondes ; un hôte qui disparaît sans prévenir est vu hors ligne en une minute et demie au plus ; une révocation arrive en moins d'une minute ; le lien se rouvre après un serveur redémarré au bout de quelques secondes, puis d'un peu plus à chaque essai manqué, deux minutes au plus.
