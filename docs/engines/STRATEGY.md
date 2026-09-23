# Stratégie moteurs : frontières avec Sunshine et Moonlight

> **Stratégie de transition.** ZyrDesk écrit son propre moteur ([D219](../DECISIONS.md), jalon MZ de [ROADMAP.md](../ROADMAP.md)). Ce qui suit reste la règle pour Sunshine et Moonlight tant qu'ils servent de filet, et tombe avec eux au débranchement.

Objectif : utiliser les moteurs officiels comme fondations invisibles, avec un nombre de points de contact volontairement minimal, pour que leurs mises à niveau restent simples pendant des années.

Règle absolue : AUCUNE fonctionnalité ZyrDesk ne vit dans le code des moteurs. Un patch ne peut que retirer de l'habillage (fenêtre, marque), exposer un interrupteur, ou corriger un défaut du moteur qui se mesure sans ZyrDesk et se propose en amont. Toute logique produit vit dans nos crates Rust et pilote les moteurs par leurs interfaces officielles : fichier de configuration, ligne de commande, API REST locale, journaux, codes de sortie.

## 0. La règle qui rend une mise à niveau possible : la marque `zyr:`

**Il n'y a pas de plafond au nombre de patchs.** Il y en avait un, posé au début du projet quand personne ne savait encore combien de fonctionnalités le produit porterait. Il a été franchi une dizaine de fois, chaque fois pour une bonne raison, et il n'a jamais rien signalé d'utile : il est levé le 2026-09-15. Ce qui le remplace n'est pas un chiffre, c'est une discipline, et c'est elle qui décide si une mise à niveau reste faisable.

**Toute ligne de notre fait à l'intérieur du code d'un moteur porte un commentaire commençant par `zyr:` qui dit pourquoi elle est là.** Sans exception : pour une ligne comme pour deux cents, pour un nombre changé comme pour une fonction entière. C'est ce qui permet à un seul `grep -rn "zyr:"` de rendre l'écart complet avec son intention, sans ouvrir un seul diff.

Trois règles vont avec, qui n'en sont que la conséquence :

- Tout commit dans un fork commence par `zyr:` dans son message. C'est ce qui rend la pile rebasable d'un `git rebase --onto`.
- Tout fichier qui n'existe que pour nous s'appelle `zyr*` et vit à côté de ses voisins upstream. Un fichier à nous n'entre jamais en conflit : upstream n'y touchera jamais. C'est la forme à préférer chaque fois qu'un patch a le choix.
- Tout patch a son entrée dans [`patches/MANIFEST.md`](../../patches/MANIFEST.md), avec sa raison d'être et son plan de sortie. Le manifeste est mis à jour dans le même mouvement que le patch, jamais après coup.

**Pourquoi tout ceci, en une phrase.** Le jour où un moteur est rebasé sur une version amont plus récente, git donne le *quoi* de chaque patch et le retrouve tout seul. Le *pourquoi* n'existe que dans ces commentaires et dans le manifeste. Un patch dont le pourquoi est perdu ne se rebase pas : il se réinvente, contre du code qui a changé, sans savoir ce qu'il devait obtenir.

**Ceci s'adresse autant à une IA qu'à une personne.** Une IA qui reprend ce dépôt sans le contexte de ce qui précède n'a que ces marques pour retrouver l'écart. Les poser est donc une obligation, pas un confort, et une modification d'un moteur sans marque `zyr:` est un défaut au même titre qu'un test qui ne passe pas.

## 1. Intégration retenue : forks légers en submodules

