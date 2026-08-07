# Première lecture, réseau local, HEVC

Relevés obtenus au jalon M1, moteurs pilotés par ZyrDesk, sans tunnel.

## Conditions

| Élément | Valeur |
|---|---|
| Hôte | Windows 11, NVIDIA GeForce RTX 3090, plusieurs écrans |
| Client | Windows 11, Intel UHD Graphics, liaison sans fil |
| Réseau | Réseau local, client en Wi-Fi |
| Session | 1920x1080, 60 images par seconde demandées, 20 Mb/s, HEVC, décodage matériel imposé, régulation du rythme d'affichage active |
| Contenu | Vidéo en ligne 60 images par seconde, entrecoupée de navigation sur le bureau |

## Relevés

| Mesure | Session A (2 min 13) | Session B (6 min 18) |
|---|---|---|
| Images par seconde reçues | 34,49 | 52,36 |
| Images par seconde décodées | 34,49 | 52,36 |
| Images par seconde affichées | 34,49 | 52,34 |
| Latence hôte min/max/moyenne | 1,9 / 14,1 / 2,3 ms | 2,0 / 111,3 / 2,5 ms |
| Images perdues par le réseau | 0,00 % | 0,00 % |
| Images perdues par gigue | 0,00 % | 0,03 % |
| Latence réseau moyenne | 1 ms (variance 0) | 2 ms (variance 6) |
| Temps de décodage moyen | 0,81 ms | 0,74 ms |
| Délai de file moyen | 1,53 ms | 3,66 ms |
| Temps de rendu moyen | 0,54 ms | 0,55 ms |

## Lecture

La chaîne est saine. Aucune perte réseau, une latence réseau de 1 à 2 ms, un décodage sous la milliseconde et un rendu sous 0,6 ms : le budget de latence est dominé par le réseau et l'affichage, pas par notre pilotage. Les pics de latence hôte (14 ms, puis 111 ms) correspondent aux moments où le contenu change brutalement, la sortie de la vidéo vers le bureau.

Le débit d'images observé ne mesure pas la qualité perçue. Le moteur n'encode que lorsque l'écran change, et sa valeur par défaut ne garantit que la moitié de la cadence demandée quand rien ne bouge. Les moyennes de 34 et 52 images par seconde reflètent donc surtout la proportion de contenu animé pendant chaque session. C'est aussi ce qui rendait le déplacement de la souris et les animations de fenêtres saccadés sur un bureau immobile.

## Suite

Ces relevés ne peuvent pas servir de base de comparaison en l'état : le contenu était mixte, et ils précèdent le passage de la cadence minimale garantie à 60 images par seconde.

La base de référence du jalon M1, celle à laquelle le tunnel du jalon M2 devra se comparer, demande une campagne à contenu constant et reproductible, sur cinq minutes, avec la cadence minimale corrigée, en H.264 puis en HEVC, et un relevé équivalent obtenu en lançant les moteurs sans ZyrDesk.
