# Mise à niveau des moteurs upstream

Ce document est la procédure de référence pour mettre à niveau Sunshine et Moonlight des mois ou des années plus tard, sans fusion monstrueuse. Elle est conçue pour être exécutable par un développeur ou par une IA (type Claude Code) avec un minimum de contexte.

## Principe

Chaque moteur est un fork qui ne contient que : le tag upstream épinglé + notre pile de commits, tous préfixés `zyr:`. Mettre à niveau = rebaser cette pile sur le nouveau tag.

La difficulté n'est pas bornée par un nombre de commits, il n'y a pas de plafond ([STRATEGY.md §0](STRATEGY.md)). Elle est bornée par la marque `zyr:` : chaque ligne de notre fait à l'intérieur d'un fichier upstream porte un commentaire qui commence par là et qui dit pourquoi elle existe. Un `grep -rn "zyr:"` dans un moteur rend donc l'écart complet, avec son intention, avant même d'ouvrir un diff. **Commencer par là.**

Deux choses à savoir avant de s'y mettre :

- Les fichiers nommés `zyr*` n'appartiennent qu'à nous et ne peuvent pas entrer en conflit. Seuls leurs points d'accroche dans le code upstream le peuvent.
- Les endroits denses sont connus et se comptent sur une main. Au 2026-09-15 : `src/video.cpp` de Sunshine, où notre travail traverse le fil de capture, la boucle d'encodage et le choix de l'écran ; `app/streaming/session.cpp` de Moonlight. Si upstream a réécrit l'un des deux, ce n'est plus un rebase mais une reconstruction : lire le manifeste pour savoir ce que le patch devait obtenir, et le réécrire contre le code neuf.

## Procédure pas à pas

1. Lire l'écart actuel : [`patches/MANIFEST.md`](../../patches/MANIFEST.md) donne la liste de nos modifications et leur raison d'être, et `grep -rn "zyr:"` dans chaque fork donne leur emplacement exact. Le manifeste dit le pourquoi, le code dit le où.
2. Dans le fork concerné :
   - `git fetch upstream` puis identifier le nouveau tag cible (pour Sunshine, ne jamais descendre sous la version plancher notée dans le manifeste ; vérifier les notes de version pour les correctifs de sécurité).
   - Créer la nouvelle branche : `git checkout -b zyr/<nouveau-tag> <nouveau-tag>`.
   - Rebaser la pile : `git cherry-pick` des commits `zyr:` de l'ancienne branche (ou `git rebase --onto`). Les conflits sont localisés par construction, et ce qui est en conflit porte toujours la marque : elle dit quoi préserver.
   - Mettre à jour les submodules imbriqués du moteur comme le fait upstream (`git submodule update --init --recursive`).
3. Compiler le moteur avec nos scripts CI (Sunshine : MSYS2 UCRT64 ; Moonlight : MSVC + Qt). Corriger ce qui casse À L'INTÉRIEUR de notre pile uniquement ; si upstream a cassé autre chose, c'est son problème ou un signe qu'il faut attendre une version plus mûre.
4. Exécuter la suite « contrat moteur » (ci-dessous). C'est elle qui décide si la mise à niveau est sûre.
5. Dans le monorepo : bump du submodule (c'est lui, et lui seul, qui met un patch en circulation), mise à jour du manifeste (tag, hash, changements notables côté upstream qui nous concernent, patchs devenus inutiles et retirés).
6. Passer les tests d'intégration et le banc de performance (voir [../TESTING.md](../TESTING.md)) : une mise à niveau qui fait régresser les seuils G-* est refusée ou investiguée.
7. Vérifier l'interopérabilité N-1 : nouveau client contre ancien hôte, ancien client contre nouvel hôte.

## Suite « contrat moteur »

Les surfaces qu'on utilise (options de configuration, drapeaux CLI, endpoints REST, formats de journaux, codes de sortie) ne sont PAS des API stables côté upstream. Cette suite automatisée vérifie chacune de nos dépendances et transforme ces surfaces en contrat testé :

Sunshine :
- La configuration générée est acceptée (aucune clé inconnue signalée dans les journaux).
- Liaison réseau effective : avec `bind_address=127.0.0.1`, un scan confirme qu'AUCUN port n'écoute sur les interfaces externes ; les 7 ports attendus écoutent en local aux offsets attendus.
- Aucune annonce mDNS sur le réseau.
- `--creds` fonctionne ; `GET /serverinfo` répond ; `POST /api/pin` accepte un PIN et l'appairage aboutit de bout en bout avec le client.
- `system_tray=disabled` : aucune icône créée.
- Arrêt : le signal d'arrêt produit une fin propre ; le code de sortie spécial « arrêt volontaire » est bien celui attendu par notre superviseur.
- Comportement de chiffrement en mode 0 sur loopback conforme à ce qui est documenté dans le manifeste (pas de double chiffrement surprise).
- Le binaire produit porte bien notre nom de produit, notre icône et notre éditeur (patch P-S2 et options de configuration).

Moonlight :
- `pair --pin` aboutit sans interaction et sans fenêtre (patch P-M1).
- `stream` accepte tous les drapeaux que nous passons ; la session démarre ; aucune fenêtre du moteur avant l'image (patch P-M1).
- Le mode portable isole bien l'état dans le dossier fourni.
- `--packet-size` est honoré (vérifié par capture de paquets : taille maximale observée conforme).
- Les statistiques nécessaires au banc sont présentes dans les journaux/overlay au format attendu par notre parseur.
- Les codes de sortie distinguent bien fin normale (0), session en échec (2), machine injoignable (3), appairage refusé (4) et ordinateur distant qui ne nous reconnaît pas (6), tels que posés par P-M5.

## Répétition mensuelle automatique

Un job CI mensuel (`upgrade-rehearsal`) tente à blanc la mise à niveau vers le dernier tag upstream : fetch, rebase automatique de la pile, build, suite contrat moteur. Résultat publié en issue. Objectif : découvrir la dérive upstream en semaines, jamais au moment où la mise à niveau devient urgente (correctif de sécurité).

## Règles pour que ça reste vrai

- **Poser la marque `zyr:` sur chaque ligne de notre fait dans du code upstream**, avec la raison écrite à côté. C'est la règle qui remplace l'ancien plafond, et c'est elle qui rend ce document exécutable. Une modification sans marque est un défaut au même titre qu'un test qui ne passe pas.
- Ne JAMAIS ajouter une fonctionnalité produit dans un moteur : elle vivrait dans la zone de conflit permanente.
- Préférer un fichier `zyr*` à nous chaque fois qu'un patch a le choix : il ne peut pas entrer en conflit.
- Chaque nouveau patch doit être inscrit au manifeste avec sa raison et son plan de sortie, dans le même mouvement que le patch et jamais après coup (contribution upstream envisageable ? suppression possible quand upstream expose l'option ?).
- Si une évolution upstream rend un de nos patchs inutile : le supprimer immédiatement à la mise à niveau suivante.
- Avant d'écrire un patch de plus, chercher le mécanisme officiel manquant ou l'interrupteur à proposer en amont. Ce n'est plus un plafond qui le rappelle, c'est cette ligne.
