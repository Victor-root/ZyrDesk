# Jalon M4 : le produit se pilote entièrement à la souris

Ce document se déroule sur les deux mêmes PC Windows que les jalons précédents. Il ne demande **aucune ligne de commande en dehors de la mise à jour et d'un réglage Windows à faire une seule fois** : tout le reste se passe dans la fenêtre ZyrDesk.

Vocabulaire : **PC hôte** = celui qu'on contrôle. **PC client** = celui depuis lequel on se connecte. La plupart des étapes se font sur les deux, et c'est écrit à chaque fois.

Ce protocole remplace celui des versions précédentes, qui passait par `zyr-cli` et `zyrdeskd` à chaque étape. La ligne de commande existe toujours, mais c'est devenu un outil de diagnostic et non le chemin du produit.

---

## Où on en est

Ce tableau de bord est la seule chose à lire pour savoir quoi essayer. Le reste du document est la référence, à ouvrir quand un essai échoue ou qu'on veut le détail d'un attendu.

**Règle de tenue :** un essai ne passe en « confirmé » que quand il a été essayé et dit tel quel. Rien n'y monte parce que le code a l'air juste, parce que les tests automatiques passent, ou parce que ça marchait la semaine d'avant. Un essai qu'un changement touche redescend dans « à vérifier ».

### À vérifier maintenant

Ce que le dernier lot a changé, et rien d'autre. C'est la liste du jour.

| Essai | Ce qui a changé |
|---|---|
| **R17**, **R17bis** | La qualité disparaît. La taille, le débit et le codec se règlent dans le menu de la session, un cran par clic, et survivent à la fermeture |
| **R34** | Une ligne **Appliquer les changements** apparaît dans le menu de la session dès que ce qui est choisi n'est plus ce qui est à l'écran. Elle relance l'image sans fermer la session, et on peut changer plusieurs valeurs avant de la cliquer |
| **S18**, **S18ter** | La croix ramène **toujours** à l'accueil, en trois secondes au plus, y compris quand la session a lâché et que l'ordinateur d'en face ne répond plus |
| **S9bis** | Touché par le changement des touches système : la façon d'y faire perdre le premier plan à ZyrDesk change, le comportement attendu du bouton flottant non |
| **S21** | Plus aucune touche ne doit rester coincée. Le premier correctif ne se déclenchait jamais ; il est maintenant demandé à chaque tour de la surveillance de session |
| **S9sexies** | **La cause est trouvée et elle était chez nous : ZyrDesk mémorisait l'état d'Alt.** Un appui d'Alt qui n'arrive pas, et tous les Tab suivants sont jugés « Tab tout seul » et laissés au système, définitivement tant que le doigt reste dessus. Alt se lit maintenant dans le nom que Windows donne à chaque frappe, ce qui ne peut ni vieillir ni se perdre. Trois défauts de la même famille corrigés avec : les frappes envoyées par un programme ne mènent plus cet état, il est semé au bon endroit, et la reposée du crochet, qui perdait elle-même des frappes, est retirée en entier |
| **S19** | Ces touches doivent redevenir celles de ce PC-là dès que le premier plan quitte ZyrDesk, et à la fin de la session. Le journal nomme désormais chaque fenêtre qui prend le premier plan pendant une session |
| **R12sexies** | Un diagnostic si **Statistiques** ne montre toujours rien : le journal dit si un autre programme tient déjà cette combinaison |
| **R5** | Nouveau logo, et dessiné à chaque taille au lieu d'être réduit d'une seule : à comparer aux icônes voisines dans la barre des tâches |
| **R32** | Le plein écran n'a plus ni angles arrondis ni liseré, et l'image touche vraiment les quatre bords |
| **R33** | Deux réglages nouveaux côté hôte : renvoyer ou non un écran immobile, et la façon de filmer l'écran. Ce sont les deux seuls leviers qui restent sur la cadence |
| **S2**, **S7** | Le bureau distant ne change plus de définition : il déménage sur un écran que ZyrDesk fait pousser, et l'écran physique de l'hôte s'éteint le temps de la session |
| **R30**, **R31** | Ce qu'il reste de l'écran virtuel à vérifier : que tout soit bien remis en place à la fin d'une session, et que le retrait du produit ne laisse rien |
| **S6**, **S8** | Rien n'a changé pour eux, mais ils passent par le même chemin : à refaire une fois pour être sûr que l'écran virtuel ne réintroduit pas de bande noire |

### Confirmé

Ce qui a été essayé sur les deux vraies machines et dit tel quel. La colonne de droite reprend ce qui a été dit, pour qu'on puisse juger de la force de la confirmation.

