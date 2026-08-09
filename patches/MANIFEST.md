# Manifeste des moteurs et de leurs adaptations

Ce fichier est la source de vérité sur l'écart entre nos moteurs et leurs projets upstream. Il se lit sans ouvrir les dépôts des moteurs.

Règle : voir `docs/engines/STRATEGY.md`. Aucune fonctionnalité ZyrDesk ne vit dans un moteur. Un patch ne peut que retirer de l'habillage ou exposer un interrupteur.

## Versions épinglées

| Moteur | Upstream | Référence | Commit |
|---|---|---|---|
| Sunshine | https://github.com/LizardByte/Sunshine | tag `v2026.516.143833` | `14ffa6fdaa53f7b51512be2b3d24f3939695403c` |
| Moonlight | https://github.com/moonlight-stream/moonlight-qt | tag `v6.1.0` | `f786e94c7b2f943e24e65d7d74deb539b827fc84` |

Le dépôt référence les forks ([ZyrDesk-Sunshine](https://github.com/Victor-root/ZyrDesk-Sunshine), [ZyrDesk-Moonlight](https://github.com/Victor-root/ZyrDesk-Moonlight)) et non les projets d'origine : c'est là que vit notre pile de correctifs, sur une branche `zyr/<tag>` partant du tag épinglé. Les forks ne servent qu'à ça ; la version de référence reste celle du tableau ci-dessus.

Contrainte de sécurité : Sunshine ne doit jamais être épinglé sous `v2026.516.143833`, première version corrigeant un contournement critique de la validation des certificats clients (CVSS 9.8).

Les deux moteurs sont épinglés sur des versions publiées. Le choix de `v6.1.0` pour Moonlight, plutôt que sa branche principale, est motivé dans [../docs/DECISIONS.md](../docs/DECISIONS.md) (D14).

## Pile de patchs

Les deux moteurs portent notre marque, le moteur client n'ouvre aucune fenêtre à lui et ne joint rien hors du tunnel. Tout le reste du pilotage passe par leurs interfaces officielles (fichier de configuration, ligne de commande, API REST locale).

| Moteur | Patchs appliqués | Plafond |
|---|---|---|
| Sunshine | 1 | 2 |
| Moonlight | 5 | 6 |

Dépasser un plafond est un signal d'architecture : chercher le mécanisme officiel manquant ou proposer l'interrupteur en amont, jamais empiler.

## Patchs prévus

| Id | Moteur | Objet | Jalon | Statut |
|---|---|---|---|---|
| P-M1 | Moonlight | Suppression des fenêtres du moteur en lancement ligne de commande (session et appairage), erreurs vers la sortie d'erreur | M4 | **Appliqué** (`7ecea76` puis complété, branche `zyr/v6.1.0`), avec P-M5. Les deux commandes que nous utilisons suivent désormais le chemin sans interface graphique que le moteur utilisait déjà pour lister les applications : ce que les fenêtres affichaient part sur la sortie d'erreur, en UTF-8 pour que les accents survivent au journal. Les vues sont supprimées plutôt que laissées inatteignables |
| P-M5 | Moonlight | Codes de sortie distincts (fin normale, session en échec, machine injoignable, appairage refusé) | M1 | **Appliqué** (`7ecea76` puis complété, branche `zyr/v6.1.0`), avec P-M1. Le moteur sortait avec un code de succès même après un échec, l'erreur ne vivant que dans une fenêtre. Il rend maintenant 2 quand la session a échoué, 3 quand la machine n'a pas répondu et 4 quand l'appairage a été refusé, ce que notre superviseur lit pour décider |
| P-M2 | Moonlight | Rebranding : titre de fenêtre, icônes, noms d'organisation et de produit, métadonnées de l'exécutable, nom affiché par le mélangeur de volume | M4 | **Appliqué** (`e8f6d0c` puis complété, branche `zyr/v6.1.0`). Ne touche que des noms et des images, aucun comportement. Le changement des noms d'organisation et d'application déplace aussi l'endroit où le moteur range ses réglages, ce qui est sans effet ici puisqu'il tourne en mode portable. Aucun candidat à une contribution en amont : c'est notre marque |
| P-M6 | Moonlight | Le moteur ne s'annonce plus et ne joint plus rien hors du tunnel | M4 | **Appliqué** (`067328a`, branche `zyr/v6.1.0`). Découvert en cherchant les traces visibles : la présence Discord, active par défaut, annonçait chaque session aux contacts de l'utilisateur sous l'identité du projet d'origine ; les données de compatibilité et les correspondances de manettes étaient téléchargées sur son site à chaque session. Retiré aussi la boîte de dialogue qu'une erreur de ligne de commande ouvrait, qui aurait attendu un clic que personne n'est là pour donner. Que du retrait, aucun comportement produit ajouté |
| P-M7 | Moonlight | Fermeture de l'application sur l'hôte en ligne de commande, sans fenêtre, avec un code de sortie propre | M4 | **Appliqué** (`04a1ba1`, branche `zyr/v6.1.0`). Le troisième et dernier chemin en ligne de commande qui ouvrait une fenêtre du projet d'origine, et le plus mal placé : il se demande depuis une session en cours, donc sa fenêtre serait apparue par-dessus l'image. Même forme que P-M1 : ce que la fenêtre disait part sur la sortie d'erreur, la vue est supprimée. Un échec sort désormais sur le code 5 au lieu de laisser le processus sur une boîte de dialogue que personne n'est là pour fermer : l'appelant doit apprendre que l'hôte tient toujours son bureau |
| P-M3 | Moonlight | Ligne de statistiques lisible par machine | M2 | Seulement si les journaux existants ne suffisent pas au banc de mesure |
| P-M4 | Moonlight | Interrupteur pour ne pas demander le chiffrement vidéo interne | M1 | Contingence : seulement si la vérification M1 montre un double chiffrement sur loopback |
| P-S2 | Sunshine | Rebranding : nom de produit porté par l'exécutable Windows | M4 | **Appliqué** (`67b053b`, branche `zyr/v2026.516.143833`). Huit lignes dans un fichier de compilation, et aucune ne nomme ZyrDesk : le moteur expose déjà son icône et son éditeur en option de compilation, le patch ajoute la troisième de la même série. Notre nom est passé par notre script de compilation. Candidat à une contribution en amont tel quel |
| P-S1 | Sunshine | Désactivation de l'annonce mDNS | M1 | Contingence : seulement si le moteur s'annonce sur le réseau malgré la liaison loopback |

## Contraintes des moteurs relevées sur machine réelle

Ces comportements ne sont écrits nulle part dans leur documentation et ont été découverts à l'usage. Ils sont pris en charge par notre code, sans modification des moteurs.

| Moteur | Contrainte | Conséquence |
|---|---|---|
| Hôte | Résout ses ressources graphiques par rapport au dossier courant, pas à son exécutable | Il est lancé depuis son propre dossier, sans quoi l'initialisation graphique échoue |
| Hôte | La restriction d'adresse d'écoute couvre tous ses services : appairage, négociation, vidéo, audio, contrôle | Elle ne peut être posée qu'une fois le tunnel en place |
| Hôte | N'accepte un code d'appairage que pendant qu'un client l'attend, et signale un succès même sans demande en cours | L'ordre client puis hôte est imposé ; le tunnel supprimera la question |
| Hôte | Demande une priorité GPU élevée pour sa capture, refusée sans droits administrateur, et le signale dans son journal | Le service du jalon M3 tournant avec les droits système, la question disparaît ; en attendant, une capture irrégulière sous charge est attendue |
| Hôte | N'encode que lorsque l'écran change, et ne garantit par défaut que la moitié de la cadence demandée | La cadence minimale est portée à 60 dans la configuration générée : sans cela, souris et animations sont saccadées sur un bureau immobile |

## Fichiers de patchs

Ce dossier accueillera les fichiers `.patch` exportés automatiquement dès que la pile bouge (`git format-patch <tag-upstream>..<branche-zyr>`). Ils serviront deux usages : relire notre écart complet sans quitter ce dépôt, et documenter publiquement nos modifications comme l'exige la GPL.

Il est vide pour l'instant : l'export n'est pas automatisé, et une copie tenue à la main dériverait de la pile réelle sans que personne le voie. En attendant, l'écart se lit dans les forks, entre le tag épinglé et la tête de la branche `zyr/<tag>`. Les sources modifiées y sont publiques, ce qui satisfait déjà la GPL.
