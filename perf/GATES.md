# Seuils de performance

Ces seuils sont les critères de sortie chiffrés de la feuille de route. Un jalon n'est pas terminé tant qu'ils ne sont pas mesurés et tenus. « Une session qui semble fonctionner » n'est jamais un critère.

Le banc de mesure existe à partir du jalon M2 : les décisions d'architecture réseau en dépendent, il ne peut pas arriver en fin de projet.

## Définitions

| Code | Seuil | Mesure |
|---|---|---|
| G-lat | Latence ajoutée par le tunnel : médiane <= 1 ms, p99 <= 3 ms | Comparaison directe contre le même flux sans tunnel (mode diagnostic), mêmes machines, même session |
| G-loss | À 40 Mb/s, 25 ms d'aller-retour et 1 % de perte pendant 10 min : débit utile >= 95 % du nominal, aucun gel visible > 250 ms | Profil réseau simulé, mesures du lecteur + compteurs du tunnel |
| G-cpu | Processus tunnel <= 8 % d'un cœur à 40 Mb/s | Temps processeur relevé par le banc lui-même, sur la fenêtre exacte de chaque salve |
| G-start | Clic « Se connecter » vers première image : <= 4 s en réseau local, <= 8 s via Internet | Chronométrage sur 10 essais, médiane |
| G-frame | p99 de l'intervalle entre images affichées <= 20 ms sur 5 min | Mesure du lecteur (`frame_interval_p99_ms`) |

## Outil de mesure

Depuis le jalon M2, `zyr-cli bench` mesure G-lat, G-loss et G-cpu sans le vrai moteur : il envoie ses propres paquets par rafales, une par image, comme le fait un encodeur, et mesure deux fois le même trajet, avec et sans tunnel. Le trajet par le tunnel passe aussi par les liaisons locales, comme les images d'une session, et c'est un moteur de remplacement qui renvoie chaque paquet. Il sait aussi provoquer une perte réelle sous le transport (`--loss`, en pour mille), ce qui exerce ses vrais mécanismes de détection.

Trois chiffres sont relevés par le programme lui-même plutôt que par l'opérateur : le temps processeur, sur la fenêtre exacte de chaque salve ; la part des paquets manquants due à une file d'émission pleine plutôt qu'au réseau ; et la taille de paquet que le chemin permet. Un relevé pris à la main dans un gestionnaire de tâches échantillonne, arrondit, et ne couvre pas la bonne fenêtre.

Deux réserves de lecture. Le temps processeur porte sur tout le programme, fils compris : le banc enchaîne donc ses deux salves au lieu de les mener de front. Et chaque extrémité ne constate les pertes que sur ce qu'elle a émis, le transport ne les détectant que par les acquittements qui lui reviennent : le trajet retour se lit dans la fenêtre de l'autre banc.

Marche à suivre sur deux PC : [docs/testing/M2-PROTOCOLE.md](../docs/testing/M2-PROTOCOLE.md). Toujours en version release : en mode debug, le tunnel mesure environ quatre fois son coût réel.

Ce que le banc ne sait pas encore faire : simuler un aller-retour. Or c'est le produit perte x aller-retour qui fait s'effondrer un contrôleur de congestion ordinaire. La condition exacte de G-loss (25 ms d'aller-retour, 10 minutes) se mesurera donc sur un vrai chemin distant au jalon M5. En attendant, la partie « le débit ne s'effondre pas sous la perte » est vérifiée, et la propriété du contrôleur est gardée par un test qui le compare au contrôleur ordinaire du transport.

## Sources de mesure

- Lecteur, cinq fois par seconde (la fiche « Statistiques » et `zyr-cli connect` en affichent la plupart) : images par seconde (`fps`) ; temps chez l'hôte, de la capture à l'envoi (`host_ms`) ; aller-retour réseau et sa variance (`network_ms`, `network_variance_ms`) ; temps de décodage (`decode_ms`) et d'affichage (`render_ms`) ; débit (`bitrate_mbps`) ; images perdues en route et images remplacées par une plus récente avant l'affichage (`dropped_network_pct`, `dropped_jitter_pct`) ; temps depuis la dernière image (`since_frame_ms`) ; latence de bout en bout, de la capture sur l'hôte à l'affichage ici (`latency_ms`) ; p99 de l'intervalle entre images affichées (`frame_interval_p99_ms`). Ce dernier chiffre, qui est G-frame, est calculé mais pas encore affiché.
- Tunnel : paquets et octets par canal, datagrammes jetés sur file pleine, aller-retour QUIC, chemin actif (direct ou relais), migrations de chemin.
- Journal du moteur hôte (`engine.log`, lignes marquées `engine`) : encodeurs essayés et celui qui sert, écran filmé, chaque flux ouvert (taille, cadence, codec, encodeur, débit), et le bilan des images capturées, envoyées, répétées, jetées, avec le temps moyen et maximal de conversion et d'encodage. Côté client, le journal de la fenêtre dit à chaque ouverture combien de temps l'image a mis (G-start).
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
| Jalon M1 | Couple de moteurs officiels non pilotés, mêmes machines | Écart <= 5 % : notre pilotage ne doit rien coûter. Sans objet depuis que ces moteurs ont quitté le produit ([D222](../docs/DECISIONS.md)) |
| Jalon M2 | Mesures de M1 | G-lat, G-loss, G-cpu tenus, sinon la décision du tunnel systématique est révisée |
| Chaque version | Version précédente | Aucune régression au-delà des marges, sinon la publication est bloquée |
| Jalon MZ | Anciens moteurs, mêmes machines | Latence de bout en bout inférieure ou égale, G-frame et G-start tenus, qualité au moins égale à débit égal ([ROADMAP.md](../docs/ROADMAP.md)) |
| Mise à jour de FFmpeg | Version précédente | Aucune régression, sinon la mise à jour est investiguée avant fusion |

## Bases de comparaison

Les relevés de référence sont versionnés dans ce dossier au fur et à mesure, avec la description du matériel et des conditions réseau de chaque campagne.
