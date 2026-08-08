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
   ZyrDesk.exe (UI Tauri, tray) ── pipe ──┐   ┌── pipe ── ZyrDesk.exe (UI, tray,
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

## 3. Les quatre processus (un seul produit visible)

| Processus | Rôle | Compte | Durée de vie |
|---|---|---|---|
| `ZyrDesk.exe` | Interface (Tauri), icône de zone de notification, agent presse-papiers côté hôte | Utilisateur connecté | Session utilisateur |
| `zyrdeskd.exe` | Service Windows : identité de l'appareil, lien broker, LES DEUX extrémités de tunnel (rôle client et rôle hôte), cycle de vie des moteurs, serveur IPC | LocalSystem | Démarre avec Windows |
| `zyrdesk-host-engine.exe` | Sunshine dérivé : capture, encodage, protocole, strictement lié à 127.0.0.1 | SYSTEM, dans la session console | Tant que « Autoriser l'accès distant » est actif |
| `zyrdesk-session.exe` | Moonlight dérivé : fenêtre vidéo, décodage, entrées | Utilisateur connecté | Une session distante |

Pourquoi le tunnel vit dans le service et pas dans l'interface :

- L'interface devient sans état : elle peut planter, être mise à jour ou être fermée sans couper une session en cours. Le lecteur `zyrdesk-session.exe` est lancé détaché, et l'interface se rattache à la session au redémarrage.
- Côté hôte, le service existe de toute façon (accès non supervisé avant ouverture de session) ; côté client, il porte l'identité de l'appareil et le lien broker. Un seul endroit gère donc l'authentification et les chemins réseau.
- Conséquence assumée : le service est requis même pour un usage purement client (installation avec droits administrateur). Un mode client sans service pourra être étudié plus tard.

## 4. Pilotage des moteurs (aucune interface upstream visible)

Sunshine est lancé par le service avec une configuration générée : liaison sur 127.0.0.1 uniquement, port de base tiré dans la plage 42000 à 42999 (coexistence automatique avec un éventuel Sunshine « normal » déjà installé), interface web verrouillée au PC local avec identifiants aléatoires, icône de zone de notification désactivée, liste d'applications réduite à « Desktop », état et journaux dans le dossier de données du produit. Aucune modification de son code : tout passe par son fichier de configuration, sa ligne de commande et son API locale. Détails : [engines/STRATEGY.md](engines/STRATEGY.md).

Moonlight est lancé par session avec un dossier d'état isolé par appareil distant (mécanisme « portable » officiel), entièrement piloté par sa ligne de commande (résolution, fps, bitrate, codec, taille de paquet, frame pacing, mode souris). La fenêtre de flux est une fenêtre native pure (titre et icône ZyrDesk).

## 5. Flux : activer l'accès distant

1. L'utilisateur active « Autoriser l'accès distant » dans l'interface.
2. L'interface le demande au service par le pipe local (action réservée aux administrateurs de la machine).
3. Le service génère la configuration Sunshine, lance le moteur dans la session console et surveille sa santé (sonde locale sur son endpoint `/serverinfo`).
4. Le service annonce l'appareil « disponible » au broker (connexion sortante persistante).
5. Au démarrage de Windows, si l'option est active, le service refait les étapes 3 et 4 sans aucune intervention : le PC est joignable depuis l'écran de connexion.

## 6. Flux : se connecter (appairage invisible)

1. Clic sur « Se connecter » : l'interface demande la session au service.
2. Le service client demande un ticket au broker ; le broker vérifie que les deux appareils appartiennent au même compte et remet aux deux extrémités un ticket signé de courte durée + les informations de chemin (candidats réseau, relais).
3. Les deux services établissent le tunnel QUIC avec authentification mutuelle par clés d'appareil (voir [SECURITY.md](SECURITY.md)) : chemin relais d'abord si nécessaire, promotion vers le direct en parallèle (voir [NETWORK.md](NETWORK.md)).
4. Première connexion entre ces deux appareils seulement : le service client génère un code PIN, l'envoie au service hôte PAR LE TUNNEL authentifié (jamais via le broker), le service hôte le soumet à l'API locale de Sunshine, et le lecteur s'appaire avec ce PIN. Invisible pour l'utilisateur, et conforme au protocole d'appairage officiel des moteurs.
5. Le service client expose des ports locaux factices (adresse loopback stable 127.77.x.y par appareil distant) et rend cette adresse à l'appelant, qui lance `zyrdesk-session.exe` vers elle. Pour Moonlight, l'hôte est « local » ; en réalité chaque paquet traverse le tunnel.
6. L'appelant indique au service quel processus la voie sert désormais. Le service la referme dès que ce processus disparaît, et referme aussi une voie que personne n'a réclamée passé un court délai : une voie sans utilisateur est une fuite.

Pourquoi le lecteur est lancé par l'appelant et non par le service, contrairement au moteur hôte : côté client, quelqu'un est forcément connecté, et le lecteur doit s'afficher sur son bureau, avec ses droits et son périphérique audio. Le lancer depuis la session du service imposerait la même duplication de jeton que pour le moteur hôte, pour rien, et ferait tourner en compte système un programme qui n'a aucune raison de l'être. Le service reste malgré tout le seul propriétaire de la voie : il la tient, la surveille et la referme.
6. La fenêtre vidéo s'ouvre. L'interface affiche l'état (chemin direct ou relais, latence).

## 7. Flux : reprise et résilience

- Coupure réseau courte : le tunnel QUIC absorbe (les datagrammes perdus sont couverts par la correction d'erreur du protocole vidéo). Coupure plus longue : fenêtre de reprise de 60 secondes, le lecteur est relancé dans la même géométrie pendant que le tunnel se rétablit ou migre de chemin ; l'utilisateur voit un état « reconnexion » discret.
- Verrouillage, déconnexion utilisateur, changement d'utilisateur côté hôte : le service détecte le changement de session console Windows et relance le moteur dans la nouvelle session (interruption de quelques secondes, reconnexion automatique du client).
- Chaque composant est surveillé : le service est relancé par Windows en cas de crash ; le moteur hôte par le service (avec backoff, et respect de son code de sortie spécial « arrêt volontaire ») ; le lecteur par le superviseur de session selon son code de sortie.
- Le lecteur rapporte des faits, pas un diagnostic : fin normale, session en échec, machine injoignable (patch P-M5). Classer une panne en « perte réseau » ou « erreur fatale » est une décision produit, elle vit dans nos crates et jamais dans un moteur.

## 8. États dégradés : détectés et expliqués

Règle : jamais un écran noir sans explication. Le service détecte et l'interface explique :

- Session RDP entrante active sur l'hôte (la console physique est détachée) : « Hôte indisponible : une session Bureau à distance Windows est active ».
- Écran hôte éteint ou veille : pendant une session, le service maintient l'affichage éveillé (`ES_DISPLAY_REQUIRED`) ; un PC en veille est injoignable (Wake-on-LAN : plus tard).
- Changement rapide d'utilisateur en pleine session : redémarrage du flux vers la nouvelle session console, avec message.
- PC hôte sans écran branché : fonction « écran virtuel » (jalon M9) ; sans elle, message clair avec la marche à suivre.

## 9. Intégration Windows

- IPC local : named pipe `\\.\pipe\zyrdesk`, RPC typé + événements poussés (état de session, statistiques, présence). Chaque message est autorisé selon l'identité Windows de l'appelant (SID) : l'activation de l'hôte exige un administrateur.
- Secrets : clé privée de l'appareil et identifiants de l'interface web Sunshine protégés par DPAPI dans le profil SYSTEM (pas DPAPI « machine », déchiffrable par tout utilisateur local), fichiers ACLés SYSTEM + Administrateurs. Côté interface, les jetons de compte vont dans le gestionnaire d'identifiants Windows de l'utilisateur.
- Pare-feu : une seule règle UDP entrante, pour `zyrdeskd` uniquement. Les moteurs, liés au loopback, n'en ont besoin d'aucune. Règles nommées, supprimées à la désinstallation.
- Journaux : tous les composants écrivent dans le sous-dossier `logs` des données du produit (rotation), bundle de diagnostic expurgé en un clic. `zyr-cli doctor` vérifie : encodeurs disponibles, GPU hybride, règle pare-feu, service actif, broker joignable, type de NAT, latence relais.
- Mises à jour : canal unique ; l'interface télécharge et vérifie le paquet, refuse d'appliquer pendant une session active, puis arrête le service, remplace les binaires (moteurs compris) et redémarre. Poignée de main de version entre interface, service et broker : les décalages de versions sont détectés proprement.
- Presse-papiers : canal ZyrDesk dédié dans le tunnel + agent dans `ZyrDesk.exe` côté hôte (seul un processus de la session utilisateur peut lire/écrire son presse-papiers). Texte uniquement en v1. Indisponible sur l'écran de connexion (assumé).
- Consentement et visibilité : pendant une session entrante, l'hôte affiche un indicateur (icône d'état + notification au début de session).

## 10. Organisation du dépôt

```text
ZyrDesk/
├─ Cargo.toml                  # workspace Rust
├─ crates/
│  ├─ zyr-proto/               # types partagés : RPC pipe, messages broker, framing tunnel, versions
│  ├─ zyr-transport/           # trait ZyrTransport ; implémentations iroh ET quinn ; contrôleur média ; budget MTU
│  ├─ zyr-tunnel/              # pompes de ports : TCP<->stream, UDP<->datagramme ; adresses loopback 127.77.x.y
│  ├─ zyr-engine-host/         # superviseur Sunshine : config générée, lancement en session console, /api/pin, santé
│  ├─ zyr-engine-client/       # superviseur Moonlight : dossiers d'état par appareil, CLI, parsing stats, codes de sortie
│  ├─ zyr-device/              # identité Ed25519, secrets DPAPI (profil SYSTEM), enrôlement
│  ├─ zyr-broker-client/       # client WSS présence/signalisation, tickets
│  ├─ zyrdeskd/                # binaire service Windows : registre de sessions, serveur pipe, tous les tunnels
│  ├─ zyr-ui/                  # app Tauri v2 (cœur Rust + web/) : design system, écrans, tray
│  └─ zyr-cli/                 # doctor, session sans UI, banc de mesure, bundle de diagnostic
├─ broker/zyr-broker/          # binaire unique axum + WSS + SQLite (AGPLv3) ; deploy/ (Docker, auto-hébergement)
├─ engines/
│  ├─ sunshine/                # submodule -> fork, tag upstream épinglé + 0 à 2 commits
│  └─ moonlight-qt/            # submodule -> fork, tag upstream épinglé + 6 commits maximum
├─ patches/                    # miroirs .patch exportés par la CI + MANIFEST.md
├─ packaging/                  # installateur NSIS, install service, règles pare-feu, désinstallation propre
├─ perf/                       # GATES.md (seuils chiffrés), scripts, profils de perte, procédure photon-à-photon
├─ docs/                       # ce dossier
└─ .github/workflows/          # ci, build moteurs, répétition de mise à niveau, smoke contrat moteurs
```

## 11. Interfaces entre composants (résumé)

- Interface <-> service (tube nommé `\\.\pipe\ZyrDesk`) : un message par ligne, un verbe puis des champs `clé=valeur`, lisible à l'oeil pour le diagnostic. Posé au jalon M4 avec `standing` (empreinte de la machine, accès distant actif, voies ouvertes), `reach` (ouvrir une voie vers un ordinateur), `hold` (dire quel processus la voie sert) et `release`. Les champs inconnus sont ignorés et un verbe inconnu se nomme, pour qu'une moitié du produit plus ancienne que l'autre le dise au lieu de se tromper en silence. La liste d'accès du tube donne le contrôle au compte système et aux administrateurs, la lecture et l'écriture à la personne connectée à la machine : sans elle, l'interface ne pourrait pas écrire un seul message. Restent à ajouter : comptes, enrôlement, activation de l'hôte, liste des appareils, sessions actives, presse-papiers, diagnostic, mises à jour, et les événements poussés.
- Service <-> broker (WSS + un peu de REST) : création de compte, connexion, enrôlement et révocation d'appareils, présence, demande/remise de tickets de session, jetons de relais, synchronisation de la liste d'appareils, révocations poussées.
- Service <-> Sunshine : processus + configuration générée + REST loopback (`/serverinfo` santé, `POST /api/pin` appairage).
- Superviseur <-> Moonlight : processus + ligne de commande + parsing des journaux/statistiques + codes de sortie.
- Tunnel (une connexion QUIC par session) : stream 0 = canal de contrôle ZyrDesk (versions, carte des ports, PIN, presse-papiers, statistiques, sonde de débit) ; streams suivants = flux TCP GameStream (HTTP, HTTPS, RTSP) ; datagrammes = vidéo, contrôle temps réel, audio, précédés d'un octet de canal. Détails : [NETWORK.md](NETWORK.md).
