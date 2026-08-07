# Jalon M2 : mesurer ce que coûte le tunnel

Ce document se déroule sur les deux mêmes PC Windows qu'au jalon M1. Il ne fait pas de session distante : il mesure le chemin, seul, pour répondre à une question dont dépend toute l'architecture réseau.

**La question.** ZyrDesk fait passer tout le trafic par un tunnel chiffré, y compris en réseau local. C'est ce qui permet de n'ouvrir qu'un seul port, de chiffrer de bout en bout et, plus tard, de basculer d'un relais vers une connexion directe sans que la session s'en aperçoive. Mais ça n'a de sens que si le tunnel ne coûte presque rien. Si les mesures d'ici sont mauvaises, la décision est révisée.

Aucun moteur n'est nécessaire : le banc ne mesure que le chemin.

Vocabulaire : **PC hôte** = celui qu'on contrôlerait. **PC client** = celui depuis lequel on se connecterait.

---

## 1. Préparation

Sur les **deux PC**, compiler en release. Ce point n'est pas un détail : en mode debug, le tunnel mesure quatre fois plus cher, et le chiffre n'a aucune valeur.

```
cargo build --release
```

Sur chaque PC, afficher son empreinte :

```
zyr-cli identite
```

Chaque machine en a une, créée à la première demande, conservée dans `data\identite`, et qui ne change plus. C'est elle que l'autre ordinateur épingle : les deux PC ne s'acceptent que s'ils connaissent celle d'en face.

Noter les deux empreintes. Le plus simple est de copier celle du PC hôte dans un message vers le PC client, et l'inverse.

Sur le **PC hôte**, autoriser le banc dans le pare-feu Windows, une seule fois, dans une fenêtre **administrateur** :

```powershell
New-NetFirewallRule -DisplayName "ZyrDesk (banc de mesure)" -Direction Inbound `
  -Program "$PWD\target\release\zyr-cli.exe" -Action Allow
