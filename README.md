# ZyrDesk

Bureau à distance open source très faible latence, pensé pour un usage réellement fluide : Blender, CAO, Unreal Engine, bureaux très animés, et éventuellement le jeu. Objectif d'expérience utilisateur : une fluidité et une simplicité comparables aux meilleures solutions commerciales du marché.

Une seule application à installer. Le même ZyrDesk sert d'hôte (le PC que l'on contrôle), de client (le PC depuis lequel on se connecte), ou des deux à la fois.

```text
Mes ordinateurs

● PC-BUREAU        [ Se connecter ]
● PC-PORTABLE      [ Se connecter ]
○ PC-ATELIER       Hors ligne
```

## Principes

- Performance d'abord : 1080p60 et 1440p60 réels, encodage et décodage matériels, frame pacing soigné, latence minimale. Jamais sacrifiés pour simplifier l'architecture.
- Un moteur à nous, dédié à la performance : capture, encodage, transport, décodage, affichage, son, clavier et souris sont écrits par ZyrDesk, en Rust ([docs/MOTEUR.md](docs/MOTEUR.md)). L'encodage et le décodage passent par FFmpeg, qui parle aux puces vidéo des cartes NVIDIA, AMD et Intel, avec un encodeur logiciel de secours.
- Connexion directe prioritaire : le flux vidéo va d'un PC à l'autre sans intermédiaire chaque fois que possible. Un petit serveur (broker) sert uniquement à la mise en relation : comptes, liste des appareils, présence, échange des informations de connexion. En dernier recours, un relais transporte des paquets chiffrés qu'il ne peut pas lire.
- Le serveur est facultatif : sur un réseau local, par un VPN ou par une adresse publique avec un port ouvert, ZyrDesk se passe de tout serveur et de tout compte. Avec un serveur, auto-hébergé sur un Debian en quelques questions, on gagne les comptes, la présence, les contacts et la connexion en un clic d'où qu'on soit ([docs/SERVER.md](docs/SERVER.md)).
- Chiffré de bout en bout : les clés de session ne quittent jamais les appareils. Ni le broker ni le relais ne peuvent déchiffrer la vidéo, l'audio ou les entrées clavier/souris.
- Un vrai produit : interface moderne, premium, minimaliste. Windows 11 d'abord, scénario NVIDIA vers NVIDIA en premier.
- Maintenable dans la durée : aucun programme tiers à piloter de l'extérieur, et des outils et bibliothèques tenus à leur dernière version stable ([D221](docs/DECISIONS.md)).

## État du projet

Jalon en cours : **MZ, le moteur ZyrDesk**. Sunshine et Moonlight ont quitté le produit : ZyrDesk a maintenant son propre moteur, en Rust, qui filme l'écran, le compresse par la carte graphique, le fait passer par le tunnel et l'affiche dans la fenêtre de ZyrDesk elle-même ([docs/MOTEUR.md](docs/MOTEUR.md), [D222 et D223](docs/DECISIONS.md)). Il est construit et passe ses essais automatiques, mais n'a encore jamais tourné sur un vrai Windows : c'est ce que fait vérifier [docs/testing/MZ-PROTOCOLE.md](docs/testing/MZ-PROTOCOLE.md), sur les deux PC.

Les jalons M5 et M6 ont apporté le serveur facultatif et le relais. Le serveur, `zyrdesk-server`, s'installe sur un Debian en quelques questions ([server/README.md](server/README.md)), tient des comptes, met les ordinateurs d'un même compte en relation sans jamais regarder passer l'image, et porte leurs paquets chiffrés quand aucun chemin direct n'existe. Dans la fenêtre, une section Compte, et les ordinateurs du compte dans « Mes ordinateurs » avec leur présence. Leurs protocoles, [docs/testing/M5-PROTOCOLE.md](docs/testing/M5-PROTOCOLE.md) et [docs/testing/M6-PROTOCOLE.md](docs/testing/M6-PROTOCOLE.md), restent à dérouler en entier.

Le jalon M4, l'interface, a rendu le produit entièrement pilotable à la souris. Sur un réseau local, deux ZyrDesk se trouvent seuls et s'autorisent seuls : aucune adresse à recopier, aucune empreinte à transporter, aucun code à quatre chiffres à taper d'un écran à l'autre. Le service s'installe depuis la fenêtre, et un journal complet se copie en un clic. Son protocole, [docs/testing/M4-PROTOCOLE.md](docs/testing/M4-PROTOCOLE.md), reste à dérouler en entier sur les deux machines.

