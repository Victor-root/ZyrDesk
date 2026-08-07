# Coût du tunnel en boucle locale

Relevés obtenus au jalon M2 avec `zyr-cli banc`, sur la machine de développement, les deux extrémités dans le même ordinateur.

Ces chiffres ne valident aucun seuil. Ils servent de point de comparaison pour les mesures sur deux vraies machines, et de garde contre une régression de la même ampleur que celles trouvées ici. Une boucle locale n'a ni carte réseau, ni Wi-Fi, ni pare-feu, ni aller-retour réaliste.

## Conditions

| Élément | Valeur |
|---|---|
| Machine | Linux, les deux extrémités dans le même ordinateur |
| Compilation | release |
| Chemin | boucle locale, aller-retour d'environ 0,15 ms |
| Trajet mesuré | tunnel complet, canal vidéo, ports des moteurs compris |
| Taille de paquet | 1353 octets, décidée par la découverte de chemin |
| Cadence | rafales de paquets, une par image, 60 images par seconde |

## Relevés

Une campagne, quatre débits, 5 secondes par salve dans chaque sens.

| Débit visé | Débit tenu | Perte, sans tunnel | Perte, avec tunnel | Médiane, sans tunnel | Médiane, avec tunnel | Écart médian | Écart au centile 99 |
|---|---|---|---|---|---|---|---|
| 5 Mb/s | 4,6 Mb/s | 0,00 % | 0,00 % | 0,16 ms | 0,64 ms | +0,48 ms | +0,71 ms |
| 20 Mb/s | 19,5 Mb/s | 0,00 % | 0,00 % | 0,16 ms | 0,92 ms | +0,76 ms | +0,95 ms |
| 50 Mb/s | 49,5 Mb/s | 0,00 % | 0,00 % | 0,19 ms | 1,38 ms | +1,19 ms | +1,92 ms |
| 100 Mb/s | 99,7 Mb/s | 5,60 % | 8,50 % | 0,28 ms | 2,03 ms | +1,76 ms | +2,52 ms |

La ligne à 100 Mb/s mesure la limite de la machine de test, pas celle du produit : le trajet sans tunnel y perd déjà 5,6 % des paquets, ce qui n'arrive pas sur un vrai lien à ce débit. Elle n'est gardée que pour montrer où la boucle locale rend les armes.

## Sous perte provoquée

Perte injectée sous le transport, à 40 Mb/s pendant 6 secondes.

| Perte provoquée | Perte constatée bout en bout | Débit tenu | Aller-retour médian avec tunnel |
|---|---|---|---|
| 0 % | 0,00 % | 39,7 Mb/s | 1,34 ms |
| 1 % | 0,98 % | 39,7 Mb/s | 1,44 ms |
| 2 % | 1,95 % | 39,7 Mb/s | 1,46 ms |

Aucune amplification de perte, aucun effondrement de débit. C'est la propriété dont dépend la décision de tout faire passer par le tunnel.

## Ce que ces relevés ont permis de trouver

- En compilation debug, le tunnel coûtait 5,44 ms à 50 Mb/s, contre 1,2 ms sur la même machine en release. Toute mesure doit être faite en release, sans exception.
- La taille de paquet était figée avant la fin de la découverte de chemin : 1101 octets au lieu de 1353, et le moteur l'aurait gardée pour toute la session. Corrigé.
- Le banc ne servait qu'une mesure à la fois et laissait expirer la connexion suivante. Corrigé.
