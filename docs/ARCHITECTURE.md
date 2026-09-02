# Architecture ZyrDesk

Ce document décrit l'architecture d'ensemble : ce qui est réutilisé des moteurs, les processus qui composent le produit sous Windows, les flux de connexion et le cycle de vie. Les choix sont issus d'une étude réelle des dépôts officiels Sunshine et Moonlight (code lu sur leurs branches principales, août 2026) : les faits cités (options, ports, comportements) ont été vérifiés à la source.

## 1. Faisabilité : ce qu'on réutilise, ce qu'on construit

Réutilisé tel quel (le cœur de la valeur) :

- Côté hôte, Sunshine : capture d'écran DXGI Desktop Duplication (y compris écran de connexion et invites UAC), encodage matériel NVENC natif, AMF et Quick Sync via leur FFmpeg, audio Opus, réception des entrées, protocole GameStream serveur, appairage.
- Côté client, Moonlight : protocole GameStream client, décodage matériel D3D11VA, présentation Direct3D 11 (swap chain flip discard, tearing maîtrisé), frame pacing éprouvé (Pacer synchronisé sur le vsync DXGI), audio faible latence, clavier/souris (mode relatif et absolu), overlay de statistiques très complet.

Construit par ZyrDesk (là où les moteurs ne font rien) :

