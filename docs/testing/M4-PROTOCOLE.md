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
> Clic droit sur l'icône, **Quitter**. Attendu : l'icône disparaît, et depuis le **PC client** l'ordinateur hôte disparaît de la liste en moins d'une minute. Vérifier dans le gestionnaire des tâches qu'il ne reste **ni `ZyrDesk`, ni `zyrdeskd`, ni `zyrdesk-host-engine`, ni `zyrdesk-session`**.
>
> Le dernier est celui qui manquait : Windows ne ferme pas un programme parce que celui qui l'a lancé s'en va, et chaque moteur de session survivait à l'application, invisible, jusqu'au redémarrage de la machine. Ils sont maintenant tenus en laisse par le système, qui les ramasse même quand l'application est tuée sans ménagement.
>
> À vérifier aussi, et c'est le cas qui les accumulait : **pendant une session**, tuer `ZyrDesk.exe` par le gestionnaire des tâches. Attendu : l'image se ferme d'elle-même dans la seconde.

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
> Après R6bis, terminer la session et fermer l'application sur le **PC client**, puis la relancer.
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
> Terminer la session par le menu flottant, puis se reconnecter.
>
> Attendu : plus d'étape « Premier accès ». Le tunnel s'établit et l'image arrive. Les deux ordinateurs se connaissent maintenant.
>
> Deux choses se regardent ici, parce que c'est la session ordinaire. Le temps entre le clic et l'image se compte en secondes et non en dizaines de secondes : le moteur client s'arrêtait cinq à huit secondes à chaque session pour laisser lire des messages sur une fenêtre qu'il n'a pas. Et entre l'écran d'ouverture et l'image, sa fenêtre doit être sombre : un cadre clair, même un instant, veut dire que le moteur installé n'est pas celui que nous compilons.

