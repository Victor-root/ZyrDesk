# Licences et conformité

État vérifié en septembre 2026, sur les fichiers de licence des sources exactes que ZyrDesk compile et livre (`vendor/ffmpeg/licenses/`, `vendor/ecran-virtuel/LICENSE`).

## 1. Licences des composants

| Composant | Licence | Note |
|---|---|---|
| ZyrDesk (application, service, moteur, outillage) | GPLv3 | Le dépôt est sous GPLv3 |
| ZyrDesk broker et relais | AGPLv3 | Copyleft réseau : un service hébergé modifié doit publier ses modifications |
| FFmpeg 9.0.2 (`vendor/ffmpeg`) | GPL version 2 ou ultérieure | Compilé par nous avec `--enable-gpl`, réduit à ce que le moteur emploie, par `packaging/ffmpeg/build.sh`. FFmpeg seul est sous LGPL ; x264 fait passer l'ensemble sous GPL, ce que les DLL rapportent elles-mêmes (`avcodec_license()`) |
| x264, compilé dans les DLL FFmpeg | GPL version 2 ou ultérieure | Commit épinglé de sa branche `stable` |
| Opus 1.6.1, compilé dans les DLL FFmpeg | BSD | Notice à conserver |
| libvpl 2.17.0 (répartiteur Intel), compilé dans les DLL FFmpeg | MIT | Notice à conserver |
| En-têtes AMF 1.5.2 (AMD) | MIT | Seuls les en-têtes servent à la compilation ; le pilote AMD est celui de la machine |
| nv-codec-headers n13.0.19.1 (NVIDIA) | MIT | Même chose : le pilote NVIDIA est celui de la machine. Chaque en-tête porte sa notice, recopiée dans un seul fichier |
| Virtual-Display-Driver (écran virtuel) | MIT | Signé par SignPath Foundation. Redistribué **tel quel** dans `vendor/ecran-virtuel/` avec sa licence, jamais modifié ni recompressé : ses trois fichiers sont signés comme un tout. Voir [ECRAN-VIRTUEL.md](ECRAN-VIRTUEL.md) |

La GPL version 2 « ou ultérieure » de FFmpeg et de x264 est compatible avec la GPLv3 de ZyrDesk : l'ensemble se distribue sous GPLv3.

Composants explicitement ÉCARTÉS pour raison de licence : pilotes d'écran virtuel propriétaires de solutions commerciales concurrentes (aucune autorisation de redistribution) ; pilote manettes libvirtualhid de LizardByte (licence source-available non commerciale avec clause anti-concurrence : incompatible avec ZyrDesk).

## 2. Pourquoi tout le produit est GPLv3

Le produit client/hôte est entièrement sous GPLv3, par choix : c'est cohérent avec un projet open source revendiqué, simple à expliquer, et juridiquement confortable. Le FFmpeg qu'il embarque est sous GPL, qui demande de toute façon une licence compatible à ce qui le charge. Le serveur, œuvre indépendante qui ne charge pas FFmpeg, est en AGPLv3 pour protéger aussi l'hébergement.

## 3. Obligations concrètes (liste de contrôle)

- Publier le code source correspondant de CHAQUE binaire distribué. Pour ZyrDesk, c'est le dépôt au commit de la publication. Pour les DLL FFmpeg, ce sont les sources exactes épinglées en tête de `packaging/ffmpeg/build.sh` (FFmpeg 9.0.2, x264 au commit noté, Opus 1.6.1, nv-codec-headers n13.0.19.1, en-têtes AMF 1.5.2, libvpl 2.17.0), plus ce script, qui les recompile. Chaque publication qui contient les DLL joint les archives de ces sources, plutôt que de compter sur les sites d'origine, qui peuvent changer ou disparaître ; et elles restent disponibles aussi longtemps que la publication est distribuée. Le détail est dans `vendor/ffmpeg/README.md`.
- Conserver toutes les notices de copyright et fichiers de licence des composants dans ce qui est distribué. `vendor/ffmpeg/licenses/` accompagne les DLL partout où elles sont livrées. Le pilote d'écran virtuel aussi : sa licence MIT voyage avec lui dans `vendor/ecran-virtuel/LICENSE`, et exige de conserver son avis de copyright dans toute redistribution.
- Ne modifier aucune source tierce sans le dire. Aujourd'hui aucune ne l'est : FFmpeg et ses dépendances sont compilés tels que publiés, seules les options de compilation sont les nôtres, écrites dans le script et rapportées par les DLL elles-mêmes (`avcodec_configuration()`).
- Écran « À propos » dans l'application : versions, crédits (FFmpeg, x264, Opus, l'écran virtuel), licences complètes consultables, lien vers le code source.
- Pas de restriction supplémentaire à la GPL dans nos conditions d'utilisation éventuelles.

## 4. Marques et posture vis-à-vis des projets tiers

- Les noms des projets et des fabricants (FFmpeg, x264, NVIDIA, AMD, Intel) se citent factuellement, dans les crédits et la documentation, jamais de manière à suggérer une affiliation ou un endossement.
- Nos exécutables et fenêtres portent des noms ZyrDesk. Le moteur hôte est `zyrdeskd.exe` lui-même, lancé pour une session ; le lecteur tourne dans `ZyrDesk.exe`.
- Le moteur a été conçu en étudiant comment Sunshine et Moonlight traitent chaque problème ([D219](DECISIONS.md)). La règle est d'y comprendre la technique sans reprendre leur code, et toute exception doit être annoncée avant ; aucune ne l'a été. Les créditer pour ce qu'ils ont appris au projet reste juste.

## 5. Brevets codecs (H.264/HEVC)

La GPL ne confère aucun droit sur les brevets des codecs (H.264/HEVC, pools type Via-LA), et les en-têtes AMF le rappellent dans leur propre licence. Réalité pratique pour un projet open source non commercial : l'encodage et le décodage matériels sont réalisés par les cartes graphiques (licences couvertes par les fabricants pour le matériel) ; l'encodeur logiciel x264, livré dans notre FFmpeg, est le point sensible en cas de distribution commerciale. Position ZyrDesk : projet open source gratuit ; le sujet est documenté ici pour être réévalué si une offre commerciale voyait le jour. AV1 (libre de redevances) est privilégié à mesure que le matériel le supporte.

## 6. Signature de code

Aucun certificat payant, aucun compte payant, aucune entité légale : contrainte de projet actée. Conséquences assumées :

- Binaires non signés au début : Windows SmartScreen affiche un avertissement « application non reconnue » au premier lancement. Documenté honnêtement pour les utilisateurs.
- Le pilote d'écran virtuel n'est PAS signé par nous : on installe (avec consentement et vérification d'empreinte) un pilote tiers open source déjà signé via SignPath Foundation. Si Windows cesse de l'accepter, la fonction se désactive proprement (voir ROADMAP M9).
- Quand le projet sera public et actif : candidature au programme gratuit de SignPath Foundation pour la signature des exécutables des projets open source. Gratuit, sans entité commerciale.
