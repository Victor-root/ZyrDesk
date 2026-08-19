# Jalon M4 : le produit se pilote entièrement à la souris

Ce document se déroule sur les deux mêmes PC Windows que les jalons précédents. Il ne demande **aucune ligne de commande en dehors de la mise à jour et d'un réglage Windows à faire une seule fois** : tout le reste se passe dans la fenêtre ZyrDesk.

Vocabulaire : **PC hôte** = celui qu'on contrôle. **PC client** = celui depuis lequel on se connecte. La plupart des étapes se font sur les deux, et c'est écrit à chaque fois.

Ce protocole remplace celui des versions précédentes, qui passait par `zyr-cli` et `zyrdeskd` à chaque étape. La ligne de commande existe toujours, mais c'est devenu un outil de diagnostic et non le chemin du produit.

---

## Avant de commencer

### Le programme qui récupère les moteurs, une fois pour toutes

Les moteurs ne se compilent pas sur la machine qui s'en sert : l'un veut MSYS2 et GCC, l'autre Qt et Visual Studio, et chacun prend près d'une heure. La CI les compile une fois, et un script les met en place. Comme le dépôt est privé, ce script passe par le programme GitHub officiel, qui a déjà les accès.

À faire une seule fois, sur **les deux PC** :

```
winget install --id GitHub.cli && gh auth login
```

