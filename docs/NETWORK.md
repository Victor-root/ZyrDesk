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
- Un seul port UDP à ouvrir ou mapper côté hôte ; les moteurs n'ont besoin d'aucune règle pare-feu.
- Chiffrement et authentification uniformes, portés par le tunnel (clés d'appareil), quel que soit le chemin.
- La migration de chemin (relais vers direct) devient possible sans que les moteurs s'en aperçoivent.
- Coût mesuré sur deux vraies machines, et non plus estimé : en Ethernet gigabit, à 40 Mb/s sur deux minutes, le tunnel complet ajoute 0,54 ms d'aller-retour médian et 0,81 ms au centile 99, pour un seuil admis à 1 et 3 ms, et coûte 7,5 points d'un coeur pour un seuil à huit ([perf/baselines/M2-lan-ethernet.md](../perf/baselines/M2-lan-ethernet.md)). L'estimation initiale de 0,1 à 0,5 ms était optimiste d'un facteur deux. La décision du tunnel systématique est donc confirmée par la mesure.

Un mode « direct sans tunnel » est conservé UNIQUEMENT comme outil de diagnostic en ligne de commande (`zyr-cli`), pour pouvoir isoler en minutes un problème tunnel d'un problème moteur. Il n'apparaît jamais dans l'interface.

Le protocole GameStream garde ses hypothèses intactes à travers le tunnel : ses datagrammes ne sont jamais retransmis par nous (ses pertes restent gérées par sa correction d'erreur FEC et ses mécanismes de récupération), et la vidéo ne subit aucun blocage tête de ligne puisqu'elle voyage en datagrammes, pas en flux fiable.

## 2. Transport : QUIC sur quinn (état au jalon M2)

Une session = une connexion QUIC entre les deux services :

- Flux fiables : les trois flux TCP du protocole des moteurs (HTTP, HTTPS, RTSP), un flux par connexion interceptée, plus un canal propre à ZyrDesk (versions, code d'appairage, presse-papiers, statistiques, sonde de débit) qui sera rempli aux jalons suivants. Le canal est annoncé par un octet en tête du flux.
- Datagrammes non fiables : vidéo, contrôle temps réel et audio, préfixés du même octet d'identifiant de canal. `[canal u8][données]`.
- L'interface web du moteur hôte n'est délibérément pas transportée : elle reste joignable depuis la seule machine qui l'héberge. Un test le vérifie.

Ce qui est en place et mesuré :

- Authentification mutuelle par empreinte de certificat épinglée, TLS 1.3 uniquement, protocole annoncé `zyrdesk/1`. Chaque machine a une identité durable, gardée dans `data/identity`, affichée par `zyr-cli identity`.
- Contrôleur de congestion média (section 3), file d'émission de datagrammes de 128 Kio, file de réception de 8 Mio, expiration d'inactivité à 30 s, maintien de correspondance toutes les 5 s.
- Découverte de la taille de paquet attendue avant de la figer (section 4), et pas de retard de Nagle sur les flux fiables relayés.

Asymétrie du protocole à connaître : le client présente son certificat en dernier et l'hôte ne le juge qu'ensuite. Un client refusé voit donc sa connexion réussir, puis se rompre aussitôt. L'interface ne doit jamais annoncer une session établie avant le premier échange réussi.

Le choix de bibliothèque, et la date à laquelle il est réexaminé, sont consignés en D13 dans [DECISIONS.md](DECISIONS.md). Un seul fichier du produit nomme la bibliothèque de transport : `crates/zyr-transport/src/point.rs`. Tout le reste ne connaît que la connexion et les deux types de flux qu'il expose.

## 3. Le point dur : neutraliser le contrôle de congestion pour le média

Problème identifié (et disqualifiant si ignoré) : les datagrammes QUIC ne sont pas retransmis, mais ils SONT soumis à la fenêtre de congestion de la connexion. Or un contrôle de congestion classique fondé sur la perte s'effondre : à 1 % de perte et 25 ms d'aller-retour, il converge vers environ 5 Mb/s, alors qu'un flux 1080p60 confortable en veut 30 à 40. Résultat avec les réglages par défaut : vidéo étranglée ou file d'attente qui gonfle en secondes de latence. Inacceptable.

Fait au jalon M2 :

- Contrôleur de congestion média sur mesure : fenêtre = 2 x débit de session x aller-retour + une image entière ; les signaux de perte ne la réduisent jamais. Ne pas réagir aux pertes serait déraisonnable pour un flux capable de saturer un lien ; ce n'est pas le cas ici, le débit est fixé par l'encodeur et ne dépasse jamais sa consigne. La fenêtre ne sert donc pas à émettre davantage, seulement à ne pas bloquer ce que l'encodeur produit déjà. Le trafic fiable reste minuscule et ne peut pas être affamé. L'aller-retour retenu est plafonné à 500 ms, pour qu'une mesure aberrante ne produise pas une fenêtre absurde.
- Ce plancher neutralise aussi le lissage d'émission : chaque image part en rafale de plusieurs dizaines de paquets ; un lisseur les étalerait, ajoutant une gigue régulière que la régulation d'affichage du client devrait ensuite absorber.
- File d'émission de datagrammes courte (128 Kio) : sous congestion, on JETTE le périmé (la correction d'erreur du protocole l'absorbe) au lieu d'empiler de la latence.
- Garde-fou permanent : un test compare le contrôleur média au contrôleur ordinaire du transport sous une série de pertes ; le second tombe sous la fenêtre nécessaire à 40 Mb/s et 25 ms, le premier non. Le banc sait par ailleurs provoquer une perte réelle sous le transport (`--perte`, en pour mille), ce qui exerce ses vrais mécanismes de détection.

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

Modèle « relais d'abord, direct en parallèle » (zéro attente perçue, leçon Tailscale ; Valve montre même qu'un bon relais bat parfois le chemin direct) :

1. Le broker remet aux deux services le ticket de session + les candidats : adresses locales (LAN), IPv6 globale, IPv4 publique observée, mappages UPnP/NAT-PMP/PCP (crate portmapper), relais assigné.
2. La connexion démarre immédiatement par le meilleur chemin disponible (souvent le relais, parfois le LAN découvert par mDNS).
3. En parallèle : perforation NAT (hole punching) sur les candidats directs. Dès qu'un chemin direct fonctionne, la connexion MIGRE dessus sans interruption (iroh) ou se rétablit en ~2 s (plan B).
4. En LAN pur, le direct est établi d'emblée (mDNS + adresses locales) ; le relais n'est même pas contacté.
5. L'interface affiche toujours le chemin actif (Direct ou Relais) et la latence.

Découverte LAN sans compte : mDNS (crate mdns-sd) annonce et découvre les appareils ZyrDesk du réseau local ; l'appairage local suit le même mécanisme de PIN par tunnel, avec confirmation visuelle côté hôte (pas de broker impliqué).

## 6. Relais

- Rôle : transporter des paquets chiffrés, rien d'autre. Pas de GPU, pas de décodage, pas d'accès aux clés (le chiffrement est de bout en bout entre les deux appareils ; voir [SECURITY.md](SECURITY.md)). CPU très léger, débit réseau dimensionnant.
- Accès contrôlé par jetons émis par le broker (session autorisée), avec quotas par compte et plafond de débit en mode relais (protection contre l'abus du service hébergé).
- Auto-hébergeable dès le premier jour (même binaire ou conteneur, documentation fournie) : un utilisateur peut pointer son ZyrDesk vers son propre broker + relais.
- Écoute UDP sur 443 (les réseaux d'entreprise laissent passer QUIC/HTTP3 plus souvent que des ports exotiques). Repli TCP/TLS : hors périmètre v1, documenté comme limite connue.

## 7. Débit et qualité (pas de bitrate adaptatif dans GameStream)

Le protocole fixe le débit vidéo au lancement de la session ; il ne s'adapte pas en cours de route (certaines solutions commerciales concurrentes le font). Stratégie v1, honnête et simple :

- Sonde de débit de 2 secondes à travers le tunnel avant la session : choisit le préréglage de départ (débit, résolution) avec une marge prudente.
- Changement de qualité en un clic pendant la session = redémarrage rapide du flux (~2 à 3 s), en conservant fenêtre et tunnel.
- Plafond de débit automatique en mode relais.
- Les statistiques (pertes, jitter, latence) restent visibles ; si le lien se dégrade nettement, l'interface propose de baisser la qualité.
- Plus tard : renégociation plus fine, voire boucle de retour entre les métriques tunnel et l'encodeur (transport et encodeur gagnent à dialoguer directement plutôt qu'en couches strictement séparées).

## 8. Ports en clair

- Hôte : UN port UDP entrant pour le tunnel (mappé automatiquement si possible ; sinon relais). C'est tout.
- Broker : WSS sortant sur 443 depuis chaque appareil.
- Relais : UDP 443 sortant depuis les deux appareils.
- Moteurs : loopback uniquement (base 42000 à 42999, offsets GameStream standard), invisibles du réseau.
