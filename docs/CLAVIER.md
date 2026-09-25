# Les touches que Windows garde pour lui

Ce document existe pour une raison précise : Alt+Tab a coûté une quinzaine d'allers-retours entre deux ordinateurs avant d'être compris, et rien dans le code ne disait pourquoi. Il est écrit pour que personne, nous compris, ne recommence.

À lire avant de toucher quoi que ce soit qui ressemble à « la session n'envoie pas telle touche à l'ordinateur d'en face ».

## Le problème, tel qu'il se voit

Une session est ouverte, tout marche, Alt+Tab bascule bien entre les fenêtres de l'ordinateur d'en face. Puis on agrandit la fenêtre, ou on passe en plein écran, ou on ouvre le menu du bouton flottant. À partir de là, Alt+Tab ouvre le sélecteur de fenêtres **de l'ordinateur qui regarde** au lieu d'aller au loin, et ça ne revient plus jusqu'à la fin de la session.

Le détail qui trompe : ça ressemble à s'y méprendre à un problème de premier plan. Ce n'en est pas un.

## Le deuxième problème, découvert le 2026-08-27

« La touche Windows n'arrive jamais sur la session. »

Deux moitiés sont nécessaires pour qu'une de ces touches parte au loin : que Windows ne l'attrape pas ici, et que ce qui l'envoie l'envoie vraiment. Le lecteur d'avant, un autre programme, fermait sa porte devant la touche Windows tant que sa fenêtre n'était pas au premier plan, ce qu'une fenêtre portée dans celle de ZyrDesk ne pouvait jamais être (piège numéro 2 ci-dessous). Depuis que l'image est une fenêtre de ZyrDesk, cette porte n'existe plus : ce que le crochet prend part directement au lecteur.

## La règle qui commande tout

Sous Windows, un programme qui veut voir les touches avant tout le monde pose un crochet bas niveau sur le clavier. Ces crochets forment une file, et **le dernier arrivé est servi en premier**. Chaque frappe descend la file, du plus récemment posé au plus ancien, et n'importe lequel peut l'avaler avant les suivants.

ZyrDesk posait le sien **une seule fois, au début d'une session, et plus jamais**. Tout programme qui posait le sien après passait donc devant lui, pour le reste de la session.

Les moments où la panne apparaissait sont exactement ceux où un autre crochet se pose : une fenêtre agrandie, un plein écran, un menu ouvert. Ce n'était pas une coïncidence, c'était la cause.

Et le journal le disait, à qui savait le lire : le compteur des frappes vues par ZyrDesk ne bougeait pas. Pas « refusées », pas « perdues » : **jamais arrivées**. Tout ce qui a été tenté autour du premier plan, du focus et des délais travaillait sur un événement qui n'existait pas.

## Ce que fait le produit aujourd'hui

L'image est une fenêtre de ZyrDesk, dans la fenêtre de ZyrDesk, et le lecteur tourne dans le même programme. Le clavier n'a donc plus à passer d'un programme à l'autre : la fenêtre de l'image reçoit les touches comme n'importe quelle fenêtre, et les envoie au lecteur par leur place sur le clavier. Alt, F10 et Alt+Espace ne sont jamais rendus à Windows pendant une session : ils vont à l'ordinateur d'en face au lieu d'ouvrir un menu ici.

Pour les touches que Windows garde pour lui, ZyrDesk pose un crochet bas niveau sur le clavier, sur un fil qui ne fait rien d'autre. Ce crochet :

- est **reposé à chaque fois que le clavier revient à l'image**, donc il redevient le plus récent de la file aux moments précis où la panne se produisait. C'est la moitié qui compte. Le nouveau est posé **avant** de retirer l'ancien, sur le même fil : aucune frappe ne tombe dans un trou ;
- ne prend une touche que si **le clavier vient réellement à l'image** : la fenêtre de l'image a le focus du fil qui est au premier plan, question posée d'un coup au système ;
- n'avale que **Tab et F4 avec Alt, Échap avec Alt ou Ctrl, les deux touches Windows, Impr. écran et lecture/pause**. Alt, Control et Majuscule passent intactes ;
- décide sans verrou, sans journal et sans attente : chaque frappe de l'ordinateur attend sa réponse.

