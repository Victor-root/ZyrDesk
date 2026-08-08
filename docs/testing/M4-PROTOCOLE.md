# Jalon M4 : le produit se pilote, et les moteurs deviennent les nôtres

Ce document se remplit au fur et à mesure du jalon. Chaque partie se teste dès qu'elle est écrite, sur les deux mêmes PC Windows qu'aux jalons précédents.

Vocabulaire : **PC hôte** = celui qu'on contrôle. **PC client** = celui depuis lequel on se connecte.

**Deux choses séparées à tenir à jour, sur les deux PC.** Les moteurs (téléchargés en artefact et décompressés dans `data\engines\...`) sont une chose. Le programme ZyrDesk lui-même (`zyr-cli`, `zyrdeskd`) en est une autre, et se met à jour par :

```
git pull && cargo build --release && zyrdeskd stop && zyrdeskd install && zyrdeskd start && zyrdeskd status
```

Remplacer un moteur sans refaire cette commande laisse tourner l'ancien `zyr-cli`/`zyrdeskd` : les messages ne correspondront pas à ce que ce document décrit. Le faire sur les deux PC avant chaque partie évite cette confusion.

---

## Partie 1 : le service tient les sessions, la ligne de commande ne tient plus rien

### Ce qui change, et pourquoi

Jusqu'ici, la fenêtre de commande qui lançait `zyr-cli connect` portait elle-même le tunnel. La fermer, ou la perdre, coupait la session. C'est tenable pour un outil de diagnostic, pas pour un produit : l'interface graphique du jalon M4 doit pouvoir être fermée, mise à jour ou plantée sans que l'image s'arrête.

Le service porte donc maintenant les deux bouts du tunnel, y compris celui des sessions sortantes. La ligne de commande lui demande une voie, lance le lecteur sur les adresses locales qu'il lui rend, et lui indique quel processus cette voie sert. Le service referme la voie tout seul quand ce processus disparaît.

**Conséquence à connaître : le service est désormais nécessaire sur le PC client aussi.** C'était prévu (décision D2), c'est le prix d'une interface qu'on peut fermer sans conséquence.

### Préparation

Sur les **deux PC**, en fenêtre **administrateur** :

```
git pull && cargo build --release && zyrdeskd install && zyrdeskd start && zyrdeskd status
```

Attendu : « En marche » des deux côtés. Si le service tournait déjà, `zyrdeskd stop` d'abord.

> **M4-R1 (le service répond)**
>
> Sur le **PC client**, sans rien d'autre de lancé :
>
> ```
> zyr-cli connect 1.2.3.4 --pair 0000000000000000000000000000000000000000000000000000000000000000
> ```
>
> Attendu : un refus qui parle d'une adresse injoignable, **et surtout pas** « le service ZyrDesk ne tourne pas ». Ce dernier message voudrait dire que la ligne de commande n'atteint pas le service, et rien d'autre ne marchera.

### Session normale

Sur le **PC client**, comme au jalon M3 :

```
zyr-cli connect <adresse IP du PC hote> --pair <empreinte du PC hote> --stats
```

