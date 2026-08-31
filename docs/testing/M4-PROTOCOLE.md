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
| **S9bis**, **S19**, **S21** | Touchés par le retrait de l'ancienne voie des touches système : ZyrDesk n'en prend plus aucune, c'est le moteur qui les prend. Le comportement attendu ne change pas, le chemin oui |
| **S9sexies** | **La voie qui restait est la seule.** Celle que ZyrDesk portait a été retirée en entier, réglage compris : elle ne pouvait pas marcher, un crochet du système étant servi du plus récent au plus ancien et le nôtre n'étant posé qu'une fois par session. Tout est expliqué dans [../CLAVIER.md](../CLAVIER.md), à lire avant de retoucher à ça |
| **R12sexies** | Un diagnostic si **Statistiques** ne montre toujours rien : le journal dit si un autre programme tient déjà cette combinaison |
| **R5** | Nouveau logo, et dessiné à chaque taille au lieu d'être réduit d'une seule : à comparer aux icônes voisines dans la barre des tâches |
| **R32** | Le plein écran n'a plus ni angles arrondis ni liseré, et l'image touche vraiment les quatre bords |
| **R33** | Deux réglages nouveaux côté hôte : renvoyer ou non un écran immobile, et la façon de filmer l'écran. Ce sont les deux seuls leviers qui restent sur la cadence |
| **S2**, **S7** | **Repris par R59 : c'est l'écran principal de l'hôte qui prend la taille demandée, et il la reprend à la fin.** Aucun écran n'est éteint et aucun n'est créé. La version précédente de cet essai, où le bureau déménageait sur un écran fabriqué pendant que les autres s'éteignaient, ne vaut plus |
| **R30**, **R31** | Ce qu'il reste de l'écran virtuel à vérifier : que tout soit bien remis en place à la fin d'une session, et que le retrait du produit ne laisse rien |
| **S6**, **S8** | Rien n'a changé pour eux, mais ils passent par le même chemin : à refaire une fois pour être sûr que l'écran virtuel ne réintroduit pas de bande noire |
| **S23** | Nouveau. Éteindre l'hôte depuis la session lui laissait son écran à la taille du client. Le moteur hôte est maintenant prié de partir avant d'être pris, et le journal du service dit lequel des deux s'est produit |
| **S24** | Nouveau. Le bouton flottant avait entièrement disparu à une reconnexion, sans une ligne nulle part. Une fenêtre qui n'a rien dessiné au bout de trois secondes est refermée et remontée, et tout ce qui l'empêche va au journal |
| **R12septies** | Nouveau. Une trace blanche restait derrière le bouton après avoir cliqué une entrée du menu, le curseur étant loin du logo. La découpe de la fenêtre se pose maintenant après le dessin et non avant |
| **R30** | **Une machine qui n'avait pas d'écran virtuel doit en avoir un au prochain démarrage du service.** Il n'était posé qu'à l'inscription du service, donc jamais sur un ordinateur inscrit avant que ce code existe. À vérifier dans `service.log` de l'hôte : `virtual screen already in place`, ou la suite des étapes de la pose |
| **S23** | **Refait, et c'est le sujet du lot.** Éteindre l'hôte depuis la session laissait toujours son écran dans la taille du client, même après un redémarrage complet. Le service lisait « emporté par sa session » comme une chute et redémarrait le moteur sur-le-champ : trois moteurs en cinq secondes pendant que la machine s'en allait, et le seul papier qui disait ce qu'étaient les écrans avant la session y est passé. Il attend maintenant au lieu de fournir |
| **S5** | Changé. En mode fenêtre, une session s'ouvre maintenant avec la fenêtre **agrandie** au lieu de la taille où elle avait été laissée. Le plein écran ne change pas |
| **R12**, **R17**, **R34** | Le menu de la session change de forme. « Souris bureau ou jeu » devient un interrupteur Bureau / Jeu qui montre où l'on en est. Taille, débit et codec deviennent trois curseurs au lieu de trois listes qui s'ouvraient sur le côté |
| **R12nonies** | Nouveau. Un liseré blanc apparaissait par moments sur la gauche du bouton, surtout en le déplaçant : la découpe de la fenêtre réclamait une colonne de pixels que la page ne peignait pas. Elle arrondit maintenant vers l'intérieur |
| **R12octies** | Nouveau. Le bouton flottant disparaissait dès que le premier plan partait ailleurs, ce qui coûtait son bouton à toute session regardée sur un deuxième écran. Il ne suit plus le premier plan du tout |
| **S27** | Nouveau. Un troisième ordinateur joint par un tunnel privé était découvert mais refusait toutes les sessions. La découverte n'apprenait qu'à celui qui appelle : celui qui était appelé ne retenait rien de son appelant, donc ne le reconnaissait pas. Celui qui appelle se présente maintenant |
| **S26** | Nouveau. Une ouverture sur deux ou trois sautait l'écran de chargement : l'accueil revenait avec la carte verte d'une session en cours, puis l'image apparaissait quelques secondes plus tard sans rien annoncer. L'écran d'ouverture partait quand le service prenait la session, pas quand il y avait une image. Il attend maintenant l'image |
| **R35** | Nouveau. Dans la barre du menu de la session, **Réseau** et la cadence sortaient vides depuis toujours, les deux autres chiffres étant justes. Le moteur ne les accumule pas comme les autres, il ne les pose qu'en fondant deux mesures ensemble, ce que sa propre surcouche fait et ce que nous ne faisions pas. Les quatre doivent maintenant porter un nombre |
| **R36** | **Refait, et c'est le sujet du lot.** La frappe partait bien et l'ordinateur d'en face répondait oui, sans que rien n'apparaisse à l'écran : elle était confiée à un aller simple lancé dans la session de l'écran, qui n'est ni un service ni un programme à manifeste, donc aucun des deux cas que Windows accepte. C'est le service lui-même qui presse maintenant. Le journal du service hôte écrit la stratégie réellement en place, sa propre session et celle de l'écran : c'est la ligne à lire si ça ne marche toujours pas |
| **R37** | Nouveau. Un interrupteur **Son** dans le menu de la session, Actif ou Coupé. Il coupe le son **de cette machine-ci**, sur la tranche du lecteur dans le mélangeur de Windows, et rien d'autre de ce qui joue ici |
| **R34** | **Refait.** « Appliquer les changements » relançait souvent l'image sans rien appliquer : la relance partait avant que le choix qu'on venait de faire soit écrit, et repartait donc sur les anciens réglages. Elle attend maintenant. À refaire sur les trois réglages, en cliquant **tout de suite** après avoir lâché le curseur |
| **R17** | Touché. Le codec n'est plus un curseur mais des boutons, comme la souris. Les autres restent des curseurs |
| **R40** | Nouveau. Le menu du bouton flottant était coupé quand le bouton était posé en bas de l'image. Il s'ouvre maintenant vers le haut quand il n'y a plus de place en dessous |
| **R41** | Nouveau. Le curseur passait au sens interdit pendant le déplacement du bouton flottant, là où c'est une main qui agrippe qu'il faut voir |
| **R42** | Nouveau. La touche Windows n'arrivait jamais sur la session : le moteur la retenait derrière une porte qui demandait à sa fenêtre d'être au premier plan, ce qu'une fenêtre portée dans la nôtre ne peut jamais être. Elle part maintenant, avec Impr. écran et Alt+F4, sous un interrupteur **Clavier : Partagé ou Immersif** |
| **R43** | Nouveau. Le bouton flottant décrochait du curseur pendant un déplacement de haut en bas, et une croix restait par-dessus l'image. Une seule cause : le sens d'ouverture du menu était lu au coeur alors que seule la page sait dans quel sens elle vient de dessiner |
| **R44** | Nouveau. Une entrée **Verrouiller** dans le menu de la session, qui verrouille l'ordinateur distant. C'est la réponse à Windows+L, qui ne peut pas voyager : la demande prend le canal du produit comme Ctrl+Alt+Suppr, et le service d'en face lève l'écran |
| **R45** | Nouveau. Le pointeur reste dans l'image quand elle occupe tout l'écran. Le moteur savait le faire et ne le faisait jamais : il posait la question à sa propre fenêtre, qui n'est jamais un écran entier chez nous. Sur une machine à deux écrans le pointeur s'en allait |
| **R43** | Complété. Le bouton flottant additionnait l'écart depuis le début du geste : une main qui sortait de l'image laissait chaque pixel refusé dans la somme, et revenir ne bougeait rien tant qu'ils n'étaient pas tous rendus |
| **R46** | Nouveau. Une ligne **Écran d'en face : Fluide ou Économe** dans le menu de la session. C'est le réglage de cadence de la machine regardée, demandé depuis celle qui regarde, et il part avec **Appliquer les changements** |
| **R46bis** | **Refait, et c'est un défaut du moteur hôte.** « Fluide » ne faisait rien : la cadence plancher était passée à l'attente d'une image, donc ajoutée à l'encodage au lieu de le couvrir, et la période devenait attente plus encodage. Le calcul se vérifie sur trois relevés du client. À revérifier après recompilation du **moteur hôte** |
| **R47** | Nouveau. La session demande la cadence de l'écran sur lequel elle va s'afficher, mesurée comme l'est déjà sa taille. Elle demandait soixante images par seconde à tout le monde, ce qui est juste sur un écran à soixante et faux sur tous les autres |
| **R47bis** | Nouveau, et c'est un second défaut du **moteur hôte**. Il partait plus d'images que la session n'en demandait : la répétition d'un écran immobile avançait sur une grille de même pas que la capture, et les deux finissaient par se toucher. À vérifier après recompilation du moteur hôte |
| **R46ter** | **Refait, et c'est un défaut que j'avais introduit moi-même en corrigeant R46bis.** « Fluide » n'atteignait jamais la cadence demandée et variait exactement comme « Économe » : relevé à 50 images/s pour 60 demandées, avec 2,35 ms d'encodage et 1 ms de réseau, donc rien n'était à l'étroit. Une image capturée achetait **deux** périodes comptées depuis son arrivée, si bien qu'un écran changeant entre la moitié de la cadence visée et la cadence visée ne déclenchait jamais la moindre répétition : le moteur servait ce que le bureau produisait, c'est-à-dire ce que donne le fait de ne rien demander. Les deux réglages étaient donc identiques sur tout bureau qui change plus de trente fois par seconde. La grille des créneaux est maintenant fixe et une image presque à l'heure prend son propre créneau. **À vérifier après recompilation du moteur hôte** |
| **R66** | **Nouveau, et c'est la réponse à ce que D97 laissait ouvert.** Le bord du bouton flottant est lisse **et d'épaisseur égale tout autour**, y compris sur un fond blanc. Deux corrections pour un seul défaut : la transparence par pixel était demandée à moitié, il manquait de déclarer la fenêtre *layered* avec une opacité constante de 255 ; et la découpe arrondissait vers l'intérieur d'une fraction différente sur chaque bord, ce qui rognait le contour d'un pixel d'un côté et de deux de l'autre. Elle arrondit maintenant vers le dehors, également sur les quatre bords. La couleur de fond part avec la transparence, les deux s'excluant, et un pixel pas encore peint ne montre plus rien au lieu de montrer du blanc |
| **R65** | **Nouveau.** Une ligne **Écran de l'hôte** dans le menu du bouton flottant, qui liste les écrans allumés de la machine d'en face et permet d'en changer. Elle n'apparaît que quand cette machine en a plusieurs. Les écrans éteints n'y sont pas, ni l'écran virtuel du produit. Le moteur d'en face ne lit quel écran filmer qu'à son démarrage, donc changer d'écran le redémarre et la session se rouvre toute seule : l'écran d'ouverture le dit. Une session qui ne choisit rien est servie sur l'**écran principal** d'en face, y compris après une session qui en avait choisi un autre |
| **R64** | **Nouveau.** Pendant une session, la vignette de ZyrDesk dans Win+Tab et Alt+Tab est de la taille des autres. Elle était nettement plus petite, quelle que soit la taille de la fenêtre, plein écran compris : ZyrDesk disait à Windows de ne pas photographier sa fenêtre et lui fournissait l'image lui-même, ce qui plafonne la vignette à la taille que Windows réclame. C'était nécessaire quand l'image de la session était une fenêtre posée par-dessus la nôtre, ça ne l'est plus depuis qu'elle est portée par notre fenêtre |
| **R62** | **Nouveau.** Un bouton **Journal** sur chaque carte de « Mes ordinateurs », qui ouvre la même fenêtre que le journal local mais rempli de ce que la machine d'en face a écrit chez elle. L'aller-retour physique jusqu'à l'autre PC pour copier son journal était le dernier que le produit imposait. La page est rassemblée par le **service** de la machine lue, donc c'est mot pour mot celle qu'on lirait devant elle. **Vider** marche aussi à distance, parce qu'une panne se cherche en vidant les deux journaux, en refaisant ce qui ne marche pas, puis en lisant les deux. Seul **Ouvrir le dossier** disparaît : ces fichiers ne sont pas ici |
| **R63** | **Nouveau.** Un codec que la machine d'en face ne sait pas encoder est **barré** dans le menu de la session, avec le mot qui le dit au survol. Elle est la seule à le savoir, ça dépend de sa carte graphique : on le lui demande, et elle répond en lisant ce que son propre moteur a écrit en démarrant. Choisir AV1 vers une machine qui n'en fait pas ne cassait rien, les deux moteurs s'entendaient sur autre chose en silence, mais le menu continuait d'afficher AV1 pour toute la session. « Automatique » n'est jamais barré, c'est le choix de ne pas choisir, et c'est le réglage par défaut |
| **R61** | **Instrumentation, pas encore une correction.** L'ouverture d'une session paraît nettement plus lente qu'avant, sans que rien ne dise où passe le temps : les horodatages du journal sont à la seconde et chaque morceau de l'ouverture est plus court que ça. Une ligne unique la découpe maintenant en millisecondes, dans `interface.log` du client : joindre l'ordinateur distant, lui demander ce qu'il faut, lancer le lecteur, attendre sa première image. C'est cette ligne qu'il faut relever avant de toucher à quoi que ce soit |
| **R60** | **Nouveau, et c'est la réponse à R59septies.** Sur une machine dont la carte graphique ne dessine rien de plus grand que sa dalle, une session en résolution du client est maintenant servie sur l'écran que la machine fait pousser : il est réveillé à la taille demandée et le bureau déménage dessus le temps de la session. Les écrans physiques sont **éteints** le temps de la session, et c'est voulu : laissés allumés ils forment la seconde moitié d'un bureau que personne ne voit, les fenêtres y disparaissent et le pointeur sort de l'image. Vu de loin, une machine à un écran doit ressembler à une machine à un écran. Tout revient à la déconnexion, le bureau d'abord et l'écran virtuel ensuite. La bascule s'apprend en essayant : la **première** session qui découvre le mur est servie comme avant, et c'est la **suivante** qui en profite, le moteur ne lisant quel écran filmer qu'à son démarrage |
| **R59septies** | **Nouveau, sur un troisième ordinateur.** Depuis le portable 1920x1200 vers un PC 1920x1080, en résolution du client, il ne se passait rien : le journal disait dans la même seconde que l'écran dessinait un bureau 1920x1200 **et** qu'il ne savait pas le dessiner. Windows avait répondu « c'est fait » sans rien faire. Deux causes : l'interrupteur qui autorise un bureau plus grand que la dalle était posé **après** la lecture du bureau, alors que ce qu'un chemin d'affichage dit de lui-même est calculé au moment où on le lit ; et on autorisait Windows à ajuster la demande, ce qui lui permet de répondre oui sans rien ajuster. L'interrupteur est posé en premier, la demande est faite exactement, et le résultat est **relu** au lieu d'être cru |
| **R59sexies** | **Nouveau, et c'est la moitié de R59quinquies qui manquait.** Le bureau de l'hôte revenait bien à sa taille, mais le client recevait **l'autre écran** : le moteur n'était jamais dit lequel filmer, donc il prenait celui que la carte graphique énumère en premier, et il reprenait le premier écran qui répond chaque fois qu'il doit recommencer à filmer. Or un écran dont on change la définition disparaît de cette énumération pendant tout le changement : celui que la session venait de régler était exactement celui que le moteur laissait tomber. L'écran filmé est maintenant nommé sur toute machine, et le moteur redemande l'écran nommé pendant trois secondes avant de se rabattre. **À vérifier après recompilation du moteur hôte** |
| **R59quinquies** | **Refait, et le rendu du bureau ne dépend plus d'une course.** Il n'était fait que si aucune autre session n'était ouverte, or la session qui se ferme et celle qui s'ouvre au basculement sont comptées ensemble pendant un instant : les mêmes trois clics marchaient un soir et ne faisaient rien le lendemain. La condition est retirée. Et sur une machine qui empruntait l'écran qu'elle fait pousser, cet écran est maintenant endormi avec le bureau qu'on rend, sinon le moteur, qui vise cet écran-là, filmerait un fond d'écran vide |
| **R59quinquies** | **Nouveau, et c'est un vieux défaut revenu par une autre porte.** Passer une session en cours de **Résolution du client** à **Résolution de l'hôte** laissait l'hôte à la taille du client pour le reste de la soirée. Basculer ferme une voie et en ouvre une autre dans la même seconde, et la remise en place du bureau, elle, tourne sur le fil qui tient le moteur et n'a pas encore eu son tour. Une session qui demande l'écran de l'hôte rend maintenant son bureau elle-même avant de répondre |
| **R59quater** | **Nouveau, et c'est la moitié qui manquait à R59ter.** La taille revenait, l'agrandissement non : un écran passé à 175 % pendant que son bureau était en 4K se retrouve, une fois ce bureau rentré, sur un cran que Windows ne sait plus nommer, et il ne répond alors plus rien du tout. Ce qui est remis est maintenant l'agrandissement relevé avant la session, pour **tout écran allumé**, et un cran devenu illisible est réécrit sans être lu. Ce que chaque écran dessine est en plus retenu d'une session à l'autre dans `data/screen/screen-scales.txt`, pour qu'un écran devenu muet reprenne l'agrandissement de son propriétaire et jamais celui que Windows recommande |
| **R59ter** | **Refait deux fois, et c'est la seconde qui compte.** Un portable 1920x1200 à qui on demande du 3840x2160 refusait, et le relevé a montré que sa carte graphique n'offre réellement rien au-delà de sa dalle : la liste s'arrête à 1920x1200. Le produit de référence y arrive quand même, parce qu'il ne demande pas la même chose. L'ancienne interface d'affichage de Windows ne connaît qu'une taille par écran, qui est le signal envoyé à la dalle ; la moderne sépare la taille du **bureau** de celle de la **dalle**, et la carte graphique réduit la première dans la seconde. Un portable dessine alors un vrai bureau 4K, et son propriétaire le voit en petit, avec des bandes là où les formats diffèrent. C'est la seconde tentative maintenant, quand la dalle n'offre pas la taille. **La moitié taille est acquise** sur le vrai portable, journal à l'appui : `draws a 3840x2160 desktop, shrunk into its own panel`, puis `this computer is showing 3840x2160`. Ce qui reste à vérifier est le retour et les deux cas qui finissent mal |
| **R59bis** | **Refait, et c'est l'autre sens qui n'allait pas.** Depuis un écran 4K à 175 % vers un portable 1920x1200, l'hôte refusait la taille demandée, gardait la sienne, et prenait quand même l'agrandissement du client : le portable restait en 1920x1200 mais à 175 %, et ne revenait jamais à 125 % à la déconnexion, ce qu'il fallait réparer à la main. Deux causes. L'agrandissement était relevé et jamais remis, donc une session qui n'avait changé que lui ne remettait rien du tout. Et il était posé même quand la taille avait été refusée, alors qu'il n'appartient plus à rien dans ce cas |
| **R59** | **Refait, et c'est le sujet du lot : ZyrDesk n'éteint plus aucun écran.** Une session posait la taille demandée sur l'écran virtuel de l'hôte et **éteignait tous les autres** le temps de la session, une télé éteinte depuis des semaines revenant en prime à chaque démarrage du service. Une session règle maintenant la taille de l'écran principal de l'hôte et ne touche à rien d'autre. Tout le bureau est relevé avant, et remis après : écrans éteints, places, tailles, cadences, agrandissements, orientations et écran principal. Le moteur ne s'occupe plus des écrans du tout |
| **R58** | **Nouveau.** La session portait la taille de l'écran du client mais pas son agrandissement : depuis un portable à 125 %, le bureau distant arrivait à la bonne résolution avec tout écrit deux fois plus petit qu'à la maison. L'agrandissement voyage maintenant avec la taille, et ZyrDesk le pose lui-même sur l'écran servi, le moteur hôte n'ayant aucun réglage pour ça |
| **R57** | **Nouveau, et c'est le pire de la série parce qu'il se répétait à l'infini.** Lancer ZyrDesk rallumait des écrans que leur propriétaire avait éteints, à chaque démarrage. Le moteur gardait un arrangement d'écrans à remettre qui nommait notre écran virtuel, lequel dort entre les sessions : l'essai échouait donc toujours, et ce qu'il fait quand il échoue est rallumer tout ce qu'il trouve. Un tel arrangement est maintenant jeté au démarrage du service |
| **R56** | **Nouveau, et c'est la correction que R55 a permis de trouver.** Le gel au verrouillage venait du moteur hôte, qui redemandait l'écran deux fois autour de deux cents millisecondes de sommeil : quatre cents millisecondes dormies par réinitialisation, trois réinitialisations par verrouillage. Il redemande maintenant toutes les vingt-cinq millisecondes. À vérifier après recompilation du **moteur hôte** |
| **R55** | **Instrumentation, pas encore une correction.** L'image se fige une à deux secondes quand on verrouille l'ordinateur distant depuis le menu flottant. Le chemin du verrouillage est maintenant chronométré de bout en bout, des deux côtés, et le programme qui verrouille attend que le bureau change réellement de mains au lieu de répondre sur la simple prise en compte de l'ordre |
| **R54** | **Nouveau.** La longueur de la route est écrite dans le journal à l'ouverture de chaque session, puis à chaque fois qu'elle double ou qu'elle est divisée par deux. Une session dont le trajet change en cours de route, parce qu'un VPN prend la route par défaut d'une des deux machines, restait totalement invisible en dehors de la fenêtre de statistiques |
| **R53** | **Nouveau, et c'est la vraie cause du gel.** La session se figeait totalement deux secondes après son ouverture, connexion toujours vivante. Le transport retombe au plus petit paquet garanti dès qu'il juge que le chemin ne porte plus les gros, ce qui arrive sur un tunnel porté dans un autre ; le moteur, lui, garde pour toute la session la taille qu'on lui a dite au départ. Chaque paquet vidéo devenait alors trop gros et était jeté en silence. La taille se calcule sur le plancher garanti maintenant, et ce qui est jeté est écrit dans le journal |
| **R52bis** | **Refait, et la correction d'avant ne servait à rien.** La course entre les adresses était juste, mais il n'y en avait qu'une à essayer : un ordinateur qui se présentait ne disait pas où il répond, donc l'autre ne connaissait que l'adresse d'où la réponse était arrivée. Une machine à quatre cartes n'en montrait qu'une. Elle les nomme toutes maintenant, et la course a enfin de quoi courir |
| **R52** | **Nouveau, et c'est un vrai défaut de fond.** L'adresse d'un ordinateur à plusieurs cartes était tirée au sort : la dernière réponse arrivée gagnait. Avec un VPN actif, le tirage tombait sur le chemin qui traverse le tunnel, d'où soixante-trois millisecondes de latence sur un bureau. Toutes les adresses sont gardées maintenant, et la voie s'ouvre vers toutes à la fois : la première qui répond gagne |
| **R49quinquies** | **Refait, et la moitié annoncée la fois d'avant n'avait jamais été écrite.** Les cinq secondes perdues à chaque ouverture étaient toujours là. Les écrans se comptent maintenant au périphérique, qui appartient à la machine et répond donc depuis un service, au lieu de passer par un bureau que le service n'a pas |
| **R49quater** | **Refait.** L'attente à l'ouverture ne voyait rien : compter les écrans depuis un service revient à interroger un poste de travail sans bureau, donc cinq secondes perdues à chaque session. Et l'écran était rangé pendant que le moteur remettait les autres en place, ce qui lui faisait rallumer des écrans que le propriétaire avait éteints |
| **R49ter** | **Refait, et c'est la racine des deux d'avant.** L'état de l'écran virtuel était lu dans des drapeaux qui disent ce qui a été demandé, pas ce qui s'est passé : un écran que Windows avait refusé d'éteindre se lisait comme éteint, donc plus rien ne l'éteignait, et le moteur le capturait en 1280x720 en permanence. La question se pose au périphérique maintenant |
| **R49bis** | **Refait, et c'est ce qui faisait tout tomber.** L'écran virtuel se réveillait à la plus petite taille de sa liste, pas à celle demandée, donc le moteur devait réarranger le bureau une seconde fois pendant qu'il l'arrangeait déjà. Et un refus de Windows d'endormir l'écran était compté comme un succès, donc l'écran restait allumé pour toujours |
| **R51** | **Nouveau.** « Taille » devient **Résolution** et s'ouvre en sous-menu : la résolution du client, celle de l'hôte, puis quinze tailles avec leur rapport. Celle de l'hôte est nouvelle et ne réarrange rien chez lui |
| **R49** | **Nouveau, et c'est le sujet du lot.** L'écran virtuel était actif en permanence : deux écrans en permanence dans les paramètres d'affichage, sur une machine que personne ne regarde. Il dort maintenant et ne se réveille que pour une session, à la demande de celui qui regarde |
| **R50** | **Refait, et c'est un défaut du moteur hôte.** Plus moyen de s'appairer : `400 Invalid uniqueid` à chaque tentative, de plus en plus tôt. Un appairage abandonné restait dans la table du moteur et cassait tous les suivants entre les deux mêmes ordinateurs, définitivement, jusqu'au redémarrage de ce moteur. À vérifier après recompilation du **moteur hôte** |
| **R48** | Nouveau. Le gel de l'image sur Ctrl+Alt+Suppr. Changer de bureau retire la duplication d'écran au moteur, et il attendait deux cents millisecondes en aveugle avant de redemander, puis vingt de plus avant de réencoder. Le journal du moteur hôte dit maintenant le chiffre. À vérifier après recompilation du **moteur hôte** |
| **R27** | **Refait, et c'est le sujet du lot.** L'écran virtuel ne s'installait sur aucune machine à qui le pilote n'avait pas été donné à la main : la lecture de sa signature posait au fichier une question trop large, à laquelle Windows répond par la liste d'empreintes du catalogue et non par les certificats. À refaire sur une machine qui n'a jamais eu d'écran virtuel |
| **R39** | Nouveau. Le thème ne suivait pas Windows quand on basculait clair/sombre, fenêtre ouverte. La vue web se voit imposer une réponse figée à la construction de la fenêtre, et le seul mécanisme qui la rafraîchissait était éteint par notre propre façon d'accorder la barre de titre. C'est le coeur qui écoute Windows maintenant |
| **R38** | **Refait, et il change de côté.** Le réglage était sur la machine regardée, ce qui obligeait à aller physiquement dessus pour couper le son de sa pièce. Il est maintenant dans les réglages de la session, sur l'ordinateur qui regarde, et la demande part avec la session. Rien à régler en face |
| **R33** | Touché. La dépendance à Steam était active par omission : sans ligne de son écrite, le moteur cherchait sa carte son, l'installait s'il en trouvait les fichiers, et y faisait passer le son de la machine à chaque session. Deux lignes ferment ça |
| **S28** | Nouveau. Fermer une session dans les six secondes qui suivent son ouverture, ou une relance par **Appliquer les changements**, repartait en appairage : « l'ordinateur distant ne reconnaît plus celui-ci », puis un refus. La surveillance qui guette un ordinateur nous ayant oubliés demande maintenant à la fenêtre si la session est toujours voulue |
| **S29** | Nouveau. Une voie restait ouverte pour toujours dès qu'une ouverture échouait, et la fenêtre affichait « Sessions ouvertes : 1 » sans session. La voie est maintenant rendue sur toutes les routes de sortie |
| **S25** | Nouveau. Un ordinateur sans écran virtuel voit toujours sa définition suivre la session, c'est voulu et ça reste. Ce qui change est le retour : si le moteur n'arrive pas à remettre l'écran, le service le lit dans le journal du moteur et le redémarre, ce qui lui redonne trois occasions de le faire. À provoquer en prenant la main avec un autre bureau à distance juste après avoir quitté la session, et à lire dans `service.log` de l'hôte |

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
> | Souris | Un interrupteur **Bureau / Jeu** : le côté en place est allumé, cliquer l'autre bascule le pointeur |
> | Masquer ce bouton | Le logo disparaît, et l'entrée dit par quelle combinaison le rappeler |
> | Terminer la session | L'image se ferme, la fenêtre ZyrDesk revient sur l'accueil, et le PC hôte rend son bureau |
>
> **Cinq entrées, pas six.** Il n'y a plus qu'une façon de finir : les moteurs en offraient deux, dont une qui laissait le bureau distant ouvert et en attente. Une session est en cours ou terminée.
>
> Si une entrée ne fait rien : ouvrir le journal (partie 7) et regarder les lignes de « La fenêtre ». Elles disent ce que le bouton a demandé, et à quelle fenêtre. Une entrée ne peut agir que si l'image est au premier plan ; la remettre devant est fait avant chaque envoi, et attendu, parce que Windows ne change pas de fenêtre de tête sur-le-champ.

