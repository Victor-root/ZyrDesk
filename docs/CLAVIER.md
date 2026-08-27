# Les touches que Windows garde pour lui

Ce document existe pour une raison précise : Alt+Tab a coûté une quinzaine d'allers-retours entre deux ordinateurs avant d'être compris, et rien dans le code ne disait pourquoi. Il est écrit pour que personne, nous compris, ne recommence.

À lire avant de toucher quoi que ce soit qui ressemble à « la session n'envoie pas telle touche à l'ordinateur d'en face ».

## Le problème, tel qu'il se voit

Une session est ouverte, tout marche, Alt+Tab bascule bien entre les fenêtres de l'ordinateur d'en face. Puis on agrandit la fenêtre, ou on passe en plein écran, ou on ouvre le menu du bouton flottant. À partir de là, Alt+Tab ouvre le sélecteur de fenêtres **de l'ordinateur qui regarde** au lieu d'aller au loin, et ça ne revient plus jusqu'à la fin de la session.

Le détail qui trompe : ça ressemble à s'y méprendre à un problème de premier plan. Ce n'en est pas un.

## Le deuxième problème, découvert le 2026-08-27

« La touche Windows n'arrive jamais sur la session. »

Deux moitiés sont nécessaires pour qu'une de ces touches parte au loin : que Windows ne l'attrape pas ici, et que le moteur l'envoie là-bas. Le correctif ci-dessous s'occupait de la première et le moteur refusait la seconde.

Le moteur a une porte, qu'il ferme devant la touche Windows et devant le préfixe Windows d'une combinaison. Elle demande deux choses à sa fenêtre : d'être celle que le système appelle le premier plan, et de tenir la prise clavier de la bibliothèque d'affichage. **Aucune des deux ne peut être vraie chez nous** : notre fenêtre est portée dans celle de ZyrDesk, donc jamais au premier plan (piège numéro 2 ci-dessous), et ce mode laisse exprès la prise de la bibliothèque éteinte parce qu'elle avale Alt et Control en entier. La porte répondait donc non pendant toute la session, et la touche Windows restait ici.

Elle est maintenant posée là où la réponse existe vraiment : le même « le clavier est-il réellement à cette fenêtre » que le crochet utilise pour décider. Tab et Échap ne s'en apercevaient pas, eux ne passent pas par cette porte.

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
- n'avale que **Tab, Échap, la touche Windows et Impr. écran**. Alt, Control et Majuscule passent intactes.

Ce qu'il attrape est poussé dans sa file d'événements comme n'importe quelle frappe, donc le chemin qui l'envoie au loin est celui de toutes les autres touches, sans exception à maintenir.

## L'interrupteur : clavier partagé ou immersif

Prendre ces touches tout le temps est faux dans l'autre sens : la main qui va chercher Alt+Tab veut parfois une fenêtre de cet ordinateur-ci, et la touche Windows veut parfois ce menu Démarrer-là.

C'est donc un interrupteur, dans le menu du bouton flottant, à côté de ceux de la souris et du son. **Clavier : Partagé ou Immersif**, et celui qui est en place est allumé, ce qui est tout l'intérêt : un réglage qui décide où va une touche doit dire où il en est sans qu'on essaie.

En immersif, tout ce que Windows garde d'ordinaire pour lui part dans la session : Alt+Tab, Alt+Maj+Tab, Alt+Échap, Ctrl+Échap, la touche Windows seule et toutes ses combinaisons, la touche Impr. écran, et Alt+F4. Cette dernière n'est pas volée par le système mais par la boîte à outils du moteur, qui ferme sa fenêtre dessus : elle est priée de n'en rien faire tant que l'interrupteur est du côté immersif.

- Il se bascule **sans relancer l'image** : ZyrDesk tape le raccourci du moteur `Ctrl+Alt+Maj+K` dans la fenêtre de l'image, exactement comme il bascule déjà la souris avec `Ctrl+Alt+Maj+M`.
- Il est **retenu** : le côté où on le laisse est celui où la session suivante s'ouvre, ce que la ligne de commande porte en deux valeurs du même mode, `zyrdesk` et `zyrdesk-off`. Ce ne sont pas deux modes : ils ne diffèrent que par le côté de départ.
- Il vaut **Immersif** par défaut. Une session dont la touche Windows ne fait rien sans qu'on sache pourquoi est exactement le défaut que tout ceci répare.

