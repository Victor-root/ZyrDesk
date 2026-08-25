# Les touches que Windows garde pour lui

Ce document existe pour une raison précise : Alt+Tab a coûté une quinzaine d'allers-retours entre deux ordinateurs avant d'être compris, et rien dans le code ne disait pourquoi. Il est écrit pour que personne, nous compris, ne recommence.

À lire avant de toucher quoi que ce soit qui ressemble à « la session n'envoie pas telle touche à l'ordinateur d'en face ».

## Le problème, tel qu'il se voit

Une session est ouverte, tout marche, Alt+Tab bascule bien entre les fenêtres de l'ordinateur d'en face. Puis on agrandit la fenêtre, ou on passe en plein écran, ou on ouvre le menu du bouton flottant. À partir de là, Alt+Tab ouvre le sélecteur de fenêtres **de l'ordinateur qui regarde** au lieu d'aller au loin, et ça ne revient plus jusqu'à la fin de la session.

Le détail qui trompe : ça ressemble à s'y méprendre à un problème de premier plan. Ce n'en est pas un.

## La règle qui commande tout

Sous Windows, un programme qui veut voir les touches avant tout le monde pose un crochet bas niveau sur le clavier. Ces crochets forment une file, et **le dernier arrivé est servi en premier**. Chaque frappe descend la file, du plus récemment posé au plus ancien, et n'importe lequel peut l'avaler avant les suivants.

ZyrDesk posait le sien **une seule fois, au début d'une session, et plus jamais**. Tout programme qui posait le sien après passait donc devant lui, pour le reste de la session.

Les moments où la panne apparaissait sont exactement ceux où un autre crochet se pose : une fenêtre agrandie, un plein écran, un menu ouvert. Ce n'était pas une coïncidence, c'était la cause.

Et le journal le disait, à qui savait le lire : le compteur des frappes vues par ZyrDesk ne bougeait pas. Pas « refusées », pas « perdues » : **jamais arrivées**. Tout ce qui a été tenté autour du premier plan, du focus et des délais travaillait sur un événement qui n'existait pas.

## Ce que fait le produit aujourd'hui

Un seul propriétaire de ces touches, et c'est **le moteur client**, dans le processus qui reçoit vraiment le clavier. ZyrDesk n'en prend aucune.

Le moteur reçoit le mode `--capture-system-keys zyrdesk`, qui est à nous (patch P-M10) et qui n'est aucun des trois modes d'origine. Ce mode :

- **repose son crochet à chaque fois que le clavier revient à sa fenêtre**, donc il redevient le plus récent de la file aux moments précis où la panne se produisait. C'est la moitié qui compte ;
- décide du **focus de sa propre fenêtre**, jamais du premier plan ;
- ne prend une touche que si **le clavier vient réellement à cette fenêtre**, ce qui demande le focus *et* le premier plan, question posée d'un coup au système ;
- n'avale que **Tab et Échap**. Alt, Control, Majuscule et la touche Windows passent intactes.

Ce qu'il attrape est poussé dans sa file d'événements comme n'importe quelle frappe, donc le chemin qui l'envoie au loin est celui de toutes les autres touches, sans exception à maintenir.

## Trois pièges, et pourquoi ils sont des pièges

**1. Le premier plan n'est pas le focus.** Le premier plan désigne la file d'entrée qui reçoit le clavier ; le focus désigne quelle fenêtre, dans cette file, le reçoit. Ce sont deux questions différentes et elles se répondent différemment.

**2. L'image d'une session ne peut jamais être au premier plan.** Elle est portée comme fenêtre fille de celle de ZyrDesk pour toute la durée d'une session, et le système donne le premier plan au chef de famille, jamais à un enfant. Toute condition de la forme « la fenêtre du moteur est-elle celle du premier plan » répond non pour la session entière. La bibliothèque d'affichage du moteur pose exactement cette question pour décider de son propre focus, ce qui fait qu'elle signale la première perte du clavier et ne peut plus jamais signaler un retour : c'est pour ça que notre correctif lit les deux messages que le système envoie directement à la fenêtre plutôt que de croire la bibliothèque.

**3. Le focus seul ne suffit pas non plus.** ZyrDesk joint son entrée à celle du moteur et rend le focus à l'image à chaque tour de sa veille, ce qui réussit quel que soit le premier plan. Donc le focus seul répond « oui » pendant que quelqu'un travaille dans un autre programme du même ordinateur. Un essai l'a montré : dix-sept Alt+Tab tapés dans une autre fenêtre sont partis à l'ordinateur d'en face.

## Ce qu'il ne faut jamais faire

**Ne jamais avaler Alt.** Tous les raccourcis de ZyrDesk sont des combinaisons Alt, et ils passent par l'enregistrement de combinaisons du système, qui ne voit jamais une touche avalée par un crochet. Le mode `always` du moteur avale Alt et Control en entier : il a cassé tous les raccourcis du produit d'un coup ([D32](DECISIONS.md)).

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
| La capture elle-même | `engines/moonlight-qt/app/streaming/input/zyrsystemkeys.{h,cpp}` |
| Sa mise en marche et son arrêt | `engines/moonlight-qt/app/streaming/input/input.cpp` |
| Le mode en ligne de commande | `engines/moonlight-qt/app/cli/commandlineparser.cpp` |
| Ce qui le demande | `crates/zyr-engine-client/src/command.rs` |
| Le patch au manifeste | P-M10, `patches/MANIFEST.md` |

Il n'y a **rien** côté ZyrDesk. C'est voulu : la deuxième voie a existé, dans `crates/zyr-ui/src/keys.rs`, et elle a été retirée en entier une fois celle-ci validée ([D47](DECISIONS.md)). Deux crochets sur le même clavier, c'est chacun qui répond à l'autre.

## Si ça revient un jour

Le moteur écrit dans son propre journal (`session.log`, la trace du moteur client) une ligne par changement, et un relevé de ce qu'il a fait des touches. Ce qu'on y cherche, dans l'ordre :

1. `zyr: the session has the keyboard` / `has lost the keyboard`. Si le clavier ne revient jamais après une première perte, c'est que le focus est lu au mauvais endroit, et le piège numéro 2 ci-dessus est de retour.
2. Le compte des touches portées à l'ordinateur d'en face contre celui des touches laissées passer. Beaucoup de « laissées passer » avec la session à l'écran veut dire que le crochet n'est plus le premier de la file : regarder si la reposée à chaque retour du clavier fonctionne encore.
3. Dans le journal de la fenêtre, `le premier plan passe ailleurs : processus N (nom.exe)`. Un tiers qui prend le premier plan pendant qu'on tape est une explication ordinaire et pas une panne.

Ce qu'il ne faut pas faire, si le relevé ne dit rien de clair : ajouter un délai, une exception ou un rattrapage. Les quatre lignes du tableau plus haut sont exactement ça, et elles ont coûté une semaine.
