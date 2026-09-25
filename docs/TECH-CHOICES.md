# Choix de technologies

Chaque brique, le choix retenu, la raison, et les alternatives sérieusement considérées puis rejetées.

## Tableau de synthèse

| Brique | Choix | Raison principale |
|---|---|---|
| Cœur, service, tunnel, moteur, CLI, broker | Rust | Sûreté mémoire pour du code réseau exposé en permanence, performances, écosystème exact (tokio, quinn/iroh, axum, windows-service) |
| Interface | Dessinée par le produit, en Direct2D et DirectWrite, dans une fenêtre Win32 à lui | Rien n'est embarqué, le texte est rendu par le moteur du système ; zéro processus de navigateur ; le système de design reste écrit une seule fois. Aucune boîte à outils d'interface : 321 caisses de moins dans le verrou du projet |
| Moteur | Le moteur ZyrDesk, écrit en Rust ([D219](DECISIONS.md), [D222](DECISIONS.md)) : capture, conversion, encodage, découpe en paquets, décodage, affichage, son, clavier et souris ([MOTEUR.md](MOTEUR.md)) | Latence, fluidité et qualité d'image avant tout ; plus aucune jointure avec un programme qu'on ne contrôle pas ; le débit de l'encodeur pourra enfin suivre ce que voit le tunnel |
| Graphisme du moteur | Direct3D 11 et DXGI | La capture qui voit l'écran de connexion n'existe qu'en Direct3D 11, le décodage matériel le plus éprouvé aussi, et l'affichage passe par DXGI : aucune traduction d'un bout à l'autre, rien à installer |
| Codecs | FFmpeg 9.0.2, compilé par nous, réduit au moteur, chargé au démarrage depuis `vendor/ffmpeg` | Une seule porte vers les encodeurs matériels des trois fabricants, x264 en secours, les décodeurs matériels et Opus ; rien à installer ni à lier à la compilation |
| Correction d'erreurs | Reed-Solomon (reed-solomon-simd), 20 % de parité par image | Une perte se répare sans attendre d'aller-retour : n'importe quels morceaux en nombre suffisant reconstruisent l'image |
| Transport | quinn, et sous QUIC une couche de chemins à nous : aiguilleur, sondes signées, branche de relais | Contrôleur média mesuré au banc M2 (D13) ; la migration relais vers direct se fait sans que QUIC le sache, donc sans changer de transport ; examen d'iroh clos par D119 |
| IPC local | Named pipes (tokio) : le tube de commande de la fenêtre, et un tube par session entre chaque moitié du moteur et le service | Natif Windows, simple, contrôle d'accès par l'identité Windows, injoignable depuis le réseau |
| Secrets | DPAPI (profil SYSTEM) côté service, lien de compte compris ; la fenêtre ne tient aucun secret | Standard Windows, zéro dépendance exotique |
| Serveur (broker et relais) | Un binaire Rust, `zyrdesk-server` : axum + WebSocket + SQLite (rusqlite, WAL, migrations numérotées), quinn pour le relais, AGPLv3 | Auto-hébergeable dès le premier jour sur un Debian, en quelques questions ; SQLite suffit largement ; Postgres possible ensuite ([SERVER.md](SERVER.md)) |
| Relais | Le nôtre, dans le même binaire : datagrammes QUIC, paquets opaques entre les deux empreintes qu'un laissez-passer nomme | Ne voit que du chiffré, CPU minimal ; pas de blocage en tête de ligne, contrairement aux relais sur TCP (DERP, iroh) |
| Découverte LAN | mdns-sd | Éprouvé, sans runtime imposé |
| Mappage de ports | portmapper (UPnP + NAT-PMP + PCP) | Crate maintenue et utilisée en production par iroh |
| Comptes | Argon2id (paramètres OWASP), jetons opaques hachés, appareil prouvé par sa clé, TOTP après le MVP | Standard moderne |
| Installateur | NSIS, script à nous + étapes personnalisées (service, pare-feu) | Un seul format, scriptable, sans dépendance d'outillage |

## Justifications détaillées et alternatives rejetées

Interface : ce qui a été choisi, puis ce qui s'est passé

