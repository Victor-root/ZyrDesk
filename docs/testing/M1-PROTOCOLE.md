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

---

## 2. Première session

Sur le **PC hôte**, dans une fenêtre de commandes :

```
zyr-cli host start
```

Attendu : « Accès distant actif ». Laisser cette fenêtre ouverte.

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

### V2. Le moteur hôte reste-t-il invisible sur le réseau ?

La configuration lie le moteur à `127.0.0.1` seul. Reste à vérifier qu'il ne s'annonce pas malgré tout sur le réseau local.

Test : pendant que `host start` tourne, sur le **PC client**, ouvrir un client de streaming tiers standard (ou un explorateur de services réseau) et regarder si un ordinateur apparaît spontanément dans sa liste.

Depuis le PC client, vérifier aussi qu'aucun port du moteur n'est joignable de l'extérieur :

```
Test-NetConnection <adresse-du-pc-hote> -Port 42001
```

Attendu : échec de connexion (le port n'écoute que sur le PC hôte lui-même).

> Un ordinateur apparaît-il dans un client tiers ? ................................................
>
> Le port 42001 répond-il depuis l'autre PC ? ................................................
>
> **Si oui à l'une des deux** : le patch P-S1 devient nécessaire. Le noter dans `patches/MANIFEST.md`.

---

### V3. Peut-on supprimer la fenêtre d'attente du moteur client ?

Au lancement d'une session, le moteur client affiche brièvement sa propre fenêtre de chargement avant la fenêtre vidéo. Elle doit disparaître pour que le produit soit crédible.

L'hypothèse : cette fenêtre dépend de la couche graphique du moteur, alors que la fenêtre vidéo n'en dépend pas. La neutraliser devrait donc masquer la première sans toucher la seconde.

Test, sur le **PC client** :

```
zyr-cli connect <adresse-du-pc-hote> --masquer-attente
```

> La fenêtre de chargement apparaît-elle encore ? ................................................
>
> La fenêtre vidéo s'affiche-t-elle normalement ? ................................................
>
> **Si la fenêtre vidéo ne s'affiche plus** : l'hypothèse est fausse, le patch P-M1 est nécessaire.
> **Si tout est correct** : P-M1 peut être abandonné, à noter dans `patches/MANIFEST.md`.

Attention : dans ce mode, les messages d'erreur du moteur risquent de devenir invisibles. Provoquer volontairement un échec (couper `host start` puis se connecter) et observer.

> En cas d'erreur, obtient-on un message ou un blocage silencieux ? ................................................
>
> Un blocage silencieux impose le patch P-M5 (codes de sortie distincts), sans lequel la reprise automatique ne peut pas fonctionner.

---

### V4. Le chiffrement interne est-il bien inactif ?

Le chiffrement de bout en bout sera porté par le tunnel. Le chiffrement interne des moteurs est donc désactivé pour ne pas chiffrer deux fois. Reste à vérifier que le moteur hôte l'honore, car le moteur client le demande de son côté.

Test : pendant une session, ouvrir le journal du moteur hôte (`%ProgramData%\ZyrDesk\logs\host-engine.log`) et chercher les lignes mentionnant le chiffrement au démarrage de la session.

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
