# Coût du tunnel sur deux PC, Ethernet gigabit

Relevés du jalon M2 obtenus avec `zyr-cli bench` sur deux vraies machines, en suivant [docs/testing/M2-PROTOCOLE.md](../../docs/testing/M2-PROTOCOLE.md). Ce sont ces chiffres qui jugent les seuils, pas ceux de la boucle locale.

## Conditions

| Élément | Valeur |
|---|---|
| Liaison | Ethernet 1 Gb/s, réseau local, aucun Wi-Fi dans le trajet |
| Compilation | release |
| Aller-retour du chemin nu | environ 0,70 ms de médiane |
| Trajet mesuré | tunnel complet, canal vidéo, ports des moteurs compris |
| Taille de paquet | 1353 octets |
| Cadence | rafales de paquets, une par image, 60 images par seconde |

Le choix de l'Ethernet est délibéré : le Wi-Fi ajoute une gigue de plusieurs millisecondes qui noierait le signal recherché, lequel est de l'ordre de la milliseconde.

## Coût du tunnel

Quatre campagnes, chacune après une correction issue de la précédente.

| Campagne | Débit | Durée | Médiane sans tunnel | Médiane avec tunnel | Écart médian | Écart au centile 95 | Écart au centile 99 |
|---|---|---|---|---|---|---|---|
| 1 | 50 Mb/s | 30 s | 0,73 ms | 1,66 ms | +0,92 ms | +1,30 ms | +1,67 ms |
| 1 | 40 Mb/s | 30 s | 0,70 ms | 1,52 ms | +0,82 ms | +1,11 ms | +1,56 ms |
| 1 | 40 Mb/s | 120 s | 0,69 ms | 1,48 ms | +0,79 ms | +1,04 ms | +1,36 ms |
| 2 | 50 Mb/s | 30 s | 0,69 ms | 1,57 ms | +0,88 ms | +1,29 ms | +1,67 ms |
| 2 | 40 Mb/s | 120 s | 0,74 ms | 1,29 ms | **+0,56 ms** | +0,70 ms | **+0,80 ms** |
| 3 | 40 Mb/s | 120 s | 0,71 ms | 1,25 ms | **+0,54 ms** | +0,74 ms | **+0,81 ms** |
| 4 | 40 Mb/s | 120 s | 0,74 ms | 1,29 ms | **+0,55 ms** | +0,75 ms | **+0,93 ms** |

**G-lat : médiane <= 1 ms et centile 99 <= 3 ms. Tenu**, dans les sept mesures. Les plus fiables sont les mesures de deux minutes à 40 Mb/s, qui se répètent à trois reprises autour de +0,55 ms de médiane et +0,85 ms au centile 99, soit environ le tiers de ce qui est admis.

Le débit visé est tenu partout : 49,4 Mb/s pour 50 demandés, 39,6 Mb/s pour 40, à l'identique avec et sans tunnel.

Le pire cas relevé va de 3,11 ms à 19,31 ms selon les campagnes, pour un centile 99 qui reste entre 2 et 3 ms : ce sont des accidents isolés, de l'ordre d'un paquet sur cent mille, qu'une correction d'erreur absorbe sans que rien ne se voie.

## Débit sous perte provoquée

Perte injectée sous le transport, à 40 Mb/s pendant 30 secondes.

| Perte provoquée | Perte constatée bout en bout | Débit tenu | Médiane avec tunnel | Centile 99 |
|---|---|---|---|---|
| 0 % | 0,06 % | 39,6 Mb/s | 1,52 ms | 2,71 ms |
| 1 % | 1,76 % | 39,6 Mb/s | 1,69 ms | 3,56 ms |
| 2 % | 2,57 % | 39,6 Mb/s | 1,69 ms | 3,46 ms |

**G-loss : débit utile >= 95 % du nominal. Tenu**, à 99 % dans les trois cas. Le débit ne bouge pas d'un mégabit quand la perte double, et la latence médiane augmente de 0,17 ms seulement. C'est la propriété dont dépend toute l'architecture réseau.

Réserve : la condition complète de G-loss demande 25 ms d'aller-retour, et ce lien en a 0,7. C'est le produit perte x aller-retour qui fait s'effondrer un contrôleur de congestion ordinaire ; la condition exacte se mesurera sur un chemin distant au jalon M5.

## D'où viennent les pertes résiduelles