Le choix d'origine était Tauri v2 avec une interface en technologies web. Il a tenu jusqu'à ce qu'un défaut le mette en défaut : le liseré pâle du bouton flottant, cherché pendant onze essais, venait de la vue web elle-même, seule couche de ce bouton dont le fond est blanc. La remplacer par un dessin fait par le produit a réglé le liseré et, du même coup, quatre autres défauts que ce bouton portait depuis sa naissance ([D96](DECISIONS.md)). De là, le menu de la session, puis l'accueil, sont passés du même côté.

Le résultat : **le produit dessine son interface lui-même**, en Direct2D et DirectWrite, qui sont fournis par Windows. Rien n'est embarqué, le texte est rendu par le moteur qui rend celui du système, et il n'y a plus de processus de navigateur du tout. Le système de design n'a pas bougé : il est toujours écrit une seule fois et lu à la compilation.

Puis la fenêtre elle-même, sa boucle de messages, son icône près de l'horloge et son instance unique sont passées du même côté : **il n'y a plus de boîte à outils d'interface du tout**. C'est trois cent vingt et une caisses de moins dans le verrou du projet, et surtout la fin d'une couche qui visait autre chose entre nous et les messages de la fenêtre, là où tout ce que fait `picture` de délicat se joue.

Ce que le raisonnement d'origine avait juste : la vidéo ne traverse jamais l'interface, donc sa technologie n'influence pas la latence. Ce qu'il avait manqué : une interface posée **par-dessus** une vidéo n'est pas dans le chemin de l'image mais elle est dans le même pixel, et là un navigateur ne sait pas se taire.

Interface : ce qui avait été écarté à l'époque du choix d'origine

