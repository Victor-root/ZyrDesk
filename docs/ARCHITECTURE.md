# Architecture ZyrDesk

Ce document décrit l'architecture d'ensemble : ce que ZyrDesk construit et ce qu'il emprunte, les processus qui composent le produit sous Windows, les flux de connexion et le cycle de vie. Le moteur lui-même, de l'écran filmé à l'image affichée, est décrit dans [MOTEUR.md](MOTEUR.md), le réseau dans [NETWORK.md](NETWORK.md) et la sécurité dans [SECURITY.md](SECURITY.md).

## 1. Ce qu'on construit, ce qu'on emprunte

Construit par ZyrDesk :

- Le moteur : capture de l'écran, conversion et encodage sur la carte graphique, découpe en paquets avec correction d'erreurs, décodage, affichage dans la fenêtre du produit, son, clavier et souris ([MOTEUR.md](MOTEUR.md)).
- L'expérience produit : interface, comptes, liste d'appareils, présence, connexion en un clic.
- Le réseau moderne : tunnel chiffré unique, traversée NAT, relais de secours, reprise de session.
- L'intégration Windows propre : service, accès non supervisé, installateur, mises à jour.
- Le presse-papiers partagé, fichiers compris.

Emprunté, parce que le refaire n'apporterait rien :

