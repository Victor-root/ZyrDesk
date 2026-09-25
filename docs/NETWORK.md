# Architecture réseau

Objectifs : connexion directe prioritaire, relais chiffré en secours avec bascule automatique, chiffrement de bout en bout, latence ajoutée négligeable, zéro configuration réseau pour l'utilisateur.

## 1. Décision structurante : un tunnel unique, même en LAN

Tout le trafic de session (y compris en réseau local) passe par le tunnel ZyrDesk établi entre les deux services `zyrdeskd`. Aucune moitié du moteur n'ouvre de prise réseau : chacune parle au service de sa machine par un tube nommé, et le tunnel porte ce tube d'une machine à l'autre.

```text
lecteur (dans ZyrDesk.exe)                        moteur hôte (zyrdeskd --serve-a-session)
        │ tube du lecteur                                 │ tube du moteur
        ▼                                                 ▼
   zyrdeskd client ── UN SEUL flux UDP chiffré (QUIC) ── zyrdeskd hôte
                          direct OU via relais
```

Pourquoi c'est le bon choix :

- Un seul chemin de code à tester et à optimiser (pas de matrice direct-LAN / direct-WAN / relais).
- Un seul port UDP à ouvrir ou mapper côté hôte pour une session, le 47000 ; le moteur n'a besoin d'aucune règle pare-feu. S'y ajoutent deux ports en entrée qui ne sortent jamais du réseau local : le 5353, que mDNS réserve, et le 47001, sur lequel ce produit répond à qui l'appelle directement quand le multicast ne traverse pas (D19). Les trois règles sont posées par le service lui-même, à chaque démarrage, bornées à son propre programme.
- Chiffrement et authentification uniformes, portés par le tunnel (clés d'appareil), quel que soit le chemin.
- La migration de chemin (relais vers direct) se fait sans que le moteur s'en aperçoive.
- Coût mesuré sur deux vraies machines, et non plus estimé : en Ethernet gigabit, à 40 Mb/s sur deux minutes, le tunnel complet ajoute 0,54 ms d'aller-retour médian et 0,81 ms au centile 99, pour un seuil admis à 1 et 3 ms, et coûte 7,5 points d'un coeur pour un seuil à huit ([perf/baselines/M2-lan-ethernet.md](../perf/baselines/M2-lan-ethernet.md)). L'estimation initiale de 0,1 à 0,5 ms était optimiste d'un facteur deux. La décision du tunnel systématique est donc confirmée par la mesure. Ce relevé date du jalon M2, avec des paquets de 1353 octets et les ports locaux des anciens moteurs : il reste à refaire avec le moteur ZyrDesk et ses tubes.

Le banc de mesure de `zyr-cli bench` fait passer les mêmes paquets, à la même cadence, par une prise UDP nue puis par le tunnel entier, tubes compris : c'est ce qui isole en minutes un problème du tunnel d'un problème du réseau. Il n'apparaît jamais dans l'interface.

L'image et le son voyagent en datagrammes, que le tunnel ne retransmet jamais : leurs pertes sont réparées par la correction d'erreurs du moteur (section 4), et une image irréparable coûte une image clé, jamais un blocage en tête de ligne. Les touches, la souris et les messages du moteur prennent au contraire un flux fiable et ordonné : une touche relâchée ne doit jamais se perdre.

## 2. Transport : QUIC sur quinn

Une session = une connexion QUIC entre les deux services. Ce qui y passe :

| Canal | Nature | Numéro | Contenu |
|---|---|---|---|
| Moteur | Flux fiable, un seul par session | 1 | Le flux de contrôle entre le lecteur et le moteur hôte : présentation, changements en direct, demandes d'image clé, touches et souris, mesure de l'aller-retour, au revoir |
| ZyrDesk | Flux fiable, un par question | 4 | Les questions du produit : ouverture de la session, Ctrl+Alt+Suppr, verrouillage, enceintes, écrans, pointeur, journal, presse-papiers, fichiers |
| Image | Datagrammes, de l'hôte vers le client | 1 | Les morceaux de chaque image, puis leur parité |
| Son | Datagrammes, de l'hôte vers le client | 3 | Tranches Opus de 10 ms, numérotées |

- Un flux s'annonce par un octet en tête, une fois ; un datagramme porte le sien en tête de chaque paquet, `[canal u8][données]`. Les datagrammes gardent le numéro qu'ils ont sur le tube : ils changent de porteur, jamais de nom. Un datagramme d'un seul octet nul, la relance, ne porte rien : il n'existe que pour être accusé, au moment où une route revient (D170).
- Seul l'ordinateur qui regarde ouvre des flux. Ce que le moteur hôte a à dire au lecteur passe par le flux du moteur, que le lecteur ouvre dès qu'il est sur son tube.
- Le canal ZyrDesk porte la première parole de chaque session : la question d'ouverture, avec le débit et la cadence voulus. L'ordinateur regardé règle la fenêtre de son tunnel sur ce débit, lance le moteur de la session et ne répond « ouverte » qu'une fois ce moteur sur son tube ; un refus revient en mots lisibles. Avant cette réponse, un flux du moteur est refusé, et une seconde ouverture sur le même tunnel aussi. Cet échange sert aussi de preuve d'autorisation, pour la raison d'asymétrie décrite plus bas.
- Une question, un stream, un message dans chaque sens, en texte clair à l'intérieur du tunnel chiffré : un canal qui se lit à l'oeil est un canal qui se diagnostique. Chaque message s'ouvre sur le numéro de version du dialecte, 22 aujourd'hui, et les deux bouts doivent parler le même : deux moitiés du produit installées à des dates différentes le disent au lieu de se mécomprendre.
- Le canal ZyrDesk porte aussi le journal de la machine d'en face, et l'ordre de le vider (D96). Le premier est le premier message du produit à peser une page et non une ligne. Les deux sens n'ont donc plus la même limite : une question garde la sienne, courte, une réponse a le droit de peser une page. Un plafond protège celui qui écoute de celui qui parle, et cet ordinateur prend des questions de tous ceux qu'il laisse entrer, mais des réponses seulement de la machine où il est allé.
- Il porte le presse-papiers des deux ordinateurs, et c'est la seule question du canal à faire les deux sens à la fois : le même message donne ce qui a été copié d'un côté et redemande ce qui a été copié de l'autre (D179). Un presse-papiers est partagé ou il ne l'est pas, et lequel des deux machines on a copié dessus ne se décide d'aucun côté. Chaque clip porte une empreinte de son contenu, et c'est elle qui s'échange presque tout le temps : un presse-papiers change quelques fois par heure et se lit plusieurs fois par seconde. La question qui porte une page a son propre plafond, levé seulement une fois qu'elle s'est nommée, et la réponse a le même : un presse-papiers partagé dans un seul sens n'en serait pas un.

Ce qui est en place et mesuré :

- Authentification mutuelle par empreinte de certificat épinglée, TLS 1.3 uniquement, protocole annoncé `zyrdesk/1`. Chaque machine a une identité durable, gardée dans `data/identity`, affichée par `zyr-cli identity`.
- Contrôleur de congestion média (section 3), file d'émission de datagrammes taillée sur ce que chaque bout envoie (un mégaoctet du côté regardé, 32 Kio du côté qui regarde), file de réception de 8 Mio, expiration d'inactivité à 30 s, maintien de correspondance toutes les 5 s. Ces trente secondes sont la patience du produit entier, écrite une seule fois dans `zyr-proto` : le tunnel les tient, et le moteur hôte relâche tout ce qui est enfoncé après le même silence du lecteur (D138).
- Taille des datagrammes prise sur ce que le chemin garantit, sans attendre (section 4).
- Côté client, ce qui sort du tunnel attend dans une file de 4 Mio avant le tube du lecteur : la lecture de la connexion n'attend jamais le lecteur, et s'il prend du retard, le plus ancien part d'abord.
- Ce qu'un bout dit en partant traverse le tunnel avant qu'il ne se ferme, deux secondes au plus : l'au revoir du lecteur atteint le moteur, et celui du moteur le lecteur, ce qui dit à chacun comment la session a fini.
- Aucun datagramme n'est jeté sans compteur (trop gros pour le chemin, chassé d'une file pleine, arrivé avant le lecteur, illisible), et les deux bouts disent dans leur journal, pendant la session, ce qu'ils ont jeté et combien la route a mis.

