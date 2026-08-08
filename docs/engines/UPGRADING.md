# Mise à niveau des moteurs upstream

Ce document est la procédure de référence pour mettre à niveau Sunshine et Moonlight des mois ou des années plus tard, sans fusion monstrueuse. Elle est conçue pour être exécutable par un développeur ou par une IA (type Claude Code) avec un minimum de contexte.

## Principe

Chaque moteur est un fork qui ne contient que : le tag upstream épinglé + une petite pile de commits ZyrDesk clairement identifiés (préfixe `zyr:` dans les messages). Mettre à niveau = rebaser cette pile sur le nouveau tag. La difficulté est bornée par la taille de la pile (0 à 2 commits pour Sunshine, 6 maximum pour Moonlight), pas par la taille des moteurs.

## Procédure pas à pas

1. Lire l'écart actuel : `patches/MANIFEST.md` + les fichiers `.patch` du monorepo donnent la liste exacte de nos modifications et leur raison d'être, sans ouvrir les forks.
2. Dans le fork concerné :
   - `git fetch upstream` puis identifier le nouveau tag cible (pour Sunshine, ne jamais descendre sous la version plancher notée dans le manifeste ; vérifier les notes de version pour les correctifs de sécurité).
   - Créer la nouvelle branche : `git checkout -b zyr/<nouveau-tag> <nouveau-tag>`.
   - Rebaser la pile : `git cherry-pick` des commits `zyr:` de l'ancienne branche (ou `git rebase --onto`). Résoudre les conflits : ils sont localisés par construction (habillage et interrupteurs uniquement).
   - Mettre à jour les submodules imbriqués du moteur comme le fait upstream (`git submodule update --init --recursive`).
3. Compiler le moteur avec nos scripts CI (Sunshine : MSYS2 UCRT64 ; Moonlight : MSVC + Qt). Corriger ce qui casse À L'INTÉRIEUR de notre pile uniquement ; si upstream a cassé autre chose, c'est son problème ou un signe qu'il faut attendre une version plus mûre.
4. Exécuter la suite « contrat moteur » (ci-dessous). C'est elle qui décide si la mise à niveau est sûre.
5. Dans le monorepo : bump du submodule, régénération des `.patch` (fait par la CI), mise à jour du manifeste (tag, hash, changements notables côté upstream qui nous concernent).
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
- Les codes de sortie distinguent bien fin normale (0), session en échec (2), machine injoignable (3) et appairage refusé (4), tels que posés par P-M5.

## Répétition mensuelle automatique

Un job CI mensuel (`upgrade-rehearsal`) tente à blanc la mise à niveau vers le dernier tag upstream : fetch, rebase automatique de la pile, build, suite contrat moteur. Résultat publié en issue. Objectif : découvrir la dérive upstream en semaines, jamais au moment où la mise à niveau devient urgente (correctif de sécurité).

## Règles pour que ça reste vrai

- Ne JAMAIS ajouter une fonctionnalité produit dans un moteur : elle vivrait dans la zone de conflit permanente.
- Chaque nouveau patch doit être inscrit au manifeste avec sa raison et son plan de sortie (contribution upstream envisageable ? suppression possible quand upstream expose l'option ?).
- Si une évolution upstream rend un de nos patchs inutile : le supprimer immédiatement à la mise à niveau suivante.
- Si la pile Moonlight dépasse 6 commits ou la pile Sunshine dépasse 2, c'est un signal d'architecture : chercher le mécanisme officiel manquant ou proposer l'interrupteur upstream, pas empiler.