- Slint (Rust natif) : sérieux et léger, mais atteindre un rendu réellement premium y coûte beaucoup plus d'effort qu'en web (écosystème de design réduit), et la version gratuite impose une attribution visible (sinon licence commerciale payante). Le produit a fini par dessiner lui-même, ce qui revient au même effort sans la licence ni la dépendance.
- Flutter desktop : très beau rendu possible, mais runtime lourd, deuxième langage (Dart) à vie dans le projet, et desktop Windows moins mûr que le mobile.
- Qt Quick : capable, mais liaison Rust (cxx-qt) pré-1.0, contraintes LGPL de déploiement à gérer, et style par défaut loin de la cible.
- egui / iced : pas au niveau visuel exigé sans effort massif ; le rendu en mode immédiat consomme du CPU en continu, exactement ce qu'un produit de streaming doit éviter.
- Electron : validait aussi le besoin (c'est le choix de plusieurs concurrents commerciaux), mais 10 fois plus lourd pour le même résultat, sans bénéfice puisque notre cœur est déjà en Rust.

Le point non négociable derrière ce choix : l'image est dessinée par le lecteur, en Direct3D 11, depuis son propre fil, dans une fenêtre enfant qui n'appartient qu'à lui. L'interface n'est jamais dans le chemin de l'image, donc sa technologie n'influence pas la latence.

Moteur : le nôtre

Jusqu'en septembre 2026, l'image, le son et les entrées passaient par Sunshine et moonlight-qt, pilotés en processus enfants ; chaque gros défaut de l'époque vivait à la jointure avec ces deux programmes, et ZyrDesk les a remplacés d'un coup par son propre moteur ([D219](DECISIONS.md), [D222](DECISIONS.md)). Les codecs ne sont pas réinventés, ni ce que Windows fournit : c'est la chaîne entière entre les deux qui est à nous.

Graphisme : Direct3D 11 plutôt que Vulkan

- La capture de l'écran par Windows (Desktop Duplication), la seule qui voit l'écran de connexion et les invites d'administration, ne parle que Direct3D 11.
- Le décodage matériel le plus éprouvé sur les cartes des trois fabricants passe par Direct3D 11, et l'affichage le plus direct sur Windows par DXGI, la couche d'affichage de Direct3D.
- Vulkan obligerait à traduire l'image à chaque bout, à la capture et à l'affichage, et chaque traduction coûte du temps ou une copie. Direct3D 11 est le chemin le plus court, et il n'ajoute rien à installer : Windows l'a déjà. Le détail est dans [MOTEUR.md](MOTEUR.md) §2.

Codecs : FFmpeg 9.0.2, compilé par nous et chargé au démarrage

- FFmpeg est la bibliothèque qui parle aux encodeurs matériels des trois fabricants (NVENC chez NVIDIA, AMF chez AMD, Quick Sync chez Intel) et à celui de Windows (Media Foundation), avec x264 en secours logiciel, et aux décodeurs matériels par Direct3D 11. Écrire chacun de ces chemins à la main serait des mois de travail.
- Les versions toutes faites contiennent des centaines de formats dont le moteur n'a pas l'usage, pèsent plusieurs dizaines de Mo et demandent parfois d'autres fichiers à livrer avec. La nôtre, la dernière version stable ([D221](DECISIONS.md)), ne garde que ce qui sert : trois DLL, moins de 10 Mo, rangées dans `vendor/ffmpeg` avec leurs licences, refaites par `packaging/ffmpeg/build.sh` depuis des sources dont l'empreinte est vérifiée.
- Chargée au démarrage du moteur et jamais liée à la compilation : `cargo build` ne demande ni FFmpeg ni aucun outil de plus sur le PC de Victor.
- Les en-têtes NVIDIA restent en version 13.0 plutôt que 13.1 : NVENC marche ainsi dès le pilote 570 au lieu du 610, ce qui garde leur encodeur aux GeForce 10 ([D221](DECISIONS.md)).
- Licence : avec x264, l'ensemble est sous GPL version 2 ou ultérieure, compatible avec la GPL version 3 de ZyrDesk ; les sources exactes doivent accompagner chaque publication qui contient les DLL (voir `vendor/ffmpeg/README.md`).

Transport : quinn, une couche de chemins à nous, et pas...

- iroh : examiné deux fois, à M2 (D13) et à la conception du serveur (D119, [SERVER.md](SERVER.md) §4.8). Solide en 2026 (multichemin QUIC, perforation, relais éprouvé), mais il repose sur noq, son fork de quinn, et sur un relais TCP ; l'adopter changerait de transport pour obtenir une migration que l'aiguilleur donne sans en changer. Reste l'endroit par lequel il pourrait entrer un jour.
- webrtc-rs : lourd, architecture asynchrone contraignante, pas taillé pour notre cas.
- str0m (WebRTC sans E/S) : crédible côté serveur SFU, mais notre chemin principal (pair à pair) y est le moins éprouvé.
- boringtun (WireGuard) : en restructuration annoncée par son propre README ; et WireGuard seul n'apporte ni traversée NAT ni multiplexage fiable/non fiable.
- MoQ (Media over QUIC) : conçu pour la diffusion à grande échelle (~400 ms de latence cible), mauvais outil pour du 1:1 interactif sous 30 ms.
- TCP ou WebSocket pour le média : disqualifiés d'office (retransmissions et blocage tête de ligne incompatibles avec la latence cible).

Le risque réel du choix QUIC (contrôle de congestion qui étrangle un flux vidéo constant sous perte) est traité frontalement : contrôleur média sur mesure, critère GO/NO-GO au banc M2, profil de perte en CI permanente. Détails : [NETWORK.md](NETWORK.md).

Broker : SQLite d'abord

- Un fichier, zéro administration, sauvegarde triviale, largement suffisant pour des milliers d'appareils. Le code d'accès est écrit pour permettre Postgres quand le besoin réel arrive. Choisir Postgres maintenant serait de la complexité d'avance.

Windows : un service en Rust à nous

- Le service ZyrDesk fait bien plus que lancer le moteur (identité, broker, tunnels, IPC). Il suit le schéma éprouvé des services de capture d'écran, celui de Sunshine en particulier : jeton SYSTEM rattaché à la session qui tient l'écran, lancement sur le bureau interactif, objet de tâche qui emporte le moteur avec le service, porte rouverte au changement de session. Le moteur hôte est ce même programme, relancé avec un argument réservé : un binaire de moins à livrer.

Packaging : NSIS plutôt que MSI

- Un seul format, scriptable de bout en bout, suffisant pour app + service + FFmpeg + règles pare-feu + (plus tard) pilote optionnel. MSI reconsidérable si un besoin de déploiement d'entreprise apparaît.

## Contraintes transverses actées

- Aucun coût récurrent obligatoire : pas de certificat de signature payant, pas de compte développeur payant, pas de composant propriétaire. Les binaires partent non signés (avertissement SmartScreen documenté, normal pour un jeune projet open source) ; la signature gratuite pour projets open source (SignPath Foundation) sera demandée quand le projet sera public et actif.
- Toute brique tierce doit être : licence compatible (GPLv3/AGPLv3 côté produit), maintenue, et remplaçable (confinée derrière une interface à nous quand elle est structurante, comme le transport ou FFmpeg).
