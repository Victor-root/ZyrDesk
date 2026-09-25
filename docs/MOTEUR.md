# Le moteur ZyrDesk

Ce document décrit le moteur que ZyrDesk écrit lui-même, en Rust, à la place de Sunshine et de Moonlight ([D219](DECISIONS.md), [D222](DECISIONS.md)). Il dit ce que fait chaque morceau, dans quel ordre, et pourquoi chaque choix a été fait. La priorité absolue ne change pas : la latence d'abord, puis la fluidité, les performances et la qualité d'image. Aucune commodité d'écriture ne passe devant.

## 1. En une image

```text
PC CLIENT                                              PC HÔTE
ZyrDesk.exe (la fenêtre)                               zyrdeskd (le service)
  une fenêtre d'image dans la fenêtre du produit          reçoit la session
  le lecteur (zyr-player) : décode, affiche, joue         lance le moteur hôte pour elle
        |  tube local protégé                                  |  tube local protégé
        v                                                      v
zyrdeskd (le service)  ==== le tunnel QUIC, direct ou relais ====  zyrdeskd --serve-a-session
                                                       (zyr-host, dans la session de l'écran,
                                                        compte système) : capture, conversion,
                                                        encodage, découpe, son, clavier, souris
```

Le tunnel, les identités, le relais, le serveur, l'écran virtuel, le presse-papiers et les fichiers ne changent pas : ils étaient déjà à nous. Ce qui change, c'est ce qui passait par les deux moteurs.

## 2. Les choix, et pourquoi

**Direct3D 11, pas Vulkan.** Victor penchait pour Vulkan ; le choix se fait sur ce que Windows fournit, pas sur une préférence. La capture de l'écran par Windows (Desktop Duplication, la seule qui voit les invites d'administration et l'écran de connexion) ne parle que Direct3D 11. Le décodage matériel le plus éprouvé sur les trois marques de cartes passe par Direct3D 11, et c'est lui que Moonlight utilise. L'affichage le plus direct sur Windows passe par DXGI, la couche d'affichage de Direct3D. Vulkan obligerait à traduire l'image à chaque bout, capture et affichage, et chaque traduction coûte du temps ou une copie. Direct3D 11 est donc le chemin le plus court, et il n'ajoute rien à installer : Windows l'a déjà.

**FFmpeg 9.0.2, compilé par nous, réduit au moteur.** FFmpeg est la bibliothèque qui parle aux encodeurs matériels des trois fabricants (NVENC chez Nvidia, AMF chez AMD, Quick Sync chez Intel), avec x264 en secours logiciel, et aux décodeurs matériels. C'est ce que Sunshine et Moonlight utilisaient déjà. Nous en faisons notre propre version, la dernière stable, avec seulement ce qui sert : trois fichiers, rangés dans `vendor/ffmpeg`, chargés au démarrage du moteur. Rien à installer sur le PC de Victor, et le script qui les refait est dans `packaging/ffmpeg`.

**Des tubes locaux, plus aucun port.** Le moteur ne parle à personne sur le réseau. Chaque moitié parle au service de sa machine par un tube nommé dont Windows lui-même garde la porte : sur l'hôte, seul le compte système peut l'ouvrir ; sur le client, seul le compte système et la personne qui a demandé la session. Les sept ports des anciens moteurs, les adresses `127.77.x.y` et les règles de pare-feu qui leur étaient dédiées disparaissent.

**Plus d'appairage.** Les deux ordinateurs se sont déjà reconnus dans le tunnel, par leurs empreintes. Le code à quatre chiffres que les moteurs échangeaient en plus ne prouvait rien de plus et causait des pannes ; il n'existe plus.

**L'image dans la fenêtre du produit.** L'image se dessine dans une fenêtre qui appartient à ZyrDesk, dans le même programme que le bouton flottant. Plus de seconde fenêtre à accrocher à la nôtre, plus de clavier à se disputer entre deux programmes, plus de raccourcis simulés pour parler au lecteur : le menu appelle directement le lecteur.