> **R12octies (le bouton reste là quand on regarde ailleurs)**
>
> Pendant une session, **mettre la fenêtre de ZyrDesk sur un deuxième écran**, puis cliquer dans une autre application sur le premier écran, sans rien réduire.
>
> Attendu : le bouton flottant **reste là**, sur la session, du début à la fin. On doit pouvoir le regarder tout en travaillant sur l'autre écran.
>
> Il disparaissait entièrement dès que le premier plan partait ailleurs, et revenait à l'instant où l'on redonnait le premier plan à ZyrDesk. Il était dessiné au-dessus de toutes les fenêtres de la machine, donc il fallait bien le cacher pour qu'il n'aille pas flotter par-dessus le travail des autres ; cette hauteur datait du temps où l'image était une fenêtre à part.
>
> **Le contrôle qui va avec, sur un seul écran.** Toujours pendant une session, passer sur une autre application qui **recouvre** la fenêtre de ZyrDesk. Attendu : le bouton disparaît sous cette application, comme le reste de la fenêtre. Il ne doit jamais rester visible par-dessus.

> **R12nonies (aucun liseré blanc autour du bouton)**
>
> Pendant une session, **prendre le bouton et le déplacer lentement** sur toute la largeur de l'image, puis le reposer. Refaire menu ouvert et menu fermé.
>
> Attendu : rien de clair autour du logo ni du menu, à aucun moment du geste. Le défaut était un fin liseré blanc sur la gauche, présent par moments seulement et bien plus visible pendant le déplacement.
>
> C'est le genre de chose qu'on rate en regardant : le mieux est de **prendre une capture pendant le geste** et de la regarder ensuite.

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