## Les deux qu'aucun logiciel n'aura jamais

**Windows+L et Ctrl+Alt+Suppr ne se prennent pas, quel que soit le côté de l'interrupteur, et aucun produit de bureau à distance ne les a.**

Ce n'est pas une limite de ZyrDesk ni un morceau qui manque. Windows traite ces deux-là dans une partie du système que les crochets ne voient pas, exprès : ce sont les deux gestes qui rendent la main à la personne physiquement assise devant la machine, et un programme qui pourrait les intercepter pourrait faire passer un faux écran de connexion pour le vrai. Le crochet a beau avaler la touche Windows, le système garde son propre compte pour ce cas-là et verrouille quand même.

Symétriquement, ils ne s'envoient pas non plus : le moteur d'en face pose les touches avec le même mécanisme ordinaire, qui ne peut pas plus déclencher Windows+L là-bas qu'ici.

Ctrl+Alt+Suppr a donc sa propre entrée dans le menu, qui ne passe pas par le clavier du tout : elle voyage sur le canal du produit et c'est le service d'en face qui la presse, étant le seul programme de cette machine autorisé à le faire ([D59](DECISIONS.md)). Verrouiller l'ordinateur d'en face se fait par là, ou par le menu Démarrer distant que la touche Windows ouvre maintenant.

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
| La capture elle-même, et la liste des touches prises | `engines/moonlight-qt/app/streaming/input/zyrsystemkeys.{h,cpp}` |
| Sa mise en marche et son arrêt | `engines/moonlight-qt/app/streaming/input/input.cpp` |
| La porte qui laisse partir la touche Windows | `isSystemKeyCaptureActive()`, même fichier |
| Le raccourci qui bascule l'interrupteur | `engines/moonlight-qt/app/streaming/input/keyboard.cpp` |
| Le mode en ligne de commande | `engines/moonlight-qt/app/cli/commandlineparser.cpp` |
| Ce qui le demande | `crates/zyr-engine-client/src/command.rs` |
| L'interrupteur du menu | `crates/zyr-ui/src/floating.rs`, `crates/zyr-ui/web/bouton.{html,js}` |
| Le patch au manifeste | P-M10, `patches/MANIFEST.md` |

Il n'y a **rien** côté ZyrDesk. C'est voulu : la deuxième voie a existé, dans `crates/zyr-ui/src/keys.rs`, et elle a été retirée en entier une fois celle-ci validée ([D47](DECISIONS.md)). Deux crochets sur le même clavier, c'est chacun qui répond à l'autre.

## Si ça revient un jour

Le moteur écrit dans son propre journal (`session.log`, la trace du moteur client) une ligne par changement, et un relevé de ce qu'il a fait des touches. Ce qu'on y cherche, dans l'ordre :

1. `zyr: the session has the keyboard` / `has lost the keyboard`. Si le clavier ne revient jamais après une première perte, c'est que le focus est lu au mauvais endroit, et le piège numéro 2 ci-dessus est de retour.
2. `zyr: the system's keys now go to the session` / `to this computer`. C'est l'interrupteur, et c'est la première chose à regarder : une touche qui ne part pas alors qu'il est du côté « this computer » n'est pas une panne.
3. Le compte des touches portées à l'ordinateur d'en face contre celui des touches laissées passer. `passed: N switch off` est l'interrupteur ; beaucoup de « plain » ou de « without the keyboard » avec la session à l'écran veut dire que le crochet n'est plus le premier de la file : regarder si la reposée à chaque retour du clavier fonctionne encore.
4. Dans le journal de la fenêtre, `le premier plan passe ailleurs : processus N (nom.exe)`. Un tiers qui prend le premier plan pendant qu'on tape est une explication ordinaire et pas une panne.

Ce qu'il ne faut pas faire, si le relevé ne dit rien de clair : ajouter un délai, une exception ou un rattrapage. Les quatre lignes du tableau plus haut sont exactement ça, et elles ont coûté une semaine.
