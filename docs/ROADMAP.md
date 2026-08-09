# Roadmap

Le projet avance par jalons courts, chacun testable de bout en bout par un non-développeur sur deux PC Windows. Un jalon n'est terminé que quand ses critères de sortie, mesurés, sont atteints. « Une session qui semble fonctionner » n'est jamais un critère.

## Seuils de performance transverses (définis une fois, réutilisés partout)

| Code | Seuil |
|---|---|
| G-lat | Latence ajoutée par le tunnel, mesurée contre le même flux sans tunnel : médiane <= 1 ms, p99 <= 3 ms |
| G-loss | À 40 Mb/s, 25 ms d'aller-retour simulé et 1 % de perte pendant 10 min : débit utile >= 95 % du nominal, aucun gel visible > 250 ms |
| G-cpu | Processus tunnel <= 8 % d'un cœur à 40 Mb/s |
| G-start | Clic « Se connecter » -> première image : <= 4 s en LAN, <= 8 s via Internet |
| G-frame | p99 de l'intervalle entre images affichées <= 20 ms sur 5 min (via l'overlay de statistiques) |

## M0 : Fondations

- Objectif : dépôt opérationnel, base saine.
- Contenu : workspace Rust, CI, création des deux forks moteurs et branchement en submodules (tags épinglés), squelette d'installateur NSIS (installation/désinstallation propres), `zyr-cli doctor` v0, documents COMPLIANCE et manifeste de patchs initialisés.
- Binaires non signés (avertissement SmartScreen au premier lancement : documenté, normal pour un jeune projet open source ; signature gratuite SignPath Foundation envisagée plus tard, sans paiement ni entité).
- Résultat observable : on installe et désinstalle ZyrDesk (coquille) proprement sur les deux PC ; doctor est vert.
- Critères de sortie : installation + désinstallation sans résidu ; CI verte (lint, tests unitaires, build).

## M1 : Prototype LAN sans tunnel (la vérité terrain)

- Objectif : une session complète pilotée par ZyrDesk, moteurs officiels, zéro interface upstream visible ; et retirer par écrit toutes les hypothèses.
- Contenu : superviseur hôte en mode console (config Sunshine générée, binaire préconstruit renommé accepté à ce stade), appairage automatique par l'API PIN, `zyr-cli connect <ip>` qui lance le lecteur avec les bons drapeaux.
- Liste de vérifications à retirer (chacune consignée avec sa preuve) : Sunshine s'annonce-t-il en mDNS malgré la liaison loopback ; peut-on supprimer la fenêtre de chargement du lecteur sans le modifier ; le chiffrement interne est-il réellement inactif en mode 0 sur loopback ; tailles exactes des en-têtes GameStream par paquet (capture réseau) ; comportement de la capture écran éteint. Le pilote d'écran virtuel est éprouvé au jalon M9 : rien dans l'architecture n'en dépend, et le repli en cas d'échec est la situation de tous les tests d'ici là.
- Résultat observable : une commande sur le PC client -> bureau distant fluide.
- Critères de sortie : 1080p60 réel (H.264 et HEVC matériels), audio, clavier/souris ; statistiques de l'overlay à ±5 % du couple Sunshine+Moonlight vanilla sur les mêmes machines ; liste de vérifications entièrement documentée.

## M2 : Banc de mesure + tunnel (décision transport GRAVÉE)

