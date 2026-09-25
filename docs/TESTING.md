# Stratégie de tests

Trois familles : tests classiques (CI), banc de performance (le juge de paix du projet), tests réels sur matériel. Le banc existe dès le jalon M2 : les décisions d'architecture réseau en dépendent, il ne peut pas arriver en fin de projet.

## 1. Tests classiques (CI, à chaque commit)

- Unitaires Rust par crate : framing du tunnel, budget de taille de paquet, machine à états de reprise, formats du moteur (paquets, correction d'erreurs, touches et souris, messages), mesures du lecteur, RPC du pipe, logique de tickets.
- Intégration sans GPU : broker en mémoire (comptes, enrôlement, tickets, présence, révocation) ; tunnel bouclé localement (deux extrémités en processus, trafic synthétique) ; contrôleur de congestion média sous profils de perte simulés ; service face à un moteur de remplacement sur une vraie liaison locale (un moteur qui démarre, qui part, qui meurt avant de joindre sa liaison) ; session entière du moteur hôte au lecteur à travers le vrai tunnel, avec un écran synthétique, x264 et Opus réels, avec et sans perte ([D223](DECISIONS.md)).
- Builds Debug et Release, lint (clippy) en erreur, format vérifié, audit des dépendances (licences + vulnérabilités connues).
- Tests cryptographiques : vecteurs pour la signature/vérification des tickets, épinglage des clés, rejet des tickets expirés/rejoués/mal signés, dérive d'horloge aux bornes (±5 min).
- Les essais du moteur chargent le vrai FFmpeg : sous Windows celui de `vendor/ffmpeg`, sous Linux une compilation des mêmes sources par `packaging/ffmpeg/build.sh linux`, désignée par `ZYR_FFMPEG_DIR`. Un essai qui ne trouve pas FFmpeg échoue en disant où il a cherché, il ne passe jamais sans avoir tourné. Une mise à jour de FFmpeg repasse tous ces essais et le banc complet avant fusion ([vendor/ffmpeg/README.md](../vendor/ffmpeg/README.md)).

## 2. Banc de performance (dès M2, puis en garde permanente)

Seuils G-* définis dans [ROADMAP.md](ROADMAP.md) (G-lat, G-loss, G-cpu, G-start, G-frame), stockés avec leurs bases de comparaison dans `perf/`.

Sources de mesure :

- Mesures du lecteur, cinq fois par seconde (la fiche « Statistiques » et `zyr-cli connect` en affichent la plupart) : images par seconde, temps chez l'hôte, aller-retour réseau et sa variance, temps de décodage et d'affichage, débit, images perdues en route ou remplacées avant l'affichage, temps depuis la dernière image, latence de bout en bout de la capture à l'affichage (`latency_ms`), p99 de l'intervalle entre images affichées (`frame_interval_p99_ms`).
- Compteurs du tunnel : paquets/octets par canal, datagrammes jetés (file pleine), RTT QUIC, chemin actif (direct/relais), migrations.
- Journal du moteur hôte (`engine.log`, étiquette `engine`) : encodeurs essayés et retenu, écran filmé, chaque flux ouvert (taille, cadence, codec, encodeur, débit), et le bilan des images capturées, encodées, envoyées, jetées, avec le temps moyen et maximal de conversion et d'encodage.
- CPU/GPU par processus (compteurs Windows).

Conditions réseau simulées : profils reproductibles de perte (0,5 %, 1 %, 2 %), latence (10, 25, 50 ms), gigue et limitation de débit, appliqués entre les deux PC de test (outil de conditionnement réseau côté Windows, scripté).

Latence bout en bout réelle (photon à photon) : procédure documentée pour l'opérateur : chronomètre milliseconde affiché sur l'hôte, filmé avec l'écran client par un téléphone à 240 im/s, lecture image par image, 10 mesures, médiane et p95. Simple, indiscutable, réalisable par un non-développeur.

Comparaisons obligatoires :

- M1 (moteurs pilotés, sans tunnel) contre couple Sunshine+Moonlight vanilla : sans objet depuis que ces moteurs ont quitté le produit ([D222](DECISIONS.md)).
- MZ (le moteur ZyrDesk) contre les anciens moteurs : latence de bout en bout, G-frame, qualité à débit égal, G-start ([ROADMAP.md](ROADMAP.md), jalon MZ).
- M2 tunnel contre M1 : seuils G-lat/G-loss/G-cpu.
- Chaque release ensuite contre la base de la release précédente : toute régression au-delà des marges bloque.

## 3. Tests réels GPU (M10 : automatisés en nocturne)

Un PC Windows physique dédié (NVIDIA d'abord, puis un deuxième AMD ou Intel) devient runner : matrice nocturne 1080p60 et 1440p60, H.264/HEVC/AV1, direct et relais, reconnexion, écran virtuel, audio, clavier/souris synthétiques. Rapport avec tendance des métriques ; alerte sur dérive.

Matrice matérielle visée à terme : NVIDIA vers NVIDIA (référence), AMD hôte, Intel hôte, GPU hybrides portables (cas support n°1), différentes générations d'encodeurs.

## 4. Scénarios manuels scriptés (par jalon)

Chaque jalon de [ROADMAP.md](ROADMAP.md) embarque son scénario pas à pas pour deux PC (documenté dans `docs/testing/` et exécutable par un non-développeur) : les critères de sortie listent exactement quoi mesurer et quoi observer. Exemples structurants : connexion depuis l'écran de connexion (M3), tuer l'interface en pleine session (M4), 4G vers domicile (M5), UDP bloqué puis débloqué (M6), câble débranché 10 s (M7), hôte sans écran (M9).

## 5. Interopérabilité

- Poignée de main de versions (canal de contrôle + broker) testée : paires incompatibles refusées proprement.
- N-1 systématique à chaque release : nouveau client contre ancien hôte, ancien client contre nouvel hôte.
- Le lecteur et le moteur hôte échangent leur version de moteur au premier message : deux versions différentes se refusent proprement, et c'est essayé.