> **M4-R2 (rien n'a régressé)**
>
> Attendu : le bureau du PC hôte s'affiche, exactement comme au jalon M3. La ligne « Taille de paquet » remplace l'ancienne, qui ne s'affichait que si le chemin l'avait réduite.
>
> Noter : la session s'ouvre-t-elle aussi vite ? L'image est-elle aussi fluide ?

### Le vrai test de cette partie

Session ouverte et bien en cours, **sur le PC client** : fermer la fenêtre de commande d'où est parti `zyr-cli connect`, à la croix, sans rien arrêter d'autre.

> **M4-R3 (la session survit à la fenêtre)**
>
> Attendu : **l'image continue, sans coupure ni saccade**. La souris et le clavier répondent toujours. C'est le critère principal de cette partie.
>
> Fermer ensuite la session normalement (la combinaison de touches habituelle du lecteur, ou la croix de la fenêtre vidéo).

Puis, toujours sur le **PC client** :

```
notepad data\logs\service.log
```

> **M4-R4 (la voie se referme toute seule)**
>
> Attendu, dans l'ordre, vers la fin du journal : `way 1 open towards ...`, puis `way 1 now serves process ...`, puis, après la fermeture du lecteur, `way 1 has nothing left to serve` suivi de `way 1 closed`.
>
> C'est ce qui garantit qu'une fenêtre fermée brutalement ne laisse pas un tunnel ouvert derrière elle. Sans ces lignes, chaque session abandonnée laisserait une fuite.

### Si quelque chose ne va pas

**« le service ZyrDesk ne tourne pas ».** Le service n'est pas démarré sur le PC **client** (`zyrdeskd status`), ou il n'a jamais été installé de ce côté. C'est nouveau à ce jalon.

**Le journal du service dit `control channel unavailable`.** Le service tourne mais n'a pas pu ouvrir son canal de commande. Un autre programme occupe le nom `\\.\pipe\ZyrDesk`, ou un service ZyrDesk plus ancien tourne encore. Vérifier qu'il n'y a qu'un seul `zyrdeskd.exe` dans le gestionnaire des tâches.

**« réponse incompréhensible du service ».** Les deux moitiés du produit ne datent pas du même jour : le service tourne sur un exécutable plus ancien que `zyr-cli`. Recompiler et relancer le service.

**La session se coupe quand même en fermant la fenêtre.** Regarder si un avertissement est apparu au lancement (« le service n'a pas pris la session en charge »). Il signifie que le service n'a pas pu être prévenu du processus à surveiller, et que la session reste attachée à la fenêtre.

---

## Partie 2 : le moteur client devient le nôtre

### Ce qui change, et pourquoi

Jusqu'ici le lecteur était celui du projet d'origine, récupéré tel quel. Il se nommait, s'affichait et se comportait comme lui : son nom dans le gestionnaire des tâches, son icône dans la barre des tâches, son titre de fenêtre, et surtout sa fenêtre de chargement, qui s'affichait une seconde avant l'image à chaque session.

Il est maintenant compilé par nous, à partir de la même version, avec trois modifications et rien d'autre : il porte le nom et l'icône de ZyrDesk, il ne montre plus aucune fenêtre à lui avant l'image, et il dit en s'arrêtant ce qui s'est passé. Ce dernier point compte : jusqu'ici il annonçait une réussite même quand la session avait échoué, ce qui rendait toute reprise automatique impossible.

### Préparation

Sur le **PC client** seulement, le moteur hôte n'étant pas concerné à ce stade.

1. Mettre à jour le programme ZyrDesk lui-même (voir en haut de ce document).
2. Sur GitHub, onglet **Actions**, workflow **Moteurs**, ouvrir la dernière exécution réussie.
3. Télécharger l'artefact `zyrdesk-client-engine`.
4. Décompresser son contenu **par-dessus** `data\engines\client\`, en remplaçant tout.
5. Vérifier :

```
zyr-cli engines status
```

Attendu : les deux moteurs présents.

> **M4-R5 (plus aucune trace de l'autre projet)**
>
> Ouvrir `data\engines\client\` dans l'explorateur, clic droit sur `zyrdesk-session.exe`, **Propriétés**, onglet **Détails**.
>
> Attendu : nom du produit, description et société parlent de ZyrDesk, et l'icône du fichier est le logo. Aucun de ces champs ne doit nommer l'autre projet.

### Le vrai test de cette partie

Sur le **PC client**, session normale :

```
zyr-cli connect <adresse IP du PC hote> --pair <empreinte du PC hote>
```

> **M4-R6 (rien ne s'affiche avant l'image)**
>
> Attendu : entre la commande et le bureau du PC hôte, **aucune fenêtre intermédiaire**, pas même un instant. La seule fenêtre qui s'ouvre est celle de l'image.
>
> Regarder aussi la barre des tâches et le gestionnaire des tâches pendant la session : le programme doit s'y appeler ZyrDesk et porter le logo.
>
> Noter : quelque chose clignote-t-il encore au lancement ? .......................................

L'appairage passe par un deuxième chemin, qui a sa propre fenêtre chez le moteur. Le forcer :

```
zyr-cli connect <adresse IP du PC hote> --pair <empreinte du PC hote> --pair-again
```

Un code à quatre chiffres s'affiche et la commande attend. **Sans fermer cette fenêtre**, sur le **PC hôte** : `zyr-cli host pin <le code>`.

> **M4-R6b (l'appairage non plus n'affiche rien)**
>
> Attendu : pendant toute l'attente du code, **aucune fenêtre du moteur** ne s'ouvre sur le PC client. La commande dit « Autorisé. » puis enchaîne directement sur l'image.
>
> Ce chemin avait été oublié au premier passage : il montrait la fenêtre d'appairage du projet d'origine, code compris.

### Ce que le moteur raconte au reste du monde

Trouvé en cherchant les traces visibles : le moteur se présentait ailleurs que sur l'écran. Ces trois vérifications se font **pendant une session en cours**, sur le PC client.

> **M4-R6c (le son est au nom du produit)**
>
> Clic droit sur l'icône de volume de Windows, **Mélangeur de volume**.
>
> Attendu : le programme qui joue le son de la session s'appelle ZyrDesk.

> **M4-R6d (rien n'est annoncé à personne)**
>
> Si Discord est installé et lancé sur le PC client, regarder son propre profil.
>
> Attendu : **aucune activité** affichée. Avant ce correctif, une session apparaissait à tes contacts comme une partie du projet d'origine, avec son nom et son icône. C'était actif par défaut.

> **M4-R6e (le moteur ne sort pas du tunnel)**
>
> Plus difficile à voir à l'oeil : le moteur téléchargeait deux fichiers sur le site du projet d'origine à chaque session. Le contrôle indirect est que la session démarre normalement **sans connexion Internet**, les deux PC restant sur le réseau local.
>
> Noter : la session démarre-t-elle aussi vite sans Internet ? ....................................

### Les messages ne doivent pas avoir disparu avec la fenêtre

C'est le risque de ce changement : une fenêtre en moins peut vouloir dire une erreur qu'on ne voit plus. Les messages partent maintenant dans la fenêtre de commande et dans `data\logs\session.log`.

Sur le **PC hôte**, arrêter tout le service (`zyrdeskd stop`), ce qui coupe l'accès distant entièrement, puis, sur le **PC client**, relancer la même commande `connect`.

> **M4-R7 (la machine injoignable se dit)**
>
> Attendu : après une trentaine de secondes d'attente, la commande s'arrête sur **« l'ordinateur distant n'a pas répondu »**, avec le chemin du journal.
>
> Le message ne doit surtout pas parler de session terminée normalement.

Relancer `zyrdeskd start` sur le **PC hôte**, ouvrir une session depuis le **PC client**, puis, une fois l'image affichée, tuer le processus `zyrdesk-host-engine.exe` depuis le gestionnaire des tâches du PC hôte.

> **M4-R8 (une session qui casse se dit aussi)**
>
> Attendu : l'image s'arrête, et la commande sur le PC client s'arrête sur **« la session s'est arrêtée sur une erreur »**, avec le chemin du journal.
>
> Refermer enfin une session normalement (Ctrl+Alt+Maj+Q, ou la croix de la fenêtre vidéo) : cette fois le message doit être **« Session terminée. »**. Ces trois fins doivent se distinguer, c'est tout l'intérêt du changement.

### Si quelque chose ne va pas

**Une fenêtre s'affiche encore avant l'image.** Le moteur en place est l'ancien : l'artefact n'a pas remplacé le contenu de `data\engines\client\`, ou il a été décompressé dans un sous-dossier. Vérifier la date de `zyrdesk-session.exe`.

**Le moteur ne démarre pas du tout, sans message.** Il manque une bibliothèque à côté de lui, presque toujours parce que l'artefact a été décompressé en partie. Tout supprimer dans `data\engines\client\` et recommencer.

**« le moteur s'est arrêté sans dire pourquoi ».** Le moteur s'est arrêté sur un code qu'il ne prévoit pas, ce qui veut dire un plantage. Le journal `data\logs\session.log` est alors la seule piste.

**Toutes les fins donnent « Session terminée. ».** Le moteur en place est l'ancien, comme au premier point.

---

## Partie 3 : le moteur hôte devient le nôtre

### Ce qui change, et pourquoi

C'est le programme que tu voyais s'appeler Sunshine dans le gestionnaire des tâches du PC hôte. Renommer son fichier n'y changeait rien : Windows n'affiche pas le nom du fichier, il affiche le nom de produit inscrit à l'intérieur du binaire, qui n'est posé qu'à la compilation.

Il est donc compilé par nous à son tour, à partir de la même version épinglée. Le patch tient en huit lignes et ne nomme même pas ZyrDesk : le moteur laissait déjà choisir son icône et son éditeur, il manquait juste le nom de produit. Notre nom est passé par notre script de compilation, ce qui veut dire que la marque ne vit pas dans le moteur.

Rien d'autre ne change dans son comportement.

### Préparation

Sur le **PC hôte** cette fois.

1. Mettre à jour le programme ZyrDesk lui-même (voir en haut de ce document).
2. Sur GitHub, onglet **Actions**, workflow **Moteurs**, ouvrir la dernière exécution réussie.
3. Télécharger l'artefact `zyrdesk-host-engine`.
4. Arrêter le service : `zyrdeskd stop`.
5. Vider `data\engines\host\`, puis y décompresser le contenu de l'artefact.
6. Vérifier :

```
zyr-cli engines status && zyrdeskd start
```

Attendu : les deux moteurs présents, service en marche.

> **M4-R9 (le fichier porte notre nom)**
>
> Dans `data\engines\host\`, clic droit sur `zyrdesk-host-engine.exe`, **Propriétés**, onglet **Détails**.
>
> Attendu : nom du produit, description et société parlent de ZyrDesk, et l'icône du fichier est le logo.

### Le vrai test de cette partie

Ouvrir une session depuis le **PC client**, puis, **sur le PC hôte**, ouvrir le gestionnaire des tâches et regarder l'onglet **Détails**.

> **M4-R10 (plus rien ne s'appelle Sunshine)**
>
> Attendu : le processus s'appelle `zyrdesk-host-engine.exe`, et sa colonne **Description** dit ZyrDesk. Chercher « sun » dans la liste ne doit plus rien donner.
>
> Noter : reste-t-il quelque chose au nom de l'autre projet ? ....................................

### Rien ne doit avoir régressé

Le moteur est le même, mais il sort d'une chaîne de compilation différente de celle des binaires officiels. C'est le seul vrai risque de cette partie.

> **M4-R11 (la session est aussi bonne qu'avant)**
>
> Avec `--stats`, comparer à ce que tu avais au jalon M3 : cadence d'images, latence, fluidité de la souris, son.
>
> Attendu : aucune différence perceptible. Vérifier aussi le déverrouillage de session à distance et l'invite UAC, qui dépendent de la capture d'écran sécurisée.
>
> Noter : cadence ......... latence ......... quelque chose de moins bon ? ........................

### Si quelque chose ne va pas

**Le moteur hôte ne démarre pas.** Regarder `data\logs\engine.log` et `data\logs\engine-console.log`. S'il manque un fichier d'image ou de shader, c'est que l'artefact a été décompressé en partie : le dossier `assets` doit être présent à côté de l'exécutable.

**Le gestionnaire des tâches dit encore Sunshine.** Le binaire en place est l'ancien, renommé à la main. Vérifier la date de `zyrdesk-host-engine.exe`.

**La capture est saccadée alors qu'elle ne l'était pas.** Noter précisément dans quelles conditions et repasser au binaire officiel renommé pour comparer : c'est le seul moyen de dire si la chaîne de compilation y est pour quelque chose.

---

## Partie 4 : la fenêtre, première tranche

### Ce qui change, et pourquoi

C'est le début de l'application, celle qui remplacera la ligne de commande. Cette première tranche pose deux choses : le design system, qui décide une fois pour toutes des couleurs, des espacements et du rythme de l'ensemble, et l'accueil, qui montre cet ordinateur et permet d'en joindre un autre.

Ce n'est pas encore le produit fini. Ce qui manque et qui viendra ensuite : l'interrupteur d'accès distant est affiché mais pas encore actionnable (le service héberge en permanence pour l'instant), il n'y a pas de liste d'ordinateurs connus, pas de réglages, et rien ne se rattache à une session déjà en cours.

Ce qui marche : ouvrir une session complète, appairage compris, sans taper une seule commande.

### Préparation

Sur les **deux PC**, la mise à jour habituelle :

```
git pull && cargo build --release && zyrdeskd stop && zyrdeskd install && zyrdeskd start && zyrdeskd status
```

La compilation sera plus longue que d'habitude cette fois : l'application amène ses propres dépendances, une seule fois.

### Ouvrir la fenêtre

Sur les **deux PC** :

```
.\target\release\ZyrDesk.exe
```

> **M4-R12 (la fenêtre s'ouvre et se reconnaît)**
>
> Attendu : une fenêtre sombre s'ouvre, avec le nom de l'ordinateur, une pastille verte et **« Prêt à être contrôlé »**. L'empreinte de la machine est affichée en dessous, en petit.
>
> Vérifier que la fenêtre porte bien le logo ZyrDesk dans la barre des tâches.
>
> Noter : le nom affiché est-il le bon ? ..........................................................

Puis, pour vérifier que la fenêtre dit la vérité et ne se contente pas d'afficher du texte : sur ce même PC, dans une fenêtre administrateur, `zyrdeskd stop`.

> **M4-R13 (la fenêtre suit l'état réel)**
>
> Attendu : en quelques secondes, **sans rien toucher**, la pastille passe au gris, le texte devient « Service arrêté » et un bandeau rouge apparaît.
>
> Relancer `zyrdeskd start` : tout revient au vert de la même façon.

### Le vrai test de cette partie

Sur le **PC client**, dans la fenêtre ZyrDesk : recopier l'adresse du PC hôte, puis son empreinte, qui est affichée dans la fenêtre ZyrDesk **du PC hôte** (elle se sélectionne à la souris). Cliquer sur **Se connecter**.

> **M4-R14 (une session sans ligne de commande)**
>
> Attendu : la fenêtre annonce le tunnel, puis la session démarre et l'image apparaît. Aucune commande n'a été tapée.
>
> Le bouton reste éteint tant que l'adresse est vide ou que l'empreinte n'a pas la bonne longueur : c'est voulu.

Pour vérifier le chemin de l'appairage, sur le **PC client** : supprimer le dossier `data\devices`, puis recommencer.

> **M4-R15 (le code d'appairage s'affiche dans la fenêtre)**
>
> Attendu : un code à quatre chiffres apparaît en grand dans la fenêtre, avec la consigne. Sur le **PC hôte** : `zyr-cli host pin <le code>`. La fenêtre enchaîne toute seule sur la session.
>
> Cette dernière commande disparaîtra quand l'hôte aura sa propre fenêtre pour accepter.

Enfin, session ouverte : fermer la fenêtre ZyrDesk à la croix.

> **M4-R16 (la fenêtre n'est pas la session)**
>
> Attendu : **l'image continue**. C'est tout l'intérêt du montage : l'interface peut être fermée, mise à jour ou plantée sans que la session s'arrête.

### Si quelque chose ne va pas

**La fenêtre s'ouvre mais reste vide, ou tout est figé sur « … ».** La partie web n'a pas démarré. Appuyer sur F12 pour ouvrir les outils de développement et me donner ce qui est écrit en rouge dans l'onglet Console.

**« Le service ZyrDesk ne répond pas » alors qu'il tourne.** L'application et le service ne datent pas du même jour : recompiler les deux avec la commande de préparation.

**Rien ne se passe au clic sur Se connecter.** Même chose, F12 puis Console.

**La fenêtre s'ouvre sans logo.** Sans conséquence, mais à signaler.

---

## Partie 5 : les ordinateurs se trouvent seuls, et le thème obéit

### Ce qui change, et pourquoi

Deux ajouts à la fenêtre.

Les ZyrDesk allumés sur le même réseau apparaissent d'eux-mêmes, en cartes. Plus d'adresse ni d'empreinte à recopier : un clic sur une carte ouvre la session. C'est le service qui s'annonce et qui écoute, pas la fenêtre, pour qu'un ordinateur reste trouvable même quand personne n'a ouvert ZyrDesk dessus.

Et le thème suit le système par défaut, tout en pouvant être forcé au clair ou au sombre. Le choix est en haut à droite.

### Préparation

Sur les **deux PC**, la mise à jour habituelle, puis ouvrir `ZyrDesk.exe` des deux côtés.

> **M4-R17 (chacun voit l'autre)**
>
> Attendu : sur chaque PC, une carte portant le nom de l'autre, sa pastille verte et son adresse. Le PC lui-même n'apparaît jamais dans sa propre liste.
>
> Compter le temps entre l'ouverture de la deuxième fenêtre et l'apparition de la carte : ......... s

> **M4-R18 (un clic suffit)**
>
> Cliquer la carte sur le **PC client**.
>
> Attendu : la session s'ouvre, sans avoir rien tapé. Si les deux ordinateurs ne se connaissent pas encore, le code d'appairage s'affiche d'abord.

Puis, sur le **PC hôte**, `zyrdeskd stop`.

> **M4-R19 (un ordinateur éteint finit par disparaître)**
>
> Attendu : la carte disparaît de la liste du PC client. Immédiatement si le service s'est arrêté proprement, sinon au bout d'une minute et demie.
>
> C'est le délai voulu : un ordinateur débranché ou en veille ne dit pas au revoir, et une liste qui ne se vide jamais ne vaut rien.

### Le thème

> **M4-R20 (le thème suit, puis obéit)**
>
> Dans les paramètres Windows, basculer entre le mode clair et le mode sombre pendant que la fenêtre est ouverte.
>
> Attendu : la fenêtre bascule avec, **sans être relancée**, barre de titre comprise.
>
> Cliquer ensuite **Clair** puis **Sombre** en haut à droite : la fenêtre obéit et ignore le système. Fermer et rouvrir l'application : le choix est retenu. Revenir sur **Système** rend la main à Windows.

### Si quelque chose ne va pas

**Les cartes n'apparaissent pas.** L'annonce sur le réseau local est bloquée. Regarder `data\logs\service.log` : la ligne « announced on the local network as ... » doit y être. Si elle manque, le message juste après en dit la raison. Le pare-feu Windows peut aussi bloquer la découverte réseau si le réseau est déclaré « public » plutôt que « privé ».

**Une carte reste alors que l'ordinateur est éteint.** Attendre une minute et demie : c'est le délai avant d'oublier une machine qui ne répond plus.

**Un ordinateur apparaît deux fois.** À signaler avec les deux noms affichés : c'est un défaut, une machine ne doit occuper qu'une carte.

**La barre de titre reste sombre en thème clair.** À signaler.

---

## Partie 6 : l'interrupteur d'accès distant

### Ce qui change, et pourquoi

Jusqu'ici l'interrupteur était dessiné mais ne servait à rien : le service hébergeait en permanence, sans qu'on puisse dire non. Il fonctionne maintenant.

Deux choses valent d'être sues.

Le choix est **retenu** : il est écrit sur le disque et honoré au démarrage suivant. Un ordinateur qu'on a rendu injoignable ne redevient pas joignable tout seul le lendemain matin.

Et couper l'accès distant **n'arrête pas le service**. Celui-ci continue d'ouvrir les sessions sortantes, de répondre à la fenêtre et de découvrir le réseau. Seul le fait d'être contrôlable s'arrête. C'est voulu : un ordinateur peut vouloir en contrôler d'autres sans accepter de l'être.

**Ce qui n'est pas fait** : n'importe quelle personne connectée à la machine peut actionner cet interrupteur. La réserver aux administrateurs est prévue au jalon M5, où la décision ouverte O2 la porte déjà.

### Le test

Sur le **PC hôte**, fenêtre ZyrDesk ouverte, avec une session en cours depuis le PC client.

> **M4-R21 (couper coupe vraiment)**
>
> Cliquer l'interrupteur **Accès distant** pour le mettre sur non.
>
> Attendu : la pastille passe au gris, le texte devient « Accès distant désactivé », et **la session en cours s'arrête** côté client. Dans le gestionnaire des tâches du PC hôte, `zyrdesk-host-engine.exe` disparaît. `zyrdeskd.exe`, lui, reste.

> **M4-R22 (et empêche de revenir)**
>
> Depuis le **PC client**, tenter de se reconnecter en cliquant la carte.
>
> Attendu : la connexion échoue. C'est le tunnel qui n'a plus personne en face.

> **M4-R23 (le choix survit au redémarrage)**
>
> Redémarrer le **PC hôte** entièrement, puis rouvrir ZyrDesk dessus.
>
> Attendu : l'interrupteur est **toujours sur non**. C'est le point qui compte le plus de cette partie : un choix oublié au redémarrage rendrait la machine joignable sans que personne l'ait demandé.
>
> Vérifier au passage `data\preferences.conf` : il doit contenir `remote_access = no`.

> **M4-R24 (rallumer remarche)**
>
> Remettre l'interrupteur sur oui.
>
> Attendu : la pastille passe à l'orange (« Démarrage en cours… ») pendant quelques secondes, puis au vert. Une nouvelle session s'ouvre depuis le PC client.

> **M4-R25 (une panne n'est pas un choix)**
>
> Sur le **PC hôte**, avec l'accès distant sur oui : `zyrdeskd stop`.
>
> Attendu : le texte devient « Service arrêté » et l'interrupteur devient **inactionnable en restant sur oui**. Il ne doit pas sauter sur non : personne n'a pris cette décision, le service est simplement absent.

### Si quelque chose ne va pas

**L'interrupteur revient sur oui tout seul après un redémarrage.** Regarder `data\preferences.conf`. S'il est absent ou dit `yes`, l'écriture a échoué : le journal du service en donnera la raison.

**La session ne s'arrête pas quand on coupe.** Regarder `data\logs\service.log` : la ligne « remote access turned off, the engine is being stopped » doit y être.

**L'interrupteur ne réagit pas au clic.** Le service et la fenêtre ne datent pas du même jour : recompiler et relancer le service.
