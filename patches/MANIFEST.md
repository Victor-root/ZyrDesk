# Manifeste des moteurs et de leurs adaptations

Ce fichier est la source de vérité sur l'écart entre nos moteurs et leurs projets upstream. Il se lit sans ouvrir les dépôts des moteurs.

Règle : voir `docs/engines/STRATEGY.md`. Aucune fonctionnalité ZyrDesk ne vit dans un moteur. Un patch ne peut que retirer de l'habillage ou exposer un interrupteur.

## Versions épinglées

| Moteur | Upstream | Référence | Commit |
|---|---|---|---|
| Sunshine | https://github.com/LizardByte/Sunshine | tag `v2026.516.143833` | `14ffa6fdaa53f7b51512be2b3d24f3939695403c` |
| Moonlight | https://github.com/moonlight-stream/moonlight-qt | `master` du 2026-08-07 | `2e13ed9977bc31c73caf8428f08f58d793313ece` |

Contrainte de sécurité : Sunshine ne doit jamais être épinglé sous `v2026.516.143833`, première version corrigeant un contournement critique de la validation des certificats clients (CVSS 9.8).

Moonlight est épinglé sur un commit de `master` et non sur la dernière version stable (6.1.0, septembre 2024) : `master` apporte l'AV1, le 4:4:4 et un pipeline de rendu nettement plus récent, indispensables aux objectifs de qualité.

## Pile de patchs

Aucun patch appliqué à ce jour : les deux moteurs sont à l'état upstream exact. Le pilotage passe uniquement par leurs interfaces officielles (fichier de configuration, ligne de commande, API REST locale).

| Moteur | Patchs appliqués | Plafond |
|---|---|---|
| Sunshine | 0 | 2 |
| Moonlight | 0 | 6 |

Dépasser un plafond est un signal d'architecture : chercher le mécanisme officiel manquant ou proposer l'interrupteur en amont, jamais empiler.

## Patchs prévus

| Id | Moteur | Objet | Jalon | Statut |
|---|---|---|---|---|
| P-M1 | Moonlight | Suppression de la fenêtre de chargement en lancement ligne de commande, erreurs vers la sortie d'erreur | M4 | **Confirmé nécessaire** : la piste sans patch, qui consistait à neutraliser la couche graphique par l'environnement, est écartée. La version Windows du moteur n'embarque qu'une seule couche d'affichage et refuse de démarrer sans elle |
| P-M5 | Moonlight | Codes de sortie distincts (sortie utilisateur, perte réseau, erreur fatale) | M1 | **Confirmé nécessaire** : observé sur machine réelle, le moteur sort avec un code de succès alors que la session a échoué, l'erreur ne vivant que dans une fenêtre. Sans ce patch, la reprise automatique ne peut pas décider s'il faut relancer. Fusionne en pratique avec P-M1 |
| P-M2 | Moonlight | Rebranding : titre de fenêtre, icônes, noms d'organisation et de produit, métadonnées de l'exécutable | M4 | Requis |
| P-M3 | Moonlight | Ligne de statistiques lisible par machine | M2 | Seulement si les journaux existants ne suffisent pas au banc de mesure |
| P-M4 | Moonlight | Interrupteur pour ne pas demander le chiffrement vidéo interne | M1 | Contingence : seulement si la vérification M1 montre un double chiffrement sur loopback |
| P-S1 | Sunshine | Désactivation de l'annonce mDNS | M1 | Contingence : seulement si le moteur s'annonce sur le réseau malgré la liaison loopback |

## Contraintes des moteurs relevées sur machine réelle

Ces comportements ne sont écrits nulle part dans leur documentation et ont été découverts à l'usage. Ils sont pris en charge par notre code, sans modification des moteurs.

| Moteur | Contrainte | Conséquence |
|---|---|---|
| Hôte | Résout ses ressources graphiques par rapport au dossier courant, pas à son exécutable | Il est lancé depuis son propre dossier, sans quoi l'initialisation graphique échoue |
| Hôte | La restriction d'adresse d'écoute couvre tous ses services : appairage, négociation, vidéo, audio, contrôle | Elle ne peut être posée qu'une fois le tunnel en place |
| Hôte | N'accepte un code d'appairage que pendant qu'un client l'attend, et signale un succès même sans demande en cours | L'ordre client puis hôte est imposé ; le tunnel supprimera la question |
| Hôte | N'encode que lorsque l'écran change, et ne garantit par défaut que la moitié de la cadence demandée | La cadence minimale est portée à 60 dans la configuration générée : sans cela, souris et animations sont saccadées sur un bureau immobile |
| Client | Sort avec un code de succès même après un échec de session | Le journal est la seule source fiable en attendant P-M5 |

## Fichiers de patchs

Ce dossier accueillera les fichiers `.patch` exportés automatiquement à chaque changement d'épinglage (`git format-patch <tag-upstream>..<branche-zyr>`). Il est vide tant que la pile l'est.

Ils servent deux usages : relire notre écart complet sans quitter ce dépôt, et documenter publiquement nos modifications comme l'exige la GPL.
