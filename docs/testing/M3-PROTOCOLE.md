# Jalon M3 : accès distant sans personne devant la machine

Ce document se déroule sur les deux mêmes PC Windows qu'aux jalons M1 et M2. Il vérifie une seule chose, mais c'est celle qui sépare un bricolage d'un vrai produit : **le PC hôte est joignable sans que personne n'ait ouvert de session dessus**, et il le reste quand la session change.

Jusqu'ici, il fallait ouvrir une session Windows, lancer une fenêtre de commandes en administrateur et laisser `zyr-cli host start` tourner. Autrement dit : il fallait déjà être devant le PC qu'on voulait contrôler à distance. Le service supprime ça.

Vocabulaire : **PC hôte** = celui qu'on contrôle. **PC client** = celui depuis lequel on se connecte.

---

## 1. Ce qui change, et pourquoi c'est délicat

Windows range chaque utilisateur connecté dans une **session** numérotée. Un service, lui, tourne dans une session à part, la session 0, qui n'a ni écran ni bureau. Un moteur de capture démarré là ne verrait rigoureusement rien.

Le service ZyrDesk démarre donc le moteur **dans la session attachée à l'écran physique**, celle où s'affiche l'écran de connexion Windows. Il le fait avec son propre jeton d'accès, celui du compte système, simplement déplacé vers cette session. C'est ce détail qui permet de voir l'écran de connexion et les invites de sécurité, qu'un jeton d'utilisateur ordinaire n'a pas le droit de capturer.

Cette session change dès que quelqu'un se connecte, se déconnecte ou change d'utilisateur. Le service la surveille et relance le moteur dans la nouvelle : un moteur laissé dans une session morte n'affiche plus rien.

---

## 2. Préparation

Sur les **deux PC**, compiler en release et vérifier que les moteurs sont en place :

```
cargo build --release
zyr-cli engines status
```

Sur le **PC hôte**, la règle de pare-feu du jalon M1 doit toujours exister. Sinon, dans une fenêtre **administrateur** :

```powershell
New-NetFirewallRule -DisplayName "ZyrDesk (moteur hote)" -Direction Inbound `
  -Program "$PWD\data\engines\host\zyrdesk-host-engine.exe" -Action Allow