Asymétrie du protocole à connaître : le client présente son certificat en dernier et l'hôte ne le juge qu'ensuite. Un client refusé voit donc sa connexion réussir, puis se rompre aussitôt. L'interface ne doit jamais annoncer une session établie avant le premier échange réussi, la réponse à la question d'ouverture.

Le choix de bibliothèque, et la date à laquelle il est réexaminé, sont consignés en D13 dans [DECISIONS.md](DECISIONS.md). Seul le crate `zyr-transport` nomme la bibliothèque de transport ; tout le reste ne connaît que la connexion, ses flux et ses datagrammes.

## 3. Le point dur : neutraliser le contrôle de congestion pour le média

Problème identifié (et disqualifiant si ignoré) : les datagrammes QUIC ne sont pas retransmis, mais ils SONT soumis à la fenêtre de congestion de la connexion. Or un contrôle de congestion classique fondé sur la perte s'effondre : à 1 % de perte et 25 ms d'aller-retour, il converge vers environ 5 Mb/s, alors qu'un flux 1080p60 confortable en veut 30 à 40. Résultat avec les réglages par défaut : vidéo étranglée ou file d'attente qui gonfle en secondes de latence. Inacceptable.

Fait au jalon M2, et tenu depuis :

- Contrôleur de congestion média sur mesure : fenêtre = deux fois ce que le flux de la session produit pendant toute la limite d'inactivité du transport, trente secondes ; les signaux de perte ne la réduisent jamais. Une fenêtre est ce qui peut être en vol sans réponse, et rien ne part au-delà ; or une connexion vit trente secondes sans rien recevoir avant de se déclarer morte, et tout ce qui est parti pendant ce temps est en vol. Une fenêtre plus courte que cela, une demi-seconde de flux jusqu'au 4 septembre, se remplit au premier silence un peu long, et le transport n'envoie plus alors que ses propres sondes, qu'il espace en doublant à chaque fois : sept secondes dans un silence, la sonde suivante est à cinq secondes, et le retour de la route ne change rien tant qu'elle n'est pas partie et revenue. Un silence que le tunnel aurait passé devenait ainsi un silence que la session ne passait pas (D136). Ne pas réagir aux pertes serait déraisonnable pour un flux capable de saturer un lien ; ce n'est pas le cas ici, le débit est fixé par l'encodeur et ne dépasse jamais sa consigne. La fenêtre ne sert donc pas à émettre davantage, seulement à ne jamais retenir ce que l'encodeur produit déjà. Le facteur deux couvre ce qui voyage à côté de l'image et n'est pas compté dans son débit : la parité que le moteur ajoute à chaque image, le son, les en-têtes, les images clés qui dépassent. Le trafic fiable reste minuscule et ne peut pas être affamé.
- Une fenêtre de cette taille neutralise aussi le lissage d'émission : chaque image part en rafale de plusieurs dizaines de paquets ; un lisseur les étalerait, ajoutant une gigue régulière que le lecteur devrait ensuite absorber.
- Le débit retenu est celui de la session en cours, et il change avec elle. La fenêtre est recalculée à chaque demande du transport à partir de ce que la porte sert à l'instant : l'ordinateur regardé ouvre son tunnel au démarrage de son service, bien avant qu'une session existe, et tenait sinon la fenêtre d'un débit nominal quel que soit le débit demandé (D134). Il l'apprend du premier mot de la session, la question d'ouverture, puis de ce que le moteur hôte dit servir après chaque démarrage ou changement de son encodeur, et revient au débit nominal quand la dernière session se ferme. Sa branche de relais partage la même mesure, puisque c'est elle qui porte la vidéo quand la route est relayée. L'ordinateur qui regarde, lui, n'envoie aucun flux : sa fenêtre est celle du flux le plus rapide que le produit propose, et rien de ce qu'il envoie ne s'en approche (D136).
- La file d'émission des datagrammes est taillée sur ce que **ce bout-là** envoie, et les deux bouts ne sont pas semblables : l'un envoie une image, l'autre une main (D135). L'ordinateur qui regarde prend 32 Kio : il n'envoie aucune image, et ses touches et sa souris voyagent sur le flux fiable du moteur, pas en datagrammes. La branche de relais suit le rôle de sa machine.
- Du côté regardé, la file tient six images du débit le plus haut que le produit propose, soit un mégaoctet. Sous congestion, on JETTE le périmé (la correction d'erreurs du moteur l'absorbe, ou une image clé) au lieu d'empiler de la latence, mais jamais au point de couper une image en deux. Une image part d'un bloc et la pompe la pousse dans la file bien plus vite que le transport ne la met sur le fil : une file plus courte qu'une image perd des paquets de chaque image clé sur le meilleur des réseaux, et cette perte-là ne se répare pas (D125). La file ne peut pas suivre la session comme la fenêtre : le transport la fixe à la création de la connexion et ne la rouvre jamais, alors que le débit, lui, bouge (D134). Depuis `quinn-proto` 0.11.18, elle compte trente-deux octets de plus par paquet, et la vidéo y tient environ 3 % de moins (D221).
- Garde-fou permanent : un test compare le contrôleur média au contrôleur ordinaire du transport sous une série de pertes ; le second tombe sous la fenêtre nécessaire à 40 Mb/s et 25 ms, le premier non. Le banc sait par ailleurs provoquer une perte réelle sous le transport (`--loss`, en pour mille), ce qui exerce ses vrais mécanismes de détection.