## 3. Le trajet d'une image

1. **Capture.** Le moteur hôte demande à Windows la dernière image de l'écran choisi, directement dans la mémoire de la carte graphique. Il la reçoit dès que Windows l'a composée.
2. **Cadence.** Une image capturée part tout de suite, sauf si elle arrive plus vite que la cadence de la session (alors seule la plus récente part). Quand l'écran ne bouge plus et que « Fluide » est choisi, l'image précédente est renvoyée à la cadence exacte de la session, pour que le curseur et la qualité s'affinent ; une vraie image ne cède jamais sa place à une répétition.
3. **Conversion.** La carte graphique convertit l'image du format de l'écran vers celui des encodeurs (NV12, couleurs BT.709), la met à la taille de la session et y dessine le curseur quand c'est l'hôte qui doit le montrer. Une seule passe, sans que l'image quitte la carte.
4. **Encodage.** L'encodeur matériel de la carte (ou x264 en secours) encode l'image sans image différée, sans anticipation et avec un débit qu'une image ne dépasse pas. Une image clé n'est produite qu'au début et quand le client la demande.
5. **Découpe et correction d'erreurs.** L'image encodée est découpée en paquets qui tiennent dans le tunnel, et 20 % de paquets de réparation sont ajoutés (Reed-Solomon) : n'importe quels paquets en nombre suffisant reconstruisent l'image, sans rien redemander. Une petite image part en paquets d'un peu plus de la moitié de la taille permise : le transport ne peut alors jamais en ranger deux dans le même envoi, et une perte n'emporte qu'un morceau, jamais un morceau et sa réparation ensemble ([D223](DECISIONS.md)).
6. **Tunnel.** Les paquets partent en datagrammes QUIC, chiffrés une seule fois, direct ou par le relais.
7. **Réassemblage.** Le client reconstruit chaque image dès que tous ses morceaux sont là, ou dès qu'assez de morceaux permettent de réparer les manquants.
8. **Décodage.** Le décodeur matériel de la carte du client décode dans la mémoire de la carte.
9. **Affichage.** L'image est dessinée dans la fenêtre du produit, aux proportions exactes, et présentée à l'écran à la prochaine occasion. Une image plus récente remplace toujours une image qui attend encore.

Chaque paquet porte l'heure de capture : le client connaît donc la latence de bout en bout de chaque image, pas seulement le temps du réseau.

## 4. Quand un paquet se perd

La réparation vient d'abord des paquets de réparation, sans attendre. Si une image ne peut vraiment pas être reconstruite, le client ne montre pas d'image abîmée : il garde la dernière image juste, demande une image clé une seule fois (redemandée au bout d'un quart de seconde si rien n'arrive, jamais en rafale), et reprend dès qu'elle arrive. C'est la leçon de [D136](DECISIONS.md) : une demande par image reçue avait saturé le lien au pire moment.

## 5. Le son

L'hôte enregistre ce que la carte son joue, avant son propre volume et sa propre coupure : couper les enceintes de l'ordinateur d'en face garde donc le son de la session ([D61](DECISIONS.md)). Il l'encode en Opus par tranches de 10 ms. Le client le joue avec une petite réserve de deux tranches, et une tranche perdue est remplacée plutôt qu'attendue. Le son a sa propre file et son propre fil : il ne retarde jamais l'image. Sans carte son chez le client, rien n'est ouvert, et rien n'est attendu ([D211](DECISIONS.md) à [D214](DECISIONS.md)).

## 6. Clavier et souris

Le client envoie les touches par leur place sur le clavier (code de balayage), jamais par leur lettre : un A d'un clavier AZERTY reste la touche du A. Tout passe par un flux fiable et ordonné, pour qu'une touche relâchée ne se perde jamais. Le mode « Immersif » prend Alt+Tab, Windows, Impr. écran et Alt+F4 pour la session, sans jamais avaler Alt, Ctrl ou Maj, dont les raccourcis de ZyrDesk ont besoin ([CLAVIER.md](CLAVIER.md)). À la perte du clavier, à la fin de la session ou à la perte du lien, tout ce qui est enfoncé est relâché, des deux côtés : l'hôte relâche lui-même ce qu'il a appuyé si le lien se tait ([D198](DECISIONS.md)).

