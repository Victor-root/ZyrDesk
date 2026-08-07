# Jalon M1 : protocole de test sur deux PC

Ce document se déroule sur deux vrais PC Windows. Il poursuit deux buts : obtenir une première session distante pilotée par ZyrDesk, et surtout **lever par écrit les hypothèses** sur lesquelles repose l'architecture. Une hypothèse non vérifiée est une dette : chaque encadré ci-dessous attend un résultat noté.

Aucun de ces points n'a pu être vérifié pendant le développement : il n'y a ni Windows ni carte graphique sur la machine où le code est écrit. Les réponses obtenues ici décident de plusieurs choix pour la suite.

Vocabulaire : **PC hôte** = celui qu'on contrôle. **PC client** = celui depuis lequel on se connecte.

---

## 1. Préparation (sur les deux PC)

1. Installer ZyrDesk (ou compiler : `cargo build --release`).
2. Lancer le diagnostic :

   ```
   zyr-cli doctor
   ```

   Attendu : plateforme et carte graphique reconnues, ports disponibles, dossier de données accessible. Les moteurs sont signalés absents, c'est normal à cette étape.

3. Déposer les moteurs. Lancer :

   ```
   zyr-cli engines status
   ```

   La commande indique les dossiers attendus et le nom exact à donner à chaque exécutable. Les versions à récupérer sont celles épinglées dans `patches/MANIFEST.md` : les binaires officiels préconstruits conviennent à cette étape, le projet compilera les siens au jalon M4.

4. Relancer `zyr-cli engines status` jusqu'à obtenir les deux moteurs en place.

