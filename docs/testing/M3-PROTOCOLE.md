# Jalon M3 : accès distant sans personne devant la machine

Ce document se déroule sur les deux mêmes PC Windows qu'aux jalons M1 et M2. Il vérifie ce qui sépare un bricolage d'un vrai produit : **le PC hôte est joignable sans que personne n'ait ouvert de session dessus**, il le reste quand la session change, et **un seul port est ouvert sur la machine**.

Jusqu'ici il fallait ouvrir une session Windows, lancer une fenêtre de commandes en administrateur et laisser `zyr-cli host start` tourner : autrement dit, il fallait déjà être devant le PC qu'on voulait contrôler à distance. Le service supprime ça.

Vocabulaire : **PC hôte** = celui qu'on contrôle. **PC client** = celui depuis lequel on se connecte.

---

## 1. Ce qui change, et pourquoi c'est délicat

**La session.** Windows range chaque utilisateur connecté dans une session numérotée. Un service, lui, tourne dans une session à part, la session 0, qui n'a ni écran ni bureau. Un moteur de capture démarré là ne verrait rigoureusement rien.

Le service ZyrDesk démarre donc le moteur **dans la session attachée à l'écran physique**, celle où s'affiche l'écran de connexion Windows. Il le fait avec son propre jeton d'accès, celui du compte système, simplement déplacé vers cette session. C'est ce détail qui permet de voir l'écran de connexion et les invites de sécurité, qu'un jeton d'utilisateur ordinaire n'a pas le droit de capturer. Cette session change dès que quelqu'un se connecte, se déconnecte ou change d'utilisateur : le service la surveille et relance le moteur dans la nouvelle.

**Le tunnel.** Le moteur n'écoute plus sur le réseau : il est refermé sur la machine locale, injoignable de l'extérieur. Tout passe par le tunnel chiffré que porte le service, qui multiplexe les sept ports du moteur dans une seule connexion. D'où un seul port ouvert dans le pare-feu, le **47000 en UDP**.

**Deux autorisations, à ne pas confondre.** Le tunnel n'accepte que les ordinateurs dont l'empreinte est inscrite sur l'hôte (`zyr-cli host authorize`). Une fois à l'intérieur, le moteur demande en plus son code à quatre chiffres, une seule fois par paire d'ordinateurs. La première est celle de ZyrDesk, la seconde celle du moteur.

---

## 2. Préparation

Sur les **deux PC**, compiler en release et vérifier que les moteurs sont en place :

```
cargo build --release
zyr-cli engines status
```

Sur **chaque PC**, afficher son empreinte et la noter :

```
zyr-cli identity
```

Elle fait 64 caractères, ne change plus une fois créée, et c'est elle que l'autre ordinateur épingle. Le plus simple est de se les envoyer par message.

Sur le **PC hôte**, autoriser le PC client, puis vérifier :

```
zyr-cli host authorize <empreinte du PC client>
zyr-cli host devices
```

> Attention au sens : sur l'hôte on inscrit l'empreinte du **client**, et sur le client on indiquera celle de l'**hôte**.

Toujours sur le **PC hôte**, ouvrir le port du tunnel dans une fenêtre **administrateur**, une seule fois :

```powershell
New-NetFirewallRule -DisplayName "ZyrDesk (tunnel)" -Direction Inbound `
  -Protocol UDP -LocalPort 47000 -Action Allow
