# Stratégie de tests

Trois familles : tests classiques (CI), banc de performance (le juge de paix du projet), tests réels sur matériel. Le banc existe dès le jalon M2 : les décisions d'architecture réseau en dépendent, il ne peut pas arriver en fin de projet.

## 1. Tests classiques (CI, à chaque commit)

- Unitaires Rust par crate : framing du tunnel, calcul du budget MTU et de la taille de paquet, machine à états de reprise, génération de configuration moteur, parsing des statistiques du lecteur, RPC du pipe, logique de tickets.
- Intégration sans GPU : broker en mémoire (comptes, enrôlement, tickets, présence, révocation) ; tunnel bouclé localement (deux extrémités en processus, trafic synthétique) ; contrôleur de congestion média sous profils de perte simulés ; superviseurs face à des moteurs factices (bons/mauvais codes de sortie, crashs, blocages).
- Builds Debug et Release, lint (clippy) en erreur, format vérifié, audit des dépendances (licences + vulnérabilités connues).
- Tests cryptographiques : vecteurs pour la signature/vérification des tickets, épinglage des clés, rejet des tickets expirés/rejoués/mal signés, dérive d'horloge aux bornes (±5 min).
- Suite « contrat moteur » (voir [engines/UPGRADING.md](engines/UPGRADING.md)) : à chaque bump de submodule moteur + job mensuel de répétition de mise à niveau.

## 2. Banc de performance (dès M2, puis en garde permanente)

Seuils G-* définis dans [ROADMAP.md](ROADMAP.md) (G-lat, G-loss, G-cpu, G-start, G-frame), stockés avec leurs bases de comparaison dans `perf/`.

Sources de mesure :

- Statistiques du lecteur (overlay/journaux Moonlight) : fps réseau/décodage/rendu, latence hôte min/max/moyenne, pertes réseau, pertes par jitter, latence réseau moyenne et variance, temps de décodage, délai de file, temps de rendu.
- Compteurs du tunnel : paquets/octets par canal, datagrammes jetés (file pleine), RTT QUIC, chemin actif (direct/relais), migrations.
- Journaux Sunshine : encodeur retenu, fps capturés/encodés, temps d'encodage.
- CPU/GPU par processus (compteurs Windows).

Conditions réseau simulées : profils reproductibles de perte (0,5 %, 1 %, 2 %), latence (10, 25, 50 ms), gigue et limitation de débit, appliqués entre les deux PC de test (outil de conditionnement réseau côté Windows, scripté).

Latence bout en bout réelle (photon à photon) : procédure documentée pour l'opérateur : chronomètre milliseconde affiché sur l'hôte, filmé avec l'écran client par un téléphone à 240 im/s, lecture image par image, 10 mesures, médiane et p95. Simple, indiscutable, réalisable par un non-développeur.

Comparaisons obligatoires :

- M1 (moteurs pilotés, sans tunnel) contre couple Sunshine+Moonlight vanilla : notre pilotage ne doit rien coûter (±5 %).
- M2 tunnel contre M1 : seuils G-lat/G-loss/G-cpu.
- Chaque release ensuite contre la base de la release précédente : toute régression au-delà des marges bloque.

## 3. Tests réels GPU (M10 : automatisés en nocturne)

Un PC Windows physique dédié (NVIDIA d'abord, puis un deuxième AMD ou Intel) devient runner : matrice nocturne 1080p60 et 1440p60, H.264/HEVC/AV1, direct et relais, reconnexion, écran virtuel, audio, clavier/souris synthétiques. Rapport avec tendance des métriques ; alerte sur dérive.

Matrice matérielle visée à terme : NVIDIA vers NVIDIA (référence), AMD hôte, Intel hôte, GPU hybrides portables (cas support n°1), différentes générations d'encodeurs.

## 4. Scénarios manuels scriptés (par jalon)

Chaque jalon de [ROADMAP.md](ROADMAP.md) embarque son scénario pas à pas pour deux PC (documenté dans `perf/` et exécutable par un non-développeur) : les critères de sortie listent exactement quoi mesurer et quoi observer. Exemples structurants : connexion depuis l'écran de connexion (M3), tuer l'interface en pleine session (M4), 4G vers domicile (M5), UDP bloqué puis débloqué (M6), câble débranché 10 s (M7), hôte sans écran (M9).

## 5. Interopérabilité

- Poignée de main de versions (canal de contrôle + broker) testée : paires incompatibles refusées proprement.
- N-1 systématique à chaque release : nouveau client contre ancien hôte, ancien client contre nouvel hôte.
- Mise à niveau moteur : suite contrat moteur + banc complet avant merge.