Ce qu'il prend part au lecteur précédé des modificateurs qu'il a vus tenus : Alt, puis Tab. Le Alt que la fenêtre de l'image lit ensuite est un appui que le lecteur a déjà, et qu'il n'envoie pas deux fois. Quand l'image perd le clavier, le lecteur relâche tout ce qui est enfoncé de l'autre côté, et l'hôte relâche de lui-même ce qu'il tient si le lien se tait.

## L'interrupteur : clavier partagé ou immersif

Prendre ces touches tout le temps est faux dans l'autre sens : la main qui va chercher Alt+Tab veut parfois une fenêtre de cet ordinateur-ci, et la touche Windows veut parfois ce menu Démarrer-là.

C'est donc un interrupteur, dans le menu du bouton flottant, à côté de ceux de la souris et du son. **Clavier : Partagé ou Immersif**, et celui qui est en place est allumé, ce qui est tout l'intérêt : un réglage qui décide où va une touche doit dire où il en est sans qu'on essaie.

En immersif, tout ce que Windows garde d'ordinaire pour lui part dans la session : Alt+Tab, Alt+Maj+Tab, Alt+Échap, Ctrl+Échap, la touche Windows seule et toutes ses combinaisons, la touche Impr. écran, et Alt+F4. En partagé, les touches Windows restent à cet ordinateur, et Alt+F4 sur l'image termine la session, comme la croix.

- Il se bascule **sans relancer l'image** : le crochet est posé ou retiré sur-le-champ.
- Il est **retenu** : le côté où on le laisse est celui où la session suivante s'ouvre.
- Il vaut **Immersif** par défaut. Une session dont la touche Windows ne fait rien sans qu'on sache pourquoi est exactement le défaut que tout ceci répare.

## Les deux qu'aucun logiciel n'aura jamais

**Windows+L et Ctrl+Alt+Suppr ne se prennent pas, quel que soit le côté de l'interrupteur, et aucun produit de bureau à distance ne les a.**

Ce n'est pas une limite de ZyrDesk ni un morceau qui manque. Windows traite ces deux-là dans une partie du système que les crochets ne voient pas, exprès : ce sont les deux gestes qui rendent la main à la personne physiquement assise devant la machine, et un programme qui pourrait les intercepter pourrait faire passer un faux écran de connexion pour le vrai. Le crochet a beau avaler la touche Windows, le système garde son propre compte pour ce cas-là et verrouille quand même.

Symétriquement, ils ne s'envoient pas non plus : le moteur d'en face pose les touches avec le même mécanisme ordinaire, qui ne peut pas plus déclencher Windows+L là-bas qu'ici.

**Elles ont donc chacune leur entrée dans le menu**, qui ne passe pas par le clavier du tout : la demande voyage sur le canal du produit et c'est le service d'en face qui l'exécute, étant le seul programme de cette machine à qui son Windows l'accorde.

| Ce qu'on veut | Ce qu'on fait |
|---|---|
| Ctrl+Alt+Suppr sur l'ordinateur distant | l'entrée **Ctrl+Alt+Suppr** du menu ([D59](DECISIONS.md)) |
| Verrouiller l'ordinateur distant | l'entrée **Verrouiller** du menu ([D71](DECISIONS.md)) |

Les deux sont exactement symétriques et pour la même raison, vue des deux côtés. Ctrl+Alt+Suppr, Windows ne l'accepte **que** d'un service, donc le service la presse dans son propre processus. Lever un écran de verrouillage, Windows ne l'accepte **que** d'un programme assis sur le bureau interactif, ce qu'un service n'est pas, donc le service se relance une seconde dans la session qui tient l'écran, exactement comme il le fait déjà pour couper les enceintes. Les deux refus protègent la même chose : ce que vaut un écran de verrouillage tient à ce que personne ne puisse le lever, ni le baisser, depuis l'extérieur du bureau auquel il appartient.