```

L'installateur du produit pose cette règle lui-même ; en compilant depuis les sources, elle se fait à la main. C'est la seule : les moteurs n'écoutent plus sur le réseau.

---

## 3. Installer le service

Sur le **PC hôte**, dans une fenêtre **administrateur** :

```
zyrdeskd install
zyrdeskd start
zyrdeskd status
```

Attendu : « Service installé. Il démarrera avec Windows. », puis « Service démarré. », puis « En marche ».

Le service écrit tout ce qu'il fait dans `data\logs\service.log` :

```
notepad data\logs\service.log
```

> **M3-R1 (le service tient son moteur et ouvre le tunnel)**
>
> Recopier les lignes du journal. Attendu, dans l'ordre : `service started`, puis `engine started in session N, process P, on base port ...`, puis `tunnel open on port 47000, fingerprint of this computer ...`, puis `remote access active`.
>
> Le numéro de session compte : ce ne doit **pas** être 0. Zéro voudrait dire que le moteur est resté dans la session du service, celle sans écran.

Vérifier aussi dans le gestionnaire des tâches, onglet « Détails » (clic droit sur les en-têtes de colonnes pour ajouter « ID de session ») : `zyrdesk-host-engine.exe` doit porter le numéro de processus du journal et le même numéro de session.

---

## 4. Première session, et appairage du moteur

L'appairage du moteur demande quelqu'un devant l'hôte pour taper le code : il se fait donc **maintenant**, avant le test à froid. Une fois fait, il ne se refait plus.

Sur le **PC client** :

```
zyr-cli connect <adresse IP du PC hote> --pair <empreinte du PC hote> --stats
```

La commande affiche « Tunnel établi », puis un code à quatre chiffres et la commande exacte à lancer sur l'hôte. Sur le **PC hôte**, dans une autre fenêtre :

```
zyr-cli host pin <le-code-affiche>
```

> **M3-R2 (session à travers le tunnel)**
>
> Attendu : le bureau du PC hôte s'affiche sur le PC client, comme au jalon M1, mais cette fois par le tunnel.
>
> Noter : le tunnel s'établit-il ? La ligne « Taille de paquet réduite par le chemin » apparaît-elle, et avec quelle valeur ? La session est-elle aussi fluide qu'au jalon M1 ?

---

## 5. Le moteur est-il vraiment injoignable ?

C'est la vérification qui donne son sens au tunnel. Sur le **PC client**, session fermée, dans PowerShell :

```powershell
Test-NetConnection <adresse IP du PC hote> -Port 42000
Test-NetConnection <adresse IP du PC hote> -Port 47000 -InformationLevel Quiet
```

> **M3-R3 (une seule porte)**
>
> Attendu : le premier test **échoue** (`TcpTestSucceeded : False`), le moteur n'écoutant plus que sur la machine hôte elle-même. Le second ne prouve rien en UDP et sert seulement de contrôle.
>
> Si le port 42000 répond, c'est qu'un `zyr-cli host start` traîne quelque part : il ouvre le moteur au réseau, c'est son rôle de mode diagnostic. L'arrêter et refaire le test.

---

## 6. Le vrai test : se connecter avant toute ouverture de session

Sur le **PC hôte** :

1. Redémarrer complètement le PC.
2. **Ne pas se connecter.** Rester sur l'écran de connexion Windows, ou même simplement s'éloigner.

Sur le **PC client**, une fois l'hôte redémarré, laisser une minute au service puis :

```
zyr-cli connect <adresse IP du PC hote> --pair <empreinte du PC hote> --stats
```

> **M3-R4 (accès avant ouverture de session)**
>
> Attendu : **l'écran de connexion Windows du PC hôte s'affiche sur le PC client**, et le clavier et la souris y répondent. Taper le mot de passe à distance doit ouvrir la session, et le bureau doit apparaître ensuite.
>
> Noter : l'écran de connexion s'affiche-t-il ? Le clavier répond-il ? La session s'ouvre-t-elle ? Après l'ouverture, l'image revient-elle toute seule, et au bout de combien de secondes ?
>
> C'est le critère principal du jalon. Sans lui, rien d'autre ici ne compte.

L'ouverture de session change la session attachée à l'écran : le service arrête le moteur et le relance dans la nouvelle. Le PC client perd donc l'image quelques secondes. Relire le journal après coup :

```
notepad data\logs\service.log
```

Attendu : une ligne `the screen left session N, the engine starts over in the new one`, puis un nouveau `engine started in session M` avec un autre numéro.

---

## 7. Invite de sécurité (UAC)

Session ouverte, toujours depuis le PC client, lancer sur l'hôte quelque chose qui déclenche une demande d'élévation : par exemple un clic droit sur l'invite de commandes, « Exécuter en tant qu'administrateur ».

> **M3-R5 (bureau sécurisé)**
>
> Attendu : l'invite bleue s'affiche sur le PC client, et le bouton « Oui » est cliquable à distance.
>
> Noter : l'écran devient-il noir ? L'invite est-elle visible ? Peut-on cliquer dedans ?
>
> Un écran noir à cet instant est un vrai défaut, pas un détail d'affichage : il signifie que le moteur n'a pas le droit de capturer le bureau sécurisé, et il faudra le signaler tel quel.

---

## 8. Verrouillage, déverrouillage, changement d'utilisateur

Toujours connecté depuis le PC client, sur l'hôte :

1. Verrouiller la session (Windows + L), attendre dix secondes, déverrouiller.
2. Si le PC hôte a un second compte : changer d'utilisateur, ouvrir l'autre session, puis revenir.

> **M3-R6 (transitions de session)**
>
> Pour chacune des deux manipulations : l'image revient-elle toute seule ? Au bout de combien de secondes ? Faut-il relancer quoi que ce soit à la main ?
>
> **Seuil à tenir : coupure de 5 secondes au plus, récupérée sans intervention.**

---

## 9. Le service se relève

Toujours sur l'hôte, dans le gestionnaire des tâches, **terminer de force** le processus `zyrdesk-host-engine.exe`.

> **M3-R7 (relance après incident)**
>
> Attendu : le service le relance tout seul, et le journal l'écrit (`engine stopped (code ...) after N s, restarting in 0 s`).
>
> Noter au bout de combien de temps le PC client retrouve l'image.
>
> La règle appliquée : un moteur qui a tenu plus d'une minute est relancé immédiatement ; un moteur qui retombe aussitôt est relancé de plus en plus lentement, et abandonné après cinq échecs rapprochés, plutôt que de tourner en boucle en masquant la panne.

---

## 10. Arrêt et désinstallation propres

Sur le **PC hôte**, en administrateur :

```
zyrdeskd stop
zyrdeskd status
zyrdeskd uninstall
```

> **M3-R8 (retrait sans résidu)**
>
> Attendu : « Service arrêté. », puis « Arrêté », puis « Service retiré. »
>
> Vérifier ensuite dans le gestionnaire des tâches qu'**aucun** `zyrdesk-host-engine.exe` ne survit. Un moteur orphelin après l'arrêt du service est un défaut : c'est exactement ce que l'objet de travail Windows est censé empêcher.
>
> Vérifier aussi que « ZyrDesk » a disparu de la console des services (`services.msc`).

---

## 11. Ce que ce jalon ne fait pas encore

- **Les empreintes s'échangent à la main.** C'est le serveur de mise en relation, au jalon M5, qui les fournira automatiquement ; le mécanisme de vérification, lui, ne changera pas.
- **L'appairage du moteur demande quelqu'un devant l'hôte**, une fois par paire d'ordinateurs, pour la même raison.
- **Une seule session sortante à la fois** sur le PC client : les ports locaux qui remplacent ceux de l'hôte sont pris par la première.
- **L'arrêt du moteur est brutal.** Le service le termine au lieu de lui demander poliment de s'en aller. Sans réglage d'affichage à restaurer, ça ne coûte rien aujourd'hui ; ça changera avec l'écran virtuel.
- **Aucune interface.** Tout passe par la ligne de commande jusqu'au jalon M4.

---

## 12. Si quelque chose ne va pas

**`zyrdeskd install` refuse.** La fenêtre n'est pas administrateur. Le service s'inscrit auprès de Windows, ce qu'un utilisateur ordinaire n'a pas le droit de faire.

**`zyrdeskd install` dit que le service existe déjà.** L'installateur du produit l'enregistre aussi : un ZyrDesk installé et un ZyrDesk compilé se disputent le même nom de service. Retirer celui qui ne sert pas (`zyrdeskd uninstall` depuis son propre dossier) avant d'inscrire l'autre.

**« a refusé cet ordinateur, ou son empreinte a changé ».** L'hôte n'a pas inscrit l'empreinte du client, ou les deux empreintes ont été inversées. Vérifier avec `zyr-cli host devices` sur l'hôte : la liste doit contenir ce qu'affiche `zyr-cli identity` sur le client.

**« ne répond pas sur le port 47000 ».** Le service n'est pas démarré (`zyrdeskd status`), la règle de pare-feu manque, ou l'adresse IP est mauvaise (`ipconfig` sur l'hôte).

**Le journal dit `no device authorised yet`.** Aucune empreinte n'est inscrite. Le service relit la liste toutes les cinq secondes : après un `zyr-cli host authorize`, il n'y a rien à redémarrer.

**Le journal dit `host engine not found`.** Le service cherche les moteurs au même endroit que `zyr-cli`, à savoir le dossier `data` du projet. Il tourne sous le compte système : si le projet est sur une clé USB, un disque réseau ou dans un dossier d'utilisateur protégé, ce compte peut ne pas y accéder. Déplacer le projet sur un disque local ordinaire.

**Le journal dit `no session on screen, waiting for one`.** Aucune session n'est attachée à l'écran, ce qui arrive brièvement entre deux ouvertures. Si le message se répète sans fin, c'est que la machine n'a pas d'écran de connexion actif : vérifier qu'il ne s'agit pas d'une session Bureau à distance Windows, qui déplace l'écran ailleurs.

**L'image est noire alors que la connexion tient.** Relire le numéro de session du journal et le comparer à celui du gestionnaire des tâches. S'ils diffèrent, le moteur a été lancé dans la mauvaise session.

**« les ports locaux n'ont pas pu être ouverts ».** Une autre session ZyrDesk est déjà ouverte sur le PC client. La fermer d'abord.

**`zyr-cli host pin` dit qu'aucun accès distant n'est actif.** Le fichier `data\host-runtime.conf` est écrit par le service, sous le compte système, et relu par la commande sous le compte utilisateur. Vérifier qu'il existe et qu'il est lisible.
