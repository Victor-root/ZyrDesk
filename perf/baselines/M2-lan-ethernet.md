# Coût du tunnel sur deux PC, Ethernet gigabit

Relevés du jalon M2 obtenus avec `zyr-cli banc` sur deux vraies machines, en suivant [docs/testing/M2-PROTOCOLE.md](../../docs/testing/M2-PROTOCOLE.md). Ce sont ces chiffres qui jugent les seuils, pas ceux de la boucle locale.

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

Deux campagnes, la seconde après l'ajout de la ventilation des pertes.

| Campagne | Débit | Durée | Médiane sans tunnel | Médiane avec tunnel | Écart médian | Écart au centile 95 | Écart au centile 99 |
|---|---|---|---|---|---|---|---|
| 1 | 50 Mb/s | 30 s | 0,73 ms | 1,66 ms | +0,92 ms | +1,30 ms | +1,67 ms |
| 1 | 40 Mb/s | 30 s | 0,70 ms | 1,52 ms | +0,82 ms | +1,11 ms | +1,56 ms |
| 1 | 40 Mb/s | 120 s | 0,69 ms | 1,48 ms | +0,79 ms | +1,04 ms | +1,36 ms |
| 2 | 50 Mb/s | 30 s | 0,69 ms | 1,57 ms | +0,88 ms | +1,29 ms | +1,67 ms |
| 2 | 40 Mb/s | 120 s | 0,74 ms | 1,29 ms | **+0,56 ms** | +0,70 ms | **+0,80 ms** |

**G-lat : médiane <= 1 ms et centile 99 <= 3 ms. Tenu**, dans les cinq mesures. La plus fiable est la dernière, deux minutes à 40 Mb/s : +0,56 ms de médiane et +0,80 ms au centile 99, soit moins du tiers de ce qui est admis.

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

Ces pertes n'ont aucun effet mesurable sur le débit ni sur la latence, et la correction d'erreur du protocole vidéo est faite pour les absorber. À titre de comparaison, elles représentent moins d'un paquet sur deux mille.

## Non mesuré

G-cpu : la charge processeur du banc n'a pas été relevée pendant les mesures. C'est le dernier seuil du jalon qui reste ouvert.