```

Sans cette règle, le PC client ne joindra rien et la mesure échouera sur un délai d'attente.

---

## 2. Mesure de référence

Sur le **PC hôte** :

```
zyr-cli banc hote --pair <empreinte du PC client>
```

Il affiche son port d'écoute et attend. Le laisser tourner : il sert toutes les mesures.

Sur le **PC client** :

```
zyr-cli banc client <adresse IP du PC hote> --pair <empreinte du PC hote> --duree 30
```

Le banc mesure deux fois le même trajet, avec les mêmes paquets à la même cadence : une fois en UDP nu, une fois à travers le tunnel complet. Les paquets partent par rafales, une par image, comme le fait un encodeur vidéo. Compter environ une minute.

> **M2-R1 (coût du tunnel à 50 Mb/s)**
>
> Recopier le bloc « Ce que coûte le tunnel » en entier, et les deux blocs de détail au-dessus.
>
> **Seuil à tenir (G-lat) : médiane <= 1 ms, centile 99 <= 3 ms.**
>
> Si la médiane dépasse 1 ms, ce n'est pas forcément grave, mais il faut le savoir : noter aussi si les deux PC sont en Wi-Fi, et si oui, refaire la mesure en Ethernet. Le Wi-Fi ajoute une gigue qui écrase le signal qu'on cherche.

---

## 3. Le tunnel tient-il sous la perte ?

C'est le point le plus important du jalon. Un contrôle de congestion ordinaire prend une perte de paquet pour un ordre de ralentir : à 1 % de perte et 25 ms d'aller-retour, il descend vers 5 Mb/s, alors qu'une session confortable en demande quarante. ZyrDesk embarque un contrôleur qui refuse ce raisonnement, parce que le débit est déjà fixé par l'encodeur et que les pertes sont réparées par la correction d'erreur du protocole vidéo.

Le banc sait provoquer une perte réelle sous le tunnel, exprimée pour mille paquets émis. Sur le **PC client**, enchaîner :

```
zyr-cli banc client <adresse IP du PC hote> --pair <empreinte du PC hote> --debit 40 --duree 30 --perte 0
zyr-cli banc client <adresse IP du PC hote> --pair <empreinte du PC hote> --debit 40 --duree 30 --perte 10
zyr-cli banc client <adresse IP du PC hote> --pair <empreinte du PC hote> --debit 40 --duree 30 --perte 20
```

> **M2-R2 (débit sous perte)**
>
> Pour chacune des trois mesures, noter le « débit tenu » et la « perte » du bloc « À travers le tunnel ».
>
> **Seuil à tenir (G-loss) : le débit tenu ne descend pas sous 38 Mb/s, soit 95 % de 40, dans les trois cas. La perte constatée reste proche de la perte provoquée : si elle double ou triple, c'est que le tunnel amplifie, et c'est un problème.**

---

## 4. Coût en processeur

Rien à ouvrir : le banc lit son propre temps processeur, sur exactement la fenêtre qu'il mesure. Sur le **PC client** :

```
zyr-cli banc client <adresse IP du PC hote> --pair <empreinte du PC hote> --debit 40 --duree 120
```

Le PC client affiche un bloc « Processeur de ce banc » à la fin. Le PC hôte affiche sa propre ligne dans sa fenêtre, à la fin de chaque mesure.

> **M2-R3 (processeur)**
>
> Recopier le bloc « Processeur de ce banc » du PC client, et la ligne en pourcentage du PC hôte.
>
> **Seuil à tenir (G-cpu) : au plus 8 % d'un coeur à 40 Mb/s.** Cent pour cent vaut un coeur entier saturé, indépendamment du nombre de coeurs de la machine.
>
> Attention : le banc travaille dans les deux sens en même temps, alors qu'une vraie session n'en fait qu'un par extrémité. Un résultat au double du seuil reste donc acceptable ; le noter tel quel.

---

## 5. Taille de paquet retenue

> **M2-R4**
>
> La valeur « taille de paquet » affichée dans le bloc « À travers le tunnel », et la présence ou non de la mention « réduite par le chemin ».
>
> Attendu sur un réseau local en Ethernet : au-dessus de 1300 octets. Nettement en dessous, c'est que quelque chose rétrécit le chemin (VPN, Wi-Fi maillé, tunnel de la box) et il faudra le savoir avant d'accuser ZyrDesk.

---

## 6. Ce que le jalon suivant attend de ces mesures

- M2-R1 et M2-R3 décident du maintien du tunnel systématique (décision D3 dans [DECISIONS.md](../DECISIONS.md)).
- M2-R2 valide le contrôleur de congestion média, sans lequel toute l'architecture réseau tombe.
- M2-R4 alimente le budget de taille de paquet ([NETWORK.md](../NETWORK.md), section 4).

---

## 7. Ce que la première campagne a donné

Deux PC en Ethernet gigabit, relevés complets dans [perf/baselines/M2-lan-ethernet.md](../../perf/baselines/M2-lan-ethernet.md).

| Mesure | Résultat | Seuil | Verdict |
|---|---|---|---|
| M2-R1, coût du tunnel à 50 Mb/s | +0,88 ms de médiane, +1,67 ms au centile 99 | médiane <= 1 ms, centile 99 <= 3 ms | tenu |
| M2-R1, à 40 Mb/s sur 2 minutes | +0,56 ms de médiane, +0,80 ms au centile 99 | idem | tenu |
| M2-R2, débit sous 1 % et 2 % de perte | 39,6 Mb/s dans les trois cas | >= 38 Mb/s | tenu |
| M2-R3, processeur | non relevé | <= 8 % d'un coeur | à faire |
| M2-R4, taille de paquet | 1353 octets | > 1300 | tenu |

La question des pertes résiduelles est tranchée : la file d'émission du tunnel n'y est pour rien, zéro datagramme jeté faute de place aux deux débits. Le reste est de la perte réseau ordinaire, moins d'un paquet sur deux mille, sans effet sur le débit ni sur la latence.

Les relevés en boucle locale sur la machine de développement restent dans [perf/baselines/M2-boucle-locale.md](../../perf/baselines/M2-boucle-locale.md). Ils ne servent que de garde contre une régression : ils n'ont ni carte réseau, ni pare-feu, ni aller-retour réaliste.

---

## 8. Si quelque chose ne va pas

**« connexion impossible : timed out » côté client.** Le pare-feu du PC hôte bloque, ou l'adresse IP est mauvaise. Vérifier l'adresse avec `ipconfig` sur le PC hôte, et refaire la règle de pare-feu de la section 1.

**« empreinte du pair inattendue ».** Les deux empreintes ont été inversées, ou recopiées incomplètement. Chacune fait exactement 64 caractères. Le `--pair` du PC hôte est l'empreinte du PC **client**, et inversement.

**« identité incomplète ».** Un des deux fichiers de `data\identite` a été effacé. Effacer le dossier entier pour en refaire une, en sachant que l'autre PC devra recevoir la nouvelle empreinte.

**Des chiffres qui n'ont aucun sens** (médiane de plusieurs millisecondes en Ethernet, débit très en dessous de la consigne). Vérifier d'abord que la compilation est bien en release : `zyr-cli` doit venir de `target\release`, pas de `target\debug`.
