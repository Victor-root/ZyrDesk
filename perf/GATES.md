# Seuils de performance

Ces seuils sont les critères de sortie chiffrés de la feuille de route. Un jalon n'est pas terminé tant qu'ils ne sont pas mesurés et tenus. « Une session qui semble fonctionner » n'est jamais un critère.

Le banc de mesure existe à partir du jalon M2 : les décisions d'architecture réseau en dépendent, il ne peut pas arriver en fin de projet.

## Définitions

| Code | Seuil | Mesure |
|---|---|---|
| G-lat | Latence ajoutée par le tunnel : médiane <= 1 ms, p99 <= 3 ms | Comparaison directe contre le même flux sans tunnel (mode diagnostic), mêmes machines, même session |
| G-loss | À 40 Mb/s, 25 ms d'aller-retour et 1 % de perte pendant 10 min : débit utile >= 95 % du nominal, aucun gel visible > 250 ms | Profil réseau simulé, statistiques du moteur client + compteurs du tunnel |
| G-cpu | Processus tunnel <= 8 % d'un cœur à 40 Mb/s | Temps processeur relevé par le banc lui-même, sur la fenêtre exacte de chaque salve |
| G-start | Clic « Se connecter » vers première image : <= 4 s en réseau local, <= 8 s via Internet | Chronométrage sur 10 essais, médiane |
| G-frame | p99 de l'intervalle entre images affichées <= 20 ms sur 5 min | Statistiques du moteur client |

## Outil de mesure

Depuis le jalon M2, `zyr-cli bench` mesure G-lat, G-loss et G-cpu sans qu'aucun moteur soit nécessaire : il envoie ses propres paquets par rafales, une par image, comme le fait un encodeur, et mesure deux fois le même trajet, avec et sans tunnel. Il sait aussi provoquer une perte réelle sous le transport (`--loss`, en pour mille), ce qui exerce ses vrais mécanismes de détection.

Trois chiffres sont relevés par le programme lui-même plutôt que par l'opérateur : le temps processeur, sur la fenêtre exacte de chaque salve ; la part des paquets manquants due à une file d'émission pleine plutôt qu'au réseau ; et la taille de paquet que le chemin permet. Un relevé pris à la main dans un gestionnaire de tâches échantillonne, arrondit, et ne couvre pas la bonne fenêtre.

Deux réserves de lecture. Le temps processeur porte sur tout le programme, fils compris : le banc enchaîne donc ses deux salves au lieu de les mener de front. Et chaque extrémité ne constate les pertes que sur ce qu'elle a émis, le transport ne les détectant que par les acquittements qui lui reviennent : le trajet retour se lit dans la fenêtre de l'autre banc.

Marche à suivre sur deux PC : [docs/testing/M2-PROTOCOLE.md](../docs/testing/M2-PROTOCOLE.md). Toujours en version release : en mode debug, le tunnel mesure environ quatre fois son coût réel.

Ce que le banc ne sait pas encore faire : simuler un aller-retour. Or c'est le produit perte x aller-retour qui fait s'effondrer un contrôleur de congestion ordinaire. La condition exacte de G-loss (25 ms d'aller-retour, 10 minutes) se mesurera donc sur un vrai chemin distant au jalon M5. En attendant, la partie « le débit ne s'effondre pas sous la perte » est vérifiée, et la propriété du contrôleur est gardée par un test qui le compare au contrôleur ordinaire du transport.

## Sources de mesure

- Moteur client : images par seconde reçues, décodées et rendues ; latence hôte ; pertes réseau et pertes par gigue ; latence réseau et variance ; temps de décodage ; délai de file ; temps de rendu.
- Tunnel : paquets et octets par canal, datagrammes jetés sur file pleine, aller-retour QUIC, chemin actif (direct ou relais), migrations de chemin.
- Moteur hôte : encodeur retenu, images capturées et encodées, temps d'encodage.
- Système : charge processeur et graphique par processus.

## Latence bout en bout réelle (photon à photon)

Procédure manuelle, réalisable sans compétence technique, qui sert d'arbitre quand les compteurs se contredisent :

1. Afficher sur le PC hôte un chronomètre au millième de seconde.
2. Filmer simultanément l'écran hôte et l'écran client avec un téléphone à 240 images par seconde.
3. Lire la vidéo image par image, relever l'écart entre les deux affichages.
4. Répéter 10 fois. Retenir la médiane et le 95e centile.

## Comparaisons obligatoires

| Quand | Contre quoi | Attendu |
|---|---|---|
| Jalon M1 | Couple de moteurs officiels non pilotés, mêmes machines | Écart <= 5 % : notre pilotage ne doit rien coûter |
| Jalon M2 | Mesures de M1 | G-lat, G-loss, G-cpu tenus, sinon la décision du tunnel systématique est révisée |
| Chaque version | Version précédente | Aucune régression au-delà des marges, sinon la publication est bloquée |
| Mise à niveau d'un moteur | Épinglage précédent | Aucune régression, sinon la mise à niveau est investiguée avant fusion |

## Bases de comparaison

Les relevés de référence sont versionnés dans ce dossier au fur et à mesure, avec la description du matériel et des conditions réseau de chaque campagne.