Ensuite, les moteurs suivent la mise à jour ci-dessous et il n'y a plus jamais rien à télécharger à la main. Si tu préfères t'en passer, les artefacts du workflow « Moteurs » se décompressent dans `data\engines\host\` et `data\engines\client\` : le résultat est le même, et la fenêtre dit en clair ce qui manque.

### Mettre à jour ZyrDesk et les moteurs

Sur **les deux PC**, dans une fenêtre PowerShell **administrateur** placée dans le dossier du projet, une seule ligne :

```
taskkill /IM ZyrDesk.exe /F 2>$null; .\target\release\zyrdeskd stop; git pull && cargo build --release && pwsh -NoProfile -ExecutionPolicy Bypass -File .\packaging\engines\fetch-engines.ps1 && .\target\release\zyrdeskd start
```

Elle ferme l'application, arrête le service, récupère les changements, compile, met les moteurs à jour s'ils ont bougé, et remet le service en marche.

L'ordre compte : Windows refuse de remplacer un fichier qu'un programme tient encore ouvert, et compiler ou remplacer un moteur avant d'avoir arrêté échoue sur « Accès refusé ».

Les moteurs ne sont retéléchargés que s'ils ont changé, ce qui arrive une poignée de fois sur la vie du projet. Le journal de la fenêtre dit toujours de quelle compilation viennent ceux qui sont en place.

**La toute première fois**, le service n'existe pas encore : la ligne se réduit à `git pull && cargo build --release && pwsh -NoProfile -ExecutionPolicy Bypass -File .\packaging\engines\fetch-engines.ps1`, et c'est la fenêtre qui installera le service (vérification R2).

### Le réseau doit être privé

Windows classe chaque carte réseau en **privé** ou en **public**, et sur un réseau public il coupe la découverte : les deux ZyrDesk ne se verront jamais, quelles que soient les règles de pare-feu. Un portable en Wi-Fi hérite très souvent de « public » sans que personne ne le lui demande.

Rien à vérifier à la main : le journal le dit à chaque démarrage du service, une ligne par carte, `network Wi-Fi : Public` ou `network Ethernet : Private`.

Si la carte qui porte l'adresse du réseau local dit `Public`, une ligne la passe en privé, dans une fenêtre PowerShell administrateur, sur le PC concerné :

```
Set-NetConnectionProfile -InterfaceAlias "Wi-Fi" -NetworkCategory Private
```

Remplacer `"Wi-Fi"` par le nom de carte que donne le journal. Les cartes virtuelles d'autres logiciels peuvent rester en public : seule compte celle qui porte l'adresse du réseau local.

### Lancer l'application

Sur **les deux PC** : double-clic sur `target\release\ZyrDesk.exe`.

**Pas depuis la fenêtre administrateur.** Un programme lancé depuis une fenêtre administrateur hérite de ses droits, et ZyrDesk n'a aucune raison de tourner en administrateur : il écrirait ses fichiers sous une identité que le lancement normal du lendemain ne pourrait plus relire.

---

## Partie 1 : la fenêtre suffit à tout mettre en route

### Ce qui change, et pourquoi

Jusqu'ici, rendre un ordinateur joignable demandait quatre commandes : installer le service, le démarrer, lire une empreinte sur une machine, l'autoriser sur l'autre. Plus rien de tout cela. Le service s'installe depuis la fenêtre, et deux ZyrDesk sur le même réseau se reconnaissent d'eux-mêmes ([D17](../DECISIONS.md)).

> **R1 (la fenêtre s'ouvre seule)**
>
> Sur les **deux PC**, double-clic sur `ZyrDesk.exe`.
>
> Attendu : la fenêtre ZyrDesk, et **rien d'autre** : aucune fenêtre de commande, aucune fenêtre d'un autre programme, aucun logo qui ne soit pas le nôtre.

> **R2 (le service se démarre depuis la fenêtre)**
>
> À faire une fois par PC, la première fois seulement. Le service s'enregistre alors, et c'est la seule fois où Windows demande quelque chose : ensuite, ouvrir ZyrDesk le démarre et « Quitter » l'arrête, sans jamais rien redemander. Si le service tourne déjà, l'accueil affiche « Prêt à être contrôlé » et il n'y a rien à faire ici.
>
> Sur le **PC où le service n'est pas installé** : l'accueil affiche un bandeau rouge « Le service ZyrDesk ne tourne pas ». Cliquer **Démarrer le service**.
>
> Attendu : la demande d'autorisation de Windows apparaît, une fois. Après avoir accepté, le bandeau disparaît de lui-même en quelques secondes et l'état passe à « Prêt à être contrôlé ».
>
> Ce même geste pose les trois règles de pare-feu dont le service a besoin : le port du tunnel, celui de l'annonce sur le réseau local, et celui de l'appel direct qui la remplace quand le réseau ne la porte pas. Elles sont d'ailleurs réécrites à chaque démarrage du service. Le journal (partie 7) le dit en toutes lettres, ligne `firewall opened for …`.
>
> À vérifier aussi : refuser la demande de Windows doit afficher « les droits administrateur ont été refusés » et rien de plus. Pas de plantage, pas de bandeau bloqué.

> **R3 (rien ne tourne quand personne ne s'en sert)**
>
> Sur le **PC hôte**, réglages : **Démarrer avec Windows** doit être décoché, ce qui est le cas par défaut. Redémarrer la machine, et sans ouvrir de session Windows dessus, attendre une minute.
>
> Attendu : depuis le **PC client**, l'ordinateur hôte **n'apparaît pas**. Rien de ZyrDesk ne tourne, et c'est le but ([D20](../DECISIONS.md)).
>
> Ouvrir une session Windows sur le PC hôte, puis ZyrDesk. Attendu : l'icône apparaît en bas à droite, l'état passe à « Prêt à être contrôlé » sans aucune demande de droits administrateur, et le PC réapparaît côté client.

> **R3bis (l'ordinateur répond avant l'ouverture de session)**
>
> Sur le **PC hôte**, réglages : cocher **Démarrer avec Windows**. Redémarrer la machine et, sans ouvrir de session dessus, attendre une minute.
>
> Attendu : depuis le **PC client**, l'ordinateur hôte apparaît avec sa pastille verte. C'est tout l'intérêt du service : la machine répond avant que quiconque s'y soit connecté.
>
> Ouvrir une session Windows dessus. Attendu : ZyrDesk revient tout seul, fenêtre fermée, icône présente en bas à droite.

> **R3ter (l'icône dit la vérité, et « Quitter » arrête tout)**
>
> Sur le **PC hôte**, ZyrDesk ouvert : fermer la fenêtre par sa croix.
>
> Attendu : la fenêtre disparaît, l'icône reste, et le PC reste joignable depuis le client. Un clic sur l'icône ramène la fenêtre.
>
> Couper l'interrupteur **Accès distant**. Attendu : l'icône s'atténue, et son infobulle dit « cet ordinateur n'est pas joignable ». Le rallumer la rend nette à nouveau.
>
> Clic droit sur l'icône, **Quitter**. Attendu : l'icône disparaît, et depuis le **PC client** l'ordinateur hôte disparaît de la liste en moins d'une minute. Vérifier dans le gestionnaire des tâches qu'il ne reste **ni `ZyrDesk`, ni `zyrdeskd`, ni `zyrdesk-host-engine`**.

> **R4 (un moteur qui manque se dit, et ne casse rien d'autre)**
>
> Sur l'un des deux PC, renommer temporairement `data\engines\host\zyrdesk-host-engine.exe`. Attendre dix secondes.
>
> Attendu : l'état de la carte passe à « Moteur hôte absent », un bandeau l'explique, et le bouton **Ouvrir le dossier** ouvre le bon dossier. Surtout pas « Démarrage en cours » indéfiniment.
>
> Vérifier ensuite que **la fenêtre continue de marcher** : les autres ordinateurs restent listés, et une session sortante reste possible depuis ce PC. Un ordinateur sans moteur hôte reste un client à part entière ([D18](../DECISIONS.md)).
>
> Remettre le nom du fichier. En dix secondes, l'état doit repasser à « Prêt à être contrôlé » sans rien relancer.

---

## Partie 2 : les deux ordinateurs se trouvent seuls

> **R5 (chacun voit l'autre)**
>
> Les deux applications ouvertes, sur le même réseau.
>
> Attendu : sur **chaque** PC, l'autre apparaît dans « Mes ordinateurs » avec son nom Windows, son adresse et une pastille verte. Aucune adresse à taper, aucune empreinte à recopier.
>
> Cela peut passer par l'un ou l'autre des deux chemins, et le journal dit lequel : `found … on the local network` pour l'annonce, `… answered a call on the local network` pour l'appel direct. Le second met jusqu'à une trentaine de secondes la première fois.
>
> Si la liste reste vide des deux côtés : voir R6bis.

> **R6 (une machine qui s'en va disparaît tout de suite)**
>
> Sur le **PC hôte**, clic droit sur l'icône en bas à droite, **Quitter**.
>
> Attendu : sur le **PC client**, il disparaît de la liste **en une seconde ou deux**. Une pastille verte doit vouloir dire joignable maintenant, pas joignable il y a une minute.
>
> Puis, pour le cas de la machine qui ne dit rien : éteindre brutalement le PC hôte, ou le débrancher du réseau. Attendu : il disparaît en une quinzaine de secondes. Personne ne peut faire mieux sans harceler le réseau.
>
> Le remettre en marche avant de continuer.

> **R6bis (le rattrapage quand le réseau n'annonce rien)**
>
> À ne faire que si R5 a échoué. Le geste est le même des deux côtés : chacun doit connaître l'autre, sinon la machine d'en face est refusée à l'arrivée et on n'a fait que la moitié du chemin.
>
> Sur le **PC hôte**, cliquer **Ajouter un ordinateur**, coller l'empreinte du PC client, laisser l'adresse vide, cliquer **Autoriser**.
>
> Sur le **PC client**, cliquer **Ajouter un ordinateur**, coller l'empreinte du PC hôte, saisir son adresse, éventuellement un nom, cliquer **Se connecter**.
>
> L'empreinte se lit sur la carte « Cet ordinateur » de l'autre fenêtre, bouton **Copier**.
>
> Attendu : le PC hôte affiche « Cet ordinateur est autorisé à venir sur celui-ci », et la session s'ouvre depuis le client. Ce chemin existe pour les réseaux qui bloquent la découverte ; il ne devrait servir à personne d'autre.

> **R6ter (un ordinateur ajouté reste à l'écran)**
>
> Après R6bis, quitter la session et fermer l'application sur le **PC client**, puis la relancer.
>
> Attendu : le PC hôte est là, sur une carte, avec sa pastille grise et la mention « ajouté à la main ». Un clic dessus rouvre la session. **Plus rien à ressaisir, jamais.** La pastille est grise parce que ce réseau ne porte pas les annonces, pas parce que la machine serait éteinte, et c'est écrit à côté.
>
> Pour le retirer : **Ajouter un ordinateur**, puis **Oublier** en bas du dialogue. Il disparaît de l'accueil et n'a plus le droit d'entrer.

---

## Partie 3 : la première session, sans code ni empreinte

### Ce qui change, et pourquoi

Les moteurs réclament entre eux un code à quatre chiffres, affiché sur un écran et tapé sur l'autre. Ce code voyage maintenant dans le tunnel, qui a déjà reconnu les deux ordinateurs à leur empreinte avant qu'un octet ne passe. Personne ne le voit plus.

> **R7 (la toute première session)**
>
> Pour tester vraiment le premier contact, effacer d'abord le dossier `data\devices\` sur le **PC client**, application fermée. Il se recrée tout seul.
>
> Relancer l'application, cliquer sur la carte du PC hôte.
>
> Attendu sur le PC client : la fenêtre entière passe sur « Établissement de la connexion », avec en dessous le nom de l'ordinateur visé (son adresse s'il n'a pas de nom), et une ligne qui suit ce qui se passe, dans l'ordre :
>
> 1. « Ouverture du tunnel… »
> 2. « Tunnel établi, paquets de N octets. »
> 3. « Premier accès à cet ordinateur : les deux font connaissance. Rien à faire. »
> 4. « Les deux ordinateurs se connaissent. »
> 5. « Démarrage de l'image… »
> 6. L'image du PC hôte, en plein écran
>
> **Aucun code à quatre chiffres ne doit apparaître nulle part**, ni sur le PC client, ni sur le PC hôte. Et rien à faire sur le PC hôte pendant tout ce temps.
>
> L'étape 3 ne doit pas durer plus de quelques secondes.

> **R8 (la deuxième session est directe)**
>
> Quitter la session (Ctrl+Alt+Maj+Q), puis se reconnecter.
>
> Attendu : plus d'étape « Premier accès ». Le tunnel s'établit et l'image arrive. Les deux ordinateurs se connaissent maintenant.
>
> Deux choses se regardent ici, parce que c'est la session ordinaire. Le temps entre le clic et l'image se compte en secondes et non en dizaines de secondes : le moteur client s'arrêtait cinq à huit secondes à chaque session pour laisser lire des messages sur une fenêtre qu'il n'a pas. Et entre l'écran d'ouverture et l'image, sa fenêtre doit être sombre : un cadre clair, même un instant, veut dire que le moteur installé n'est pas celui que nous compilons.

> **R8bis (l'ordinateur d'en face a oublié)**
>
> Sur le **PC hôte**, en fenêtre administrateur, une ligne : `.\target\release\zyrdeskd stop; Remove-Item -Recurse -Force .\data\host; .\target\release\zyrdeskd start`. Le moteur hôte repart sans se souvenir de personne, ce qui est exactement ce que produit une réinstallation.
>
> Depuis le **PC client**, se reconnecter normalement.
>
> Attendu : « Cet ordinateur ne nous reconnaît plus », puis les deux se représentent tout seuls, puis l'image. Toujours aucun code, et rien à faire d'aucun côté.
>
> C'est la seule panne que le produit ne peut pas prévoir : ce que le client retient d'un appairage n'est qu'une note qu'il s'est écrite, et l'hôte est le seul à décider.

> **R9 (l'image est bonne)**
>
> Pendant la session : bouger la souris, taper du texte, ouvrir une fenêtre, lancer une vidéo.
>
> Attendu : la souris répond sans décalage sensible, le clavier suit, l'image est nette et fluide. Ce jugement à l'oeil se fait en `--release` uniquement.

---

## Partie 4 : le bouton flottant

> **R10 (il arrive avec l'image, pas avant)**
>
> Attendu : le logo ZyrDesk apparaît en haut à droite **une fois l'image affichée**, et pas pendant l'ouverture du tunnel.

> **R11 (il se déplace)**
>
> Prendre le logo et le faire glisser ailleurs sur l'écran.
>
> Attendu : il suit la souris, se pose où on le lâche, et **n'ouvre pas** le menu à la fin du déplacement. Un clic net, sans bouger, ouvre le menu.

> **R12 (chaque entrée du menu fait ce qu'elle dit)**
>
> Ouvrir le menu et essayer les entrées une par une :
>
> | Entrée | Attendu |
> |---|---|
> | Plein écran | La fenêtre de la session bascule |
> | Statistiques | Les chiffres apparaissent puis disparaissent sur l'image |
> | Souris bureau ou jeu | Le pointeur change de comportement |
> | Masquer ce bouton | Le logo disparaît jusqu'à la fin de la session |
> | Quitter la session | L'image se ferme, la fenêtre ZyrDesk revient |
>
> Si une entrée ne fait rien : ouvrir le journal (partie 7) et regarder les lignes de « La fenêtre ». Elles disent ce que le bouton a demandé, et à quelle fenêtre.

> **R13 (fermer, et pas seulement quitter)**
>
> Rouvrir une session, puis choisir **Fermer sur l'ordinateur distant** dans le menu.
>
> Attendu : l'image se ferme, et le PC hôte rend réellement son bureau. La différence avec « Quitter » : quitter laisse le bureau distant ouvert, prêt pour un retour immédiat.

---

## Partie 5 : la fenêtre n'est pas la session

> **R14 (fermer la fenêtre ne coupe rien)**
>
> Pendant une session, fermer la fenêtre ZyrDesk par sa croix.
>
> Attendu : l'image continue, le bouton flottant reste.

> **R15 (elle retrouve la session)**
>
> Toujours pendant la session, relancer `ZyrDesk.exe`.
>
> Attendu : la fenêtre revient et affiche « Session en cours vers … » avec sa durée, au lieu d'un accueil vide. Les cartes des ordinateurs sont grisées : une seule session à la fois. Et **un seul** bouton flottant à l'écran.

> **R16 (elle survit à pire)**
>
> Tuer `ZyrDesk.exe` dans le gestionnaire des tâches, pendant une session, puis le relancer.
>
> Attendu : même résultat qu'en R15. C'est le service qui tient la session, pas la fenêtre.

---

## Partie 6 : les réglages

> **R17 (la qualité change vraiment)**
>
> Aucune session en cours. Ouvrir les réglages (engrenage), passer la qualité à **Qualité**.
>
> Attendu : la ligne sous le réglage annonce 2560 x 1440 et un débit plus élevé. Ouvrir une session : l'image doit être plus détaillée.
>
> Remettre **Équilibré** pour la suite.

> **R18 (un réglage survit à tout)**
>
> Passer le thème en **Clair** et le codec en **HEVC**, fermer l'application, la relancer.
>
> Attendu : les deux choix sont toujours là. Le thème vit dans la fenêtre, le codec dans le service : les deux doivent tenir.

> **R19 (la confiance au réseau local se coupe)**
>
> Sur le **PC hôte**, dans les réglages, couper **Ordinateurs du réseau local**.
>
> Attendu : depuis le PC client, une nouvelle session est refusée avec un message qui parle d'un ordinateur refusé, et non d'un délai d'attente. Une session déjà en cours, elle, n'est pas coupée.
>
> Rallumer l'interrupteur : une nouvelle session doit repasser dans les cinq secondes.

> **R20 (l'accès distant se coupe)**
>
> Sur le **PC hôte**, couper l'interrupteur **Accès distant** de la carte.
>
> Attendu : l'état passe à « Accès distant désactivé » et le PC client ne peut plus ouvrir de session. Redémarrer le PC hôte : l'accès doit rester désactivé, c'est une décision et non un état.
>
> Le rallumer avant de continuer.

---

## Partie 7 : le journal et la version

### Ce qui change, et pourquoi

Quand quelque chose ne marche pas, la première question est toujours la même : quelle version tourne, et qu'a-t-elle écrit. Les deux sont maintenant sous les yeux.

> **R21 (la version est affichée)**
>
> Attendu : en bas de l'accueil, une ligne du genre `ZyrDesk 0.1.0 (a1b2c3d 2026-08-18)`. Le premier morceau est le numéro de version, le second le commit et sa date.

> **R22 (une moitié en retard se voit)**
>
> Compiler **sans** arrêter ni redémarrer le service, c'est-à-dire lancer seulement `git pull && cargo build --release` alors qu'une version plus ancienne du service tourne.
>
> Attendu : la ligne de version passe en ambre et dit « mais le service tourne encore en … ». C'est exactement la panne que personne ne pense à vérifier.
>
> Refaire la mise à jour complète pour revenir à la normale.

> **R23 (le journal dit tout)**
>
> Cliquer l'icône **journal** en haut de la fenêtre.
>
> Attendu, dans l'entête : la version de la fenêtre, celle du service, le nom de l'ordinateur, ses adresses carte par carte, son empreinte, l'état de l'accès distant, celui du réseau local, les ordinateurs vus, la présence des deux moteurs et la compilation dont ils viennent, le nombre de sessions. Puis le contenu des quatre journaux, la fin de chacun.
>
> Cliquer **Copier tout**, coller dans un bloc-notes : tout doit s'y retrouver, tel quel.
>
> Le bouton **Vider** sert à partir d'une page blanche avant un essai. L'ordre compte : vider, **puis** relancer le service, puis lire. Vider après coup efface ce que le service a écrit en démarrant, c'est-à-dire précisément ce qu'on voulait lire.

> **R24 (le journal raconte l'appairage)**
>
> Après la session de R7, chercher dans le journal du **PC hôte** une ligne du genre `… paired with this computer`, et dans celui du **PC client** `way 1 handed its pairing code over`.
>
> Attendu : les deux y sont. C'est la preuve que le code a voyagé dans le tunnel et n'a été tapé par personne.

---

## Partie 8 : rien ne doit trahir les moteurs

> **R25 (aucune trace visible)**
>
> Pendant une session, sur les deux PC : ouvrir le gestionnaire des tâches, regarder la barre des tâches, la zone de notification et les titres de fenêtres.
>
> Attendu : aucun nom, aucun logo, aucune fenêtre appartenant à Sunshine, Moonlight ou GameStream. Les processus s'appellent `ZyrDesk`, `zyrdeskd`, `zyrdesk-host-engine`, `zyrdesk-session`.

> **R26 (l'icône est nette)**
>
> Regarder l'icône de ZyrDesk dans la barre des tâches et dans l'explorateur, en petite et en grande taille.
>
> Attendu : nette dans les deux cas, jamais floue.

---

## Si quelque chose ne va pas

La marche à suivre est toujours la même : ouvrir le journal, cliquer **Copier tout**, coller le résultat. Il porte la version, l'état des deux machines et la fin de chaque trace : c'est tout ce qu'il faut, et il n'y a rien d'autre à chercher sur le disque.

Deux cas courants, et leur cause habituelle :

- **Les ordinateurs ne se voient pas.** Le journal porte de quoi trancher, sans rien lancer d'autre, dans l'ordre où il faut le lire :
  1. `this computer answers at …`, sur les deux machines : si les deux adresses ne sont pas sur le même sous-réseau, rien d'autre ne peut marcher.
  2. `network <carte> : Public` : sur un réseau classé public, Windows coupe la découverte, quelles que soient les règles de pare-feu. Il faut `Private` sur la carte qui porte l'adresse du réseau local, sur les deux machines.
  3. `announcement sent from …` : les cartes par lesquelles l'annonce sort réellement. Une machine qui n'annonce que par une carte virtuelle ou un VPN ne sera entendue de personne.
  4. `calling on <carte> through <adresse>` : l'appel direct, celui qui marche quand le multicast ne traverse pas. Si la ligne dit `with no broadcast address` et `0 addresses`, cette carte ne peut appeler personne.
  5. `firewall rules laid for …` puis `firewall opened for …`, trois fois : les règles sont réécrites à chaque démarrage, pour le programme nommé sur la ligne. Si elles manquent, un autre pare-feu que celui de Windows est probablement en cause.
  6. `a question was answered from …` : cette machine reçoit bien du trafic sur cette carte. Si ces lignes sont là et qu'aucun ordinateur n'apparaît, le multicast ne traverse pas entre les deux machines, ce qui arrive couramment entre le Wi-Fi et l'Ethernet d'une box. C'est précisément le cas que l'appel direct rattrape : la ligne à chercher alors est `… answered a call on the local network`.

  Le rattrapage R6bis permet de continuer sans attendre.
- **La session est refusée avec un message d'ordinateur refusé.** La confiance au réseau local est coupée sur l'hôte (R19), ou son accès distant est désactivé (R20).
- **La session s'ouvre puis se referme aussitôt, sans image.** Le journal de la fenêtre le raconte pas à pas, de `session asked for towards …` à `session ended: …`. Si la ligne `the far computer no longer knows this one` y figure, le produit s'est rattrapé tout seul et il n'y a rien à faire. Sinon, la fin du journal du moteur client (`session.log`) porte le dernier mot du moteur, qui est toujours la vraie raison.

Deux entrées du menu flottant méritent leur propre explication :

- **« La fenêtre de la session n'est pas au premier plan ».** C'est une sécurité : les raccourcis partent vers la fenêtre active, et ZyrDesk refuse de les envoyer ailleurs qu'à la session. Cliquer une fois dans l'image, puis rouvrir le menu.
- **Un clic sur le bouton part vers l'ordinateur distant.** Le mode souris est sur Jeu : le pointeur appartient alors entièrement à l'autre machine. Ctrl+Alt+Maj+M pour revenir à la souris de bureau.