La première campagne laissait une question ouverte : le tunnel perdait 0,19 % à 0,59 % des paquets là où le chemin nu n'en perdait presque aucun. L'hypothèse était la file d'émission du transport, qui fait place aux nouveaux datagrammes en jetant silencieusement les plus anciens. C'est le bon comportement pour de la vidéo, une image périmée ne valant plus rien, mais le banc ne savait pas le distinguer d'une perte du réseau.

Le banc ventile désormais les deux causes, des deux côtés du tunnel. Réponse de la seconde campagne :

| Débit | Paquets manquants | Jetés faute de place | Perdus à l'aller |
|---|---|---|---|
| 50 Mb/s sur 30 s | 257 sur 136 876 | 0 | 23 |
| 40 Mb/s sur 120 s | 190 sur 439 261 | 0 | 17 |

**La file d'émission n'y est pour rien**, à aucun des deux débits. Le dimensionnement retenu, 128 Kio, tient le débit visé sans jamais déborder.

Le reste se joue sur le trajet retour, invisible depuis le côté qui mesure : le transport ne constate les pertes que par les acquittements qui lui reviennent, donc chaque extrémité ne connaît que ce qu'elle a émis. Le banc hôte affiche maintenant sa propre ventilation à la fin de chaque mesure ; il suffit de lire son terminal pour fermer le compte.

Ces pertes n'ont aucun effet mesurable sur le débit ni sur la latence. Leur cause réelle est établie plus bas ; une fois corrigée, il n'en reste que deux sur la mesure longue, toutes deux constatées par le transport lui-même.

## Processeur

Relevé par le banc lui-même, sur la fenêtre exacte de chaque salve, à 40 Mb/s pendant 120 secondes.

| Côté | Sans tunnel | Avec tunnel | Coût pour 80 Mb/s traversés | Équivalent session à 40 Mb/s | Coeurs |
|---|---|---|---|---|---|
| Client | 5,2 % | 15,5 % | +10,2 points | **5,1 points** | 12 |
| Hôte | 3,7 % | 13,1 % | +9,3 points | **4,7 points** | 24 |

**G-cpu : au plus 8 % d'un coeur à 40 Mb/s. Tenu**, à 5,1 et 4,7 points.

Le chiffre brut n'est pas directement comparable au seuil, et c'est pour cela que le banc en affiche deux. Il émet et reçoit à la fois, si bien que chaque extrémité voit passer deux fois le débit demandé, là où une session réelle n'en fait qu'un sens : l'hôte envoie la vidéo, le client la reçoit. Le second chiffre ramène le coût à ce qu'une session paierait, en supposant que le calcul suit le nombre de paquets traités. Une mesure strictement unidirectionnelle demanderait de dire à l'autre banc d'émettre, donc le canal de contrôle propre à ZyrDesk, qui n'a pas encore de contenu.

Ces relevés varient d'une campagne à l'autre avec la charge des machines : la même mesure avait donné 7,5 points côté client la veille. Vérifié en boucle locale sur trois exécutions identiques, où l'écart ne dépasse pas 0,2 point : la variation vient des machines de test, pas de la mesure.

## Les tampons de socket, et ce qu'ils coûtaient

Les 817 paquets manquants de la troisième campagne n'étaient expliqués ni par la file d'émission du tunnel (zéro jeté), ni par le transport (zéro perdu, des deux côtés). Ils étaient donc jetés par le noyau, dans les tampons des sockets qui relient le moteur au tunnel.

C'est cohérent avec leur dimensionnement d'alors, celui du système : souvent 64 Kio, soit une dizaine de millisecondes de vidéo à ce débit. Il suffit que la pompe soit privée de processeur le temps d'une préemption pour que le noyau jette, en silence et sans que rien ne le compte. Ces sockets demandent maintenant quatre mébioctets, ce qui couvre largement une interruption d'ordonnancement. Le système peut n'en accorder qu'une partie, sans que ce soit une erreur.

Cette correction était prévue par [NETWORK.md](../../docs/NETWORK.md) ; elle est simplement arrivée plus tôt que le jalon M3, faute d'autre explication au chiffre.

Résultat à la campagne suivante, dans les mêmes conditions : **2 paquets manquants sur 439 261**, contre 817. Et les deux sont expliqués, le transport les ayant lui-même constatés perdus au retour. Le compte est donc exact, sans reliquat.
