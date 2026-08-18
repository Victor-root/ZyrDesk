# Jalon M4 : le produit se pilote entièrement à la souris

Ce document se déroule sur les deux mêmes PC Windows que les jalons précédents. Il ne demande **aucune ligne de commande en dehors de la mise à jour** : tout le reste se passe dans la fenêtre ZyrDesk.

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
> À faire une fois par PC, la première fois seulement. Si le service tourne déjà, l'accueil affiche « Prêt à être contrôlé » et il n'y a rien à faire ici.
>
> Sur le **PC où le service n'est pas installé** : l'accueil affiche un bandeau rouge « Le service ZyrDesk ne tourne pas ». Cliquer **Démarrer le service**.
>
> Attendu : la demande d'autorisation de Windows apparaît, une fois. Après avoir accepté, le bandeau disparaît de lui-même en quelques secondes et l'état passe à « Prêt à être contrôlé ».
>
> Ce même geste pose les deux règles de pare-feu dont le service a besoin : le port du tunnel, et celui de la découverte du réseau local. Le journal (partie 7) le dit en toutes lettres, ligne `firewall opened for …`.
>
> À vérifier aussi : refuser la demande de Windows doit afficher « les droits administrateur ont été refusés » et rien de plus. Pas de plantage, pas de bandeau bloqué.

> **R3 (le service survit au redémarrage)**
>
> Redémarrer le **PC hôte**. Sans ouvrir de session Windows dessus, attendre une minute.
>
> Attendu : depuis le **PC client**, l'ordinateur hôte apparaît toujours dans « Mes ordinateurs » avec sa pastille verte. C'est tout l'intérêt du service : la machine répond avant que quiconque s'y soit connecté.

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
> Si la liste reste vide des deux côtés : voir R6bis.

> **R6 (une machine éteinte disparaît)**
>
> Éteindre le PC hôte, ou y arrêter le service en fenêtre administrateur (`.\target\release\zyrdeskd stop`).
>
> Attendu : au bout d'une à deux minutes, il disparaît de la liste sur le PC client. Une liste qui ne ferait que grandir serait inutilisable.
>
> Le remettre en marche avant de continuer.

> **R6bis (le rattrapage quand le réseau n'annonce rien)**
>
> À ne faire que si R5 a échoué. Sur le **PC hôte**, copier l'empreinte affichée sur la carte « Cet ordinateur ». Sur le **PC client**, cliquer la tuile **Ajouter un ordinateur**, saisir l'adresse du PC hôte et coller l'empreinte.
>
> Attendu : la session s'ouvre quand même. Ce chemin existe pour les réseaux qui bloquent la découverte ; il ne devrait servir à personne d'autre.

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
> Attendu, dans l'ordre, sur le PC client :
>
> 1. « Ouverture du tunnel… »
> 2. « Tunnel établi », avec une taille de paquet
> 3. « Premier accès à cet ordinateur : les deux ordinateurs font connaissance. Rien à faire. »
> 4. « Démarrage de la session… »
> 5. L'image du PC hôte, en plein écran
>
> **Aucun code à quatre chiffres ne doit apparaître nulle part**, ni sur le PC client, ni sur le PC hôte. Et rien à faire sur le PC hôte pendant tout ce temps.
>
> L'étape 3 ne doit pas durer plus de quelques secondes.

> **R8 (la deuxième session est directe)**
>
> Quitter la session (Ctrl+Alt+Maj+Q), puis se reconnecter.
>
> Attendu : plus d'étape « Premier accès ». Le tunnel s'établit et l'image arrive. Les deux ordinateurs se connaissent maintenant.

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
> Attendu, dans l'entête : la version de la fenêtre, celle du service, le nom de l'ordinateur, son empreinte, l'état de l'accès distant, celui du réseau local, la présence des deux moteurs et la compilation dont ils viennent, le nombre de sessions. Puis le contenu des quatre journaux, la fin de chacun.
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

- **Les ordinateurs ne se voient pas.** Regarder le journal : si la ligne `firewall opened for ZyrDesk (réseau local)` n'y est pas, la règle n'a pas été posée, et un autre pare-feu que celui de Windows est probablement en cause. Sinon, les deux machines ne sont peut-être pas sur le même sous-réseau. Le rattrapage R6bis permet de continuer sans attendre.
- **La session est refusée avec un message d'ordinateur refusé.** La confiance au réseau local est coupée sur l'hôte (R19), ou son accès distant est désactivé (R20).

Deux entrées du menu flottant méritent leur propre explication :

- **« La fenêtre de la session n'est pas au premier plan ».** C'est une sécurité : les raccourcis partent vers la fenêtre active, et ZyrDesk refuse de les envoyer ailleurs qu'à la session. Cliquer une fois dans l'image, puis rouvrir le menu.
- **Un clic sur le bouton part vers l'ordinateur distant.** Le mode souris est sur Jeu : le pointeur appartient alors entièrement à l'autre machine. Ctrl+Alt+Maj+M pour revenir à la souris de bureau.
