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
- Coût mesurable minime : à 1440p60 et 80 Mb/s, environ 9500 paquets/s ; le double saut loopback + chiffrement AES ajoute typiquement 0,1 à 0,5 ms et quelques pour cent d'un cœur CPU. Validé ou invalidé par les seuils du jalon M2 (voir [ROADMAP.md](ROADMAP.md)) ; en cas d'échec des seuils, la décision est révisée.

Un mode « direct sans tunnel » est conservé UNIQUEMENT comme outil de diagnostic en ligne de commande (`zyr-cli`), pour pouvoir isoler en minutes un problème tunnel d'un problème moteur. Il n'apparaît jamais dans l'interface.

Le protocole GameStream garde ses hypothèses intactes à travers le tunnel : ses datagrammes ne sont jamais retransmis par nous (ses pertes restent gérées par sa correction d'erreur FEC et ses mécanismes de récupération), et la vidéo ne subit aucun blocage tête de ligne puisqu'elle voyage en datagrammes, pas en flux fiable.

## 2. Transport : QUIC, avec iroh en pari principal

Une session = une connexion QUIC entre les deux services :

- Streams fiables : canal de contrôle ZyrDesk (stream 0 : versions, carte des ports, PIN d'appairage, presse-papiers, statistiques, sonde de débit) + les trois flux TCP GameStream (HTTP, HTTPS, RTSP), un stream par connexion TCP interceptée.
- Datagrammes non fiables : vidéo, contrôle temps réel (ENet) et audio, préfixés d'un octet d'identifiant de canal. `[canal u8][données]`.

Choix d'implémentation :

- Pari principal : iroh (fork de quinn maintenu en production depuis mars 2026) parce qu'il intègre exactement nos besoins : traversée NAT de qualité production, relais traité comme un chemin QUIC de première classe, MIGRATION transparente relais vers direct en cours de session, découverte d'adresses. C'est le modèle Tailscale (démarrer par le relais, promouvoir en direct en parallèle) déjà outillé.
- Plan B assumé et chiffré (2 à 3 semaines) : quinn pur + rendez-vous et échange de candidats via notre broker + relais UDP minimal maison. On y perd la migration transparente : la promotion relais vers direct se fait alors par une reconnexion d'environ 2 secondes, masquée par la reprise automatique.
- Dans les deux cas, le transport est confiné derrière un trait Rust `ZyrTransport` (connexion par identité d'appareil, ouverture de streams, envoi/réception de datagrammes, taille maximale de datagramme, événements de chemin), version épinglée exactement, mise à niveau uniquement dans des fenêtres dédiées.

## 3. Le point dur : neutraliser le contrôle de congestion pour le média

Problème identifié (et disqualifiant si ignoré) : les datagrammes QUIC ne sont pas retransmis, mais ils SONT soumis à la fenêtre de congestion de la connexion. Or un contrôle de congestion classique fondé sur la perte s'effondre : à 1 % de perte et 25 ms d'aller-retour, il converge vers environ 5 Mb/s, alors qu'un flux 1080p60 confortable en veut 30 à 40. Résultat avec les réglages par défaut : vidéo étranglée ou file d'attente qui gonfle en secondes de latence. Inacceptable.

Solution (critère GO/NO-GO du banc M2) :

- Contrôleur de congestion média sur mesure (l'interface de contrôleur est publique dans quinn ; sa disponibilité dans iroh est justement ce que le banc valide) : plancher de fenêtre = 2 x débit de session x RTT + marge de rafale ; les signaux de perte ne peuvent pas descendre sous ce plancher. Le trafic fiable (RTSP, contrôle) reste minuscule et ne peut pas être affamé.
- Ce plancher neutralise aussi le lissage d'émission (pacing) : chaque image vidéo part en rafale de 50 à 120 paquets ; un pacer qui étalerait la rafale ajouterait du jitter structurel que le frame pacing du client devrait ensuite absorber.
- File d'émission de datagrammes courte (64 à 128 Kio) : sous congestion réelle, on JETTE le périmé (la FEC l'absorbe) au lieu d'empiler de la latence.
- Fréquence d'acquittements réduite (environ 1 ACK pour 10 paquets, délai maximal ~5 ms) : à 9500 paquets/s en descente, les ACK par défaut génèrent des milliers de paquets montants inutiles.
- Pompes d'E/S événementielles (jamais de boucles d'attente), priorité temps réel Windows (MMCSS), tampons socket de 4 Mio, offload de segmentation UDP activé.
- Garde-fou permanent : à l'établissement de chaque session, le type de contrôleur actif est vérifié ; le profil de perte du banc tourne en CI sur chaque build de release (risque n°2 du plan : livrer l'étranglement par accident).

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