| Essai | Ce qui a été dit |
|---|---|
| S3, S4, S5 (l'ouverture, aucune deuxième fenêtre, aucun éclair) | « ça a l'air d'avoir corrigé le bug du flash », puis « ok tout à l'air de marcher » |
| S8bis, S8ter, S8quinquies (redimensionner, les coins, la diagonale) | « ok le redimensionnement c'est bon », puis « ok tout à l'air de marcher » |
| S9, S9quater (déplacer, agrandir et restaurer) | « ok c'est nickel ! », puis « ok tout à l'air de marcher » |
| Le clavier pendant une session (dans la famille S9) | « le clavier remarche » |
| S11, S12 (le menu du bouton flottant) | « le fab n'est toujours pas revenu », corrigé, puis « ok tout à l'air de marcher » |
| R27, R28, R29 (l'écran virtuel, et le 4K net servi par un portable 1080p) | « c'est bon ça fonctionne nickel ça fait comme [le produit de référence] ». La netteté est acquise ; la cadence ne l'est pas, voir ci-dessous |
| R12septies (le bouton **Statistiques**) | « pour les statistiques c'est bon ». Le moteur le confirme dans son propre journal : `Detected stats toggle combo` |
| ~~S9sexies (Alt+Tab part vers l'ordinateur distant)~~ | **Confirmé puis démenti, et retiré d'ici.** Une session l'a donné pour bon (`8 candidate(s), 4 portée(s)`), la suivante l'a repris : dix candidates, aucune portée. La différence entre les deux n'est pas expliquée, et c'est ce que le lot en cours cherche |
| S20 (les raccourcis de ZyrDesk pendant toute la session) | Confirmé par le même journal : sept `sessions will open fullscreen/windowed from now on` répartis sur toute la session, qui sont le raccourci du plein écran répondant à chaque fois |
| ~~S21 (aucune touche coincée)~~ | **Revenu, et c'est l'essai à refaire en premier.** La session suivante l'a ramené : « j'ai perdu l'accès au clavier en ouvrant et fermant le fab », et le `Raising 1 keys` avec lui. Le relâchement était conditionné à un retour du clavier qui n'avait jamais lieu ; il est maintenant demandé à chaque tour |
| R12quinquies (le clavier après le menu du bouton flottant) | Confirmé par la même session : quatre ouvertures et fermetures du menu, et plus une seule ligne `le clavier n'est pas à la session` |

### Confirmé, mais pas fini

Ce qui marche et qui ne suffit pas. La différence avec la liste du dessus est qu'il reste quelque chose à faire, pas quelque chose à vérifier.

| Essai | Ce qu'il reste |
|---|---|
| R29 (le 4K est net) | Net, mais à **20 images par seconde** au lieu de 60. Le client n'y est pour rien : les statistiques du moteur client donnent 0 % de perte réseau, 1 ms de latence, 0,32 ms de décodage. Ce qui prend le temps est l'hôte, qui met **43 ms en moyenne** à capturer et encoder une image, alors qu'il en faut moins de 16,7 pour en tenir soixante. La taille, le débit et le codec se règlent maintenant depuis le menu de la session (R17), ce qui permet de chercher où est le mur |

### Jamais confirmé

Ni réussi ni échoué : personne ne les a essayés depuis qu'ils existent. Ils ne sont pas urgents, mais ils ne comptent pas comme acquis.

R1 à R4, R6 à R16, R18 à R26, S1, S2bis, S8quater, S8sexies, S9ter, S9quinquies, S10, S13, S14, S15, S16, S17, S18bis.

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

> **R5 (le logo est net partout, et c'est le même partout)**
>
> Le dessin a changé : deux écrans identiques, l'un blanc et l'autre or, décalés sur une diagonale, sans plaque ni fond derrière eux. Le regarder aux six endroits où il se voit, et **de près**, un écran 4K rendant le moindre flou évident :
>
> 1. la **barre des tâches**, à côté des autres icônes épinglées ;
> 2. l'**icône à côté de l'horloge** ;
> 3. la **barre de titre** de la fenêtre ZyrDesk, en haut à gauche ;
> 4. l'**en-tête de l'accueil**, à côté du nom du produit ;
> 5. l'**écran d'ouverture** d'une session ;
> 6. le **bouton flottant** pendant une session.
>
> Attendu : le même dessin partout, et **aussi net que les icônes voisines**. Les deux écrans se distinguent l'un de l'autre, leurs coins arrondis sont propres et non baveux. Comparer directement avec les icônes des autres applications de la barre des tâches : ZyrDesk ne doit plus être celle qui pique les yeux.
>
> Et à essayer sur une barre des tâches claire comme sombre, et dans les deux thèmes de l'application. Le dessin n'a pas de fond : c'est le trait sombre autour des écrans qui les délimite sur un fond clair, et le blanc et l'or qui les portent sur un fond sombre, où ce même trait disparaît et sépare alors les deux écrans au lieu de les cerner. Les deux lectures sont bonnes, mais il faut vérifier les deux.
>
> Sur le **bouton flottant**, la découpe suit les deux écrans et rien d'autre : le vide entre eux n'est pas dessiné, donc rien n'y est visible et les clics y traversent jusqu'à l'image.
>
> **Ce qui rendait le logo flou, et il a fallu trois essais pour le trouver.** Ce n'était ni le dessin, ni les tailles, ni le cache : **la fenêtre ne se dessine pas avec l'icône du programme**. Une fenêtre à qui on a donné une icône est dessinée avec celle-là, et la boîte à outils en donne une à toutes ses fenêtres en prenant la **première entrée** du fichier d'icône, une seule, pour la barre des tâches comme pour le bandeau. Les tailles d'un tel fichier étant rangées de la plus petite à la plus grande, c'était le dessin de **seize pixels, agrandi en quarante-deux**. Agrandir est bien pire que réduire, ce qui explique que cette icône ait été la seule molle d'une barre d'icônes nettes ; et les vingt autres tailles du fichier n'étaient jamais lues, ce qui explique que deux corrections successives du fichier n'aient rien changé.
>
> Le programme pose donc maintenant lui-même l'icône de sa fenêtre, prise dans la ressource compilée aux deux tailles exactes que Windows s'apprête à dessiner, la grande et la petite. Le journal le dit à l'ouverture : `icône de la fenêtre posée en 56 et 28 px (écran à 175 %)`.
>
> Deux autres défauts ont été trouvés en chemin et corrigés, chacun aurait suffi à gâcher le résultat. Les tailles du fichier y étaient rangées en PNG, or Windows ne lit un PNG dans un fichier d'icône **qu'à 256 pixels** : le fichier est maintenant écrit à la main, chaque taille sous la forme lue à cette taille-là. Et la zone de notification, qui ne sait pas lire un fichier d'icône du tout, recevait un dessin de 256 à écraser en 28 : elle reçoit maintenant l'image à la taille qu'elle demande.
>
> Le dessin se refait avec `python3 packaging/brand/build-icons.py`, qui écrit l'icône, l'image du programme, celles de la zone de notification, et recopie le dessin là où l'interface le lit. Une seule source, jamais deux à tenir d'accord.

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
> Deux choses le faisaient clignoter, et il a fallu les deux. La page était accrochée par le coin haut **gauche** de sa fenêtre alors que celle-ci grandit vers la gauche : ouvrir le menu emportait le logo hors de la fenêtre. Elle est maintenant accrochée par le coin haut **droit**. Et la fenêtre changeait de taille à chaque ouverture, ce qui fait remettre la page en page : le temps que ça prend, le logo n'est dessiné nulle part. Elle garde maintenant **la même taille du début à la fin de la session**, mesurée sur le menu déplié et sur ses trois sous-menus à la fois, et c'est la découpe seule qui change. Ce qui n'est pas dessiné n'existe pas : la partie de la fenêtre qui ne sert pas ne se voit pas et laisse passer les clics jusqu'à l'image.
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

> **R12quinquies (le clavier revient à la session après le menu)**
>
> Pendant une session, ouvrir le menu du bouton, cliquer dans un sous-menu, choisir une valeur, puis refermer le menu. **Taper ensuite dans l'image**, du texte dans le bloc-notes de l'ordinateur distant par exemple, **puis essayer Alt+Tab** (voir S9sexies, partie 5) : il doit encore agir sur l'ordinateur distant, pas sur ce PC-là.
>
> Attendu : les touches arrivent au loin, tout de suite, et Alt+Tab part toujours au loin. Refaire en masquant le bouton au lieu de refermer le menu : même résultat.
>
> C'est le bug le plus vicieux du bouton, parce qu'il ne se voit pas. Cliquer sur cette fenêtre-là ne la rend jamais active, ce qui était voulu ; mais sa page prend quand même le clavier **à l'intérieur de ZyrDesk**, et c'est le clavier, pas la fenêtre active, que la session suit. La session restait sourde en ayant l'air parfaitement normale, et il fallait la rouvrir.
>
> **Deux corrections fausses avant la bonne**, ce qui vaut d'être écrit. Les deux premières demandaient que le **premier plan** revienne à l'image. Il ne peut pas : l'image est portée comme une fenêtre fille de celle de ZyrDesk pendant toute la session, et Windows donne le premier plan au chef de famille, jamais à un membre. La demande réussissait donc à réactiver notre propre fenêtre, là où le premier plan était déjà, et tout avait l'air fait. Le journal le disait depuis le début, et personne ne l'avait lu : `le premier plan est à ZyrDesk`, jamais `à l'image`, de la première image d'une session à la dernière.
>
> Le journal le dit maintenant à chaque fois, des deux côtés : `le clavier est bien à la session ; le premier plan est [...]`, ou `le clavier n'est pas à la session : le focus a été refusé à l'image ; le premier plan est [...]`. Et `menu du bouton flottant ouvert` puis `fermé`, qui n'existaient pas et sans lesquels une session devenue sourde et une session jamais touchée se lisaient pareil.

> **R12septies (Statistiques marche)**
>
> Pendant une session, ouvrir le menu du bouton flottant et cliquer sur **Statistiques**.
>
> Attendu : des chiffres apparaissent en bas à gauche de l'image, cadence et débit surtout. Rouvrir le menu et recliquer : ils disparaissent.
>
> Pourquoi ça ne marchait pas, et c'est la même cause qu'au-dessus : cliquer sur le bouton donne le clavier à la page de ce bouton. La frappe envoyée juste après était lue par notre propre vue web et jetée, pendant que Windows répondait que l'envoi avait réussi, ce qu'il répond toujours. Le journal disait `statistiques envoyé au lecteur N`, et c'était vrai : envoyé chez nous.
>
> Le clavier est maintenant rendu à l'image et **vu y arriver** avant chaque frappe. S'il ne l'est pas, le menu le dit (« la session n'a pas repris le clavier ») au lieu d'envoyer dans le vide, et le journal écrit `statistiques refusé : l'image du lecteur N n'a pas repris le clavier`.

> **R12sexies (si Statistiques ne montre toujours rien)**
>
> À essayer seulement si l'entrée **Statistiques** du menu reste sans effet malgré tout : ouvrir une session, cliquer dans l'image, puis **Statistiques**.
>
> Attendu d'ordinaire : des chiffres apparaissent en bas à gauche de l'image, cadence et débit surtout, et un second clic les retire.
>
> **Si rien n'apparaît**, regarder le journal. La combinaison que cette entrée tape, Ctrl+Alt+Maj+S, n'est pas à nous : c'est celle que le moteur écoute, et rien ici ne peut la changer. Un autre programme sur cet ordinateur peut l'avoir prise pour lui-même, auquel cas la frappe part bien mais n'arrive jamais à la session : elle est allée ailleurs. C'est maintenant vérifié avant l'envoi plutôt que deviné après coup, et le journal le dit sans ambiguïté :
>
> - `statistiques envoyé au lecteur N : Ctrl+Alt+Maj+S, à la place 0x1f` : la combinaison est bien partie vers la session. Si les chiffres ne montrent toujours rien après ça, la cause est ailleurs, du côté du moteur, et pas de ce que ZyrDesk a fait.
> - `statistiques refusé : Ctrl+Alt+Maj+S est déjà pris par un autre programme` : trouvé et nommé. La fenêtre affiche le même message : fermer ce programme, ou changer son raccourci, puis réessayer.
>
> Ce que ça vérifie tient en une phrase : Windows dit toujours qu'une frappe a été envoyée, que quelque chose l'ait vraiment reçue ou non. La seule façon de savoir si quelqu'un d'autre la tenait déjà est de demander à la tenir soi-même, un instant, juste avant d'envoyer, et de la rendre aussitôt.

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

> **S2 (noter la définition du PC hôte, et celle du PC client)**
>
> Sur **chacun des deux PC** : clic droit sur le bureau, **Paramètres d'affichage**, noter la définition affichée. Sur un portable seize-dixièmes ce sera `1920 x 1200`.
>
> Les deux, et non plus seulement celle de l'hôte : ce que la session demande, c'est la taille de l'écran **du client**, et ce que l'hôte fournit dépend de ce qu'il sait dessiner. S7 et S16 comparent l'un à l'autre, et sans les deux nombres ils ne veulent rien dire.

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

> **S7 (le bureau distant est à la taille demandée)**
>
> Toujours pendant la session, **dans l'image** : clic droit sur le bureau distant, **Paramètres d'affichage**.
>
> Attendu : la définition est **celle demandée par la session**, que la ligne du journal du client annonce mot pour mot (`image demandée au loin en …`). En qualité **Qualité**, c'est la définition de l'écran du PC client notée en S2 ; sur les deux autres marches, c'est le plafond de la marche, `1280 x 720` ou `1920 x 1080`.
>
> Ce n'est plus l'écran de l'hôte qui a changé de taille : c'est un écran que ZyrDesk fait pousser sur l'hôte, sur lequel son bureau déménage le temps de la session. C'est ce qui permet à un portable de servir un écran plus grand que le sien sans rien agrandir. L'écran physique de l'hôte s'éteint pendant ce temps, et se rallume à la fin (R29, R30).
>
> Si la définition n'a pas changé, c'est la cause des bandes noires **et** du flou, et rien d'autre ne les corrigera : le moteur hôte filme le bureau tel quel, remplit de noir ce qui manque et agrandit le reste. Le journal du service hôte dit pourquoi, avec `virtual screen` et `screens the engine sees`.

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
>
> Regarder aussi le **bord vers lequel la fenêtre avance** pendant qu'on la promène : rien ne doit y clignoter, ni en ligne droite ni en diagonale, où ce sont les deux bords concernés.
>
> Deux choses le faisaient clignoter. Une fenêtre qui change de taille ne peut pas garder ce qui était dessiné dedans, sinon un morceau de l'ancienne image reste collé dans le nouveau cadre jusqu'au dessin suivant : le système est donc prié de tout jeter. Prié de le faire aussi pour une fenêtre qui ne fait que se déplacer, il jetait une image parfaitement bonne et la faisait repeindre à chaque cran du trajet. Ce n'est demandé maintenant que si la taille change vraiment.
>
> Puis deux demandes faites à chaque cran et qui n'avaient lieu d'être qu'une fois : remonter l'image au sommet de la pile des fenêtres, et l'afficher. Demandées soixante fois par seconde, elles font défaire et refaire toute la pile pour une fenêtre qui n'y bougeait pas, et rien ne l'y maintient de toute façon puisqu'elle appartient à la nôtre. Elles ne sont plus faites que si l'image n'est pas à l'écran, c'est-à-dire une fois, au début de la session.
>
> Ces économies ne suffisaient pourtant pas, parce que la fenêtre et l'image restaient déplacées par **deux demandes séparées**. Aussi rapprochées soient-elles, ce sont deux transactions, et le compositeur dessine ce qui est debout au moment où il se réveille, de temps en temps entre les deux : une bande de la page derrière l'image se voyait alors le long du bord vers lequel la fenêtre va, quel que soit l'ordre des deux demandes, qui ne choisit que le côté où la bande tombe. La seule transaction unique que Windows offre est l'arbre des fenêtres lui-même : le temps du geste, l'image est donc prise comme un **morceau de la fenêtre**, dessinée là où la fenêtre est, dans la même composition, et un cran du déplacement ne coûte plus rien du tout. Elle redevient une fenêtre à part entière quand la main lâche, ou au premier cran qui change la taille : un morceau ne se redimensionne pas avec son porteur, et le redimensionnement suit son propre chemin, l'image posée d'abord et la fenêtre dans la foulée, à l'intérieur du même message.
>
> **Le journal le dit.** La ligne du geste devient `déplacement (image portée par la fenêtre) : ...`. Si une ligne `l'image n'a pas pu être portée par la fenêtre (...)` apparaît à la place, le système a refusé l'adoption et son code dit pourquoi ; le déplacement retombe alors sur le chemin pas à pas. Et si le bord clignote encore **alors que la ligne dit « portée »**, la bande ne peut plus venir de l'image : promener la fenêtre **hors session**, sur l'écran d'accueil, départage, car si elle clignote là aussi, c'est la fenêtre de base elle-même.
>
> Regarder aussi les **coins du bas pendant le déplacement** : ils restent arrondis tout du long. La découpe qui les arrondit coûte trop cher pour être refaite à chaque cran d'un redimensionnement, et elle est donc retirée le temps du geste ; mais un déplacement ne change pas la taille, donc la découpe y reste juste et n'a aucune raison de partir. Retirée à tout geste, elle rendait l'image carrée le temps qu'on porte la fenêtre, par-dessus un cadre qui, lui, gardait ses coins. Elle ne part maintenant qu'au premier cran qui change vraiment la taille.

> **S9quater (agrandir et restaurer d'un seul mouvement)**
>
> En fenêtré, pendant une session : cliquer sur le bouton du **milieu** de la barre de titre, celui entre Réduire et Fermer, pour agrandir la fenêtre. Puis recliquer pour la remettre en fenêtre. Recommencer plusieurs fois, en regardant le bord de l'image.
>
> Attendu : **l'animation est celle de Windows**, la même que pour n'importe quelle autre fenêtre de la machine. Et pendant tout ce mouvement, **l'image et le cadre ne se quittent pas** : à aucun moment l'écran distant n'est à une taille et le cadre à une autre.
>
> Faire le même essai en **double-cliquant sur la barre de titre**, qui est le même ordre par un autre chemin.
>
> Puis le troisième chemin, qui n'en est pas un : **attraper la barre de titre et pousser la fenêtre contre le haut de l'écran**, jusqu'à ce que Windows propose de l'agrandir, et lâcher. Attendu exactement pareil, la fenêtre couvre l'écran et la session est dedans.
>
> Celui-là mérite son propre essai parce qu'il ne passe pas du tout par le même endroit. Le bouton et le double-clic sont un ordre, que le système nous adresse et qu'on lui rend. L'ancrage n'est pas un ordre : la main tient encore la fenêtre, et c'est le système qui change sa taille au milieu du geste, sans rien annoncer. Il a été cassé un moment, et de deux façons à la fois : le verrou de proportions, qui n'a de sens que pour une main posée sur un bord, s'appliquait au rectangle que le système avait choisi et la fenêtre atterrissait à une taille à elle, ni celle d'avant ni celle de l'écran, avec le bureau visible à côté ; et l'image était ressortie de la fenêtre à la taille qu'elle avait avant l'ancrage, si bien que ce qui s'affichait dedans était la page d'accueil de ZyrDesk et pas la session.
>
> Ce qui les départage est demandé au système avant que le geste commence : il dit si la main a pris la fenêtre par sa barre de titre ou par un bord. Après, plus rien ne le dit, les messages sont les mêmes pour les deux.
>
> Windows anime ce changement, et il l'anime bien : il tient le dessin de la fenêtre, l'étire vers son nouveau rectangle sur la carte graphique, au rythme de l'écran, et ne montre ce qui est vraiment là qu'à la fin. La fenêtre, elle, ne change de taille qu'**une seule fois**.
>
> Ce qu'il ne sait pas faire, c'est animer deux fenêtres comme une seule, et l'image de la session est une fenêtre à part : elle prenait sa taille définitive tout de suite, et on voyait l'écran distant bondir puis le cadre le rejoindre. C'est pour ça que le mouvement a d'abord été **joué à la main**, cran par cran.
>
> Ça a été une impasse, et elle vaut d'être écrite parce qu'elle a coûté cher. Jouer le mouvement à la main veut dire changer la taille de la fenêtre à chaque image dessinée, et à chacune Windows jette la surface dans laquelle il dessine cette fenêtre pour en allouer une plus grande, neuf mégaoctets une fois qu'elle couvre l'écran, puis redessine tout le cadre autour. Le journal a fini par mettre **les trois quarts du coût d'un cran là-dedans**, sur une puce qui a cent vingt-huit mégaoctets à elle. Aucun réglage de courbe, de rythme ou d'horloge ne pouvait rattraper ça, et tous ont été essayés.
>
> L'ordre est donc rendu au système, et **l'image est rangée dans la fenêtre le temps qu'il joue**. C'est le mécanisme qui a réglé le scintillement du déplacement : une fenêtre-fille n'a pas de place à elle sur l'écran, elle est dessinée à l'intérieur de la composition de sa mère, donc ce que le compositeur étire, c'est la paire. Une seule demande de taille pour la fenêtre, une seule pour l'image.
>
> Hors session, rien de tout ça : la fenêtre est seule et Windows l'anime comme n'importe quelle autre.
>
> **Deux choses à regarder à l'arrivée** :
>
> 1. **La fenêtre revient là où elle était.** La mettre à une taille bien reconnaissable dans un coin de l'écran, agrandir, redescendre : elle retombe exactement au même endroit, trois ou quatre fois de suite.
> 2. **Les coins et le liseré reviennent.** Redescendue, la fenêtre retrouve ses **coins arrondis** et le liseré de couleur tout autour, celui de Windows 11.
>
> **Ce qu'il faut regarder pendant**, et c'est le seul vrai risque de cette façon de faire : si l'écran distant **bondit à sa taille finale pendant que le cadre grandit derrière**, c'est que le compositeur a étiré notre fenêtre sans étirer ce qui était rangé dedans. C'est l'ancien défaut qui revient, et il faut le dire tel quel.
>
> **Et une demi-seconde après**, quand l'image ressort de la fenêtre. Une fenêtre qu'on sort de sa mère garde les nombres qu'elle avait, et l'écran les lit à partir d'une autre origine : ce qui était « le coin de notre intérieur » devient « le coin de l'écran ». Remise droite juste après, comme c'était le cas, il reste entre les deux un instant où l'image se tient au coin du bureau à sa taille pleine, **complètement en dehors de notre fenêtre**, et cet instant est un appel au système qui doit atteindre un autre programme et l'attendre, donc assez long pour être attrapé. Il l'a été. L'image reçoit maintenant ses nombres d'arrivée **pendant qu'elle est encore dans la fenêtre** : les mêmes nombres se lisent juste à la seconde où elle en sort. En échange ils sont faux tant qu'elle est encore dedans, donc un coin de la page peut apparaître le temps d'une image, mais une fenêtre-fille est découpée par sa mère : ça ne peut pas sortir de la fenêtre, et c'est toute la différence entre les deux erreurs.
>
> **Si l'écran distant apparaît zoomé un instant**, comme un changement de résolution beaucoup trop bref pour en être un, l'image a porté une taille à laquelle son lecteur n'avait pas encore dessiné : le compositeur étire alors ce qu'il a sous la main pour remplir. Une seule cause possible, donc une seule chose à vérifier : **`image redimensionnée` doit dire `1 fois` par geste**. Deux fois veut dire qu'on lui a donné une taille, puis une autre, et que la première était fausse.
>
> C'est arrivé en devinant l'intérieur futur à partir de la proposition moins ce que coûte le cadre d'aujourd'hui. Ça tient sous la main, où le cadre est le même avant et après ; c'est faux pour « agrandir », où le cadre lui-même change. L'intérieur futur est maintenant demandé au système, au message où il le calcule et avant que rien n'ait bougé, donc il est exact et il n'y a plus qu'une taille par geste.
>
> **Si la page ZyrDesk apparaît en éclair pendant que la fenêtre grandit**, ce n'est pas la traversée mais la règle « l'image n'est jamais la plus petite des deux » qui n'a pas été appliquée. L'image reçoit la taille de l'intérieur tel qu'il est au début du geste ; si le système agrandit ensuite sans qu'on ait posé l'image sur l'intérieur futur, elle reste plus petite que lui de tout ce que la fenêtre vient de gagner, et une fenêtre rangée dans une autre ne grandit pas avec elle. Ce qui se voit dans l'écart, c'est notre propre page. La règle existe depuis le travail sur le glissement ; le piège est qu'elle ne soit consultée que sous la main et pas quand c'est le système qui agrandit.
>
> **Les deux lignes « mauvaise lecture »** encadrent le geste, une à chaque bout, et disent la seule chose qu'aucune lecture du résultat ne peut donner : le résultat est juste à tous les coups, c'est la traversée qui clignote. Une fenêtre garde ses nombres quand elle cesse d'être rangée dans une autre, et l'écran se met à les lire à partir d'une autre origine : ce qui voulait dire « le coin de notre intérieur » se met à vouloir dire « le coin de l'écran ». L'une des deux lectures est fausse le temps de la traversée, et **chaque appel qui doit atteindre le programme du lecteur et attendre sa réponse allonge ce temps**. Un écran se redessine toutes les 16,7 ms : au-delà, le compositeur se réveille forcément dedans et ça se voit.
>
> Donc la seule chose à regarder sur ces deux lignes est **le premier nombre**. Sous 5 ms, la traversée passe presque toujours entre deux images. Au-delà de 16, elle est vue à tous les coups.
>
> Et la traversée du début tombe **sur le clic**, celle de la fin une demi-seconde après. « L'image sautille une fois quand on vient d'agrandir » désigne la première ; « l'image apparaît loin en dehors de la fenêtre » désignait la seconde.
>
> **L'image est rangée dans la fenêtre à l'ouverture de la session, avant d'être montrée, et y reste jusqu'à la fin.** Avant, c'était au premier geste, et l'éclair restant était exactement ça : la traversée dure environ une milliseconde et demie, elle tombait sur une fenêtre en pleine vue et se dessinait à peu près une fois sur onze. Elle se dessinait : le journal a fini par attraper l'image posée en (594, 278) là où (297, 139) était demandé, soit deux fois le coin de notre intérieur au pixel près, une fois, au premier geste. Faite pendant que l'image n'est pas encore à l'écran, il n'y a plus rien à dessiner de travers.
>
> **Ce qu'une image rangée dans notre fenêtre cesse d'être, et que tout le reste doit savoir : une fenêtre de premier niveau.** Deux choses la cherchaient en parcourant les fenêtres du système, ce qui ne parcourt que le premier niveau, et ne la trouvaient donc plus : **le bouton flottant** ne montait plus du tout (il attend de savoir où est l'image avant de se montrer) et **les raccourcis du moteur** étaient refusés, la question « le premier plan est-il au lecteur ? » répondant non pour toute la session puisque le premier plan est à nous. Une fenêtre qu'on tient ne se cherche pas : elle se demande à la partie du programme qui la tient, et « le premier plan est à la session » veut dire à nous ou au lecteur, pas au lecteur seul.
>
> **Ce qui a dû suivre l'image sur ce chemin-là**, et qu'il faut vérifier à l'essai puisque ces deux choses ne passaient que par l'autre : la **taille redite au lecteur** au démarrage (`taille de l'image redite au lecteur`, sans quoi le lecteur dessine jusqu'à 155 pixels trop court dans sa propre fenêtre) et la **coupe des deux coins du bas** (`découpe de l'image posée`, sans quoi la session a des angles droits dans un cadre arrondi). La coupe manquait d'ailleurs déjà après le premier geste dans la version d'avant.
>
> **L'image était rangée dans la fenêtre au premier geste d'une session et y restait jusqu'à la fin.** C'est le seul remède à l'éclair, et il ne tient pas à la durée d'une traversée mais à leur nombre. Chacune dure environ une milliseconde et demie contre une image écran de 16,7 ms, soit une chance sur onze d'être dessinée ; il y en avait deux par geste, donc une session de vingt gestes en montrait quatre. Une seule traversée par session, c'est une chance sur onze d'en voir une, jamais.
>
> **Le prix, et c'est ce qu'il faut essayer en premier : le clavier.** Une fenêtre rangée dans une autre n'est jamais la fenêtre de premier plan, et le clavier va au premier plan. Donc : lancer une session, bouger la fenêtre une fois pour déclencher le rangement, puis **taper quelque chose sur l'ordinateur distant**. La souris, elle, doit marcher dans tous les cas, parce qu'un clic va à ce qui est physiquement sous le curseur.
>
> **Le focus ne tient pas tout seul, et c'est le piège de ce mécanisme.** Partager une entrée entre les deux programmes rend le focus *donnable* à l'image, ça ne le lui laisse pas. Finir un geste réactive la fenêtre qu'on vient de manipuler, donc la nôtre, et notre propre vue web reprend le focus dedans ; cliquer n'importe où sur notre fenêtre fait pareil. La session devient alors sourde tout en ayant l'air parfaitement normale, ce qui est le pire des cas. Le focus est donc redonné à l'image à chaque moment qui peut le lui avoir pris, et le journal dit `le clavier est bien à la session` ou `le clavier n'est pas à la session : le focus a été refusé à l'image` quand ça change.
>
> **Le clavier est confié à la session au moment du rangement**, et le journal dit lequel des trois cas s'est produit : `clavier confié à la session : les deux programmes partagent une entrée, l'image a le focus` est le bon, les deux autres nomment ce qui a échoué. `clavier repris à la session` clôt le tout à la fin.
>
> L'essai a été fait sans ça d'abord, et le clavier ne passait pas : une fenêtre rangée dans une autre n'est jamais celle de premier plan, et le clavier va au premier plan. Passer les touches à la main ne répond pas non plus, pour deux raisons : elles n'arrivent même pas jusqu'à nous, la vue web sous l'image les prenant d'abord, et transmises elles arriveraient sans l'état qui dit lesquelles de majuscule, contrôle et alt sont enfoncées, cet état appartenant au fil qui les a vraiment reçues. Tous les raccourcis seraient faux. Les deux programmes partagent donc une seule entrée le temps de la session, et le focus se donne à travers.
>
> Ce que ça coûte, et qu'il faut surveiller : un programme qui cesse de répondre retient l'entrée de l'autre avec lui. Les deux s'attendent déjà plusieurs fois par seconde, donc aucun ne peut se taire sans que la session s'arrête de toute façon, mais si l'interface se fige pendant une session, c'est la première chose à soupçonner.
>
> **L'image reste dans la fenêtre une demi-seconde après chaque geste**, et pas seulement après un agrandissement. La raison est une affaire de fréquence et pas de durée : chaque traversée dure une à trois millisecondes contre un écran qui se redessine toutes les 16,7, donc environ une chance sur dix d'être vue, mais il y en avait **deux par geste**, y compris pour la plus petite poussée de la fenêtre. Trente poussées d'affilée font soixante occasions, et plusieurs finissent par se voir. Gardée d'un geste à l'autre, une série entière coûte une traversée au lieu de soixante.
>
> Le prix est à connaître et à vérifier : une image rangée dans notre fenêtre ne peut pas être la fenêtre de premier plan, donc **pendant cette demi-seconde le clavier ne part pas vers l'ordinateur distant**. La souris, si, parce qu'un clic va à ce qui est dessous. Et le premier plan est rendu à l'image dès qu'elle ressort, **sauf si quelqu'un est parti ailleurs entre-temps** : à essayer, cliquer sur une autre application juste après avoir bougé la fenêtre, ZyrDesk ne doit pas reprendre l'écran une demi-seconde plus tard.
>
> **Enchaîner les gestes** fait partie de l'essai : agrandir puis attraper tout de suite la barre de titre, agrandir deux fois de suite très vite, ancrer puis agrandir. L'attente d'une demi-seconde ne doit jamais se terminer pendant qu'une main tient la fenêtre, sinon la fin du geste se joue à deux fenêtres et le scintillement revient.
>
> **Le journal chiffre le geste.** Une ligne est écrite à chaque fois, quand l'image ressort de la fenêtre :
>
> ```
> agrandissement rendu au système : agrandie en 502 ms, image redimensionnée 1 fois ; fenêtre Some((-9, -9, 1929, 1149)), cadre dessiné Some((0, 0, 1920, 1140)), image Some((0, 0, 1920, 1111)), dedans Some(((0, 29), 1920, 1111))
> ```
>
> Le nom au début est celui du geste : `agrandissement`, `retour en fenêtre`, ou `ancrage` quand c'est la fenêtre poussée contre un bord de l'écran. Elle a un seul travail : dire laquelle des deux moitiés a échoué si l'ancien défaut revient. Vu de l'extérieur, « l'image a bondi et le cadre a suivi » a exactement la même tête que l'image posée au mauvais endroit, ce que ces nombres montrent, ou que le compositeur qui n'étire pas ce qui est rangé dedans, ce qu'ils ne peuvent pas montrer mais qui est alors la seule explication restante.
>
> 1. **Image redimensionnée N fois.** **Un**, c'est tout l'objet : la fenêtre change de taille une fois, l'image dedans une fois, et le compositeur s'occupe du reste. Treize voudrait dire que quelque chose rejoue le mouvement cran par cran dans notre dos. Compté depuis le moment où l'image est rangée dans la fenêtre, et pas depuis celui où l'attente est armée : l'attente n'est armée qu'une fois l'ordre passé, et l'unique redimensionnement du geste a lieu pendant ce passage-là. Compté depuis l'attente, il affichait zéro à tous les coups.
>
> **Et ce redimensionnement-là ne recopie rien.** C'est le seul du geste, mais il est aussi le seul endroit où l'image change de taille, et sans le dire franchement au système, celui-ci recopie l'ancienne image dans le coin du nouveau cadre et l'y laisse jusqu'à ce que le lecteur redessine. Le lecteur dessine trente-sept fois par seconde, donc ça fait jusqu'à vingt-sept millisecondes d'écran distant à la mauvaise taille dans le coin de la bonne : **un sautillement, une fois, vu souvent mais pas toujours**. L'autre chemin qui redimensionne l'image porte un commentaire de dix lignes là-dessus depuis longtemps ; celui-ci a été écrit sans.
> 2. **Fenêtre, cadre dessiné, image, dedans.** Les quatre rectangles côte à côte à la fin du geste. `image` et `dedans` doivent se correspondre : si l'image est là où l'intérieur de la fenêtre est, notre moitié est juste.
> 3. **En N ms.** L'attente avant de ressortir l'image, une demi-seconde environ. Windows ne prévient pas quand son animation est finie et sa durée n'est écrite nulle part : c'est une marge prise large, parce que ressortir l'image trop tôt fait bondir l'écran distant alors que la ressortir trop tard ne se voit pas du tout.

> **S9quinquies (Alt+Tab montre la session, pas l'écran d'accueil)**
>
> Pendant une session, faire **Alt+Tab** et regarder la vignette de ZyrDesk. Puis, sans session, refaire Alt+Tab.
>
> Attendu : pendant une session, la vignette montre **l'écran de l'ordinateur distant**. Sans session, elle montre l'écran d'accueil, comme n'importe quelle fenêtre. Passer aussi la souris sur le bouton ZyrDesk de la barre des tâches : le grand aperçu montre la même chose.
>
> Ce que ces vignettes montrent est une photographie que Windows prend d'une fenêtre. Il en photographie **une**, et la session est dans une autre posée par-dessus : il rendait donc l'écran d'accueil, c'est-à-dire la page que la session est en train de cacher. Windows sait demander sa photo à un programme plutôt que de la prendre lui-même, et c'est ce qui lui est répondu ici.
>
> **Si la vignette est noire**, dis-le : copier une fenêtre qui dessine directement sur la carte graphique n'est pas toujours possible, et cela dépend de la machine. La réponse n'est alors pas donnée du tout et Windows reprend sa propre photo, donc une vignette **noire** serait un vrai défaut, à la différence d'une vignette qui montre l'accueil.
>
> **Depuis S9sexies, ci-dessous, taper Alt+Tab pendant une session n'ouvre plus le sélecteur de ce PC-là** : la combinaison part vers l'ordinateur distant. Pour comparer les deux vignettes il faut donc l'ouvrir autrement : cliquer d'abord sur le **bouton flottant** avant de taper Alt+Tab, ou passer directement la souris sur l'icône de ZyrDesk dans la barre des tâches, ce que la phrase du dessus couvre déjà.

> **S9sexies (Alt+Tab et la touche Windows agissent sur l'ordinateur distant)**
>
> Sur le **PC hôte**, avant de se connecter, ouvrir deux ou trois fenêtres bien reconnaissables (le Bloc-notes, l'Explorateur de fichiers). Depuis le **PC client**, ouvrir une session, cliquer dans l'image pour lui donner le clavier, puis taper **Alt+Tab**.
>
> Attendu : dans l'**image**, le bureau distant change de fenêtre au premier plan, exactement comme si Alt+Tab avait été tapé assis devant le PC hôte. Sur le **PC client**, rien ne bouge : pas de sélecteur de fenêtres local, ZyrDesk garde le premier plan et sa barre de titre reste allumée.
>
> Essayer aussi **Alt+Échap** et **Ctrl+Échap**, qui suivent le même chemin.
>
> **À refaire après être passé par le bouton flottant** : ouvrir son menu, le refermer, puis rejouer Alt+Tab **tout de suite**. C'est le chemin qui a lâché quatre fois, et c'est celui qui compte le plus.
>
> **Première cause, et le journal la donne à la seconde près.** Windows appelle la reprise des touches et **chaque frappe de tout l'ordinateur attend cette réponse** ; passé un tiers de seconde, il remet la touche comme s'il n'y avait pas de reprise du tout. Or ZyrDesk posait, à chaque touche, deux questions au gestionnaire de fenêtres : où est le premier plan, et l'image existe-t-elle encore. Ces questions attendent quand **un autre fil du même programme déplace des fenêtres**, ce qui prend une demi-seconde. Le journal montre les deux collés : `retour en fenêtre rendu au système : en fenêtre en 489 ms`, et à cette même seconde le premier plan qui part au sélecteur de Windows et un relâchement de Tab arrivant sans son appui.
>
> **Plus rien n'est demandé depuis là.** Le premier plan est calculé ailleurs et laissé sous forme de nombre ; la fenêtre de l'image est lue comme un nombre aussi. Un nombre périmé coûte un message envoyé dans le vide, que le système refuse et qui ne coûte rien.
>
> **Deuxième cause : ce nombre était vieux d'une seconde.** Il n'était recalculé qu'au redessin de la barre de titre et à chaque tour de la surveillance de session. Le sélecteur de Windows, lui, prend le premier plan et le rend en bien moins que ça, et le journal montre la conséquence : `barre de titre active : le premier plan est à ZyrDesk` à 18:23:40, puis la touche suivante refusée pour `premier plan ailleurs` à 18:23:41. Un seul refus entretient le suivant, puisque la touche laissée passer rouvre ce sélecteur. Le premier plan est maintenant **suivi** : Windows le dit à l'instant où il le déplace, et le journal le nomme à chaque fois (`le premier plan passe ailleurs : processus N (xxx.exe), titre « ... »`).
>
> **Troisième cause, et c'est bien le bouton flottant.** Cliquer dessus donne le focus à sa page ; donner le focus à une fenêtre active celle-ci ou celle dont elle dépend, et cette fenêtre-là est marquée pour ne jamais être activée. Résultat : notre propre fenêtre perd le premier plan sans que rien ne l'ait pris, et il tombe sur ce qu'il y a derrière, le bureau de Windows quand la session est en fenêtre. Le journal le montre une seconde après la fermeture du menu, `le premier plan est ailleurs : processus 34640 (explorer.exe)`, avant le moindre Alt+Tab. Refermer le menu redemande donc le premier plan pour la fenêtre de ZyrDesk, et le journal dit ce que Windows en a fait. Cette demande n'est faite que là, que pendant une session, et que si le premier plan a réellement quitté ZyrDesk : partir vers un autre programme pendant que le menu est ouvert doit continuer de marcher, et c'est ce que S19 vérifie.
>
> **Une chose a été essayée et refusée par Windows** : réclamer ces combinaisons au système, comme ZyrDesk réclame ses propres raccourcis, ce qui aurait été plus propre. Le journal a répondu `1 combinaison tenue, 3 refusées`. Alt+Tab, Alt+Maj+Tab et Alt+Échap sont à Windows et il ne les cède pas. Se mettre devant les frappes n'est donc pas un choix : c'est le seul moyen.
>
> **Ce qui reste imparfait** : menu du bouton flottant **ouvert**, le clavier est à ce menu, donc l'ordinateur distant reçoit un Tab seul plutôt qu'un Alt+Tab. Une seconde ou deux par ouverture de menu. À signaler si ça gêne, pas à confondre avec le défaut ci-dessus.
>
> **La touche Windows, elle, reste celle de ce PC-là** et ouvre le menu Démarrer d'ici. C'est la seule que ce chemin ne peut pas servir : le moteur refuse de la transmettre à l'ordinateur distant tant que sa propre capture des touches système ne tourne pas, ce qui dans ce produit n'arrive jamais. La reprendre n'ouvrirait donc de menu nulle part, ce qui serait pire.
>
> **Comment revenir sur ce PC-là.** Ces touches partent au loin dès que l'image tient le clavier : pour joindre une autre fenêtre d'ici pendant ce temps, c'est la souris, un clic sur sa vignette dans la barre des tâches par exemple, comme le fait S9bis plus bas. Les raccourcis de ZyrDesk, eux, marchent toujours : voir S20 juste en dessous.
>
> **Ce qui a changé sous le capot, et pourquoi il a fallu s'y reprendre.** Le moteur client a une option qui fait exactement ça, elle lui a été demandée, et elle vient d'être retirée. Deux raisons. D'abord il ne peut pas s'en servir : il décide qu'il tient le clavier en comparant sa propre fenêtre à celle que le système appelle « la fenêtre du premier plan », or sa fenêtre est portée dans la nôtre, donc c'est une fenêtre fille, et une fenêtre fille n'est jamais celle-là ; quelques secondes après le début il en conclut qu'il a perdu le clavier et lâche tout. Ensuite, et surtout, la façon dont il reprend ces touches est d'**avaler Alt et Ctrl en entier** avant que quiconque les voie, ce qui coupait tous les raccourcis de ZyrDesk (S20).
>
> La fenêtre que le système appelle celle du premier plan, c'est la nôtre. C'est donc ZyrDesk qui reprend ces touches, sans toucher ni à Alt ni à Ctrl, et qui les porte à l'image telles quelles. Le moteur reçoit une frappe ordinaire à sa propre fenêtre et la transmet comme n'importe quelle autre : rien n'est ajouté dedans, il ne lui est rien demandé de nouveau.
>
> Le journal le dit une fois par seconde au plus, jamais depuis le chemin des touches lui-même. La ligne à lire :
>
> `touches système : N frappe(s) vues, M candidate(s), K portée(s) ; [compte de chaque réponse] ; la dernière était [...]`
>
> - **N à zéro** : ZyrDesk ne voit passer aucune touche, le mécanisme n'est pas branché.
> - **N qui monte, M à zéro** : il est branché, mais Alt+Tab ne lui parvient pas.
> - **M qui monte, K à zéro** : elles lui parviennent et il les laisse passer, et le compte de chaque réponse dit pourquoi, en toutes lettres.
> - **K qui monte d'une unité par Alt+Tab** : c'est ce qu'on veut.
>
> **La suite de la ligne dit si la touche arrive seulement jusqu'ici.** Un Alt+Tab fait quatre frappes, et `vues : Tab X enfoncée(s) et Y relâchée(s), Alt A et B` les compte à part. **X et Y doivent être égaux**, comme A et B : une session ne peut pas tenir deux relâchements de Tab pour un appui. Un appui qui manque veut dire que Windows n'a pas appelé ZyrDesk pour cette touche-là, ce qui est un défaut d'une autre nature que tous les précédents, et les trois nombres suivants disent lequel :
>
> - `au plus X ms d'attente avant nous` : ce que le système, et tout autre programme accroché devant nous, a consommé avant de nous passer la touche. Grand, l'attente n'est pas la nôtre.
> - `Y µs chez nous` : ce que ZyrDesk a mis à répondre. C'est le seul dont ce programme réponde, et il doit rester très petit ; le système rend la touche telle quelle passé un tiers de seconde, soit 300 000 µs.
> - `Z appel(s) hors sujet` : des appels qui ne parlaient pas d'une frappe. Zéro attendu.
> - `N portée(s) sauvée(s) par le délai de grâce` : combien de frappes ont été portées à la session alors que le premier plan brut était déjà ailleurs. Fermer le menu du bouton flottant fait rebondir le premier plan une fraction de seconde sur le shell de Windows avant qu'il ne revienne ; une frappe tombée pile là était laissée passer et ouvrait le sélecteur local, qui prenait alors le premier plan pour de vrai et bloquait tout le reste. Un premier plan parti depuis moins d'une demi-seconde n'est plus compté comme perdu. Non nul après un passage par le bouton, avec les Alt+Tab qui continuent de passer à la session, c'est ce délai qui a coupé la cascade.
>
> **Deux réponses à ne plus confondre.** `relâchements de touches laissées passer` est normal : l'appui est bien arrivé et ZyrDesk l'a laissé au système exprès, la session n'étant plus devant. `relâchements dont l'appui n'est jamais arrivé jusqu'ici` est le vrai défaut : Windows ne nous a pas appelés pour cet appui. Les deux se ressemblaient et étaient comptées ensemble, ce qui a rendu un journal ambigu.
>
> **Ce qui ne doit surtout plus revenir.** Une version reposait le crochet en démontant son fil depuis le fil qui dessine, ce qui bloquait le clavier de tout l'ordinateur le temps de le faire : « ça m'a carrément bloqué le alt tab sur mon propre pc ». Cette reposée est retirée en entier. Si un essai ramène un clavier figé, même une fraction de seconde, c'est à dire immédiatement.
>
> Et `relâchements dont l'appui n'est jamais arrivé jusqu'ici` compte exactement le défaut ci-dessus, séparé de `relâchements de touches laissées passer`, qui lui est normal : c'est le retour d'un Alt+Tab que ZyrDesk a laissé au système exprès, parce que la session n'était plus devant.
>
> Le compte est par **réponse** et pas seulement sur la dernière touche, et c'est ce qui a fini par trancher : Alt+Tab arrive par paires de sens opposé, celle qui sort de la session et doit partir au loin, et celle qui y revient et ne doit pas. Lue sur la dernière touche seulement, une session où toutes les sorties échouent et toutes les rentrées sont correctement refusées se lit comme une session où tout va bien.

> **S20 (les raccourcis de ZyrDesk marchent pendant toute la session)**
>
> Pendant une session, essayer les trois raccourcis de la fenêtre **Réglages**, section **Raccourcis clavier** : celui du plein écran, celui du menu du bouton flottant, celui qui met fin à la session. À faire **dès les premières secondes** de la session, puis de nouveau après une minute, puis après un passage par le menu du bouton flottant.
>
> Attendu : les trois marchent à chaque fois, sans exception.
>
> C'est l'essai qui a manqué. Ces raccourcis sont tous des combinaisons **Alt**, et le moteur, tant qu'il reprenait les touches du système, avalait Alt en entier avant que ZyrDesk ne le voie. Dit par Victor : « je perdais mes raccourcis clavier de zyrdesk comme par exemple alt + & pour switcher plein ecran/fenetré ». Le symptôme était fuyant parce qu'il ne durait que le début d'une session : dès qu'on touchait au bouton flottant, le moteur lâchait ces touches et les raccourcis revenaient.

> **S21 (aucune touche ne reste coincée)**
>
> Pendant une session, ouvrir le bloc-notes de l'ordinateur distant et **taper une phrase entière**. Puis provoquer exprès une perte de clavier : ouvrir le menu du bouton flottant et le refermer, cliquer sur une autre fenêtre de ce PC-là puis revenir dans l'image, taper Alt+Tab. Après chacune, **retaper une phrase entière**.
>
> Attendu : le texte s'écrit à chaque fois, en entier, lettres normales.
>
> Ce que ça cherche : une touche modificatrice restée enfoncée **du côté distant**. Si le clavier part vers l'image alors qu'Alt est enfoncé, et que le clavier lui est repris avant qu'Alt ne remonte, l'ordinateur distant ne voit jamais Alt remonter et croit qu'il est tenu pour toujours. Tout ce qu'on tape ensuite y arrive en Alt + lettre : rien ne s'écrit, et **ça ressemble trait pour trait à un clavier mort**. Dit par Victor : « j'ai même carrément perdu le clavier dans la session ». Le moteur le signalait sans qu'on le lise, dans son propre journal, en trois mots à la fin de chaque session : `Raising 1 keys`, une touche encore enfoncée.
>
> ZyrDesk relâche, du côté distant, chaque modificatrice qu'aucun doigt ne tient. **À chaque tour de la surveillance de session**, soit environ une fois par seconde, et non plus seulement quand le clavier revient à l'image : cette condition-là n'était jamais remplie, parce que ce qui abandonne une touche c'est le premier plan qui s'en va, et le clavier ne le suit pas forcément. La correction n'a donc jamais eu lieu une seule fois, et le défaut est revenu tel quel.
>
> Le signe à chercher, si ça devait revenir encore, est cette ligne à la fin du journal du **moteur client** : `Raising N keys`. Elle ne doit plus y être.

> **S19 (ces touches redeviennent celles de ce PC-là dès qu'il n'y a plus de session)**
>
> C'est l'essai qui compte le plus du lot, parce que le défaut qu'il cherche serait pénible : Alt+Tab ou la touche Windows qui ne répondent plus **sur ce PC-là** alors qu'il n'y a plus de session.
>
> Trois moments à essayer, dans l'ordre :
>
> 1. **Pendant une session, en ayant cliqué sur une autre fenêtre de ce PC.** Cliquer sur le Bloc-notes local par exemple, puis taper Alt+Tab : le sélecteur **de ce PC-là** doit s'ouvrir normalement. C'est le cas qui compte le plus, et il est lu à chaque touche : le premier plan n'est plus à la session, donc la touche part au système.
> 2. **En quittant ZyrDesk pendant que le menu du bouton flottant est ouvert.** Ouvrir ce menu, puis, sans le refermer, cliquer sur une autre fenêtre de ce PC : elle doit **rester** au premier plan. ZyrDesk redemande le premier plan en refermant ce menu, et il ne doit le faire que quand c'est lui qui l'a perdu tout seul, jamais quand on est parti volontairement.
> 3. **Session terminée.** Fermer la session par la croix, revenir à l'accueil, taper Alt+Tab et Ctrl+Échap : tout doit être redevenu **strictement normal** sur ce PC.
> 4. **ZyrDesk fermé.** Quitter le programme entièrement, puis refaire les deux : normal aussi.
>
> Essayer également, pendant une session, **Tab seul** et **Échap seul** dans une fenêtre de l'ordinateur distant : ce sont des touches ordinaires, elles ne sont pas reprises et doivent faire ce qu'elles font toujours.
>
> Si l'un de ces quatre moments échoue, fermer ZyrDesk suffit à tout remettre en place, et il faut le dire.

> **S8sexies (l'image descend jusqu'au bas de la fenêtre)**
>
> Pendant une session, regarder le **bas** de la fenêtre, juste au-dessus du liseré de couleur. À faire sur chaque écran, et surtout sur un écran très défini où Windows agrandit l'affichage.
>
> Attendu : **rien entre l'image et le liseré**. Pas de ligne claire, pas même de deux pixels.
>
> L'image est une fenêtre à part et il lui est demandé de couvrir tout l'intérieur de la nôtre ; ce qu'elle laisse à découvert est une bande de la page derrière elle. Une fenêtre appartient au programme qui l'a ouverte, et ce programme peut répondre à une demande de taille par une taille à lui : une taille minimale, un pas auquel il arrondit, ou celle que le système lui donne quand lui et nous ne mesurons pas un écran de la même façon. Aucune de ces trois-là ne se lit sur une capture d'écran.
>
> **Le test qui tranche** : la bande est-elle toujours là **après avoir agrandi la fenêtre puis l'avoir remise en fenêtre** ? Si elle disparaît à ce moment, la cause est celle décrite ci-dessous et le correctif tient ; si elle reste, elle est ailleurs.
>
> Le lecteur jette les changements de taille qui lui arrivent pendant qu'il vide sa file au démarrage, et ne redemande jamais : sa fenêtre annonce une taille et ce qu'il dessine en fait une autre, d'où une bande de fenêtre vide en bas de l'image pour toute la session. Son propre journal les nomme, `dropping window event during flush`, et celui qu'il jette est le nôtre. Sa taille lui est donc redite une fois, quand la session est réellement établie et que ce vidage est fini depuis longtemps ; le journal l'écrit, `taille de l'image redite au lecteur : LxH`. Le vrai correctif est un correctif du moteur lui-même, et celui-ci tiendra en attendant.
>
> **Le journal donne la mesure.** Deux lignes à chercher. `coins de l'image : image LxH, bordure de N px, rayon de M px` dit à quelle taille l'image est posée et comment ses coins sont découpés. Et `image demandée en [...], posée en [...] : écart de [...] sur les quatre bords` n'apparaît que si l'image n'a pas obtenu le rectangle qu'on lui a donné : la bande claire vaut alors exactement cet écart. Sans cette seconde ligne, l'image couvre l'intérieur de la fenêtre au pixel près et la bande vient d'ailleurs, ce qui est déjà une réponse.

> **S9quinquies (revenir en fenêtre revient à la bonne taille)**
>
> En session, mettre la fenêtre à une taille bien reconnaissable, par exemple un petit rectangle dans un coin de l'écran. Agrandir. Puis **Niveau inférieur**.
>
> Attendu : la fenêtre revient **exactement** au petit rectangle, à sa taille et à sa place. Recommencer trois ou quatre fois d'affilée : elle doit retomber au même endroit à chaque tour, sans grandir petit à petit.
>
> Porter la fenêtre pas à pas ressemble, pour Windows, à une main qui la déplacerait : il notait chaque pas comme étant « la place de cette fenêtre ». Agrandir finissait donc par lui apprendre que sa place, c'était l'écran entier, et le retour revenait à peu près à l'écran entier. La place est maintenant lue avant que le premier pas ne bouge quoi que ce soit, et réécrite telle quelle à la fin, avec seulement l'état demandé posé dessus.

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
> **Puis l'inverse**, qui compte autant : cliquer sur une autre fenêtre, par exemple depuis la barre des tâches. La barre doit **griser** immédiatement, comme il se doit. Revenir sur ZyrDesk : elle se rallume.
>
> Cliquer et non Alt+Tab, depuis S9sexies (plus haut) : cette touche part maintenant vers l'ordinateur distant dès que l'image tient le clavier, et ne fait plus perdre le premier plan à ZyrDesk du tout. Un clic à la souris sur une autre fenêtre, lui, n'a jamais dépendu de ça.
>
> Le premier plan appartient au lecteur pendant presque toute une session, et au bouton flottant quand une main le touche : ni l'un ni l'autre n'est « quelqu'un d'autre », et la fenêtre est bel et bien celle qu'on utilise. Windows pose la question au moment même où il change de premier plan, quand ce qu'il est en train de donner n'est pas encore posé : la réponse est donc donnée deux fois, une tout de suite et une par un message que le programme s'envoie à lui-même et que Windows ne rend qu'une fois l'affaire finie.
>
> **Le journal note chaque bascule** : `barre de titre active` ou `inactive`, avec à qui est le premier plan, à ZyrDesk, à l'image, ou ailleurs. Une bascule vers `inactive` pendant l'une des quatre étapes ci-dessus est le défaut, et la ligne dit lequel des trois cas c'était.

> **S9bis (le bouton flottant reste chez lui)**
>
> Pendant une session en fenêtré, **cliquer sur une autre fenêtre** (par exemple depuis la barre des tâches), la regarder quelques secondes, puis revenir sur ZyrDesk.
>
> Attendu : le bouton flottant **disparaît** dès que l'autre application passe devant, et **revient** quand ZyrDesk ou l'image reprend le premier plan. Il ne flotte jamais au-dessus du travail de quelqu'un d'autre.
>
> Il est dessiné au-dessus de toutes les fenêtres de la machine, ce qu'il faut pour tenir sur l'image ; il suit donc le premier plan, qui est celui de l'image autant que le nôtre puisque l'image appartient au lecteur.
>
> **Un clic, et non plus Alt+Tab.** Cet essai passait par Alt+Tab pour faire perdre le premier plan à ZyrDesk ; depuis S9sexies (plus haut), cette touche part vers l'ordinateur distant dès que l'image tient le clavier, et ne fait plus perdre le premier plan à ZyrDesk du tout. Un clic à la souris atteint le même but sans dépendre de ça, ce qui est tout ce que cet essai a jamais vérifié : que le bouton suit le premier plan, peu importe ce qui le lui a fait perdre.

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

> **S18ter (la croix marche aussi quand la session a lâché)**
>
> Ouvrir une session, puis couper l'ordinateur d'en face en pleine session : débrancher son câble réseau, couper son Wi-Fi, ou l'éteindre. L'image se fige. Cliquer sur la **croix**.
>
> Attendu : au bout de **trois secondes au plus**, l'image disparaît et l'accueil revient. Pas quinze secondes, pas « rien du tout jusqu'à ce que ça revienne tout seul ».
>
> **Pourquoi trois secondes.** Fermer proprement veut dire rendre son bureau à l'ordinateur d'en face, et ça se demande **à travers le tunnel**, donc à un ordinateur qui peut très bien ne plus répondre. La question est posée sur un fil à part et personne ne l'attend : l'image a trois secondes pour s'en aller toute seule, ce qu'elle fait quand la réponse arrive, et sinon elle est arrêtée ici. La croix ramène à l'accueil dans tous les cas.
>
> Le journal dit lequel des deux chemins a été pris : `bureau distant rendu` quand la question a abouti, `bureau distant non rendu : …` sinon, et dans ce cas `l'ordinateur distant n'a pas rendu la main à temps : lecteur N arrêté ici`. **Aucune ligne rouge** ne doit traverser l'écran : c'est ce que la personne a demandé, pas une panne.

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

> **R17 (la taille, le débit et le codec se règlent depuis la session)**
>
> Pendant une session, ouvrir le menu du bouton flottant. Trois lignes nouvelles : **Taille**, **Débit**, **Codec**.
>
> Attendu : chacune porte un **chevron à gauche**, du côté où sa liste s'ouvre, et les icônes des trois lignes restent dans la même colonne que celles du reste du menu. Un clic sur une ligne **ouvre sa liste** de valeurs à sa gauche, avec une coche sur celle en place ; un clic dans la liste la choisit et referme. Une seule liste ouverte à la fois. La taille dit à quoi « Écran » revient sur ce PC-là (`Écran, 3840 x 2160`), sinon on ne saurait pas ce qu'on demande. **Rien ne bouge dans l'image en cours** : le choix est retenu, et c'est R34 qui le pose à l'écran.
>
> **Le point qui a lâché deux fois, à refaire dans cet ordre exact.** Ouvrir **Taille** (la liste la plus large), la refermer, ouvrir **Débit** (la plus étroite), la refermer, ouvrir **Codec**. Deux choses à regarder :
>
> 1. **Rien n'est coupé** : ni le bord droit du menu, ni le bas d'une liste, et aucune bande blanche ou vide à côté de quoi que ce soit.
> 2. **Rien ne clignote** : le menu ne doit pas disparaître ni se redessiner entre deux clics. La fenêtre du bouton prend sa taille au moment où la session s'ouvre, mesurée sur les trois listes à la fois, et n'en change plus ensuite. Une fenêtre qui change de taille fait remettre la page en page, et pendant ce temps-là rien n'est dessiné.
>
> Le journal tranche entre les deux, une ligne par changement de taille : `bouton flottant : 1630x1614 demandés, 91x91 avant, 1630x1614 après ; 2 morceaux dessinés jusqu'à 1098x1272`. Il doit y en avoir **deux ou trois au démarrage de la session, et plus aucune ensuite** : une ligne qui apparaît en cliquant dans le menu est un redimensionnement, donc un clignotement. « après » doit valoir « demandés », sinon c'est Windows qui a refusé la taille ; et « dessinés jusqu'à » doit rester en dessous, sinon la page dessine plus grand que sa fenêtre.
>
> Une ligne de plus est normale la première fois qu'on change un réglage : c'est **Appliquer les changements** qui apparaît et allonge le menu. Une seule fois par session.
>
> Le menu doit aussi porter le **même thème que le reste de ZyrDesk** : sombre sur une application sombre, clair sur une claire.
>
> Refermer le menu avec une liste ouverte, puis le rouvrir : les listes doivent être repliées. Une liste laissée ouverte garderait la fenêtre du bouton à sa hauteur de liste, ce qui pose une nappe invisible sur l'image et avale les clics.
>
> Régler quelque chose, **fermer la session, en rouvrir une** : les trois valeurs doivent être celles qu'on a laissées. C'est le point qui compte, sans quoi il faudrait tout refaire à chaque connexion.
>
> Puis **fermer et relancer ZyrDesk** : elles doivent encore être là. Elles vivent dans le service, pas dans la fenêtre.

> **R17bis (les réglages de l'app n'ont plus de section qualité)**
>
> Ouvrir les réglages (engrenage).
>
> Attendu : plus de boutons Fluide / Équilibré / Qualité. À la place, une ligne **Ce qu'une session demande** qui rappelle la taille, la cadence et le débit du moment. Elle doit suivre ce qui vient d'être réglé dans le menu de la session.

> **R34 (appliquer les changements sans fermer la session)**
>
> Pendant une session, ouvrir le menu du bouton flottant. Tant qu'on n'a rien changé, **aucune ligne « Appliquer »** ne doit s'y trouver.
>
> Changer la **taille**. Une ligne **Appliquer les changements** apparaît, en bleu, sous les trois réglages. Ne pas la cliquer : changer aussi le **débit** et le **codec**. La ligne reste, une seule fois. C'est tout l'intérêt : on règle ce qu'on veut, et l'image ne se relance qu'une fois.
>
> Cliquer **Appliquer les changements**.
>
> Attendu : le menu se referme, l'image disparaît quelques secondes, l'écran d'ouverture revient avec **Nouveaux réglages, l'image se relance…**, puis l'image revient **avec les nouvelles valeurs**. La fenêtre garde sa taille et son plein écran ; la session n'est pas fermée et on ne revient pas à l'accueil.
>
> Rouvrir le menu : la ligne **Appliquer** a disparu, puisque ce qui est choisi est de nouveau ce qui est à l'écran. Les trois lignes montrent bien les valeurs demandées.
>
> **Pourquoi ça relance l'image.** Le moteur apprend la taille, le débit et le codec **à son démarrage et jamais après** : il n'existe aucune façon de les lui changer en marche. Le reste du menu, lui, se demande au moteur en marche et prend effet tout de suite. C'est pour ça que ces trois-là seulement ont un bouton, et que les autres n'en ont pas.
>
> Le journal du client raconte la relance : `réglages appliqués : le lecteur N est relancé`, `lecteur N arrêté`, `image relancée avec ce qui est choisi maintenant`, puis les lignes d'une ouverture ordinaire.

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

## Partie 9 : l'écran virtuel

### Ce qui change, et pourquoi

Un ordinateur ne peut envoyer que ce qu'il dessine. Un portable en 1920 x 1080 à qui on demande du 4K agrandit ce qu'il a avant d'encoder : l'image remplit l'écran du client et elle est floue, pour quatre fois le débit. ZyrDesk fait donc pousser sur l'hôte un écran que Windows croit réel et dessine à la taille demandée. Le fond de l'affaire est dans [ECRAN-VIRTUEL.md](../ECRAN-VIRTUEL.md).

Le cas qui compte pour cette partie : **un PC client dont l'écran est plus grand que celui du PC hôte**. Un client 4K vers un portable 1080p est l'exemple parfait.

> **R27 (l'écran virtuel est là)**
>
> Sur le **PC hôte**, après installation : ouvrir le Gestionnaire de périphériques, dérouler **Cartes graphiques**.
>
> Attendu : une entrée **Virtual Display Driver** à côté de la vraie carte, sans point d'exclamation. Aucune fenêtre n'a demandé quoi que ce soit pendant l'installation.
>
> Si elle est absente, le journal du service dit à quelle étape ça a lâché : chercher `virtual screen`. Il n'y a rien d'autre à chercher, chaque étape y écrit une ligne.

> **R28 (le moteur le vise)**
>
> Sur le **PC hôte**, ouvrir le journal du service et chercher `screens the engine sees`.
>
> Attendu : la liste des écrans, dont un nommé **VDD by MTT**, puis la ligne `the engine is capturing the virtual screen (…)`. Au tout premier démarrage après l'installation, une ligne dit à la place que le moteur redémarre pour le viser : c'est normal et ça n'arrive qu'une fois.

> **R29 (le 4K est vraiment net)**
>
> Depuis le **PC client 4K**, qualité **Qualité**, ouvrir une session vers le portable 1080p, et mettre la fenêtre en plein écran.
>
> Attendu, dans l'ordre :
> - Le journal de la fenêtre annonce `écran de cet ordinateur : 3840x2160 pixels réels` puis `image demandée au loin en 3840x2160`, avec `l'écran est demandé entier, un pixel envoyé pour un pixel affiché`.
> - L'écran du portable **s'éteint** au début de la session, et se rallume à la fin. C'est voulu : le bureau entier déménage sur l'écran virtuel, sans quoi la session montrerait un bureau vide.
> - Le texte est **net**. C'est le seul juge. Ouvrir le bloc-notes sur le bureau distant : les lettres doivent être franches, pas baveuses.
>
> Le point de comparaison honnête est le même essai avant cette version : l'image remplissait déjà l'écran, mais floue.

> **R30 (tout est remis en place)**
>
> Fermer la session, aller voir le **PC hôte**.
>
> Attendu : son écran est rallumé, à sa taille d'origine, avec ses icônes là où elles étaient.

> **R31 (le retrait ne laisse rien)**
>
> Désinstaller ZyrDesk sur le **PC hôte**, puis rouvrir le Gestionnaire de périphériques.
>
> Attendu : plus de **Virtual Display Driver**, et aucun périphérique en erreur. Le journal du service porte `virtual screen device removed` et `taken out of the store`.

> **R32 (le plein écran est vraiment plein)**
>
> Session ouverte en plein écran, sur chacun des deux écrans si le PC en a deux. Regarder les quatre coins et les quatre bords.
>
> Attendu : **angles droits** aux quatre coins, aucun liseré clair en haut ni sur les côtés, et l'image touche le bord de l'écran partout. Repasser en fenêtre : les angles se réarrondissent, et c'est là qu'ils ont leur place.
>
> Le journal donne la mesure exacte si quelque chose reste : la ligne `cadre de la fenêtre :` dit l'écran, la fenêtre et son intérieur côte à côte. Les deux derniers nombres doivent être **0 px et 0 px** en plein écran ; tout ce qui n'est pas zéro est la largeur du liseré.

> **R33 (ce que cet ordinateur fait quand c'est lui qu'on regarde)**
>
> Sur le **PC hôte**, dans ses propres réglages : deux entrées nouvelles, **Renvoyer un écran immobile** et **Façon de filmer l'écran**.
>
> Ce sont des réglages de l'ordinateur qui **sert**, pas de celui qui regarde : ils ne changent rien à une session ouverte depuis lui, et tout à une session ouverte vers lui. C'est pour ça qu'ils sont ici et pas dans le menu de la session. Son moteur les lit à son démarrage, donc en changer un le redémarre, et coupe une session que quelqu'un aurait en cours vers cette machine.
>
> Attendu : le journal du service dit `this computer will serve with a steady rate ... and ... capture`, puis `how this computer serves was changed, the engine starts over with it`. Le moteur redémarre dans la foulée. Rouvrir une session depuis le PC client : elle doit s'ouvrir normalement.
>
> **Ce que ça sert à mesurer.** Ces deux réglages sont les deux seuls leviers qui restent sur la cadence quand ni la taille, ni le débit, ni le codec n'ont rien changé. Couper le renvoi d'un écran immobile enlève une image complète encodée soixante fois par seconde pour rien ; passer en **Rapide** change la façon dont Windows livre les images au moteur, ce qui n'a pas le même coût sur toutes les machines. Regarder `Host processing latency` après chacun.

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
- **L'image remplit l'écran mais reste floue.** Deux lignes tranchent, et rien d'autre. Sur le **client**, `image demandée au loin en …` dit la taille demandée : si elle est plus petite que l'écran, c'est le plafond de la qualité qui l'a rabotée, il faut monter d'une marche. Sur l'**hôte**, `no virtual screen among them` dit que l'écran virtuel n'est pas là : la taille est bien demandée, mais l'hôte ne sait pas la dessiner et agrandit la sienne. Remonter alors à `virtual screen` dans son journal, qui dit à quelle étape la pose a lâché.
- **La session s'ouvre puis se referme aussitôt, sans image.** Le journal de la fenêtre le raconte pas à pas, de `session asked for towards …` à `session ended: …`. Si la ligne `the far computer no longer knows this one` y figure, le produit s'est rattrapé tout seul et il n'y a rien à faire. Sinon, la fin du journal du moteur client (`session.log`) porte le dernier mot du moteur, qui est toujours la vraie raison.

Deux entrées du menu flottant méritent leur propre explication :

- **« La fenêtre de la session n'est pas au premier plan ».** C'est une sécurité : les raccourcis partent vers la fenêtre active, et ZyrDesk refuse de les envoyer ailleurs qu'à la session. Cliquer une fois dans l'image, puis rouvrir le menu.
- **Un clic sur le bouton part vers l'ordinateur distant.** Le mode souris est sur Jeu : le pointeur appartient alors entièrement à l'autre machine. Ctrl+Alt+Maj+M pour revenir à la souris de bureau.

---

## Essai A/B des touches système (Alt+Tab)

Deux façons de reprendre les touches que Windows garde pour lui coexistent le temps de les départager ([D43](../DECISIONS.md)). **La nouvelle est celle par défaut** : il n'y a rien à activer, une session l'utilise. Le réglage sert à revenir à l'ancienne, par une ligne du fichier de réglages du service :

```
system_keys_in_the_engine = yes   # le moteur les prend lui-même (défaut)
system_keys_in_the_engine = no    # ZyrDesk les prend et les remet au moteur
```

Le service doit être arrêté puis redémarré pour que la ligne soit relue. La ligne de commande revient à l'ancienne sans toucher au fichier : `zyr-cli connect … --system-keys-in-zyrdesk`.

Les moteurs doivent avoir été recompilés : un moteur d'avant ne connaît pas le mode demandé et refuse de démarrer. La routine de mise à jour habituelle s'en charge, à condition d'attendre que la compilation des moteurs ait abouti avant de les récupérer.

**Tout l'essai se fait pendant une seule session, sans jamais se reconnecter.** C'est le point important : la panne se déclenche une fois et ne se répare qu'à la reconnexion, donc un essai coupé en deux ne prouve rien.

1. Se connecter, puis **Alt+Tab tout de suite**. La fenêtre doit changer sur le PC hôte, jamais ici.
2. **Agrandir puis restaurer la fenêtre, cinq fois**, en retestant Alt+Tab après chacune.
3. **Basculer plein écran puis fenêtre, cinq fois**, en retestant après chacune.
4. **Ouvrir et refermer le bouton flottant plusieurs fois**, en retestant après chacune.
5. Utiliser **Statistiques** et le **changement de mode souris** depuis ce menu, puis retester.
6. **Aller volontairement dans une vraie application locale** (navigateur, terminal) : Alt+Tab doit alors rester ici.
7. **Revenir dans la session** en cliquant l'image : Alt+Tab doit repartir vers l'hôte.
8. Vérifier qu'aucune touche Alt ou Control n'est restée coincée, sur les deux machines, en tapant du texte.
9. Vérifier que **tous les raccourcis de ZyrDesk répondent encore** : plein écran, statistiques, mode souris, menu, fin de session.

Ce qu'il faut lire ensuite, selon la voie essayée :

- **Voie ZyrDesk** (`no`) : dans `interface.log`, la ligne `touches système : …`. `portées à la session` doit monter à chaque Alt+Tab, `premier plan ailleurs` doit rester à zéro tant que l'étape 6 n'a pas eu lieu, et `relâchements dont l'appui n'est jamais arrivé jusqu'ici` est le compteur de la panne : au-dessus de zéro, des appuis n'arrivent pas jusqu'au produit.
- **Voie moteur** (`yes`) : dans `interface.log`, la ligne `touches système laissées au moteur` doit apparaître à l'ouverture, et aucune ligne `touches système : …` ensuite, les deux voies ne pouvant pas tourner ensemble. Le reste est dans `session.log`, sous `zyr:` : `the session has the keyboard` à chaque reprise du clavier, et `system keys: Tab … carried to the host …` qui donne les appuis et relâchements vus, ce qui est parti vers l'hôte, ce qui a été laissé passer et pourquoi, et le nombre de fois où le crochet a été reposé.
