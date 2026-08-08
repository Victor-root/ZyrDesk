# Stratégie moteurs : frontières avec Sunshine et Moonlight

Objectif : utiliser les moteurs officiels comme fondations invisibles, avec un nombre de points de contact volontairement minimal, pour que leurs mises à niveau restent simples pendant des années.

Règle absolue : AUCUNE fonctionnalité ZyrDesk ne vit dans le code des moteurs. Un patch ne peut que retirer de l'habillage (fenêtre, marque) ou exposer un interrupteur. Toute logique produit vit dans nos crates Rust et pilote les moteurs par leurs interfaces officielles : fichier de configuration, ligne de commande, API REST locale, journaux, codes de sortie.

## 1. Intégration retenue : forks légers en submodules

- Deux forks GitHub : `zyrdesk-sunshine` et `zyrdesk-moonlight-qt`.
- Dans chaque fork, une branche `zyr/<tag-upstream>` = le tag officiel épinglé + notre petite pile de commits (0 à 2 pour Sunshine, 6 maximum pour Moonlight).
- Le monorepo les référence en submodules (`engines/sunshine`, `engines/moonlight-qt`). Les submodules imbriqués des moteurs (Moonlight embarque moonlight-common-c, qui embarque enet ; Sunshine embarque un arbre third-party complet) restent intacts.
- À chaque bump de submodule, la CI exporte le delta en fichiers `.patch` lisibles dans `patches/` du monorepo, avec un manifeste (identifiant, raison d'être, candidat à une contribution upstream ou non). On voit ainsi notre écart complet sans ouvrir les forks, et cela documente publiquement nos modifications (obligation GPL de clarté sur les sources correspondantes).

Alternatives rejetées :

- git subtree : ne gère pas les submodules imbriqués des moteurs ; il faudrait les aplatir et perdre leur outillage de mise à jour.
- vendor + fichiers de patchs appliqués au build : les patchs pourrissent ; on perd la fusion à 3 voies de git, qui est précisément ce qui rend les rebases (y compris assistés par IA) fiables.
- fork lourd (type Apollo) : c'est exactement ce qu'on veut éviter ; Apollo diverge fort et reste en retard sur Sunshine officiel.

## 2. Versions épinglées

- Sunshine : version plancher `v2026.516.143833`. C'est la première version corrigeant une faille critique de validation de certificats clients (score CVSS 9.8) : on ne construit jamais sur une version antérieure.
- Moonlight : dernière release stable `v6.1.0`. Elle porte déjà l'AV1 et le YUV 4:4:4, les deux fonctions dont dépendent nos objectifs de qualité. Le choix de cette version contre la branche principale est motivé en D14.
- Accélérateur assumé pour démarrer : jusqu'au jalon M4, les binaires officiels préconstruits (renommés) peuvent être utilisés tels quels pour prototyper. Nos propres builds reproductibles (CI MSYS2 pour Sunshine, MSVC + Qt pour Moonlight) deviennent obligatoires à partir de M4 (rebranding + hygiène GPL).

## 3. Points de contact avec Sunshine (objectif : zéro patch)

Tout ce dont ZyrDesk a besoin existe déjà dans Sunshine officiel :

| Besoin | Mécanisme officiel |
|---|---|
| Isolement réseau total | `bind_address = 127.0.0.1` (s'applique à tous ses serveurs) |
| Ports sans collision | `port = <base>` tirée dans 42000 à 42999 ; offsets fixes : HTTPS -5, HTTP +0, interface web +1, vidéo UDP +9, contrôle UDP +10, audio UDP +11, RTSP TCP +21 |
| Interface web neutralisée | `origin_web_ui_allowed = pc` (accès local uniquement ; elle ne peut pas être désactivée car elle porte l'API) + identifiants aléatoires 32 octets régénérés à chaque démarrage du service, posés par `--creds <user> <pass>`, stockés via DPAPI |
| Pas d'icône de zone de notification | `system_tray = disabled` |
| Capture du secure desktop (UAC, écran de connexion) | `capture = ddx` (DXGI Desktop Duplication ; l'autre backend WGC ne capture pas les invites UAC) |
| Chiffrement interne inutile en loopback | `lan_encryption_mode = 0` (le tunnel chiffre déjà tout ; mode « paranoïaque » possible en passant à 2) |
| Pas d'UPnP côté moteur | `upnp = off` (le service ZyrDesk gère les mappages de ports lui-même) |
| Liste d'applications | `apps.json` généré, réduit à « Desktop » |
| État, identifiants, journaux hors du dossier d'installation | options de chemins (`file_state`, `credentials_file`, `log_path`) vers le dossier de données du produit |
| Écran cible et GPU | `output_name`, `adapter_name` |
| Écran virtuel (plus tard) | options `dd_*` (résolution/fréquence du client, `ensure_only_display`, restauration à la déconnexion) : prévues par Sunshine pour piloter un pilote d'écran tiers |
| Santé | `GET /serverinfo` sur son port HTTP local |
| Appairage automatisé | `POST /api/pin` avec `{"pin": "...", "name": "..."}` sur son port web local (authentification Basic ; l'exemption CSRF pour les clients sans en-tête Origin est un comportement documenté) |
| Surcharges ponctuelles | tout paramètre peut aussi être passé en ligne de commande `nom=valeur` |
| Arrêt propre | signal console + respect de son code de sortie spécial « arrêt volontaire » (sinon son contrat de supervision attend un respawn) |

Contingences identifiées (patchs UNIQUEMENT si la vérification M1 l'exige) :

- P-S1 : désactiver l'annonce mDNS si Sunshine s'annonce sur le réseau malgré la liaison loopback et qu'aucune option ne le contrôle.
- Aucune autre contingence connue.

## 4. Points de contact avec Moonlight (6 micro-patchs maximum)

Mécanismes officiels utilisés :

| Besoin | Mécanisme officiel |
|---|---|
| État isolé par appareil distant | fichier `portable.dat` à côté de l'exécutable : tout l'état (réglages, identité client, hôtes appairés) part dans un dossier local que nous plaçons dans `devices\<id>` sous les données du produit |
| Session sans interface Moonlight | commande `stream <hôte> "Desktop"` avec options : `--resolution WxH`, `--fps N`, `--bitrate K`, `--packet-size B` (force le mode « local », minimum 1025), `--display-mode fullscreen|windowed|borderless`, `--video-codec auto|H.264|HEVC|AV1`, `--video-decoder hardware`, `--frame-pacing`, `--absolute-mouse`, `--capture-system-keys`, `--performance-overlay`, `--hdr`, `--yuv444` |
| Appairage sans interaction | commande `pair <hôte> --pin NNNN` |
| Statistiques | overlay de performances + journaux (débit d'images réseau/décodage/rendu, latence hôte, pertes, jitter, temps de décodage, délai de file, temps de rendu) |
| Réglages fins non exposés en CLI | clés du fichier INI portable (écrites avant lancement, jamais pendant une session) |

Pile de patchs prévue (le manifeste `patches/MANIFEST.md` fait foi) :

| Id | Patch | Taille attendue | Statut |
|---|---|---|---|
| P-M1 | Suppression de la fenêtre de chargement en lancement ligne de commande, erreurs vers la sortie d'erreur | ~40 à 60 lignes | Confirmé nécessaire sur machine réelle : la piste sans patch, qui neutralisait la couche graphique du moteur, est écartée faute d'alternative embarquée dans sa version Windows |
| P-M2 | Rebranding : titre de la fenêtre vidéo, icônes, noms d'organisation/produit, métadonnées de l'exécutable (~8 emplacements identifiés, mécaniques) | mécanique | Requis à partir de M4 |
| P-M5 | Codes de sortie distincts (sortie utilisateur / perte réseau / erreur fatale) câblés dans la fin de session | ~20 lignes | Requis pour la reprise automatique (fusionne en pratique avec P-M1) |
| P-M3 | Ligne de statistiques périodique lisible par machine sur stdout | ~60 lignes | Seulement si les journaux existants ne suffisent pas au banc de mesure |
| P-M4 | Interrupteur pour ne pas demander le chiffrement vidéo interne (Moonlight le demande par défaut sur CPU avec accélération AES) | ~10 lignes | Contingence : seulement si la vérification M1 montre que Sunshine chiffre quand même en mode 0 sur loopback (double chiffrement inutile) |

## 5. Schéma d'adressage loopback côté client

Chaque appareil distant reçoit une adresse loopback stable `127.77.x.y` (Windows accepte tout 127.0.0.0/8 sans configuration), avec le même port de base que le moteur hôte en face et une correspondance de ports 1:1. Avantages : l'état Moonlight (hôtes appairés) reste cohérent dans le temps, deux sessions sortantes simultanées ne se marchent pas dessus, et les journaux restent lisibles.

## 6. Coexistence avec de vrais Sunshine/Moonlight installés

- Ports moteur dans 42000 à 42999 : aucune collision avec un Sunshine standard (base 47989).
- État totalement séparé (dossiers ZyrDesk, mode portable Moonlight) : aucune interaction avec les réglages ou appairages d'installations existantes.
- Le service ZyrDesk ne touche jamais aux services ou processus d'un Sunshine officiel présent sur la machine.