> **R8ter (l'appairage survit au redémarrage du service)**
>
> Après une session réussie, sur le **PC hôte** : quitter ZyrDesk par l'icône, le rouvrir, puis se reconnecter depuis le **PC client**.
>
> Attendu : **aucune étape « Cet ordinateur ne nous reconnaît plus »**. L'image arrive directement.
>
> Le moteur hôte rangeait ses appairages et les identifiants de son interface locale dans le même fichier, ce qu'il fait par défaut. Poser des identifiants neufs, ce que le service fait à chaque démarrage, lui faisait relire et réécrire ce fichier à travers une bibliothèque qui ne rend pas une liste JSON telle qu'elle l'a lue : la liste des ordinateurs appairés revenait illisible. Deux fichiers désormais.

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

> **R9bis (une seule fenêtre)**
>
> Pendant une session : regarder la barre des tâches et faire Alt+Tab.
>
> Attendu : **une seule entrée ZyrDesk**, jamais deux. L'image est dans la fenêtre ZyrDesk, qui garde sa barre de titre.
>
> Réglages, **Fenêtre de la session** sur « Fenêtre » : déplacer la fenêtre, la redimensionner, la passer d'un écran à l'autre. Attendu : l'image suit sans décoller, et elle n'est **jamais déformée**. Réduire la fenêtre puis la restaurer : l'image revient à sa place. Les bandes noires font l'objet de la partie 5.
>
> Réglages sur « Plein écran » : à l'ouverture de la session, la fenêtre prend l'écran entier avant même l'image, et le rend à la fin de la session.
>
> L'entrée **Fenêtré ou plein écran** du menu flottant bascule cette fenêtre. Elle ne parle plus au moteur : c'est notre fenêtre qui change.

> **R10 (il arrive avec l'image, pas avant)**
>
> Attendu : le logo ZyrDesk apparaît en haut à droite **une fois l'image affichée**, et pas pendant l'ouverture du tunnel.

> **R10bis (rien que le logo)**
>
> Regarder le bouton de près, sur une zone claire de l'image puis sur une zone sombre. Ouvrir le menu, le refermer.
>
> Attendu : **seul le logo se voit**, avec ses coins arrondis, posé directement sur l'image. Aucun carré, aucune plaque, aucun fond derrière lui ni dans ses coins. Menu ouvert : la carte du menu est là, ses coins arrondis, et rien autour d'elle non plus.
>
> Une fenêtre est un rectangle, et la transparence est une chose que chacune des couches sous la page doit accorder : l'une d'elles ne l'accordait pas, et son rectangle se voyait dans les coins arrondis du logo. La fenêtre est maintenant découpée sur ce que la page dessine, mesuré par la page elle-même, et rien n'est jamais dessiné hors d'une découpe. À vérifier aussi sur écran agrandi : la découpe suit l'échelle.

> **R10ter (le menu s'ouvre sans clignoter)**
>
> Cliquer sur le logo pour ouvrir le menu, le refermer, recommencer une dizaine de fois en regardant **le logo lui-même** et non le menu.
>
> Attendu : le logo **ne bouge pas et ne disparaît jamais**, pas même le temps d'une image. Le menu apparaît en dessous, le logo reste exactement au même point de l'écran.
>
> Deux choses le faisaient clignoter, et il a fallu les deux. La page était accrochée par le coin haut **gauche** de sa fenêtre alors que celle-ci grandit vers la gauche : ouvrir le menu emportait le logo hors de la fenêtre. Elle est maintenant accrochée par le coin haut **droit**. Et la fenêtre changeait de taille à chaque ouverture, ce qui fait remettre la page en page : le temps que ça prend, le logo n'est dessiné nulle part. Elle garde maintenant **la même taille du début à la fin de la session**, celle du menu déplié, et c'est la découpe seule qui change. Ce qui n'est pas dessiné n'existe pas : la partie de la fenêtre qui ne sert pas ne se voit pas et laisse passer les clics jusqu'à l'image.
>
> Passer aussi la souris sur le logo sans cliquer : il **grandit doucement**, entièrement, sans qu'aucun de ses quatre coins arrondis soit rogné, et redescend quand la souris s'en va. Il grandit vers l'intérieur de la fenêtre, et la découpe suit l'animation image par image.

> **R11 (il se déplace)**
>
> Prendre le logo et le faire glisser ailleurs sur l'écran.
>
> Attendu : il suit la souris sans décrocher, y compris quand le geste est rapide et large, se pose où on le lâche, et **n'ouvre pas** le menu à la fin du déplacement. Un clic net, sans bouger, ouvre le menu.
>
> Le geste est suivi par le système et non par la page : c'est ce qui permet à un bouton de cinquante pixels de rester sous une souris qui en sort au premier centimètre.

> **R11bis (il revient là où on l'a laissé)**
>
> Déplacer le bouton en bas à gauche de l'image, terminer la session, **fermer ZyrDesk entièrement** (icône près de l'horloge, Quitter), rouvrir ZyrDesk et ouvrir une nouvelle session.
>
> Attendu : le bouton apparaît **du premier coup en bas à gauche**, à la place où il avait été laissé. Il ne naît pas ailleurs pour s'y rendre ensuite, et il n'a jamais l'air de sauter d'un coin à l'autre à l'ouverture.
>
> Sa place est écrite dans `data\floating-button.conf`, en décalage depuis le coin haut droit de l'image et non en pixels d'écran : un autre écran, ou une image d'une autre taille, le retrouve quand même. Elle est écrite une fois, quand la main lâche, et relue une fois, à l'ouverture du programme. Le saut d'avant venait d'ailleurs : la fenêtre était créée par la boîte à outils, dont la taille demandée n'est appliquée qu'un tour de file plus tard, si bien que le bouton naissait à la mauvaise taille dans le mauvais coin et ne trouvait sa place qu'une fois la page chargée.

> **R12 (chaque entrée du menu fait ce qu'elle dit)**
>
> Ouvrir le menu et essayer les entrées une par une :
>
> | Entrée | Attendu |
> |---|---|
> | Plein écran | La fenêtre de la session bascule |
> | Statistiques | Les chiffres apparaissent puis disparaissent sur l'image |
> | Souris bureau ou jeu | Le pointeur change de comportement |
> | Masquer ce bouton | Le logo disparaît, et l'entrée dit par quelle combinaison le rappeler |
> | Terminer la session | L'image se ferme, la fenêtre ZyrDesk revient sur l'accueil, et le PC hôte rend son bureau |
>
> **Cinq entrées, pas six.** Il n'y a plus qu'une façon de finir : les moteurs en offraient deux, dont une qui laissait le bureau distant ouvert et en attente. Une session est en cours ou terminée.
>
> Si une entrée ne fait rien : ouvrir le journal (partie 7) et regarder les lignes de « La fenêtre ». Elles disent ce que le bouton a demandé, et à quelle fenêtre. Une entrée ne peut agir que si l'image est au premier plan ; la remettre devant est fait avant chaque envoi, et attendu, parce que Windows ne change pas de fenêtre de tête sur-le-champ.

> **R12bis (le bouton masqué revient)**
>
> Après avoir masqué le bouton, taper **Alt + ²** (la touche à gauche du 1).
>
> Attendu : le bouton reparaît, menu déjà ouvert. C'est le seul chemin de retour, et sans lui masquer serait un aller simple jusqu'à la fin de la session.
>
> Si rien ne se passe, le journal dit si Windows a pris la combinaison : elle peut être déjà tenue par un autre programme. Elle se change alors dans **Réglages, Raccourcis clavier**.

> **R12quater (le bouton reste joignable en souris de jeu)**
>
> Passer en **souris de jeu** par le menu, puis essayer de pointer le bouton : impossible, et c'est normal, le pointeur appartient entièrement à l'ordinateur distant. Taper alors la combinaison qui ouvre le menu.
>
> Attendu : le pointeur revient, le menu s'ouvre, et tout redevient cliquable. L'entrée qui redonne la souris à la session est dans ce menu.
>
> Demander le menu, c'est demander à faire quelque chose : le pointeur est rendu d'abord. Il ne l'est que s'il était réellement tenu, ce qui se lit dans les limites que le système donne au curseur, et non deviné.

> **R12ter (les raccourcis se choisissent)**
>
> Dans **Réglages**, section **Raccourcis clavier** : cliquer sur la combinaison en face de « Fenêtré ou plein écran », taper Ctrl + Alt + F, puis ouvrir une session et l'essayer.
>
> Attendu : la combinaison s'affiche telle qu'elle est gravée sur ce clavier, elle survit à la fermeture de la fenêtre, et elle bascule l'image. Échap pendant l'attente annule, Retour arrière retire la combinaison.
>
> Ce qui est retenu est la place de la touche et non le signe dessus : une combinaison choisie sur un clavier français reste sous les mêmes doigts sur un clavier anglais.

> **R13 (le bouton s'en va avec l'image)**
>
> Terminer la session par le menu, et regarder le coin où était le bouton.
>
> Attendu : le logo disparaît **en même temps que l'image**, pas une seconde après. Ce qui suit une session était surveillé une fois par seconde, et cette seconde se voyait.

---

## Partie 5 : la session du début à la fin

### Ce qui change, et pourquoi

Cette partie suit une session entière, dans l'ordre, du premier clic au gestionnaire des tâches après coup. Elle est numérotée **S** pour se lire d'une traite : chaque essai suppose le précédent, et sauter un rang fait rater ce qu'il préparait.

Trois choses s'y jouent qui ne se jouent nulle part ailleurs. Une seule fenêtre du début à la fin, ce qui veut dire qu'à aucun instant, même un dixième de seconde, une deuxième fenêtre ne doit se voir, et que tout ce qui arrive à cette fenêtre doit arriver à la session dedans : la réduire, la déplacer, la fermer. Aucune bande noire, ce qui se règle aux deux bouts à la fois. Et ce que la session retient d'une fois sur l'autre.

**Ce qu'il faut sous la main.** Le PC hôte doit être visible : les essais S7 et S19 regardent sa définition d'écran pendant et après la session. S'il est dans une autre pièce, faire ces deux-là en dernier, en s'y déplaçant.

### Avant

> **S1 (départ propre)**
>
> Les deux ZyrDesk fermés. Sur chaque PC, ouvrir le gestionnaire des tâches, onglet **Détails**, trier par nom.
>
> Attendu : aucun `zyrdesk-session.exe`, aucun `zyrdesk-host-engine.exe`, aucun `ZyrDesk.exe`. Seul `zyrdeskd.exe` peut tourner, c'est le service.
>
> Tout ce qui traîne ici fausse la suite : un moteur resté d'une session précédente tient encore le bureau distant et sa définition.

> **S2 (noter la définition du PC hôte)**
>
> Sur le **PC hôte** : clic droit sur le bureau, **Paramètres d'affichage**, noter la définition affichée. Sur un portable seize-dixièmes ce sera `1920 x 1200`.
>
> C'est le point de comparaison de S7 et de S16. Sans lui, ces deux essais ne veulent rien dire.

> **S2bis (ce qu'une machine neuve propose)**
>
> Sur un ZyrDesk qui n'a jamais rien choisi, ouvrir les réglages, **Avancé**, **Fenêtre de la session**.
>
> Attendu : **Fenêtre** est le choix marqué, pas **Plein écran**.
>
> Une première session qui prend l'écran entier laisse quelqu'un devant le bureau d'un autre ordinateur, sans rien de ce produit en vue et sans qu'on lui ait montré la sortie. Le choix est retenu ensuite : qui veut l'écran le demande une fois.

### L'ouverture

> **S3 (le clic et l'attente)**
>
> Sur le **PC client**, réglage **Fenêtre de la session** sur **Plein écran**. Cliquer sur la carte du PC hôte, et regarder l'écran sans le quitter des yeux.
>
> Attendu, dans cet ordre et rien d'autre entre :
>
> 1. La fenêtre passe **immédiatement** en plein écran, avec « Établissement de la connexion » au milieu.
> 2. Les lignes d'avancement défilent dessous (voir R7).
> 3. L'image du PC hôte remplace l'écran d'ouverture, **dans la même fenêtre**.
>
> Du clic à l'image : quelques secondes, pas des dizaines.

> **S4 (aucune deuxième fenêtre, à aucun moment)**
>
> Le même essai que S3, mais en ne regardant que cela, et en le refaisant deux ou trois fois : c'est un défaut qui ne se voit qu'un instant.
>
> Attendu : **rien d'autre que la fenêtre ZyrDesk ne doit apparaître**. Ni une fenêtre à barre de titre au milieu de l'écran, ni un cadre vide, ni un retour en fenêtre avant de reprendre le plein écran.
>
> **Sur deux écrans surtout.** Mettre ZyrDesk sur le deuxième écran et refaire l'essai en regardant le **premier**. C'est là que le défaut se voyait : le moteur choisit l'écran principal pour sa fenêtre, sans égard pour celui où ZyrDesk se trouve, donc l'éclair de cadre blanc apparaissait sur l'écran que personne ne regardait.
>
> La fenêtre du moteur naît maintenant cachée et n'est montrée qu'une fois tout réglé ; ZyrDesk la prend en main pendant ce temps-là et la pose avant que quiconque puisse la voir. Cela demande les **moteurs recompilés** : si l'éclair est toujours là, vérifier dans le journal que le moteur client en place est bien celui de la compilation du jour.

> **S5 (l'écran n'est pris qu'une fois)**
>
> Refaire S3 avec le réglage sur **Fenêtre**.
>
> Attendu : la fenêtre **ne prend jamais l'écran entier**, ni à l'ouverture ni à l'arrivée de l'image. Elle reste une fenêtre ordinaire, et l'image se pose dedans.

> **S5bis (la session s'ouvre comme la dernière a été laissée)**
>
> Session ouverte en plein écran : basculer en fenêtre par le menu flottant, terminer la session, en rouvrir une.
>
> Attendu : elle s'ouvre **en fenêtre**. Refaire dans l'autre sens : basculer en plein écran, quitter, rouvrir. Elle s'ouvre en plein écran.
>
> Ce qui est basculé pendant une session est un choix comme un autre, et il s'écrit à côté des autres : les réglages doivent montrer la même valeur après coup, dans **Avancé, Fenêtre de la session**.

### Pendant : l'image

> **S6 (aucune bande noire en plein écran)**
>
> Session ouverte en plein écran. Regarder les quatre bords de l'écran.
>
> Attendu : **l'image touche les quatre bords**. Aucune bande noire, ni en haut, ni en bas, ni sur les côtés.
>
> C'est l'essai qui compte le plus sur un grand écran regardant un portable : les deux n'ont pas la même forme, et jusqu'ici la différence était remplie de noir.

> **S7 (le bureau distant a bien changé de définition)**
>
> Toujours pendant la session, **dans l'image** : clic droit sur le bureau distant, **Paramètres d'affichage**.
>
> Attendu : la définition n'est plus celle notée en S2. Elle est celle de la qualité choisie dans les réglages : `1280 x 720` en Fluidité, `1920 x 1080` en Équilibre, `2560 x 1440` en Qualité.
>
> Si elle n'a pas changé, c'est la cause des bandes noires et rien d'autre ne la corrigera : le moteur hôte filme le bureau tel quel et remplit de noir ce qui manque. Le journal du moteur hôte le dit, sur le PC hôte, dans `logs\engine.log`.

> **S8 (aucune bande noire en fenêtre)**
>
> Passer en fenêtre par le menu flottant, puis tirer le coin en bas à droite, largement, dans les deux sens. Tirer ensuite **chaque bord seul** : le bas, le haut, un côté.
>
> Attendu : la fenêtre garde la forme de l'image **pendant le geste**, quel que soit le bord tenu. Tirer un côté change la hauteur en même temps, tirer le haut ou le bas change la largeur en même temps ; il est impossible de donner à la fenêtre une autre forme que celle de l'image. Le bord opposé à la main ne bouge pas : tirer le haut laisse le bas en place, tirer la gauche laisse la droite en place. L'image remplit toujours la fenêtre entière, sans bande noire et sans déformation.
>
> C'est le comportement d'un lecteur vidéo, et c'est voulu : une fenêtre libre de sa forme redemanderait une bande noire à chaque geste.

> **S8bis (redimensionner est fluide)**
>
> En fenêtre : prendre le coin en bas à droite et le promener, largement et vite, pendant plusieurs secondes sans lâcher.
>
> Attendu : la fenêtre suit la souris **sans à-coups**, et l'image dedans suit la fenêtre. Ni saccade, ni fenêtre qui s'arrête pour rattraper ensuite.
>
> Regarder aussi que **les deux ne se décalent jamais** : le bord de l'image et le bord de la fenêtre bougent ensemble, sans que l'un traîne derrière l'autre. Et que **rien ne clignote** le long des bords de l'image : la découpe des coins est retirée le temps du geste et remise à la fin, sans quoi une fenêtre qui grandit reste découpée à la taille qu'elle avait au début du geste et laisse voir la page derrière elle sur toute la bande neuve.
>
> Quatre choses le rendaient impossible, et la dernière était de loin la plus lourde : le moteur détruisait et reconstruisait tout son décodeur à chaque changement de taille, soit 350 ms par cran, mesurés. Il encaisse maintenant un changement de taille ([D25](../DECISIONS.md)). Les trois autres : la forme corrigée après coup, qui redimensionnait la fenêtre deux fois par cran ; l'image posée à travers la file d'événements de la boîte à outils, qui arrive une file plus tard que la fenêtre elle-même ; et le bouton flottant déplacé en demandant deux fois à cette même boîte, cent fois par seconde.
>
> **Le journal chiffre le geste.** Après avoir lâché, ouvrir le journal : une ligne `redimensionnement par ...` dit quel bord était tenu, combien de crans le geste a pris et ce que chaque partie a coûté. C'est par là qu'on saura, sans deviner, si quelque chose se remet à traîner un jour.

> **S8quinquies (l'image ne tremble pas sous une main en diagonale)**
>
> En fenêtre : prendre **le coin** en bas à droite et le promener lentement en diagonale, en le faisant onduler un peu, dix ou quinze secondes sans lâcher. C'est le geste le plus exigeant : la main descend et s'écarte en même temps.
>
> Attendu : la fenêtre grandit et rétrécit **de façon continue**, sans jamais faire un pas en arrière. Aucun frémissement, aucun tremblement de l'image sous une main qui avance régulièrement.
>
> Une fenêtre tenue à une forme n'a qu'une seule taille libre : l'autre s'en déduit. Laquelle des deux mène était relu à chaque cran, et une main en diagonale bouge les deux côtés de presque autant : la réponse changeait d'un cran à l'autre, et les deux réponses sont écartées de plusieurs pixels. C'était le tremblement. Le bord tenu est maintenant lu une fois pour tout le geste, dans ce que le système laisse immobile, et un coin qui tient les deux bords à la fois reçoit le point milieu entre les deux réponses au lieu de sauter de l'une à l'autre.
>
> **Le journal le chiffre aussi.** Après avoir lâché : la ligne `redimensionnement par un coin` porte un nombre de `changements de sens`. Une main qui n'a fait que tirer vers l'extérieur doit en montrer **zéro**.

> **S8ter (les coins de la fenêtre)**
>
> En fenêtre, regarder les deux coins du bas de la fenêtre ZyrDesk pendant une session.
>
> Attendu : **les deux coins du bas de l'image sont arrondis**, exactement comme ceux de la fenêtre, comme n'importe quelle fenêtre de Windows 11. Pas de rectangle à angles droits dans un rectangle à angles arrondis.
>
> Windows arrondit les coins de toutes les fenêtres, et l'image est une fenêtre à part qui reste un rectangle : c'est elle qui est découpée pour suivre. Seulement en bas, le haut de l'image étant sous la barre de titre, là où le cadre est droit. À vérifier aussi sur un écran agrandi : la courbe grandit avec le reste.
>
> Regarder l'angle de près, la fenêtre étant **active**, donc avec le liseré de couleur que Windows 11 dessine autour d'elle : ce liseré doit garder **sa couleur jusque dans l'angle**, exactement comme sur une fenêtre ordinaire. Ni assombri, ni interrompu, ni recouvert.
>
> C'est ce que la découpe décide, et elle a été prise deux fois de travers. Une fenêtre a deux courbes : celle du cadre, sur laquelle tourne le liseré, et celle du contenu, un poil plus rentrée, qui est là où le contenu d'une fenêtre s'arrête. L'image est du contenu, donc c'est la seconde. Découpée sur la première, l'image gardait les pixels qui séparent les deux, c'est-à-dire ceux du liseré lui-même, et le peignait avec l'écran distant : le liseré devenait sombre dans les deux coins du bas pour toute la durée de la session. Découpée trop court, à l'inverse, elle laissait voir la page derrière elle.
>
> **Le journal donne les deux nombres.** Une ligne `coins de l'image : bordure de N px, rayon de M px` est écrite à chaque fois qu'ils changent. C'est ce qu'il faut envoyer si l'angle n'est toujours pas juste : sur un écran agrandi ils ne valent pas la même chose, et ça ne se devine pas depuis une capture.
>
> **Puis l'inverse**, qui compte tout autant : passer en plein écran, et maximiser la fenêtre (double-clic sur la barre de titre). Dans ces deux cas, Windows dessine la fenêtre à **angles droits**, et l'image doit l'être aussi : aucun coin rogné, aucune morsure dans le bas de l'écran distant. Revenir en fenêtre : les coins se réarrondissent.

> **S8quater (la fenêtre ne peut pas être réduite à un filet)**
>
> En fenêtre, pendant une session : attraper le bord **du bas** et le remonter aussi haut que la souris veut bien aller. Recommencer avec le bord du **haut**, puis avec un coin.
>
> Attendu : la fenêtre **s'arrête** à une taille où l'image et le bouton flottant tiennent encore. Elle ne devient jamais un filet, et **ZyrDesk ne se ferme pas**.
>
> Windows borne la taille d'une fenêtre qu'on redimensionne, puis nous laisse corriger la forme, et ne reborne pas ce qu'on lui rend : tenir une forme oblige donc à tenir aussi un plancher. Sans lui, tirer le bord du bas emmenait la fenêtre bien en dessous de tout ce que Windows aurait permis, et le bouton flottant n'ayant plus de place où se poser, le programme s'arrêtait net.

> **S9 (l'image suit la fenêtre partout)**
>
> En fenêtre : **prendre la barre de titre et promener la fenêtre**, la passer sur l'autre écran s'il y en a deux, la réduire dans la barre des tâches, la restaurer.
>
> Attendu : la fenêtre **se déplace normalement**, et l'image reste exactement dedans à chaque instant, sans décoller ni rester derrière.
>
> Le déplacement compte autant que le reste : tenir la forme de l'image se fait sur le message par lequel passe aussi un simple déplacement, et une correction appliquée à tort y remettait la fenêtre à son point de départ à chaque pas, donc la rendait immobile.

> **S9ter (la barre de titre reste allumée tant que la fenêtre sert)**
>
> En fenêtré, pendant une session, faire dans l'ordre en regardant **la barre de titre de ZyrDesk** :
>
> 1. cliquer dans l'image, taper quelques lettres ;
> 2. ouvrir le menu flottant, le refermer ;
> 3. prendre le bouton flottant et le déplacer ;
> 4. réduire ZyrDesk dans la barre des tâches, le restaurer, puis cliquer dans l'image.
>
> Attendu : la barre de titre reste **allumée** du début à la fin, comme celle de n'importe quelle fenêtre au premier plan. Elle ne doit jamais griser, pas même une seconde.
>
> **Puis l'inverse**, qui compte autant : Alt+Tab vers une autre application. La barre doit **griser** immédiatement, comme il se doit. Revenir sur ZyrDesk : elle se rallume.
>
> Le premier plan appartient au lecteur pendant presque toute une session, et au bouton flottant quand une main le touche : ni l'un ni l'autre n'est « quelqu'un d'autre », et la fenêtre est bel et bien celle qu'on utilise. Windows pose la question au moment même où il change de premier plan, quand ce qu'il est en train de donner n'est pas encore posé : la réponse est donc donnée deux fois, une tout de suite et une par un message que le programme s'envoie à lui-même et que Windows ne rend qu'une fois l'affaire finie.
>
> **Le journal note chaque bascule** : `barre de titre active` ou `inactive`, avec à qui est le premier plan, à ZyrDesk, à l'image, ou ailleurs. Une bascule vers `inactive` pendant l'une des quatre étapes ci-dessus est le défaut, et la ligne dit lequel des trois cas c'était.

> **S9bis (le bouton flottant reste chez lui)**
>
> Pendant une session en fenêtré, faire Alt+Tab vers une autre application, la regarder quelques secondes, puis revenir sur ZyrDesk.
>
> Attendu : le bouton flottant **disparaît** dès que l'autre application passe devant, et **revient** quand ZyrDesk ou l'image reprend le premier plan. Il ne flotte jamais au-dessus du travail de quelqu'un d'autre.
>
> Il est dessiné au-dessus de toutes les fenêtres de la machine, ce qu'il faut pour tenir sur l'image ; il suit donc le premier plan, qui est celui de l'image autant que le nôtre puisque l'image appartient au lecteur.

> **S10 (le plein écran va et vient)**
>
> Basculer plein écran et retour, cinq ou six fois de suite, au bouton flottant puis au raccourci clavier.
>
> Attendu : chaque bascule est nette, l'image reste dedans, et **le clavier continue d'aller à l'ordinateur distant** après chaque bascule. Taper quelques lettres pour le vérifier à chaque fois.
>
> Prendre l'écran ramène notre fenêtre devant, et le moteur perd alors le clavier qu'il avait demandé au système. Il lui est rendu tout de suite après, et c'est ce rendu que cet essai vérifie.

### Pendant : ce qui est à nous

> **S11 (le menu flottant)**
>
> Dérouler la partie 4 en entier sans fermer la session : R10, R11, R12, R12bis, R12quater, R12ter.
>
> Attendu : rien n'a changé de ce côté. Le bouton se prend, se déplace, se masque, se rappelle, et chaque entrée fait ce qu'elle dit.

> **S12 (le bouton ne quitte jamais l'image)**
>
> Session en plein écran. Basculer en fenêtre par le menu flottant, puis déplacer la fenêtre, la redimensionner, la repasser en plein écran, y revenir.
>
> Attendu : à chaque instant le bouton est **dans le coin de l'image**, à la distance où il a été laissé. Jamais au milieu de l'écran, jamais en dehors de la fenêtre.
>
> Il était posé une fois pour toutes quand il montait, sur le coin qu'avait l'image à ce moment-là. Une session repassée en fenêtre le laissait donc suspendu là où le plein écran l'avait mis.

> **S13 (la barre de titre reste allumée)**
>
> Session en fenêtre. Regarder la barre de titre de la fenêtre ZyrDesk, puis cliquer dans l'image, puis passer sur un autre programme et revenir.
>
> Attendu : tant que ZyrDesk est devant, sa barre de titre est celle d'une fenêtre **active**, image cliquée ou non. Passer sur un autre programme l'atténue, comme n'importe quelle fenêtre. Revenir la rallume.
>
> Le premier plan appartient à l'image, parce que c'est là que le moteur doit être pour tenir le clavier. Ce qui prend le devant étant notre propre image dans notre propre fenêtre, une barre atténuée disait quelque chose de faux.

> **S14 (réduire emporte tout)**
>
> Pendant la session, réduire la fenêtre ZyrDesk dans la barre des tâches.
>
> Attendu : **l'écran redevient l'écran**. Ni image, ni bouton flottant nulle part. Restaurer la fenêtre : les deux reviennent avec elle, à leur place, sans passer par ailleurs.
>
> Le bouton flottant restait seul dans le coin d'un bureau vide, par-dessus le travail des autres, et il devenait alors impossible à déplacer comme à ouvrir.

> **S15 (l'icône dit ce qui se passe)**
>
> Poser la souris sur l'icône ZyrDesk à côté de l'horloge, sans cliquer.
>
> Attendu : « ZyrDesk : une session est en cours, cliquez pour revenir à la fenêtre ». Cliquer dessus ramène la fenêtre réduite, image comprise.

> **S16 (un deuxième lancement ne fait pas un deuxième ZyrDesk)**
>
> Réduire la fenêtre, puis relancer `ZyrDesk.exe`.
>
> Attendu : la même fenêtre revient, avec la session dedans. **Un seul** bouton flottant, **une seule** icône à côté de l'horloge.

### La fin

> **S17 (terminer la session)**
>
> Menu flottant, **Terminer la session**.
>
> Attendu : l'image se ferme, la fenêtre ZyrDesk revient à sa taille d'accueil et quitte le plein écran, et le bouton flottant disparaît **en même temps que l'image** et non une seconde après. **Aucune ligne rouge** ne traverse l'écran au passage.
>
> L'écran d'accueil réaffiche les cartes des ordinateurs, cliquables à nouveau.

> **S18 (la croix termine la session, elle aussi)**
>
> Rouvrir une session, en plein écran puis en fenêtre, et la fermer par la **croix** de la fenêtre.
>
> Attendu : exactement le résultat de S17. La session se termine, et la fenêtre **reste, sur l'accueil**. Elle ne disparaît pas, et il ne faut rien rouvrir.
>
> Sur l'accueil, en revanche, la croix range la fenêtre sans rien arrêter : l'icône à côté de l'horloge reste, et un clic dessus ramène la fenêtre. Le vérifier dans la foulée, c'est l'autre moitié de l'essai.

> **S18bis (la fenêtre revient de là où elle était)**
>
> Trois fins de session à faire l'une après l'autre, en terminant chaque fois **depuis l'ordinateur distant** (fermer la session dans l'image, ou éteindre l'écran distant) plutôt que par le menu :
>
> 1. ZyrDesk **réduit dans la barre des tâches** pendant la session ;
> 2. ZyrDesk **derrière une autre application** (Alt+Tab, puis attendre) ;
> 3. la session **en plein écran**, puis Alt+Tab vers autre chose.
>
> Attendu, dans les trois cas : à la fin de la session, la fenêtre ZyrDesk **revient devant, sur l'accueil**, à sa taille de fenêtre, avec le message de fin lisible. Elle ne reste pas en bas dans la barre des tâches.
>
> Une session finit toujours par dire quelque chose, une erreur le plus souvent, et ce quelque chose se dit sur l'accueil : derrière un bouton de la barre des tâches, il ne se dit à personne. Windows range de lui-même une fenêtre qui couvre tout l'écran quand le premier plan la quitte, ce qui suffit à faire disparaître ZyrDesk pendant une session.
>
> **Le journal encadre la fin.** Deux lignes `fin de session, avant` et `fin de session, après` disent l'état de la fenêtre des deux côtés : `réduit`, `visible`, `plein écran`. C'est par là qu'on saura laquelle des trois situations s'était produite si l'une d'elles revenait.

> **S19 (le bureau distant retrouve sa définition)**
>
> Sur le **PC hôte**, quelques secondes après : **Paramètres d'affichage**.
>
> Attendu : la définition est revenue à celle notée en S2.
>
> Le moteur hôte attend sinon l'arrêt de ce qu'il diffuse pour remettre en place, et ce qu'il diffuse est le bureau lui-même, qui ne s'arrête jamais. Il lui est demandé de remettre en place dès que le client s'en va.

> **S20 (rien ne reste en attente sur le PC hôte)**
>
> Après S17 ou S18, aller sur le **PC hôte** et regarder le gestionnaire des tâches.
>
> Attendu : plus aucun `zyrdesk-host-engine.exe` qui tiendrait encore un bureau. Terminer une session la termine des deux côtés.
>
> Il n'y a plus qu'une façon de finir. Les moteurs en offrent deux, dont une qui laisse le bureau distant ouvert et en attente d'un retour : c'était une session ni en cours ni terminée, et ce troisième état n'existe plus dans le produit.

> **S21 (quitter ZyrDesk pendant une session)**
>
> Rouvrir une session. Sur le **PC client**, clic droit sur l'icône à côté de l'horloge, **Quitter**.
>
> Attendu : tout s'arrête. L'image, la fenêtre, le bouton, l'icône.

> **S22 (rien ne traîne)**
>
> Sur les deux PC, gestionnaire des tâches, onglet **Détails**.
>
> Attendu : exactement l'état de S1. Aucun `zyrdesk-session.exe`, aucun `zyrdesk-host-engine.exe`, aucun `ZyrDesk.exe`.
>
> C'est l'essai qui a le plus servi : des moteurs restaient en vie après un « Quitter », et la mise à jour suivante butait dessus sans dire pourquoi. Ils sont maintenant attachés au programme qui les lance et s'en vont avec lui, quelle que soit la façon dont il s'en va.

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
