# Licences et conformité

État vérifié en août 2026 sur les dépôts officiels (fichiers de licence lus à la source).

## 1. Licences des composants

| Composant | Licence | Note |
|---|---|---|
| Sunshine | GPL-3.0-only (sans clause « ou ultérieure ») | Fichier LICENSE verbatim GPLv3 ; identifiant SPDX déclaré dans leur build |
| moonlight-qt | GPL-3.0 | |
| moonlight-common-c | GPL-3.0 | LICENSE.txt présent et lu (une vieille discussion « licence à clarifier » traîne chez eux, mais le fichier fait foi) |
| FFmpeg embarqué par moonlight-qt | LGPL (build sans option GPL) | Vérifié dans leur script de build |
| FFmpeg embarqué par Sunshine | Build maison LizardByte | À inventorier précisément au moment des builds M4 (options d'encodeurs logiciels x264/x265 = composants GPL) |
| Qt (moonlight-qt) | LGPLv3 (édition open source) | Lien dynamique : DLL remplaçables par l'utilisateur, obligation LGPL satisfaite |
| SDL2 | zlib | |
| Dépendances MIT des moteurs (ViGEmClient, Simple-Web-Server, inputtino, tray, nvapi) | MIT | Notices à conserver |
| Virtual-Display-Driver (écran virtuel) | MIT | Signé par SignPath Foundation. Redistribué **tel quel** dans `vendor/ecran-virtuel/` avec sa licence, jamais modifié ni recompressé : ses trois fichiers sont signés comme un tout. Voir [ECRAN-VIRTUEL.md](ECRAN-VIRTUEL.md) |
| ZyrDesk (application, moteurs forkés, outillage) | GPLv3 | Le dépôt est déjà sous GPLv3 |
| ZyrDesk broker et relais | AGPLv3 | Copyleft réseau : un service hébergé modifié doit publier ses modifications |

Composants explicitement ÉCARTÉS pour raison de licence : pilotes d'écran virtuel propriétaires de solutions commerciales concurrentes (aucune autorisation de redistribution) ; pilote manettes libvirtualhid de LizardByte (licence source-available non commerciale avec clause anti-concurrence : incompatible avec ZyrDesk).

## 2. Pourquoi tout le produit est GPLv3

Sunshine et Moonlight sont GPLv3 : toute distribution d'une version dérivée (nos forks rebrandés) impose la GPLv3 et la publication du code source correspondant. Plutôt que de jouer sur des frontières de processus pour garder des morceaux sous une autre licence, ZyrDesk assume : TOUT le produit client/hôte est GPLv3. C'est cohérent avec le projet (open source revendiqué), simple à expliquer, et juridiquement confortable. Le serveur, œuvre indépendante qui ne contient pas de code des moteurs, est en AGPLv3 pour protéger aussi l'hébergement.

## 3. Obligations concrètes (liste de contrôle)

- Publier le code source correspondant de CHAQUE binaire distribué, y compris les forks moteurs exacts (tags + nos commits) : garanti par les forks publics épinglés et les miroirs de patchs dans `patches/`.
- Conserver toutes les notices de copyright et fichiers de licence des composants dans les binaires distribués (dossier `licenses/` de l'installateur). Le pilote d'écran virtuel y compris : sa licence MIT voyage avec lui dans `vendor/ecran-virtuel/LICENSE`, et exige de conserver son avis de copyright dans toute redistribution.
- Marquer nos modifications des moteurs (GPL §5a) : fait par les messages de commits `zyr:` et le manifeste de patchs.
- Écran « À propos » dans l'application : versions, crédits (« ZyrDesk s'appuie sur les projets open source Sunshine et Moonlight »), licences complètes consultables, lien vers le code source. La discrétion des moteurs dans l'expérience utilisateur ne dispense PAS des mentions légales : elles vivent ici.
- Pas de restriction supplémentaire à la GPL dans nos conditions d'utilisation éventuelles.

## 4. Marques et posture vis-à-vis d'upstream

- « Sunshine », « Moonlight », « LizardByte » : ne jamais les utiliser dans le nom, le marketing ou l'interface de manière à suggérer une affiliation ou un endossement. Les citer factuellement dans les crédits et la documentation est correct et souhaitable.
- Nos exécutables et fenêtres portent des noms ZyrDesk. Le nom du fichier ne suffit pas : le gestionnaire des tâches lit le nom de produit compilé dans le binaire, et c'est ce champ que nos compilations posent.
- Posture recommandée : crédit visible et contributions upstream quand nos patchs peuvent leur servir. Une bonne relation avec les mainteneurs des moteurs est un atout stratégique du projet.

## 5. Brevets codecs (H.264/HEVC)

La documentation officielle de Sunshine le dit explicitement : la GPL ne confère aucun droit sur les brevets des encodeurs (H.264/HEVC, pools type Via-LA). Réalité pratique pour un projet open source non commercial : l'encodage/décodage matériel est réalisé par les GPU (licences couvertes par les fabricants pour le matériel), les encodeurs logiciels (x264/x265) sont le point sensible en cas de distribution commerciale. Position ZyrDesk : projet open source gratuit, situation identique à celle de Sunshine et Moonlight eux-mêmes ; le sujet est documenté ici pour être réévalué si une offre commerciale voyait le jour. AV1 (libre de redevances) est privilégié à mesure que le matériel le supporte.

## 6. Signature de code

Aucun certificat payant, aucun compte payant, aucune entité légale : contrainte de projet actée. Conséquences assumées :

- Binaires non signés au début : Windows SmartScreen affiche un avertissement « application non reconnue » au premier lancement. Documenté honnêtement pour les utilisateurs.
- Le pilote d'écran virtuel n'est PAS signé par nous : on installe (avec consentement et vérification d'empreinte) un pilote tiers open source déjà signé via SignPath Foundation. Si Windows cesse de l'accepter, la fonction se désactive proprement (voir ROADMAP M9).
- Quand le projet sera public et actif : candidature au programme gratuit de SignPath Foundation pour la signature des exécutables des projets open source. Gratuit, sans entité commerciale.