Mesuré en boucle locale, version release, 40 Mb/s pendant 6 s : à 1 % de perte provoquée, 0,98 % constaté bout en bout et 39,7 Mb/s tenus ; à 2 %, 1,95 % constaté et 39,7 Mb/s tenus. Aucune amplification, aucun effondrement.

Reste à faire :

- Simulation d'aller-retour dans le banc : c'est le produit perte x aller-retour qui fait s'effondrer un contrôleur ordinaire, et la boucle locale n'a que 0,15 ms. La condition exacte de G-loss (25 ms, 10 minutes) se mesurera sur un vrai chemin au jalon M5.
- Fréquence d'acquittements réduite : à plusieurs milliers de paquets par seconde en descente, les acquittements par défaut produisent beaucoup de paquets montants inutiles.
- Priorité temps réel Windows (MMCSS) pour les fils du tunnel dans le service : les fils du moteur la demandent déjà, ceux du service pas encore.
- Vérification du contrôleur actif à chaque établissement de session, et profil de perte joué en intégration continue sur chaque version publiée.

## 4. Budget des datagrammes, taille des paquets et correction d'erreurs

Aucune fragmentation IP, jamais : un seul fragment perdu détruirait le paquet entier, et la latence s'en ressentirait.

Le surcoût du transport n'est pas calculé à la main. Les en-têtes QUIC varient avec la longueur des identifiants de connexion et l'état du chemin ; les estimer reviendrait à refaire, moins bien, un calcul que le transport tient déjà à jour. La taille part de ce que le chemin **garantit**, pas de ce qu'il porte à l'instant : le transport part d'une taille prudente, sonde vers le haut, et retombe au plancher le jour où il décide que le chemin ne porte plus rien de plus grand, et un datagramme taillé sur la mesure du moment serait perdu ce jour-là. Le plancher de QUIC, 1200 octets par paquet, est ce que tout chemin porte par définition, relais compris ; le point d'accès qui passe par l'aiguilleur ne découvre d'ailleurs rien et reste à cette taille toute la connexion, puisque l'aiguilleur change de route sous elle ([SERVER.md](SERVER.md) §4.1).

