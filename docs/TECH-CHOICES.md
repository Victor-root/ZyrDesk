# Choix de technologies

Chaque brique, le choix retenu, la raison, et les alternatives sérieusement considérées puis rejetées.

## Tableau de synthèse

| Brique | Choix | Raison principale |
|---|---|---|
| Cœur, service, tunnel, CLI, broker | Rust | Sûreté mémoire pour du code réseau exposé en permanence, performances, écosystème exact (tokio, quinn/iroh, axum, windows-service) |
| Interface | Tauri v2 (web + cœur Rust) | Plafond visuel maximal pour un rendu premium ; d'autres produits commerciaux de streaming réputés utilisent une UI en technologies web sans compromis sur la latence ; ~3 Mo ; updater intégré ; la vidéo ne traverse jamais la WebView |
| Moteur hôte | Sunshine officiel en processus enfant | Zéro modification visée : pilotage complet par config/CLI/REST vérifié sur le code |
| Moteur client | moonlight-qt officiel en processus enfant | Pipeline décodage D3D11VA + présentation D3D11 + frame pacing : des années de réglages Windows qu'on ne réécrit pas |
| Transport | iroh (plan B quinn) derrière un trait `ZyrTransport` | Traversée NAT + relais + migration de chemin intégrés et en production ; décision verrouillée par le banc M2 |
| IPC local | Named pipes (tokio) + RPC typé maison | Natif Windows, simple, contrôle d'accès par identité de l'appelant |
| Secrets | DPAPI (profil SYSTEM) côté service ; gestionnaire d'identifiants côté interface | Standard Windows, zéro dépendance exotique |
| Broker | axum + WebSocket + SQLite (Postgres possible ensuite) | Un binaire auto-hébergeable dès le premier jour ; SQLite suffit largement au début |
| Relais | Relais iroh auto-hébergé (plan B : forwarder quinn maison) | Ne voit que du chiffré, CPU minimal |
| Découverte LAN | mdns-sd | Éprouvé, sans runtime imposé |
| Mappage de ports | portmapper (UPnP + NAT-PMP + PCP) | Crate maintenue et utilisée en production par iroh |
| Comptes | Argon2id, jetons courts, TOTP | Standard moderne |
| Installateur | NSIS via Tauri + étapes personnalisées (service, pare-feu) | Voie recommandée par Tauri, extensible |

## Justifications détaillées et alternatives rejetées

Interface : Tauri v2 plutôt que...

- Slint (Rust natif) : sérieux et léger, mais atteindre un rendu réellement premium y coûte beaucoup plus d'effort qu'en web (écosystème de design réduit), et la version gratuite impose une attribution visible (sinon licence commerciale payante). Reconsidérable si la WebView devenait un problème avéré.
- Flutter desktop : très beau rendu possible, mais runtime lourd, deuxième langage (Dart) à vie dans le projet, et desktop Windows moins mûr que le mobile.
- Qt Quick : capable, mais liaison Rust (cxx-qt) pré-1.0, contraintes LGPL de déploiement à gérer, et style par défaut loin de la cible.
- egui / iced : pas au niveau visuel exigé sans effort massif ; le rendu en mode immédiat consomme du CPU en continu, exactement ce qu'un produit de streaming doit éviter.
- Electron : validerait aussi le besoin (c'est le choix de plusieurs concurrents commerciaux), mais 10 fois plus lourd que Tauri pour le même résultat, sans bénéfice puisque notre cœur est déjà en Rust.

Le point non négociable derrière ce choix : la fenêtre vidéo est un processus natif séparé (Direct3D via le moteur client). L'interface n'est jamais dans le chemin de la vidéo, donc sa technologie n'influence pas la latence.

Moteur client : processus moonlight-qt plutôt qu'un lecteur natif maison sur moonlight-common-c

- La bibliothèque cœur moonlight-common-c a une API C propre et un lecteur maison (fenêtre instantanée, intégration parfaite, reprise plus fine) est la bonne évolution v2/v3.
- Mais le decodeur/présentateur/frame pacing de moonlight-qt représente des années de cas particuliers Windows réglés (choix DXGI, tearing, pacing sur vsync réel, GPU hybrides). Le réécrire d'emblée = des mois pour retrouver la parité, avec régressions probables, en contradiction avec « ne pas réécrire les moteurs éprouvés ».
- La frontière processus + superviseur construite en v1 est exactement la couture qui permettra de remplacer le lecteur plus tard sans toucher au reste, avec le banc de performance pour prouver la parité.

Transport : iroh d'abord, quinn en plan B, et pas...

- webrtc-rs : lourd, architecture asynchrone contraignante, pas taillé pour notre cas.
- str0m (WebRTC sans E/S) : crédible côté serveur SFU, mais notre chemin principal (pair à pair) y est le moins éprouvé.
- boringtun (WireGuard) : en restructuration annoncée par son propre README ; et WireGuard seul n'apporte ni traversée NAT ni multiplexage fiable/non fiable.
- MoQ (Media over QUIC) : conçu pour la diffusion à grande échelle (~400 ms de latence cible), mauvais outil pour du 1:1 interactif sous 30 ms.
- TCP ou WebSocket pour le média : disqualifiés d'office (retransmissions et blocage tête de ligne incompatibles avec la latence cible).

Le risque réel du choix QUIC (contrôle de congestion qui étrangle un flux vidéo constant sous perte) est traité frontalement : contrôleur média sur mesure, critère GO/NO-GO au banc M2, profil de perte en CI permanente. Détails : [NETWORK.md](NETWORK.md).

Broker : SQLite d'abord

- Un fichier, zéro administration, sauvegarde triviale, largement suffisant pour des milliers d'appareils. Le code d'accès est écrit pour permettre Postgres quand le besoin réel arrive. Choisir Postgres maintenant serait de la complexité d'avance.

Windows : service en Rust maison plutôt que réutiliser le service de Sunshine

- Le service ZyrDesk fait bien plus que lancer le moteur (identité, broker, tunnels, IPC). On réplique en Rust le schéma éprouvé du service Sunshine (duplication du jeton SYSTEM vers la session console, lancement sur le bureau interactif, job object, relance au changement de session) : le schéma est documenté et testé chez eux, l'implémentation nous appartient.

Packaging : NSIS plutôt que MSI

- Recommandation actuelle de Tauri (un seul format), suffisant pour app + service + règles pare-feu + (plus tard) pilote optionnel. MSI reconsidérable si un besoin de déploiement d'entreprise apparaît.

## Contraintes transverses actées

- Aucun coût récurrent obligatoire : pas de certificat de signature payant, pas de compte développeur payant, pas de composant propriétaire. Les binaires partent non signés (avertissement SmartScreen documenté, normal pour un jeune projet open source) ; la signature gratuite pour projets open source (SignPath Foundation) sera demandée quand le projet sera public et actif.
- Toute brique tierce doit être : licence compatible (GPLv3/AGPLv3 côté produit), maintenue, et remplaçable (confinée derrière une interface à nous quand elle est structurante, comme le transport).