> **R12septies (rien ne traîne derrière le bouton quand le menu se referme)**
>
> Pendant une session, ouvrir le menu du bouton, cliquer **Statistiques**, puis **laisser le curseur là où il est**, loin du logo. Refaire avec **Mode de la souris**, et avec **Fenêtré ou plein écran**.
>
> Attendu : le logo reste le logo. Aucune trace blanche, aucun morceau de fond, ni derrière lui ni autour, à aucun moment.
>
> Laisser le curseur loin du logo est le cœur de l'essai et pas un détail : le survol redessine le bouton, donc une trace laissée là disparaissait à la seconde où l'on allait la regarder. Ce qu'on cherche ne se voit que si la main ne bouge plus.

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

> **S5 (en mode fenêtre, la session s'ouvre agrandie)**
>
> Refaire S3 avec le réglage sur **Fenêtre**. Avant d'ouvrir, **réduire la fenêtre de ZyrDesk à un petit rectangle** au milieu de l'écran, pour que la différence se voie.
>
> Attendu : dès la demande de session, la fenêtre s'**agrandit** (le bouton « niveau inférieur » remplace « agrandir » dans la barre de titre), l'écran de chargement s'affiche déjà à cette taille, et l'image se pose dedans. La barre des tâches reste visible et la barre de titre aussi : agrandie n'est pas le plein écran.
>
> Elle **ne prend jamais l'écran entier** : pas de plein écran, ni à l'ouverture ni à l'arrivée de l'image. C'est ce que S3 vérifie dans l'autre sens.
>
> **Et à la fin de la session, la fenêtre reste agrandie.** Elle n'est pas remise à la taille qu'elle avait avant : une fenêtre qu'on rapetisse toute seule après une heure de session est une fenêtre qui fait un geste que personne ne lui a demandé.
>
> **Le plein écran ne doit rien voir de tout ça.** Refaire l'essai avec le réglage sur **Plein écran** : le comportement doit être exactement celui d'avant, sans passage visible par une fenêtre agrandie.

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
> C'est l'écran principal de l'hôte qui prend la taille demandée, et lui seul : les autres écrans restent allumés, à leur place et à leur taille, et rien n'est créé. Tout son bureau est relevé avant d'être touché et remis à la fin, écrans éteints compris (R59). La version d'avant, où le bureau déménageait sur un écran fabriqué pendant que les autres s'éteignaient, a été retirée : elle rendait l'hôte inutilisable pour la personne assise devant.
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
> **Qui prend ces touches, et pourquoi ce n'est pas ZyrDesk.** C'est le moteur client, dans le processus qui reçoit vraiment le clavier. ZyrDesk n'en prend aucune. Toute l'affaire, qui a coûté une quinzaine d'allers-retours, est racontée dans [../CLAVIER.md](../CLAVIER.md) : le symptôme, la règle de Windows qui commande tout, les trois pièges, et les quatre pistes déjà essayées qu'il ne faut pas reprendre. À lire avant de toucher à quoi que ce soit ici.
>
> **La touche Windows, elle, reste celle de ce PC-là** et ouvre le menu Démarrer d'ici. C'est la seule que ce chemin ne peut pas servir : le moteur ne la transmet au loin que quand sa propre capture des touches système tourne, et elle ne tourne jamais dans ce produit, puisque c'est elle qui avalerait Alt et Control en entier et couperait tous nos raccourcis. La reprendre n'ouvrirait donc de menu nulle part, ce qui serait pire.
>
> **Comment revenir sur ce PC-là.** Ces touches partent au loin dès que l'image tient le clavier : pour joindre une autre fenêtre d'ici pendant ce temps, c'est la souris, un clic sur sa vignette dans la barre des tâches par exemple, comme le fait S9bis plus bas. Les raccourcis de ZyrDesk, eux, marchent toujours : voir S20 juste en dessous.
>
> **Ce qu'il faut lire, et où.** Dans `session.log`, la trace du moteur client, sous `zyr:` :
>
> - `the session has the keyboard` et `the session has lost the keyboard` : le clavier doit revenir après chaque perte. Une perte sans retour, et la session reste sourde jusqu'au bout.
> - `system keys: Tab … carried to the host …` : les appuis et relâchements vus, ce qui est parti vers l'hôte, ce qui a été laissé passer et pourquoi, et **le nombre de fois où le crochet a été reposé**. C'est ce dernier nombre qui compte : il doit monter quand la fenêtre est agrandie, mise en plein écran, ou quand le menu du bouton s'ouvre, puisque ce sont les moments où un autre programme se met devant nous dans la file.
>
> Et dans `interface.log`, `le premier plan passe ailleurs : processus N (nom.exe)` nomme qui a pris le premier plan. Un tiers qui le prend pendant qu'on tape est une explication ordinaire, jamais une panne.
>
> **Ce qui ne doit surtout plus revenir.** Une version, du temps où ZyrDesk prenait ces touches lui-même, reposait son crochet en démontant son fil depuis le fil qui dessine, ce qui bloquait le clavier de tout l'ordinateur le temps de le faire : « ça m'a carrément bloqué le alt tab sur mon propre pc ». Tout ce chemin est retiré ([D47](../DECISIONS.md)). Si un essai ramène un clavier figé, même une fraction de seconde, c'est à dire immédiatement.

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

> **S23 (éteindre l'hôte depuis la session lui rend son écran)**
>
> Ouvrir une session vers le PC hôte, puis **l'éteindre depuis l'image** : menu Démarrer du bureau distant, **Arrêter**. Attendre qu'il soit vraiment éteint, puis le rallumer et regarder son écran physique.
>
> Attendu : il rallume à **sa** définition et à **son** agrandissement, ceux notés en S2, et pas à ceux du PC client.
>
> **Ce qui se joue.** Le moteur hôte remet l'écran comme il l'a trouvé **en s'en allant**, et seulement là. Le service ne le prend donc plus de force : il lui demande d'abord de partir, lui laisse vingt secondes, et ne le prend que s'il ne part pas. Il prévient aussi Windows que son arrêt demande du temps, sinon Windows ne lui en laisse que quelques secondes en s'éteignant.
>
> **Le journal du service dit lequel des deux s'est produit**, dans `service.log` du PC hôte, juste après `stop asked for` : `the engine went by itself, having put the screen back`, ou `the engine would not go and was taken, so the screen stays as the session left it`. La deuxième ligne avec un écran revenu de travers, c'est la même panne ; la première avec un écran de travers, c'est autre chose et il faut le dire.
>
> À refaire une seconde fois en arrêtant simplement le service (`zyrdeskd stop` sur l'hôte, ou **Arrêter le service** depuis la fenêtre) pendant une session : même attendu, même paire de lignes.
>
> **Ce qui a lâché malgré tout, et qui est corrigé.** Le premier essai passait quand on arrêtait le service à la main, et échouait quand on éteignait vraiment la machine. Windows emporte alors le moteur avec la session où il vit, avant que le service ait été prévenu de quoi que ce soit : le moteur n'a pas le temps de remettre l'écran. Ce n'est pas grave en soi, parce que ce qu'étaient les écrans avant la session reste écrit dans un fichier que le moteur du prochain démarrage relit. Ce qui était grave, c'est que le service prenait ça pour une chute et redémarrait un moteur sur-le-champ, deux fois, pendant les cinq secondes que la machine mettait à s'éteindre. Chacun de ces moteurs dépensait ce fichier sur un ordinateur qui n'allait plus avoir d'écrans du tout, et le lendemain il ne restait rien à remettre.
>
> **Et pourquoi le moteur n'avait pas remis l'écran lui-même.** Parce qu'il attendait trois secondes avant d'essayer, son réglage d'origine, que nous ne remplacions pas. Entre la fin de la session et Windows emportant le moteur, il s'est écoulé **une** seconde. C'est corrigé : il remet l'écran sans attendre.
>
> **Ce qu'il faut lire dans `service.log` de l'hôte, à l'extinction.** Une seule ligne, et elle est en toutes lettres : `engine stopped after N s, taken away with the session it lived in, which is somebody signing out, somebody switching user, or this computer going down; another starts in 10 s`. Les dix secondes sont là pour que la machine s'en aille avant. **Il ne doit y avoir aucun `engine started in session …` après cette ligne.** S'il y en a un, la machine a mis plus de dix secondes à s'éteindre : le suivant attendra vingt secondes, puis quarante, et c'est voulu.
>
> **L'attendu au rallumage, et c'est le point du lot.** L'écran doit être à sa taille **dès l'écran de connexion**, avant que ZyrDesk ait démarré quoi que ce soit. Pas un écran de connexion en 1920x1200 qui bascule en 4K trente secondes plus tard : ça, c'est le filet de secours, et il ne doit pas avoir à servir.
>
> Ce qui rend ça possible tient dans un réglage que nous ne posions pas : le moteur attendait **trois secondes** avant de remettre l'écran, ce qui est son défaut à lui. La session finit une seconde avant que Windows emporte le moteur, donc la remise n'avait jamais lieu. Elle se fait maintenant sur-le-champ, avant que la machine ait fini de partir, et Windows garde cette taille tout seul.
>
> **Et si le filet a quand même dû servir**, `service.log` le montre : quelques secondes après le démarrage, `screens the engine sees: … (…, on at 3840x2160)`. C'est la taille réelle de chaque écran au moment où le moteur démarre. Si elle est bonne alors que l'écran de connexion était de travers, c'est le filet qui a rattrapé le coup ; si elle est encore celle de la session, rien n'a marché, et cette fois on le sait sans avoir à se planter devant la machine.

> **S24 (le bouton flottant est là à chaque session, même longtemps après)**
>
> Ouvrir une session, la terminer, **laisser ZyrDesk ouvert** et l'ordinateur tranquille un long moment, veille comprise si elle arrive. Rouvrir une session vers le même hôte.
>
> Attendu : le bouton flottant est là, dans son coin, comme à la première session, et le raccourci du menu l'ouvre.
>
> **Si jamais il manque**, le journal de la fenêtre le dit maintenant, ce qui n'était pas le cas : soit `bouton flottant : rien de dessiné après 3 s, la fenêtre est refermée et remontée`, et le bouton doit revenir dans la foulée, soit `le bouton flottant n'a pas pu s'ouvrir : …`, qui nomme le refus. Un silence complet du journal sur ce point n'est plus une réponse possible.

> **S26 (l'écran de chargement couvre toute l'ouverture, à chaque fois)**
>
> Ouvrir et fermer une session **cinq ou six fois de suite** vers le même ordinateur, sans rien changer entre deux.
>
> Attendu, à chaque ouverture sans exception : l'écran de chargement (le logo, « Établissement de la connexion », le nom de l'ordinateur, la barre bleue) reste à l'écran **jusqu'à ce que l'image apparaisse**. On ne doit jamais revoir l'accueil entre les deux, et surtout pas la carte verte d'une session en cours pendant que l'image n'est pas encore là.
>
> **Pourquoi il faut le faire plusieurs fois.** Ça ne se produisait pas à tous les coups. Le chemin ordinaire passe par une attente de six secondes qui couvrait l'écart par hasard ; l'ouverture où les deux ordinateurs se présentent à nouveau saute cette attente, et l'écart se voyait alors tout nu. Le journal de la fenêtre dit laquelle des deux on vient de faire : `l'ordinateur distant ne reconnaît plus celui-ci, nouvelle présentation`, puis `les deux ordinateurs se connaissent`. **C'est cette ouverture-là qu'il faut avoir vue au moins une fois.** Pour la provoquer à coup sûr, ouvrir une session, puis sur l'hôte arrêter et redémarrer le service, puis rouvrir.
>
> **Ce que le journal doit montrer**, dans cet ordre : `image du lecteur N posée dans la fenêtre de ZyrDesk`, **puis** `session en cours, lecteur N`. Jamais l'inverse. Et si jamais `le lecteur N n'a pas ouvert d'image en 20 s` apparaît, l'écran de chargement se retire quand même : c'est voulu, une fenêtre couverte pour toujours serait pire.

> **S27 (un troisième ordinateur, joint par un tunnel privé)**
>
> À faire avec un PC qui n'est pas sur le réseau de la maison, joint par un VPN entre les deux (WireGuard, Tailscale, peu importe).
>
> Attendu : il apparaît dans la liste des deux côtés, et la session s'ouvre. **Regarder les deux écrans d'accueil, pas un seul** : c'est là que se cachait le défaut. L'ordinateur d'ici voyait celui de là-bas, et pas l'inverse.
>
> **Ce qui n'allait pas.** La découverte n'apprenait qu'à celui qui appelle : « qui est là ? » ne disait rien de qui demandait. Sur un réseau ordinaire les deux appellent, donc les deux apprennent, et ça ne se voit jamais. Dans un tunnel privé, un seul des deux bouts a un voisinage à balayer ; l'autre n'a personne à appeler, n'apprenait rien, et refusait toutes les sessions.
>
> **Le symptôme exact, si ça revient.** Dans `service.log` de l'ordinateur qui essaie : `no way to <adresse>:47000 … Détail : read error: connection lost`. Ce message veut dire que la connexion s'est faite puis a été coupée par l'autre au moment où il juge qui arrive. Ce n'est **jamais** le pare-feu ni la route : ceux-là donnent « ne répond pas sur le port 47000 ».
>
> **Le dépannage immédiat**, si jamais un cas y échappe encore : sur l'ordinateur qui refuse, ouvrir « ajouter un ordinateur », coller l'**empreinte** de celui qui essaie, laisser l'adresse vide, valider. Un bandeau vert confirme. En ligne de commande : `zyr-cli host authorize <empreinte>`.

> **S25 (quitter la session rend son écran à l'hôte, même quand ça se passe mal)**
>
> **Premier temps, le cas normal.** Ouvrir une session vers l'hôte, la terminer par la croix, et aller regarder l'écran physique de l'hôte.
>
> Attendu : il revient à sa définition et à son agrandissement de S2, tout seul, en quelques secondes. C'est le moteur hôte qui le fait et il le fait bien ; ce premier temps est là pour vérifier que rien n'a été cassé.
>
> **Deuxième temps, le cas qui a fait tout ce lot.** Refaire une session, la quitter, et **dans la foulée prendre la main sur l'hôte avec un autre bureau à distance** (Parsec fait très bien l'affaire). C'est ce qui empêche le moteur de remettre l'écran : les deux programmes se disputent les moniteurs, et l'hôte se met à claquer ses écrans en boucle.
>
> Attendu : ça s'arrête tout seul. Le service lit la plainte du moteur et le redémarre, ce qui remet l'écran en sortant et réessaie en rentrant.
>
> **Ce que dit `service.log` de l'hôte**, dans cet ordre : `the engine could not put this computer's screens back the way it found them, so it is started over…`, puis la ligne de départ du moteur (`the engine went by itself, having put the screen back` en principe), puis un nouveau `engine started in session …`.
>
> **Si le moteur redémarré n'y arrive toujours pas**, ce qui arrive quand l'autre programme tient toujours les écrans, le service ne s'acharne pas : `the engine still cannot put this computer's screens back… something else on this computer is holding them`, puis `the engine is told to stop trying…`. À ce moment-là l'écran reste comme il est, mais l'ordinateur cesse de claquer ses moniteurs. C'est le résultat attendu, pas une panne.
>
> **Troisième temps, celui qui casse tout si on l'a mal fait.** Quitter une session et **se reconnecter tout de suite**, dans la seconde, deux ou trois fois de suite. C'est ce que fait naturellement quelqu'un dont l'écran est revenu de travers.
>
> Attendu : rien de spécial. Les sessions s'ouvrent et se ferment normalement, aucune n'est coupée, et le service ne redémarre pas le moteur pendant qu'on est dedans.
>
> **Ce qu'il ne doit jamais se passer**, et c'est le vrai piège de cet essai : le moteur ne doit pas redémarrer en boucle. Une seule paire de lignes par session, jamais une suite de `engine started in session …` qui se répète toutes les quelques secondes.

---

## Partie 6 : les réglages

> **R17 (la taille, le débit et le codec se règlent au curseur)**
>
> Pendant une session, ouvrir le menu du bouton flottant. Trois réglages : **Taille**, **Débit**, **Codec**, chacun avec sa valeur écrite à droite de son nom et une barre à curseur en dessous.
>
> Attendu : pousser un curseur fait **suivre le mot au-dessus, cran par cran**, pendant qu'on tient le pouce. Le choix ne part qu'une fois lâché. Les icônes des trois réglages restent dans la même colonne que celles du reste du menu. La taille dit à quoi « Écran » revient sur ce PC-là (`Écran, 3840 x 2160`), sinon on ne saurait pas ce qu'on demande. **Rien ne bouge dans l'image en cours** : le choix est retenu, et c'est R34 qui le pose à l'écran.
>
> **Trois choses à regarder en poussant les trois curseurs de bout en bout :**
>
> 1. **Rien n'est coupé** : ni le bord droit du menu, ni le bas, et aucune bande blanche ou vide à côté de quoi que ce soit.
> 2. **Rien ne clignote** : le menu ne doit ni disparaître ni se redessiner pendant qu'on pousse.
> 3. **La fenêtre ne change pas de taille en boucle.** Les valeurs n'ont pas toutes la même longueur (`Écran, 3840 x 2160` contre `1280 x 720`), donc le menu peut s'élargir d'un cran à l'autre, et c'est normal. Ce qui ne l'est pas serait une ligne de redimensionnement par cran traversé.
>
> Le journal en garde la trace, une ligne par changement de taille : `bouton flottant : 1630x1614 demandés, 91x91 avant, 1630x1614 après ; 2 morceaux dessinés jusqu'à 1098x1272`. « après » doit valoir « demandés », sinon c'est Windows qui a refusé la taille ; et « dessinés jusqu'à » doit rester en dessous, sinon la page dessine plus grand que sa fenêtre.
>
> Une ligne de plus est normale la première fois qu'on change un réglage : c'est **Appliquer les changements** qui apparaît et allonge le menu. Une seule fois par session.
>
> **Trois choses de plus à regarder dans ce menu.** Les lignes doivent toutes partir du **même bord gauche** que les chiffres du haut et que les traits de séparation : aucune ne doit être rentrée vers la droite. Le curseur de la **taille** doit aller de la plus petite à gauche à la plus grande à droite. Et un clic **dans l'image**, menu ouvert, doit le refermer, sans avoir à recliquer le logo.
>
> **L'interrupteur de la souris, dans le même menu.** Il porte **Bureau** à gauche et **Jeu** à droite, et le côté en place est allumé. Cliquer l'autre bascule le pointeur et allume l'autre côté ; cliquer celui qui est déjà allumé ne fait rien. Fermer le menu, basculer la souris **au raccourci** (Ctrl+Alt+Maj+M), rouvrir le menu : l'interrupteur doit avoir suivi.
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

> **R35 (les quatre chiffres du menu sont tous remplis)**
>
> Pendant une session, ouvrir le menu du bouton flottant et regarder la barre du haut : **Décodage**, **Encodage**, **Réseau**, **Débit**, et en dessous le codec, la taille et la cadence.
>
> Attendu : **les quatre portent un nombre**, aucun ne porte un tiret, et la ligne du dessous se lit par exemple `HEVC · 1920x1200 · 60 images/s`. Les valeurs bougent d'une seconde à l'autre : la barre se remplit tant que le menu est ouvert, une fois par seconde.
>
> Le réseau doit valoir quelques millisecondes sur un réseau local, jamais zéro. C'est le vrai aller-retour entre les deux ordinateurs et non un aller-retour local : le tunnel transporte cette mesure de bout en bout, il ne la termine pas de ce côté.
>
> **Ce qui n'allait pas.** Le réseau et la cadence sortaient vides à chaque fois, et eux seuls, depuis le premier jour. Le moteur ne les accumule pas comme les autres sur sa fenêtre de mesure : il ne les pose qu'en fondant deux fenêtres l'une dans l'autre, ce que fait sa propre surcouche dessinée et ce que nous ne faisons pas. Ils sont maintenant demandés et calculés au moment d'écrire la ligne.
>
> **Un tiret veut toujours dire « pas de mesure », jamais zéro.** Une seconde sans image décodée n'a pas un temps de décodage nul, et écrire zéro serait mentir. Un tiret qui persiste sur les quatre à la fois, en revanche, veut dire que la ligne n'est pas écrite du tout : le fichier de mesures vit à côté du journal du lecteur, dans `logs`.

> **R36 (Ctrl+Alt+Suppr part sur l'ordinateur distant)**
>
> Pendant une session, ouvrir le menu du bouton flottant : une entrée **Ctrl+Alt+Suppr**, sous l'interrupteur du son. Cliquer dessus.
>
> Attendu : le menu se referme, et **l'écran de sécurité de Windows apparaît dans l'image**, celui qui propose de verrouiller, changer d'utilisateur, se déconnecter ou ouvrir le gestionnaire des tâches. Rien de tel ne doit apparaître sur l'ordinateur qui regarde.
>
> Appuyer sur Échap dans l'image pour en sortir, puis vérifier que la session est toujours vivante et que le clavier y répond encore.
>
> **Pourquoi c'est un essai à part.** Cette combinaison est la seule que Windows garde entièrement pour lui, aux deux bouts. Elle ne passe pas par le clavier du lecteur comme **Statistiques** ou la souris : elle voyage sur le canal que ZyrDesk se réserve dans le tunnel, et c'est le service de l'ordinateur d'en face qui la presse, dans son propre processus.
>
> **Ce que le journal du service hôte doit dire**, deux lignes, dans cet ordre :
>
> ```
> Ctrl+Alt+Suppr: policy 1, this service is in session 0, the screen is on session 1
> Ctrl+Alt+Suppr pressed for the far computer
> ```
>
> C'est la première qui compte, et elle est là justement parce que Windows ne répond rien à cet appel : une frappe qui ne fait rien et une frappe jamais autorisée se lisent sinon exactement pareil.
>
> - `policy 1` : la stratégie est en place. `policy unset` ou `policy 0` veut dire que Windows refusera, et que la stratégie n'a pas pu être écrite ; la ligne `Ctrl+Alt+Suppr cannot be pressed ... (code N)` au démarrage du service dit pourquoi. Sur une machine dont les stratégies sont tenues par un employeur, il n'y a rien à faire.
> - `session 0` : le service tourne bien comme service. Toute autre valeur veut dire que la frappe part d'un endroit d'où Windows la jette sans un mot.
> - `the screen is on session N` avec N différent de 0 : quelqu'un est bien connecté sur cette machine. `none` veut dire personne, et il n'y a alors aucun écran à réveiller.
>
> **Le service hôte doit avoir été redémarré au moins une fois** avec cette version, sinon la stratégie n'est pas posée. Elle l'est maintenant à chaque démarrage du service, donc il n'y a plus rien à réinstaller.

> **S28 (fermer tout de suite après avoir ouvert)**
>
> Ouvrir une session vers un ordinateur déjà connu, attendre que l'image apparaisse, et la fermer **dans les trois secondes** qui suivent, par le menu ou par la croix.
>
> Attendu : retour à l'accueil, sans un mot. **Aucune** ligne « l'ordinateur distant ne reconnaît plus celui-ci », **aucun** écran de chargement qui revient, **aucun** refus d'appairage.
>
> À refaire dans le cas qui l'avait révélé : pendant une session, ouvrir le menu du bouton flottant, changer la taille, cliquer **Appliquer les changements**, laisser l'image revenir, puis fermer aussitôt.
>
> **Pourquoi c'est un piège.** L'ouverture ne s'arrête pas quand l'image apparaît : elle surveille le lecteur six secondes de plus, parce qu'un ordinateur qui nous a oubliés refuse la session en moins d'une seconde et qu'il faut alors se représenter. Or fermer une session arrête le lecteur exactement de la même façon. Seule la fenêtre sait qu'on a cliqué, et c'est elle qu'on interroge maintenant.

> **S29 (aucune voie ne reste ouverte derrière une session ratée)**
>
> Après **n'importe quel** échec d'ouverture, regarder la ligne **Sessions ouvertes** de la fenêtre, et le journal du service.
>
> Attendu : **0**, et une ligne `way N closed` pour chaque `way N open` du journal. Un nombre qui ne redescend plus est une voie que personne ne fermera : le service ne referme une voie que quand le processus qu'on lui a désigné s'en va, et on ne le lui désigne qu'à la toute fin d'une ouverture réussie.
>
> Le plus simple pour le provoquer : couper l'accès distant sur l'ordinateur d'en face, tenter une session, la voir échouer, puis vérifier le compte.

> **R37 (couper le son de la session, ici et pas là-bas)**
>
> Pendant une session où l'ordinateur distant joue quelque chose de sonore, ouvrir le menu du bouton flottant : un interrupteur **Son**, avec **Actif** à gauche et **Coupé** à droite, qui doit montrer **Actif**.
>
> Lancer aussi de la musique **sur l'ordinateur qui regarde**, en local.
>
> Cliquer **Coupé**.
>
> Attendu : le son de la session se tait, **la musique locale continue**, et le menu reste ouvert avec **Coupé** allumé. Ouvrir le mélangeur de volume de Windows : la tranche de la session y est bien marquée muette, comme n'importe quel programme.
>
> Recliquer **Actif** : le son revient.
>
> Refermer le menu, le rouvrir : l'interrupteur doit toujours dire la vérité. Le couper **depuis le mélangeur de Windows**, puis rouvrir le menu : il doit dire **Coupé**, parce qu'il relit l'état au lieu de se souvenir de ce qu'il a fait.
>
> Le journal de la fenêtre dit `son du lecteur N coupé` puis `son du lecteur N rendu`.

> **R38 (couper le son de l'ordinateur d'en face, depuis celui qui regarde)**
>
> Le réglage est **sur l'ordinateur depuis lequel on regarde**, dans les réglages de la session, à côté de la taille et du codec : **Couper le son de l'ordinateur distant**. Éteint par défaut. L'allumer.
>
> Mettre de la musique sur l'ordinateur **d'en face**, à un volume audible dans sa pièce. Puis ouvrir une session vers lui.
>
> Attendu : **ses enceintes se taisent** dès que la session s'ouvre, et **le son arrive dans la session**. Fermer la session : ses enceintes se remettent à jouer toutes seules.
>
> Il n'y a **rien à régler sur la machine d'en face**, et c'est tout l'objet de l'essai : aller y pousser un interrupteur serait exactement le déplacement que la prise en main à distance existe pour éviter.
>
> **Ce que les journaux doivent dire.** Côté client : `way N asked the far computer's speakers to be silent`. Côté hôte :
>
> ```
> the far computer asked this one's speakers to be silent
> somebody is now watching this computer, and its speakers are to be silent while they do
> the speakers of this computer are silent while it is being watched
> ```
>
> Et au départ : `the speakers of this computer play again`.
>
> - `its speakers are left alone, nobody having asked for that` : la demande n'est pas arrivée. Soit le réglage n'est pas allumé du côté qui regarde, soit les deux machines n'ont pas la même version.
> - `the speakers would not move:` suivi d'une raison : Windows a refusé sur la machine d'en face, et la raison est écrite.
> - Côté client, `les enceintes de l'ordinateur distant restent allumées : …` : la machine d'en face a refusé, et la session continue quand même. C'est voulu : un ordinateur qui ne peut pas se taire a quand même une session à donner.
>
> **Le cas du service qui ne va pas au bout.** Ouvrir une session avec le son coupé, puis **éteindre brutalement l'ordinateur d'en face** (bouton d'alimentation maintenu). Le rallumer. Attendu : ses enceintes rejouent toutes seules, et son journal dit `this computer was left silent by a session that did not end properly` suivi de `the speakers of this computer play again`. Windows se souvient d'une carte coupée à travers un redémarrage : sans ce rattrapage, la machine resterait muette pour toujours.
>
> **Le cas des enceintes déjà coupées.** Couper le son de la machine d'en face à la main **avant** d'ouvrir la session, puis ouvrir et fermer. Attendu : le son y reste coupé à la fin. Ce produit ne rend que ce qu'il a pris.
>
> **Ce qu'il ne faut surtout pas voir.** Aucune carte son nouvelle dans la liste des périphériques audio de la machine d'en face, ni pendant ni après. C'est là toute la différence avec la façon dont les moteurs font ça d'habitude, qui est d'installer celle de Steam. Le journal du moteur hôte doit dire une fois par session `Couldn't find the specified virtual audio sink aucune-carte-son-virtuelle` : c'est la réponse voulue, écrite noir sur blanc.

> **R39 (le thème suit Windows, fenêtre ouverte)**
>
> Dans les réglages de ZyrDesk, choisir **Système** pour le thème. Laisser la fenêtre ouverte, bien en vue.
>
> Aller dans Windows, **Personnalisation > Couleurs**, et basculer **Choisir votre mode par défaut pour les applications** de Clair à Sombre.
>
> Attendu : **ZyrDesk bascule tout seul**, sans être touché ni relancé, en même temps que les autres applications. La barre de titre bascule avec la page, pas une seconde plus tard et pas dans l'autre sens. Rebasculer : elle revient.
>
> Le journal de la fenêtre dit `Windows demande maintenant une interface sombre`, puis `... claire`. Aucune ligne : le coeur n'écoute pas, et le journal dit pourquoi au démarrage (`le thème de Windows ne sera pas suivi : ...`).
>
> **Sur le bouton flottant aussi.** Refaire la bascule pendant une session : le menu du bouton flottant doit changer de thème comme l'accueil.
>
> **Et un choix explicite reste un choix.** Mettre **Clair**, basculer Windows en sombre : ZyrDesk reste clair, barre de titre comprise. C'est voulu, et c'est exactement ce que « Système » ne doit pas faire.
>
> **Ce qui n'allait pas.** La vue web se voit imposer une réponse fixe au moment où la fenêtre est bâtie, et le seul mécanisme qui la rafraîchissait était éteint par le fait même que nous imposions un thème à la fenêtre pour accorder sa barre de titre. C'est le coeur qui écoute Windows maintenant.

> **R40 (le menu s'ouvre vers le haut quand il n'y a plus de place en bas)**
>
> Pendant une session, prendre le bouton flottant et le poser **tout en bas** de l'image, contre le bord. Puis l'ouvrir.
>
> Attendu : le menu s'ouvre **au-dessus** du logo, entier, rien de coupé. Le logo lui-même ne bouge pas d'un pixel : il reste exactement là où la main l'a laissé.
>
> Remonter le bouton vers le haut : le menu repasse en dessous, toujours entier. Le faire menu ouvert : il doit se retourner sans laisser de morceau de l'ancien dessin derrière lui.
>
> **À vérifier de près, c'est le piège de cette reprise.** Ouvrir le menu en bas, puis changer la **taille** : la ligne **Appliquer les changements** apparaît, ce qui rend le menu plus haut. La fenêtre doit grandir **vers le haut**, et le menu rester entier et bien découpé, sans liseré ni morceau fantôme au-dessus.
>
> Le journal donne la mesure : `bouton flottant : LxH demandés, ... ; N morceaux dessinés jusqu'à LxH`. Les deux hauteurs doivent se suivre.

> **R41 (le curseur montre une main pendant qu'on déplace le bouton)**
>
> Pendant une session, prendre le bouton flottant et le déplacer sur l'image.
>
> Attendu : le curseur est une **main qui agrippe** pendant tout le geste. Jamais un rond barré, jamais un sens interdit, à aucun moment du déplacement.

> **R42 (la touche Windows et Alt+Tab changent de machine)**
>
> Pendant une session, cliquer dans l'image pour être bien dessus. L'interrupteur **Alt+Tab, Windows** du menu doit être sur **Session**, ce qui est sa valeur d'usine.
>
> **Côté Immersif.** Appuyer sur la touche **Windows** : le menu Démarrer de l'ordinateur **d'en face** s'ouvre, dans l'image. Rien ne bouge sur celui-ci. Puis, toujours d'en face : **Windows+E** ouvre l'explorateur, **Windows+R** ouvre Exécuter, **Alt+Tab** fait défiler les fenêtres, **Impr. écran** capture l'écran distant, et **Alt+F4** ferme la fenêtre distante au premier plan **sans terminer la session**.
>
> **Côté Partagé.** Basculer l'interrupteur sur **Partagé**, sans rien relancer : l'image ne doit ni clignoter ni se rouvrir, c'est tout l'intérêt. Réappuyer sur **Windows** : c'est le menu Démarrer **de cet ordinateur-ci** qui s'ouvre. **Alt+Tab** fait défiler les fenêtres d'ici. **Impr. écran** ouvre l'outil de capture d'ici.
>
> **Le retour.** Rebasculer sur **Immersif** : la touche Windows repart au loin. Refermer et rouvrir le menu entre-temps : l'interrupteur doit dire où l'on en est, pas où l'on en était.
>
> **Ce qui ne doit pas casser.** Les raccourcis du produit sont tous des combinaisons Alt et doivent marcher des deux côtés : **Alt+&** (plein écran), **Alt+é** (fin de session) et **Alt+²** (menu). S'ils s'éteignent, c'est qu'Alt est avalé, ce qui est la faute que ce mode existe pour éviter.
>
> **Ce qui reste hors de portée, et c'est normal.** **Windows+L** verrouille cet ordinateur-ci quel que soit le côté, et **Ctrl+Alt+Suppr** ouvre l'écran de sécurité d'ici. Windows traite ces deux-là dans une partie du système qu'aucun programme ne peut atteindre, et aucun produit de bureau à distance ne les a. Pour verrouiller l'ordinateur d'en face : l'entrée **Ctrl+Alt+Suppr** du menu, ou son menu Démarrer.
>
> **Et ça survit à une relance.** Laisser l'interrupteur sur **Partagé**, terminer la session, en rouvrir une : il doit encore être sur **Partagé**.
>
> Le journal du moteur client (`session.log`) dit `zyr: the system's keys now go to the session` et `... to this computer` à chaque bascule, et une ligne de comptes une fois par seconde où `passed: N switch off` est ce que l'interrupteur a laissé passer.

> **R44 (verrouiller l'ordinateur distant)**
>
> Pendant une session, ouvrir le menu du bouton flottant et cliquer **Verrouiller**.
>
> Attendu : l'ordinateur d'en face se verrouille, et **on le voit dans l'image**. Le moteur hôte tourne avec les droits du système et sait capturer l'écran de verrouillage, donc la session ne se coupe pas : elle montre l'écran de connexion.
>
> Depuis là, se déverrouiller de loin : taper le mot de passe. Si cette machine réclame Ctrl+Alt+Suppr avant, l'entrée du menu juste au-dessus le fait.
>
> **Sur l'ordinateur d'en face**, si tu peux le voir : son écran physique montre la même chose. C'est un vrai verrouillage de Windows, pas une image.
>
> Le journal du service **d'en face** dit `the far computer asked this one to lock itself`, et celui de l'ordinateur qui regarde `way N asked the far computer to lock itself`. Un refus est écrit en clair : `this computer not locked: ...`.
>
> **Le cas où il n'y a personne.** Si l'ordinateur d'en face est déjà sur son écran de connexion, personne n'est en session dessus et il n'y a rien à verrouiller : le menu répond `no session owns the screen`, ce qui est la vérité et pas une panne.

> **R48 (Ctrl+Alt+Suppr ne gèle plus l'image)**
>
> Pendant une session, envoyer **Ctrl+Alt+Suppr** depuis le menu du bouton flottant, cinq ou six fois de suite en laissant deux secondes entre chaque.
>
> Attendu : l'image se fige un instant à chaque fois, c'est inévitable, mais **c'est court**. Ce qu'il faut regarder, c'est le chiffre, pas l'impression.
>
> **Le chiffre est au journal du moteur hôte**, c'est-à-dire sur l'ordinateur d'en face, dans son fichier de journal. Une ligne par bascule :
>
> `Capture reinitialized after 24ms (2ms waiting for the encoders to let the display go, 22ms finding it again)`
>
> Avant ce lot, la deuxième moitié valait deux cents millisecondes de plus, et il fallait ajouter jusqu'à vingt millisecondes après la ligne avant que l'image ne reparte. Si tu relis des nombres autour de deux cents, la correction n'est pas en place : vérifie que c'est bien le moteur hôte recompilé qui tourne.
>
> **À faire aussi dans l'autre sens.** Se verrouiller avec l'entrée **Verrouiller**, puis Ctrl+Alt+Suppr sur l'écran de connexion : c'est la même bascule de bureau, dans l'autre sens, et elle passe par le même code.

> **R49 (l'écran virtuel ne vit que pendant une session)**
>
> **Sur l'ordinateur hôte**, clic droit sur le bureau, **Paramètres d'affichage**, hors de toute session.
>
> Attendu : **un seul écran**, le sien. Pas de deuxième, pas de « VDD by MTT », rien.
>
> **La seule exception, et elle n'arrive qu'une fois par ordinateur.** Au tout premier démarrage du service après ce lot, l'écran se montre une seconde puis repart : le moteur doit le voir une fois pour lui donner un nom, et c'est ce nom qui sert ensuite. Le journal du service le dit : `the virtual screen has never been named by an engine, waking it for this one start so it can be`. Aux démarrages suivants, plus rien.
>
> **Puis ouvrir une session** depuis l'autre ordinateur, en demandant une taille plus grande que l'écran de l'hôte (4K vers un hôte 1080p).
>
> Pendant la session, le journal du service **d'en face** dit `virtual screen woken`, et celui d'ici `way N asked the far computer to wake its virtual screen for 3840x2160`. Si tu peux regarder l'écran physique de l'hôte, il montre bien le bureau distant.
>
> **Fermer la session, puis retourner dans les paramètres d'affichage de l'hôte** : un seul écran de nouveau, dans la seconde. Le journal dit `virtual screen asleep, this machine has its own screens back`.
>
> **Le cas qui compte vraiment : la session qui finit mal.** Ouvrir une session, puis **arracher le Wi-Fi** de l'ordinateur qui regarde, ou fermer son couvercle. Personne ne dit rien à l'hôte. Attendre quelques secondes et regarder ses paramètres d'affichage : l'écran doit être reparti quand même. Le journal de l'hôte dit `nobody is watching this computer any more, its virtual screen goes back to sleep`. C'est le cas pour lequel ce filet existe.

> **R51 (la résolution se choisit dans une liste)**
>
> Pendant une session, ouvrir le menu du bouton flottant : la ligne s'appelle maintenant **Résolution** et porte un chevron. À droite, ce qui est choisi : `client, 1920x1200` par exemple.
>
> Cliquer dessus : le menu laisse la place à la liste. En haut un retour, puis **Résolution du client**, **Résolution de l'hôte**, chacune avec une phrase qui dit ce qu'elle fait, puis les quinze tailles avec leur rapport à droite (16:9, 21:9, 16:10, 4:3, 5:4). Celle qui est en place porte une coche.
>
> Choisir une taille referme la liste et revient au menu, la ligne dit la nouvelle valeur, et **Appliquer les changements** apparaît. Le retour en haut ramène au menu sans rien changer.
>
> **Résolution du client** : ce qu'on avait déjà. L'ordinateur d'en face est mis à la taille de cet écran-ci, un pixel envoyé pour un pixel affiché. Le journal de la fenêtre dit `l'écran est demandé entier`.
>
> **Résolution de l'hôte**, à vérifier avec soin, c'est le nouveau. Choisir, appliquer, et regarder **l'écran physique de l'ordinateur d'en face** : sa résolution ne doit **pas** changer, et **aucun écran virtuel ne doit apparaître**. Le journal du service d'ici dit `way N asked the far computer to keep its own screen` puis `way N: the far computer is showing 1920x1080`, et celui de la fenêtre `l'ordinateur distant affiche 1920x1080, c'est ce qui est demandé au lecteur`.
>
> **Le cas qui prouve que ça marche vraiment** : depuis un écran 4K, prendre la main sur une machine 1080p en **Résolution de l'hôte**. L'image doit arriver en 1080p et être agrandie ici, pas rognée et pas déformée. En **Résolution du client**, la même session doit passer l'écran principal de la machine d'en face en 4K, s'il en est capable, et le lui rendre en 1080p à la fin (R59).

> **R49bis (l'écran virtuel naît à la bonne taille, et il se rendort vraiment)**
>
> **Sur l'ordinateur hôte**, ouvrir les paramètres d'affichage et les laisser ouverts. Ouvrir une session vers lui depuis l'autre machine.
>
> Attendu : l'écran virtuel apparaît **directement à la taille demandée**. S'il apparaît en 1280x720 puis change, la correction n'est pas en place. Le journal du service d'en face dit `virtual screen on the desktop after ... ms`, puis `this computer will be showing 1920x1080` doit être la vérité.
>
> Fermer la session. L'écran doit repartir. **S'il ne repart pas tout de suite**, ce n'est pas grave : Windows refuse parfois pendant que le moteur remet les écrans en place. Le journal dit alors `the virtual screen would not go to sleep, trying again in a moment`, et il doit repartir dans les deux à quatre secondes. Ce qu'il ne faut plus jamais voir, c'est un écran resté allumé sans session.
>
> **Le cas qui prouve le second défaut** : ouvrir et fermer trois sessions d'affilée, vite. Puis regarder les paramètres d'affichage : un seul écran de plus que d'habitude pendant les sessions, aucun après.

> **R49ter (le journal dit la vérité sur l'écran virtuel)**
>
> C'est le contrôle qui prouve que les deux d'avant tiennent. **Sur l'ordinateur hôte**, hors session, redémarrer le service et lire son journal.
>
> Il doit dire `virtual screen already asleep` **et** la ligne `screens the engine sees:` juste après ne doit **pas** contenir `VDD by MTT`. Les deux ensemble, ou aucune confiance : si le service dit qu'il dort et que le moteur le voit, c'est ce défaut-là qui est de retour.
>
> Et `Capture size` dans le journal du moteur hôte doit être la taille de l'écran physique de cette machine, pas 1280x720.
>
> **Le seul cas où l'écran doit être vu par le moteur au démarrage**, c'est la toute première fois de la vie de l'ordinateur, celle où il est réveillé une seconde pour être nommé (R49). Le journal le dit alors en clair.

> **R49quater (l'ouverture ne traîne plus, et la disposition d'écrans est respectée)**
>
> **Le chrono.** Ouvrir une session et regarder le journal du service **d'en face** : entre `session open with` et `this computer will be showing`, il ne doit plus y avoir cinq secondes. La ligne `virtual screen on the desktop after ... ms` doit donner quelques centaines de millisecondes, et surtout **plus jamais** `has not joined the desktop after 5000 ms`.
>
> **La disposition d'écrans, et c'est le vrai sujet.** Sur l'ordinateur hôte, s'il a plusieurs écrans, en **éteindre un** dans les paramètres d'affichage de Windows avant la session. Ouvrir une session, la fermer.
>
> Attendu : cet écran reste éteint. Avant, le moteur n'arrivait plus à remettre sa disposition en place, se rabattait sur « allumer tout ce qui existe », et l'écran éteint se rallumait tout seul.
>
> Le journal du service d'en face dit `the desktop stopped changing after ... ms` avant d'endormir l'écran, et celui du moteur ne doit **pas** contenir `Failed to revert display device configuration`.

> **R49quinquies (le chrono, pour de bon cette fois)**
>
> **Exactement le même essai que le chrono de R49quater**, à refaire parce que la correction annoncée alors n'avait jamais été écrite dans le code : les cinq secondes étaient toujours là au relevé suivant.
>
> Ce qui change dessous : les écrans se comptent au périphérique, comme l'écran virtuel est déjà trouvé et réveillé, au lieu de passer par un bureau que le service n'a pas. `virtual screen on the desktop after ... ms` doit donner quelques centaines de millisecondes.

> **R52 (la session prend le chemin le plus court, VPN ou pas)**
>
> **Avec le VPN allumé**, ouvrir une session entre deux machines du même réseau local.
>
> Le journal du service dit maintenant vers combien d'adresses il court, puis laquelle a gagné :
>
> `opening a way to <empreinte>, racing 3 addresses: 192.168.2.20:47000, 192.168.1.20:47000, 10.x.x.x:47000`
> `192.168.1.20:47000 answered first, after 3 ms`
>
> **Le chiffre qui tranche.** Ouvrir les statistiques (Ctrl+Alt+Maj+S) et lire **Réseau**. Sur deux machines du même réseau, ce doit être **1 ou 2 ms**, VPN allumé ou éteint. Si tu lis soixante et quelques, la session est partie par le tunnel du VPN et la correction n'est pas en place.
>
> **À faire dans les deux sens**, et avec le VPN allumé sur une seule des deux machines puis sur les deux : ce sont trois situations différentes et la course doit gagner les trois.

> **R53 (l'image ne se fige plus, VPN dans VPN compris)**
>
> **L'essai qui échouait.** VPN allumé, ouvrir une session vers une machine joignable par un tunnel privé. Avant, l'image arrivait, tenait une à deux secondes et se figeait complètement : la fenêtre répondait encore, le clavier partait encore, et plus une seule image nouvelle. Il fallait fermer avec la croix.
>
> Attendu : l'image continue. **Tenir la session au moins cinq minutes** en bougeant des fenêtres sur la machine d'en face, ce qui est ce qui produit le plus d'images.
>
> **La ligne à surveiller dans le journal du service**, et elle ne doit plus jamais apparaître :
>
> `way 1: the path no longer carries packets the size the engine was told to send, so the picture is stopping`
>
> Si elle apparaît quand même, elle donne la place restante sur le chemin et le nombre de rétrécissements : c'est exactement ce qu'il faut m'envoyer.
>
> L'autre ligne, plus ordinaire, dit que le chemin ne prend pas les paquets aussi vite que le moteur les produit : `the path is not taking packets as fast as the engine makes them`. Celle-là est un problème de débit, pas de taille, et elle donne le temps d'aller-retour.
>
> **Et la session doit s'ouvrir deux secondes plus vite qu'avant** : l'attente qui servait à mesurer le chemin n'a plus lieu d'être.

> **R46ter (« Fluide » tient enfin la cadence demandée)**
>
> **L'essai le plus simple de la série** : ouvrir une session, mettre **Écran d'en face : Fluide**, appliquer, puis ouvrir les statistiques (Ctrl+Alt+Maj+S) et lire la cadence.
>
> Attendu : **la cadence demandée**, soixante si la session est ouverte à soixante, et stable. Ce qu'on voyait avant : cinquante, ou quarante-huit, variant sans arrêt, exactement comme en **Économe**.
>
> **Le cas qui prouve que les deux réglages font enfin deux choses différentes** : sur le même bureau, basculer entre **Économe** et **Fluide** et comparer. En Économe la cadence suit ce que le bureau produit ; en Fluide elle doit remonter à ce que la session a demandé, en complétant avec des images renvoyées.
>
> **Et le cas d'origine, à refaire aussi** : un bureau parfaitement immobile, rien qui bouge, en Fluide. La cadence doit rester à ce qui a été demandé et non tomber à la moitié.
>
> **Demande la recompilation du moteur hôte.**

> **R66 (le bord du bouton flottant)**
>
> **Le cas de Victor** : session ouverte, et une fenêtre blanche sur le bureau distant, sous le bouton flottant. Une page web blanche, le bloc-notes, n'importe quoi de bien blanc.
>
> Attendu : le contour noir du logo est **lisse**, **de la même épaisseur des quatre côtés**, et il n'y a **ni plaque ni liseré** d'aucune couleur autour. Ce qu'on voyait avant : un contour en escalier, puis, la transparence acquise, un contour plus épais à gauche qu'ailleurs.
>
> **À regarder aussi au survol et à l'ouverture du menu**, qui sont les deux moments où la découpe est refaite : rien de blanc ne doit apparaître ni au bord gauche pendant l'animation, ni sous le menu quand il se referme.
>
> **Et le cas qui dit que ça n'a pas marché** : le bouton posé sur une plaque de couleur, ou un carré blanc à la place du logo. Le journal de la fenêtre le dit alors mot pour mot : `bouton flottant : Windows a refusé la transparence par pixel`. S'il ne le dit pas et que la plaque est là quand même, c'est que le compositeur accepte les appels sans les honorer, et il faut le signaler : c'est le seul cas que ni le code ni le journal ne savent voir.
>
> **Le bord à comparer côté par côté** : le contour à gauche du logo et celui en haut doivent faire la même épaisseur. C'est là que le défaut se voyait, et c'est là qu'il faut regarder.

> **R65 (choisir lequel des écrans de l'hôte on regarde)**
>
> **Le cas à reproduire est celui de Victor** : un PC hôte avec deux écrans allumés et une télé branchée mais désactivée.
>
> Session ouverte vers cette machine, puis le menu du bouton flottant. Une ligne **Écran de l'hôte** est là, sous **Résolution**, avec le nom de l'écran regardé à droite. Elle ouvre une liste.
>
> Attendu dans la liste : **les deux écrans allumés, et eux seuls**. La télé éteinte n'y est pas, l'écran virtuel du produit non plus. Le principal est marqué « (principal) », sa taille est écrite à droite de chaque ligne, et c'est lui qui est coché tant que personne n'a choisi.
>
> Choisir le second écran, puis **Appliquer les changements**. L'écran d'ouverture revient et dit **« L'ordinateur distant change d'écran, il redémarre… »**, puis la session revient sur l'autre écran. Ça prend quelques secondes : le moteur d'en face ne lit quel écran filmer qu'à son démarrage, il n'y a pas d'autre moyen.
>
> **La ligne du service de l'hôte, à relever** : `a session asked to be served from {…}, so this computer's engine starts over`, puis, au démarrage suivant, `the engine is filming the screen a session asked to be served from ({…})`.
>
> **Le retour à l'écran principal, qui est la moitié qu'il ne faut pas rater.** Fermer la session, puis en rouvrir une sans rien choisir : elle doit être servie sur l'**écran principal**, pas sur celui de la session d'avant. Le même redémarrage a lieu, et le journal de l'hôte le dit : `a session asked this computer to be served from its main screen`.
>
> **Le cas où la ligne ne doit pas exister** : une session vers une machine à un seul écran. Pas de ligne **Écran de l'hôte** du tout, puisqu'il n'y a rien à choisir. Pareil pendant que le moteur d'en face démarre encore : une absence de réponse ne doit pas afficher une liste d'un seul élément.
>
> **Et le cas de l'écran débranché** : choisir le second écran, puis le débrancher côté hôte. À la session suivante, la machine est filmée sur son écran principal, et son journal le dit : `a session asked to be served from a screen this computer is not showing on (…), so its main screen is filmed instead`.

> **R64 (la vignette de la session dans Win+Tab)**
>
> Session ouverte, fenêtre **agrandie**. Faire Win+Tab, ou maintenir Alt+Tab.
>
> Attendu : la carte de ZyrDesk est **de la même taille** que celles des autres fenêtres agrandies, et elle montre le bureau distant. Ce qu'on voyait avant : une carte deux fois plus petite que ses voisines, avec la bonne image dedans.
>
> **À refaire en plein écran**, où c'était pareil, et **fenêtre réduite en taille**, où la carte doit simplement suivre la fenêtre comme celle de n'importe quel programme.
>
> **Et une fois la session fermée** : la carte montre la page d'accueil, à sa taille normale.
>
> **Ce que ça remplace** : ZyrDesk levait les deux attributs qui veulent dire « cette fenêtre fournit elle-même son image » pendant toute la session. Une fenêtre qui répond ça n'est plus jamais photographiée en direct, et l'image qu'elle fournit ne peut pas dépasser la taille que Windows réclame dans sa demande, bien plus petite que la carte dessinée.
>
> **Le cas où l'ancien comportement doit revenir tout seul** : une machine où Windows refuse que notre fenêtre adopte celle du moteur. Le journal du client le dit alors mot pour mot : `l'image n'a pas pu être portée par la fenêtre`. Sur celle-là, la session redevient deux fenêtres posées l'une sur l'autre, ZyrDesk refournit son image, et la vignette est de nouveau petite mais **juste** : sans ça elle montrerait la page d'accueil au lieu du bureau distant.

> **R63 (un codec que la machine d'en face ne sait pas faire)**
>
> **Le cas à reproduire est celui de Victor** : un hôte à carte Intel, qui fait du H.264 et du HEVC mais pas d'AV1. Son journal le dit, à chaque démarrage de son moteur :
>
> ```
> Found H.264 encoder: h264_qsv [quicksync]
> Found HEVC encoder: hevc_qsv [quicksync]
> ```
>
> Aucune ligne pour l'AV1, et plus haut la raison : `Could not open codec [av1_qsv]`.
>
> Ouvrir une session vers cette machine, puis le menu du bouton flottant, et regarder la ligne **Codec**.
>
> Attendu : **AV1 barré et pâle**, et le survol qui dit « Cet ordinateur ne sait pas encoder ce format ». Les trois autres restent cliquables. Barré et non retiré : une possibilité qui disparaît d'un ordinateur à l'autre laisse croire à un menu qui change d'avis, alors que c'est la machine regardée qui n'a pas la même carte.
>
> **Ce que ça remplace** : choisir AV1 ne cassait rien, les deux moteurs s'entendaient sur du HEVC en silence, mais le menu affichait AV1 pour toute la session sur un choix qui n'avait jamais été honoré.
>
> **La ligne du service de l'hôte, à relever** : `a session asked what this computer can encode: H.264 HEVC`.
>
> **Le cas où il ne faut rien griser** : le menu ouvert hors session, ou pendant que le moteur d'en face démarre encore. Rien n'est barré, parce que la question n'a pas de réponse. Une absence de réponse n'est pas « il ne sait rien faire » : un ordinateur qui n'encoderait rien ne pourrait pas être regardé du tout.
>
> **Et vers une machine NVIDIA**, qui fait les trois : rien ne doit être barré.

> **R62 (le journal de l'ordinateur d'en face, lu d'ici)**
>
> **Sans ouvrir la moindre session.** Sur l'accueil, dans « Mes ordinateurs », chaque carte porte maintenant une petite icône de journal en haut à droite. Cliquer celle d'un autre ordinateur.
>
> Attendu : la même fenêtre que le journal local, titrée **Journal de PC-SAV**, et dedans la page de cette machine-là. Elle commence par sa version, son nom et ses adresses, puis vient ce que seul son service sait dire :
>
> ```
> ZyrDesk 0.1.0 (...)
> Ordinateur       : PC-SAV
> Adresses         : 192.168.1.31 (Ethernet)
> Service          : ..., dialecte 22
> Empreinte        : 0829cc7e...
> Accès distant    : activé, prêt à être contrôlé
> Réseau local     : ordinateurs de confiance
> Sessions ouvertes: 0
> Ordinateurs vus  : PC-VICTOR (192.168.1.20)
> ```
>
> Puis les quatre fichiers, exactement comme sur place : le service, le moteur client, le moteur hôte, la fenêtre.
>
> **Ce qu'il faut vérifier en priorité** : que c'est bien le nom de l'**autre** machine sur la ligne « Ordinateur », et pas celui de celle où l'on est. C'est la seule erreur qui rendrait la page trompeuse plutôt qu'absente.
>
> **Le seul bouton qui doit avoir disparu** : **Ouvrir le dossier**. Ces fichiers-là ne sont pas ici. **Vider**, **Actualiser** et **Copier tout** restent.
>
> **Vider à distance, qui est l'autre moitié de l'essai.** Cliquer **Vider** puis **Confirmer** dans les quatre secondes. Attendu : la page se recharge presque vide, avec pour seule ligne du service `a computer asked this one to empty its journal`. C'est la manière dont on cherche une panne : vider les deux journaux, refaire ce qui ne marche pas, lire les deux.
>
> **À faire dans cet ordre**, comme pour le journal local : vider, **puis** refaire l'essai, puis lire. Vider après coup efface exactement ce qu'on voulait lire.
>
> **Le cas qui finit mal, et il doit finir proprement** : cliquer le journal d'un ordinateur éteint, ou débranché du réseau. Attendu : la fenêtre s'ouvre quand même et affiche le refus en toutes lettres, celui-là même qu'aurait donné une tentative de session, plutôt que de rester sur « Lecture… » ou de ne rien faire.
>
> **Et une trace des deux côtés** : dans `service.log` de la machine lue, `a computer asked this one for its journal, and it was handed over`. Lire une machine à distance laisse une ligne chez elle, comme tout le reste.
>
> **Le journal local ne doit pas avoir changé** : le bouton de l'en-tête ouvre la même page qu'avant, avec Vider et Ouvrir le dossier. Elle vient maintenant du service ; si le service est arrêté, la page arrive quand même, avec la raison écrite sur la ligne **Service** et les quatre fichiers en dessous.

> **R61 (où passent les secondes d'une ouverture)**
>
> **Rien à faire, juste à lire.** Ouvrir une session, n'importe laquelle, et relever la dernière ligne de l'ouverture dans `interface.log` du **client** :
>
> `l'image est là après 5240 ms : 380 ms pour joindre l'ordinateur distant, 1900 ms à lui demander ce qu'il faut, 60 ms à lancer le lecteur, 2900 ms avant sa première image`
>
> Les quatre morceaux ne se réparent pas au même endroit et un seul est vraiment le nôtre. « Joindre l'ordinateur distant » est le réseau et la course entre les adresses ; « lui demander ce qu'il faut » est ZyrDesk, ce sont les enceintes, la cadence et l'écran, et c'est là que le travail sur les écrans a pu coûter ; « lancer le lecteur » est Windows qui démarre un programme ; « avant sa première image » est le moteur client qui se connecte, s'appaire et décode.
>
> **À relever dans les deux sens et sur les trois machines**, parce que le morceau qui coûte n'est pas le même partout : celle qui emprunte son écran virtuel paie une seconde de plus que les autres dans le deuxième morceau.

> **R60 (l'écran virtuel en dernier recours, et tout revient)**
>
> **Sur le PC dont la carte graphique refuse tout bureau plus grand que sa dalle.** Cet essai en compte deux à la suite, et c'est voulu.
>
> **La première session**, en **Résolution du client** : elle est servie à la taille de l'hôte, comme avant. Le journal de l'hôte dit ce qui vient d'être appris :
>
> `\\.\DISPLAY1 draws nothing larger than itself, so the next engine on this computer films the screen it grows instead and a session can borrow that one`
>
> **À la fermeture**, le bureau revient d'abord, **puis** le moteur redémarre tout seul, personne ne regardant plus. L'ordre compte et il se lit dans le journal :
>
> `this computer's screens are back the way they were (N of them)`
> `this computer's own screens draw nothing larger than themselves, so the engine starts over to film the screen it grew instead`
>
> Si la seconde ligne arrive **avant** la première, ou si tu vois `a desk was left the way a session left it by a run that did not finish`, c'est que le moteur est reparti trop tôt : envoie-le moi.
> `this computer is filmed on the screen it grew for itself ({...}), its own drawing nothing larger than themselves`
>
> **La deuxième session**, la même : elle doit arriver à la taille demandée.
>
> `this computer's own screens draw nothing larger than themselves, so the one it grew is woken at the size asked for and the desktop moves onto it`
> `this computer's desktop is on \\.\DISPLAY2 at 1920x1200 for the length of the session, and its own screens are still on`
> `this computer is showing 1920x1200`
>
> **Ce qu'il faut regarder du côté client, et c'est le point le plus important** : l'image doit se comporter comme **un seul écran**. Une fenêtre agrandie remplit l'image, le pointeur ne sort pas par un bord dans le vide, rien ne disparaît sur le côté. L'écran physique de l'hôte, lui, s'éteint le temps de la session : c'est voulu, et c'est le prix pour que la session ressemble à une machine à un écran.
>
> **Et le retour, qui compte encore plus.** À la déconnexion, dans cet ordre : le bureau revient sur l'écran physique, puis l'écran virtuel s'endort. Le journal le dit dans cet ordre aussi.
>
> `this computer's screens are back the way they were (N of them)`
> puis les lignes de l'écran virtuel qui s'endort.
>
> **Les trois cas qui finissent mal, à faire tous les trois** : fermer la session normalement, arracher le réseau du client en pleine session, et tuer le service de l'hôte en pleine session puis le relancer. Dans les trois, l'écran physique de l'hôte doit revenir exactement comme avant et l'écran virtuel disparaître des paramètres d'affichage.

> **R59septies (un « c'est fait » de Windows qui n'a rien fait)**
>
> **L'essai** : depuis le portable **1920x1200**, prendre la main sur un PC dont l'écran est **1920x1080**, en **Résolution du client**.
>
> Attendu : le PC dessine un bureau **1920x1200** dans sa dalle 1920x1080, donc avec des bandes noires en haut et en bas, et le journal annonce `this computer is showing 1920x1200`. Ce qui se passait avant : deux lignes qui se contredisaient dans la même seconde, la seconde ayant raison, et l'image arrivait en 1920x1080 comme si on avait choisi la résolution de l'hôte.
>
> **Les lignes qui le disent**, dans `service.log` de l'hôte :
>
> `\\.\DISPLAY1 offered no 1920x1200 of its own (...), so a desktop that size is asked for instead: \\.\DISPLAY1 draws a 1920x1200 desktop, shrunk into its own panel`
> `this computer is showing 1920x1200`
>
> **Et si cette machine ne sait vraiment pas le faire**, la ligne le dit maintenant avec les deux chiffres et la raison, au lieu d'annoncer une réussite :
>
> `Windows said yes to a 1920x1200 desktop on \\.\DISPLAY1 (asked for exactly) and left it drawing 1920x1080; the request carried everything Windows asks for, so what is left is a graphics card that draws no desktop larger than the panel on this output`
>
> C'est cette ligne-là qu'il faut m'envoyer : elle sépare « cette machine ne peut pas » de « on a mal demandé », et les deux se réparent différemment. **Toutes les machines ne savent pas le faire** : celles qui y arrivent jusqu'ici ont une carte Intel et une dalle interne, celle qui n'y arrive pas a une carte d'un autre fabricant et un écran externe. Sur celle-là, résolution de l'hôte et résolution du client donnent la même image, et c'est honnête plutôt que cassé.
>
> **Le retour compte autant** : à la déconnexion, le PC doit revenir en 1920x1080 sans bandes, tout seul.

> **R59sexies (le moteur filme l'écran principal, et il y reste)**
>
> **L'essai se fait sur l'hôte à plusieurs écrans**, et c'est le seul endroit où il se voit. Ouvrir une session en **Résolution du client** vers le PC à deux écrans 4K, en regardant ce qui arrive.
>
> Attendu : c'est **l'écran principal** de l'hôte qui arrive, celui de droite, et il y reste. Ce qui se passait avant : l'écran principal prenait bien la taille demandée, et l'image reçue était celle de l'écran de gauche, aplatie dans cette taille.
>
> **La ligne qui le dit**, dans `service.log` de l'hôte, au démarrage du moteur :
>
> `the engine is aimed at this computer's main screen ({...})`
>
> Et la liste juste au-dessus dit maintenant lequel des écrans est le principal :
>
> `screens the engine sees: SAMSUNG ({...}, off) ; U28G2G6B ({...}, on at 3840x2160, the main one) ; ...`
>
> **Le premier démarrage après cette mise à jour redémarre le moteur une fois**, le temps d'apprendre ce nom, et le journal le dit : `this computer's main screen is {...} and the engine was aimed at whichever it found first, so it starts over to film the right one`. Aux démarrages suivants, plus de redémarrage.
>
> **La moitié qui est dans le moteur hôte**, et qui demande sa recompilation : c'est elle qui empêche le moteur de se rabattre sur l'écran d'à côté pendant qu'on change la définition du bon. Sans elle, le nom seul ne suffit pas au moment précis du basculement.
>
> **Les deux essais à refaire ensemble** : R59quinquies juste en dessous, qui est le basculement lui-même, et celui-ci, qui est l'écran filmé. C'est le même clic.

> **R59quinquies (basculer sur la résolution de l'hôte lui rend son bureau)**
>
> **L'essai, et il tient en trois clics.** Ouvrir une session en **Résolution du client** vers l'hôte 4K, laisser l'image arriver, puis, **sans fermer la session**, ouvrir le menu du bouton flottant et passer en **Résolution de l'hôte**.
>
> Attendu : l'hôte revient en **3840x2160 à 175 %**, donc à son propre bureau, et l'image reçue devient une image 4K. Ce qui se passait avant : il restait en 1920x1200, et il y restait pour toutes les sessions suivantes de la soirée, y compris après les avoir fermées.
>
> **Les lignes qui le disent**, dans `service.log` de l'hôte, au moment du basculement :
>
> `a session asks this computer to keep its own screen`
> `this session wants this computer's own screen, so the desk an earlier one took is given back first`
> `this computer's screens are back the way they were (N of them)`
> `this computer is showing 3840x2160`
>
> La dernière est celle qui tranche : c'est ce que l'hôte annonce au client. Si elle dit encore 1920x1200, le bureau n'a pas été rendu.
>
> **Et le chemin inverse, à faire dans la foulée** : repasser en **Résolution du client** sans fermer la session. L'hôte doit reprendre 1920x1200, et revenir à son 4K à la fermeture.

> **R59quater (l'agrandissement revient, y compris celui que Windows ne sait plus dire)**
>
> **L'essai, et il se fait dans les deux sens.** Ouvrir une session en **Résolution du client**, la garder une minute, la fermer. Puis recommencer depuis l'autre machine. À chaque fois, ce qu'il faut regarder est l'ordinateur **hôte**, une fois la session finie : sa définition **et** son agrandissement doivent être exactement ceux d'avant.
>
> **Le cas qui a mis quatre essais à tomber.** Sur le portable, l'agrandissement ne revenait jamais : il restait à 100 % au lieu de 125 %, et il fallait le remettre à la main dans les paramètres d'affichage après chaque session. Windows ne garde pas « 125 % » mais un cran le long d'une liste, compté depuis celui qu'il recommande pour cet écran **à la taille qu'il a en ce moment**. Poser 175 % pendant que le bureau est en 4K, puis rendre le bureau, laisse l'écran sur un cran qui n'existe plus dans la liste : interrogé, il ne répond alors plus rien, et un écran qui ne répond rien n'était plus jamais remis en place.
>
> **Les lignes qui le disent**, dans `service.log` de l'hôte, à la fermeture :
>
> `this computer's screens are back the way they were (N of them)`
> `\\.\DISPLAY1 draws at 125 %`
>
> La seconde est celle qui compte. Elle n'apparaît **que si l'agrandissement avait bougé** : un écran déjà revenu au bon chiffre ne dit rien, et c'est normal.
>
> **La mémoire, à essayer exprès.** Changer l'agrandissement du portable dans Windows, par exemple de 125 % à 150 %, puis ouvrir et fermer une session. Il doit revenir à **150 %**, c'est-à-dire au chiffre choisi et non à celui que Windows recommande. Le fichier qui le retient est `data/screen/screen-scales.txt` à côté du service, une ligne par écran.
>
> **La ligne du dernier recours**, qui ne doit apparaître que sur un écran devenu muet :
>
> `\\.\DISPLAY1 will not say how large it draws, so what it drew at last time is used: 150 %`
>
> **Et si ça échoue**, la ligne à m'envoyer est celle-ci : `\\.\DISPLAY1 was not put back to 125 %: it is not among the screens Windows describes`. C'est la seule raison qui reste d'abandonner un écran, et elle veut dire que Windows ne le décrivait plus du tout au moment où on le lui a demandé.

> **R59ter (un portable dessine un bureau plus grand que sa propre dalle)**
>
> **Le même essai que R59bis, et il doit maintenant réussir au lieu de ne rien faire** : depuis le **PC 4K à 175 %**, prendre la main sur le **portable 1920x1200**, en **Résolution du client**.
>
> Attendu : le portable dessine un vrai bureau **3840x2160**, sans qu'aucun écran virtuel n'apparaisse et sans que sa dalle change de définition. Ce qu'on voit sur la dalle du portable est ce bureau réduit dedans, donc tout en petit, avec des bandes noires en haut et en bas puisque le 16:9 ne remplit pas un 16:10. C'est exactement ce que fait le produit de référence, capture d'écran à l'appui. Sur le PC 4K, l'image doit être **nette**, sans le flou d'un 1920x1200 étiré.
>
> **Les lignes qui le disent**, dans `service.log` du portable :
>
> `\\.\DISPLAY1 offers no 3840x2160 of its own (1920x1200, 1920x1080, ...), so a desktop that size is asked for instead: \\.\DISPLAY1 draws a 3840x2160 desktop, shrunk into its own panel`
> `this computer is showing 3840x2160`
>
> La seconde est la preuve que ça a pris : c'est ce que l'hôte annonce au client. Si elle dit encore 1920x1200, le bureau n'a pas été agrandi.
>
> **Si Windows refuse**, la ligne le dit avec son propre numéro (`Windows would not give ... a 3840x2160 desktop (answer N)`), et c'est cette ligne qu'il faut m'envoyer : ce chemin est le seul du produit qui touche à l'interface d'affichage moderne, et il n'a encore jamais tourné sur une vraie machine.
>
> **À vérifier avec au moins autant de soin : le retour.** À la déconnexion, le portable doit revenir en 1920x1200 **à 125 %**, en 16:10, sans bandes, sans rien faire à la main. Un bureau resté plus grand que la dalle est précisément ce qu'il faudrait réparer à la main, donc c'est le point qui compte le plus. Le journal doit dire `this computer's screens are back the way they were`, et sa dalle doit être exactement comme avant.
>
> **Les deux cas qui finissent mal, à refaire ici aussi** : arracher le Wi-Fi du PC 4K en pleine session, et tuer le service du portable en pleine session puis le relancer. Le bureau du portable doit revenir tout seul dans les deux cas.
>
> **Le cas jumeau, dans l'autre sens** : depuis le portable vers le PC 4K, rien ne doit avoir changé de ce qui marchait déjà (R59). Ce chemin ne passe pas par le nouveau code, puisque l'écran 4K offre bel et bien la taille demandée.

> **R59bis (l'hôte qui ne sait pas dessiner ce qu'on lui demande garde tout ce qu'il a)**
>
> **Le sens qui manquait, et il se fait dans l'autre sens que R59** : depuis le **PC 4K à 175 %**, prendre la main sur le **portable 1920x1200 à 125 %**, en **Résolution du client**.
>
> Le portable ne sait pas dessiner du 3840x2160. Ce qui doit se passer : il **garde sa taille et son agrandissement**, l'image arrive en 1920x1200 et elle est agrandie sur l'écran 4K. Ce qui ne doit **pas** se passer : le portable à 1920x1200 dessiné à 175 %, tout deux fois trop gros.
>
> **Et surtout, à la déconnexion** : le portable doit être **exactement** comme avant, 1920x1200 à 125 %. C'est ce qui obligeait à remettre 125 % à la main.
>
> **Les lignes qui le disent**, dans `service.log` du portable :
>
> `Windows would not put \\.\DISPLAY1 at 3840x2160: that screen does not have that size`
> `\\.\DISPLAY1 cannot draw 3840x2160, so it keeps its own size and its own magnification`
>
> La deuxième est la nouvelle, et c'est elle qui compte. L'ancienne trace disait `\\.\DISPLAY1 draws at 175 %` juste après le refus : si elle revient, le défaut est revenu avec.
>
> **Le cas jumeau, à faire aussi** : en **Résolution de l'hôte** dans ce même sens, rien ne doit changer du tout sur le portable, ni la taille ni l'agrandissement.

> **R59 (tes écrans restent allumés, et tout revient exactement comme avant)**
>
> **C'est l'essai le plus important du lot, et il se prépare.** Sur le **PC hôte**, arranger les écrans exactement comme on les veut : celui de droite et celui de gauche dans le bon ordre, la télé **éteinte** dans les paramètres d'affichage de Windows, l'écran principal désigné. Noter mentalement, ou en photo, ce que montrent les paramètres d'affichage.
>
> **L'essai :** ouvrir une session en **Résolution du client** depuis un portable 1920x1200, la garder une minute, la fermer.
>
> **Ce qui doit se passer pendant :** l'écran principal de l'hôte passe en 1920x1200, et **rien d'autre ne bouge**. Les autres écrans restent allumés, à leur taille, à leur place. La télé reste éteinte. Aucun écran nouveau n'apparaît.
>
> **Ce qui doit se passer après :** tout revient exactement comme avant, dans la seconde. Rouvrir les paramètres d'affichage et comparer avec la photo : mêmes écrans allumés, mêmes tailles, mêmes places, même écran principal, télé toujours éteinte.
>
> **Les lignes qui le disent**, dans `service.log` de l'hôte :
>
> `a session asks this computer's main screen for 1920x1200@125, and its desk is written down first`
> `this computer's desk is written down before the session touches it (N screens)`
> `\\.\DISPLAY1 is showing 1920x1200`
> `this computer is showing 1920x1200`
>
> puis, à la fin :
>
> `this computer's screens are back the way they were (N of them)`
>
> **Ce qu'on ne doit plus jamais voir :** `dd_configuration_option` dans la configuration du moteur, un écran qui s'éteint pendant une session, ou la télé qui se rallume au démarrage du service.
>
> **Le fichier qui sert de preuve.** Pendant la session, `data/screen/desk-before.txt` porte une ligne par écran, lisible à l'oeil : c'est ce qui sera remis. Après la session il doit avoir **disparu**. S'il est encore là, c'est que la remise a échoué, et ses lignes disent ce qu'on attendait.
>
> **Les deux cas qui finissent mal, et ils comptent autant que le premier.** Ouvrir une session, puis **arracher le Wi-Fi du portable** : le bureau de l'hôte doit revenir tout seul en quelques secondes (`nobody is watching this computer any more, its desk goes back the way it was`). Puis, session ouverte, **tuer le service de l'hôte** et le relancer : au démarrage il doit dire `a desk was left the way a session left it by a run that did not finish, putting it back`.
>
> **Et la machine sans écran, si tu peux l'essayer.** Sur une machine dont on débranche physiquement l'écran, l'écran virtuel reprend son rôle et c'est le seul cas où il sert encore : le journal dit `no screen is plugged into this computer, so the engine is aimed at the one it grew for itself`.

> **R58 (le bureau distant est à la taille de l'écran d'ici, et écrit de la même taille)**
>
> **Il faut un client dont l'agrandissement Windows n'est pas 100 %.** Un portable est le cas courant : Windows y met 125 % ou 150 % tout seul. Le vérifier dans ses paramètres d'affichage, sous « Mise à l'échelle ».
>
> **L'essai :** depuis ce client, en **Résolution du client**, ouvrir une session vers l'hôte. Le bureau distant doit arriver à la bonne taille **et écrit de la même taille qu'ici**. Avant, tout y était deux fois plus petit : la résolution passait, l'agrandissement non.
>
> **Les lignes qui le disent.** Dans le journal de la fenêtre, côté client, la ligne d'ouverture porte maintenant l'agrandissement mesuré : `écran de cet ordinateur : 1920x1200 pixels réels à 60 Hz, agrandissement 125 %`.
>
> Dans `service.log` de l'**hôte**, trois lignes dans cet ordre :
>
> `the virtual screen is asked to draw at 125 %, the way the screen watching it does`
> `the virtual screen draws at 125 %`
> `the magnification was asked for from the session on screen (...)`
>
> **Les deux cas où l'agrandissement ne voyage pas, et c'est voulu.** En **Résolution de l'hôte**, rien n'est touché du tout, ni la taille ni l'agrandissement : c'est ce que cette entrée promet. Avec une **taille choisie à la main**, l'écran virtuel prend l'agrandissement que Windows recommande pour cette taille-là, puisque cette taille n'est l'écran de personne. Le journal de l'hôte le dit : `the session named no magnification, so the virtual screen goes back to the one Windows recommends for it`.
>
> **Ce qu'il ne faut pas voir :** l'écran physique de l'hôte changer d'agrandissement. Seul l'écran virtuel est touché, jamais celui de son propriétaire.
>
> **Les deux machines doivent porter ce lot.** L'agrandissement ajoute un champ au dialecte que les deux moitiés du produit parlent entre elles, donc la version de ce dialecte monte : une machine restée en arrière le dit au lieu de mal comprendre, et la session est refusée avec un message qui nomme la version.

> **R57 (lancer ZyrDesk ne touche plus aux écrans de personne)**
>
> **Le préalable :** sur la machine hôte, arranger les écrans comme on les veut, **en éteindre un** dans les paramètres d'affichage de Windows.
>
> **L'essai :** arrêter ZyrDesk, le relancer. Trois fois de suite.
>
> Attendu : l'écran éteint **reste éteint**, les trois fois. Avant, il se rallumait à chaque lancement.
>
> **Les lignes qui le disent.** Dans `service.log`, une seule fois, au premier démarrage après la mise à jour :
>
> `the engine was holding an arrangement of screens it can never put back, naming this computer's virtual screen (...); it has been dropped`
>
> Et dans `engine-console.log`, ces deux-là ne doivent **plus jamais** apparaître au démarrage :
>
> `Failed to change topology to: [["{...}"]]`
> `Failed to revert display device configuration ... Enabling all of the available devices`
>
> **L'autre moitié**, à vérifier en tuant le service en pleine session puis en le relançant : la ligne `a screen was left awake by a run that did not finish, putting it back` doit arriver **après** `engine started`, jamais avant.

> **R56 (le verrouillage ne fige plus une seconde et demie)**
>
> **Le moteur hôte doit avoir été recompilé**, sans quoi il n'y a rien à voir.
>
> Session ouverte, **Verrouiller** dans le menu flottant. L'image doit repartir presque tout de suite au lieu de rester figée une à deux secondes.
>
> **Le chiffre qui le prouve**, dans `engine-console.log` de la machine verrouillée :
>
> `Capture reinitialized after ...ms (...ms waiting for the encoders to let the display go, ...ms finding it again)`
>
> La seconde moitié, « finding it again », donnait quatre cent vingt-huit à six cent dix millisecondes. Elle doit maintenant tomber à quelques dizaines. La première moitié, elle, ne change pas : ce n'est pas ce qui a été touché.
>
> **Compter aussi les lignes.** Il y en avait trois pour un seul verrouillage. Si elles restent trois, c'est le prochain sujet ; si le total du gel est déjà supportable, ça peut attendre.

> **R55 (le verrouillage, chronométré de bout en bout)**
>
> **Ouvrir une session, puis Verrouiller dans le menu flottant.** L'image se fige encore, c'est attendu : ce lot mesure, il ne corrige pas.
>
> **Trois journaux à envoyer, et ils se lisent ensemble par leurs horodatages.**
>
> Sur la machine qui regarde, `interface.log` :
>
> `verrouillage de l'ordinateur distant : fait en 420 ms`
>
> Sur la machine verrouillée, `service.log`, deux lignes qui encadrent l'attente :
>
> `the far computer asked this one to lock itself`
> `this computer locked itself after 380 ms (150 ms starting a program in the session on screen, 230 ms waiting for it)`
>
> La première moitié est le prix que Windows demande pour lancer un programme dans la session de l'écran ; la seconde est le verrouillage lui-même, bureau compris, puisque ce programme attend maintenant que le bureau change vraiment de mains.
>
> Sur la même machine, `engine-console.log`, à la même seconde :
>
> `Capture reinitialized after 900ms (300ms waiting for the encoders to let the display go, 600ms finding it again)`
>
> **C'est cette dernière ligne qui tranche, et son absence tranche autant.** Si elle est là, le gel est le moteur qui refait sa capture parce que le bureau a changé, et c'est de ce côté qu'il faut travailler. Si elle n'est pas là, le moteur n'a rien refait du tout et le gel est simplement une image qui ne change plus pendant que Windows dessine l'écran de verrouillage : ce n'est alors pas du tout le même travail.

> **R54 (le journal dit quand le trajet s'allonge)**
>
> À l'ouverture d'une session, le journal du service donne le point de départ : `way 1 open towards 192.168.2.5 on 127.77.27.36, round trip 12 ms`.
>
> **L'essai.** Session ouverte et qui tourne, activer le VPN commercial sur l'une des deux machines. Le journal doit écrire, dans les deux secondes :
>
> `way 1 towards 192.168.2.5: the road is now 24 ms, it was 12 ms`
>
> Puis le désactiver : la ligne repart dans l'autre sens.
>
> **Ce que ça ne fait pas**, et c'est volontaire : ça ne déplace pas la session. La destination n'a pas changé, c'est le trajet vers elle qui a changé, et ce trajet-là appartient à la machine et à ses réglages de VPN, pas à ZyrDesk. La ligne existe pour que ça se voie sans ouvrir les statistiques.

> **R52bis (chaque ordinateur dit toutes ses adresses)**
>
> **La ligne à lire avant même d'ouvrir une session.** Sur chaque machine, dans le journal du service, chercher la ligne où l'autre est trouvée. Elle doit maintenant nommer plus d'une adresse quand la machine d'en face en a plusieurs :
>
> `PC-VICTOR at 192.168.2.20, also answering at 192.168.1.20, 10.141.87.37`
>
> Si elle n'en nomme qu'une, c'est que la machine d'en face n'en a qu'une, ou qu'elle n'a pas été recompilée. **Les deux machines doivent l'être** : ce dialogue change de version, et deux versions différentes ne se parlent plus du tout par ce chemin (l'annonce mDNS, elle, continue de marcher).
>
> **Puis ouvrir la session** et vérifier la ligne de R52 : `racing N addresses`, avec N supérieur à 1. Si le journal dit encore `opening a way to <adresse>, expecting <empreinte>`, c'est qu'il n'y a toujours qu'un seul chemin connu et rien n'est réglé.

> **R50 (l'appairage se refait autant de fois qu'on veut)**
>
> Dans les réglages, **oublier** l'ordinateur d'en face, puis rouvrir une session : les deux se réappairent tout seuls et l'image arrive.
>
> **Le cas qui cassait tout.** Recommencer, mais cette fois **fermer la fenêtre pendant l'appairage**, avant que l'image n'arrive. Puis rouvrir une session immédiatement.
>
> Attendu : ça repart. Avant ce lot, l'appairage interrompu restait coincé dans le moteur d'en face et **tous** les suivants échouaient sur `400 Invalid uniqueid`, définitivement, jusqu'au redémarrage du service de cette machine. Recommencer trois ou quatre fois d'affilée : chaque tentative doit se comporter comme la première.

> **R43 (le bouton suit le curseur, et ne laisse rien derrière lui)**
> 
> Pendant une session, prendre le bouton flottant tout en haut de l'image et le **descendre lentement jusqu'en bas**, sans lâcher, puis remonter, plusieurs fois.
> 
> Attendu : le logo reste **collé au curseur** du début à la fin. Il ne doit jamais sauter d'un coup vers le haut de l'écran, ni rester en arrière pendant que la main continue.
> 
> Lâcher en bas, puis ouvrir le menu : il s'ouvre vers le haut, entier (R40). Le refermer, remonter le bouton, rouvrir : il s'ouvre vers le bas.
> 
> **Et rien ne doit rester sur l'image.** Après chaque déplacement et chaque ouverture, regarder autour du bouton : aucun rectangle, aucune croix, aucun morceau clair par-dessus l'image. Le passage du curseur ne doit rien effacer non plus, parce qu'il n'y a rien à effacer.
> 
> Dans le journal de la fenêtre, la ligne `N morceaux dessinés jusqu'à LxH` ne doit jamais porter une hauteur de **0** : c'est la signature d'un dessin lu à l'envers, et c'était la cause des deux.

> **R45 (le pointeur reste dans l'image en plein écran)**
>
> À faire sur un ordinateur à **deux écrans**, sinon il n'y a rien à voir : c'est là que le pointeur avait où s'échapper.
>
> Pendant une session **fenêtrée**, pousser le pointeur vers le bord droit de l'image : il doit en sortir et passer sur le reste du bureau, puis sur le deuxième écran. C'est voulu, les autres fenêtres sont là.
>
> Passer en **plein écran** (Alt+&). Repousser le pointeur vers le même bord : il doit maintenant **s'arrêter au bord de l'image** et ne jamais atteindre le deuxième écran.
>
> Repasser en fenêtré : il doit pouvoir ressortir aussitôt.
>
> Le journal de la fenêtre dit `pointeur tenu dans l'image, qui est tout l'écran` et `pointeur rendu à l'écran, l'image n'en occupe plus la totalité` à chaque bascule.
>
> **Et il ne faut pas se retrouver enfermé.** En plein écran avec le pointeur tenu et le clavier immersif, les raccourcis du produit marchent toujours : Alt+& rend l'écran, Alt+é termine la session, Alt+² ouvre le menu.

> **R46 (la cadence de l'écran d'en face se règle depuis ici)**
>
> Pendant une session, ouvrir le menu et regarder la ligne **Écran d'en face**. Deux mots : **Économe** et **Fluide**.
>
> Mettre **Fluide**, puis **Appliquer les changements**. L'image se relance, **et le moteur de la machine d'en face redémarre au passage** : c'est plus long qu'une relance ordinaire, c'est normal, son moteur ne lit ce réglage qu'à son démarrage.
>
> Attendu, une fois revenu : sur un bureau où rien ne bouge, **le pointeur glisse au lieu d'avancer par à-coups**. C'est le seul endroit où ça se voit.
>
> Repasser en **Économe** et appliquer : le pointeur redevient saccadé sur un bureau immobile, et la machine d'en face cesse d'encoder pour rien. C'est le bon réglage pour une machine qui n'arrive pas à suivre.
>
> Le journal du service **d'en face** dit `a session asked this computer to start resending a still screen`, puis `how this computer serves was changed, the engine starts over with it`. Celui d'ici dit `way N asked the far computer to start resending a still screen`.
>
> **Et ça se retient.** Terminer la session, en rouvrir une : la ligne est restée où on l'a laissée, et la machine d'en face est demandée pareil.
>
> **Le chiffre qui tranche, et il faut le lire.** Ouvrir les statistiques (Ctrl+Alt+Maj+S) et **ne plus toucher à rien** : ni souris, ni clavier, pendant dix secondes. La cadence doit tenir **60**, ou tout près. Si elle s'installe nettement en dessous, faire le calcul : `1000 / (16,7 + temps d'encodage de l'hôte)`. Si le résultat tombe sur ce que tu lis, c'est ce défaut-là qui est de retour, et pas la machine d'en face qui plafonne.
>
> **Et il faut vraiment ne pas bouger la souris.** Dès qu'elle bouge, les images arrivent d'elles-mêmes et la cadence remonte quoi qu'il arrive : c'est ce qui a masqué le défaut pendant tout le temps où il était là.

> **R47 (la session demande la cadence de ton écran)**
>
> D'abord savoir ce qu'est ton écran : clic droit sur le bureau, **Paramètres d'affichage**, **Paramètres d'affichage avancés**. Le nombre en hertz est celui qui compte.
>
> Ouvrir une session. Sur l'écran d'accueil, la ligne sous **Qualité** doit dire ce nombre-là : `1920 x 1080, 60 images par seconde, 20 Mb/s` sur un écran à soixante, `144 images par seconde` sur un écran à cent quarante-quatre.
>
> Le journal de la fenêtre dit la mesure en entier : `écran de cet ordinateur : 1920x1080 pixels réels à 60 Hz, agrandissement 100 %`, puis `image demandée au loin en 1920x1080 à 60 images/s et 20 Mb/s en H.264`.
>
> **Ce qui est borné, et pourquoi.** En dessous de trente, la session demande trente quand même : un écran plus lent que ça est une lecture qui a mal tourné. Au-dessus de cent quarante-quatre, elle demande une part entière de la cadence de l'écran, donc cent vingt sur un écran à deux cent quarante : chaque image supplémentaire est payée entièrement par la machine d'en face, et une cadence qui ne divise pas celle de l'écran ferait revenir l'irrégularité qu'on cherche à supprimer.
>
> **À faire sur deux écrans différents si tu en as.** Déplacer la fenêtre de ZyrDesk sur l'autre écran avant d'ouvrir la session : c'est l'écran de la fenêtre qui est mesuré, pas l'écran principal.

> **R47bis (il ne part plus d'images en trop)**
>
> Pendant une session, avec **Fluide**, ouvrir les statistiques (Ctrl+Alt+Maj+S) et **bouger la souris en continu** pendant dix secondes, en larges cercles.
>
> Attendu : la cadence tient la valeur demandée et **ne la dépasse pas**. Sur un écran à soixante, elle doit rester à soixante, pas monter à soixante-cinq ou soixante-dix.
>
> **Ce que ça corrige, et pourquoi ça ne se voyait pas à l'oeil.** La machine d'en face envoyait par moments deux images pour une : celle qu'elle venait de capturer et la répétition de la précédente, partie un cheveu trop tôt. Celui qui regarde jetait la répétition, il n'a nulle part où la montrer, donc il n'y a jamais eu de déchirure ni d'image doublée à l'écran. Ce que ça coûtait, c'est du travail en face et de la place sur le lien.
>
> **Le chiffre à côté.** Dans la ligne de statistiques, `dropped_jitter_pct` compte exactement ces images jetées faute de place à l'écran. Il doit rester très bas, sous le dixième de pour cent.

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
>
> **Et quand le service est arrêté**, la page doit venir quand même : c'est le moment où on l'ouvre le plus. La ligne **Service** porte alors la raison de son silence, et les quatre fichiers sont là malgré tout.

> **R23bis (le journal de l'ordinateur d'en face)**
>
> Sur l'accueil, la petite icône de journal en haut à droite d'une carte de « Mes ordinateurs ». Aucune session n'est nécessaire.
>
> Attendu : la même fenêtre, titrée **Journal de …** au nom de cette machine, et remplie de ce qu'elle a écrit chez elle. La ligne **Ordinateur** doit porter son nom à elle : c'est la seule erreur qui rendrait la page trompeuse plutôt qu'absente. Seul **Ouvrir le dossier** disparaît, ces fichiers n'étant pas ici ; **Vider**, **Actualiser** et **Copier tout** restent, et **Vider** vide bien celui d'en face.
>
> Sur un ordinateur éteint, la fenêtre s'ouvre quand même et montre le refus en toutes lettres, le même qu'aurait donné une tentative de session.
>
> Lire une machine à distance laisse une ligne chez elle : `a computer asked this one for its journal, and it was handed over`.

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

## Essai des touches système (Alt+Tab)

Ces touches sont prises par le moteur client et par lui seul, sans réglage pour l'éteindre : la façon de faire est expliquée en entier dans [../CLAVIER.md](../CLAVIER.md), et l'autre voie a été retirée ([D47](../DECISIONS.md)). Les moteurs doivent avoir été recompilés : un moteur d'avant ne connaît pas le mode demandé et refuse de démarrer.

**Tout l'essai se fait pendant une seule session, sans jamais se reconnecter.** C'est le point important : la panne d'origine se déclenchait une fois et ne se réparait qu'à la reconnexion, donc un essai coupé en deux ne prouve rien.

1. Se connecter, puis **Alt+Tab tout de suite**. La fenêtre doit changer sur le PC hôte, jamais ici.
2. **Agrandir puis restaurer la fenêtre, cinq fois**, en retestant Alt+Tab après chacune.
3. **Basculer plein écran puis fenêtre, cinq fois**, en retestant après chacune.
4. **Ouvrir et refermer le bouton flottant plusieurs fois**, en retestant après chacune.
5. Utiliser **Statistiques** et le **changement de mode souris** depuis ce menu, puis retester.
6. **Aller volontairement dans une vraie application locale** (navigateur, terminal) : Alt+Tab doit alors rester ici.
7. **Revenir dans la session** en cliquant l'image : Alt+Tab doit repartir vers l'hôte.
8. Vérifier qu'aucune touche Alt ou Control n'est restée coincée, sur les deux machines, en tapant du texte.
9. Vérifier que **tous les raccourcis de ZyrDesk répondent encore** : plein écran, statistiques, mode souris, menu, fin de session.

Ce qu'il faut lire ensuite, dans `session.log` (la trace du moteur client), sous `zyr:` :

- `the session has the keyboard` à chaque reprise du clavier, et jamais une perte sans retour ensuite ;
- `system keys: Tab … carried to the host …`, qui donne les appuis et relâchements vus, ce qui est parti vers l'hôte, ce qui a été laissé passer et pourquoi, et le nombre de fois où le crochet a été reposé. C'est ce dernier nombre qui compte : il doit monter aux moments des étapes 2, 3 et 4.

Et dans `interface.log`, `le premier plan passe ailleurs : processus N (nom.exe)` nomme qui a pris le premier plan, ce qui est l'explication ordinaire de l'étape 6 et jamais une panne.