5. Sur le **PC hôte**, autoriser le moteur dans le pare-feu Windows, une seule fois, dans une fenêtre **administrateur** :

   ```powershell
   New-NetFirewallRule -DisplayName "ZyrDesk (moteur hote)" -Direction Inbound `
     -Program "$PWD\data\engines\host\zyrdesk-host-engine.exe" -Action Allow
   ```

   Les binaires officiels déposés à la main n'apportent aucune règle : sans celle-ci, le PC client ne peut joindre aucun port. L'installateur du produit s'en chargera.

---

## 2. Première session

Sur le **PC hôte**, dans une fenêtre de commandes :

```
zyr-cli host start
```

Attendu : « Accès distant actif ». Laisser cette fenêtre ouverte.

Note sur l'exposition réseau : tant qu'il n'y a pas de tunnel, le moteur écoute sur toutes les interfaces, faute de quoi rien n'est joignable. Le jalon M2 le referme sur la machine locale, le tunnel devenant l'unique chemin. La vérification V2 ci-dessous en tient compte.

Sur le **PC client** :

```
zyr-cli connect <adresse-du-pc-hote> --stats
```

La commande affiche un code à quatre chiffres et la commande exacte à lancer sur le PC hôte. Ouvrir une **seconde** fenêtre sur le PC hôte (la première est occupée) et y lancer :

```
zyr-cli host pin <le-code-affiche>
```

Attendu : le bureau du PC hôte s'affiche en plein écran sur le PC client, avec les statistiques en surimpression.

> **V1. La session démarre.** Résultat : ................................................
>
> Si l'appairage échoue par manque de temps, noter le délai observé : le code doit être saisi avant que le moteur client n'abandonne. C'est justement ce que le tunnel automatisera au jalon M5.

---

## 3. Hypothèses à lever

### V2. Le moteur hôte s'annonce-t-il de lui-même sur le réseau ?

À ce stade le moteur écoute sur le réseau, faute de tunnel : ses ports sont donc joignables, c'est attendu. La question porte sur autre chose : s'annonce-t-il **spontanément**, au point d'apparaître dans la liste d'un client de streaming tiers du réseau ? Si oui, un ordinateur nommé d'après la machine surgirait chez les voisins, ce que le produit ne doit jamais faire.

Test : pendant que `host start` tourne, sur le **PC client**, ouvrir un client de streaming tiers standard (ou un explorateur de services réseau) et regarder si un ordinateur apparaît spontanément dans sa liste, sans qu'on ait saisi d'adresse.

> Un ordinateur apparaît-il tout seul dans un client tiers ? ................................................
>
> **Si oui** : le patch P-S1 devient nécessaire. Le noter dans `patches/MANIFEST.md`.

Ce point sera à revérifier au jalon M2, une fois le moteur refermé sur la machine locale : le contrôle deviendra alors qu'**aucun** de ses ports ne répond depuis un autre ordinateur.

```
Test-NetConnection <adresse-du-pc-hote> -Port 42001
```

---

### V3. Peut-on supprimer la fenêtre d'attente du moteur client ? RÉPONDU : non

Au lancement d'une session, le moteur client affiche sa propre fenêtre de chargement avant la fenêtre vidéo. Elle doit disparaître pour que le produit soit crédible.

L'hypothèse testée était de neutraliser sa couche graphique par l'environnement, en pariant que la fenêtre vidéo n'en dépendait pas. **Réfutée sur machine réelle** : la version Windows du moteur n'embarque qu'une seule couche d'affichage, et refuse de démarrer sans elle.

Conséquence actée : le patch **P-M1 est nécessaire**, et l'approche par variable d'environnement est écartée définitivement. Le code qui la tentait a été supprimé.

Reste à vérifier, une fois P-M1 écrit : les messages d'erreur du moteur ne doivent pas devenir invisibles. Provoquer volontairement un échec (couper `host start` puis se connecter) et observer.

> En cas d'erreur, obtient-on un message exploitable ? ................................................

---

### V4. Le chiffrement interne est-il bien inactif ?

Le chiffrement de bout en bout sera porté par le tunnel. Le chiffrement interne des moteurs est donc désactivé pour ne pas chiffrer deux fois. Reste à vérifier que le moteur hôte l'honore, car le moteur client le demande de son côté.

Test : pendant une session, ouvrir le journal du moteur hôte (`data\logs\engine.log` dans le dossier du projet) et chercher les lignes mentionnant le chiffrement au démarrage de la session.

> Le journal indique-t-il un chiffrement vidéo actif ? ................................................
>
> **Si oui** : le patch P-M4 devient nécessaire (le coût est faible, environ 1 à 2 % de processeur, mais inutile).

---

### V5. Quelle est la taille réelle des en-têtes par paquet ?

Le budget de taille de paquet du tunnel (jalon M2) repose sur une estimation d'environ 28 octets d'en-tête par paquet vidéo. Une erreur ici provoque de la fragmentation réseau, donc de la latence.

Test : pendant une session, capturer le trafic sur le PC client (Wireshark, filtre `udp.port == 42009`), relever la taille de plusieurs paquets vidéo.

> Taille maximale de paquet UDP observée : ................................................
>
> Taille utile annoncée par le moteur (option de taille de paquet employée) : ................................................
>
> Différence, donc en-tête réel : ................................................

---

### V6. Que se passe-t-il quand l'écran de l'hôte s'éteint ?

Test : pendant une session, éteindre physiquement l'écran du PC hôte, puis attendre la mise en veille de l'affichage.

> L'image continue-t-elle d'arriver ? ................................................
>
> Si elle se fige, l'écran virtuel du jalon M9 devient indispensable plutôt que confortable.

---

### V7. Le pilote d'écran virtuel s'installe-t-il proprement ?

**Ce test décide de toute la stratégie « PC sans écran ».**

Test, sur le **PC hôte** uniquement, sur un Windows 11 à jour : télécharger la dernière version publiée de Virtual-Display-Driver (projet open source sous licence MIT, signé gratuitement via SignPath) et l'installer en suivant sa procédure officielle.

> Windows accepte-t-il le pilote sans qu'on ait à modifier la moindre autorisation ou à installer un certificat ? ................................................
>
> Un écran supplémentaire apparaît-il dans les paramètres d'affichage ? ................................................
>
> **Si oui aux deux** : la stratégie tient, le jalon M9 est réalisable tel que prévu.
> **Si non** : la fonction sera désactivée et le PC hôte devra garder un écran branché. À acter dans `docs/DECISIONS.md`.

Désinstaller le pilote après le test pour ne pas fausser les mesures suivantes.

---

## 4. Mesures de performance

Critère de sortie du jalon : **notre pilotage ne doit rien coûter**. Les mesures obtenues avec ZyrDesk doivent rester à moins de 5 % de celles obtenues avec les moteurs employés directement, dans les mêmes conditions.

### Mesure avec ZyrDesk

Sur le PC client, avec les statistiques affichées :

```
zyr-cli connect <adresse-du-pc-hote> --stats
```

Laisser tourner cinq minutes sur un contenu animé (une vidéo en plein écran sur le PC hôte fait l'affaire), puis relever :

| Mesure | Valeur |
|---|---|
| Images par seconde reçues | |
| Images par seconde affichées | |
| Latence hôte (moyenne) | |
| Temps de décodage (moyen) | |
| Latence réseau (moyenne et variance) | |
| Images perdues par le réseau | |
| Images perdues par gigue | |
| Charge processeur du PC hôte | |
| Charge processeur du PC client | |

Refaire en imposant chaque codec :

```
zyr-cli connect <adresse> --stats --codec h264
zyr-cli connect <adresse> --stats --codec hevc
```

> Le décodage est-il bien matériel dans les deux cas ? ................................................
>
> Le décodage matériel est imposé : si aucun n'est disponible, la session échoue au lieu de basculer silencieusement en logiciel. Un échec ici est une information, pas un défaut.

### Mesure de référence

Refaire exactement les mêmes relevés en lançant les moteurs officiels directement, sans ZyrDesk, avec des réglages identiques (même résolution, même nombre d'images par seconde, même débit, même codec, régulation du rythme d'affichage activée).

> Écart constaté entre les deux séries : ................................................
>
> Au-delà de 5 %, chercher la cause avant de passer au jalon M2 : le tunnel ajoutera ses propres effets et masquerait le problème.

### Latence bout en bout

Suivre la procédure décrite dans `perf/GATES.md` (chronomètre filmé à 240 images par seconde). Dix mesures, noter la médiane.

> Latence médiane : ................................................

---

## 5. Clôture du jalon

Le jalon M1 est terminé quand :

- [ ] Une session 1080p60 fonctionne, en H.264 et en HEVC, avec décodage matériel, audio, clavier et souris.
- [ ] L'écart avec les moteurs employés directement est inférieur à 5 %.
- [ ] Les vérifications V2 à V7 sont toutes renseignées.
- [ ] Les conséquences sont reportées dans `patches/MANIFEST.md` (patchs devenus nécessaires ou abandonnés) et dans `docs/DECISIONS.md` (stratégie d'écran virtuel).
- [ ] Les mesures de référence sont archivées dans `perf/` : elles servent de base de comparaison à tous les jalons suivants.