## Trois pièges, et pourquoi ils sont des pièges

**1. Le premier plan n'est pas le focus.** Le premier plan désigne la file d'entrée qui reçoit le clavier ; le focus désigne quelle fenêtre, dans cette file, le reçoit. Ce sont deux questions différentes et elles se répondent différemment.

**2. L'image d'une session ne peut jamais être au premier plan.** Elle est une fenêtre fille de celle de ZyrDesk, et le système donne le premier plan au chef de famille, jamais à un enfant. Toute condition de la forme « la fenêtre de l'image est-elle celle du premier plan » répond non pour la session entière. Le focus, lui, se lit sur la fenêtre de l'image elle-même : ce sont les deux messages que le système lui envoie quand elle reçoit et perd le clavier.

**3. Le focus seul ne suffit pas non plus.** Le focus d'une fenêtre reste posé dans son programme quand un autre programme passe devant, donc le focus seul répond « oui » pendant que quelqu'un travaille ailleurs. Un essai l'a montré, du temps du lecteur d'avant : dix-sept Alt+Tab tapés dans une autre fenêtre sont partis à l'ordinateur d'en face. D'où la question posée d'un coup : le focus du fil qui est au premier plan.

## Ce qu'il ne faut jamais faire

**Ne jamais avaler Alt.** Tous les raccourcis de ZyrDesk sont des combinaisons Alt, et ils passent par l'enregistrement de combinaisons du système, qui ne voit jamais une touche avalée par un crochet. Un mode qui avalait Alt et Control en entier a cassé tous les raccourcis du produit d'un coup ([D32](DECISIONS.md)).

**Ne jamais reprendre ces quatre pistes.** Elles ont toutes été essayées, elles sont toutes documentées avec leur relevé, et aucune ne pouvait marcher puisque la frappe n'arrivait pas :

| Piste | Pourquoi elle échoue | Où c'est écrit |
|---|---|---|
| Réparer le premier plan quand il revient | Windows refuse de rendre le premier plan à qui ne l'a pas déjà | [D39](DECISIONS.md) |
| Lire l'état réel des doigts pendant la frappe | Le système n'a pas fini avec la frappe dont il parle à ce moment-là | [D40](DECISIONS.md) |
| Vérifier que la remise au moteur a réussi | Elle réussissait déjà, à chaque fois | [D41](DECISIONS.md) |
| Traiter l'explorateur Windows comme « pas quelqu'un d'autre » | Son processus porte aussi les vraies fenêtres et la barre des tâches | [D42](DECISIONS.md) |

## Où le code vit

| Quoi | Où |
|---|---|
| Le crochet, la liste des touches prises et la décision | `crates/zyr-ui/src/system_keys.rs` |
| Le fil qui tient un crochet et le repose sans trou | `crates/zyr-ui/src/hook.rs` |
| La fenêtre de l'image, qui lit les autres touches et la souris | `crates/zyr-ui/src/video.rs` |
| L'interrupteur du menu | `crates/zyr-ui/src/floating.rs`, `crates/zyr-ui/src/menu.rs` |
| Ce qui relâche tout de l'autre côté | `crates/zyr-player` (perte du clavier), `crates/zyr-host` (fin ou silence du lien) |

## Si ça revient un jour

Le journal de la fenêtre dit, à chaque changement, ce qu'il en est du clavier :

1. `touches du système prises pour la session` / `rendues à cet ordinateur`, et `clavier immersif` / `clavier partagé` à chaque bascule. C'est l'interrupteur, et c'est la première chose à regarder : une touche qui ne part pas alors qu'il est du côté partagé n'est pas une panne.
2. `touches du système non prises : Windows a refusé le crochet du clavier`, avec le code d'erreur de Windows. Le crochet est redemandé à chaque retour du clavier sur l'image.

Ce qu'il ne faut pas faire, si le relevé ne dit rien de clair : ajouter un délai, une exception ou un rattrapage. Les quatre lignes du tableau plus haut sont exactement ça, et elles ont coûté une semaine.
