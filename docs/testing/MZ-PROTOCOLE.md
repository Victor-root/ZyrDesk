# Jalon MZ : le moteur ZyrDesk, pour la première fois sur deux vrais PC

Ce document se déroule sur les deux mêmes PC Windows qu'aux jalons précédents, côte à côte, sur le réseau local. Le serveur n'est pas nécessaire.

Vocabulaire : **PC hôte** = celui qu'on contrôle. **PC client** = celui depuis lequel on se connecte. **Moteur** = ce qui filme l'écran de l'hôte, le compresse et l'envoie. **Lecteur** = ce qui, sur le client, reçoit l'image, la décompresse et l'affiche.

**Ce que ce jalon fait.** Sunshine et Moonlight ne sont plus là. ZyrDesk a son propre moteur, écrit en Rust ([MOTEUR.md](../MOTEUR.md)). Sur l'hôte, il filme l'écran et le compresse avec la carte graphique. Sur le client, le lecteur décompresse l'image avec la carte graphique et la dessine dans la fenêtre de ZyrDesk elle-même. Le son, le clavier et la souris passent par le même tunnel qu'avant. Il n'y a plus de deuxième programme, plus de fenêtre posée sur la nôtre, plus de code d'appairage.

**Ce qu'il faut savoir avant de commencer.** Ce moteur n'a **jamais tourné sur un vrai Windows**. Il a passé 806 essais automatiques, dont une session entière à travers le vrai tunnel, mais sur une machine Linux, sans carte graphique, sans écran et sans réseau ([D223](../DECISIONS.md)). Ce protocole est donc son tout premier vrai essai. Il est normal que quelque chose ne marche pas du premier coup. Ce qui compte, c'est de dire exactement ce qu'on voit, et d'envoyer les journaux (voir la fin du document).

**Ce qu'il ne fait pas encore, et qu'il ne faut donc pas chercher.** Le débit ne s'adapte pas encore tout seul au réseau : c'est celui choisi dans le menu. Le 4:4:4 et plusieurs écrans de l'hôte affichés en même temps ne sont pas faits. La régularité des images (le seuil G-frame) est calculée par le lecteur mais pas encore affichée.

---

## Où on en est

**Règle de tenue :** un essai ne passe en « confirmé » que quand il a été essayé et dit tel quel. Rien n'y monte parce que le code a l'air juste ou parce que les tests automatiques passent.

### À vérifier maintenant

| Essai | Ce qu'il vérifie |
|---|---|
| **Z1** | Chaque PC trouve FFmpeg et sait se servir de sa carte graphique |
| **Z2** | Une session s'ouvre depuis l'accueil, et l'image arrive |
| **Z3** | La souris, en mode Bureau et en mode Jeu |
| **Z4** | Le clavier, en Partagé et en Immersif, et Ctrl+Alt+Suppr |
| **Z5** | Le son, et l'interrupteur « Son » |
| **Z6** | La fiche « Statistiques », et ce que dit chaque chiffre |
| **Z7** | Les changements en pleine session : débit, codec, résolution, écran de l'hôte |
| **Z8** | Le plein écran |
| **Z9** | L'écran de verrouillage et l'invite administrateur de l'hôte, vus et utilisés à distance |
| **Z10** | La fin de la session |
| **Z11** | Le réseau coupé 10 secondes, et la reprise |

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

Plus de script de moteurs à lancer : FFmpeg, la seule chose dont le moteur a besoin en plus, arrive avec `git pull`, déjà compilé, dans `vendor\ffmpeg`.

**Les deux ordinateurs doivent être à jour.** Un ordinateur resté sur l'ancienne version cherche encore les anciens moteurs, et aucune session ne s'ouvrira entre les deux.

### Faire le ménage (facultatif)

Les anciens moteurs ne servent plus à rien. Sur les deux PC, dans le dossier du projet, on peut supprimer les dossiers `data\engines`, `data\host` et `data\devices`, et les fichiers `data\session-stats.txt`, `data\session-wanted.txt` et `data\session-pointer.txt` s'ils existent. Plus rien ne les lit.