Le jalon M0 (ossature Rust, moteurs épinglés, diagnostic, installateur, intégration continue) est terminé. Le jalon M1 a produit une première session distante réelle en 1080p ; ses hypothèses restantes sont listées dans [docs/testing/M1-PROTOCOLE.md](docs/testing/M1-PROTOCOLE.md). Le jalon M2 a livré le tunnel chiffré et son banc de mesure : les trois seuils de performance sont tenus sur deux PC en Ethernet gigabit ([perf/baselines/M2-lan-ethernet.md](perf/baselines/M2-lan-ethernet.md)). Le jalon M3 a livré le service Windows, qui rend la machine joignable avant qu'on y ouvre une session. La feuille de route complète est dans [docs/ROADMAP.md](docs/ROADMAP.md).

## Utiliser

Lancer `ZyrDesk.exe` sur les deux ordinateurs. La première fois, un bouton de la fenêtre installe le service, ce qui demande une autorisation Windows ; ensuite il démarre tout seul avec la machine.

Chaque ordinateur apparaît alors dans la fenêtre de l'autre. Un clic sur sa carte ouvre la session. Rien d'autre n'est demandé à personne.

Avec un serveur, ce qui reste facultatif : Réglages, section Compte, « Se connecter à un serveur ». Les ordinateurs du compte apparaissent alors dans « Mes ordinateurs », en ligne ou non, et les appareils du compte se renomment et se révoquent depuis n'importe lequel d'entre eux.

Tout ce qu'une session transporte passe par un seul port, le 47000 en UDP. Le service en écoute un second, le 5353, celui que mDNS réserve pour que deux ordinateurs se trouvent sur un réseau local. Les deux règles de pare-feu correspondantes sont posées par le service au moment où il s'installe, et retirées quand on le retire ; elles ne valent que pour lui.

## En ligne de commande, pour diagnostiquer

Rien de ce qui suit n'est nécessaire à l'usage du produit : c'est l'outillage qui sert à isoler une panne, et il ne passe pas toujours par les mêmes chemins que l'interface.

```bash
zyr-cli doctor            # cette machine est-elle prête : FFmpeg, encodeurs de la carte graphique
zyr-cli identity          # empreinte de cette machine
zyr-cli connect <adresse> --pair <empreinte>       # ouvrir une session sans fenêtre, et lire ce qu'elle coûte
zyr-cli bench host --pair <empreinte>              # mesurer le tunnel, côté attente
zyr-cli bench client <adresse> --pair <empreinte>  # mesurer le tunnel, côté mesure
zyr-cli account status    # le compte auquel cet ordinateur est rattaché
```

Un ordinateur que le réseau local ne montre pas s'ajoute depuis la fenêtre, « Ajouter un ordinateur », sur les deux machines ([docs/SECURITY.md](docs/SECURITY.md) §1.1).

Le service se gère aussi à la main, en fenêtre administrateur :

```bash
zyrdeskd setup       # inscrire le service et le lancer, ce que fait la fenêtre
zyrdeskd status      # savoir où il en est
zyrdeskd stop        # l'arrêter, avant de recompiler
zyrdeskd uninstall   # le retirer
```

## Construire

Prérequis : Rust stable.

Sous Windows, Rust ne suffit pas : il assemble avec l'éditeur de liens de Microsoft, qui n'est pas livré avec lui. Sans lui, la compilation s'arrête sur `linker \`link.exe\` not found` dès la première caisse, avant même d'avoir touché au code de ZyrDesk. Il vient des **Build Tools for Visual Studio**, avec la charge de travail « Développement Desktop en C++ » ; Visual Studio Code n'est pas la même chose et ne le porte pas.

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Écrit ici parce qu'une machine neuve est le seul endroit où cela se voit, et que le message d'erreur ne dit pas quoi installer.

```bash
git clone https://github.com/Victor-root/ZyrDesk
cd ZyrDesk
cargo build --release
```

Pour se mettre à jour ensuite : `git pull`, puis `cargo build --release`. C'est tout. FFmpeg, dont le moteur se sert pour compresser et décompresser l'image et le son, est livré avec le dépôt, déjà compilé, dans `vendor/ffmpeg` ([vendor/ffmpeg/README.md](vendor/ffmpeg/README.md)) : rien d'autre à télécharger ni à compiler.

Les essais se lancent par `cargo test --workspace`. Sous Windows, ceux du moteur prennent le FFmpeg du dépôt ; sous Linux, ils demandent une compilation des mêmes sources pour Linux, par `bash packaging/ffmpeg/build.sh linux <dossier>`, puis `ZYR_FFMPEG_DIR=<dossier>/lib cargo test --workspace`.

Construction de l'installateur Windows : voir [packaging/windows/README.md](packaging/windows/README.md).

## Conventions du code

Ceci s'adresse autant à une IA qu'à une personne qui reprend le dépôt.