- Les codecs (H.264, HEVC et AV1 pour l'image, Opus pour le son), par FFmpeg 9.0.2, compilé par nous et réduit à ce que le moteur emploie ([TECH-CHOICES.md](TECH-CHOICES.md)).
- Ce que Windows fournit : Desktop Duplication pour filmer l'écran, Direct3D 11 et DXGI pour convertir, décoder et afficher, WASAPI pour le son, SendInput pour le clavier et la souris.
- QUIC, par la bibliothèque quinn, sous une couche de chemins à nous.
- Le pilote de l'écran virtuel, signé par son auteur ([ECRAN-VIRTUEL.md](ECRAN-VIRTUEL.md)).

Jusqu'en septembre 2026, l'image, le son et les entrées passaient par Sunshine et Moonlight, pilotés de l'extérieur ; ZyrDesk les a remplacés d'un coup par son propre moteur ([D219](DECISIONS.md), [D222](DECISIONS.md)).

Points risqués identifiés et traités (détails dans [NETWORK.md](NETWORK.md) et [ROADMAP.md](ROADMAP.md)) :

- Le contrôle de congestion QUIC standard étranglerait la vidéo sous perte : contrôleur sur mesure obligatoire, validé par un banc de mesure dédié (jalon M2, critère GO/NO-GO).
- La pérennité de la signature du pilote d'écran virtuel tiers : testée dès le jalon M1.
- La latence et la fluidité du moteur : le lecteur mesure lui-même la latence de bout en bout de chaque image et l'intervalle entre deux images affichées ([MOTEUR.md](MOTEUR.md) §9), jugés sur les seuils de [perf/GATES.md](../perf/GATES.md).

Idées initiales abandonnées après étude :

- « Un seul exécutable » réel : le service doit démarrer avant l'ouverture de session, et le moteur hôte doit vivre dans la session qui tient l'écran, un par session ; le produit reste donc en plusieurs processus. Le moteur hôte est toutefois le programme du service lui-même, relancé : l'utilisateur ne voit toujours qu'un seul produit.
- Loger le tunnel dans le processus d'interface : voir §3, le tunnel vit dans le service.

## 2. Vue d'ensemble

```text
                         Serveur ZyrDesk (facultatif)
             broker : comptes, appareils, présence, tickets de
             session signés, laissez-passer du relais
                    /                              \
             WSS sortant                        WSS sortant
                  /                                  \
PC CLIENT                                              PC HÔTE
ZyrDesk.exe (compte de la personne)                    ZyrDesk.exe (compte de la personne)
  interface, bouton flottant, icône                      interface, icône
  lecteur : l'image dans la fenêtre                        │
    │ tube de commande    │ tube du lecteur                │ tube de commande
    ▼                     ▼                                ▼
zyrdeskd (service, SYSTEM) ═══════ tunnel QUIC ═══════ zyrdeskd (service, SYSTEM)
                               ║                           │ tube du moteur (SYSTEM seul)
                  UN SEUL flux UDP chiffré                 ▼
                  direct OU via relais                 zyrdeskd --serve-a-session
                               ║                       moteur hôte, un par session,
        relais ZyrDesk (ne voit que des paquets        SYSTEM, dans la session de l'écran
        chiffrés, jamais les clés)
```

En fonctionnement normal, le flux média circule directement entre les deux PC. Le broker ne voit jamais un octet de média ; le relais, quand il sert, ne transporte que des paquets chiffrés qu'il ne peut pas déchiffrer. Aucune moitié du moteur n'ouvre de prise réseau : chacune parle au service de sa machine par un tube nommé, et le tunnel porte ce tube d'une machine à l'autre.

Le serveur est facultatif. Sans lui, ZyrDesk se joint sur le réseau local, par un VPN ou par une adresse publique avec un port ouvert, sans compte ; avec lui, un compte, la présence, les contacts, les partages et la connexion en un clic d'où qu'on soit. Les deux façons cohabitent dans le même produit, et un service sans lien de compte ne contient aucun code qui parle au serveur. La conception entière est dans [SERVER.md](SERVER.md).

## 3. Les processus (un seul produit visible)

| Processus | Rôle | Compte | Durée de vie |
|---|---|---|---|
| `ZyrDesk.exe` | Interface, dessinée par le produit lui-même dans sa propre fenêtre Win32, icône de zone de notification. Pendant une session : le lecteur (`zyr-player`), qui décode, dessine l'image dans une fenêtre enfant de la fenêtre principale et joue le son, et le bouton flottant au-dessus | Utilisateur connecté | Session utilisateur |
| `zyrdeskd.exe` | Service Windows : identité de l'appareil, lien de compte, LES DEUX extrémités de tunnel (rôle client et rôle hôte), tube de commande de la fenêtre, un tube par session pour le moteur, lancement du moteur hôte | LocalSystem | Démarré par la fenêtre, ou avec Windows quand l'ordinateur doit répondre avant toute ouverture de session |
| `zyrdeskd.exe --serve-a-session <tube>` | Moteur hôte (`zyr-host`) : capture, conversion, encodage, découpe en paquets, son, clavier et souris ; ne parle qu'au service, par son tube | SYSTEM, dans la session de l'écran | Une session entrante |

Le service se relance aussi, brièvement, dans la session de l'écran pour quelques courses que Windows ne permet que de là : couper ou rendre les enceintes, verrouiller l'écran, tenir et rendre l'arrangement des écrans, suivre la forme du pointeur. L'assistant du presse-papiers est de la même famille, à ceci près qu'il tourne sous le compte de la personne (§9).

Pourquoi le tunnel vit dans le service et pas dans la fenêtre :

- Côté hôte, le service existe de toute façon (accès non supervisé avant ouverture de session) ; côté client, il porte l'identité de l'appareil et le lien de compte. Un seul endroit tient donc les clés, l'authentification et les chemins réseau, et la fenêtre ne tient aucun secret.
- Le service est le propriétaire de chaque voie : il la tient, la surveille et la referme. Une voie à laquelle aucun lecteur ne vient est refermée au bout de deux minutes, une voie dont le lecteur est venu puis reparti se referme d'elle-même, et une voie rattachée à un processus disparu aussi.
- Contrepartie : l'image vit dans la fenêtre, si bien qu'une fenêtre qui plante emporte sa session, son tube se fermant avec elle. Rouvrir ZyrDesk suffit pour rouvrir la session. Pendant une session, l'image remplit la fenêtre principale, le bouton flottant reste au-dessus, et la croix de la fenêtre termine la session ([D23](DECISIONS.md)).
- Conséquence assumée : le service est requis même pour un usage purement client (installation avec droits administrateur). Un mode client sans service pourra être étudié plus tard.

Pourquoi le moteur hôte est un processus à part, lancé pour chaque session :

- Il doit voir l'écran de connexion et les invites d'administration, ce que seul le compte système permet, et il doit vivre dans la session qui tient l'écran, alors que le service vit dans une session sans écran. Le service le lance donc dans la session de l'écran, avec son propre jeton système.
- Il naît avec sa session et meurt avec elle : rien ne reste d'une session à l'autre, et un moteur qui tombe n'emporte que la sienne. Il naît dans un objet de tâche (job object) qui l'emporte si le service s'arrête brutalement.

## 4. Le moteur, en bref

Le moteur a deux moitiés qui ne se voient jamais directement. Le moteur hôte (`zyr-host`) est lancé par le service pour chaque session entrante. Le lecteur (`zyr-player`) est une bibliothèque que la fenêtre fait tourner dans son propre processus ; la ligne de commande la fait tourner aussi, sans fenêtre, pour le diagnostic.

Chaque moitié parle au service de sa machine par un tube nommé, la liaison locale, qui porte quatre canaux : le flux de contrôle (touches, souris, réglages, demandes d'image clé), l'image, le son, et les mots du service. Le tunnel porte cette liaison d'une machine à l'autre. FFmpeg est chargé au démarrage de chaque moitié, depuis `vendor/ffmpeg`, et jamais lié à la compilation.

Le lecteur dessine l'image en Direct3D 11, depuis son propre fil, dans une fenêtre enfant de la fenêtre principale : l'interface n'est jamais sur le chemin de l'image. Le détail, les choix et leurs raisons : [MOTEUR.md](MOTEUR.md).

## 5. Flux : activer l'accès distant

1. L'utilisateur active « Autoriser l'accès distant » dans l'interface.
2. L'interface le demande au service par le tube de commande (`hosting`).
3. Le service vérifie que FFmpeg est complet dans `vendor/ffmpeg`, ouvre sa porte (le port UDP du tunnel) et suit la session qui tient l'écran. Aucun moteur ne tourne encore : chaque session entrante lancera le sien.
4. S'il a un lien de compte, le service annonce l'appareil « disponible » au broker (connexion sortante persistante).
5. Au démarrage de Windows, si le service est réglé pour démarrer avec lui, il refait les étapes 3 et 4 sans aucune intervention : le PC est joignable depuis l'écran de connexion.

## 6. Flux : se connecter

1. Clic sur « Se connecter » : la fenêtre vérifie d'abord que FFmpeg est là, puis demande une voie au service (`reach`).
2. Le service client demande un ticket au broker ; le broker vérifie que les deux appareils appartiennent au même compte, ou qu'un partage les lie, et remet aux deux extrémités un ticket signé de courte durée, puis leur fait passer leurs candidats de chemin et, s'il en a un, l'adresse du relais.
3. Les deux services établissent le tunnel QUIC avec authentification mutuelle par clés d'appareil (voir [SECURITY.md](SECURITY.md)) : chemin relais d'abord si nécessaire, promotion vers le direct en parallèle (voir [NETWORK.md](NETWORK.md)).
4. Premier mot de la session, sur le canal ZyrDesk du tunnel : la question d'ouverture, avec le débit et la cadence voulus. Le service hôte taille son tunnel pour ce débit, puis lance le moteur de cette session. Il crée un tube que seul le compte système peut ouvrir, démarre `zyrdeskd --serve-a-session <nom du tube>` dans la session de l'écran, attend que le moteur rejoigne son tube (dix secondes au plus ; s'il s'arrête avant, le service le voit aussitôt), vérifie que c'est bien ce processus-là qui s'y est connecté, lui dit la taille d'un datagramme et l'écran à filmer, et répond seulement alors que la session est ouverte. Un refus revient à l'autre ordinateur en mots lisibles, qu'il affiche. Il n'y a pas d'appairage : le tunnel a déjà reconnu les deux ordinateurs à leur empreinte, et le moteur ne demande rien de plus ([D222](DECISIONS.md)).
5. Le service client crée alors le tube du lecteur, que peuvent ouvrir le compte système et les personnes connectées à la machine, et rend son nom à la fenêtre. Avant toute image, la fenêtre dit à l'ordinateur d'en face ce que le lecteur ne sait pas dire : quel écran filmer, si ses enceintes se taisent, et quel écran montrer (l'écran virtuel au besoin).
6. La fenêtre démarre son lecteur avec le nom du tube. Le lecteur s'y connecte, le tunnel ouvre le flux du moteur vers le moteur hôte, et le lecteur se présente : taille, cadence, débit et codec voulus, et ce que sa carte graphique sait décoder. Le moteur hôte choisit le codec, démarre la capture et l'encodeur, et les images partent.
7. À la première image, l'image s'affiche et la fenêtre rattache la voie à son processus (`hold`) : le service compte désormais cette session parmi les siennes, et la referme si ce processus disparaît. L'interface affiche l'état (chemin direct ou relais, latence).

Pourquoi le lecteur tourne chez l'appelant et non dans le service, contrairement au moteur hôte : côté client, quelqu'un est forcément connecté, et l'image doit s'afficher sur son bureau, dans sa fenêtre, avec ses droits et son périphérique audio. Le faire tourner dans le service imposerait la même duplication de jeton que pour le moteur hôte, pour rien, et ferait tourner en compte système un programme qui n'a aucune raison de l'être. Le service reste malgré tout le seul propriétaire de la voie : il la tient, la surveille et la referme.

Sur un réseau local, les étapes 2 et 3 se passent du broker : les deux services s'annoncent en mDNS, chacun connaît donc l'adresse et l'empreinte de l'autre, et le service hôte admet les empreintes ainsi annoncées tant que la confiance au réseau local est accordée (D17). Il n'y a alors rien à recopier ni à taper d'un ordinateur à l'autre, ni avant la première session ni après.

## 7. Flux : reprise et résilience

- Coupure réseau courte : le tunnel l'absorbe. Une image dont des paquets manquent est réparée par la correction d'erreurs du moteur, sans rien redemander ; une image irréparable coûte une seule demande d'image clé, et le lecteur garde la dernière image juste en attendant. La connexion survit à trente secondes de silence, une seule patience pour tout le produit ([D138](DECISIONS.md)) : l'aiguilleur garde la dernière route, et le moteur hôte relâche tout ce qui est enfoncé si le lecteur se tait aussi longtemps.
- Coupure plus longue, ou session tombée pour une autre raison : la fenêtre rouvre la session d'elle-même, dans la même fenêtre, après trois secondes de pause, en affichant « Connexion perdue, reprise en cours… ». Au plus cinq reprises de suite, et deux ouvertures manquées, avant de le dire à la personne ([D174](DECISIONS.md)).
- Verrouillage, invite d'administration, écran de connexion côté hôte : le moteur suit le bureau qui reçoit les entrées, sans rien relancer. Déconnexion ou changement d'utilisateur : la session qui tient l'écran change, le service referme sa porte et la rouvre dans la nouvelle une seconde plus tard, et le client rouvre la session de lui-même comme ci-dessus.
- Fin d'une session : le service ferme le tube du moteur, lui laisse deux secondes pour relâcher les touches et les boutons encore tenus, et l'arrête s'il est encore là. Un moteur qui s'arrête ferme son tube, ce qui termine sa session. Ce qu'un bout dit en partant (son au revoir, ou pourquoi il abandonne) traverse le tunnel avant la fermeture, deux secondes au plus : c'est ce qui dit à l'autre bout comment la session a fini.
- Le lecteur rapporte des faits, pas un diagnostic : fin demandée, hôte parti, lien perdu, moteur en échec avec sa raison. Classer une panne en « perte réseau » ou « erreur fatale » est une décision de la fenêtre.

## 8. États dégradés : détectés et expliqués

Règle : jamais un écran noir sans explication. Le service et le moteur détectent, l'interface explique :

- FFmpeg absent ou incomplet dans `vendor/ffmpeg` : côté hôte, l'accès distant reste coupé et l'accueil affiche « FFmpeg absent » (le service regarde de nouveau toutes les cinq secondes) ; côté client, la session s'arrête avant de rien demander et dit quels fichiers manquent.
- Moteur hôte qui ne démarre pas, ne rejoint pas son tube ou s'arrête avant : l'ordinateur d'en face reçoit la raison en mots et l'affiche.
- Aucun encodeur pour le codec demandé, carte graphique du client incapable de le décoder, capture ou son en difficulté : le moteur hôte ou le lecteur le disent en français, jamais par un repli silencieux sur un décodage logiciel ; la fenêtre l'affiche pendant l'ouverture et l'écrit au journal ensuite.
- Écran hôte éteint ou en veille : pendant une session, le moteur hôte garde l'écran allumé et l'ordinateur éveillé ; un PC déjà en veille est injoignable (Wake-on-LAN : plus tard).
- PC hôte sans écran branché : le service fait pousser l'écran virtuel ([ECRAN-VIRTUEL.md](ECRAN-VIRTUEL.md)).
- Pas encore détecté : une session Bureau à distance Windows entrante active sur l'hôte, qui détache la console physique. Le message prévu est « Hôte indisponible : une session Bureau à distance Windows est active ».

## 9. Intégration Windows

- IPC local : tube nommé `\\.\pipe\ZyrDesk`, questions et réponses typées (§11). Qui peut parler au service se décide à la création du tube, pas à chaque message : le compte système et les administrateurs en contrôle total, la personne connectée à la machine en lecture et écriture, personne d'autre et rien depuis le réseau. Réserver l'activation de l'hôte aux administrateurs, message par message, reste à faire ([SECURITY.md](SECURITY.md) §5).
- Tubes du moteur : un par session et par machine, créés par le service sous un nom tiré au sort, une seule connexion chacun, jamais depuis le réseau. Chez l'hôte, le compte système seul ; chez le client, le compte système et les personnes connectées à la machine ([SECURITY.md](SECURITY.md) §5).
- Secrets : la clé privée de l'appareil et le jeton du lien de compte sont au service ; la fenêtre ne tient aucun jeton. Leur protection par DPAPI dans le profil SYSTEM (pas DPAPI « machine », déchiffrable par tout utilisateur local), fichiers réservés au compte système et aux administrateurs, reste à poser : aujourd'hui ils sont en clair dans le dossier de données ([SECURITY.md](SECURITY.md) §4).
- Pare-feu : des règles entrantes en UDP pour `zyrdeskd` uniquement, une par port qu'il écoute : le tunnel (47000), le réseau local (5353) et le voisinage (47001). Chaque règle est liée au programme du service, réécrite à chaque démarrage et supprimée à la désinstallation. Le moteur n'ouvre aucune prise et n'a besoin d'aucune règle ; celles que les versions précédentes posaient pour les anciens moteurs sont retirées là où elles restent.
- Journaux : tous les composants écrivent dans le sous-dossier `logs` des données du produit (rotation), en temps universel et sous la même forme, le service et la fenêtre partageant le même écrivain. Quatre traces : `service.log` (le service), `engine.log` (le moteur hôte), `engine-console.log` (ce que le moteur hôte écrit sur sa console avant d'avoir ouvert son journal, et ce qu'un plantage laisse derrière lui) et `interface.log` (la fenêtre, lecteur compris). Chaque ligne porte entre crochets le nom de la partie du produit qui l'a écrite, une étiquette par module, et l'écran du journal porte une boîte de tri dans la langue du journal d'un téléphone : un mot seul est un nom d'étiquette, plusieurs gardent l'un ou l'autre, `"entre guillemets"` cherche dans le texte, un moins écarte, `level:debug` choisit la voix. Le tri voyage jusqu'au service et jusqu'à l'ordinateur d'en face, et se fait à la lecture des fichiers avant la coupe. La page se resserre d'elle-même peu après la dernière lettre, et c'est cette page que « Copier » emporte ([D183](DECISIONS.md)). Les noms qu'une page porte vraiment sont annoncés dans son entête par la machine qui tient les fichiers, et se cochent d'un clic au-dessus de la boîte ([D188](DECISIONS.md)). Chaque ligne porte aussi sa voix, et tout est toujours écrit : ce que le produit dit de lui-même d'un côté, ce qui compte ou mesure de l'autre. La voix ne décide pas si une ligne existe, elle décide comment on la retrouve, et c'est le tri qui sépare les deux après coup (`level:debug`, `-level:debug`). Rien à allumer : une ligne qu'il faut activer n'est jamais là le soir où on la veut, et l'activer est une corvée demandée à quelqu'un qui est déjà coincé. Détails : [D181](DECISIONS.md), [D182](DECISIONS.md) et [D191](DECISIONS.md). Chaque binaire porte l'empreinte du code dont il a été compilé, gravée par un script de compilation, et l'écrit en tête de sa trace : une panne se lit toujours contre la version qui l'a produite. La fenêtre rassemble les quatre traces sur un écran, sous cet entête, avec un bouton qui copie l'ensemble. `zyr-cli doctor` vérifie la plateforme, les cartes graphiques, le dossier de données, qu'FFmpeg est là et se charge, les encodeurs vidéo qui s'ouvrent vraiment (sous Windows) et que le service tourne.
- Mises à jour : canal unique ; l'interface télécharge et vérifie le paquet, refuse d'appliquer pendant une session active, puis arrête le service, remplace les binaires (FFmpeg compris) et redémarre. Poignée de main de version entre interface, service et broker : les décalages de versions sont détectés proprement.
- Presse-papiers : canal ZyrDesk dédié dans le tunnel, des deux côtés à la fois, plus un assistant que le service relance dans la session qui tient l'écran (seul un programme de cette session peut lire ou écrire son presse-papiers ; le service est assis sur une station de fenêtres qui n'en porte aucune). Cet assistant-là, seul de tous, tourne sous le compte de la personne connectée et non sous celui du service : du texte et une image se posent en clair sur une station de fenêtres, des fichiers jamais, et les atteindre veut dire appeler dans un autre programme, ce que Windows ne permet qu'entre deux programmes de la même personne au même niveau ([D184](DECISIONS.md)). Texte et images, les images en PNG, l'imagerie de Windows faisant le PNG d'une capture d'écran. Interrupteur dans le menu du bouton flottant, allumé par défaut. Indisponible sur l'écran de connexion, qui n'a pas de session pour tenir un presse-papiers (assumé). Détails : [D179](DECISIONS.md).
- Fichiers au presse-papiers : au copier ne traverse que la liste des noms et des poids, quel que soit ce qu'ils pèsent ; les octets ne partent qu'au coller, l'assistant tenant entretemps la place des fichiers sur le presse-papiers de celui qui colle. Un morceau de deux cent cinquante mille octets à la fois dans chaque sens, ce qui borne par construction ce qu'un fichier peut prendre au lien sans rien avoir à régler. Les octets se posent dans un dossier du produit, jamais directement là où la personne colle, et Windows fait lui-même cette dernière copie. La marque du bouton flottant se remplit comme une barre de chargement. Détails : [D180](DECISIONS.md).
- Consentement et visibilité : pendant une session entrante, l'hôte affiche un indicateur (icône d'état + notification au début de session).

## 10. Organisation du dépôt

```text
ZyrDesk/
├─ Cargo.toml                  # workspace Rust
├─ crates/
│  ├─ zyr-proto/               # types partagés : chemins, journal horodaté, empreinte de compilation, réglages de session, patience d'une session
│  ├─ zyr-media/               # les formats du moteur, sans rien de Windows : paquets d'image et de son, correction d'erreurs, touches et souris, messages, cadence, mesures
│  ├─ zyr-codec/               # FFmpeg chargé au démarrage : encodeurs, décodeurs, Opus, conversion du son
│  ├─ zyr-host/                # le moteur hôte : capture, conversion, encodage, son, clavier et souris injectés
│  ├─ zyr-player/              # le lecteur : réassemblage, décodage, affichage, son
│  ├─ zyr-transport/           # la connexion QUIC (quinn n'est nommé que dans ce crate), identité et empreintes, confiance TLS et épinglage, l'aiguilleur et ses sondes signées, la branche de relais et la porte sur laquelle un serveur pose le sien, contrôleur média, budget des datagrammes
│  ├─ zyr-tunnel/              # le passage entre le tube du moteur et la connexion : flux du moteur, datagrammes d'image et de son, canal ZyrDesk
│  ├─ zyr-clipboard/           # le presse-papiers de l'ordinateur : ce qu'il porte, ce qu'on lui donne, les images en PNG, et la place tenue aux fichiers d'en face
│  ├─ zyr-control/             # le dialecte entre la fenêtre et le service, sur le tube de commande, et les tubes du moteur
│  ├─ zyr-session/             # ouverture d'une session de bout en bout, partagée par l'interface et la ligne de commande
│  ├─ zyr-lan/                 # annonce mDNS de cet ordinateur, appel direct, découverte des autres
│  ├─ zyr-broker/              # ce que le service et le serveur se disent : messages, tickets et laissez-passer signés
│  ├─ zyr-account/             # le lien de compte, le rattachement, le canal vivant, la présence, le rendez-vous
│  ├─ zyr-screen/              # l'écran virtuel : pilote, réveil, sommeil, arrangement des écrans
│  ├─ zyr-sound/               # le son de la session, dans le mélangeur de Windows
│  ├─ zyr-cli/                 # doctor, session sans fenêtre, banc de mesure, identité, compte
│  ├─ zyr-ui/                  # l'application : cœur Rust, écrans dessinés par le produit, image de la session, journal, bouton flottant
│  └─ zyrdeskd/                # binaire service Windows : registre des voies, serveur du tube, tous les tunnels, superviseur ; relancé, c'est aussi le moteur hôte
├─ server/                     # zyr-server, le serveur facultatif (comptes, mise en relation et relais ; un binaire, AGPLv3), install.sh, README
├─ vendor/
│  ├─ ecran-virtuel/           # pilote de l'écran virtuel, signé par son auteur
│  └─ ffmpeg/                  # FFmpeg 9.0.2 réduit au moteur : trois DLL Windows, leurs licences, README
├─ packaging/
│  ├─ windows/                 # installateur NSIS, install service, règles pare-feu, désinstallation propre
│  ├─ ffmpeg/                  # build.sh, qui refait les DLL de vendor/ffmpeg depuis des sources vérifiées
│  └─ brand/                   # logo et icônes
├─ perf/                       # GATES.md (seuils chiffrés), relevés de référence
├─ docs/                       # ce dossier
└─ .github/workflows/          # ci (format, analyse statique, tests Windows et Linux, installateur), serveur (un binaire statique x86_64)
```

## 11. Interfaces entre composants (résumé)

- Interface <-> service (tube nommé `\\.\pipe\ZyrDesk`) : un message par ligne, un verbe puis des champs `clé=valeur`, lisible à l'oeil pour le diagnostic. Parmi les verbes : `standing` (empreinte de la machine, compilation du service, accès distant actif et ce qui l'empêche, confiance au réseau local, voies ouvertes), `reach` (ouvrir une voie vers un ordinateur ; la réponse donne la voie et le nom du tube de son lecteur), `hold` (dire quel processus la voie sert), `release`, `peers` (les ZyrDesk vus sur le réseau local), `sessions` (celles que le service tient, avec la machine visée et depuis quand), `hosting` (activer ou couper l'accès distant), `trusting` (accorder ou retirer la confiance au réseau local), `settings` et `choose` (ce à quoi ressemble une session ouverte d'ici), ce qu'une session demande à l'ordinateur d'en face par sa voie (`filmfar`, `hush`, `farscreen`, `farscreens`, `farpointer`, `sas`, `lock`, `far-journal`), et les gestes du compte (`account`, `attach`, `detach`, `devices`, `rename-device`, `revoke-device`). Une liste voyage en un message par élément, terminée par `done` : le canal garde sa forme et une liste vide se distingue d'un service qui s'est tu. Les champs inconnus sont ignorés, ceux ajoutés après coup se lisent avec un défaut plutôt qu'en refusant le message, et un verbe inconnu se nomme : une moitié du produit plus ancienne que l'autre perd ce qu'elle ne connaît pas, pas la conversation. La liste d'accès du tube donne le contrôle au compte système et aux administrateurs, la lecture et l'écriture à la personne connectée à la machine : sans elle, l'interface ne pourrait pas écrire un seul message. Le presse-papiers n'y passe pas et n'y passera pas : il est partagé par les deux services, et une page de pixels n'a rien à faire sur le canal que la fenêtre emploie pour une poignée de champs. Restent à ajouter : diagnostic, mises à jour, et les événements poussés.
- Service <-> serveur (HTTPS et WSS, JSON, seulement quand un lien de compte existe) : création de compte, connexion, rattachement et révocation d'appareils prouvés par leur clé, présence, contacts et partages, tickets de session et rendez-vous, laissez-passer de relais, révocations poussées. Détails : [SERVER.md](SERVER.md) §6.
- Service <-> moteur hôte (tube du moteur, compte système seul) : le service lance `zyrdeskd --serve-a-session <tube>` dans la session de l'écran. Sur le canal du service, il dit au moteur la taille d'un datagramme (`Setup`), l'écran à filmer (`Film`) et quand s'arrêter (`Stop`) ; le moteur dit ce qu'il sait encoder et quels écrans il voit (`Ready`, `Displays`), l'écran qu'il filme (`Filming`), ses ennuis en français (`Trouble`) et ce qu'il sert (`Serving`), sur quoi le service taille son tunnel. Codes de sortie : 0 session finie comme demandé, 2 tube fermé sans au revoir, 3 tube injoignable, 4 FFmpeg introuvable, 5 échec dont le journal dit la raison.
- Lecteur <-> moteur hôte (flux de contrôle, par les deux tubes et le tunnel) : le lecteur se présente (`Hello` : taille, cadence, débit, codec, pointeur dessiné dans l'image ou non, son, cadence d'un écran immobile, et ce qu'il sait décoder), change en direct (`Change`), demande une image clé (`Recover`), envoie touches et souris, mesure l'aller-retour (`Ping`) ; le moteur répond (`Welcome` : ce qu'il sait encoder, taille de son écran), annonce chaque nouveau flux (`Streaming`), renvoie son heure (`Pong`) et dit ses ennuis (`Notice`). Chacun dit au revoir (`Bye`). Détails : [MOTEUR.md](MOTEUR.md).
- Service <-> lecteur (canal du service sur le tube du lecteur) : chaque seconde, l'aller-retour du tunnel et si la route passe par le relais.
- Fenêtre <-> lecteur (même processus) : des appels qui n'attendent jamais (démarrer, changer, envoyer une touche, couper le son, lire les mesures, arrêter) et des événements en retour (nouveau flux, première image, message, fin).
- Tunnel (une connexion QUIC par session) : un seul flux fiable pour le moteur, ouvert par l'ordinateur qui regarde dès que son lecteur est sur son tube, qui porte le flux de contrôle entre le lecteur et le moteur hôte ; un flux par question pour le canal ZyrDesk, le produit qui se parle à lui-même (ouverture de la session, Ctrl+Alt+Suppr, verrouillage, enceintes, écrans, pointeur, journal, presse-papiers, morceaux des fichiers qu'on colle), un message de texte dans chaque sens ouvert par le numéro de version du dialecte ; des datagrammes pour l'image et le son, précédés d'un octet de canal. Détails : [NETWORK.md](NETWORK.md).