Rien à faire pour le pare-feu : le service retire lui-même, à son démarrage, les anciennes règles des moteurs. Il ne reste que les siennes : le port UDP 47000 du tunnel, et le 5353 qui sert aux ordinateurs à se trouver sur le réseau local.

### Lancer l'application

Sur **les deux PC** : double-clic sur `target\release\ZyrDesk.exe`, jamais depuis la fenêtre administrateur.

---

## Z1. Chaque PC trouve FFmpeg et sait se servir de sa carte graphique

Sur **les deux PC**, dans une fenêtre PowerShell **ordinaire** (pas administrateur), dans le dossier du projet :

```
.\target\release\zyr-cli doctor
```

**Attendu.** Deux lignes comptent :

- **FFmpeg** : « présent et chargeable, version 9.0.2 ».
- **Encodeurs vidéo** : ce que la carte graphique sait compresser. Par exemple `h264_nvenc, hevc_nvenc, av1_nvenc` sur une NVIDIA récente, des noms en `_qsv` sur Intel, en `_amf` sur AMD. `libx264` est toujours là : c'est le secours logiciel, qui marche sans carte graphique mais coûte plus au processeur.

Noter la liste des deux PC : c'est elle qui dira, plus loin, quels codecs sont proposés.

**Ce qu'il ne faut pas voir.** La ligne FFmpeg en échec : le dossier `vendor\ffmpeg` manque ou est incomplet, `git pull` n'est pas allé au bout. Sur un PC NVIDIA, aucun nom en `_nvenc` : le pilote NVIDIA est sans doute plus ancien que la version 570 (voir « Ce qui n'est pas encore sûr »). La session marchera quand même, mais avec l'encodeur logiciel.

## Z2. Une session s'ouvre, et l'image arrive

Sur le **PC client**, dans l'accueil, cliquer sur la carte du PC hôte.

**Attendu.**

- L'écran d'ouverture s'affiche, puis l'image du bureau de l'hôte apparaît **dans la fenêtre de ZyrDesk**, en quelques secondes au plus. Pas de deuxième fenêtre.
- Le logo ZyrDesk est posé en haut à droite de l'image. Un clic dessus ouvre le menu de la session.
- L'image est nette et bouge avec fluidité quand on déplace une fenêtre sur l'hôte.
- Dans le journal du client (voir la fin du document), une ligne dit combien de temps l'image a mis : `image à l'écran … ms après la demande`. **Noter ce chiffre** : c'est le seuil G-start, qui demande 4 secondes au plus en réseau local. Le mieux est de refaire l'ouverture dix fois et de noter les dix.

**Ce qu'il ne faut pas voir.** Une image noire qui ne part pas, un message d'erreur, ou la fenêtre qui revient à l'accueil. Dans ces cas, aller directement à la fin du document et envoyer les journaux : c'est l'essai le plus important de tous, et le reste en dépend.

## Z3. La souris

Le menu de la session a une ligne **Souris : Bureau / Jeu**. Le côté allumé est celui en place.

**En mode Bureau** (celui par défaut) :

- Le pointeur suit la main exactement, sans retard, parce que c'est celui du PC client qui est dessiné, avec la forme de celui de l'hôte (flèche, main sur un lien, curseur de texte).
- Clic gauche, clic droit, double-clic, glisser une fenêtre, la molette dans une page longue : tout agit sur l'hôte, là où est le pointeur.

**En mode Jeu** :

- Le pointeur reste enfermé dans l'image, et c'est l'hôte qui dessine le sien dans l'image.
- La souris envoie des mouvements et non une position : c'est ce que demandent les jeux et les logiciels 3D qui font tourner la vue.
- Le bouton ZyrDesk n'est plus cliquable, puisque la souris appartient à l'hôte. Pour revenir au mode Bureau, ouvrir le menu avec le raccourci **Alt+²** (la touche à gauche du 1).

**Ce qu'il ne faut pas voir.** Un clic qui tombe à côté de l'endroit visé, un pointeur décalé par rapport à l'image, ou un pointeur qui reste enfermé après être revenu en mode Bureau.

## Z4. Le clavier

Le menu a une ligne **Clavier : Partagé / Immersif**. Immersif est le réglage par défaut.

- **Taper du texte** dans le Bloc-notes de l'hôte, avec les accents (é, è, à, ç), les majuscules et les chiffres. **Attendu** : exactement ce qu'on tape, sur un clavier AZERTY comme sur l'autre.
- **En Immersif**, Alt+Tab, la touche Windows et Alt+F4 vont à l'hôte. **Attendu** : Alt+Tab change de fenêtre sur l'hôte, la touche Windows ouvre le menu Démarrer de l'hôte, pas celui du client.
- **En Partagé**, ces touches restent au client. **Attendu** : Alt+Tab et la touche Windows agissent sur le PC client, et Alt+F4 sur l'image termine la session, comme la croix.
- **Ctrl+Alt+Suppr** ne se tape pas : aucun logiciel ne peut l'envoyer ([CLAVIER.md](../CLAVIER.md)). Il se demande par l'entrée **Ctrl+Alt+Suppr** du menu. **Attendu** : l'écran bleu de Ctrl+Alt+Suppr de l'hôte apparaît **dans l'image**, et on peut y cliquer « Annuler ».

**Ce qu'il ne faut pas voir.** Une touche qui reste enfoncée sur l'hôte (une lettre qui se répète seule, ou Alt qui reste tenu), ou des lettres inversées.

## Z5. Le son

Lancer une vidéo ou de la musique sur l'hôte.

**Attendu.**

- Le son sort sur le PC client, sans craquement et bien calé sur l'image.
- La ligne **Son : Actif / Coupé** du menu coupe et rend le son sur le client, tout de suite, sans toucher à l'image.
- Si le réglage « Couper le son de l'ordinateur distant » est activé (Réglages, Avancé), les enceintes de l'hôte restent muettes pendant la session, et le son revient sur l'hôte à la fin.

**Ce qu'il ne faut pas voir.** Un son qui arrive en retard sur l'image, qui hache, ou pas de son du tout alors que l'hôte en joue.

## Z6. La fiche « Statistiques »

Menu de la session, entrée **Statistiques**. Une petite fiche s'affiche en bas à gauche de l'image, mise à jour cinq fois par seconde. Chaque chiffre est une moyenne sur la dernière seconde.

| Ligne | Ce qu'elle dit |
|---|---|
| Première ligne | Le codec, la taille de l'image et le nombre d'images reçues par seconde. Sur un bureau qui bouge, on attend 60 |
| **Hôte** | Le temps passé sur l'hôte, de la capture de l'écran à l'envoi de l'image |
| **Réseau** | Le temps d'un aller-retour entre les deux PC |
| **Décodage** | Le temps que met la carte graphique du client à décompresser une image |
| **Affichage** | Le temps pour dessiner l'image dans la fenêtre |
| **Débit** | Ce que l'image consomme sur le réseau, en mégabits par seconde |
| **Pertes** | Les images perdues en route, puis celles arrivées trop tard, remplacées par une plus récente avant d'être montrées |
| **Latence de bout en bout** | **Le chiffre le plus important.** Le temps entre le moment où l'hôte a filmé une image et le moment où elle est posée à l'écran du client. C'est ce qu'on ressent quand on bouge une fenêtre. Il ne compte pas le temps que met l'écran lui-même à allumer ses pixels : seule la mesure au téléphone de [perf/GATES.md](../../perf/GATES.md) le voit |

**Attendu.** Des chiffres stables. En réseau local, les pertes restent à 0 %, le réseau à quelques millisecondes, et la latence de bout en bout à un chiffre stable, de l'ordre d'une ou de quelques dizaines de millisecondes. **Noter la latence de bout en bout** sur un bureau qui bouge : c'est le chiffre que le jalon compare aux anciens moteurs.

En haut du menu, quatre de ces chiffres sont aussi lus en direct : Décodage, Encodage (qui est le temps « Hôte »), Réseau, Débit.

**Ce qu'il ne faut pas voir.** Des tirets `-` qui ne se remplissent jamais alors que l'image bouge, ou une latence qui grimpe sans cesse pendant la session.

## Z7. Les changements en pleine session

Tout se change dans le menu, sans rien valider, et **rien ne doit se relancer** : la fenêtre reste, la session reste.

- **Débit** : pousser la barre, de 5 à 80 Mb/s. **Attendu** : à 5 Mb/s, l'image devient plus floue quand beaucoup de choses bougent ; à 50, elle redevient nette. La ligne « Débit » de la fiche suit. Pas de coupure.
- **Codec** : les boutons Automatique, H.264, HEVC, AV1. Ceux que l'hôte ne sait pas produire sont barrés (comparer avec la liste de Z1). **Attendu** : un clic sur un autre codec, et l'image revient en une ou deux secondes au plus ; la première ligne de la fiche dit le nouveau codec.
- **Résolution** : choisir une autre taille dans la liste. **Attendu** : l'image revient à la nouvelle taille en quelques secondes, sans bande noire, et le bureau de l'hôte a changé de taille. Revenir ensuite à « Résolution du client ».
- **Écran de l'hôte** : cette ligne n'apparaît que si l'hôte a au moins deux écrans. **Attendu** : l'image passe sur l'autre écran en une seconde environ.

**Ce qu'il ne faut pas voir.** L'image qui reste figée après un changement, la fenêtre qui se ferme, ou un retour à l'accueil.

## Z8. Le plein écran

Menu, entrée **Fenêtré ou plein écran**.

**Attendu.** L'image prend tout l'écran du client, sans bande noire quand les deux écrans ont la même forme, et le logo ZyrDesk reste en haut à droite. La même entrée ramène la fenêtre. La session suivante s'ouvre comme on a laissé celle-ci.

## Z9. L'écran de verrouillage et l'invite administrateur de l'hôte

C'est l'essai qui montre que le moteur voit les écrans protégés de Windows : il tourne avec le compte système de l'hôte pour ça.

- **Verrouiller** : entrée **Verrouiller** du menu. **Attendu** : l'écran de verrouillage de l'hôte apparaît dans l'image. Taper le mot de passe au clavier, Entrée : le bureau revient, et la session continue. Une courte coupure qui se répare seule reste acceptable pour ce premier essai : noter sa durée.
- **L'invite administrateur** : sur l'hôte, à travers la session, clic droit sur « Invite de commandes » dans le menu Démarrer, puis « Exécuter en tant qu'administrateur ». **Attendu** : la fenêtre « Voulez-vous autoriser cette application… » apparaît dans l'image, sur un fond assombri, et les boutons Oui et Non se cliquent.

**Ce qu'il ne faut pas voir.** Une image noire ou figée pendant que l'hôte montre l'écran de verrouillage ou l'invite. Si c'est le cas, rapporter aussi ce que la personne voyait sur l'écran de l'hôte à ce moment-là.

## Z10. La fin de la session

Menu, **Terminer la session**.

**Attendu.** Le client revient à l'accueil. Sur l'hôte, le bureau reprend sa taille d'avant la session. Dans le journal de l'hôte, une ligne `the engine, process …, went with code 0` : le moteur de la session s'est arrêté proprement. Une nouvelle session s'ouvre ensuite normalement.

**Ce qu'il ne faut pas voir.** Une ligne `was still there after … s and was taken` : le moteur n'est pas parti de lui-même et le service a dû l'arrêter de force.

## Z11. Le réseau coupé 10 secondes

Pendant une session, **débrancher le câble réseau** du PC client, compter 10 secondes, le rebrancher. Avec le Wi-Fi, le couper puis le rallumer revient au même, mais un câble donne un essai plus net.

**Attendu.**

- Pendant la coupure, l'image se fige. Rien ne se ferme.
- Au retour du réseau, l'image repart **toute seule**, sans clic, en quelques secondes. Le jalon M7 visera 5 secondes au plus : noter le temps.
- Aucune touche ne reste enfoncée sur l'hôte.

**Ce qu'il ne faut pas voir.** La session qui se termine avec un message d'erreur, ou une image qui ne repart jamais.

---

## Ce qu'il faut envoyer si un essai échoue

Les journaux des **deux PC**, pris **juste après** l'essai raté, sans rien redémarrer :

- sur chaque PC : « Journal » depuis l'accueil (l'icône ▤ en haut à droite), puis le bouton **Copier tout**, et coller le tout dans le message ;
- ou, depuis le PC client seulement : l'icône du journal sur la carte de l'autre ordinateur ouvre **son** journal, avec le même bouton ;
- les fichiers eux-mêmes sont dans le dossier `data\logs` du projet, sur chaque PC.

**Si l'image reste noire**, les lignes qui comptent le plus sont celles du moteur. Dans la boîte de tri du journal, taper les noms, séparés par un espace, puis **Copier le tri** :

- sur le **PC hôte** : `engine gateway`. Les lignes `engine` sont celles du moteur lui-même : quels encodeurs il a trouvés, quel écran il filme, chaque flux qu'il ouvre. Les lignes `gateway` disent quand le service l'a lancé, et comment il s'est arrêté ;
- sur le **PC client** : `player picture session`. Les lignes `player` et `picture` sont celles du lecteur : le décodage de chaque flux (`decoding stream …`), et ce qui a été perdu ou refusé.

**Si l'image n'est pas fluide** (la fiche dit 60 images par seconde, mais une fenêtre qu'on déplace avance par à-coups), ce sont les lignes écrites chaque seconde qui comptent ([MOTEUR.md](../MOTEUR.md), section 9) :

1. sur les deux PC, dans le journal, **Vider** puis **Confirmer** ;
2. ouvrir la session, puis déplacer une fenêtre en rond sur l'hôte pendant 30 secondes, sans s'arrêter ;
3. terminer la session ;
4. dans la boîte de tri, taper les noms, puis **Copier le tri** :
   - sur le **PC hôte** : `pace engine tunnel`. Les lignes `pace` disent, image par image, quand chacune est partie du moteur et ce qu'elle y a attendu ; les lignes `tunnel` ce qui a traversé le service, et ce que la connexion mesure du chemin ;
   - sur le **PC client** : `flow measures tunnel`. Les lignes `flow` disent quand chaque image est arrivée, son décodage, et ce que l'écran en a vraiment montré ; les lignes `tunnel` la même chose que sur l'hôte, dans l'autre sens.

Dire aussi comment chaque PC était relié à Internet (câble, Wi-Fi, fibre, 4G) et la fréquence de l'écran du client (60 Hz, 144 Hz…).

Dire aussi, en une phrase, ce qui était branché où : quel PC était le client, câble ou Wi-Fi, quelle carte graphique de chaque côté.

## Ce qui n'est pas encore sûr

Honnêtement, et pour savoir quoi regarder si ça coince :

- **Rien de ce moteur n'a encore tourné sur un vrai Windows.** Les dessins faits par la carte graphique (conversion de l'image sur l'hôte, affichage sur le client) n'ont été vérifiés que par des outils, jamais sur une vraie carte. Une erreur là se voit comme une image noire ou aux couleurs fausses, avec une ligne au journal.
- **NVIDIA : pilote 570 ou plus récent.** En dessous, l'encodeur de la carte refuse de démarrer, et le moteur prend l'encodeur logiciel : la session marche, mais le processeur de l'hôte travaille beaucoup plus. La version du pilote se lit dans l'application NVIDIA ou dans le Gestionnaire de périphériques.
- **Les écrans protégés** (verrouillage, Ctrl+Alt+Suppr, invite administrateur) demandent que le moteur tourne avec le compte système de l'hôte. C'est ce que fait le service, mais ça n'a jamais été essayé en vrai.
- **Un écran HDR** sur l'hôte pourrait donner une image délavée. Le journal de l'hôte dit le format de l'écran filmé : `DXGI format 87` est l'ordinaire, `DXGI format 10` un écran HDR.
- **Un écran tourné** (en portrait) sur l'hôte n'a jamais été essayé.
- **L'image dans la fenêtre** : une image dessinée dans une fenêtre rangée à l'intérieur de celle de ZyrDesk, avec les coins arrondis de Windows 11, n'a jamais été affichée par une vraie carte. Si l'image reste noire alors que le journal du client dit `decoding stream …`, c'est sans doute là que ça coince : envoyer alors aussi les lignes `picture` et `video` du client.
- **Changer de sortie son** sur le client pendant la session devrait faire suivre le son ; jamais essayé non plus.
- **La première image** : si la carte graphique met plus d'un quart de seconde à préparer son décodeur, l'image démarre avec une image clé de plus. Sans gravité, une ligne `fallen … ms behind` au journal du client le dit.