- Objectif : le tunnel prouve qu'il ne coûte rien, ou la décision est révisée.
- Contenu : banc de mesure (`zyr-cli bench`, comparaison directe contre tunnel sur le même trajet, perte provoquée sous le transport, compteurs du tunnel) ; tunnel complet sur quinn (identité d'appareil épinglée, multiplexage des sept ports des moteurs, budget de taille de paquet) ; contrôleur de congestion média.
- Fait : le tunnel transporte de bout en bout, le contrôleur média est écrit et gardé par un test contre le contrôleur ordinaire, la décision transport est écrite (D13). **G-lat, G-loss et G-cpu tenus sur deux PC en Ethernet gigabit** : [perf/baselines/M2-lan-ethernet.md](../perf/baselines/M2-lan-ethernet.md).
- Reste à faire : simulation d'aller-retour dans le banc (reportée à M5, voir NETWORK.md section 3) ; passage des moteurs en loopback strict, qui vient avec le service du jalon M3 puisque c'est lui qui portera les extrémités de tunnel ; procédure photon-à-photon ; comparaison A/B contre M1.
- Critères de sortie : G-lat (fait), G-loss sur le débit (fait ; la condition à 25 ms d'aller-retour est reportée à M5), G-cpu (fait), mémo de décision transport (fait). **Jalon atteint pour ce qui dépendait du tunnel** ; le reste ci-dessus est rattaché aux jalons qui le portent.

## M3 : Service Windows et accès non supervisé

- Objectif : l'hôte fonctionne avant toute ouverture de session et survit aux transitions.
- Contenu : `zyrdeskd` en service LocalSystem, lancement du moteur dans la session console dès le démarrage, relance au changement de session (verrouillage, changement d'utilisateur), arrêt propre.
- Fait : le service s'installe, démarre avec Windows, tient son moteur et le relance selon une politique qui distingue un arrêt système d'un incident ; le moteur est lancé dans la session attachée à l'écran, avec le jeton du service déplacé vers cette session, et il est relancé quand cette session change.
- Fait : le service porte l'extrémité hôte du tunnel, sur un seul port UDP. Le moteur est refermé sur la machine locale, injoignable depuis le réseau, et `zyr-cli connect` monte l'extrémité client. Les ordinateurs admis sont une liste d'empreintes relue à chaud (`zyr-cli host authorize`), et le client apprend les ports du moteur distant par le canal ZyrDesk plutôt que de les deviner.
- Fait : l'installateur livre `zyrdeskd.exe`, enregistre le service, pose la règle de pare-feu du tunnel, et retire les trois à la désinstallation sans laisser ni service orphelin ni fichier verrouillé.
- Fait : vérifié sur les deux machines le 2026-08-08, protocole [docs/testing/M3-PROTOCOLE.md](testing/M3-PROTOCOLE.md) déroulé en entier. Hôte redémarré et laissé sur l'écran de connexion, session ouverte à distance depuis le client, invite UAC visible et cliquable, verrouillage et déverrouillage sans coupure perceptible, moteur tué de force relancé seul par le service, désinstallation sans résidu. Deux défauts trouvés en cours de route et corrigés : un paquet vers un port pas encore ouvert terminait toute la session sous Windows, et le service refusait une installation ou un arrêt déjà satisfaits.
- Limite connue : après un incident du moteur hôte, le client ne se reconnecte pas seul, il faut relancer la connexion. Le comportement vient des moteurs, qui ne reprennent jamais une session dont le serveur a disparu, et il est identique hors ZyrDesk. La machine à états de reprise du jalon M7 le couvre.
- Reste à faire : l'interface parlera au service par tube nommé au jalon M4, et l'échange d'empreintes deviendra automatique avec le broker au jalon M5.
- Résultat observable : PC hôte redémarré, personne de connecté : on se connecte depuis l'autre PC, on voit l'écran de connexion Windows, on tape le mot de passe, on ouvre la session.
- Critères de sortie : connexion depuis l'écran de connexion (tenu) ; invite UAC visible et cliquable à distance (tenu) ; verrouillage/déverrouillage en cours de session = coupure <= 5 s auto-récupérée (tenu). **Jalon atteint.**

## M4 : Interface v1 + moteurs auto-compilés

- Objectif : le produit ressemble à ZyrDesk et se pilote à la souris.
- Contenu : app Tauri (accueil, interrupteur hôte, liste LAN sans compte via mDNS, connexion, réglages minimaux), design system v1 (tokens + composants de base), builds reproductibles des moteurs avec rebranding (patchs P-M2), lecteur lancé détaché + rattachement.
- Fait : le service tient les deux extrémités de tunnel, y compris sortantes, et se pilote par tube nommé. `zyr-cli connect` ne tient plus rien : il demande une voie, lance le lecteur sur les adresses locales rendues, et dit au service quel processus cette voie sert. Fermer la fenêtre de commande ne coupe plus la session ; une voie que plus personne n'utilise se referme seule. Conséquence attendue de D2 : le service est désormais requis sur le PC client aussi.
- Fait : le moteur client est compilé par nos soins et porte les patchs P-M2 (marque), P-M1 (aucune fenêtre à lui avant l'image) et P-M5 (codes de sortie distincts, lus par notre superviseur).
- Fait : le moteur hôte est compilé par nos soins et porte notre nom, notre icône et notre éditeur. Le patch P-S2 se réduit à exposer le seul champ que le moteur ne laissait pas encore choisir ; le reste est passé à la configuration.
- Fait : première tranche de l'application. Le design system v1 (couleurs, espacements, typographie, mouvement, composants de base) est posé en tokens, et l'accueil montre cet ordinateur puis ouvre une session vers un autre, sans ligne de commande. L'ouverture d'une session vit dans `zyr-session`, partagée avec la ligne de commande plutôt que réécrite.
- Fait : les ZyrDesk du réseau local se trouvent seuls. Le service s'annonce en mDNS et collecte les autres, l'interface les montre en cartes, et un clic sur une carte ouvre la session. Ni adresse ni empreinte à recopier tant que les deux machines sont sur le même réseau. Le thème suit le système par défaut, et se force au clair ou au sombre.
- Fait : l'interrupteur d'accès distant est réel. Le choix est écrit dans `data/preferences.conf` et honoré au démarrage suivant ; le service reste debout quand il est sur non, seul le fait d'être joignable s'arrête. La position de l'interrupteur et l'état du moteur sont deux choses distinctes, ce qui laisse voir « démarrage en cours » sans faire croire que l'interrupteur a menti. Restriction aux administrateurs : prévue en M5 par la décision ouverte O2, le tube de commande étant aujourd'hui ouvert à toute personne connectée à la machine.
- Fait : l'interface se rattache à une session en cours. Le service dit quelles sessions il tient, vers quelle machine et depuis combien de temps ; une fenêtre ouverte après coup, ou relancée après un plantage, montre la session au lieu d'un accueil vide, et refuse d'en ouvrir une deuxième par-dessus. Une voie ouverte mais que personne n'utilise encore n'est pas annoncée comme une session : c'est une tentative en cours, surveillée par qui l'a lancée.
- Fait : l'écran de réglages. Simple par défaut (qualité d'image en trois préréglages, thème), « Avancé » replié pour le jargon (codec, fenêtre de la session, souris, statistiques, accès au dossier des journaux). Les choix vivent dans le service, à côté de l'accès distant : ils survivent à la fenêtre et se corrigent à la main dans `data/preferences.conf`. La qualité est un barreau et non trois molettes, taille d'image et débit montant ensemble ; ce que le préréglage donne est écrit sous lui, en clair. Laissés de côté sciemment : l'audio, qu'aucun réglage du produit ne pilote encore, et le démarrage avec Windows, qui n'a de sens qu'avec l'icône de la zone de notification, prévue plus tard. La ligne de commande garde ses propres options : c'est l'outil de diagnostic, et un banc d'essai qui dépendrait d'un fichier de réglages ne serait plus comparable d'une machine à l'autre.
- Fait : le bouton flottant d'une session. Le logo ZyrDesk reste posé en haut à droite de l'image pendant toute la session ; un clic déplie un menu qui bascule le plein écran, les statistiques et le mode de la souris, masque le bouton, ou termine la session. C'est une fenêtre à nous, toujours au-dessus et qui ne prend jamais le premier plan, et ce qu'elle demande passe par les raccourcis que le moteur client expose déjà : aucun patch de plus, et rien de ZyrDesk dans le moteur. Conséquence assumée : une session s'ouvre désormais en fenêtre sans bordure, l'exclusif restant choisissable sans le bouton. Décision [D16](DECISIONS.md).
- Fait : partir d'une session et la fermer sont deux gestes distincts. Quitter laisse l'ordinateur distant garder son bureau ouvert, prêt pour un retour immédiat ; fermer le lui rend. Le moteur savait déjà le demander mais ouvrait une fenêtre pour le faire, d'où le patch P-M7, de la même forme que les deux autres. Le menu flottant porte les deux, et le service dit désormais à quelle adresse de tunnel joindre l'ordinateur d'en face, seule adresse à laquelle le moteur peut le toucher.
- Résultat observable : deux PC, tout en interface, zéro ligne de commande.
- Critères de sortie : parcours complet à la souris ; tuer l'interface en pleine session -> le flux survit et l'interface se rattache au relancement ; aucune trace visible de Sunshine/Moonlight (fenêtres, titres, icônes, processus au nom trompeur) ; G-start LAN tenu. **Contenu du jalon terminé** ; le jalon sera atteint quand [docs/testing/M4-PROTOCOLE.md](testing/M4-PROTOCOLE.md) aura été déroulé en entier sur les deux machines.

## M5 : Broker, comptes, Internet en direct

- Objectif : « Mes ordinateurs » à travers Internet.
- Contenu : broker v1 (comptes Argon2id, enrôlement d'appareils Ed25519, présence WSS, tickets de session, synchronisation de liste), intégration service (identité, lien broker), candidats directs (mappage de ports, adresses observées), perforation NAT, PIN par tunnel, adresses loopback 127.77.x.y par appareil.
- Résultat observable : client sur partage de connexion 4G, hôte sur le réseau domestique : connexion en un clic.
- Critères de sortie : G-start WAN tenu ; 30 min de session stable 1080p60 ; compteurs broker : zéro octet de média ; révocation d'appareil effective en moins d'une minute.

## M6 : Relais et bascule automatique

- Objectif : ça marche depuis les réseaux hostiles, et ça s'améliore tout seul.
- Contenu : relais déployé (auto-hébergeable), jetons de relais, démarrage « relais d'abord, direct en parallèle », migration vers le direct sans coupure (iroh) ou reconnexion ~2 s (plan B), indicateur de chemin dans l'interface, plafond de débit en mode relais.
- Résultat observable : UDP direct bloqué au pare-feu entre les deux PC -> la session s'établit quand même ; on débloque -> passage en direct sans interruption perceptible.
- Critères de sortie : session via relais avec surcoût de latence <= 1 aller-retour supplémentaire vers le relais ; promotion automatique vérifiée ; l'utilisateur voit toujours le chemin actif.

## M7 : Résilience

- Objectif : le produit encaisse la vraie vie.
- Contenu : machine à états de reprise (fenêtre 60 s), gestion veille/réveil et écran éteint, matrice de crash (kill -9 de chaque composant), updater v1 (refus pendant session, arrêt service, remplacement, redémarrage), bundles de diagnostic.
- Résultat observable : on débranche le câble réseau 10 s en pleine session -> reprise automatique <= 5 s après retour ; on tue chaque processus un par un -> tout se répare seul.
- Critères de sortie : matrice de kill 100 % auto-réparée ; updater refuse pendant une session et réussit après ; bundle de diagnostic produit en un clic.

## M8 : Confort

- Objectif : les fonctions qui font « produit fini ».
- Contenu : presse-papiers texte bidirectionnel (canal tunnel + agent hôte), sélection d'écran (multi-écran hôte), préréglages de qualité avec bascule rapide, sonde de débit pré-session, politique spectateur unique/reprise, consentement et indicateur côté hôte.
- Critères de sortie : presse-papiers <= 500 ms ; changement de qualité ou d'écran <= 3 s en conservant fenêtre et tunnel ; deuxième client entrant traité selon la politique (occupé ou reprise).

## M9 : Écran virtuel

- Objectif : un PC hôte sans écran branché, utilisable.
- Contenu : intégration du pilote tiers signé Virtual-Display-Driver (MIT) en installation OPTIONNELLE et consentie (téléchargement vérifié par empreinte au moment où l'utilisateur active la fonction ; validé dès M1 sur Windows 11 à jour), orchestration des options `dd_*` de Sunshine : écran virtuel à la résolution/fréquence du client, `ensure_only_display` pendant la session, restauration de la topologie à la déconnexion. Base HDR posée.
- Critères de sortie : hôte sans aucun écran -> session 1440p60 ; résolution qui suit celle du client ; topologie d'écrans restaurée après déconnexion ET après crash ; si le pilote tiers est refusé par Windows, la fonction se désactive proprement avec explication (repli : écran branché ; l'utilisateur final qui veut un PC sans écran peut utiliser un adaptateur du commerce, facultatif et à sa charge).

## M10 : Durcissement et bêta

- Objectif : ouvrable au public.
- Contenu : régressions de performance nocturnes en CI sur un PC Windows GPU dédié (matrice : 1080p60/1440p60, H.264/HEVC/AV1, direct/relais, reconnexion, écran virtuel), passe de sécurité (ACL, quotas, TOTP obligatoire), page « code source correspondant » et COMPLIANCE finalisés, documentation d'auto-hébergement broker+relais, installateur et docs de bêta.
- Critères de sortie : seuils G-* verts 2 semaines consécutives sur NVIDIA + (AMD ou Intel) ; installation à froid réussie par un tiers avec la seule documentation ; aucun point critique ouvert de la liste sécurité.

## Politique d'interopérabilité (dès M5)

- Le canal de contrôle du tunnel échange les versions au premier contact ; le broker refuse proprement les paires incompatibles avec un message de mise à jour.
- Fenêtre de compatibilité N-1 : chaque release est testée « nouveau client -> ancien hôte » et « ancien client -> nouvel hôte ».
- Les moteurs suivent leur propre compatibilité GameStream (éprouvée upstream) ; nos mises à niveau moteurs sont couvertes par la suite contrat moteur + le banc de performance.

## Évolutions post-v1 (ordre indicatif)

Transfert de fichiers, presse-papiers images/fichiers, manettes (via installation optionnelle du pilote historique, ou mieux si l'écosystème évolue), coupure de l'audio côté hôte (périphérique virtuel déjà présent chez l'utilisateur, type enceintes de streaming Steam), HDR complet, 120 FPS, multi-écran simultané, Wake-on-LAN, partage entre comptes (invités), lecteur natif maison (v2), clients autres plateformes.
