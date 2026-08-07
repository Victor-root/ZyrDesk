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

| Débit | Durée | Médiane sans tunnel | Médiane avec tunnel | Écart médian | Écart au centile 95 | Écart au centile 99 |
|---|---|---|---|---|---|---|
| 50 Mb/s | 30 s | 0,73 ms | 1,66 ms | +0,92 ms | +1,30 ms | +1,67 ms |
| 40 Mb/s | 30 s | 0,70 ms | 1,52 ms | +0,82 ms | +1,11 ms | +1,56 ms |
| 40 Mb/s | 120 s | 0,69 ms | 1,48 ms | +0,79 ms | +1,04 ms | +1,36 ms |

**G-lat : médiane <= 1 ms et centile 99 <= 3 ms. Tenu**, avec de la marge sur le centile 99. La mesure longue est la plus fiable des trois, et c'est la meilleure.

Le débit visé est tenu partout : 49,4 Mb/s pour 50 demandés, 39,6 Mb/s pour 40, à l'identique avec et sans tunnel.

## Débit sous perte provoquée

Perte injectée sous le transport, à 40 Mb/s pendant 30 secondes.

| Perte provoquée | Perte constatée bout en bout | Débit tenu | Médiane avec tunnel | Centile 99 |
|---|---|---|---|---|
| 0 % | 0,06 % | 39,6 Mb/s | 1,52 ms | 2,71 ms |
| 1 % | 1,76 % | 39,6 Mb/s | 1,69 ms | 3,56 ms |
| 2 % | 2,57 % | 39,6 Mb/s | 1,69 ms | 3,46 ms |

**G-loss : débit utile >= 95 % du nominal. Tenu**, à 99 % dans les trois cas. Le débit ne bouge pas d'un mégabit quand la perte double, et la latence médiane augmente de 0,17 ms seulement. C'est la propriété dont dépend toute l'architecture réseau.

Réserve : la condition complète de G-loss demande 25 ms d'aller-retour, et ce lien en a 0,7. C'est le produit perte x aller-retour qui fait s'effondrer un contrôleur de congestion ordinaire ; la condition exacte se mesurera sur un chemin distant au jalon M5.

## Point ouvert : d'où viennent les pertes résiduelles

Deux observations que ces relevés ne permettent pas d'expliquer.

À 50 Mb/s sans perte provoquée, le tunnel perd 0,59 % des paquets là où le chemin nu n'en perd que 0,10 %. À 40 Mb/s, au contraire, le tunnel perd moins que le chemin nu (0,06 % contre 0,11 %). Il y a donc un seuil entre les deux.

Sous perte provoquée, la perte constatée dépasse la perte injectée : 1,76 % pour 1 % injecté, 2,57 % pour 2 %. L'écart n'est pas proportionnel, ce qui écarte une erreur de calcul du taux d'injection.

Ces deux observations ont une cause plausible commune : quand la file d'émission du transport est pleine, il y fait place en jetant silencieusement les datagrammes les plus anciens. C'est le bon comportement pour de la vidéo, une image périmée ne valant plus rien, mais le banc ne savait pas le distinguer d'une perte du réseau. Le banc ventile désormais les deux causes ; la mesure reste à refaire pour trancher.

Aucune de ces pertes n'a d'effet mesurable sur le débit ni sur la latence, et la correction d'erreur du protocole vidéo est faite pour les absorber. Le point est ouvert par principe, pas parce qu'il gêne.

## Non mesuré

G-cpu : la charge processeur du banc n'a pas été relevée pendant les mesures.