La souris « Bureau » envoie une position absolue et le client montre son propre curseur, avec la forme de celui d'en face : pas d'aller-retour derrière la main. La souris « Jeu » envoie des déplacements relatifs lus directement sur la souris, et c'est l'hôte qui dessine son curseur dans l'image.

## 7. Ce qui passe dans le tunnel

| Canal | Contenu | Fiabilité |
|---|---|---|
| Flux « moteur » | Messages du lecteur et du moteur hôte : demande de session, changements en direct, demande d'image clé, touches et souris, mesure de l'aller-retour | Fiable et ordonné |
| Datagrammes « vidéo » | Paquets d'images et paquets de réparation | Non fiable, réparé par la correction d'erreurs |
| Datagrammes « son » | Tranches Opus numérotées | Non fiable, tranche perdue remplacée |
| Canal ZyrDesk | Les questions du produit, inchangées : écran de l'hôte, débit, codecs, presse-papiers, fichiers, journal, Ctrl+Alt+Suppr, verrouillage | Fiable |

## 8. Changer en direct

Tout se change pendant la session, sans bouton « Appliquer » ([D117](DECISIONS.md)) : le débit est appliqué à l'encodeur en place quand il le permet ; la taille, la cadence et le codec refont l'encodeur et le décodeur dans la même fenêtre ; l'écran filmé change sans rien relancer. Le moteur ne redémarre jamais pour un réglage.

## 9. La mesure

Le lecteur tient ses mesures cinq fois par seconde, indépendamment du décodage, pour qu'elles continuent de parler quand l'image se fige : images par seconde, temps de décodage et d'affichage, temps de l'hôte, aller-retour du réseau, débit, images perdues ou remplacées, temps depuis la dernière image, et deux mesures nouvelles : la latence de bout en bout, de la capture à l'affichage, et l'intervalle entre deux images affichées (le seuil G-frame de [perf/GATES.md](../perf/GATES.md)). La fiche « Statistiques », le menu et les voyants les lisent directement, sans fichier. L'intervalle entre images n'y est pas encore affiché.

## 10. Sécurité

Tout ce qui sort de la machine est dans le tunnel, chiffré une fois, entre deux empreintes épinglées. Le moteur hôte tourne avec le compte système dans la session de l'écran, parce que c'est la seule façon de voir et de piloter l'écran de connexion et les invites d'administration ; c'est pourquoi son tube n'accepte que le compte système. Le moteur n'écrit jamais une touche dans le journal.

## 11. Ce qui a disparu

Sunshine, Moonlight, leurs vingt-quatre patchs, leurs sous-modules, leurs compilations, les caisses `zyr-engine-host` et `zyr-engine-client`, l'appairage par code, les fichiers échangés avec le lecteur (`session-wanted.txt`, `session-pointer.txt`, `session-stats.txt`), la lecture des journaux des moteurs, les raccourcis simulés, les redémarrages de moteur pour changer un réglage, et presque tout ce qui accrochait la fenêtre d'un autre programme à la nôtre.

## 12. Les caisses

| Caisse | Rôle |
|---|---|
| `zyr-media` | Les formats du moteur, sans rien de Windows : paquets, correction d'erreurs, touches et souris, messages, cadence, mesures |
| `zyr-codec` | FFmpeg chargé au démarrage : encodeurs, décodeurs, Opus |
| `zyr-control` | Le tube de la fenêtre au service, et désormais les tubes du moteur |
| `zyr-tunnel` | Le passage entre les tubes du moteur et le tunnel |
| `zyr-host` | Le moteur hôte : capture, conversion, encodage, son, clavier et souris injectés |
| `zyr-player` | Le lecteur : réassemblage, décodage, affichage, son |
