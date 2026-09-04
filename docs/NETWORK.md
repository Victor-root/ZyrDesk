# Architecture réseau

Objectifs : connexion directe prioritaire, relais chiffré en secours avec bascule automatique, chiffrement de bout en bout, latence ajoutée négligeable, zéro configuration réseau pour l'utilisateur.

## 1. Décision structurante : un tunnel unique, même en LAN

Tout le trafic de session (y compris en réseau local) passe par le tunnel ZyrDesk établi entre les deux services `zyrdeskd`. Les moteurs sont strictement liés au loopback des deux côtés : pour Sunshine et Moonlight, la contrepartie est toujours « locale ».

```text
zyrdesk-session (Moonlight)                                zyrdesk-host-engine (Sunshine)
        │ loopback 127.77.x.y                                      │ loopback 127.0.0.1
        ▼                                                          ▲
   zyrdeskd client ── UN SEUL flux UDP chiffré (QUIC) ──────► zyrdeskd hôte
                          direct OU via relais
```

Pourquoi c'est le bon choix :

- Un seul chemin de code à tester et à optimiser (pas de matrice direct-LAN / direct-WAN / relais).
- Un seul port UDP à ouvrir ou mapper côté hôte pour une session, le 47000 ; les moteurs n'ont besoin d'aucune règle pare-feu. S'y ajoutent deux ports en entrée qui ne sortent jamais du réseau local : le 5353, que mDNS réserve, et le 47001, sur lequel ce produit répond à qui l'appelle directement quand le multicast ne traverse pas (D19). Les trois règles sont posées par le service lui-même, à chaque démarrage, bornées à son propre programme.
- Chiffrement et authentification uniformes, portés par le tunnel (clés d'appareil), quel que soit le chemin.
- La migration de chemin (relais vers direct) devient possible sans que les moteurs s'en aperçoivent.
- Coût mesuré sur deux vraies machines, et non plus estimé : en Ethernet gigabit, à 40 Mb/s sur deux minutes, le tunnel complet ajoute 0,54 ms d'aller-retour médian et 0,81 ms au centile 99, pour un seuil admis à 1 et 3 ms, et coûte 7,5 points d'un coeur pour un seuil à huit ([perf/baselines/M2-lan-ethernet.md](../perf/baselines/M2-lan-ethernet.md)). L'estimation initiale de 0,1 à 0,5 ms était optimiste d'un facteur deux. La décision du tunnel systématique est donc confirmée par la mesure.

Un mode « direct sans tunnel » est conservé UNIQUEMENT comme outil de diagnostic en ligne de commande (`zyr-cli`), pour pouvoir isoler en minutes un problème tunnel d'un problème moteur. Il n'apparaît jamais dans l'interface.

Le protocole GameStream garde ses hypothèses intactes à travers le tunnel : ses datagrammes ne sont jamais retransmis par nous (ses pertes restent gérées par sa correction d'erreur FEC et ses mécanismes de récupération), et la vidéo ne subit aucun blocage tête de ligne puisqu'elle voyage en datagrammes, pas en flux fiable.

## 2. Transport : QUIC sur quinn (état au jalon M2)

Une session = une connexion QUIC entre les deux services :

- Flux fiables : les trois flux TCP du protocole des moteurs (HTTP, HTTPS, RTSP), un flux par connexion interceptée, plus un canal propre à ZyrDesk. Le canal est annoncé par un octet en tête du flux.
- Le canal ZyrDesk porte depuis le jalon M3 la première parole du tunnel : le client demande les ports du moteur d'en face, l'hôte répond. Ce n'est pas une convention qu'on pourrait deviner, le moteur choisissant son port de base au démarrage et annonçant ensuite ses vrais numéros à chaque étape de son protocole ; les ports locaux qui le remplacent côté client doivent donc porter exactement les mêmes. Cet échange sert aussi de preuve d'autorisation, pour la raison d'asymétrie décrite plus bas.
- Depuis le jalon M4, ce même canal porte le code d'appairage des moteurs : le client l'envoie, l'hôte le remet au sien et répond quand c'est fait. C'est ce qui remplace un code affiché sur un écran et tapé sur l'autre (D17). Une question, un stream, un message dans chaque sens, en texte clair à l'intérieur du tunnel chiffré : un canal qui se lit à l'oeil est un canal qui se diagnostique. Chaque message s'ouvre sur le numéro de version du dialecte, de sorte que deux moitiés du produit installées à des dates différentes le disent au lieu de se mécomprendre. Le presse-papiers, les statistiques et la sonde de débit s'y ajouteront aux jalons suivants.
- Toujours au jalon M4, il porte aussi le journal de la machine d'en face, et l'ordre de le vider (D96). Le premier est le premier message du produit à peser une page et non une ligne. Les deux sens n'ont donc plus la même limite : une question garde la sienne, courte, une réponse a le droit de peser une page. Un plafond protège celui qui écoute de celui qui parle, et cet ordinateur prend des questions de tous ceux qu'il laisse entrer, mais des réponses seulement de la machine où il est allé.
- Datagrammes non fiables : vidéo, contrôle temps réel et audio, préfixés du même octet d'identifiant de canal. `[canal u8][données]`.
- L'interface web du moteur hôte n'est délibérément pas transportée : elle reste joignable depuis la seule machine qui l'héberge. Un test le vérifie.

Ce qui est en place et mesuré :

- Authentification mutuelle par empreinte de certificat épinglée, TLS 1.3 uniquement, protocole annoncé `zyrdesk/1`. Chaque machine a une identité durable, gardée dans `data/identity`, affichée par `zyr-cli identity`.
- Contrôleur de congestion média (section 3), file d'émission de datagrammes taillée sur le profil du flux (six images, 256 Kio au minimum), file de réception de 8 Mio, expiration d'inactivité à 30 s, maintien de correspondance toutes les 5 s. Ces trente secondes sont la patience du produit entier, écrite une seule fois dans `zyr-proto` : le tunnel les tient, et le moteur hôte se les voit poser dans sa configuration (`ping_timeout`), parce que c'est le plus court des trois qui décide et que les moteurs en portaient dix (D138). Le moteur client reçoit la même sur une option de sa ligne de commande, son canal de contrôle renonçant lui aussi à dix secondes ; c'est le patch P-M12, et il a demandé un fork de la bibliothèque de protocole que ce moteur enveloppe.
- Découverte de la taille de paquet attendue avant de la figer (section 4), et pas de retard de Nagle sur les flux fiables relayés.
- Un paquet refusé par le système ne coûte que ce paquet. Le moteur n'ouvre ses ports média qu'une fois la négociation finie : tout ce que le tunnel relaie avant n'a personne à qui parler, et Windows le signale par une erreur sur la lecture *suivante* d'une socket par ailleurs saine. La socket demande donc à Windows de se taire là-dessus, comme le font les autres systèmes ; et les pompes comptent ces refus au lieu de s'arrêter, une pompe qui s'arrête emportant toute la session. Sans ces deux points, la négociation RTSP échouait au deuxième message, sans cause visible.

Asymétrie du protocole à connaître : le client présente son certificat en dernier et l'hôte ne le juge qu'ensuite. Un client refusé voit donc sa connexion réussir, puis se rompre aussitôt. L'interface ne doit jamais annoncer une session établie avant le premier échange réussi.

Le choix de bibliothèque, et la date à laquelle il est réexaminé, sont consignés en D13 dans [DECISIONS.md](DECISIONS.md). Un seul fichier du produit nomme la bibliothèque de transport : `crates/zyr-transport/src/endpoint.rs`. Tout le reste ne connaît que la connexion et les deux types de flux qu'il expose.

## 3. Le point dur : neutraliser le contrôle de congestion pour le média

Problème identifié (et disqualifiant si ignoré) : les datagrammes QUIC ne sont pas retransmis, mais ils SONT soumis à la fenêtre de congestion de la connexion. Or un contrôle de congestion classique fondé sur la perte s'effondre : à 1 % de perte et 25 ms d'aller-retour, il converge vers environ 5 Mb/s, alors qu'un flux 1080p60 confortable en veut 30 à 40. Résultat avec les réglages par défaut : vidéo étranglée ou file d'attente qui gonfle en secondes de latence. Inacceptable.

Fait au jalon M2 :

- Contrôleur de congestion média sur mesure : fenêtre = deux fois ce que le flux de la session produit pendant toute la limite d'inactivité du transport, trente secondes ; les signaux de perte ne la réduisent jamais. Une fenêtre est ce qui peut être en vol sans réponse, et rien ne part au-delà ; or une connexion vit trente secondes sans rien recevoir avant de se déclarer morte, et tout ce qui est parti pendant ce temps est en vol. Une fenêtre plus courte que cela, une demi-seconde de flux jusqu'au 4 septembre, se remplit au premier silence un peu long, et le transport n'envoie plus alors que ses propres sondes, qu'il espace en doublant à chaque fois : sept secondes dans un silence, la sonde suivante est à cinq secondes, et le retour de la route ne change rien tant qu'elle n'est pas partie et revenue. Un silence que le tunnel aurait passé devenait ainsi un silence que les moteurs ne passent pas, le canal de contrôle du moteur client renonçant à dix secondes (D136). Ne pas réagir aux pertes serait déraisonnable pour un flux capable de saturer un lien ; ce n'est pas le cas ici, le débit est fixé par l'encodeur et ne dépasse jamais sa consigne. La fenêtre ne sert donc pas à émettre davantage, seulement à ne jamais retenir ce que l'encodeur produit déjà. Le facteur deux couvre ce qui voyage à côté de l'image et n'est pas compté dans son débit : la correction d'erreurs que le moteur ajoute à chaque image, le son, les en-têtes, les images clés qui dépassent. Le trafic fiable reste minuscule et ne peut pas être affamé.
- Une fenêtre de cette taille neutralise aussi le lissage d'émission : chaque image part en rafale de plusieurs dizaines de paquets ; un lisseur les étalerait, ajoutant une gigue régulière que la régulation d'affichage du client devrait ensuite absorber.
- Le débit retenu est celui de la session en cours, et il change avec elle. La fenêtre est recalculée à chaque demande du transport à partir de ce que la porte sert à l'instant : l'ordinateur regardé ouvre son tunnel au démarrage de son service, bien avant qu'une session existe, et tenait sinon la fenêtre d'un débit nominal quel que soit le débit demandé (D134). Il l'apprend du premier mot de la session, la question des ports, et de la demande de changement de débit en cours de route. Sa branche de relais partage la même mesure, puisque c'est elle qui porte la vidéo quand la route est relayée. L'ordinateur qui regarde, lui, n'envoie aucun flux : sa fenêtre est celle du flux le plus rapide que le produit propose, et rien de ce qu'il envoie ne s'en approche, pas même le canal de contrôle de son moteur renvoyant tout ce qui n'est pas accusé pendant que la route se tait (D136).
- La file d'émission est taillée sur ce que **ce bout-là** envoie, et les deux bouts ne sont pas semblables : l'un envoie une image, l'autre une main. L'ordinateur regardé prend six images du débit le plus haut ; celui qui regarde prend 32 Kio, parce qu'il n'encode rien et que le canal portant ses entrées est fiable : une file plus profonde que la patience de ce canal transforme chaque perte en renvoi, et le renvoi retombe dans la file qui l'a jeté (D135). La branche de relais suit le rôle de sa machine.
- File d'émission du côté regardé, taillée sur le flux le plus rapide que le produit propose : sous congestion, on JETTE le périmé (la correction d'erreur du protocole l'absorbe) au lieu d'empiler de la latence, mais jamais au point de couper une image en deux. Une image part d'un bloc et la pompe la pousse dans la file bien plus vite que le transport ne la met sur le fil : une file plus courte qu'une image perd des paquets de chaque image clé sur le meilleur des réseaux, et cette perte-là ne se répare pas (D125). Six images du débit le plus haut, soit un mégaoctet. Elle ne peut pas suivre la session comme la fenêtre : le transport la fixe à la création de la connexion et ne la rouvre jamais, alors que le débit, lui, bouge (D134).
- Garde-fou permanent : un test compare le contrôleur média au contrôleur ordinaire du transport sous une série de pertes ; le second tombe sous la fenêtre nécessaire à 40 Mb/s et 25 ms, le premier non. Le banc sait par ailleurs provoquer une perte réelle sous le transport (`--loss`, en pour mille), ce qui exerce ses vrais mécanismes de détection.

Mesuré en boucle locale, version release, 40 Mb/s pendant 6 s : à 1 % de perte provoquée, 0,98 % constaté bout en bout et 39,7 Mb/s tenus ; à 2 %, 1,95 % constaté et 39,7 Mb/s tenus. Aucune amplification, aucun effondrement.

Reste à faire :

- Simulation d'aller-retour dans le banc : c'est le produit perte x aller-retour qui fait s'effondrer un contrôleur ordinaire, et la boucle locale n'a que 0,15 ms. La condition exacte de G-loss (25 ms, 10 minutes) se mesurera sur un vrai chemin au jalon M5.
- Fréquence d'acquittements réduite : à plusieurs milliers de paquets par seconde en descente, les acquittements par défaut produisent beaucoup de paquets montants inutiles.
- Priorité temps réel Windows (MMCSS), à faire avec le service du jalon M3. Les tampons des sockets qui relient le moteur au tunnel sont en revanche déjà portés à quatre mébioctets : leur valeur par défaut, souvent 64 Kio, ne couvrait qu'une dizaine de millisecondes de vidéo, et le noyau y jetait des paquets sans que rien ne puisse le compter.
- Vérification du contrôleur actif à chaque établissement de session, et profil de perte joué en intégration continue sur chaque version publiée.

## 4. Budget MTU et taille de paquet

Aucune fragmentation IP, jamais : un seul fragment perdu détruirait le paquet entier, et la latence s'en ressentirait.

Le surcoût du transport n'est pas calculé à la main. Les en-têtes QUIC varient avec la longueur des identifiants de connexion et l'état du chemin ; les estimer reviendrait à refaire, moins bien, un calcul que le transport tient déjà à jour et corrige au fil de sa découverte de MTU. La taille de paquet part donc de la charge utile que le transport annonce pour le chemin en cours.

| Élément retranché | Octets |
|---|---|
| En-tête ZyrDesk devant chaque datagramme (identifiant de canal) | 1 |
| En-têtes ajoutés par le protocole des moteurs à chaque paquet vidéo | 28, à confirmer par mesure |
| Marge conservée tant que l'en-tête réel n'est pas mesuré | 32 |

La valeur obtenue est plafonnée à 1392, celle qu'emploie nativement le moteur client en réseau local : aller au-delà n'apporte rien et rapproche du seuil de fragmentation. Elle est plancher à 1025, minimum accepté par le moteur client ; rester au-dessus garde sa détection de réseau distant désactivée, puisque c'est nous qui gérons le chemin. Un chemin trop étroit pour ce plancher est refusé plutôt que raboté en silence.

Sur un chemin Ethernet ordinaire, le résultat dépasse 1300 octets. Le calcul est implémenté et couvert par des tests dans le module `mtu` du transport, y compris la propriété qui compte : la taille rendue tient toujours dans le datagramme annoncé.

Le moment où on interroge le transport compte autant que le calcul. Il part d'une taille prudente et sonde vers le haut ; l'interroger dès la connexion établie donnait 1101 octets là où le chemin en permettait 1353. Le moteur aurait gardé cette valeur pour toute la session, puisqu'il ne sait pas en changer en cours de route. La taille n'est donc figée qu'une fois la découverte stabilisée, ce qui coûte quelques centaines de millisecondes au démarrage de session.

Les deux estimations du tableau se resserreront une fois la taille réelle des en-têtes mesurée par capture réseau (vérification V5 du jalon M1).

## 5. Établissement de session et chemins

Au-delà du réseau local, la conception complète est dans [SERVER.md](SERVER.md) §4 ; en résumé, « relais d'abord, direct en parallèle » (zéro attente perçue, leçon Tailscale), sous une couche à nous qui choisit le chemin sans que QUIC le sache :

1. Le broker remet aux deux services un ticket de session signé, et leur fait passer leurs candidats au fur et à mesure : adresses locales, IPv6 globale, IPv4 publique vue par le miroir du serveur, mappage UPnP/NAT-PMP/PCP (crate portmapper), adresse écrite à la main ; et, s'il en a un, l'adresse du relais avec un laissez-passer.
2. La connexion QUIC démarre tout de suite vers une adresse de carte, fictive et stable, que l'aiguilleur du transport traduit vers le chemin élu du moment : le premier chemin direct validé par une sonde signée, sinon le relais dès qu'il est prêt.
3. La perforation se fait des deux côtés en même temps, par les sondes ; dès qu'un chemin direct répond, l'aiguilleur bascule dessus, et la connexion QUIC ne voit rien changer : mêmes clés, même fenêtre de congestion, aucune reconnexion. Un direct qui meurt revient au relais, gardé chaud toute la session : la branche est rouverte chaque fois qu'elle casse, tant que la session tient la carte (D135). La dernière route, elle, ne s'abandonne jamais : une session sans route élue n'envoie plus rien du tout, sondes et accusés compris, et l'ordinateur d'en face meurt d'une absence en trente secondes. Et tant qu'aucune route ne répond, la session reste sur celle qui la porte : choisir entre deux routes également muettes n'est pas un choix, et l'aller-retour entre elles mange la patience du moteur sans rien porter (D139).
4. En réseau local, le direct est établi d'emblée (mDNS et adresses locales) ; le serveur n'est même pas consulté.
5. L'interface affiche toujours le chemin actif (direct ou relais) et l'aller-retour.

Découverte LAN sans compte : mDNS (crate mdns-sd) annonce et découvre les appareils ZyrDesk du réseau local, chaque annonce portant le nom de la machine et son empreinte. Depuis le jalon M4, le service hôte admet les empreintes ainsi annoncées, sous un interrupteur activé par défaut, et le code d'appairage des moteurs voyage dans le tunnel : sur un réseau local, il n'y a donc rien à recopier ni à taper d'un ordinateur à l'autre (D17, [SECURITY.md](SECURITY.md) §1.1). Aucun broker n'est impliqué.

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

## 7. Débit et qualité (pas de bitrate adaptatif dans GameStream)

Le protocole fixe le débit vidéo au lancement de la session ; il ne s'adapte pas en cours de route (certaines solutions commerciales concurrentes le font). Stratégie v1, honnête et simple :

- Sonde de débit de 2 secondes à travers le tunnel avant la session : choisit le préréglage de départ (débit, résolution) avec une marge prudente.
- Changement de débit, de taille ou de codec pendant la session sans rien relancer (D117) : le débit est demandé au moteur d'en face là où il est, la taille et le codec font refaire son flux au lecteur dans sa propre fenêtre.
- Plafond de débit automatique en mode relais.
- Les statistiques (pertes, jitter, latence) restent visibles ; si le lien se dégrade nettement, l'interface propose de baisser la qualité.
- Plus tard : renégociation plus fine, voire boucle de retour entre les métriques tunnel et l'encodeur (transport et encodeur gagnent à dialoguer directement plutôt qu'en couches strictement séparées).

## 8. Ports en clair

- Hôte : UN port UDP entrant pour le tunnel, le 47000, qui porte aussi les sondes et la question au miroir (mappé automatiquement si la box l'accorde ; sinon perforation, sinon relais). C'est tout. S'y ajoutent les deux ports du réseau local, 5353 et 47001, qui n'en sortent jamais.
- Deux interrupteurs d'essai, dans la fenêtre sous « Essais réseau » et dans `preferences.conf`, faits pour comparer deux séances et jamais pour l'usage ordinaire (D137) ; basculés depuis la fenêtre, ils rouvrent la porte de l'ordinateur, ce qui coupe une session ouverte vers lui : `ecn = no` retire des paquets du tunnel et des branches de relais le marquage ECN que QUIC pose dans l'en-tête IP ; `fixed_port = no` fait écouter la porte sur un port choisi par le système à chaque démarrage, que seule une rencontre par le serveur sait nommer, ce qui laisse de côté le réseau local et tout renvoi de port fait à la main.
- Broker : HTTPS et WSS sortants sur 443 depuis chaque appareil rattaché à un compte ; rien du tout sans lien de compte.
- Relais et miroir : UDP 443 sortant depuis les deux appareils, depuis une prise éphémère pour la branche de relais, depuis la prise du tunnel pour le miroir.
- Moteurs : loopback uniquement (base 42000 à 42999, offsets GameStream standard), invisibles du réseau.