- Deux forks GitHub : [ZyrDesk-Sunshine](https://github.com/Victor-root/ZyrDesk-Sunshine) et [ZyrDesk-Moonlight](https://github.com/Victor-root/ZyrDesk-Moonlight).
- Dans chaque fork, une branche `zyr/<tag-upstream>` = le tag officiel épinglé + notre pile de commits, tous préfixés `zyr:`.
- Le monorepo les référence en submodules (`engines/sunshine`, `engines/moonlight-qt`). Les submodules imbriqués des moteurs (Moonlight embarque moonlight-common-c, qui embarque enet ; Sunshine embarque un arbre third-party complet) restent intacts.
- L'écart complet se lit dans [`patches/MANIFEST.md`](../../patches/MANIFEST.md) : identifiant, raison d'être, commits, candidat à une contribution upstream ou non. Le manifeste dit le pourquoi ; les forks, publics, portent le quoi.
- L'obligation GPL est remplie par les forks eux-mêmes : ils sont publics, la branche `zyr/<tag>` porte la source correspondant exactement aux binaires distribués, et le journal du produit écrit le numéro de compilation des moteurs, qui la désigne. Rien d'autre n'est dû.

Alternatives rejetées :

- git subtree : ne gère pas les submodules imbriqués des moteurs ; il faudrait les aplatir et perdre leur outillage de mise à jour.
- vendor + fichiers de patchs appliqués au build : les patchs pourrissent ; on perd la fusion à 3 voies de git, qui est précisément ce qui rend les rebases (y compris assistés par IA) fiables.
- fork lourd (type Apollo) : c'est exactement ce qu'on veut éviter ; Apollo diverge fort et reste en retard sur Sunshine officiel.

## 2. Versions épinglées

- Sunshine : version plancher `v2026.516.143833`. C'est la première version corrigeant une faille critique de validation de certificats clients (score CVSS 9.8) : on ne construit jamais sur une version antérieure.
- Moonlight : dernière release stable `v6.1.0`. Elle porte déjà l'AV1 et le YUV 4:4:4, les deux fonctions dont dépendent nos objectifs de qualité. Le choix de cette version contre la branche principale est motivé en D14.
- Accélérateur assumé pour démarrer : jusqu'au jalon M4, les binaires officiels préconstruits (renommés) pouvaient être utilisés tels quels pour prototyper. Depuis M4, les deux moteurs sortent de nos propres compilations (MSYS2 + GCC pour Sunshine, MSVC + Qt pour Moonlight), pour le rebranding et l'hygiène GPL.

## 3. Points de contact avec Sunshine

À une ligne près, tout ce dont ZyrDesk a besoin existe déjà dans Sunshine officiel :

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
| Bureau distant à la forme de la session, et remis après | `dd_configuration_option = ensure_active` (sans quoi les trois suivantes ne sont pas même lues), `dd_resolution_option = auto` et `dd_refresh_rate_option = auto` (la taille et la fréquence demandées par le client), `dd_config_revert_on_disconnect = enabled` (le moteur attend sinon l'arrêt de l'application diffusée, et la nôtre est le bureau, qui ne s'arrête jamais). C'est ce qui retire les bandes noires gravées dans le flux ([D22](../DECISIONS.md)) |
| Écran virtuel | la même série `dd_*`, avec `dd_configuration_option = ensure_only_display` et `output_name` visant l'écran que ZyrDesk fait pousser sur l'hôte. Sunshine n'a aucun écran virtuel à lui, mais prévoit explicitement de piloter celui d'un tiers ; le nôtre est posé par `crates/zyr-screen/`, voir [ECRAN-VIRTUEL.md](../ECRAN-VIRTUEL.md) |
| Nom de l'écran à capturer | lu dans le journal du moteur, qui écrit sa liste complète d'écrans à chaque démarrage. Le moteur reste seul à savoir comment il nomme un écran : recalculer ce nom chez nous serait recopier une recette qui, fausse d'un octet, donne un nom qui ne désigne rien et sur lequel il retombe silencieusement sur l'écran principal |
| Aucune carte son de personne | `install_steam_audio_drivers = disabled` et `virtual_sink = aucune-carte-son-virtuelle`. Sans ces deux lignes, le moteur cherche la carte son virtuelle de Steam sur la machine, l'installe s'il en trouve les fichiers, et y bascule la sortie de l'ordinateur le temps de chaque session. C'est sa façon de vider une pièce en gardant le son dans le flux ; ZyrDesk le fait lui-même sur la vraie carte, sans rien installer ([D61](../DECISIONS.md)). Le second champ ne peut pas rester vide : vide veut dire « celle de Steam » pour le moteur, pas « aucune » |
| Curseur non gravé dans le flux | `draw_the_pointer` dans la porte `POST /api/zyr/serve` ([P-S8](../../patches/MANIFEST.md)), énoncé et non basculé. L'interrupteur existait déjà, atteignable par le seul raccourci `Ctrl+Alt+Maj+N` reçu du client, que Sunshine documente lui-même pour le bureau à distance ; mais une bascule ne se lit pas, et celle-là est partagée par toutes les sessions du moteur, donc une session finie sans l'avoir remise laissait la suivante la basculer à l'envers ([D157](../DECISIONS.md)). Le protocole ne transporte aucune forme de curseur : celui de l'hôte n'existe que dessiné dans l'image, donc il ne peut se retirer que là-bas. C'est la moitié hôte du curseur local ([D150](../DECISIONS.md)). Sa limite est connue et n'est pas la sienne : pendant qu'une fenêtre est déplacée, Windows compose lui-même le curseur avec l'image avant que quoi que ce soit ne la filme, et l'interrupteur n'a alors plus rien à éteindre ([D152](../DECISIONS.md)) |
| Santé | `GET /serverinfo` sur son port HTTP local |
| Appairage automatisé | `POST /api/pin` avec `{"pin": "...", "name": "..."}` sur son port web local (authentification Basic ; l'exemption CSRF pour les clients sans en-tête Origin est un comportement documenté) |
| Surcharges ponctuelles | tout paramètre peut aussi être passé en ligne de commande `nom=valeur` |
| Arrêt propre | signal console + respect de son code de sortie spécial « arrêt volontaire » (sinon son contrat de supervision attend un respawn) |
| Icône et éditeur portés par l'exécutable | `SUNSHINE_ICON_PATH`, `SUNSHINE_PUBLISHER_NAME`, `SUNSHINE_PUBLISHER_WEBSITE` et `SUNSHINE_PUBLISHER_ISSUE_URL` à la configuration : Sunshine les prévoit et demande explicitement aux produits tiers de poser les leurs |

Patchs appliqués : voir [`patches/MANIFEST.md`](../../patches/MANIFEST.md), qui fait foi. Cette page dit ce qui passe par les interfaces officielles ; le manifeste dit ce qui a dû être patché et pourquoi. Deux listes des mêmes patchs finiraient par diverger, et l'ont fait.

## 4. Points de contact avec Moonlight

Mécanismes officiels utilisés :

| Besoin | Mécanisme officiel |
|---|---|
| État isolé par appareil distant | fichier `portable.dat` à côté de l'exécutable : tout l'état (réglages, identité client, hôtes appairés) part dans un dossier local que nous plaçons dans `devices\<id>` sous les données du produit |
| Session sans interface Moonlight | commande `stream <hôte> "Desktop"` avec options : `--resolution WxH`, `--fps N`, `--bitrate K`, `--packet-size B` (force le mode « local », minimum 1025), `--display-mode fullscreen|windowed|borderless`, `--video-codec auto|H.264|HEVC|AV1`, `--video-decoder hardware`, `--frame-pacing`, `--absolute-mouse`, `--capture-system-keys`, `--performance-overlay`, `--hdr`, `--yuv444`, `--game-optimization` |
| Autoriser l'hôte à changer la définition de son bureau | `--game-optimization` : nom hérité des jeux, mais face au moteur hôte c'est le seul et unique sens qu'il a gardé. Sans lui, les options `dd_*` de l'hôte restent lettre morte et il grave des bandes noires dans le flux |
| Appairage sans interaction | commande `pair <hôte> --pin NNNN` |
| Statistiques | overlay de performances + journaux (débit d'images réseau/décodage/rendu, latence hôte, pertes, jitter, temps de décodage, délai de file, temps de rendu) |
| Curseur dessiné ici plutôt qu'attendu du réseau | montré du seul fait qu'on demande au moteur de suivre le fichier de formes ([P-M13](../../patches/MANIFEST.md)). Le raccourci `Ctrl+Alt+Maj+C` existe toujours et fait la même chose, mais n'est plus le chemin de ZyrDesk : n'importe quel programme de la machine peut réserver cette combinaison pour lui et l'avaler avant qu'elle n'arrive, sans que rien ne le dise ([D157](../DECISIONS.md)). Le mode jeu reste protégé par le moteur lui-même, qui cache ce curseur avec le mode ([D150](../DECISIONS.md)) |
| Mouvement de la souris en mode jeu | lu du système par le moteur, au seul endroit où les messages de sa fenêtre passent avant sa bibliothèque d'affichage ([P-M14](../../patches/MANIFEST.md)). Cette bibliothèque jette le mouvement brut destiné à une fenêtre dont elle croit qu'elle n'a pas le clavier, ce qu'elle décide du premier plan, qu'une fenêtre portée dans la nôtre ne peut jamais tenir : le mode jeu n'a jamais rien envoyé du tout ([D158](../DECISIONS.md)). Même cause que les touches système ([D43](../DECISIONS.md)). Lu seulement tant que la fenêtre sous le pointeur est celle du moteur : ses boutons à lui sont posés par-dessus l'image et sa fenêtre ne couvre pas toujours l'écran, donc une main partie ailleurs conduisait deux pointeurs à la fois. La cage du pointeur qui va avec ce mode reste à ZyrDesk : le système ne laisse enfermer le pointeur qu'au programme du premier plan |
| Réglages fins non exposés en CLI | clés du fichier INI portable (écrites avant lancement, jamais pendant une session) |

Patchs appliqués : voir [`patches/MANIFEST.md`](../../patches/MANIFEST.md), qui fait foi, pour la même raison que côté Sunshine.

Ce qu'il faut chercher avant d'en écrire un de plus n'a pas changé avec la levée du plafond : un mécanisme officiel qu'on aurait manqué, ou un interrupteur à proposer en amont. Un patch qui corrige un défaut du moteur, mesurable sans ZyrDesk, est le bon genre de patch : il a vocation à remonter chez eux et à disparaître de chez nous.

## 5. Schéma d'adressage loopback côté client

Chaque appareil distant reçoit une adresse loopback stable `127.77.x.y` (Windows accepte tout 127.0.0.0/8 sans configuration), avec le même port de base que le moteur hôte en face et une correspondance de ports 1:1. Avantages : l'état Moonlight (hôtes appairés) reste cohérent dans le temps, deux sessions sortantes simultanées ne se marchent pas dessus, et les journaux restent lisibles.

## 6. Coexistence avec de vrais Sunshine/Moonlight installés

- Ports moteur dans 42000 à 42999 : aucune collision avec un Sunshine standard (base 47989).
- État totalement séparé (dossiers ZyrDesk, mode portable Moonlight) : aucune interaction avec les réglages ou appairages d'installations existantes.
- Le service ZyrDesk ne touche jamais aux services ou processus d'un Sunshine officiel présent sur la machine.