Sur ce plancher, le transport laisse 1162 octets à un datagramme. Le service en retire l'octet du canal et dit le reste au moteur hôte dès qu'il est sur son tube. Ni marge ni estimation : le moteur écrit lui-même ses en-têtes, et sait exactement ce qu'ils pèsent.

| Élément | Octets |
|---|---|
| Datagramme que le chemin garantit (paquet de 1200 octets, moins ce qu'y met le transport) | 1162 |
| Octet de canal ZyrDesk devant chaque datagramme | 1 |
| Budget dit au moteur hôte | 1161 |
| En-tête vidéo du moteur (flux, numéros d'image et de morceau, tailles, heure de capture, codec) | 28 |
| Morceau d'image le plus grand (arrondi au pair, ce que la correction d'erreurs exige) | 1132 |

La taille des morceaux suit l'image. Une grande image est répartie en parts égales sur aussi peu de datagrammes pleins qu'il en faut, pour qu'aucun paquet ne parte à moitié vide. Une petite image part en datagrammes d'un peu plus de la moitié du budget, jamais moins : des morceaux de 568 octets au moins, soit 597 octets de datagramme sur les 1162.

Ce plancher est ce qui donne à la parité d'une petite image une raison d'exister. Le transport range dans un même paquet les datagrammes qui tiennent ensemble : les deux morceaux d'une petite image, donnée et parité, partaient dans le même paquet, et une seule perte les emportait tous les deux. L'essai de bout en bout en perdait sept sur vingt et une touchées à 5 % de perte, là où des pertes indépendantes en auraient perdu une. Plus long que la moitié du budget, un datagramme ne partage jamais son paquet, et un paquet perdu ne coûte plus qu'un morceau, que la parité remplace ([D223](DECISIONS.md)). Le prix : environ un kilo-octet par petite image, quelques centaines de kilobits par seconde sur un écran immobile.

La correction d'erreurs : à chaque image, le moteur hôte ajoute 20 % de morceaux de parité (Reed-Solomon), au moins un. N'importe quels morceaux, pourvu qu'il y en ait autant que de morceaux de données, reconstruisent l'image, sans rien redemander. Le lecteur attend 3 ms un morceau en retard une fois qu'une image plus récente arrive ; au-delà, l'image est perdue : il garde la dernière image juste et demande une image clé, une seule fois, redemandée au bout d'un quart de seconde si rien ne vient ([MOTEUR.md](MOTEUR.md) §4).

Un datagramme trop gros pour le chemin est refusé par le transport, compté et jeté, jamais fragmenté ; ce compteur ne bouge que si le moteur écrit plus grand que ce qu'on lui a dit. Le budget et la découpe sont couverts par des tests, dans le module `mtu` du transport et dans `zyr-media`.

## 5. Établissement de session et chemins

Au-delà du réseau local, la conception complète est dans [SERVER.md](SERVER.md) §4 ; en résumé, « relais d'abord, direct en parallèle » (zéro attente perçue, leçon Tailscale), sous une couche à nous qui choisit le chemin sans que QUIC le sache :

1. Le broker remet aux deux services un ticket de session signé, et leur fait passer leurs candidats au fur et à mesure : adresses locales, IPv6 globale, IPv4 publique vue par le miroir du serveur, mappage UPnP/NAT-PMP/PCP (crate portmapper), adresse écrite à la main ; et, s'il en a un, l'adresse du relais avec un laissez-passer.
2. La connexion QUIC démarre tout de suite vers une adresse de carte, fictive et stable, que l'aiguilleur du transport traduit vers le chemin élu du moment : le premier chemin direct validé par une sonde signée, sinon le relais dès qu'il est prêt.
3. La perforation se fait des deux côtés en même temps, par les sondes. Une sonde et son écho ont exactement la forme des paquets qui vont les suivre : la taille plancher de QUIC, qui est celle de tous les paquets de ce transport, et la même marque de congestion. Les paquets de la connexion partent aussi de la même adresse que les sondes : celle que le système prend pour la route du moment, jamais celle où la connexion est arrivée la première fois, qui peut appartenir à une autre route, voire à l'autre famille d'adresses (D225). C'est la seule chose qui mesure une route, puisque la connexion QUIC ne sait pas qu'on la change de route sous elle ; plus petite ou plus nue que le trafic, elle disait seulement que le menu fretin passe, et une route qui laissait passer ça en jetant le reste prenait la session et ne lui portait rien, tout en répondant à chaque sonde (D154). Dès qu'un chemin direct répond, l'aiguilleur bascule dessus, et la connexion QUIC ne voit rien changer : mêmes clés, même fenêtre de congestion, aucune reconnexion. Un direct qui meurt revient au relais, gardé chaud toute la session : la branche est rouverte chaque fois qu'elle casse, tant que la session tient la carte (D135). La dernière route, elle, ne s'abandonne jamais : une session sans route élue n'envoie plus rien du tout, sondes et accusés compris, et l'ordinateur d'en face meurt d'une absence en trente secondes. Et tant qu'aucune route ne répond, la session reste sur celle qui la porte : choisir entre deux routes également muettes n'est pas un choix, et l'aller-retour entre elles mange la patience de la session sans rien porter (D139).
4. En réseau local, deux cas, et la différence est réelle. Sans compte, ou quand le serveur ne peut pas présenter la machine d'en face, le direct est établi d'emblée par mDNS et les adresses locales, et le serveur n'est même pas consulté. Avec un compte joignable, deux ordinateurs du même compte passent malgré tout par un rendez-vous, y compris sur le même réseau : le rendez-vous apporte plus de routes qu'une adresse, l'aiguilleur sonde les adresses locales en premier et la session reste bien locale, mais son ouverture, elle, a demandé quelque chose au serveur. La carte d'un ordinateur que ce réseau annonce porte donc un second bouton, une maison, qui ouvre la session par ce réseau et rien d'autre : les adresses d'ici, une seule route, aucun relais, et le compte pas même consulté (D148).
5. L'interface affiche toujours le chemin actif (direct ou relais) et l'aller-retour.

Découverte LAN sans compte : mDNS (crate mdns-sd) annonce et découvre les appareils ZyrDesk du réseau local, chaque annonce portant le nom de la machine et son empreinte. Le service hôte admet les empreintes ainsi annoncées, sous un interrupteur activé par défaut : sur un réseau local, il n'y a donc rien à recopier ni à taper d'un ordinateur à l'autre (D17, [SECURITY.md](SECURITY.md) §1.1). Aucun broker n'est impliqué.

Ce que la découverte suppose, et qui ne dépend pas de nous : le multicast doit traverser le réseau, et Windows doit classer la carte en réseau **privé**. Sur un profil public il coupe la découverte quelles que soient les règles de pare-feu, et un portable en Wi-Fi hérite souvent de ce classement. Une machine portant plusieurs cartes (seconde carte, adaptateur virtuel, VPN) annonce toutes ses adresses : elles sont triées, version 4 d'abord puis par ordre croissant, pour qu'un même ordinateur soit toujours joint au même endroit.

Quand le multicast ne traverse pas, et c'est fréquent entre une carte filaire et une carte sans fil derrière la même box, ZyrDesk appelle au lieu d'attendre d'être entendu : un petit port à lui, UDP 47001, sur lequel chaque service répond à qui l'appelle. Toutes les trois secondes il envoie un datagramme vers l'adresse de diffusion de chaque carte et un vers chaque ordinateur déjà connu, à son adresse ; tant que personne n'a répondu, il passe en plus le réseau adresse par adresse, au plus toutes les trente secondes et seulement jusqu'à 256 adresses (D19). C'est du trafic ordinaire, routé comme une session : un réseau qui porte une session porte cela. Ce qui répond entre dans la même liste que ce qui s'annonce, et une machine qui s'en va le dit avant de partir, plutôt que de laisser sa carte sur l'écran des autres jusqu'à ce qu'ils remarquent son silence.

Rien de tout cela ne se devine de l'extérieur, et le service écrit donc tout dans son journal à chaque démarrage : les adresses de la machine carte par carte, le classement Windows de chaque réseau, les cartes par lesquelles l'annonce sort réellement, celles par lesquelles une question a été reçue et à qui la porte est ouverte. Deux ordinateurs qui ne se voient pas se lisent alors dans un seul collage, sans une commande à taper : c'est ce qui distingue « personne en face », « personne ne nous entend » et « Windows a coupé la découverte ».

Quand le réseau ne laisse rien passer, une empreinte saisie à la main dans la fenêtre remplace l'annonce, sur chacune des deux machines. Elle est écrite dans la liste des appareils admis, que le service relit toutes les cinq secondes : rien à redémarrer, et l'autorisation survit à tout.

## 6. Relais

- Rôle : transporter des paquets chiffrés, rien d'autre. Pas de GPU, pas de décodage, pas d'accès aux clés (le chiffrement est de bout en bout entre les deux appareils ; voir [SECURITY.md](SECURITY.md)). CPU très léger, débit réseau dimensionnant.
- Une connexion QUIC extérieure par session et par appareil, en datagrammes, vers le relais ; chaque datagramme porte un paquet entier du tunnel, que le relais remet tel quel à l'autre bout de la même session. Ni blocage en tête de ligne ni retransmission : une perte vers le relais est une perte, comme sur un chemin direct ([SERVER.md](SERVER.md) §4.5).
- Accès contrôlé par un laissez-passer signé par le broker, qui nomme les deux empreintes d'une session : le relais ne transmet qu'entre elles. Plafond de débit par session relayée, plafond de sessions, compte des octets pour les quotas.
- Auto-hébergeable dès le premier jour, dans le même binaire que le broker (`zyrdesk-server`), débrayable.
- Écoute UDP sur 443 (les réseaux d'entreprise laissent passer QUIC/HTTP3 plus souvent que des ports exotiques). Le même port répond au miroir, qui dit à un appareil son adresse vue de l'extérieur. Repli TCP/TLS : hors périmètre v1, documenté comme limite connue.
- Une branche de relais est une route de l'aiguilleur comme une autre : elle est sondée et mesurée par les mêmes sondes signées, et un chemin direct validé la déloge dès qu'il répond, quel que soit l'aller-retour qu'elle mesure. Elle reste chaude toute la session, pour que le retour au relais, si le direct meurt, ne coûte pas une reconnexion ([SERVER.md](SERVER.md) §4.6).
- La branche a besoin de porter 1200 octets par datagramme, plus son enveloppe : elle part donc du plancher d'IPv6, 1280 octets, et découvre au-dessus. C'est vrai de tout Internet ordinaire ; un chemin qui n'y arrive pas rend le relais inutilisable, et le service le dit plutôt que de tenter (D127).

## 7. Débit et qualité

Le débit est celui que la personne choisit, de 5 à 80 Mb/s. Le moteur hôte en retire la parité et le son avant de le donner à l'encodeur, pour que ce qui part sur le fil reste celui-là, et l'encodeur ne le dépasse pas.

- Tout se change pendant la session sans rien relancer ([D117](DECISIONS.md)) : le lecteur dit au moteur hôte ce qu'il veut désormais ; le débit est appliqué à l'encodeur en place quand il le permet, la taille, la cadence et le codec refont l'encodeur et le décodeur dans la même fenêtre. Le moteur hôte dit ensuite au service ce qu'il sert, et la fenêtre du tunnel suit.
- Les statistiques (images par seconde, temps de l'hôte, du réseau, du décodage et de l'affichage, débit, pertes, latence de bout en bout) restent visibles dans la fiche des statistiques, et deux voyants s'allument sur l'image quand elle se fige, quand des images se perdent en route, ou quand l'un des deux ordinateurs ne suit plus l'encodage ou le décodage.
- Rien n'adapte encore le débit tout seul : le transport ne mesure pas ce que le chemin porte, il tient une fenêtre que rien ne remplit (section 3). Le moteur étant à nous, le tunnel, qui voit le réseau se dégrader, pourra régler lui-même le débit de l'encodeur ([D219](DECISIONS.md)).
- Prévus aussi : une sonde de débit de 2 secondes à travers le tunnel avant la session, pour choisir le départ avec une marge prudente, et un plafond automatique quand la route passe par le relais, qui jette ce qui dépasse le sien (60 Mb/s par défaut, [SERVER.md](SERVER.md) §4.5).

## 8. Ports en clair

- Hôte : UN port UDP entrant pour le tunnel, le 47000, qui porte aussi les sondes et la question au miroir (mappé automatiquement si la box l'accorde ; sinon perforation, sinon relais). C'est tout. S'y ajoutent les deux ports du réseau local, 5353 et 47001, qui n'en sortent jamais.
- Deux interrupteurs d'essai, dans la fenêtre sous « Essais réseau » et dans `preferences.conf`, faits pour comparer deux séances et jamais pour l'usage ordinaire (D137) ; basculés depuis la fenêtre, ils rouvrent la porte de l'ordinateur, ce qui coupe une session ouverte vers lui : `ecn = no` retire des paquets du tunnel et des branches de relais le marquage ECN que QUIC pose dans l'en-tête IP ; `fixed_port = no` fait écouter la porte sur un port choisi par le système à chaque démarrage, que seule une rencontre par le serveur sait nommer, ce qui laisse de côté le réseau local et tout renvoi de port fait à la main. Cet interrupteur-là se lit de loin, et il le faut : d'en face, un ordinateur qui n'écoute pas sur 47000 est indiscernable d'un réseau qui jette les paquets. L'en-tête du journal porte donc une ligne **Tunnel**, relevée sur la prise elle-même et non sur ce qui a été demandé, qui dit le port réellement ouvert et nomme l'interrupteur quand ce n'est pas le nôtre (D149).
- Broker : HTTPS et WSS sortants sur 443 depuis chaque appareil rattaché à un compte ; rien du tout sans lien de compte.
- Relais et miroir : UDP 443 sortant depuis les deux appareils, depuis une prise éphémère pour la branche de relais, depuis la prise du tunnel pour le miroir.
- Moteur : aucun port. Chaque moitié parle au service de sa machine par un tube nommé local, que le réseau ne peut pas atteindre.