- L'expérience produit : interface, comptes, liste d'appareils, présence, connexion en un clic.
- Le réseau moderne : tunnel chiffré unique, traversée NAT, relais de secours, reprise de session.
- L'intégration Windows propre : service, accès non supervisé, installateur, mises à jour.
- Le presse-papiers partagé (le protocole GameStream n'a aucun canal presse-papiers).

Points risqués identifiés et traités (détails dans [NETWORK.md](NETWORK.md) et [ROADMAP.md](ROADMAP.md)) :

- Le contrôle de congestion QUIC standard étranglerait la vidéo sous perte : contrôleur sur mesure obligatoire, validé par un banc de mesure dédié (jalon M2, critère GO/NO-GO).
- La pérennité de la signature du pilote d'écran virtuel tiers : testée dès le jalon M1.
- La dérive des interfaces de pilotage des moteurs (options, API) qui ne sont pas des contrats stables : suite de tests « contrat moteur » exécutée en continu.

Idées initiales abandonnées après étude :

- « Un seul exécutable » réel : un modèle multi-processus est plus robuste (une interface qui plante ne coupe pas la session, un service qui démarre avant l'ouverture de session, un moteur isolé). L'utilisateur ne voit toujours qu'un seul produit.
- Partir du fork Apollo pour l'écran virtuel : Apollo est un vrai fork divergent et en retard sur Sunshine officiel ; le prendre comme base détruirait notre capacité de mise à niveau upstream. On reste sur Sunshine officiel + pilote d'écran indépendant.
- Loger le tunnel dans le processus d'interface : voir §3, le tunnel vit dans le service.

## 2. Vue d'ensemble

```text
                              Serveur ZyrDesk
                    broker (comptes, appareils, présence,
                     tickets de session signés, jetons relais)
                          /                      \
                   WSS sortant                WSS sortant
                        /                          \
   PC CLIENT                                          PC HÔTE
   ZyrDesk.exe (interface, tray) ─ pipe ──┐   ┌── pipe ── ZyrDesk.exe (UI, tray,
   zyrdesk-session.exe (Moonlight dérivé) │   │            agent presse-papiers)
        │ loopback 127.77.x.y             │   │
        └── zyrdeskd (service) ═══════════╧═══╧═ zyrdeskd (service, SYSTEM)
             extrémité tunnel QUIC                extrémité tunnel + superviseur
                     ║                                  │ loopback 127.0.0.1
             UN SEUL flux UDP chiffré                   └─ zyrdesk-host-engine.exe
             direct OU via relais                          (Sunshine, session console)
                     ║
              relais ZyrDesk (ne voit que des paquets chiffrés, jamais les clés)
```

En fonctionnement normal, le flux média circule directement entre les deux PC. Le broker ne voit jamais un octet de média ; le relais, quand il sert, ne transporte que des paquets chiffrés qu'il ne peut pas déchiffrer.

Le serveur est facultatif. Sans lui, ZyrDesk se joint sur le réseau local, par un VPN ou par une adresse publique avec un port ouvert, sans compte ; avec lui, un compte, la présence, les contacts, les partages et la connexion en un clic d'où qu'on soit. Les deux façons cohabitent dans le même produit, et un service sans lien de compte ne contient aucun code qui parle au serveur. La conception entière est dans [SERVER.md](SERVER.md).

## 3. Les quatre processus (un seul produit visible)

| Processus | Rôle | Compte | Durée de vie |
|---|---|---|---|
| `ZyrDesk.exe` | Interface, dessinée par le produit lui-même dans sa propre fenêtre Win32, bouton flottant pendant une session, icône de zone de notification, agent presse-papiers côté hôte | Utilisateur connecté | Session utilisateur |
| `zyrdeskd.exe` | Service Windows : identité de l'appareil, lien broker, LES DEUX extrémités de tunnel (rôle client et rôle hôte), cycle de vie des moteurs, serveur IPC | LocalSystem | Démarre avec Windows |
| `zyrdesk-host-engine.exe` | Sunshine dérivé : capture, encodage, protocole, strictement lié à 127.0.0.1 | SYSTEM, dans la session console | Tant que « Autoriser l'accès distant » est actif |
| `zyrdesk-session.exe` | Moonlight dérivé : fenêtre vidéo, décodage, entrées | Utilisateur connecté | Une session distante |

Pourquoi le tunnel vit dans le service et pas dans l'interface :

- L'interface devient sans état : elle peut planter, être mise à jour ou être fermée sans couper une session en cours. Le lecteur `zyrdesk-session.exe` est lancé détaché, et l'interface se rattache à la session au redémarrage. Pendant une session, `ZyrDesk.exe` garde une seconde fenêtre, minuscule et toujours au-dessus : le bouton flottant. Fermer l'accueil ne la ferme pas, et c'est elle qui maintient le programme en vie tant que la session dure ([D16](DECISIONS.md)).
- Côté hôte, le service existe de toute façon (accès non supervisé avant ouverture de session) ; côté client, il porte l'identité de l'appareil et le lien broker. Un seul endroit gère donc l'authentification et les chemins réseau.
- Conséquence assumée : le service est requis même pour un usage purement client (installation avec droits administrateur). Un mode client sans service pourra être étudié plus tard.

## 4. Pilotage des moteurs (aucune interface upstream visible)

Sunshine est lancé par le service avec une configuration générée : liaison sur 127.0.0.1 uniquement, port de base tiré dans la plage 42000 à 42999 (coexistence automatique avec un éventuel Sunshine « normal » déjà installé), interface web verrouillée au PC local avec identifiants aléatoires, icône de zone de notification désactivée, liste d'applications réduite à « Desktop », état et journaux dans le dossier de données du produit. Aucune modification de son code : tout passe par son fichier de configuration, sa ligne de commande et son API locale. Détails : [engines/STRATEGY.md](engines/STRATEGY.md).

Moonlight est lancé par session avec un dossier d'état isolé par appareil distant (mécanisme « portable » officiel), entièrement piloté par sa ligne de commande (résolution, fps, bitrate, codec, taille de paquet, frame pacing, mode souris). La fenêtre de flux est une fenêtre native pure (titre et icône ZyrDesk).

## 5. Flux : activer l'accès distant

1. L'utilisateur active « Autoriser l'accès distant » dans l'interface.
2. L'interface le demande au service par le pipe local (action réservée aux administrateurs de la machine).
3. Le service génère la configuration Sunshine, lance le moteur dans la session console et surveille sa santé (sonde locale sur son endpoint `/serverinfo`).
4. S'il a un lien de compte, le service annonce l'appareil « disponible » au broker (connexion sortante persistante).
5. Au démarrage de Windows, si l'option est active, le service refait les étapes 3 et 4 sans aucune intervention : le PC est joignable depuis l'écran de connexion.

## 6. Flux : se connecter (appairage invisible)

1. Clic sur « Se connecter » : l'interface demande la session au service.
2. Le service client demande un ticket au broker ; le broker vérifie que les deux appareils appartiennent au même compte, ou qu'un partage les lie, et remet aux deux extrémités un ticket signé de courte durée, puis leur fait passer leurs candidats de chemin et, s'il en a un, l'adresse du relais.
3. Les deux services établissent le tunnel QUIC avec authentification mutuelle par clés d'appareil (voir [SECURITY.md](SECURITY.md)) : chemin relais d'abord si nécessaire, promotion vers le direct en parallèle (voir [NETWORK.md](NETWORK.md)).
4. Première connexion entre ces deux appareils seulement : le lecteur est lancé en attente d'un code, le service client tire ce code au sort et l'envoie au service hôte PAR LE TUNNEL authentifié (jamais via le broker), le service hôte le soumet à l'API locale de Sunshine, et les deux moteurs s'appairent. Invisible pour l'utilisateur, et conforme au protocole d'appairage officiel des moteurs. L'ordre est le mécanisme et non un détail d'écriture : le moteur hôte refuse un code tant que personne ne lui en demande un, si bien que le lecteur part le premier et que le résultat n'est attendu qu'après le voyage du code. Le service hôte réessaie quelques secondes, les deux moitiés arrivant dans un ordre que rien ne garantit, et l'attente côté client est bornée, le moteur n'y mettant lui-même aucune limite.
5. Le service client expose des ports locaux factices (adresse loopback stable 127.77.x.y par appareil distant) et rend cette adresse à l'appelant, qui lance `zyrdesk-session.exe` vers elle. Pour Moonlight, l'hôte est « local » ; en réalité chaque paquet traverse le tunnel.
6. L'appelant indique au service quel processus la voie sert désormais. Le service la referme dès que ce processus disparaît, et referme aussi une voie que personne n'a réclamée passé un court délai : une voie sans utilisateur est une fuite.
7. La fenêtre vidéo s'ouvre. L'interface affiche l'état (chemin direct ou relais, latence).

Pourquoi le lecteur est lancé par l'appelant et non par le service, contrairement au moteur hôte : côté client, quelqu'un est forcément connecté, et le lecteur doit s'afficher sur son bureau, avec ses droits et son périphérique audio. Le lancer depuis la session du service imposerait la même duplication de jeton que pour le moteur hôte, pour rien, et ferait tourner en compte système un programme qui n'a aucune raison de l'être. Le service reste malgré tout le seul propriétaire de la voie : il la tient, la surveille et la referme.

Sur un réseau local, les étapes 2 et 3 se passent du broker : les deux services s'annoncent en mDNS, chacun connaît donc l'adresse et l'empreinte de l'autre, et le service hôte admet les empreintes ainsi annoncées tant que la confiance au réseau local est accordée (D17). Il n'y a alors rien à recopier ni à taper d'un ordinateur à l'autre, ni avant la première session ni après.

## 7. Flux : reprise et résilience

- Coupure réseau courte : le tunnel QUIC absorbe (les datagrammes perdus sont couverts par la correction d'erreur du protocole vidéo). Coupure plus longue : fenêtre de reprise de 60 secondes, le lecteur est relancé dans la même géométrie pendant que le tunnel se rétablit ou migre de chemin ; l'utilisateur voit un état « reconnexion » discret.
- Verrouillage, déconnexion utilisateur, changement d'utilisateur côté hôte : le service détecte le changement de session console Windows et relance le moteur dans la nouvelle session (interruption de quelques secondes, reconnexion automatique du client).
- Chaque composant est surveillé : le service est relancé par Windows en cas de crash ; le moteur hôte par le service (avec backoff, et respect de son code de sortie spécial « arrêt volontaire ») ; le lecteur par le superviseur de session selon son code de sortie.
- Le lecteur rapporte des faits, pas un diagnostic : fin normale, session en échec, machine injoignable, appairage refusé (patch P-M5). Classer une panne en « perte réseau » ou « erreur fatale » est une décision produit, elle vit dans nos crates et jamais dans un moteur.

## 8. États dégradés : détectés et expliqués

Règle : jamais un écran noir sans explication. Le service détecte et l'interface explique :

- Session RDP entrante active sur l'hôte (la console physique est détachée) : « Hôte indisponible : une session Bureau à distance Windows est active ».
- Écran hôte éteint ou veille : pendant une session, le service maintient l'affichage éveillé (`ES_DISPLAY_REQUIRED`) ; un PC en veille est injoignable (Wake-on-LAN : plus tard).
- Changement rapide d'utilisateur en pleine session : redémarrage du flux vers la nouvelle session console, avec message.
- PC hôte sans écran branché : fonction « écran virtuel » (jalon M9) ; sans elle, message clair avec la marche à suivre.

## 9. Intégration Windows

- IPC local : named pipe `\\.\pipe\zyrdesk`, RPC typé + événements poussés (état de session, statistiques, présence). Chaque message est autorisé selon l'identité Windows de l'appelant (SID) : l'activation de l'hôte exige un administrateur.
- Secrets : clé privée de l'appareil et identifiants de l'interface web Sunshine protégés par DPAPI dans le profil SYSTEM (pas DPAPI « machine », déchiffrable par tout utilisateur local), fichiers ACLés SYSTEM + Administrateurs. Le lien de compte et son jeton d'appareil sont au service, sous la même protection que la clé ; la fenêtre ne tient aucun jeton.
- Pare-feu : des règles entrantes en UDP pour `zyrdeskd` uniquement, une par port qu'il écoute (tunnel, réseau local, voisinage). Les moteurs, liés au loopback, n'ont besoin de rien de l'extérieur, et reçoivent une règle qui le leur refuse : sans règle, Windows demande à la personne d'autoriser le programme, et la boîte qui pose la question affiche le nom que ce programme porte en lui. Une règle, dans un sens ou dans l'autre, veut dire aucune question. Règles nommées, supprimées à la désinstallation.
- Journaux : tous les composants écrivent dans le sous-dossier `logs` des données du produit (rotation), en temps universel et sous la même forme, le service et la fenêtre partageant le même écrivain. Chaque binaire porte l'empreinte du code dont il a été compilé, gravée par un script de compilation, et l'écrit en tête de sa trace : une panne se lit toujours contre la version qui l'a produite. La fenêtre rassemble les quatre traces sur un écran, sous cet entête, avec un bouton qui copie l'ensemble. `zyr-cli doctor` vérifie : encodeurs disponibles, GPU hybride, règle pare-feu, service actif, broker joignable, type de NAT, latence relais.
- Mises à jour : canal unique ; l'interface télécharge et vérifie le paquet, refuse d'appliquer pendant une session active, puis arrête le service, remplace les binaires (moteurs compris) et redémarre. Poignée de main de version entre interface, service et broker : les décalages de versions sont détectés proprement.
- Presse-papiers : canal ZyrDesk dédié dans le tunnel + agent dans `ZyrDesk.exe` côté hôte (seul un processus de la session utilisateur peut lire/écrire son presse-papiers). Texte uniquement en v1. Indisponible sur l'écran de connexion (assumé).
- Consentement et visibilité : pendant une session entrante, l'hôte affiche un indicateur (icône d'état + notification au début de session).

## 10. Organisation du dépôt

```text
ZyrDesk/
├─ Cargo.toml                  # workspace Rust
├─ crates/
│  ├─ zyr-proto/               # types partagés : chemins, journal horodaté, empreinte de compilation, réglages de session
│  ├─ zyr-transport/           # la connexion QUIC (quinn, un seul fichier le nomme), identité et empreintes, confiance TLS et épinglage, l'aiguilleur et ses sondes signées, contrôleur média, budget MTU ; à venir (M6) : la branche de relais
│  ├─ zyr-tunnel/              # pompes de ports : TCP<->stream, UDP<->datagramme ; canal ZyrDesk ; loopback 127.77.x.y
│  ├─ zyr-control/             # le dialecte entre la fenêtre et le service, sur le tube nommé
│  ├─ zyr-engine-host/         # superviseur Sunshine : config générée, lancement en session console, API locale, santé
│  ├─ zyr-engine-client/       # superviseur Moonlight : dossiers d'état par appareil, ligne de commande, statistiques, fichier suivi, codes de sortie
│  ├─ zyr-session/             # ouverture d'une session de bout en bout, partagée par l'interface et la ligne de commande
│  ├─ zyr-lan/                 # annonce mDNS de cet ordinateur, appel direct, découverte des autres
│  ├─ zyr-screen/              # l'écran virtuel : pilote, réveil, sommeil, arrangement des écrans
│  ├─ zyr-sound/               # le son de la session, dans le mélangeur de Windows
│  ├─ zyr-broker/              # ce que le service et le serveur se disent : messages, tickets et laissez-passer signés
│  ├─ zyr-account/             # le lien de compte, le rattachement, le canal vivant, la présence, le rendez-vous
│  ├─ zyrdeskd/                # binaire service Windows : registre des voies, serveur du tube, tous les tunnels, superviseur du moteur hôte
│  ├─ zyr-ui/                  # l'application : cœur Rust, écrans dessinés par le produit, journal, bouton flottant
│  └─ zyr-cli/                 # doctor, session sans UI, banc de mesure, bundle de diagnostic
├─ server/                     # zyr-server, le serveur facultatif (comptes et mise en relation, le relais à venir en M6 ; un binaire, AGPLv3), install.sh, README
├─ engines/
│  ├─ sunshine/                # submodule -> fork, tag upstream épinglé + une pile courte de patchs (patches/MANIFEST.md)
│  └─ moonlight-qt/            # submodule -> fork, tag upstream épinglé + une pile courte de patchs (patches/MANIFEST.md)
├─ patches/                    # miroirs .patch exportés par la CI + MANIFEST.md
├─ packaging/                  # installateur NSIS, install service, règles pare-feu, désinstallation propre
├─ perf/                       # GATES.md (seuils chiffrés), scripts, profils de perte, procédure photon-à-photon
├─ docs/                       # ce dossier
└─ .github/workflows/          # ci, build moteurs, serveur (un binaire statique par architecture), répétition de mise à niveau, smoke contrat moteurs
```

## 11. Interfaces entre composants (résumé)

- Interface <-> service (tube nommé `\\.\pipe\ZyrDesk`) : un message par ligne, un verbe puis des champs `clé=valeur`, lisible à l'oeil pour le diagnostic. Posé au jalon M4 avec `standing` (empreinte de la machine, compilation du service, accès distant actif et ce qui l'empêche, confiance au réseau local, voies ouvertes), `reach` (ouvrir une voie vers un ordinateur), `pair` (remettre à l'ordinateur d'en face le code que son moteur attend), `hold` (dire quel processus la voie sert), `release`, `peers` (les ZyrDesk vus sur le réseau local), `sessions` (celles que le service tient, avec la machine visée et depuis quand), `hosting` (activer ou couper l'accès distant), `trusting` (accorder ou retirer la confiance au réseau local), `settings` et `choose` (ce à quoi ressemble une session ouverte d'ici). Une liste voyage en un message par élément, terminée par `done` : le canal garde sa forme et une liste vide se distingue d'un service qui s'est tu. Les champs inconnus sont ignorés, ceux ajoutés après coup se lisent avec un défaut plutôt qu'en refusant le message, et un verbe inconnu se nomme : une moitié du produit plus ancienne que l'autre perd ce qu'elle ne connaît pas, pas la conversation. La liste d'accès du tube donne le contrôle au compte système et aux administrateurs, la lecture et l'écriture à la personne connectée à la machine : sans elle, l'interface ne pourrait pas écrire un seul message. Restent à ajouter : comptes, enrôlement, liste des appareils, presse-papiers, diagnostic, mises à jour, et les événements poussés.
- Service <-> serveur (HTTPS et WSS, JSON, seulement quand un lien de compte existe) : création de compte, connexion, rattachement et révocation d'appareils prouvés par leur clé, présence, contacts et partages, tickets de session et rendez-vous, laissez-passer de relais, révocations poussées. Détails : [SERVER.md](SERVER.md) §6.
- Service <-> Sunshine : processus + configuration générée + REST loopback (`/serverinfo` santé, `POST /api/pin` appairage).
- Superviseur <-> Moonlight : processus + ligne de commande + parsing des journaux/statistiques + codes de sortie.
- Tunnel (une connexion QUIC par session) : canal ZyrDesk = le produit qui se parle à lui-même (carte des ports du moteur d'en face, code d'appairage, et plus tard presse-papiers, statistiques, sonde de débit), une question par stream, un message de texte dans chaque sens ouvert par le numéro de version du dialecte ; les autres streams portent les flux TCP GameStream (HTTP, HTTPS, RTSP) ; les datagrammes portent la vidéo, le contrôle temps réel et l'audio, précédés d'un octet de canal. Détails : [NETWORK.md](NETWORK.md).