```

Le service ne pose pas encore cette règle lui-même : ce sera l'installateur du produit.

**Appairer les deux PC maintenant**, pendant qu'une session est ouverte sur l'hôte. Le PC client ne pourra plus faire appairer depuis l'écran de connexion, puisque plus personne ne sera là pour taper le code. Suivre la section 2 de [M1-PROTOCOLE.md](M1-PROTOCOLE.md), puis fermer la session distante.

Enfin, **arrêter tout `zyr-cli host start`** qui traînerait : deux moteurs à la fois se marchent dessus, et le second écrase le fichier qui dit au reste du produit où joindre le premier.

---

## 3. Installer le service

Sur le **PC hôte**, dans une fenêtre **administrateur** :

```
zyrdeskd install
zyrdeskd start
zyrdeskd status
```

Attendu : « Service installé. Il démarrera avec Windows. », puis « Service démarré. », puis « En marche ».

Le service écrit tout ce qu'il fait dans `data\logs\service.log`. Ouvrir ce fichier :

```
notepad data\logs\service.log
```

> **M3-R1 (le service tient son moteur)**
>
> Recopier les lignes du journal. Attendu, dans l'ordre : `service started`, puis `engine started in session N, process P, on base port ...`, puis `remote access active`.
>
> Le numéro de session compte : ce ne doit **pas** être 0. Zéro voudrait dire que le moteur est resté dans la session du service, celle sans écran.

Vérifier aussi que le moteur est bien là où on l'attend, dans le gestionnaire des tâches, onglet « Détails » (clic droit sur les en-têtes de colonnes pour ajouter « ID de session ») : `zyrdesk-host-engine.exe` doit porter le numéro de processus du journal et le même numéro de session.

---

## 4. Le vrai test : se connecter avant toute ouverture de session

Sur le **PC hôte** :

1. Redémarrer complètement le PC.
2. **Ne pas se connecter.** Rester sur l'écran de connexion Windows, ou même simplement s'éloigner.

Sur le **PC client**, une fois l'hôte redémarré, laisser une minute au service puis :

```
zyr-cli connect <adresse-du-pc-hote> --stats
```

> **M3-R2 (accès avant ouverture de session)**
>
> Attendu : **l'écran de connexion Windows du PC hôte s'affiche sur le PC client**, et le clavier et la souris y répondent. Taper le mot de passe à distance doit ouvrir la session, et le bureau doit apparaître ensuite.
>
> Noter : l'écran de connexion s'affiche-t-il ? Le clavier répond-il ? La session s'ouvre-t-elle ? Après l'ouverture, l'image revient-elle toute seule, et au bout de combien de secondes ?
>
> C'est le critère principal du jalon. Sans lui, rien d'autre ici ne compte.

L'ouverture de session change la session attachée à l'écran : le service arrête le moteur et le relance dans la nouvelle. Le PC client perd donc l'image quelques secondes, le temps que le moteur redémarre. Relire le journal après coup :

```
notepad data\logs\service.log
```

Attendu : une ligne `the screen left session N, the engine starts over in the new one`, puis un nouveau `engine started in session M` avec un autre numéro.

---

## 5. Invite de sécurité (UAC)

Session ouverte, toujours depuis le PC client, lancer sur l'hôte quelque chose qui déclenche une demande d'élévation : par exemple un clic droit sur l'invite de commandes, « Exécuter en tant qu'administrateur ».

> **M3-R3 (bureau sécurisé)**
>
> Attendu : l'invite bleue s'affiche sur le PC client, et le bouton « Oui » est cliquable à distance.
>
> Noter : l'écran devient-il noir ? L'invite est-elle visible ? Peut-on cliquer dedans ?
>
> Un écran noir à cet instant est un vrai défaut, pas un détail d'affichage : il signifie que le moteur n'a pas le droit de capturer le bureau sécurisé, et il faudra le signaler tel quel.

---

## 6. Verrouillage, déverrouillage, changement d'utilisateur

Toujours connecté depuis le PC client, sur l'hôte :

1. Verrouiller la session (Windows + L), attendre dix secondes, déverrouiller.
2. Si le PC hôte a un second compte : changer d'utilisateur, ouvrir l'autre session, puis revenir.

> **M3-R4 (transitions de session)**
>
> Pour chacune des deux manipulations : l'image revient-elle toute seule ? Au bout de combien de secondes ? Faut-il relancer quoi que ce soit à la main ?
>
> **Seuil à tenir : coupure de 5 secondes au plus, récupérée sans intervention.**

---

## 7. Le service se relève

Toujours sur l'hôte, dans le gestionnaire des tâches, **terminer de force** le processus `zyrdesk-host-engine.exe`.

> **M3-R5 (relance après incident)**
>
> Attendu : le service le relance tout seul, et le journal l'écrit (`engine stopped (code ...) after N s, restarting in 0 s`).
>
> Noter au bout de combien de temps le PC client retrouve l'image.
>
> La règle appliquée : un moteur qui a tenu plus d'une minute est relancé immédiatement ; un moteur qui retombe aussitôt est relancé de plus en plus lentement, et abandonné après cinq échecs rapprochés, plutôt que de tourner en boucle en masquant la panne.

---

## 8. Arrêt et désinstallation propres

Sur le **PC hôte**, en administrateur :

```
zyrdeskd stop
zyrdeskd status
zyrdeskd uninstall
```

> **M3-R6 (retrait sans résidu)**
>
> Attendu : « Service arrêté. », puis « Arrêté », puis « Service retiré. »
>
> Vérifier ensuite dans le gestionnaire des tâches qu'**aucun** `zyrdesk-host-engine.exe` ne survit. Un moteur orphelin après l'arrêt du service est un défaut : c'est exactement ce que l'objet de travail Windows est censé empêcher.
>
> Vérifier aussi que « ZyrDesk » a disparu de la console des services (`services.msc`).

---

## 9. Ce que ce jalon ne fait pas encore

- **Le tunnel ne passe pas encore par le service.** Le moteur écoute donc toujours sur le réseau local, comme aux jalons précédents, et la règle de pare-feu reste nécessaire. Quand le service portera les extrémités de tunnel, le moteur se refermera sur la machine locale et une seule règle suffira, pour `zyrdeskd`.
- **L'appairage demande encore quelqu'un devant l'hôte.** Le code à quatre chiffres se tape à la main : c'est le serveur de mise en relation, au jalon M5, qui rendra ça automatique.
- **L'arrêt du moteur est brutal.** Le service le termine au lieu de lui demander poliment de s'en aller. Sans réglage d'affichage à restaurer, ça ne coûte rien aujourd'hui ; ça changera avec l'écran virtuel.
- **Aucune interface.** Tout passe par la ligne de commande jusqu'au jalon M4.

---

## 10. Si quelque chose ne va pas

**`zyrdeskd install` refuse.** La fenêtre n'est pas administrateur. Le service s'inscrit auprès de Windows, ce qu'un utilisateur ordinaire n'a pas le droit de faire.

**Le journal dit `host engine not found`.** Le service cherche les moteurs au même endroit que `zyr-cli`, à savoir le dossier `data` du projet. Il tourne sous le compte système : si le projet est sur une clé USB, un disque réseau ou dans un dossier d'utilisateur protégé, ce compte peut ne pas y accéder. Déplacer le projet sur un disque local ordinaire.

**Le journal dit `no session on screen, waiting for one`.** Aucune session n'est attachée à l'écran, ce qui arrive brièvement entre deux ouvertures. Si le message se répète sans fin, c'est que la machine n'a pas d'écran de connexion actif : vérifier qu'il ne s'agit pas d'une session Bureau à distance Windows, qui déplace l'écran ailleurs.

**L'image est noire alors que la connexion tient.** Relire le numéro de session du journal et le comparer à celui du gestionnaire des tâches. S'ils diffèrent, le moteur a été lancé dans la mauvaise session.

**`zyr-cli host pin` dit qu'aucun accès distant n'est actif.** Le fichier `data\host-runtime.conf` est écrit par le service, sous le compte système, et relu par la commande sous le compte utilisateur. Vérifier qu'il existe et qu'il est lisible.