- **Le code s'écrit en anglais** : noms de fichiers et de modules, types, fonctions, variables, constantes, noms des tests, commentaires et documentation du code, dans le Rust comme dans les scripts, la CI et la feuille de style.
- **Ce que la personne lit reste en français** : textes de l'interface, messages affichés, et tout ce que le programme écrit pour être lu.
- **L'aide des outils en ligne de commande** (`--help`) naît des commentaires du code : elle est en anglais, comme leurs options.
- **Ce qui est écrit sur le disque ou passe sur le réseau garde son nom exact**, même français (valeurs de préférences, clés de `install.env`, champs échangés avec le serveur, règle de pare-feu, dossier `vendor/ecran-virtuel`) : le renommer casserait une installation existante ou le dialogue entre deux versions.
- **La documentation de `docs/` reste en français**, et l'historique de [docs/DECISIONS.md](docs/DECISIONS.md) ne se réécrit pas.
- **Les outils et les dépendances suivent leur dernière version stable** : Rust, chaque bibliothèque du dépôt et FFmpeg. Un retard se justifie par écrit, dans le manifeste et dans [docs/DECISIONS.md](docs/DECISIONS.md), jamais par commodité.

Le détail et ses raisons : [D220](docs/DECISIONS.md#d220-le-code-sécrit-en-anglais-ce-que-la-personne-lit-reste-en-français-2026-09-24-pendant-m6).

## Documentation

| Document | Contenu |
|---|---|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Faisabilité, composants, processus Windows, flux de connexion, cycle de vie |
| [docs/MOTEUR.md](docs/MOTEUR.md) | Le moteur ZyrDesk : capture, encodage, réparation des pertes, affichage, son, clavier et souris, mesure |
| [docs/CLAVIER.md](docs/CLAVIER.md) | Les touches que Windows garde pour lui, et comment une session les obtient |
| [docs/ECRAN-VIRTUEL.md](docs/ECRAN-VIRTUEL.md) | L'écran que ZyrDesk fait pousser sur l'hôte, pour servir un écran plus grand que le sien sans agrandir quoi que ce soit |
| [docs/NETWORK.md](docs/NETWORK.md) | Tunnel, transport QUIC, traversée NAT, relais, budget latence et MTU |
| [docs/SECURITY.md](docs/SECURITY.md) | Identités, tickets de session, chiffrement, stockage Windows, modèle de menace |
| [docs/UI-UX.md](docs/UI-UX.md) | Direction visuelle, écrans, design system |
| [docs/TECH-CHOICES.md](docs/TECH-CHOICES.md) | Choix de technologies et alternatives rejetées |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Jalons M0 à M10 avec critères de sortie mesurables |
| [docs/TESTING.md](docs/TESTING.md) | Niveaux de tests, seuils de performance, banc de mesure |
| [docs/testing/M1-PROTOCOLE.md](docs/testing/M1-PROTOCOLE.md) | Première session sur deux PC, et hypothèses à lever |
| [docs/testing/M2-PROTOCOLE.md](docs/testing/M2-PROTOCOLE.md) | Mesure du coût du tunnel sur deux PC |
| [docs/testing/M3-PROTOCOLE.md](docs/testing/M3-PROTOCOLE.md) | Accès distant sans personne devant la machine |
| [docs/testing/M4-PROTOCOLE.md](docs/testing/M4-PROTOCOLE.md) | Le produit piloté entièrement à la souris, sur deux PC |
| [docs/testing/M5-PROTOCOLE.md](docs/testing/M5-PROTOCOLE.md) | Le serveur, le compte et « Mes ordinateurs », sur deux PC et un conteneur |
| [docs/testing/M6-PROTOCOLE.md](docs/testing/M6-PROTOCOLE.md) | Le relais et la bascule automatique, sur deux PC et un conteneur |
| [docs/testing/MZ-PROTOCOLE.md](docs/testing/MZ-PROTOCOLE.md) | Le moteur ZyrDesk, essayé pour la première fois sur deux PC |
| [docs/SERVER.md](docs/SERVER.md) | Le serveur facultatif : comptes, mise en relation, relais, TLS, installation |
| [server/README.md](server/README.md) | Installer, mettre à jour et administrer le serveur sur un Debian |
| [docs/COMPLIANCE.md](docs/COMPLIANCE.md) | Licences, obligations, marques, brevets codecs |
| [vendor/ffmpeg/README.md](vendor/ffmpeg/README.md) | Le FFmpeg du moteur : ce qu'il contient, d'où il vient, comment le recompiler |
| [docs/DECISIONS.md](docs/DECISIONS.md) | Décisions actées et décisions ouvertes |
| [perf/GATES.md](perf/GATES.md) | Seuils de performance chiffrés et protocoles de mesure |

## Licences

L'application ZyrDesk est sous GPLv3 (voir [LICENSE](LICENSE)) ; le FFmpeg qu'elle embarque est sous GPL version 2 ou ultérieure, compatible. Le serveur est sous AGPLv3 (voir [server/LICENSE](server/LICENSE)). Détails et obligations : [docs/COMPLIANCE.md](docs/COMPLIANCE.md).
