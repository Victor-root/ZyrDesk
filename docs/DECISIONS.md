# Registre des décisions

Ce registre trace les décisions structurantes : ce qui est acté, pourquoi, et ce qui reste ouvert. Toute remise en cause d'une décision actée passe par une mise à jour de ce fichier avec la raison du changement.

## Contraintes posées par Victor (non négociables)

- C1. Performance d'abord : jamais sacrifiée pour simplifier l'architecture.
- C2. Moteurs officiels Sunshine et Moonlight uniquement, invisibles dans l'expérience utilisateur.
- C3. Mise à niveau upstream réalisable des mois plus tard sans fusion monstrueuse.
- C4. Un seul produit visible ; Windows 11 d'abord ; NVIDIA vers NVIDIA en premier.
- C5. Le flux vidéo ne transite pas par le serveur en fonctionnement normal ; direct prioritaire, relais chiffré en secours.
- C6. Interface premium, priorité très élevée.
- C7. ZyrDesk reste open source.
- C8. AUCUN coût récurrent ni compte payant ni entité légale : pas de certificat de signature payant, pas de compte développeur Microsoft. Les solutions retenues doivent être gratuites (décision du 2026-08-07, à l'origine du choix « pilote tiers déjà signé » pour l'écran virtuel).

## Décisions actées (2026-08-07, étude d'architecture)

- D1. Modèle multi-processus (interface, service, moteur hôte, lecteur de session) ; l'idée « un seul exécutable réel » est abandonnée. Un seul produit installé et visible.
- D2. Le service `zyrdeskd` possède l'identité, le lien broker et TOUTES les extrémités de tunnel (côté client aussi) ; l'interface est sans état ; le lecteur est lancé détaché. Conséquence assumée : installation avec droits administrateur même pour un usage client.
- D3. Tunnel systématique, y compris en LAN ; moteurs strictement en loopback ; mode direct sans tunnel conservé uniquement en diagnostic. Condition attachée : seuils G-lat/G-loss/G-cpu tenus au jalon M2, sinon révision.
- D4. Transport : QUIC ; contrôleur de congestion média sur mesure OBLIGATOIRE. Le choix de bibliothèque est tranché par D13 à l'issue de M2.
- D5. Sunshine intégré en processus enfant piloté par config/CLI/REST, objectif zéro patch (révisé en D15 : plafond de deux) ; Moonlight en processus enfant piloté par CLI + état portable, six micro-patchs maximum ; règle absolue : aucune fonctionnalité ZyrDesk dans les moteurs.
- D6. Dépôt : monorepo + deux forks légers en submodules, pile de commits rebasée sur tags épinglés, miroirs de patchs exportés, suite contrat moteur, répétition mensuelle de mise à niveau. Version plancher Sunshine : v2026.516.143833 (sécurité).
- D7. Interface : dessinée par le produit lui-même, en Direct2D et DirectWrite, dans une fenêtre Win32 à lui ; la vidéo ne traverse jamais l'interface (fenêtre native du lecteur). Le choix d'origine était Tauri v2 (web + cœur Rust), retenu par défaut après examen de Slint, Flutter, Qt, egui/iced ; révisé en D96, D110 et D111, où la dernière vue web puis la dernière boîte à outils sont sorties du produit (détails dans TECH-CHOICES.md).
- D8. Licences : application et forks en GPLv3 ; broker et relais en AGPLv3. Retenu par défaut ; alternatives documentées (tout GPLv3, serveur Apache) si Victor préfère.
- D9. Écran virtuel sans coût : pilote tiers open source déjà signé (Virtual-Display-Driver, MIT) en installation optionnelle consentie, testé dès M1 ; repli = fonction désactivée (écran branché requis ; adaptateur du commerce facultatif à la charge de l'utilisateur final qui veut un PC sans écran). Aucun certificat ni compte payant, conformément à C8.
- D10. Périmètre v1 : clavier, souris, audio, 1080p60/1440p60, direct + relais + reprise, accès non supervisé, presse-papiers texte, mode LAN sans compte, un spectateur actif. Hors v1 : manettes, transfert de fichiers, coupure audio côté hôte, HDR, 120 FPS, partage entre comptes, Wake-on-LAN.
- D11. Pas de bitrate adaptatif en v1 (limite du protocole) : sonde de débit pré-session + préréglages + bascule de qualité rapide + plafond en relais ; assumé et documenté.
- D12. Apollo (fork Sunshine avec écran virtuel intégré) rejeté comme base : divergence et retard sur upstream incompatibles avec C3.

## D13. Transport : quinn maintenant, iroh reconsidéré à M6 (2026-08-07, clôture de O5)

**Décision.** Le transport est bâti sur quinn. Le choix est réexaminé au jalon M6, quand le relais entre en scène.

**Ce qui a été vérifié.** La contrainte dure du projet est le contrôleur de congestion média : sans lui, la vidéo s'étrangle à la première perte, et tout l'édifice « tunnel systématique » tombe. Elle est satisfaite des deux côtés. Chez quinn, le contrôleur est écrit, branché et mesuré (voir plus bas). Chez iroh, l'API expose le même point d'accroche (`QuicTransportConfigBuilder`, traits `Controller` et `ControllerFactory`), ainsi que les datagrammes non fiables : le portage du contrôleur serait mécanique. Le GO/NO-GO technique d'iroh est donc **GO**.

**Pourquoi quinn quand même, et maintenant.** Ce qu'iroh apporte au-delà de quinn (relais traité comme chemin QUIC de première classe, traversée NAT de production, migration relais vers direct sans coupure) ne sert à rien avant M5. L'adopter aujourd'hui reviendrait à suivre l'évolution d'une bibliothèque jeune et en pleine restructuration pendant trois jalons qui n'en utiliseraient aucune fonction. Le fork de quinn qu'iroh maintenait a d'ailleurs été détaché dans un projet à part (noq) en 2026, et son API de transports personnalisés est annoncée comme instable même après la version 1.0.

**Ce qui rend le report peu coûteux.** Le trait d'abstraction `ZyrTransport` prévu par l'étude initiale a été remplacé par quelque chose de plus simple et d'aussi efficace : un seul fichier du produit nomme la bibliothèque de transport (`crates/zyr-transport/src/point.rs`). Tout le reste ne connaît que `Connexion`, `FluxEnvoi`, `FluxReception` et `Bytes`. Un trait n'aurait rien apporté de plus tant qu'il n'existe qu'une implémentation à la fois, et aurait ajouté de l'indirection sur un chemin où chaque paquet compte. Vérifié mécaniquement : aucun autre crate ne mentionne quinn.

**Mesures qui appuient la décision** (boucle locale, version release, taille de paquet 1353 octets ; relevés complets dans [perf/baselines/M2-boucle-locale.md](../perf/baselines/M2-boucle-locale.md)) :

| Condition | Débit tenu | Perte constatée | Aller-retour médian ajouté |
|---|---|---|---|
| 50 Mb/s, sans perte provoquée | 49,5 Mb/s | 0,00 % | +1,19 ms |
| 40 Mb/s, 1 % de perte provoquée | 39,7 Mb/s | 0,98 % | sans effet mesurable |
| 40 Mb/s, 2 % de perte provoquée | 39,7 Mb/s | 1,95 % | sans effet mesurable |

Aucune amplification de perte, aucun effondrement de débit. Un test compare en outre le contrôleur média au contrôleur ordinaire du transport : après une trentaine de pertes, ce dernier tombe sous la fenêtre nécessaire à 40 Mb/s et 25 ms d'aller-retour, le nôtre non.

**Ce que ces mesures ne prouvent pas.** La boucle locale a un aller-retour d'environ 0,15 ms. Or c'est le produit perte x aller-retour qui fait s'effondrer un contrôleur ordinaire : la condition exacte de G-loss (25 ms d'aller-retour simulé, 10 minutes) reste à mesurer sur un vrai chemin. Le banc sait provoquer la perte, pas encore le délai. À faire au jalon M5, où les conditions sont réelles.

**À réexaminer à M6, sur ces critères.** Coût réel de la traversée NAT et du relais écrits à la main contre repris d'iroh ; gain mesuré de la migration relais vers direct sans coupure contre une reconnexion d'environ deux secondes masquée par la reprise ; maturité de noq à cette date ; coût du portage, qui doit rester borné au seul fichier ci-dessus.

## D14. Moteur client épinglé sur une version publiée (2026-08-08, avant M4)

**Décision.** Moonlight est épinglé sur le tag `v6.1.0` et non sur sa branche principale. La montée vers une version plus récente attend qu'upstream en publie une, et suit alors la procédure de [engines/UPGRADING.md](engines/UPGRADING.md).

**Ce qui a motivé le réexamen.** Le jalon M4 est le premier à compiler les moteurs nous-mêmes et à leur appliquer des patchs. Épingler la branche principale n'engageait à rien tant que des binaires officiels préconstruits servaient à prototyper ; à partir d'ici, l'épinglage détermine ce que nous compilons et ce sur quoi notre pile de patchs se rebase.

**Ce qui a été vérifié, et qui corrige l'analyse initiale.** La justification retenue jusqu'ici (« la branche principale apporte l'AV1 et le YUV 4:4:4 ») est fausse : les notes de version de `v6.1.0` annoncent les deux, le 4:4:4 au titre expérimental et explicitement pour l'usage bureau à distance. Ce que la branche principale apporte réellement au-delà, c'est le passage à SDL3 et à Qt 6.11, la refonte du rendu qui l'accompagne, et deux ans de corrections. Aucune version n'a été publiée depuis, y compris après la fin de ce chantier : upstream ne l'a pas encore jugé bon pour une release.

**Ce qui appuie la décision.** Les jalons M1 à M3 ont été validés sur deux machines réelles avec le binaire officiel `v6.1.0` : c'est la seule version dont le comportement soit constaté chez nous. Écrire nos patchs sur une base non publiée reviendrait à ne plus pouvoir distinguer un défaut venant de nous d'un défaut venant d'un chantier upstream en cours.

**Ce que ça coûte, et qui est assumé.** Deux ans de corrections restent dehors, dont plusieurs nous concernent : gestion des plages de couleurs, décodage 8 bits en accélération matérielle sous Windows, robustesse du protocole RTSP, et surtout une correction de juin 2026 sur l'adresse passée en ligne de commande qui se faisait écraser. Ce dernier point touche directement notre montage, puisque nous passons au moteur client une adresse loopback qui remplace l'hôte. Le comportement observé en M3 est le bon (le moteur découvre l'hôte par mDNS à sa vraie adresse, échoue faute d'écoute réseau, et garde le tunnel), mais il tient au fait que l'hôte est injoignable autrement. À réexaminer à la première montée de version.

## D15. Un patch au moteur hôte, contre l'objectif zéro de D5 (2026-08-08, pendant M4)

**Décision.** Le moteur hôte porte un patch, P-S2, et le plafond reste à deux. L'objectif « zéro patch » de D5 devient un plafond de deux, comme pour l'autre moteur, avec la même règle : un patch ne peut qu'exposer un interrupteur ou retirer de l'habillage.

**Ce qui a motivé le réexamen.** Le jalon M4 demande qu'aucun processus au nom d'un autre projet n'apparaisse. Renommer le fichier ne suffit pas : le gestionnaire des tâches et la fenêtre de propriétés lisent le nom de produit compilé dans le binaire, qui n'est posé qu'à la compilation.

**Ce qui a été vérifié.** Le moteur laisse déjà choisir son icône et son éditeur à la configuration, et demande même aux produits tiers de poser les leurs. Le nom de produit était le seul champ de cette série qu'il n'exposait pas. Aucune autre voie n'existe : le champ est écrit dans la ressource de version, avant l'édition de liens.

**Ce que le patch fait, et ce qu'il ne fait pas.** Huit lignes dans un fichier de compilation, aucune ne nommant ZyrDesk. Notre nom est passé par notre script de compilation, donc la marque ne vit pas dans le moteur. Le patch est contribuable en amont tel quel, ce qui est la meilleure fin possible pour un patch : celle où il disparaît.

**Ce que ça coûte.** Une ligne de plus à rebaser à chaque montée de version, dans un fichier qui bouge peu. Et une contingence, P-S1, qui n'aurait plus qu'une place libre après elle : si elle devenait nécessaire, le plafond serait atteint et l'architecture serait à réexaminer, ce qui est exactement le signal que ce plafond existe pour donner.

## D16. Le bouton flottant est une fenêtre à nous, et les sessions s'ouvrent sans bordure (2026-08-08, pendant M4)

> Révisée par [D21](#d21-limage-du-bureau-distant-saffiche-dans-la-fenêtre-de-zyrdesk-2026-08-19-pendant-m4) sur un point : l'image ne s'affiche plus dans une fenêtre du moteur posée à part, mais dans la fenêtre de ZyrDesk. Le plein écran exclusif a disparu avec, et l'entrée « plein écran » du menu bascule notre fenêtre au lieu d'envoyer une combinaison au moteur.
>
> Et par [D23](#d23-la-croix-de-la-fenêtre-termine-la-session-2026-08-19-pendant-m4) sur le paragraphe « ce qui en découle pour la fenêtre d'accueil » : la croix termine désormais la session au lieu d'effacer la fenêtre. Puis par [D24](#d24-une-session-est-en-cours-ou-terminée-jamais-entre-les-deux-2026-08-19-pendant-m4) : partir et fermer ne font plus deux entrées, mais une. Le reste tient.

**Décision.** Pendant une session, la seule chose de ZyrDesk visible par-dessus l'image est une petite fenêtre à nous, toujours au-dessus, qui ne prend jamais le premier plan. Ce qu'elle propose, elle le demande au moteur client par les raccourcis clavier que celui-ci expose déjà, envoyés à la fenêtre de la session et vérifiés avant l'envoi. En conséquence, une session s'ouvre par défaut en fenêtre sans bordure et non en plein écran exclusif.

**Ce qui a été écarté.** Dessiner le bouton dans l'image elle-même. Le moteur client sait afficher des surimpressions, mais il faudrait lui apprendre ce qu'est ZyrDesk, gérer le pointage de la souris et un menu à l'intérieur : très au-delà de ce qu'un patch a le droit d'être (D5), et deux fois plus de code à rebaser à chaque montée de version.

**Ce qui a été vérifié dans le moteur, et non supposé.** Le pointeur n'est enfermé dans la fenêtre qu'en plein écran exclusif ; en fenêtre sans bordure il circule librement. Le curseur local n'est masqué que par la fenêtre du moteur : il réapparaît dès qu'il passe sur une fenêtre d'un autre programme, donc sur la nôtre. Le moteur ne confisque la souris que le temps d'un bouton maintenu hors de sa fenêtre. Et ses raccourcis Ctrl+Alt+Maj existent pour le plein écran, les statistiques, le mode de la souris et l'arrêt, et sont interceptés sans être transmis à l'ordinateur distant.

**Ce qui rend l'envoi de touches acceptable.** Une combinaison part vers la fenêtre au premier plan, quelle qu'elle soit. Avant d'envoyer quoi que ce soit, on vérifie que cette fenêtre appartient bien au processus du lecteur de cette session ; sinon on refuse et on le dit. Un « Ctrl+Alt+Maj+Q » parti dans la mauvaise fenêtre n'est pas un risque à prendre.

**Ce qui en découle pour la fenêtre d'accueil.** Fermer l'accueil pendant une session ne ferme plus le programme : la fenêtre s'efface et le bouton reste, puisque c'est lui qui donne encore prise sur la session. Le programme s'arrête quand la session finit. Et un deuxième lancement ne fait pas un deuxième ZyrDesk : il ramène la fenêtre effacée, sans quoi deux boutons flottants se poseraient sur la même session.

**Ce que ça coûte.** Le plein écran exclusif reste choisissable mais le bouton n'y apparaît pas : rien ne peut être dessiné au-dessus d'une fenêtre qui possède l'écran. Sous Windows 10 et suivants la fenêtre sans bordure ne coûte rien en latence, la chaîne d'échange du moteur étant de celles que le compositeur laisse aller directement à l'écran. Et en mode souris de jeu, le pointeur appartient entièrement à l'ordinateur distant : le bouton n'est alors pas cliquable, et les raccourcis clavier affichés dans le menu sont la façon de faire la même chose.

## D17. Sur un réseau local, plus rien à recopier ni à taper (2026-08-18, pendant M4)

**Décision.** Deux ZyrDesk allumés sur le même réseau local se joignent sans qu'on leur donne quoi que ce soit. Le service admet les empreintes qui s'annoncent en mDNS, en plus de celles écrites dans `data/authorized-devices.conf`, sous un interrupteur activé par défaut. Et le code d'appairage que les moteurs réclament ne s'affiche plus : le client le tire au sort, l'envoie par le canal ZyrDesk du tunnel, et le service hôte le remet à son moteur.

**Ce que ça remplace.** Quatre gestes en ligne de commande avant la première session : lire une empreinte sur une machine, l'autoriser sur l'autre, lire un code à quatre chiffres, le taper sur la première. Rien de tout cela n'apportait de garantie que le réseau local ne donnait pas déjà, et l'ensemble se refaisait à chaque installation.

**Pourquoi le code peut voyager.** Le tunnel reconnaît les deux ordinateurs à leur empreinte, mutuellement, avant qu'un octet ne passe. Le code d'appairage des moteurs prouve donc strictement moins que ce qui vient d'être prouvé : il n'existe que parce que le protocole des moteurs le réclame, et le porter à leur place ne retire rien.

**L'ordre est le mécanisme, pas un détail.** Le moteur hôte refuse un code tant que personne ne lui en demande un. Le moteur client est donc lancé et laissé en attente d'abord, le code part ensuite, et le résultat n'est attendu qu'après. Le service hôte réessaie quelques secondes, les deux moitiés arrivant à un cheveu l'une de l'autre et dans un ordre que rien ne garantit. Côté client, l'attente est bornée : le moteur, lui, n'y met aucune limite, et une session qui s'ouvre indéfiniment sur rien n'est pas une session.

**Ce que ça suppose, et ce que ça n'ouvre pas.** Que le réseau local soit celui de son propriétaire, ce que suppose déjà toute découverte mDNS. Rien venu d'ailleurs que du réseau local n'entre par là. Le jour où les sessions passent par Internet (M5), l'enregistrement auprès du broker prend le relais et cette confiance cesse de couvrir quoi que ce soit au-delà.

**Ce qui reste.** L'ajout par empreinte, pour un réseau où l'annonce ne passe pas, et le code à taper sur le chemin de diagnostic sans tunnel. Ni l'un ni l'autre n'apparaît dans le déroulement normal.

L'ajout par empreinte écrit l'ordinateur dans les deux sens : il le laisse entrer, et il sert de repère pour aller vers lui si une adresse est donnée. Un seul des deux ne suffisait pas, et n'ouvrait rien : sur un réseau muet, la machine d'en face était refusée à l'arrivée après un ajout qui semblait pourtant fait. Le geste est donc le même sur les deux machines, chacune écrivant l'autre.

**Un ordinateur écrit à la main reste à l'écran.** L'écrire une fois doit suffire. Sans cela, le copier-coller que ce chemin remplace ne disparaissait pas : il se répétait à chaque session, ce qui est pire que de le faire une fois. L'adresse, le nom et l'empreinte sont donc gardés dans `data/known-computers.conf`, à part de la liste des appareils admis : l'une décide qui entre, l'autre épargne seulement une recopie, et les confondre ferait d'une commodité une question de sécurité. La carte porte alors une pastille grise et la mention « ajouté à la main », parce qu'aucune annonce ne la confirme sur ce réseau. L'oubli retire des deux listes à la fois, faute de quoi un ordinateur disparu de l'écran continuerait d'entrer.

**L'appairage se refait tout seul quand l'autre l'a oublié.** Ce que le client retient d'un appairage n'est qu'une note qu'il s'est écrite à lui-même, et l'hôte est le seul à décider : réinstallé, remis à zéro ou simplement vidé de son état, il ne reconnaît plus personne. Le moteur client repart alors en moins d'une seconde, dans un journal que personne ne lit, et la fenêtre n'a qu'une session terminée à montrer. La session est donc surveillée le temps qu'elle prenne : un moteur qui abandonne aussitôt déclenche une nouvelle présentation, et la session s'ouvre. Cette surveillance ne coûte rien à la première session, où les présentations viennent d'être faites, et se confond avec le démarrage du moteur pour les suivantes.

## D18. Le moteur hôte n'est plus une condition d'existence du service (2026-08-18, pendant M4)

**Décision.** Le service tourne, ouvre les voies sortantes, s'annonce sur le réseau et répond à l'interface, que le moteur hôte soit là ou non. Un moteur absent ou qui ne tient pas rend cet ordinateur injoignable, et rien de plus. Ce qui empêche d'être joignable voyage jusqu'à la fenêtre, qui le dit en clair.

**Ce que ça corrige.** Le service s'arrêtait net quand le moteur hôte manquait. Un ordinateur qui ne sert que de client perdait donc le tunnel, la découverte du réseau et son interface, pour une moitié du produit dont il n'a pas l'usage. Et la fenêtre n'ayant que « joignable ou non » à afficher, un moteur absent s'y lisait « démarrage en cours » indéfiniment.

**Ce que ça coûte.** Un service qui ne s'arrête plus de lui-même. Il se relit toutes les cinq secondes plutôt que d'insister, et le retour en arrière après un moteur qui ne tient pas est l'interrupteur d'accès distant, coupé puis rallumé.

## D19. La découverte appelle, au lieu d'attendre d'être entendue (2026-08-19, pendant M4)

**Décision.** En plus de l'annonce mDNS, le service pose un petit port à lui, UDP 47001, sur lequel il répond à qui l'appelle, et il appelle : toutes les trois secondes, un datagramme vers l'adresse de diffusion de chaque carte **et un vers chacun des ordinateurs déjà connus, à son adresse**. Tant que personne n'a répondu, il passe en plus le réseau adresse par adresse, au plus toutes les trente secondes. Ce qui répond entre dans la même liste que ce qui s'annonce, et un ordinateur trouvé deux fois reste un seul ordinateur.

**Une pastille verte veut dire joignable maintenant.** C'est pour cela que les ordinateurs déjà listés sont réinterrogés directement : une diffusion que la box laisse tomber laisserait une machine affichée en vert jusqu'à ce qu'elle expire, et proposerait de s'y connecter. Un ordinateur qu'on quitte proprement dit au revoir et disparaît en une seconde ; un qu'on débranche cesse simplement de répondre et sort de la liste au bout d'une douzaine de secondes. Cet au revoir n'est cru que s'il vient de l'adresse où cet ordinateur est connu, faute de quoi n'importe qui sur le réseau ferait disparaître n'importe qui de l'écran de n'importe qui d'autre.

**Ce que ça corrige.** Deux machines du même sous-réseau, l'une en Ethernet et l'autre en Wi-Fi, ne se sont jamais vues : les journaux montrent les deux qui annoncent correctement par la bonne carte, les deux qui reçoivent bien du trafic sur cette même carte, et rien qui traverse. Le pare-feu est ouvert des deux côtés, le classement Windows est privé des deux côtés, aucun VPN ne tourne, et une session manuelle entre les deux fonctionne parfaitement à 0 % de perte. Le multicast, lui, ne passe pas : beaucoup de box et de points d'accès le jettent entre le filaire et le sans-fil, et rien dans les deux machines ne peut y changer quoi que ce soit.

**Pourquoi ça marche là où l'autre échoue.** Une diffusion dirigée et un datagramme adressé sont du trafic ordinaire, routé comme le reste : un réseau qui porte une session porte cela. Le multicast, lui, dépend d'un relayage que l'équipement décide seul.

**Ce que ça coûte, et ce qui le borne.** Un port de plus à ouvrir, posé par le service comme les deux autres. Un datagramme par carte toutes les dix secondes, ce qui n'est rien. Le passage adresse par adresse est le seul geste bruyant : il ne se fait que tant que la liste est vide, jamais plus d'une fois toutes les trente secondes, et seulement sur un réseau d'au plus 256 adresses. Un réseau plus large n'est jamais parcouru ainsi ; un tel réseau relaie de toute façon presque toujours le multicast.

**Ce que ça n'ouvre pas.** Ce port ne dit que ce que l'annonce disait déjà : un nom, une empreinte, un numéro de port. Rien n'y entre, rien ne s'y décide, et une empreinte connue n'ouvre toujours rien à elle seule. Ce qui arrive sans le mot de passe du format est jeté sans être lu.

## D20. Rien ne tourne quand personne ne s'en sert (2026-08-19, pendant M4)

**Décision.** Le service n'est plus enregistré pour démarrer avec Windows. C'est la fenêtre qui le lance en s'ouvrant et qui l'arrête en étant quittée, et une icône dans la zone de notification dit, tant que le produit tourne, si cet ordinateur peut être pris en main. Fermer la fenêtre la range sans rien arrêter ; « Quitter », dans le menu de cette icône, arrête tout. Un réglage, décoché par défaut, rétablit l'ancien comportement : le service démarre alors avec la machine, l'ordinateur répond avant même qu'on ouvre une session dessus, et ZyrDesk revient tout seul avec son icône.

**Ce que ça corrige.** Un service qui rend la machine joignable tournait en permanence sans que rien à l'écran ne le dise, et l'arrêter demandait une ligne de commande. Ce n'est pas une question de goût : un produit de prise en main à distance qui tourne invisiblement est un produit dont personne ne peut dire s'il est actif.

**Pourquoi l'icône n'est pas un ornement.** Elle est la réponse à la seule question qu'un tel produit ne doit jamais laisser sans réponse. Elle est nette quand la machine est joignable, atténuée quand elle ne l'est pas, et son infobulle le dit en toutes lettres, parce qu'un état ne se lit jamais à la couleur seule.

**Ce que ça suppose.** Que démarrer et arrêter le service ne demande pas les droits administrateur à chaque fois, sans quoi le produit serait inutilisable. L'enregistrement du service, qui est le seul moment où ces droits sont déjà en main, accorde donc à la personne connectée le droit de le démarrer et de l'arrêter, et rien d'autre. Changer où le service pointe reste réservé aux administrateurs : c'est le droit qui ouvrirait une élévation de privilèges, et il n'est pas donné. L'arrêt passe d'ailleurs par le canal de commande plutôt que par Windows, le service se coupant lui-même.

**Ce que ça coûte.** Un ordinateur qui redémarre pendant une absence n'est plus joignable, réglage décoché, tant que personne ne va ouvrir ZyrDesk dessus. C'est exactement ce que ce réglage sert à choisir, et le mot en dessous le dit sans détour.

## D21. L'image du bureau distant s'affiche dans la fenêtre de ZyrDesk (2026-08-19, pendant M4)

> Révisée par [D22](#d22-plus-une-seule-bande-noire-2026-08-19-pendant-m4) sur son dernier paragraphe : les bandes noires n'étaient pas le prix à payer, et il n'y en a plus. Le reste tient.

**Décision.** Une session n'ouvre plus de deuxième fenêtre. Le moteur client continue de dessiner dans une fenêtre à lui, ce qui est indispensable, mais cette fenêtre est dépouillée de son cadre, posée exactement sur l'intérieur de la nôtre et contrainte à la suivre. Ce qui se voit à l'écran est une seule fenêtre : la nôtre, avec sa barre de titre, et l'image dedans. Le moteur est donc toujours lancé en mode fenêtré, et le réglage d'affichage ne parle plus que de notre fenêtre : plein écran ou fenêtre.

**Ce qui a été écarté.** Faire de la fenêtre du moteur un enfant de la nôtre, ce que Windows appelle une fenêtre fille. C'est la façon évidente et c'était la mauvaise : une fenêtre fille n'est jamais au premier plan, et ce que le moteur obtient du système pour prendre le clavier et la souris dépend précisément du fait que sa fenêtre soit celle de devant. Ç'aurait coûté la chose même pour laquelle un bureau à distance existe. La fenêtre est donc « possédée » et non « fille » : elle reste une fenêtre de plein droit, qui ne quitte jamais le devant de la nôtre, se minimise avec elle, et disparaît de la barre des tâches et d'alt-tab où elle passait pour un second ZyrDesk.

**Ce qui a aussi été écarté.** Faire passer l'image par la vue web. Ce serait la seule façon d'avoir vraiment une seule fenêtre au sens du système, et ce serait payer en latence exactement ce que ce produit existe pour économiser. Rien de ZyrDesk n'est sur le chemin d'une image, et cette décision ne change pas cela d'un pixel.

**Ce que ça emporte.** Le plein écran exclusif du moteur disparaît, avec le réglage à trois valeurs qui le proposait. C'est notre fenêtre qui prend l'écran maintenant, et l'entrée « plein écran » du menu flottant, comme le raccourci clavier qui lui correspond, bascule cette fenêtre-là au lieu d'envoyer une combinaison au moteur. Le bouton flottant, lui, ne change pas : il reste une fenêtre à nous, toujours au-dessus, y compris au-dessus de l'image.

**Ce que ça coûte, et ce qu'il a fallu rendre au système.** Une fenêtre redimensionnée pendant une session redimensionne l'image, sans changer la définition du flux, qui reste celle de la qualité choisie. Et deux fenêtres posées l'une sur l'autre ne font une seule fenêtre que si le système les traite comme telle, ce qui ne va pas de soi et se paie en trois endroits, tous décrits par [D23](#d23-la-croix-de-la-fenêtre-termine-la-session-2026-08-19-pendant-m4) : la barre de titre, le bouton flottant et la croix.

## D22. Plus une seule bande noire (2026-08-19, pendant M4)

**Décision.** Une session ne montre jamais de bande noire, ni en haut, ni en bas, ni sur les côtés. Cela se joue aux deux bouts et il faut les deux : l'ordinateur d'en face met son bureau à la forme demandée pendant la session et le remet après, et notre fenêtre prend la forme de l'image qu'elle contient au lieu de la lui imposer.

**Ce qui les fabriquait.** Le moteur hôte filmait le bureau tel quel et le faisait entrer dans le flux en gardant ses proportions, donc en remplissant le reste de noir. Un écran seize-dixièmes regardé en seize-neuvièmes perdait quatre-vingt-seize pixels d'image de chaque côté, gravés dans chaque trame, avant même le moindre encodage : plus rien à notre bout ne pouvait les enlever. Et de notre côté, le moteur client fait la même chose dans l'autre sens, avec le même raisonnement, dès que la fenêtre n'a pas la forme de l'image.

**Ce qu'il a fallu comprendre pour le bout distant.** Quatre lignes de configuration, pas une, et chacune fait une moitié différente : la première autorise le moteur à toucher aux écrans, sans quoi les trois autres ne sont même pas lues ; les deux suivantes disent ce qui peut changer, taille et fréquence ; la dernière dit quand remettre en place, et ce n'est pas la réponse évidente. Le moteur attend sinon l'arrêt de l'application qu'il diffuse, et celle que nous diffusons est le bureau lui-même, qui ne s'arrête jamais : quitter une session sans la fermer aurait rendu un ordinateur resté à la taille qu'on lui avait donnée.

**Et une cinquième, du côté client.** Le moteur hôte ne touche à rien tant que le client ne l'a pas autorisé, par un drapeau dont le nom vient des jeux vidéo et qui ne veut plus rien dire d'autre que ça face à ce moteur-là. Il est donc envoyé explicitement à chaque session, plutôt que laissé au réglage que le moteur garde dans un fichier à lui.

**Ce qui a été écarté.** Demander la définition de notre écran plutôt qu'une définition choisie. Ce serait juste sur un écran seize-neuvièmes et faux ailleurs, et cela retirerait à la personne le seul réglage qui décide vraiment de ce que le réseau doit porter.

**Où la forme de l'image est lue.** Dans la fenêtre du moteur, à l'instant où nous la prenons et avant d'y toucher : il la crée à la forme de l'image qui va y arriver. Pas dans ce que la session a demandé, qui n'est qu'un souhait : l'ordinateur d'en face répond avec ce que son écran s'est révélé capable de faire.

**Ce que ça coûte.** En fenêtre, tirer un coin change la hauteur en même temps que la largeur. C'est ce que font les lecteurs vidéo, et c'est le seul moyen de ne pas redemander une bande noire à chaque geste.

## D23. La croix de la fenêtre termine la session (2026-08-19, pendant M4)

**Décision.** La croix veut dire deux choses selon ce que la fenêtre montre. Sur une session, elle quitte la session et rend l'accueil : l'image est dans cette fenêtre, et une croix qui se contenterait de ranger la fenêtre laisserait l'ordinateur d'en face tenu par quelque chose que plus rien à l'écran ne permet de lâcher. Sur l'accueil, elle range la fenêtre sans rien arrêter : cet ordinateur peut être joignable sans que personne ne regarde une fenêtre, l'icône à côté de l'horloge le dit, et « Quitter » dans son menu reste le seul geste qui arrête le produit.

**Ce que ça remplace.** [D16](#d16-le-bouton-flottant-est-une-fenêtre-à-nous-et-les-sessions-souvrent-sans-bordure-2026-08-08-pendant-m4) posait qu'une croix ne devait jamais couper une session, et c'était juste tant que la session avait sa propre fenêtre : fermer l'accueil ne touchait pas à l'image. Depuis [D21](#d21-limage-du-bureau-distant-saffiche-dans-la-fenêtre-de-zyrdesk-2026-08-19-pendant-m4), l'image est dans cette fenêtre-là, et le geste ne veut plus dire la même chose.

**Le même geste que le menu, et pas un second.** La croix reprend le chemin de l'entrée « terminer la session », qui rend son bureau à l'ordinateur d'en face ([D24](#d24-une-session-est-en-cours-ou-terminée-jamais-entre-les-deux-2026-08-19-pendant-m4)).

**Deux fenêtres, une seule aux yeux du système.** Trois choses restaient à rendre, et aucune n'est cosmétique.

La barre de titre. Le système donne le premier plan à la fenêtre de l'image, parce que c'est là que le moteur doit être pour tenir le clavier et la souris, et il dessine en atténué toute fenêtre qui perd le premier plan. Ce qui le lui prenait étant notre propre image dans notre propre fenêtre, cet atténuement disait quelque chose de faux, à chaque session fenêtrée. Le message par lequel le système pose la question est intercepté et répondu « active ». C'est à cela que sert ce message : le système demande au lieu de décider, précisément pour qu'une fenêtre dont la compagne tient l'activation puisse dire qu'elle reste celle qu'on utilise. La question n'est retournée que tant que le premier plan appartient à l'image, donc passer sur un autre programme atténue la barre comme il se doit.

Et une question ne se pose qu'une fois. Le premier plan part au moment même où la session s'ouvre, c'est-à-dire avant qu'il y ait une image à connaître et donc avant qu'il y ait quelqu'un pour répondre : la barre naissait atténuée et le restait jusqu'à ce qu'autre chose fasse reposer la question, un clic dessus par exemple. Elle est donc dite active à voix haute juste après, chaque fois que le premier plan est remis à l'image.

Les coins. Le système arrondit les coins de toutes les fenêtres, et l'image en est une, à part, qui reste un rectangle : un cadre arrondi montrait une image à angles droits, et les deux coins du bas vendaient la mèche. C'est l'image qui est découpée pour suivre, et seulement en bas, le haut étant sous la barre de titre là où le cadre est droit. Le rayon est celui de Windows 11, mis à l'échelle de l'écran.

Le redimensionnement, et c'est là que le plus de choses étaient à reprendre. Tenir la forme en corrigeant après coup redimensionnait la fenêtre deux fois par cran du geste, et chaque redimensionnement est un message au programme du moteur et une chaîne d'échange reconstruite là-bas ; la forme est maintenant tenue pendant le geste, sur le rectangle que le système propose avant de le prendre. Poser l'image passait par la file d'événements de la boîte à outils, qui arrive une file après la fenêtre elle-même, et une image une file derrière le cadre qui la porte est une image qui traîne visiblement ; c'est fait maintenant dans le gestionnaire de messages de la fenêtre, dans le même souffle que son déplacement. Et le bouton flottant était déplacé en demandant deux fois à la boîte à outils, cent fois par seconde ; il est déplacé directement, comme l'image.

Rien de tout cela ne passe plus par la boîte à outils. La place des fenêtres d'une session est une affaire du système, pas de l'interface, et elle avait fini par emprunter le chemin le plus long possible.

Le bouton flottant. C'était une fenêtre sans attache : réduire ZyrDesk le laissait seul dans un coin de bureau vide, par-dessus le travail des autres, et il était placé une fois pour toutes, donc une session repassée en fenêtre le laissait suspendu au milieu de l'écran, sur rien. Il est maintenant possédé par la fenêtre d'accueil, comme l'image, donc il descend et remonte avec elle sans qu'on ait à s'en occuper ; et il est reposé à chaque fois que l'image l'est, sur le même rectangle qu'elle, calculé une fois pour les deux.

L'ordre entre les deux tient à un seul fait : le bouton est marqué toujours au-dessus et l'image ne l'est pas.

**Ce que ça a coûté ailleurs.** Ces deux fenêtres possédées s'en vont avec la fenêtre réduite, ce qui est voulu, mais un bureau à distance ne se pose pas sur une fenêtre qui n'est pas là : poser l'image sur une fenêtre réduite reviendrait à la réduire à rien et à demander au moteur de dessiner pour une surface sans taille, une fois par seconde. Rien n'est posé tant que la fenêtre n'est pas debout, et tout revient de soi-même quand elle revient.

## D24. Une session est en cours ou terminée, jamais entre les deux (2026-08-19, pendant M4)

**Décision.** Le produit n'offre qu'une façon de finir une session, et elle rend son bureau à l'ordinateur distant. Il n'y a plus d'entrée « quitter » à côté d'une entrée « fermer », ni dans le menu flottant, ni dans les raccourcis clavier, ni sur la croix de la fenêtre.

**Ce que ça remplace.** Les moteurs distinguent partir et fermer : partir arrête le flux et laisse l'ordinateur d'en face tenir son bureau, prêt pour un retour immédiat ; fermer le lui rend. Cette distinction a été portée telle quelle jusqu'à la personne, et c'était une erreur : elle laissait une session ni en cours ni terminée, un état qu'il fallait connaître pour savoir dans lequel des deux on se trouvait, et que rien à l'écran ne montrait.

**Pourquoi c'est celle-là qui reste.** Entre les deux, une seule répond à la question que la personne se pose en cliquant. « J'ai fini » veut dire que la machine d'en face est libre, pas qu'elle attend. Garder l'autre aurait voulu dire l'expliquer, et une explication est le prix d'un mauvais modèle.

**Ce que ça coûte.** Terminer une session demande maintenant un aller-retour à l'ordinateur d'en face, là où partir se faisait sur place. Quelques dixièmes de seconde, et une réponse qui peut ne pas revenir : demander à un ordinateur de lâcher son bureau emporte le chemin par lequel la question a été posée. Ce coût a été payé par la personne pendant un temps, et il ne l'est plus : voir [D26](#d26-finir-une-session-ne-dépend-plus-de-lordinateur-den-face-2026-08-21-pendant-m4).

**Ce qui reste ouvert.** Le moteur client garde son propre raccourci de départ, celui qui laisse le bureau distant ouvert : il l'intercepte lui-même et rien de ce côté ne peut le lui retirer sans un patch. Il ne figure plus nulle part dans le produit.

## D25. Un septième patch au moteur client, contre le plafond de D5 (2026-08-19, pendant M4)

**Décision.** Le plafond de six micro-patchs pour Moonlight, posé par [D5](#décisions-actées-2026-08-07-étude-darchitecture), passe à sept. Le septième, P-M9, apprend au moteur de rendu D3D11 à encaisser un changement de taille de fenêtre au lieu de tout reconstruire.

**Ce qui l'a rendu nécessaire.** Redimensionner la fenêtre d'une session saccadait, au point d'être inutilisable. La cause a été cherchée deux fois de travers avant d'être mesurée : l'interface chronomètre maintenant chaque partie d'un redimensionnement et écrit une ligne dans le journal quand la main lâche. Le verdict est sans appel : onze crans en cinq secondes, 2394 ms sur 2399 passés à déplacer la fenêtre du moteur, jusqu'à 351 ms pour un seul cran ; le système et la vue web, 12 ms en tout.

**Ce que fait le moteur.** À chaque changement de taille, il détruit et reconstruit son décodeur : appareil D3D11, chaîne d'échange, sept nuanceurs, et une image clé redemandée à l'ordinateur d'en face. Non par accident, mais parce que l'appelant n'a aucun moyen de savoir ce qu'un moteur de rendu sait encaisser et suppose donc qu'il n'encaisse rien. Le moteur de rendu SDL, lui, renonce explicitement aux changements de taille sur Windows, avec un commentaire qui dit ne pas savoir pourquoi cela casse ; la réponse est le fil de dessin, et c'est ce que P-M9 traite en prenant le verrou du dessin.

**Pourquoi le plafond cède plutôt que la fonctionnalité.** Le plafond existe pour protéger la remontée de version ([C3](#contraintes-posées-par-victor-non-négociables)), et la règle attachée dit d'aller chercher le mécanisme officiel manquant plutôt que d'empiler. Il a été cherché : il n'y en a pas. Le moteur ne propose aucun réglage, aucun interrupteur, aucune façon depuis l'extérieur d'éviter la reconstruction, et rien de ce qui peut être fait à notre bout n'atteint la fenêtre avant que le moteur ne la reconstruise. Renoncer aurait voulu dire une fenêtre de session qui ne se redimensionne pas en temps réel, ce que la contrainte [C1](#contraintes-posées-par-victor-non-négociables) ne permet pas de laisser passer.

**Ce qui limite le risque.** Le patch tient dans un fichier, ne touche qu'un moteur de rendu sur six, et n'ajoute aucune notion de ZyrDesk : c'est un défaut de performance de Moonlight, mesurable sans ZyrDesk, et un candidat direct à une contribution en amont. Le chemin d'échec est celui qui existait : tout ce qui n'est pas un simple changement de taille, et toute erreur en route, repart par la reconstruction complète.

**Ce qu'il faut surveiller.** Sept patchs sur sept. Le prochain besoin d'un patch client n'a plus de marge : il faudra soit remonter le correctif en amont et attendre une version qui le porte, soit rouvrir D5 pour de bon.

## D26. Finir une session ne dépend plus de l'ordinateur d'en face (2026-08-21, pendant M4)

**Décision.** Terminer une session, par la croix comme par le menu, fait deux choses qui ne s'attendent pas l'une l'autre. L'ordinateur d'en face est prié de reprendre son bureau, sur un fil à part, et ce qu'il répond ne va que dans le journal. De ce côté-ci, l'image a trois secondes pour s'en aller toute seule, ce qu'elle fait quand la réponse arrive ; passé ce délai le lecteur est arrêté ici. La croix ramène à l'accueil dans tous les cas.

**Ce qui l'a rendue nécessaire.** L'aller-retour de [D24](#d24-une-session-est-en-cours-ou-terminée-jamais-entre-les-deux-2026-08-19-pendant-m4) était attendu par la fenêtre. Quand la session avait lâché, c'est-à-dire précisément quand on veut la fermer, la question partait vers une machine qui ne répondait plus et mettait quinze secondes à être déclarée injoignable, pendant lesquelles la croix ne faisait rien du tout. Dit par Victor : « quand une session se coupe je ne peux pas fermer la fenêtre avec la croix ça fait rien du tout ».

**Pourquoi ne pas simplement raccourcir l'attente.** Parce que les deux moitiés du geste n'ont pas le même destinataire. Rendre le bureau distant est une politesse envers l'autre machine et peut échouer sans conséquence ici ; rendre l'accueil est ce que la personne a demandé et ne peut pas échouer. Les lier, c'était faire dépendre le certain de l'incertain.

**Ce qui est arrêté, et comment.** Le lecteur, par son numéro, avec le code de sortie d'une fin normale : c'en est une, puisque c'est ce qui a été demandé. Rien n'est perdu, le lecteur ne garde rien ; le service rend le chemin dès que le processus s'en va, et le fil qui attendait ce processus se réveille au même instant.

**Ce que ça coûte.** Un bureau distant qui n'a pas eu le temps d'être rendu reste tenu par son moteur jusqu'à ce qu'il constate le départ du client. C'est le cas où la machine ne répondait déjà plus, donc le cas où il n'y avait rien à faire de mieux.

## D27. La taille, le débit et le codec s'appliquent en relançant l'image (2026-08-21, pendant M4)

**Décision.** Ces trois réglages-là se changent dans le menu de la session, et une ligne « Appliquer les changements » apparaît dès que ce qui est choisi n'est plus ce qui est à l'écran. La cliquer arrête le lecteur et le rouvre avec les nouvelles valeurs, sans fermer la session ni revenir à l'accueil : la fenêtre garde sa taille, son plein écran, et le tunnel n'est refait que le temps de l'ouverture.

**Pourquoi une ligne et pas un effet immédiat.** Le moteur client reçoit la taille, le débit et le codec en arguments de démarrage et n'a aucune façon de les apprendre en marche. Les appliquer à chaque clic relancerait l'image à chaque clic, ce qu'aucune ligne de menu ne devrait faire sans qu'on le lui demande. Le bouton existe pour qu'on puisse en changer trois et ne payer qu'une relance, ce que Victor a demandé mot pour mot.

**Ce que ça remplace.** La note « s'applique à la prochaine session », qui était vraie et inutilisable : chercher où est le mur de cadence demande d'essayer une valeur, de la regarder, et d'en essayer une autre, et fermer la session entre chaque essai fait perdre la comparaison.

**Ce qui distingue ces trois-là du reste.** Tout le reste du menu se demande au moteur en marche, par les raccourcis auxquels il répond, ou ne le regarde pas du tout : le plein écran est notre fenêtre, les statistiques et le mode de la souris sont des frappes envoyées au moteur. Seuls ces trois nombres se règlent au démarrage, et seuls eux ont un bouton.

**Ce que ça coûte.** Les quelques secondes d'une ouverture, celles que le journal montre déjà, et le fait que l'ordinateur d'en face voit un client partir et revenir. La fenêtre montre l'écran d'ouverture pendant ce temps, avec ses étapes ordinaires, pour que ce ne soit pas confondu avec une session qui a lâché.

## D28. Alt+Tab et la touche Windows agissent sur l'ordinateur distant (2026-08-23, pendant M4)

**Décision.** Une session demande maintenant au moteur client de capturer ces combinaisons, par sa propre option officielle (`--capture-system-keys always`) et non par un correctif : dès que l'image tient le clavier, Alt+Tab et la touche Windows partent vers l'ordinateur distant plutôt que d'agir sur celui qui les tape.

**Ce qui l'a rendu nécessaire.** Dit par Victor : « quand je alt tab dans la session ça alt tab sur le client au lieu de l'host ». C'est le défaut inverse du réglage par défaut du moteur, pensé pour une fenêtre parmi d'autres sur un bureau : dans ZyrDesk, l'image tient toute la fenêtre pendant qu'elle a le clavier, et il n'y a rien d'autre vers quoi basculer ici.

**Pourquoi `always` et non `fullscreen`.** Le moteur offre les deux, et le second ne s'applique que quand sa propre fenêtre couvre l'écran au sens où lui l'entend. La sienne ne le fait jamais : elle est toujours lancée fenêtrée, posée dans la nôtre, qui est seule à décider de couvrir l'écran ou non ([D21](#d21-limage-du-bureau-distant-saffiche-dans-la-fenêtre-de-zyrdesk-2026-08-19-pendant-m4)). `fullscreen` ne se serait donc jamais déclenché.

**Ce que ça coûte.** Le clavier seul ne ramène plus le premier plan à ce PC-là pendant qu'il appartient à l'image ; il faut la souris, un clic sur une autre fenêtre. Deux essais qui s'appuyaient sur Alt+Tab pour ça, S9bis et la seconde moitié de S9ter, sont réécrits pour cliquer à la place : ce qu'ils vérifient (le bouton flottant et la barre de titre suivent le premier plan) ne dépend pas de la façon dont ce premier plan a été perdu.

**~~Ce qui reste ouvert~~ corrigé le 2026-08-23 par [D32](#d32-les-touches-que-windows-garde-pour-lui-sont-reprises-par-zyrdesk-pas-par-le-moteur-2026-08-23-pendant-m4).** Il était écrit ici que nos propres raccourcis restaient joignables au clavier, sur la foi qu'un raccourci global du système est à un niveau que la capture du moteur ne touche pas. C'est faux : le moteur se met devant chaque frappe de tout l'ordinateur et avale Alt et Ctrl en entier, donc au-dessus de ce niveau-là. Tous nos raccourcis sont des combinaisons Alt, et aucun ne fonctionnait tant que le moteur tenait ces touches. L'intention de cette décision tient ; le moyen est remplacé.

## D29. Une combinaison déjà prise par un autre programme se dit, plutôt que de se taire (2026-08-23, pendant M4)

**Décision.** Avant d'envoyer Ctrl+Alt+Shift+S ou +M au lecteur, ZyrDesk essaie de la réclamer lui-même pour un instant, par un raccourci système ordinaire, puis la rend aussitôt. Un refus dit qu'un autre programme la tient déjà, et la fenêtre le dit à son tour au lieu de prétendre que la frappe est bien partie.

**Ce qui l'a rendu nécessaire.** Dit par Victor : « le bouton statistiques du FAB ne fonctionne pas ». Ces deux combinaisons sont celles que le moteur écoute et ZyrDesk ne peut pas en choisir d'autres à sa place ; il les tape en simulant une frappe (`SendInput`), et Windows répond toujours que l'envoi a réussi, qu'un programme l'ait vraiment reçue ou non. Un programme tiers qui aurait pris la même combinaison pour lui-même l'intercepte avant qu'elle n'atteigne la session, sans que rien dans ZyrDesk ne puisse jusqu'ici le savoir.

**Pourquoi ça ne prouve pas que c'était la cause.** Ce n'est pas la seule explication possible à une entrée du menu qui ne montre rien, seulement la seule que ZyrDesk peut vérifier lui-même. Si la combinaison n'est prise par personne, le journal le dit aussi (« envoyé au lecteur »), et la cause est alors ailleurs, du côté du moteur.

**Ce que ça coûte.** Rien d'observable : la réclamation et l'abandon prennent un instant avant chaque envoi, et ZyrDesk ne garde jamais la combinaison pour lui, ce qui la laisserait indisponible pour qui la tenait avant.

## D30. Redonner le clavier au bouton flottant redonne aussi le premier plan (2026-08-23, pendant M4)

**Décision.** Après avoir refermé le menu du bouton flottant, le premier plan est rendu à l'image par la même voie que le raccourci de plein écran (une demande explicite au système), et non plus seulement par l'entrée partagée entre les deux programmes. La seconde ne suffit qu'à ce que ce qui est tapé arrive ; la première est ce dont le moteur a besoin pour croire que son propre premier plan a changé, ce qui commande à son tour s'il capture Alt+Tab ([D28](#d28-alt-tab-et-la-touche-windows-agissent-sur-lordinateur-distant-2026-08-23-pendant-m4)).

**Ce qui l'a rendu nécessaire.** [D16](#d16-le-bouton-flottant-est-une-fenêtre-à-nous-et-les-sessions-souvrent-sans-bordure-2026-08-08-pendant-m4) avait déjà résolu la moitié visible de ce défaut : ce qui est tapé arrive de nouveau après le menu. Dit par Victor après coup : « si j'ouvre le fab et le referme bah le alt tab retourne sur le client ». La fenêtre du bouton est marquée pour qu'un clic dessus ne la rende jamais active elle-même, ce qui était pensé comme suffisant pour que le premier plan ne bouge pas du tout ; en réalité une telle fenêtre répond à un clic en donnant le premier plan à celle qui la possède, la fenêtre d'accueil, donc il bougeait quand même, d'un cran, sans que rien ne s'en aperçoive.

**Pourquoi la demande n'est pas faite à chaque fois.** Prendre le premier plan à un autre programme est le geste qui n'appartient pas à ce bouton, et la demande n'est donc faite que lorsque le premier plan n'a pas quitté ZyrDesk. Ailleurs, seule l'entrée reste partagée, ce qui reste la bonne réponse : le clavier continue d'être reçu sans rien arracher à qui a réellement le premier plan.

**Le diagnostic qui va avec.** Quand le premier plan n'appartient ni à ZyrDesk ni à l'image, le journal nomme maintenant la fenêtre qui le tient (processus, programme, titre) au lieu de dire seulement « ailleurs ». Une fenêtre tierce s'y est vue une fois, brièvement, à l'instant même du clic sur le bouton ; d'où elle vient n'est pas établi, et cette ligne est ce qui permettra de le savoir si ça revient.

**~~Décision~~ corrigée le 2026-08-23 par [D31](#d31-le-clavier-de-la-session-se-rend-par-le-focus-et-jamais-par-le-premier-plan-2026-08-23-pendant-m4).** Le raisonnement ci-dessus est faux sur son point central : l'image est portée comme une fenêtre fille de la nôtre pendant toute une session, et une fenêtre fille ne peut jamais être celle du premier plan. La demande décrite ici ne pouvait donc rien faire d'autre que réactiver notre propre fenêtre. Seul le diagnostic ajouté (nommer la fenêtre tierce) est conservé.

## D31. Le clavier de la session se rend par le focus, et jamais par le premier plan (2026-08-23, pendant M4)

**Décision.** Tout ce qui rend le clavier à l'image passe par une seule voie : l'entrée que ZyrDesk a jointe à celle du moteur, dans laquelle le focus est confié à l'image. Le premier plan n'est plus jamais demandé pour ça. Et rien n'est tapé vers la session sans avoir d'abord rendu ce focus et vérifié qu'il a bien atterri sur l'image.

**Ce qui l'a rendu nécessaire.** Dit par Victor : « Non ça a rien changé et le bouton statistiques dans le fab ne fait rien fréro j'ai l'impression que tu fais du bricolage là ». C'était juste. [D30](#d30-redonner-le-clavier-au-bouton-flottant-redonne-aussi-le-premier-plan-2026-08-23-pendant-m4) demandait le premier plan pour une fenêtre qui ne peut pas l'avoir : l'image est portée comme fille de la fenêtre d'accueil ([D21](#d21-limage-du-bureau-distant-saffiche-dans-la-fenêtre-de-zyrdesk-2026-08-19-pendant-m4)), et le système donne le premier plan au chef de famille, jamais à un membre. La demande réussissait donc à réactiver notre propre fenêtre, où le premier plan était déjà, et le journal le disait sans que ce soit lu : « le premier plan est à ZyrDesk », jamais « à l'image », de la première image d'une session à la dernière.

**Ce que ça répare vraiment.** Le bouton Statistiques. Cliquer sur le bouton flottant donne le clavier à la page de ce bouton ; la frappe envoyée ensuite était lue par notre propre vue web et jetée, pendant que Windows répondait que l'envoi avait réussi, comme il le fait toujours. Le journal disait « statistiques envoyé au lecteur », et c'était vrai : envoyé chez nous. Le clavier est maintenant rendu à l'image et vu y atterrir avant chaque frappe, sinon la fenêtre le dit et n'envoie rien.

**Ce que ça ne répare pas.** Alt+Tab après un passage par le bouton flottant. La cause en est établie et elle est ailleurs : le moteur client décide qu'il tient le clavier en comparant sa propre fenêtre à celle du premier plan du système. Portée dans la nôtre, la sienne ne peut plus jamais être celle-là, donc au premier message de focus qui lui parvient il conclut qu'il l'a perdu et relâche sa capture des touches du système, définitivement pour la session. Traité à part par [D32](#d32-les-touches-que-windows-garde-pour-lui-sont-reprises-par-zyrdesk-pas-par-le-moteur-2026-08-23-pendant-m4).

**Ce que ça coûte.** Le journal dit une ligne de plus par ouverture et par fermeture du menu flottant, et nomme désormais le premier plan à chaque fois qu'il parle du clavier.

## D32. Les touches que Windows garde pour lui sont reprises par ZyrDesk, pas par le moteur (2026-08-23, pendant M4)

**Décision.** Tant qu'une session est à l'écran **et au premier plan**, ZyrDesk se met devant chaque frappe de l'ordinateur, intercepte Alt+Tab, Alt+Échap et Ctrl+Échap avant que Windows n'agisse dessus, et les porte telles quelles à la fenêtre de l'image. Partout ailleurs, sur toutes les autres touches, et pour les frappes que ZyrDesk envoie lui-même, rien n'est touché. L'option du moteur client qui faisait ça, demandée par [D28](#d28-alt-tab-et-la-touche-windows-agissent-sur-lordinateur-distant-2026-08-23-pendant-m4), est retirée : l'intention de D28 tient, le moyen change.

**Pourquoi pas en les réclamant, ce qui serait plus propre.** Parce que Windows refuse. Réclamer une combinaison, comme ZyrDesk réclame ses propres raccourcis, est le moyen sans concurrence et il a été essayé : le système en a rendu **trois sur quatre**, écrit tel quel dans le journal (`1 combinaison tenue, 3 refusées`). Alt+Tab, Alt+Maj+Tab et Alt+Échap sont à lui et il ne les cède pas, quoi qu'on demande. Seul Ctrl+Échap se réclame, ce qui ne sert à rien tout seul. Se mettre devant les frappes n'est donc pas un choix parmi plusieurs : c'est le seul moyen, et c'est celui que tous les bureaux à distance emploient.

**Ce qui l'a rendu nécessaire.** Deux choses dites par Victor, à deux tours d'écart. D'abord « Non ça a rien changé » : le moteur ne peut pas se servir de son option ici. Il décide qu'il tient le clavier en comparant sa propre fenêtre à celle que le système appelle celle du premier plan ; la sienne est portée dans la nôtre pendant toute la session, donc c'est une fenêtre fille, et une fenêtre fille n'est jamais celle-là ([D31](#d31-le-clavier-de-la-session-se-rend-par-le-focus-et-jamais-par-le-premier-plan-2026-08-23-pendant-m4)). Au premier message de focus qui lui parvient, quelques secondes après le début, il conclut qu'il a perdu le clavier et relâche ces touches pour le reste de la session.

Ensuite : « je perdais mes raccourcis clavier de zyrdesk comme par exemple alt + & pour switcher plein ecran/fenetré ». C'est la seconde moitié, et elle condamne l'option pour de bon. La façon dont le moteur reprend ces touches est de se mettre devant chaque frappe de tout l'ordinateur et d'avaler **Alt et Ctrl en entier**, avant que quiconque les voie. Tous les raccourcis du produit sont des combinaisons Alt : tant que le moteur tenait ces touches, aucun ne fonctionnait, et on ne pouvait plus sortir du plein écran au clavier.

**Ce qui a fait durer ça six tours : la reprise posait des questions.** Le système appelle ce genre de reprise sur le fil qui l'a demandée, **chaque frappe de tout l'ordinateur attend cet appel**, et une réponse qui tarde plus d'un tiers de seconde fait remettre la touche au système comme s'il n'y avait eu aucune reprise. Or la reprise demandait, à chaque touche, où était le premier plan et si l'image existait encore. Ces questions-là vont au gestionnaire de fenêtres, et **un autre fil du même programme peut le tenir occupé une demi-seconde** pendant qu'il déplace des fenêtres.

Le journal l'a fini par le montrer à la seconde près : `retour en fenêtre rendu au système : en fenêtre en 489 ms`, et à cette même seconde le premier plan qui part au sélecteur de Windows et un relâchement de Tab arrivant sans l'appui qui allait avec. Basculer plein écran, redimensionner, poser le bouton flottant : chacun de ces gestes prend environ une demi-seconde de gestionnaire de fenêtres, et le bouton flottant en déclenche. D'où « ça marche, je touche au fab, ça ne marche plus », dit six fois et vrai six fois.

**Plus aucune question n'est posée depuis là.** Où est le premier plan est calculé ailleurs, sur des fils qui peuvent attendre, et laissé sous forme de nombre. La fenêtre de l'image est lue comme un nombre elle aussi, sans demander si c'est encore une fenêtre : un nombre périmé coûte un message envoyé dans le vide, que le système refuse et qui ne coûte rien. Et la reprise a un fil à elle, qui ne fait rien d'autre que lire ses messages. *La manière dont ce nombre est tenu à jour a changé le même jour : voir [D33](#d33-le-premier-plan-est-suivi-et-non-plus-sondé-2026-08-23-pendant-m4).*

**Une condition de trop est tombée avec.** Il était demandé, en plus, que le clavier soit **à l'image**. C'est hors sujet : ce qui est repris est déposé à la fenêtre de l'image par son nom, ce qu'aucun focus ne décide. Restent deux conditions, et elles se lisent sans rien demander à personne.

**Pourquoi ce n'est pas une fonctionnalité mise dans un moteur.** Elle n'y est pas mise : rien n'est ajouté au moteur, et rien de nouveau ne lui est demandé. Ce qu'il reçoit est une frappe ordinaire à sa propre fenêtre, qu'il transmet comme il transmet tout le reste. La fenêtre que le système appelle celle du premier plan est la nôtre, donc le programme qui peut reprendre ces touches est le nôtre, et il se contente de les passer.

**Ce qui garde ça sûr.** Deux conditions, toutes les deux exigées, et lues à chaque touche : une image est à l'écran, et le premier plan appartient à cette session. Faute d'une seule, la touche part au système comme d'habitude. Tab et Échap seules ne sont jamais reprises : ce sont des touches ordinaires, et seule la compagnie qu'elles gardent en fait des touches du système. Une touche reprise à l'appui l'est aussi au relâchement, quoi qu'il arrive entre les deux.

**Ce que ça coûte.** Chaque touche de cet ordinateur passe par un test de ZyrDesk tant qu'une session est à l'écran. Le test est court par construction, et rien ne s'écrit dans le journal depuis là : le journal est un verrou et un fichier vidé sur le disque, et un système qui trouve cette route lente décroche le tout sans prévenir. Ce qui est repris est compté, et la surveillance de session l'écrit une seconde plus tard.

**Ce qui reste imparfait.** Menu du bouton flottant ouvert, le clavier est à ce menu, donc l'ordinateur distant ne sait pas qu'Alt est enfoncée et reçoit un Tab seul plutôt qu'un Alt+Tab. C'est une seconde ou deux par ouverture de menu, et c'est préférable au défaut qu'on vient de fermer.

**Ce qui va avec : plus aucune touche coincée.** Si une modificatrice est enfoncée et que le premier plan part ailleurs avant qu'elle ne remonte, l'ordinateur distant ne voit jamais le relâchement et croit la touche tenue pour toujours ; tout ce qui est tapé ensuite y arrive en Alt et une lettre, ce qui ne fait rien et ressemble trait pour trait à un clavier mort. Dit par Victor : « j'ai même carrément perdu le clavier dans la session », puis, un tour plus tard, « ce bug tu me le ramène toujours ». ZyrDesk relâche donc là-bas, à chaque tour de la surveillance de session, toutes les modificatrices qu'aucun doigt ne tient, lues sur le clavier physique et nulle part ailleurs.

**Sans condition, et c'est là que le premier essai a échoué.** Ce relâchement n'était demandé que lorsque le clavier revenait à l'image après en être parti, et il n'a jamais eu lieu une seule fois : ce qui abandonne une modificatrice, c'est le premier plan qui s'en va, et le clavier n'est pas obligé de le suivre. ZyrDesk gardait donc le clavier et répondait « oui, toujours là » pendant qu'Alt restait coincée au loin. Rien n'est envoyé pour une touche qu'un doigt tient réellement, donc demander à chaque tour ne coûte rien, et une seule modificatrice touchée depuis le dernier passage suffit à déclencher le suivant.

**Et l'état d'Alt et Ctrl est compté sur le flux lui-même**, plus demandé au système au moment où une touche arrive. Cette question-là, posée de l'intérieur du traitement d'une autre touche par le système, sur une touche qu'il n'a pas fini de traiter, n'est pas une chose sur laquelle asseoir une fonctionnalité : une Alt+Tab sur quatre était lue comme un Tab tout seul et laissée passer, et Windows changeait de fenêtre sur cet ordinateur-ci. Le flux est seul juge de ce que le flux transporte. Il est amorcé au démarrage de la session par une lecture du clavier physique, pour le cas d'une session ouverte un doigt déjà sur Alt.

**Ce qui reste ouvert : la touche Windows.** Elle n'est pas reprise, et c'est la seule que ce chemin ne peut pas servir. Le moteur refuse de la transmettre à l'ordinateur distant tant que sa propre capture des touches du système ne tourne pas, ce qui dans ce produit n'arrive jamais. La reprendre ici n'ouvrirait donc de menu nulle part, ni là-bas ni ici, ce qui est pire que de la laisser tranquille : laissée tranquille, elle fait ce qu'elle a toujours fait sur cet ordinateur. Ctrl+Maj+Échap, le gestionnaire des tâches, part au loin comme les autres ; si ça gêne à l'usage, c'est une ligne à retirer de la liste.

## D33. Le premier plan est suivi, et non plus sondé (2026-08-23, pendant M4)

**Décision.** Windows dit à ZyrDesk où passe le premier plan, à l'instant où il le déplace, et ZyrDesk n'a plus à le lui demander. Le nombre que lit la reprise des touches système est écrit là, sur un fil qui ne fait que ça, pour toute la durée d'une session. Et refermer le menu du bouton flottant rend à la session le premier plan en plus du clavier.

**Ce qui l'a rendu nécessaire.** Dit par Victor : « Y'a rien à faire dès que j'ouvre ce putain de fab je perd le alt tab il repasse sur client only ». Son journal, cette fois, donne les deux moitiés de la réponse.

**Première moitié : un nombre vieux d'une seconde.** [D32](#d32-les-touches-que-windows-garde-pour-lui-sont-reprises-par-zyrdesk-pas-par-le-moteur-2026-08-23-pendant-m4) a sorti la question du chemin d'une frappe, ce qui était nécessaire ; mais la réponse n'était alors recalculée qu'à chaque redessin de la barre de titre et à chaque tour de la surveillance de session, donc au plus une fois par seconde. Or le sélecteur de fenêtres de Windows prend le premier plan et le rend en bien moins que ça. Le journal montre les deux à la suite : `barre de titre active : le premier plan est à ZyrDesk` à 18:23:40, puis la touche suivante refusée pour `premier plan ailleurs` à 18:23:41. Un seul refus suffit à entretenir le suivant, puisque la touche laissée passer rouvre ce sélecteur. D'où « ça ne marche plus **du tout** » à partir de la première fois.

À quoi s'ajoute que la question posée pouvait répondre « aucune fenêtre » : le système n'a pas de premier plan pendant l'instant où il le passe d'une fenêtre à l'autre, et une session lue exactement là était lue comme ayant perdu un premier plan qu'elle n'avait jamais quitté. Suivi plutôt que demandé, le premier plan est toujours nommé : c'est le système qui donne la fenêtre.

**Seconde moitié : le bouton flottant faisait vraiment partir le premier plan.** Cliquer sur ce bouton donne le focus à sa page ; donner le focus à une fenêtre active celle-ci ou celle dont elle dépend, et cette fenêtre-là est marquée pour ne jamais être activée. Ce qui sort de la demande est donc notre propre fenêtre qui perd le premier plan sans que rien ne l'ait pris, et il tombe sur ce qu'il y a derrière : le bureau de Windows quand la session est en fenêtre. Le journal le montre une seconde après la fermeture du menu, `le premier plan est ailleurs : processus 34640 (explorer.exe)`, avant le moindre Alt+Tab. Refermer le menu redemande donc le premier plan pour la fenêtre d'accueil, et le journal dit ce que Windows en a fait.

**Ce qui garde ça sûr.** Cette demande n'est faite que là, que pendant une session, et que si le premier plan a réellement quitté ZyrDesk et l'image. Elle défait ce que ce programme s'est fait à lui-même ; elle n'arrache rien à personne. Qui est parti vers un autre programme pendant que le menu était ouvert reste où il est allé.

**Ce que ça coûte.** Un second fil pendant une session, qui ne fait que lire ses messages, et une ligne de journal par déplacement du premier plan, laquelle nomme la fenêtre. Ce suivi n'est pas posé sur la route d'une frappe : il est raconté après coup, donc y répondre lentement ne retarde rien de l'ordinateur. Si Windows refuse de le poser, le journal le dit, et le premier plan cesse d'être suivi sans que rien d'autre ne change.

**Ce qui a été retiré avec.** Le calcul du premier plan à chaque redessin de barre de titre et à chaque tour de la surveillance de session. Une seule source, et la lecture unique faite au démarrage d'une session pour savoir d'où l'on part.

## D34. La touche perdue ne l'est pas par ZyrDesk : le crochet est mesuré (2026-08-23, pendant M4)

**Ce que le journal a prouvé, et ça change la nature du défaut.** Après [D33](#d33-le-premier-plan-est-suivi-et-non-plus-sondé-2026-08-23-pendant-m4), une session a été relue frappe par frappe. Entre deux lectures : `11 frappe(s) vues` contre `8` avant, donc **trois** touches, et **une seule** candidate, un relâchement de Tab. Un Alt+Tab en fait quatre : Alt enfoncée, Tab enfoncée, Tab relâchée, Alt relâchée. Les trois vues sont Alt enfoncée, Alt relâchée et Tab relâchée : **l'appui de Tab n'est jamais arrivé jusqu'à ZyrDesk**. Le compteur du programme monte au premier geste du crochet, donc un crochet appelé et lent compterait quand même ; celui-ci n'a pas été appelé du tout.

Toutes les causes des tours précédents étaient des conditions que ZyrDesk évaluait mal. Celle-ci n'en est pas une : ZyrDesk a répondu juste à tout ce qu'on lui a donné, et une touche ne lui a pas été donnée. Tout ce qui suit dans ce journal en découle et est correct : le sélecteur de Windows ouvert, le premier plan parti, donc les touches suivantes rendues au système, ce qui est exactement ce qu'on veut quand la session n'est plus devant.

**Trois raisons possibles, et rien dans le code ne permet de trancher entre elles.** Le système a pu passer outre le crochet parce qu'il a jugé ce programme trop lent à répondre (il tient chaque frappe de tout l'ordinateur et la rend telle quelle passé un tiers de seconde) ; ou un autre crochet posé après le nôtre, donc appelé avant, a mangé la touche ; ou l'appel a eu lieu sous une forme que rien ne comptait. Deviner laquelle serait recommencer les six tours précédents.

**Donc le crochet se mesure lui-même, sur la route même des frappes, et sans rien y ajouter qui attende.** Quatre nombres, tous des lectures de mémoire ou d'horloge, aucun appel au gestionnaire de fenêtres :

- **Le flux contre lui-même** : combien de Tab enfoncées et combien de relâchées, combien d'Alt de chaque côté. Une session ne peut pas tenir deux relâchements de Tab pour un appui, et c'est la seule façon de voir une frappe qui n'est jamais venue.
- **L'âge d'une frappe en arrivant.** Windows horodate chaque frappe ; comparé à l'heure d'arrivée, l'écart est ce que le système et tout crochet posé devant nous ont consommé avant nous.
- **Le temps passé chez nous**, en microsecondes, mesuré sur toutes les sorties de la fonction. C'est le nombre qui doit rester petit, et le seul dont ce programme réponde.
- **Les appels qui ne parlaient pas d'une touche**, que rien ne comptait jusqu'ici et qui seraient donc restés invisibles.

Grand devant, petit chez nous : l'attente n'est pas la nôtre. Grand chez nous : elle l'est. Les deux petits avec un appui manquant : personne n'a attendu et la touche a été prise ailleurs.

**Une réponse de plus, distincte, et elles se ressemblaient.** Un relâchement dont l'appui n'a pas été repris était compté d'une seule façon. Il y en a deux, de sens opposé : la session n'était pas devant et l'appui a été laissé passer exprès, ce qui est normal ; ou elle l'était, donc l'appui aurait été porté au loin s'il était venu, donc il n'est pas venu. La seconde est le défaut ci-dessus, et elle a maintenant son propre compte.

**Et le premier plan est noté pour chaque candidate**, relâchements compris. Il n'était noté qu'aux appuis, donc le journal collait à un relâchement la réponse d'un appui parfois vieux de plusieurs minutes.

**Une seule chose a changé de comportement : le fil du crochet passe au-dessus des fils ordinaires.** C'est le seul fil du produit qui ait une échéance réelle, il ne tourne jamais et ne fait que se réveiller pour répondre, donc rien sur la machine ne perd quoi que ce soit à ce qu'il passe en premier. Ce n'est pas un correctif à l'aveugle : c'est retirer un risque connu d'un fil qui n'a pas le droit de répondre en retard, sur une machine, un portable qui décode de la vidéo sur tous ses coeurs, où un fil ordinaire peut attendre bien plus longtemps qu'une frappe n'a le droit d'attendre.

## D35. Le crochet des touches est reposé en refermant le menu flottant (2026-08-23, pendant M4)

**Ce que les mesures de [D34](#d34-la-touche-perdue-ne-lest-pas-par-zyrdesk--le-crochet-est-mesuré-2026-08-23-pendant-m4) ont répondu, et elles ne laissent qu'une possibilité.** La session mesurée dit, à la ligne près : `au plus 0 ms d'attente avant nous et 53 µs chez nous, 0 appel(s) hors sujet`. Personne n'a attendu avant nous, ZyrDesk répond en cinquante microsecondes contre une limite de trois cent mille, et aucun appel n'est venu sous une forme non comptée. Les trois causes envisagées, il en reste une.

**Le flux contre lui-même dit le reste.** Dix Alt+Tab portées d'affilée, tout équilibré : `Tab 10 enfoncée(s) et 10 relâchée(s), Alt 4 et 4`. Le menu du bouton flottant est ouvert et refermé, une fois. Alt+Tab suivant : **une** frappe arrive sur les quatre, `Tab 10 enfoncée(s) et 11 relâchée(s), Alt 4 et 4`. L'appui d'Alt, l'appui de Tab et le relâchement d'Alt n'ont jamais été apportés. Tout le reste de la session s'ensuit et est correct.

**Donc quelque chose s'est posé devant nous sur cette route.** Windows appelle ces crochets du plus récent au plus ancien : un crochet posé après le nôtre voit chaque frappe avant nous et peut la garder, auquel cas nous ne sommes pas appelés du tout. Ce qui se pose là entre les deux moments, c'est une vue web à nous qui devient la fenêtre active pour la première fois de la session, ce qui est la seule chose qui s'y produise et exactement ce qu'est la première utilisation de ce menu. Le journal montre le premier plan rebondir quatre fois entre nos propres fenêtres à cette seconde-là.

**Windows n'offre aucun moyen de rester le premier.** Reposer le crochet est la façon dont ça se fait, et c'est ce que font les outils de clavier depuis toujours.

**Et ça marche : le journal le montre.** Sur la première version, juste après `1 reprise(s) du crochet`, treize Alt+Tab portées en cinq secondes sur une session qui n'en portait plus aucune. Puis ça se défait de nouveau quelques secondes plus tard, **sans que rien de chez nous ne soit touché entre-temps**. C'est ce qui fait que c'est une chose à refaire et pas une chose à faire une fois : ce qui se pose devant nous n'est pas lié au menu du bouton flottant, qui n'est qu'une des occasions.

**Donc c'est redemandé à chaque moment où quelque chose a pu se poser devant nous** : en refermant le menu du bouton flottant, et quand le premier plan revient à la session après être parti chez un autre programme. Pas à chaque tour de surveillance, pas sur minuterie, pas à chaque frappe.

**La première version a été reprise le jour même, et c'est important.** Elle démontait le fil du crochet et en construisait un autre, depuis le fil qui dessine, lequel attendait alors que l'ancien ait fini ; et pendant ce temps chaque frappe de tout l'ordinateur attendait ce fil-là. Dit par Victor : « ça m'a carrément bloqué le alt tab sur mon propre pc a un moment c'est encore pire ». C'était vrai et c'était pire. Un crochet appartient à son fil et ne peut être retiré que là : le fil est donc **prévenu** par un message posé, qui n'attend rien, et il repose son propre crochet entre deux de ses messages. Rien ne démonte rien, et rien n'attend nulle part.

**Ce que ça ne touche pas.** Ce qui est tenu enfoncé pour le compte de l'ordinateur distant reste tenu : c'est le même crochet, sur le même fil, pour la même session.

**Ce que le journal en dit.** `crochet posé N fois` compte les poses réelles, faites sur le fil, la première comprise. À lire à côté de `Tab X enfoncée(s) et Y relâchée(s)` : égaux, les frappes arrivent ; dépareillés, quelque chose les prend avant nous.

**~~Décision~~ reprise le 2026-08-23 par [D37](#d37-alt-se-lit-dans-le-nom-que-windows-donne-à-la-frappe-2026-08-23-pendant-m4) : la reposée est retirée en entier.** Elle reposait sur une hypothèse jamais prouvée, et elle perd elle-même des frappes : entre le retrait et la pose il n'y a aucun crochet, et plus largement le fil n'est plus dans l'attente de ses messages, donc la frappe qui tombe là est perdue sans trace, tous les compteurs vivant à l'intérieur de la fonction qui n'est pas appelée. Fermer le menu la déclenchait **deux fois** (une fois directement, une fois parce que reprendre le premier plan déclenche la seconde demande), d'où `crochet posé 3 fois` et exactement les deux appuis manquants de cette session-là. Ce que la reposée prétendait corriger l'est autrement et à la source.

## D36. Un premier plan perdu moins d'une demi-seconde n'est pas perdu (2026-08-23, pendant M4)

**Ce que les mesures de [D34](#d34-la-touche-perdue-ne-lest-pas-par-zyrdesk--le-crochet-est-mesuré-2026-08-23-pendant-m4) et la reprise de [D35](#d35-le-crochet-des-touches-est-reposé-en-refermant-le-menu-flottant-2026-08-23-pendant-m4) ont réglé, et ce qu'il restait.** Après la reprise du crochet, les frappes arrivent : le journal montre `Tab 7 enfoncée(s) et 7 relâchée(s)`, équilibré, ce qui n'était jamais le cas avant. Le problème « la frappe n'arrive pas » est clos. Restait un dernier défaut, d'une nature encore différente.

**Ce que le journal montre.** En refermant le menu du bouton flottant, le premier plan fait un aller-retour de quelques dixièmes de seconde : `le premier plan passe à ZyrDesk`, puis `passe ailleurs : explorer.exe`, puis revient. Le bouton est une fenêtre à nous, et l'utiliser fait rebondir le premier plan sur le shell de Windows un instant avant qu'il ne revienne. Un Alt+Tab qui tombe pile dans cet instant était jugé « premier plan ailleurs », donc laissé passer au système, où il ouvrait le sélecteur de tâches de cet ordinateur ; la fenêtre du sélecteur tenait alors le premier plan pour de vrai, et tenait dehors tous les Alt+Tab suivants. Un seul dixième de seconde mal tombé lançait toute la cascade. Le journal la nomme à la lettre : `Changement de tâche`, le titre de ce sélecteur.

**Le correctif.** Un premier plan parti depuis moins d'une demi-seconde n'est pas un premier plan perdu. Tant que ce court délai n'est pas écoulé, la session est tenue « au premier plan », l'Alt+Tab est porté, le sélecteur ne s'ouvre pas, et la cascade ne démarre jamais. Un vrai départ, en cliquant une fenêtre de cet ordinateur, reste dehors au-delà du délai et rouvre la porte comme il se doit. Le délai part de l'instant du premier départ, pas du dernier saut entre deux fenêtres tierces.

**Ce que ça coûte.** Après avoir cliqué une fenêtre locale, l'Alt+Tab part encore vers l'ordinateur distant pendant une demi-seconde. C'est court, et c'est le prix pour que le bouton flottant cesse de tout casser.

**Ce que le journal en dit.** `N portée(s) sauvée(s) par le délai de grâce` compte les frappes portées à la session alors que le premier plan brut était ailleurs. Non nul après un passage par le bouton, avec les Alt+Tab qui continuent d'être portés, la cause est bien celle-ci et le correctif tient.

## D37. Alt se lit dans le nom que Windows donne à la frappe (2026-08-23, pendant M4)

**Le défaut, prouvé par le journal et par une relecture complète du code.** `1 que le système n'aurait pas mangées`, à côté de `Alt 2 enfoncée(s) et 3 relâchée(s)`. Un appui d'Alt n'est jamais arrivé jusqu'à ZyrDesk. Or ZyrDesk ne connaissait l'état d'Alt qu'en le comptant sur les frappes qu'il reçoit lui-même : un appui manquant, et il croit qu'aucun doigt n'est sur Alt. Le Tab suivant est alors jugé un Tab ordinaire et laissé au système, qui ouvre le sélecteur de cet ordinateur, lequel prend le premier plan, et plus rien ne repart. **Un état mémorisé qui ne peut jamais se corriger : tant que le doigt reste sur Alt, les seules frappes qui pourraient le remettre d'aplomb sont justement celles qui sont mal jugées.**

**Le correctif : ne plus mémoriser ce que le système dit déjà.** Une touche frappée avec Alt tenue n'est pas une frappe ordinaire pour Windows, c'en est une « système », et il le dit dans le nom du message qu'il nous tend, pour cette frappe et aucune autre. C'est gratuit, ça ne peut pas vieillir, et surtout **ça ne peut pas se perdre** : ça vient avec la frappe au lieu d'être retenu d'une frappe précédente. Le flux reste à côté, pour Control, dont aucun nom de message ne parle.

**Ce qui a été écarté, et pourquoi.** Lire Alt sur le clavier physique au moment où le Tab arrive, ce qui semblait l'évidence : ça remet un appel au gestionnaire de fenêtres sur la route à échéance, exactement l'invariant payé par cinq rondes (voir [D32](#d32-les-touches-que-windows-garde-pour-lui-sont-reprises-par-zyrdesk-pas-par-le-moteur-2026-08-23-pendant-m4)), et un appel qui dépasse le délai fait **retirer le crochet en silence et définitivement** sur Windows 7 et suivants. Le remède aurait pu causer le mal, en permanent. Microsoft documente d'ailleurs que l'état asynchrone d'une touche n'est pas à jour pendant qu'on traite cette touche.

**Trois autres défauts trouvés au passage, tous de la même famille : un état mémorisé qui dérive.**

- **Les frappes envoyées par un programme ne mènent plus l'état d'Alt.** Chaque raccourci que le menu flottant envoie est Ctrl, Alt, Maj, la lettre, puis les trois relâchés ; ces relâchements revenaient par notre propre crochet et posaient Alt à zéro **pendant qu'un doigt la tenait**. Seules les frappes d'un doigt comptent maintenant, et le décompte du journal ne compte plus qu'elles non plus, ce qui rend l'équilibre `Tab X et Y` réellement lisible.
- **L'état est semé sur le fil du crochet et non des millisecondes avant qu'il existe.** Il était lu sur le fil appelant, avant même que le fil du crochet soit démarré : un Alt enfoncé dans cet intervalle était invisible tant que le doigt restait dessus.
- **« L'appui n'est jamais arrivé » veut enfin dire ça.** Cette réponse confondait deux choses opposées : l'appui est venu et a été laissé passer exprès, ce qui est normal, et l'appui n'est jamais venu, ce qui est le vrai défaut. C'est ce qui a rendu un journal ambigu. Les deux sont désormais comptées à part.

**Ce que le moteur n'est pas.** Vérifié en remontant jusqu'au commit exact du SDL livré : ni le moteur client ni SDL ne posent de crochet clavier quand la capture des touches système est désactivée, ce qui est notre cas. La piste « le moteur passe devant nous » est close.

## D38. L'analyse statique suit la dernière version de Rust, et c'est elle qui allume le rouge (2026-08-23, pendant M4)

**Constat.** L'intégration continue était rouge sur chaque commit alors que le produit se compilait partout sans une erreur. Sur les quatre travaux de la CI, trois passaient : tests Windows, tests Linux, et construction de l'installateur Windows. Seul « Format et analyse statique » échouait, et seulement à l'étape clippy, sur **une** règle : `chunks_exact_mut` appelé avec une taille constante, à `crates/zyr-ui/src/tray.rs`.

**La cause n'est pas dans le code, elle est dans l'écart de versions.** La CI installe la dernière version stable de Rust à chaque exécution. Cette règle-là est apparue dans une version postérieure à celle des machines de développement : localement elle n'existe pas, donc rien ne pouvait la voir avant de pousser. Écart mesuré ce jour-là : 1.94.1 en local contre 1.98.0 sur la CI.

**Corrigé à la source**, en disant quatre comme une taille et non comme un nombre : pris comme un nombre, les tranches reviennent une par une et rien ne promet qu'elles font quatre octets, donc chaque lecture doit répondre d'une longueur qui ne peut pas se produire. Vérifié en installant la version exacte de la CI : plus une seule remarque sur tout l'espace de travail.

**Ce que ça coûte de laisser la CI suivre la dernière version.** Chaque sortie de Rust peut allumer le rouge sur un commit qui n'y est pour rien. C'est le prix d'apprendre les nouvelles règles tôt, et il est payable tant que quelqu'un regarde la lampe. Ce qui a manqué ici n'est pas la politique de version : c'est que personne ne regardait. Figer la version dans `rust-toolchain.toml` supprimerait la surprise et supprimerait aussi les règles nouvelles jusqu'à une montée décidée ; à trancher si le rouge fortuit revient.

## D39. Le premier plan tombé est repris quand il tombe, et non un instant avant (2026-08-24, pendant M4)

**Constat.** Alt+Tab part vers l'ordinateur d'en face tant que la fenêtre est petite, et cesse d'y aller dès qu'on l'agrandit ou qu'on ouvre le menu du bouton flottant. À partir de là, c'est le sélecteur de fenêtres de cet ordinateur-ci qui s'ouvre, et la session reste sourde.

**Ce que le journal établit.** Le crochet clavier compte chaque frappe que Windows lui présente. Entre deux relevés encadrant un Alt+Tab, il en a vu une seule, un Tab relâché, sans l'appui du Tab ni celui du Alt. Les appuis ne sont donc pas refusés par ce programme : ils ne lui sont pas présentés. Le crochet répond en microsecondes quand il est appelé, n'attend derrière personne, et aucune frappe injectée n'est en cause ; il est appelé de nouveau la seconde suivante, donc il n'a pas été retiré.

**Le défaut qui est corrigé.** À chacun des deux moments, le premier plan tombe sur le bureau, personne ne l'ayant pris. Le code connaissait déjà cette chute et avait une réparation pour elle, mais celle-ci s'exécutait à la fermeture du menu, c'est-à-dire avant la chute : elle lisait un premier plan encore à nous et rentrait chez elle. Sa ligne est absente du journal de chaque session fautive, et la chute y arrive une ligne plus loin. La réparation est désormais armée à la fermeture du menu et dépensée par la veille qui apprend le déplacement du premier plan à l'instant où il a lieu ; elle rencontre la chute au lieu de lui courir après. Ce que ce programme s'est fait à lui-même est défait ; un premier plan que quelqu'un est réellement allé chercher ailleurs n'est pas repris, rien n'ayant été annoncé avant lui.

**Ce que l'essai suivant a montré, et la reprise a été retirée.** La réparation s'est déclenchée, et sa ligne a dit « Windows a refusé », sur chaque essai. Windows n'accorde le premier plan qu'au programme qui le tient déjà ou qui a reçu la dernière frappe, et quand la chute est suivie d'un Alt+Tab, c'est le shell qui a eu les deux. Cette route est fermée, et tout ce qui avait été ajouté pour elle a été retiré. Ce que la chute coûtait est traité à sa racine en D42 : le shell au premier plan n'est plus quelqu'un d'autre, donc une chute ne coûte plus rien aux touches.

**Ce qui reste ouvert.** Pourquoi certains appuis ne sont pas présentés au crochet n'est pas établi. Deux pistes tiennent devant les faits, sans qu'aucune soit prouvée : une fenêtre élevée au premier plan, à qui Windows interdit à un programme ordinaire de voir les touches destinées, et le sélecteur de fenêtres de Windows lui-même, qui prend le clavier entre l'appui et le relâchement de la touche qui l'ouvre. Le journal de l'essai fautif montre un « Administrator: PowerShell » au premier plan aux deux secondes exactes où des appuis manquent, ce qui suffit à contaminer l'essai sans suffire à conclure. Le prochain essai se fait sans aucune fenêtre administrateur ouverte.

## D40. Alt se lit de trois côtés, et un appui jamais reçu ne coûte plus la session (2026-08-24, pendant M4)

**Le défaut, pris sur le fait.** Une ligne du journal le montre entière :

```
Tab 1 enfoncée(s) et 0 relâchée(s), Alt 0 et 0 ; 1 que le système n'aurait pas mangées ;
la dernière était Tab enfoncée, Alt non, premier plan à l'image
```

Un Tab arrive, le premier plan est bon, et il est pourtant laissé passer parce que ce programme croit qu'aucun doigt n'est sur Alt. Le sélecteur de fenêtres de cet ordinateur s'ouvre dessus, ce qui prouve que le système, lui, avait bien vu l'Alt. À partir de là tout s'enchaîne : le sélecteur prend le premier plan, et chaque Alt+Tab suivant est refusé faute de session devant.

**Pourquoi les deux sources d'alors ne pouvaient pas le voir.** Alt était lu du nom que le système donne à la frappe, et du flux de touches que ce programme suit. Le nom ne vaut que pour Alt et n'est donné qu'aux frappes que le système appelle siennes ; le flux, lui, ne peut compter que ce qui lui est présenté, et le même relevé montre des relâchements d'Alt sans les appuis correspondants, donc des appuis qui ne sont jamais arrivés jusqu'ici. Un appui manquant laisse le flux persuadé que rien n'est tenu, et le Tab d'après est jugé ordinaire.

**Corrigé en demandant au clavier.** Une troisième source est jointe aux deux autres : l'état réel des doigts, lu de la table que le système tient en mémoire. Ce n'est pas une question posée au gestionnaire de fenêtres, donc elle peut être posée depuis la route que chaque frappe emprunte, et c'est la lecture sur laquelle le rattrapage des modificateurs restés en l'air repose déjà. Les trois ne peuvent que manquer un modificateur, jamais en inventer un, donc les joindre ne peut que réparer. Le compte des touches portées grâce aux doigts seuls est écrit dans le journal, à côté de celui du délai de grâce, pour que cette décision se juge sur un nombre.

**Conséquence tenue.** Ce qui est annoncé au moteur suit la même règle : un Tab porté pour un Alt+Tab et remis comme un Tab nu ne bougerait rien au loin, ce qui de la main se voit comme une session qui avale la touche.

## D41. Le moteur client ne refuse pas Alt+Tab, et la remise elle-même n'était vérifiée nulle part (2026-08-24, pendant M4)

**Ce que la lecture des moteurs a écarté.** Le soupçon portait sur le moteur client, qui a sa propre capture des touches du système et la croit inactive tant que sa fenêtre n'est pas celle du premier plan, ce qu'elle ne peut pas être puisqu'elle est portée dans la nôtre. Le soupçon est levé : dans `app/streaming/input/keyboard.cpp`, cette condition ne garde que la touche Windows et le modificateur Meta. Tab n'est gardé par rien, et une fois la frappe reçue elle est envoyée à l'ordinateur d'en face sans autre condition. La bibliothèque d'affichage, lue à la version que le moteur embarque, transforme un message posté en frappe exactement comme une frappe réelle, sans regarder qui a le focus. Le moteur n'avale donc pas Alt+Tab.

**Ce que cette lecture a mis au jour.** Il restait un maillon sans preuve, et c'est le nôtre. La touche est prise à cet ordinateur, donc le système n'agira jamais dessus ; elle est ensuite postée à la fenêtre du moteur, et **la réponse du système à cette remise était jetée**. Une remise refusée, ou une fenêtre disparue, fait une frappe évanouie des deux ordinateurs à la fois sans que rien nulle part ne le dise. Le journal comptait la touche comme « portée » et s'arrêtait là, ce qui se lit à tort comme un travail fait.

**Corrigé.** La réponse est lue, et deux nombres sont écrits dans le journal : les remises refusées par le moteur, et celles qui n'avaient aucune fenêtre où aller. Zéro est la réponse attendue pour les deux ; toute autre valeur raconte à elle seule ce qui manquait.

## D42. Le shell de Windows n'est pas quelqu'un d'autre (2026-08-24, pendant M4)

**La boucle, et pourquoi aucune des corrections précédentes ne pouvait en sortir.** Un relevé d'essai la montre entière :

```
10 premier plan ailleurs, 10 relâchements de touches laissées passer, 6 portées à la session
```

Dix Alt+Tab laissés passer à Windows parce que la session n'était pas au premier plan, contre six touches portées en tout sur toute la session. Chacun de ces dix ouvre le sélecteur de fenêtres de cet ordinateur ; ce sélecteur prend le premier plan ; le premier plan tenu par un tiers fait laisser passer le suivant, qui le rouvre. La boucle s'entretient et rien n'en sort, sauf un clic dans l'image.

**Ce que le garde-fou disait, et où il se trompait.** La règle était : les touches du système appartiennent à l'ordinateur d'en face tant que la session est au premier plan. Elle est juste. Ce qui était faux est la liste des façons de ne pas y être. Le premier plan quitte la session pour trois sortes de fenêtres, et deux d'entre elles appartiennent au shell de Windows : son bureau, où le premier plan tombe quand plus rien ne le tient, et son sélecteur de fenêtres, qui n'existe que parce qu'on a laissé passer la touche dont il est question. Personne ne bascule vers le shell. Le compter comme un tiers, c'est traiter la conséquence du défaut comme sa justification.

**Corrigé en nommant le shell.** Le premier plan tenu par le processus du shell, reconnu par la fenêtre où il garde le bureau, n'est plus « ailleurs » : la session garde ses touches à travers tout le battement. Quelqu'un qui part vraiment part vers un programme qui a un nom, un navigateur ou un terminal, et ceux-là restent « ailleurs » et rendent les touches comme ils doivent. Le journal les nomme séparément, sans quoi cette décision ne se vérifierait pas.

**Ce que la lecture de la référence a confirmé.** La bibliothèque d'affichage du moteur client fait la même capture pour son propre compte, et son crochet avale Alt, Control, la touche Windows, Tab et Échap d'un bloc, sans jamais demander où est le premier plan. C'est la même idée dite autrement : tant que la session tient le clavier, ces touches ne sont pas à cet ordinateur, et un premier plan qui bat n'y change rien. Ce produit ne peut pas avaler Alt, ses propres raccourcis passant par l'enregistrement de combinaisons du système, qui ne voit jamais une touche avalée ([D32](DECISIONS.md)) ; nommer le shell obtient le même résultat sans y toucher.

## D43. Les touches du système ont un seul propriétaire, et ce peut être le moteur (2026-08-24, pendant M4)

**Le fait qui commande tout.** Le journal établit que certains appuis n'arrivent jamais au crochet clavier de ZyrDesk. Ce n'est pas un refus : la fonction qui décide n'est pas appelée. Le compteur de toutes les frappes vues ne bouge pas, aucune remise n'est refusée par le moteur, aucune fenêtre ne manque, et le crochet répond en dizaines de microsecondes quand il est appelé. Windows ouvre donc son sélecteur avant que ce programme puisse dire quoi que ce soit. Aucune correction portant sur le premier plan, le focus ou un délai ne peut agir sur un événement qui n'arrive pas, et les précédentes traitaient la conséquence. La classification spéciale de l'explorateur ajoutée en D42 est retirée pour cette raison, et pour une seconde : le processus de l'explorateur porte aussi les vraies fenêtres de l'Explorateur et la barre des tâches, donc l'exception rendait à distance des Alt+Tab tapés dans un travail local.

**Ce qu'il fallait changer.** Un crochet bas niveau du clavier est une chaîne partagée, servie du plus récent au plus ancien. Celui de ZyrDesk est posé une fois à l'ouverture d'une session et jamais reposé, par choix ancien et pour de bonnes raisons ; tout ce qui s'installe après lui passe devant. Les moments où la panne apparaît sont exactement ceux où un autre programme en pose un : une fenêtre agrandie, un plein écran, un menu ouvert. La bibliothèque d'affichage du moteur, elle, repose le sien à chaque prise du clavier, et c'est la différence qui compte.

**La voie choisie.** Un seul propriétaire, et dans le programme qui reçoit réellement le clavier. Le moteur client reçoit un mode nouveau, `--capture-system-keys zyrdesk`, qui n'est aucun des trois existants :

- il décide du **focus** de sa propre fenêtre, jamais du premier plan, que cette fenêtre ne peut pas tenir puisqu'elle est portée dans la nôtre. Et il le lit des deux messages que le système envoie à cette fenêtre quand le clavier lui vient et la quitte, pas de ce que la bibliothèque d'affichage en dit : celle-ci répond à la question en comparant sa fenêtre avec celle du premier plan, donc pour une fenêtre portée dans une autre elle signale la première perte et ne peut plus jamais signaler un retour. Le premier essai l'a montré en une ligne, cinq touches portées puis « the session has lost the keyboard » à l'ouverture du menu flottant, et plus rien pendant les vingt secondes suivantes ;
- il ne prend une touche que si **le clavier vient réellement à cette fenêtre**, ce qui demande le focus **et** le premier plan. Les deux sont différents : le produit joint son entrée à celle du moteur et rend le focus à l'image à chaque tour de sa veille, ce qui réussit quel que soit le premier plan, donc le focus seul répond oui pendant que quelqu'un travaille dans un autre programme de cet ordinateur. Un essai l'a montré, dix-sept Alt+Tab tapés dans une autre fenêtre partis à l'ordinateur d'en face. La question est posée d'un coup au système, qui répond quelle fenêtre tient le clavier dans l'entrée à laquelle le premier plan appartient ;
- il **repose son crochet à chaque fois que le clavier lui revient**, donc il redevient le plus récent de la chaîne aux moments précis où la panne se produisait ;
- il n'avale **que Tab et Échap**. Alt, Control, Majuscule et la touche Windows passent intacts, ce qui est la condition pour que les raccourcis de ZyrDesk, tenus par l'enregistrement de combinaisons du système, continuent de fonctionner ; c'est ce qu'avale le mode `always` du moteur qui les avait cassés ([D32](DECISIONS.md)).

La capture propre à la bibliothèque d'affichage reste éteinte dans ce mode, pour la même raison. Ce que le moteur récupère est poussé dans sa file d'événements comme n'importe quelle frappe, donc le chemin qui l'envoie à l'ordinateur d'en face est celui de toutes les autres touches, sans exception à maintenir.

**Les deux voies ne peuvent pas tourner ensemble**, étant le même crochet du système : le choix est un réglage, `system_keys_in_the_engine`, écrit dans le fichier de réglages et porté jusqu'à la ligne de commande du moteur. **Il est à « oui » par défaut.** L'ancienne voie est celle que le journal a prise en défaut, donc la garder par défaut aurait demandé une manipulation à chaque essai pour obtenir le comportement attendu ; c'est la neuve qu'une session ouvre, et le réglage sert à revenir en arrière, pas à avancer. L'ancienne reste entière le temps de juger. Une fois la neuve validée, `crates/zyr-ui/src/keys.rs`, le délai de grâce et le suivi du premier plan qui ne sert qu'à eux s'en vont ensemble. **C'est fait, voir [D47](DECISIONS.md).**

**Conséquence à connaître.** Ce réglage voyage avec le produit, et le moteur qui comprend le mode voyage avec la compilation des moteurs. Un produit reconstruit sans que les moteurs le soient demande donc au moteur un mode qu'il ne connaît pas, et la session ne s'ouvre pas. La routine de mise à jour fait les deux à la suite, et le déplacement du sous-module déclenche la compilation des moteurs, donc les deux avancent ensemble ; il faut seulement attendre qu'elle ait abouti avant de récupérer les moteurs.

**Deux fautes trouvées en chemin et corrigées.** La lecture de l'état des touches depuis l'intérieur du crochet, ajoutée la veille, est retirée : le système n'a pas fini avec la frappe dont il parle à ce moment-là, et le journal l'a confirmé sur une session entière, ce compteur restant à zéro pendant que des appuis manquaient. Et la réponse de la demande de focus était lue comme un échec quand elle valait « personne ne l'avait avant », ce qui est une réponse ordinaire ; ce qui tranche est la lecture prise juste après, comme ailleurs dans le même fichier.

## D44. Le moteur hôte est prié de partir avant d'être pris (2026-08-25, pendant M4)

**Le défaut.** Un ordinateur éteint depuis la session qui le regardait rallume avec l'écran physique resté à la taille et à l'agrandissement du client. ZyrDesk est pourtant bien fermé dessus.

**Ce qui se passait.** Le moteur hôte met le bureau distant à la taille demandée par la session, et le remet comme il l'a trouvé en s'en allant. C'est un travail qu'il fait **en partant**, et seulement là. Le service, lui, l'arrêtait d'un `TerminateProcess`, à l'arrêt du service comme à l'extinction de Windows : le moteur n'exécute alors rien du tout, et l'écran reste à la taille de la personne qui regardait. Le service demandait par ailleurs à Windows l'avis d'extinction ordinaire, qui laisse quelques secondes, au lieu du préavis, qui en laisse cent quatre-vingts.

**Corrigé de la façon dont le moteur attend qu'on le lui demande.** Le service ne le prend plus qu'en dernier recours :

- il lui envoie d'abord l'interruption que porte une console, ce qui est la manière de demander à un programme de s'arrêter sous Windows, et que le moteur écoute déjà : sa réponse est précisément de remettre l'écran puis de s'arrêter ;
- une console appartient à la session où elle a été ouverte, et un service vit dans une session à part, donc il ne peut pas l'atteindre lui-même. Il se relance donc **lui-même dans la session du moteur**, avec un argument réservé, pour porter la demande. C'est exactement ce que fait le service du moteur amont, pour la même raison ;
- il lui laisse vingt secondes, ce que ce même service amont lui laisse, puis le prend s'il n'est pas parti ;
- et il annonce à Windows le préavis d'extinction, avec le temps que l'arrêt demande, afin que ce temps lui soit laissé.

**Vérifiable.** Le journal du service dit désormais lequel des deux s'est produit : parti de lui-même après avoir remis l'écran, ou pris. C'est la première question à poser d'un ordinateur revenu de travers, et rien ne pouvait y répondre avant.

## D45. Un bouton flottant qui n'a jamais rien dessiné n'est pas un bouton (2026-08-25, pendant M4)

**Le défaut.** À une reconnexion, le bouton flottant a entièrement disparu, et le raccourci censé le rappeler n'a rien fait non plus. Le journal de la session, complet du début à la fin, ne dit pas un mot à ce sujet.

**Ce que le relevé établit.** Le premier plan est à l'image pendant toute la session, donc rien n'a caché le bouton pour cette raison. Aucune ligne « menu du bouton flottant ouvert » et aucune ligne « raccourci du menu sans effet » : le raccourci a donc trouvé la fenêtre, l'a montrée et lui a parlé sans que la page réponde. La seule lecture qui tient les deux ensemble est **une fenêtre debout dont la page ne tournait pas** : jamais chargée, ou emportée par les dix-huit heures de veille de l'ordinateur entre les deux sessions.

**Pourquoi rien n'était visible.** La fenêtre n'est montrée que lorsque la page a mesuré ce qu'elle dessine, ce qui est juste : avant, c'est un logo sans logo dedans. Mais rien ne bornait cette attente. Une page muette laissait donc une fenêtre invisible pour toute la session et sourde au raccourci fait pour la ramener, sans trace nulle part. Deux fautes s'ajoutaient à cela : la taille de la dernière fenêtre survivait à sa fenêtre, si bien que la seule ligne qui prouve qu'un bouton a été dessiné manquait sur toutes les sessions sauf la première ; et un refus de construire la fenêtre était dit sur le flux d'erreur, que ce programme, compilé sans console, n'a pas.

**Corrigé en surveillant ce qui n'a pas de recours.** Le bouton est la seule chose à nous posée sur l'image : sans lui, une session n'a plus de sortie que le clavier. La veille, qui passe déjà une fois par seconde, lui laisse trois tours pour dire ce qu'il dessine ; passé cela elle referme sa fenêtre et le tour suivant en remonte une neuve, dont la page repart de zéro. Trois tentatives par session, puis on le laisse tranquille : un bouton qu'on ne sait pas dessiner mérite une ligne de journal, pas une par seconde. La taille est oubliée avec sa fenêtre, et le refus de construire va au journal comme tout le reste.

**Une faute trouvée en chemin.** La taille du logo dont le placement est calculé était restée à cinquante-deux points alors que la page en dessine quarante-quatre depuis qu'il a été réduit : le bouton pendait à dix vrais pixels de son coin pendant toute chaque session.

## D46. La découpe du bouton se pose après le dessin, jamais avant (2026-08-25, pendant M4)

**Le défaut.** Une trace blanche derrière le bouton flottant après avoir cliqué une entrée de son menu, qui disparaissait dès qu'on repassait le curseur sur le logo et revenait au clic suivant.

**Ce que les essais ont établi.** Elle suit le bouton quand on le déplace, donc elle est dans sa fenêtre et non sur l'image. Elle sort avec **Statistiques** et avec **Mode de la souris**, c'est-à-dire les deux entrées qui referment le menu pendant que le curseur est loin du logo. Refermer le menu en recliquant le logo la produit aussi, mais le survol du logo la redessine dans la même image et personne ne la voit jamais.

**La cause.** La fenêtre du bouton est découpée sur ce que la page dessine, et cette découpe était demandée depuis `requestAnimationFrame`, c'est-à-dire **avant** que la page ait dessiné le nouvel état. Windows redessine la fenêtre à chaque découpe, et il la redessine avec son propre fond tant que la page n'a pas repeint par-dessus. Une image d'avance suffit donc à laisser un morceau de ce fond dans la découpe, et il y reste jusqu'au prochain coup de pinceau, qui ne vient jamais si rien ne bouge.

**Corrigé en appliquant la règle qui existait déjà.** Le survol du logo était le seul endroit qui suivait le dessin image par image jusqu'à ce qu'il ne bouge plus, et c'est pour cette raison qu'il n'a jamais montré le défaut. Ce suivi devient la seule façon de retailler la fenêtre : chaque changement d'état le lance, et il s'arrête de lui-même dès que deux images de suite dessinent la même forme. Une découpe est donc toujours posée sur une image que la page a réellement dessinée.

**Et une découpe identique n'est plus posée du tout.** Elle n'était pas gratuite : le système redessine la fenêtre à chaque fois, et la page en demandait une par seconde toute la session pour la barre des mesures, presque toujours de la forme que la fenêtre portait déjà.

**Ça n'a pas suffi, et la deuxième moitié est ailleurs.** La forme appartient à la fenêtre, le dessin appartient à la vue web portée dedans, et pour le système ce sont deux fenêtres et non une. Redessiner la fenêtre extérieure toute seule laisse donc la dernière image de l'intérieure là où la nouvelle forme la laisse passer, c'est-à-dire un morceau de quelque chose qui n'est plus dessiné nulle part. Il y reste jusqu'à ce que la page bouge d'elle-même, et une page dont le menu vient de se refermer sous une main qui est ailleurs ne bouge plus. Chaque découpe redemande maintenant le dessin de la fenêtre **et de tout ce qu'elle porte**, sans faire effacer le fond au passage, puisque ce fond est précisément le blanc dont il s'agit.

**Et le journal sait le dire.** À chaque changement d'état, une ligne donne le nombre de morceaux découpés, jusqu'où ils vont, ce que le système tient réellement comme forme, et la taille de la fenêtre. C'est le seul défaut qu'une capture d'écran ne peut pas montrer : tout l'aspect du bouton est cette forme, donc une forme qui a dérivé du dessin et une page qui dessine autre chose se ressemblent exactement, vues du dehors.

## D47. L'ancienne voie des touches système est retirée, et l'affaire est écrite quelque part (2026-08-25, pendant M4)

**Ce qui est retiré.** La voie que ZyrDesk portait lui-même, gardée le temps de départager les deux ([D43](DECISIONS.md)), et tout ce qui n'existait que pour elle :

- `crates/zyr-ui/src/keys.rs` en entier, 837 lignes ;
- dans `picture.rs`, le délai de grâce et le premier plan gardé en mémoire, qui n'avaient qu'un lecteur : le crochet, qui ne pouvait pas poser la question au système depuis l'endroit d'où il répondait ;
- le rattrapage des modificateurs restés en l'air, qui ne servait que là où ce programme avalait les touches et les repostait ; il n'avale plus rien ;
- le réglage `system_keys_in_the_engine` et sa plomberie, du fichier de réglages du service jusqu'à la ligne de commande du moteur, en passant par le tube et le drapeau `--system-keys-in-zyrdesk`.

Le mode `zyrdesk` est demandé au moteur à chaque session, sans interrupteur. Un interrupteur n'aurait de sens que si les deux réponses étaient défendables ; l'autre a été retirée parce qu'elle ne **pouvait** pas marcher, pas parce qu'on lui préférait celle-ci.

**Ce qui reste, et pourquoi.** Le suivi du premier plan reste, mais il ne décide plus de rien : il n'écrit qu'une ligne de journal nommant le programme qui vient de prendre le premier plan. Une session qui se tait est presque toujours une session devant laquelle quelque chose est passé, et c'est la seule ligne du produit qui dise laquelle.

**Deux trous trouvés en faisant le ménage.** Le correctif moteur qui porte toute cette affaire n'était **inscrit nulle part** : le manifeste des patchs, qui est censé être la source de vérité sur l'écart entre nos moteurs et les leurs, l'ignorait, alors qu'il est épinglé et compilé depuis le 24 août. Il s'appelle désormais P-M10 et il y est décrit en entier. Le compte des patchs de Moonlight était resté à sept alors qu'il en portait neuf, plafond compris, et le dépassement est motivé plutôt que constaté.

**Et l'affaire est racontée en un seul endroit.** [CLAVIER.md](CLAVIER.md) dit le symptôme, la règle de Windows qui commande tout, ce que fait le produit, les trois pièges qui font perdre une semaine, les quatre pistes déjà essayées qui ne peuvent pas marcher, où le code vit, et quoi lire dans les journaux si ça revient. Quinze allers-retours entre deux machines, c'est le prix d'une chose qui n'était écrite nulle part ; elle l'est maintenant.

## D48. Un écran qui appartient à quelqu'un n'est pas à nous (2026-08-25, pendant M4)

> Retirée le jour même par [D50](#d50-zyrdesk-répond-de-lécran-de-lhôte-quand-le-moteur-ny-arrive-pas-2026-08-25-pendant-m4), et sur deux points, pas un.
>
> **La conclusion était fausse.** Elle retirait au produit l'adaptation de la résolution de l'hôte à la session, qui est une des choses qui marchaient le mieux et une de celles que Victor voulait garder : « c'était très bien avant comme il adaptait la résolution et le scaling ». La demande n'a jamais été de ne plus toucher à l'écran, mais de le remettre à coup sûr en sortant. `dd_configuration_option` est revenu à ce qu'il était, et D50 s'occupe du retour.
>
> **Et le mécanisme décrit était mal lu.** Le paragraphe « ce que faisait ZyrDesk » affirme que le moteur rallume tous les écrans quand il échoue et se relance ainsi tout seul. Non : cette phrase est l'étiquette de la liste d'écrans qu'il imprime derrière, il ne rallume rien, et il note au contraire les écrans qu'il vient de voir pour ne plus réessayer tant qu'aucun n'est ajouté ni retiré. La boucle est donc bien réelle, mais elle se joue à deux : l'autre bureau à distance change les écrans, ce qui réveille le moteur, dont la remise change les écrans à son tour, ce qui réveille l'autre. Aucun des deux ne s'entretient seul. C'est pour ça que quitter ZyrDesk l'arrêtait net.
>
> Le dernier paragraphe tombe avec le reste : la taille de chaque écran, ajoutée à la lecture de la liste du moteur pour préparer ce qu'il annonçait, ne servait à rien d'autre et a été retirée avec lui.

**La demande, et la comparaison qui la fonde.** « Quand je quitte la session, l'écran de l'hôte doit se remettre nickel comme avant d'en prendre le contrôle. Parsec y arrive. » Il y arrive parce qu'il ne touche jamais à l'écran de l'ordinateur qu'il montre : il le filme tel quel et met l'image à l'échelle de son côté. Rien à remettre, donc rien qui puisse rater.

**Ce que faisait ZyrDesk, et pourquoi ça finit mal.** Sur une machine sans écran virtuel, nous demandions au moteur hôte de mettre l'écran physique à la taille de la session, et de le remettre après. Remettre est une chose qui peut rater, et elle rate précisément quand quelque chose d'autre a bougé les écrans entre temps : un deuxième bureau à distance, un moniteur qui se réveille, un câble. Ce qui suit est pire que ce qu'on évitait. Le code du moteur, lu ligne à ligne : il échoue à revenir à ce qu'il avait trouvé, **il rallume alors tous les écrans qu'il voit**, ce qui est en soi un changement d'écrans, ce qui est exactement la condition qui le fait réessayer. La boucle s'entretient toute seule et ne s'arrête jamais. Un relevé l'a montrée sur vingt secondes, entre la fin d'une session et l'arrêt du service, la personne entendant sa tour cliquer à travers ses moniteurs.

**Corrigé en ne touchant plus rien.** Une seule question décide, et ce n'est pas celle de la session : cet ordinateur a-t-il un écran à lui à donner ? Un écran que ZyrDesk a fait pousser existe pour prendre la forme qu'une session demande, et les vrais sont éteints le temps de la session puis rendus. Un ordinateur qui n'en a pas n'a que des écrans réels, et un écran réel appartient à qui est assis devant. Sur celui-là, `dd_configuration_option = disabled` et rien d'autre : pas « touché avec précaution », pas « touché puis remis », **pas touché**. Un écran auquel on n'a jamais touché revient en n'étant jamais parti.

**Ce que ça coûte, et ce qui reste à faire.** L'image arrive dans la forme de l'ordinateur d'en face et non dans la nôtre, donc une session qui demande une autre forme reçoit des bandes noires gravées à la source. Le remède n'est pas de déplacer les meubles d'en face : c'est de demander l'image dans **sa** forme à lui. Le moteur hôte ne publie sa résolution nulle part, donc elle doit voyager par le canal entre les deux services, et le lecteur de zyr-screen sait déjà la lire dans la liste d'écrans du moteur. En attendant, choisir une taille de la bonne forme dans le menu de la session suffit à supprimer les bandes.

## D49. L'écran virtuel se pose à chaque démarrage du service, pas seulement à l'inscription (2026-08-25, pendant M4)

**Le constat.** Une machine du banc d'essai n'a pas d'écran virtuel, et son journal le dit à chaque démarrage depuis des semaines : `no virtual screen among them`. Les fichiers du pilote sont pourtant bien là, le chemin se résout, et le service tourne avec les droits qu'il faut.

**La cause, et c'est une leçon déjà apprise ailleurs.** La pose était demandée **à l'inscription du service et nulle part ailleurs**. Un ordinateur dont le service a été inscrit avant que ce code existe, ou dont la pose a échoué une fois, n'a plus jamais d'écran virtuel : rien ne réessaie et rien ne le dit. Les règles de pare-feu, deux lignes plus haut dans le même fichier, portent depuis longtemps le commentaire qui explique exactement ce défaut et le corrige pour elles seules : elles sont posées à chaque démarrage, précisément pour qu'une machine inscrite trop tôt finisse par les recevoir. Le droit de démarrer le service pour la personne connectée aussi. L'écran virtuel, non.

**Corrigé en appliquant la même règle une troisième fois.** La pose est demandée au démarrage du service en plus de l'inscription. Les deux moments conviennent pour les deux mêmes raisons : poser un pilote demande des droits d'administrateur, que le service a, et demande que personne ne regarde de session, ce qui est vrai d'un service qui n'a pas encore démarré son moteur.

**Avec une condition, et elle n'est pas décorative.** La présence est demandée d'abord, et toute la pose en dépend. Poser un pilote sur un appareil qui le porte déjà fait réinstaller ce pilote par Windows, ce qui retire l'écran et le rend : fait à chaque démarrage, ce serait un ordinateur qui claque ses moniteurs chaque fois qu'on l'allume, c'est-à-dire précisément le défaut dont [D50](#d50-zyrdesk-répond-de-lécran-de-lhôte-quand-le-moteur-ny-arrive-pas-2026-08-25-pendant-m4) s'occupe par ailleurs. Quand la question ne peut pas être répondue, rien n'est posé et le journal dit pourquoi : une pose « au cas où » est la seule façon de tomber dans ce piège.

**Ce que ça change pour une machine qui en manquait.** Au prochain démarrage du service, l'écran virtuel arrive, le moteur le voit dans sa liste, la note qui le nomme est écrite, le moteur redémarre en le visant, et les sessions cessent d'agrandir l'écran de la personne assise devant. Le journal raconte chacune de ces étapes.

## D50. ZyrDesk répond de l'écran de l'hôte quand le moteur n'y arrive pas (2026-08-25, pendant M4)

**Ce qui est gardé, et pourquoi il fallait le dire.** L'hôte met son écran à la taille de la session et le remet à la fin, l'agrandissement suivant avec. C'est ce qui fait qu'une session ne montre pas un petit bureau agrandi après coup mais un vrai bureau dessiné à la bonne taille. [D48](#d48-un-écran-qui-appartient-à-quelquun-nest-pas-à-nous-2026-08-25-pendant-m4) avait retiré ça pour éviter un retour raté. C'était jeter la fonctionnalité pour éviter son seul défaut, et sur ce point Victor a été net : « c'était très bien avant comme il adaptait la résolution et le scaling ». Elle revient entière.

**Le vrai défaut, en une phrase.** Le moteur est le seul à savoir dans quel état il a trouvé les écrans, donc le seul à pouvoir les y remettre. Quand il n'y arrive pas, personne dans le produit ne le sait, personne ne le dit, et personne ne fait rien. La personne assise devant l'hôte se retrouve avec un écran qui n'est pas le sien et une tour qui claque ses moniteurs, sans une ligne nulle part pour expliquer pourquoi.

**Le moteur le dit pourtant, et une seule fois.** Il écrit dans son journal qu'il n'a pas pu remettre la configuration d'écrans, puis il se tait et réessaie à ses conditions à lui : uniquement quand un écran est ajouté ou retiré. Ce déclencheur est exactement ce que fait l'autre programme qui lui dispute les écrans, alors les deux se défont mutuellement toute la soirée. Le service lit désormais cette phrase-là dans le journal du moteur, à partir de l'endroit où ce moteur-ci a commencé à écrire, deux secondes sur deux secondes, sans jamais relire ce qui est derrière.

**Ce qu'il en fait : redémarrer le moteur.** Ce n'est pas un tour joué au moteur, c'est lui rendre ses propres occasions. Partir et revenir sont les deux moments où il remet les écrans de lui-même : une fois en sortant, sans rien pour le gêner, puis à nouveau en entrant, et cette fois-là aussi longtemps qu'il faut. Trois tentatives de plus là où il n'y en avait aucune. Et l'essai sans fin qui prenait les moniteurs en otage s'arrête à la seconde où il s'en va, ce qui est très exactement ce que Victor observait en quittant ZyrDesk à la main.

**Et s'il le redit sans qu'aucune session ne soit passée entre-temps ?** Alors le redémarrage a déjà été essayé et n'a rien donné : quelque chose d'autre tient les écrans et compte les garder. Le moteur se voit demander d'arrêter d'essayer, une fois, par la seule porte prévue pour ça dans son interface locale. Les écrans restent où ils sont, ce qui est moins bien qu'un écran remis et infiniment mieux qu'un écran qui n'arrête jamais de bouger. Le journal le dit dans ces termes-là.

**Jamais pendant une session.** Les deux réponses couperaient la session en cours, l'une en emportant le moteur, l'autre en lui faisant oublier ce qu'il doit remettre. Et aucune des deux n'aurait de sens : une session en cours a rebougé ces écrans et le moteur les remettra en la quittant. Une plainte entendue pendant qu'on sert quelqu'un est donc oubliée, pas mise de côté. Ça compte plus qu'il n'y paraît : quelqu'un qui trouve son écran de travers se reconnecte dans la seconde, et ce serait précisément lui qu'on couperait.

**Ce qui empêche le service de tourner en rond.** Chacune des deux réponses est donnée une fois par vie de moteur, et le redémarrage n'est offert qu'à un moteur qui a servi quelqu'un depuis son démarrage. Le moteur qui vient d'être redémarré n'a donc jamais le droit de se redémarrer lui-même, et la porte se rouvre à la session suivante. C'est la porte d'entrée du tunnel qui répond à ces deux questions-là, parce que c'est elle qui voit passer les sessions et qu'elle naît et meurt avec le moteur.

**Ce que ça ne fait pas.** ZyrDesk ne lit ni ne pose lui-même les modes d'écran de Windows. Le moteur le fait déjà, il le fait bien, et un service qui s'y mettrait aussi serait un troisième programme dans une bagarre qui en compte déjà deux de trop.

## D51. Un moteur emporté par sa session ne se remplace pas dans la seconde (2026-08-26, pendant M4)

**Le relevé, et il est sans ambiguïté.** Éteindre l'hôte depuis la session, puis le rallumer : l'écran reste dans la taille de la session. Le journal du service dit ceci, en cinq secondes : `engine stopped (code 1073807364) after 1089 s, restarting in 0 s`, puis un moteur démarré, puis le même code, puis un autre moteur. Trois moteurs pendant que la machine s'en allait.

**Ce que vaut ce nombre.** `1073807364` est `DBG_TERMINATE_PROCESS` : ce que Windows laisse sur un programme qu'il a emporté lui-même avec la session où il vivait. Trois choses font ça et rien dans le code ne les distingue : une déconnexion, un changement d'utilisateur, une extinction. Ce n'est ni une panne ni une chute, et le service le lisait comme une chute.

**Pourquoi ça coûte un écran.** Ce que les écrans de l'hôte étaient avant la session est écrit par le moteur et **nulle part ailleurs**. Le premier moteur, emporté sans avoir eu le temps de remettre quoi que ce soit, laisse ce papier intact : au prochain démarrage de la machine, un moteur le lit et remet l'écran, c'est le chemin de secours prévu et il marche. Un moteur démarré dans une machine déjà à moitié dehors, lui, dépense ce papier sur un ordinateur qui n'aura bientôt plus d'écrans du tout. Le lendemain il n'y a plus rien à remettre, plus personne pour se plaindre, et un journal muet.

**Corrigé en attendant au lieu de fournir.** Un moteur emporté par sa session n'est plus jamais remplacé sur-le-champ : la machine est laissée tranquille dix secondes. Sur une extinction, elle s'en va pendant cette attente et rien de nous ne redémarre. Sur un changement d'utilisateur, quelqu'un attend dix secondes un ordinateur que personne ne demande encore. Et l'attente double à chaque fois que ça se reproduit d'affilée, parce qu'une machine qui s'éteint emporte aussi tous les moteurs démarrés derrière le premier : dix, vingt, quarante. Un moteur qui tient sa vie remet le compteur à zéro.

**Le compte des pannes ne bouge pas.** Être emporté par sa session n'est pas une faute du moteur, et le lui compter finirait par faire renoncer une machine sur laquelle on change souvent d'utilisateur.

**Ce que 1115 était vraiment.** Le service croyait reconnaître une extinction à `ERROR_SHUTDOWN_IN_PROGRESS`. C'est le code que le moteur renvoie depuis **sa propre icône de barre des tâches**, dont « quitter » veut dire quitter. Nous démarrons le moteur sans icône du tout : aucun ordinateur ZyrDesk n'a jamais renvoyé ce code, et cette branche n'a jamais servi une seule fois. Elle reste, parce que si le moteur le renvoie un jour, le laisser tranquille reste la bonne réponse.

**Et le journal dit enfin ce qu'il voit.** Deux lignes changent. La fin d'un moteur est racontée en mots plutôt qu'en numéro, parce que `1073807364` se lit comme un incident et n'en est pas un. Et la liste des écrans que le moteur voit, écrite à chacun de ses démarrages, porte maintenant leur taille : `U28G2G6B (…, on at 3840x2160)`. « Est-ce que l'écran de l'hôte est bien revenu » est la question qu'on pose le plus souvent à ce produit, et jusqu'ici elle ne se répondait qu'en allant se planter devant la machine. La taille avait été lue puis retirée le matin même avec [D48](#d48-un-écran-qui-appartient-à-quelquun-nest-pas-à-nous-2026-08-25-pendant-m4), faute de lecteur ; le lecteur, c'était le journal.

## D52. L'écran de l'hôte est remis tout de suite, pas trois secondes plus tard (2026-08-26, pendant M4)

**La question de Victor, et elle vaut mieux que la réponse qu'elle avait.** « Quand j'éteins le PC il renvoie le bon écran, ou alors au redémarrage je vais toujours avoir du 1920x1200 et c'est ZyrDesk qui remettra le 4K en démarrant ? Parce que c'est la première que je veux, comme Parsec. » [D51](#d51-un-moteur-emporté-par-sa-session-ne-se-remplace-pas-dans-la-seconde-2026-08-26-pendant-m4) ne donnait que la seconde : l'écran revenait, mais après le démarrage de Windows, après l'écran de connexion, après le service et après le moteur. C'est un filet, pas une réponse.

**Ce qui manquait tient dans un nombre, et il n'était même pas de nous.** Le moteur attend avant de remettre l'écran, et son délai par défaut est de **trois secondes**. Nous ne l'avions jamais écrit dans sa configuration, donc nous prenions le sien. Ce délai existe pour épargner deux changements d'écran à quelqu'un qui se déconnecte et revient aussitôt. Il coûte infiniment plus qu'il ne rapporte ici.

**Le relevé le dit à la seconde près.** `23:46:29 session ended`, puis `23:46:30 engine stopped`. Une seconde entre la fin de la session et Windows emportant le moteur. La remise en place était prévue à trois. Elle n'a jamais eu lieu.

**Et ce n'est pas un cas tordu, c'est le cas ordinaire.** Éteindre l'ordinateur d'en face depuis la session est une façon normale de finir. La session se termine alors **parce que** cet ordinateur s'en va déjà : il ne reste pas trois secondes, il n'en reste pas une.

**Corrigé en ne faisant plus attendre personne.** `dd_config_revert_delay = 0`. La remise en place se fait sur place, dans le fil qui vient de voir la session finir, au lieu d'être mise dans une file pour plus tard. L'écran est rentré avant que la machine ait fini de partir, Windows note cette taille comme la sienne, et l'ordinateur redémarre en 4K dès l'écran de connexion. C'est la première option, celle qui était demandée.

**Ce que ça coûte, et c'est assumé.** Quelqu'un qui quitte sa session et se reconnecte dans la seconde voit l'écran d'en face changer deux fois au lieu de zéro. Le délai de trois secondes achetait exactement ça, et il l'achetait au prix d'un écran qui ne revient pas quand la machine s'éteint. L'autre produit de référence remet aussi tout de suite.

**Les deux moitiés vont ensemble.** D51 empêche de dépenser le papier qui dit ce qu'était l'écran ; celle-ci fait que, la plupart du temps, ce papier n'a plus à servir du tout. Le filet reste, et c'est très bien : une extinction plus brutale que d'habitude retombe dessus au lieu de tomber par terre.

## D53. L'écran d'ouverture attend l'image, pas le service (2026-08-26, pendant M4)

**Le symptôme, et il n'arrivait qu'une fois sur quelques-unes.** « Des fois quand je me connecte, au lieu du bel écran de chargement, il reste sur l'accueil, il met un grand rectangle vert, et ensuite l'image arrive. » Le rectangle vert est la carte d'une session en cours, celle de l'accueil. Elle était juste : la session était bien en cours. Ce qui était faux, c'est que l'écran d'ouverture était déjà parti.

**Ce que couvre cet écran.** Les quelques secondes entre le moment où quelqu'un demande une session et le moment où il y a quelque chose à regarder. Il partait au moment où le service prenait la session, ce qui n'est pas le même moment : le service tient la session dès que le lecteur tourne, et l'image arrive plusieurs secondes plus tard.

**Pourquoi ça ne se voyait presque jamais.** Le chemin ordinaire passe par une attente que personne n'avait mise là pour ça : après avoir démarré le lecteur, l'ouverture guette six secondes pour voir si l'ordinateur d'en face le renvoie tout de suite, ce qui est le signe qu'il ne nous reconnaît plus. Ces six secondes couvraient l'écart, par accident. Quand les deux ordinateurs viennent de se présenter à nouveau, cette guette est sautée, tout à fait exprès : se faire renvoyer juste après une présentation est une autre panne, et regarder deux fois n'y changerait rien. L'écart réapparaissait alors tout nu, quatre secondes d'accueil avec une carte verte et pas d'image.

**Un relevé, deux ouvertures.** Ordinaire : lecteur démarré à 19:47:14, image posée à 19:47:17, écran d'ouverture retiré à 19:47:20. Après ré-appairage : lecteur démarré à 19:51:57, écran d'ouverture retiré à 19:51:57, image posée à 19:52:01.

**Corrigé en attendant la bonne chose.** L'écran d'ouverture se retire quand l'image est posée dans la fenêtre, et pas avant. C'est déjà ce que le produit sait faire et ce qu'il fait déjà sur un fil à part, pour poser l'image à la milliseconde où le moteur ouvre sa fenêtre ; il suffisait de demander la réponse à cette attente-là au lieu d'en inventer une autre. Là où c'était déjà juste, ça ne coûte rien : le temps que le service tienne une session ordinaire, l'image est dans la fenêtre depuis plusieurs secondes.

**Et si l'image ne vient jamais**, ce que le produit laisse vingt secondes au moteur pour faire, l'écran d'ouverture se retire quand même et le journal le dit. Une fenêtre qui reste couverte pour toujours serait pire que la panne qu'elle cache.

## D54. En mode fenêtre, une session s'ouvre agrandie (2026-08-26, pendant M4)

**Demandé, et la raison tient debout toute seule.** Une session montre le bureau d'un autre ordinateur, dessiné là-bas à la taille demandée d'ici. Une fenêtre plus petite qu'elle ne pourrait l'être, c'est cette image-là rapetissée une deuxième fois à l'arrivée, pour rien. Personne n'ouvre un bureau à distance en comptant le regarder dans un coin.

**Ce qui se passait.** La fenêtre gardait la taille où elle avait été laissée, ce qui est très bien pour l'accueil et n'a aucun sens pour une session. Elle s'agrandit maintenant au moment où la session est demandée, donc avant l'écran de chargement : la même surface est lue pendant l'ouverture et occupée par l'image ensuite, sans fenêtre qui grandit sous les yeux.

**Agrandie et non plein écran.** La barre de titre reste, la barre des tâches reste, et le coin de la fenêtre reste attrapable pour qui veut autre chose. Le plein écran est un choix à part, qui se règle, se retient d'une session à l'autre et ne passe par rien de tout ceci : une session qui s'ouvre en plein écran s'ouvre exactement comme avant.

**Seulement à l'aller.** La fin d'une session rend l'écran mais ne touche pas à la taille de la fenêtre. Rapetisser la fenêtre de quelqu'un après une heure passée dedans serait un geste que personne n'a demandé.

## D55. Celui qui appelle se présente (2026-08-26, pendant M4)

**Le relevé, sur un troisième ordinateur.** Un PC au travail, joint depuis la maison par un tunnel WireGuard. La découverte marche : `PC-SAV at 192.168.2.5 answered a call on the local network`, et son empreinte entre dans la liste de ceux qui peuvent venir. La session, elle, est refusée à tous les coups : `no way to 192.168.2.5:47000 … Détail : read error: connection lost`.

**Ce que ce message dit vraiment.** La connexion s'est faite. Le code ne se serait pas plaint de ça sinon, il aurait dit « ne répond pas sur le port 47000 ». Elle a été établie, puis coupée par l'ordinateur d'en face au premier échange réel, qui est exactement le moment où il juge le certificat de celui qui arrive. Autrement dit : ce n'est ni le pare-feu, ni le tunnel, ni la route. C'est un refus, et un refus veut dire une seule chose ici, que cet ordinateur-là ne connaissait pas celui-ci.

**Pourquoi il ne le connaissait pas.** La découverte tenait en deux mots. Une machine crie « qui est là ? » et les autres répondent « moi, voici où je suis ». Le cri ne disait rien de celui qui criait. Donc celle qui appelle apprend, celle qui est appelée n'apprend rien. Sur un réseau ordinaire ça ne se voit jamais : les deux crient, les deux apprennent, en une seconde. Sur un tunnel privé entre deux machines, un seul bout a un voisinage à balayer, l'autre porte une adresse unique et n'a personne à appeler. Ce bout-là restait un inconnu pour toujours, et refusait chaque session comme telle.

**Corrigé en faisant dire son nom à celui qui appelle.** La question porte désormais ce que portait déjà la réponse : le port, l'empreinte et le nom. Celui qui est appelé écrit son appelant sur sa liste avant de lui répondre. La découverte fonctionne alors dans les deux sens, même quand un seul des deux peut tendre la main.

**Ce que ça ne change pas.** La règle de confiance est la même mot pour mot : qui dit où il est, sur un réseau que cet ordinateur tient pour sûr, est un voisin. Appeler et répondre sont la même déclaration ; il n'y en avait qu'une des deux d'écoutée. Et une question sans présentation, qui est ce que disaient les versions d'avant, reçoit toujours sa réponse et n'apprend toujours rien : un ordinateur mis à jour d'un seul côté doit continuer de trouver l'autre, sinon la correction couperait ce qu'elle prétend réparer.

## D56. Le menu de la session dit où l'on en est (2026-08-26, pendant M4)

**Trois demandes de Victor, et la même idée derrière les trois.** Un menu doit montrer l'état des choses, pas seulement offrir des gestes.

**La souris devient un interrupteur.** « Souris bureau ou jeu » annonçait ce que le clic ferait et jamais où l'on en était. Les deux modes ne se distinguent pas à l'oeil sur un bureau immobile, donc on cliquait pour voir, ce qui est la définition d'un réglage qu'on ne comprend pas. Les deux mots sont là maintenant, côte à côte, et celui qui est en place est allumé. Le mode vit dans le coeur, qui compte chaque bascule qu'il envoie, et il est redemandé à chaque ouverture du menu : le raccourci du produit bascule la souris sans passer par cette page.

**Ce que ça ne sait pas, et il faut le dire.** Le raccourci du moteur tapé directement dans l'image ne passe par nous nulle part. Le produit croirait alors une chose et le moteur en ferait une autre, jusqu'à la bascule suivante depuis le menu. C'était déjà vrai avant ; ce qui change est qu'un désaccord se verrait, au lieu de se deviner.

**Taille, débit et codec deviennent des curseurs.** Ce sont trois échelles : plus grand, plus rapide, et on en cherche le bon cran en regardant l'image bouger. Une liste qui s'ouvrait sur le côté demandait un clic pour l'ouvrir et un pour choisir, cachait ce qu'il y avait au-dessus et au-dessous de la valeur en place, et obligeait la fenêtre du bouton à être plus large que le menu pour loger ce qui s'en échappait par la gauche. Un curseur montre l'échelle entière et où l'on est dessus, d'un coup d'oeil.

**Les crans sont nommés et non calculés.** Le curseur va de zéro au nombre de valeurs moins une, et le mot correspondant est écrit au-dessus : les débits ne sont pas espacés régulièrement, et « Écran » n'est pas un nombre. Le mot suit le pouce pendant qu'on le pousse, et le choix ne part qu'une fois lâché : autrement, traverser toute la barre enverrait une demande par cran au service, et la dernière écrite ne serait pas la dernière voulue.

**Et la fenêtre du bouton s'en trouve simplifiée.** Elle était mesurée sur l'union du menu et des trois listes, ouvertes ou non, parce que celles-ci sortaient du flux et débordaient par la gauche. Il n'y a plus rien qui déborde : elle est mesurée sur le menu, comme elle aurait toujours dû l'être.

## D57. Le bouton flottant n'a plus à suivre le premier plan (2026-08-26, pendant M4)

**Le relevé.** Session sur le deuxième écran, quelqu'un travaille sur le premier : le bouton flottant disparaît entièrement, et revient à l'instant où l'on redonne le premier plan à ZyrDesk. Rien ne le cachait pourtant, la fenêtre de la session était entièrement visible.

**Ce qui le faisait.** Le bouton était dessiné au-dessus de toutes les fenêtres de la machine. À cette hauteur-là il n'avait pas le choix : laissé en place, il aurait flotté dans un coin par-dessus le travail de quelqu'un d'autre. Il était donc caché dès que le premier plan partait ailleurs.

**Et cette hauteur n'avait plus lieu d'être.** Elle datait du temps où l'image était une fenêtre du moteur, posée à côté de la nôtre : être au-dessus de tout était le seul moyen de se poser dessus. L'image est portée à l'intérieur de notre propre fenêtre depuis [D21](#d21-limage-du-bureau-distant-saffiche-dans-la-fenêtre-de-zyrdesk-2026-08-19-pendant-m4), et la raison est partie avec. La hauteur, elle, est restée, et le fait de se cacher avec.

**Corrigé en laissant le système faire son travail.** Le bouton appartient à la fenêtre de ZyrDesk, et le système sait déjà quoi faire d'une fenêtre qui appartient à une autre : il la tient au-dessus de celle-là, la descend avec elle, et laisse n'importe quoi d'autre les recouvrir toutes les deux. Il n'y a plus une seule ligne de code pour dire quand le bouton se montre : deux questions restent, s'il est prêt et s'il a été masqué à la main, et le reste appartient à Windows.

## D58. Quatre reprises sur le menu de la session (2026-08-26, pendant M4)

**Les lignes reprennent toute la largeur.** Elles étaient rentrées vers la droite d'un chevron plus une gouttière, pour garder une colonne au sous-menu qui s'ouvrait sur le côté. Ce sous-menu est parti avec [D56](#d56-le-menu-de-la-session-dit-où-lon-en-est-2026-08-26-pendant-m4) et le retrait est resté : les lignes commençaient plus à droite que les chiffres du haut et que les traits de séparation, et le menu se lisait de travers.

**Le curseur de la taille va du plus petit au plus grand.** Il suivait l'ordre dans lequel le produit offre ses tailles, qui n'est pas celui-là. Sur une barre, pousser vers la droite veut dire demander plus ; l'inverse se lit comme une panne.

**Un clic sur la session referme le menu.** C'est ce que fait tout menu ouvert depuis que les menus existent, et il fallait recliquer le logo. Le clavier suffit à le savoir, sans rien guetter et sans crochet posé sur la machine : cette fenêtre n'est jamais activée, mais cliquer dedans donne le clavier à sa page, et cliquer ailleurs le lui reprend. « Ailleurs » couvre l'image, une autre application et le bureau, c'est-à-dire tous les endroits où un menu resté ouvert n'a plus rien à faire.

**Et le liseré blanc sur la gauche du bouton.** La découpe de la fenêtre était arrondie au plus proche, donc parfois vers le dehors : elle réclamait alors une colonne de pixels que la page n'avait pas peinte, et le système la remplissait de son propre blanc avant que la vue web ait repeint. D'où un liseré qui n'apparaissait pas toujours, et beaucoup plus en déplaçant le bouton : à chaque pas la fenêtre bouge, le système recopie ce qu'il peut et efface au pinceau la bande découverte. La découpe arrondit maintenant vers l'intérieur, bords remontés et tailles rabotées. Un pixel peint en moins ne se voit pas ; un pixel blanc de trop, si.

## D59. Ctrl+Alt+Suppr voyage sur le canal du produit, pas par le clavier (2026-08-26, pendant M4)

**Ce que Victor a demandé.** Une entrée dans le menu de la session pour envoyer Ctrl+Alt+Suppr à l'ordinateur d'en face.

**Pourquoi aucun moteur ne peut le faire.** Windows garde cette combinaison pour lui aux deux bouts d'une session. L'ordinateur qui regarde ne la voit jamais : son propre Windows la prend avant tout programme. Et l'ordinateur regardé ne peut pas la sentir arriver par le flux : la façon dont un moteur tape des touches est exactement celle que Windows refuse pour celle-là. Il n'y a qu'une porte, `SendSAS`, et elle ne s'ouvre que pour un programme que le système tient pour sûr.

**Décision : elle passe par notre propre canal, et c'est le service d'en face qui presse.** Le menu demande, la demande traverse le tunnel sur le canal que ZyrDesk se réserve, le service de l'ordinateur hôte la reçoit et presse la combinaison sur sa propre machine. Aucun correctif de moteur, aucune touche envoyée nulle part.

**Dans le processus du service lui-même, et c'est la correction d'un premier essai raté.** Windows dit précisément qui a le droit de passer cette porte : un programme **qui tourne comme service**, ou un programme portant un manifeste `uiAccess`, signé, et installé sous Program Files ou System32. La première version confiait la frappe à un aller simple lancé dans la session qui tient l'écran, comme pour taper le moteur sur l'épaule : un processus qui tourne bien sous le compte système, mais qui n'est ni un service ni un programme à manifeste, donc aucun des deux. Tout remontait « fait », parce que `SendSAS` ne renvoie rien et ne peut pas refuser, et rien n'apparaissait à l'écran. C'est donc le service qui presse, dans son propre processus.

**Un service vit en session 0, qui n'a pas d'écran, et la séquence part sur celle qui en a un.** C'est toute la raison pour laquelle la stratégie de Windows propose « les services » comme réponse : un service qui ne pourrait réveiller que sa propre session ne réveillerait personne.

**Et la stratégie qui l'autorise est posée à chaque démarrage du service.** Windows n'ouvre cette porte que si une valeur du registre le dit, et elle n'y est pas par défaut. Elle était écrite seulement à l'enregistrement du service, ce qui est la quatrième fois que ce produit apprend la même leçon : une machine dont le service a été enregistré avant que ce code existe ne l'aurait jamais reçue, et rien n'aurait réessayé ni même dit quoi que ce soit. Les règles de pare-feu, le droit de démarrer le service et l'écran virtuel sont posés à chaque démarrage pour exactement cette raison. Elle est retirée quand le service est désinstallé : ce produit ne laisse pas derrière lui une machine réglée autrement qu'il ne l'a trouvée.

**Ce que le produit ne peut pas savoir, et ce qu'il écrit à la place.** `SendSAS` ne répond rien du tout : ni succès ni refus. Une frappe qui ne fait rien et une frappe que Windows n'a jamais autorisée se lisent donc exactement pareil. Le journal du service écrit donc, au moment de presser, ce qui décide vraiment de l'issue : la valeur de la stratégie réellement en place, la session dans laquelle tourne le service, et celle qui tient l'écran.

## D60. Le son d'une session se coupe ici, dans le mélangeur de Windows (2026-08-26, pendant M4)

**Ce que Victor a demandé.** Un « son on off » dans le menu de la session.

**Ici et pas là-bas.** L'ordinateur d'en face continue de jouer ce qu'il joue ; celui qui demande le silence ne demande pas le silence d'une pièce où il n'est pas, il demande celui de la sienne. C'est donc le son de cette machine-ci qui se coupe.

**Décision : le lecteur a une tranche dans le mélangeur de Windows comme n'importe quel programme, et c'est celle-là qu'on baisse.** Rien d'autre de ce qui joue sur cet ordinateur n'est touché : la musique déjà en cours continue. Aucun correctif de moteur, aucun réglage caché, et le résultat est visible dans le mélangeur de Windows comme pour tout autre programme.

**L'état se relit, il ne se retient pas.** Contrairement au mode de la souris, à côté dans le menu, qui vit dans le moteur et ne se laisse pas interroger. Le muet, lui, vit dans le mélangeur, que tout le monde peut ouvrir et qui répond volontiers. Un interrupteur qui montre ce qu'il croit plutôt que ce qui est est un interrupteur dont on ne se sert pas deux fois.

**Une brique à part, comme l'écran virtuel.** `zyr-sound` connaît le son de Windows et rien de ZyrDesk, comme `zyr-screen` connaît les pilotes et rien de ZyrDesk. Elle prend un numéro de programme, ou rien du tout, et répond si c'est coupé. Ce qui mérite d'être coupé, et quand, se décide ailleurs.

**Et elle s'appuie sur le paquet `windows` et non `windows-sys`.** Toute la famille audio de Windows est en COM, et `windows-sys` ne porte aucune interface COM : il ne connaît que des fonctions, des structures et des constantes. Les écrire à la main reviendrait à recopier des tables de fonctions que rien ne vérifie. Le paquet `windows` est du même éditeur, il était déjà dans le verrou du dépôt, et le compilateur y contrôle chaque appel. C'est la seule brique du produit qui s'en sert, et c'est justement pour cela qu'elle est à part.

## D61. Les enceintes de l'hôte se taisent sans la carte son de personne (2026-08-26, pendant M4)

> Révisée par [D64](#d64-couper-le-son-den-face-se-décide-du-côté-qui-regarde-2026-08-27-pendant-m4) : le mécanisme est le bon et ne bouge pas, mais le réglage était du mauvais côté. Il appartenait à l'ordinateur regardé ; il appartient maintenant à celui qui regarde, et voyage avec la session.

**Ce que Victor a demandé, et la contrainte qu'il a posée ensuite.** « De base le son de l'hôte reste actif sur le pc physique, faudrait une option pour choisir si on veut que ça coupe le son physiquement pour que le son passe que par le stream. » Puis : « je ne veux absolument pas dépendre de steam, tu te débrouilles comme tu veux mais je ne veux dépendre de personne. »

**La réponse habituelle est une carte son de quelqu'un d'autre.** Les moteurs, le nôtre compris, font ça en installant une deuxième carte son à laquelle aucun câble ne mène, puis en y basculant la sortie de l'ordinateur le temps de la session. Celle qu'ils installent est celle de Steam. C'est exactement la dépendance refusée, et il n'y en a pas d'autre à installer sans acheter un certificat et faire signer un pilote.

**Ce qui rend une autre réponse possible.** Ce que le moteur hôte enregistre n'est pas ce qui sort des enceintes : c'est le mélange que Windows remet à la carte son, recopié avant que la carte applique son propre volume et son propre muet. Couper le muet de la carte vide donc la pièce et ne touche pas d'un cheveu ce qui part dans la session. C'est documenté par Microsoft et c'est ce que constate tout le monde en coupant ses enceintes pendant un enregistrement.

**Décision : le service coupe le muet de la vraie carte son, et rien n'est installé.** Un réglage dans les paramètres, éteint tant que personne ne l'a demandé : un ordinateur qui se tait tout seul dès qu'on le joint est un ordinateur que celui qui est devant appellerait cassé.

**Depuis la session qui tient l'écran, comme Ctrl+Alt+Suppr.** Quelle carte le bureau utilise dépend de qui a ouvert sa session. Posée depuis la session du service, la question nomme une carte que personne n'écoute, et la pièce continuerait de jouer. C'est donc le même mécanisme d'aller simple dans la session de l'écran, pour la troisième fois.

**Ce qui est dû n'est pas ce qui a été demandé.** Des enceintes que la personne avait déjà coupées avant la session sont laissées telles quelles, et rien ne leur est rendu à la fin : ce serait défaire un geste que ce produit n'a pas fait. Ce qui est dû survit au service dans un petit fichier, parce que Windows se souvient d'une carte coupée à travers un redémarrage : une machine éteinte au milieu d'une session resterait silencieuse pour toujours sans rien nulle part pour dire pourquoi. Le fichier est lu au démarrage du service, et le son est rendu au premier tour de garde, en réessayant tant que personne n'a ouvert de session sur l'écran.

**Et ce réglage-là ne redémarre pas le moteur.** Les deux autres réglages d'hôte oui, parce que le moteur ne les apprend qu'au démarrage, et une session en cours vers cette machine tombe avec. Celui-ci est l'affaire du service et du mélangeur de la machine : l'activer en pleine session ne coupe pas la session pour laquelle on vient de l'activer.

**Au passage, une dépendance à Steam qui était active par omission.** ZyrDesk n'écrivait aucune ligne de son dans la configuration du moteur, donc héritait de ses défauts : chercher les fichiers de la carte son de Steam sur la machine et l'installer si on les trouve, puis y faire passer le son de l'ordinateur à chaque session. Deux lignes ferment ça. La seconde ne pouvait pas rester vide : vide ne veut pas dire « aucune » pour le moteur, ça veut dire « celle de Steam ». Il lui faut un nom qu'aucune carte ne portera jamais, et il écrit alors une fois par session qu'il ne l'a pas trouvée, ce qui est exactement la réponse voulue, écrite dans son propre journal.

## D62. Fermer une session pendant qu'elle s'ouvre ne relance aucun appairage (2026-08-27, pendant M4)

**Le relevé.** Session ouverte, réglages changés et appliqués, image relancée, tout va bien. Trois secondes plus tard, la session est fermée depuis le menu. Et là : « l'ordinateur distant ne reconnaît plus celui-ci, nouvelle présentation », puis un écran de chargement, puis « l'ordinateur distant a refusé l'appairage ». Rien de tout cela n'avait été demandé.

**Ce qui le faisait.** L'ouverture d'une session ne se termine pas quand l'image apparaît. Ce que cet ordinateur retient d'un appairage n'est qu'une note qu'il s'est écrite à lui-même, et l'ordinateur d'en face peut l'avoir oubliée : il refuse alors la session en moins d'une seconde, dans un journal que personne ne lit. L'ouverture surveille donc le lecteur pendant six secondes après l'avoir démarré, et s'il s'arrête, elle en conclut qu'on ne nous reconnaît plus et représente les deux machines.

**Et fermer une session ressemble exactement à ça.** Fermer rend son bureau à l'ordinateur d'en face, cet ordinateur reprend le flux, et le moteur s'arrête de la seule façon qu'il connaisse : sur un échec. Vu depuis la surveillance, c'est mot pour mot un ordinateur qui ne nous reconnaît plus. D'où un appairage relancé par-dessus une session qu'on venait de quitter, refusé par le moteur d'en face à qui personne ne demandait de code.

**Décision : la surveillance pose la question à celle qui sait.** Le code de sortie du moteur ne distingue pas les deux cas et ne le pourra jamais. La seule chose qui les sépare est qu'une personne a cliqué, et la fenêtre est la seule à le savoir. L'ouverture reçoit donc une question à poser, « est-ce que cette session est toujours voulue », posée par petits pas plutôt qu'une fois à la fin : répondue non, la surveillance s'arrête là, sans conclusion et sans appairage.

**Ce que ça ne change pas.** Une machine qui a vraiment oublié cet ordinateur donne toujours lieu à une nouvelle présentation, et tout de suite. La question n'est posée que pendant ces six secondes-là et nulle part ailleurs.

## D63. La voie revient toujours, pas seulement quand tout s'est bien passé (2026-08-27, pendant M4)

**Le relevé, dans le même incident.** Une fois l'appairage refusé, la fenêtre affichait « Sessions ouvertes : 1 » sans aucune session, et le journal du service montrait une voie ouverte que rien n'a jamais refermée.

**Ce qui le faisait.** La voie était rendue à un seul endroit : à la fin de l'attente d'une session qui avait tourné. Toutes les autres sorties de l'ouverture la laissaient debout, et il y en a plusieurs : un moteur qui ne démarre pas, un appairage refusé, une surveillance qui conclut mal. Le service, lui, ferme une voie quand le processus qu'on lui a dit de surveiller s'en va, et on le lui dit à la toute dernière ligne de l'ouverture. Une voie abandonnée avant cette ligne était donc une voie que personne ne fermerait jamais.

**Décision : la voie est rendue par le fait même d'être lâchée.** Elle appartient à ce qui la tient, et ce qui la tient disparaît sur toutes les routes de sortie, pas seulement sur celle qui marche. Rendre une voie deux fois n'est pas une erreur, ce que le service vérifie déjà par un essai à lui, ce qui rend l'ajout sans danger à côté de tout ce qui pourrait déjà l'avoir dite.

## D64. Couper le son d'en face se décide du côté qui regarde (2026-08-27, pendant M4)

**Ce que Victor a dit en essayant [D61](#d61-les-enceintes-de-lhôte-se-taisent-sans-la-carte-son-de-personne-2026-08-26-pendant-m4).** « Faut évidemment que ça soit côté client que cette option fonctionne, si faut le faire sur l'hôte c'est de la merde. » Il a raison, et c'est une faute de conception et non un oubli.

**Pourquoi c'était le mauvais côté.** Le réglage était une préférence de l'ordinateur regardé : pour couper le son d'une machine dans une autre pièce, il fallait d'abord aller sur cette machine pousser un interrupteur. C'est exactement le geste que la prise en main à distance existe pour éviter. Et celui qui sait si la pièce doit se taire est celui qui vient d'en prendre le contrôle, pas celui qui y est resté.

**Décision : le choix vit avec la session, du côté de celui qui l'ouvre.** Il est rangé avec tout ce qui décrit une session ouverte depuis cet ordinateur, à côté de la taille, du débit, du codec et de la souris. Il part avec l'ouverture, sur le canal que ZyrDesk se réserve dans le tunnel, comme Ctrl+Alt+Suppr et comme le code d'appairage : ce n'est l'affaire d'aucun moteur.

**Ce que le mécanisme garde de D61.** Rien ne change de ce qui coupe réellement le son : le service d'en face coupe le muet de la vraie carte, depuis la session qui tient l'écran, et aucune carte son n'est installée. Ce que la capture emporte est le mélange remis à la carte, recopié avant que la carte applique son muet ; la pièce se tait, le flux garde son son.

**Ce qui est demandé n'est pas ce qui est promis.** La demande part dès que la voie tient, avant même que le moteur démarre, et un refus est écrit au journal sans jamais faire échouer la session : un ordinateur qui ne peut pas se taire, faute de session ouverte dessus ou parce que Windows n'en veut pas, a quand même une session parfaitement bonne à donner.

**Et le silence appartient à la voie, pas à la machine.** Ce que la session a demandé est oublié quand la dernière voie se ferme, de sorte qu'une session suivante qui ne demande rien n'hérite pas du silence de la précédente. Le son revient donc quand la session part, quelle que soit la façon dont elle est partie et quoi qu'il soit advenu de l'ordinateur qui avait demandé.

## D65. C'est le coeur qui écoute Windows changer de thème, pas la page (2026-08-27, pendant M4)

**Le relevé.** « Le light/dark ne suit pas le système, surtout quand je change sur Windows lui-même : ZyrDesk ne s'adapte pas tout seul alors que toutes mes autres applications le font. »

**Ce qui le faisait, et ce sont deux choses qui se tenaient.** Une page web demande d'ordinaire à son navigateur ce que le système préfère, et le navigateur suit. Ici le navigateur est une vue web posée dans notre fenêtre, et la boîte à outils fige la réponse de cette vue à une valeur fixe au moment où la fenêtre est construite : juste à la première image, gelée ensuite. Windows peut basculer, la page ne voit rien et l'événement qu'elle guette ne se déclenche jamais.

**Et le seul chemin qui rafraîchissait ça, nous l'avions bouché.** La boîte à outils remet la vue à jour quand elle voit Windows basculer, sauf si un thème a été imposé à la fenêtre. Or ce programme imposait un thème à chaque démarrage, précisément pour que la barre de titre s'accorde à la page : la barre de titre appartient au système et la page ne peut pas l'atteindre. Les deux bouts se battaient donc, et accorder le cadre coûtait le suivi.

**Décision : Windows est interrogé directement, et surveillé.** Le coeur lit la même valeur que la boîte à outils, au même endroit, et ne la sonde pas : Windows lève la main quand elle change, et chaque fenêtre est prévenue. La page ne garde plus aucune opinion sur ce que veut le système ; elle ne se sert de sa propre réponse figée que pour la toute première image, où elle est encore juste, en attendant la réponse du coeur quelques millisecondes plus tard.

**Et la fenêtre n'est forcée que si quelqu'un a choisi.** « Suivre le système » est transmis comme une absence de choix et non comme la couleur à laquelle il revient sur l'instant. Les deux se ressemblent une seconde et sont contraires ensuite : une fenêtre à qui l'on ne dit rien suit Windows d'elle-même, cadre compris, tandis qu'une fenêtre à qui l'on dit « clair » y reste pour toujours et, pire, fait taire les avis de bascule.

**Ce que ça coûte.** Un fil qui dort tout du long, réveillé par Windows lui-même. L'autre façon de faire est d'interroger le registre sur une minuterie, soit mille questions pour une réponse qui change deux fois par jour.

## D66. Quatre reprises sur le bouton flottant (2026-08-27, pendant M4)

**« Appliquer les changements » relançait l'image sans rien appliquer.** Et c'était une course, ce qui explique le « souvent » : lâcher un curseur envoie le choix au service, ce qui prend un aller-retour. La ligne « Appliquer » est déjà à l'écran depuis le choix d'avant, donc rien n'empêche de la cliquer pendant ce voyage. La relance relisait alors les réglages tels qu'ils étaient, et l'image revenait identique. Elle attend maintenant que les choix en vol soient écrits avant de demander quoi que ce soit.

**Et la même relance retombait en silence sur les valeurs d'usine.** La lecture des réglages répondait « ce que fait le produit par défaut » dès que le service ne répondait pas, ce qui est le bon défaut pour une fenêtre qui s'ouvre et le mauvais pour une image qu'on rouvre : la personne demande une chose et en voit changer trois. La relance garde désormais ce que l'image montrait déjà.

**Le codec devient des boutons.** Ce n'est pas une échelle : quelques noms sans ordre entre eux, dont un « Automatique » qui n'est pas une valeur mais un renoncement. Une barre à pousser promettait un plus et un moins qui n'existent pas. Les boutons sont bâtis depuis la liste que le produit tient, jamais recopiés dans la page.

**Le menu s'ouvre vers le haut quand il n'y a plus de place en bas.** La fenêtre du bouton est aussi haute que le menu pendant toute la session et pend par le logo, qui occupe un de ses coins. Accrochée par le haut, un bouton posé bas laissait le menu déborder sous le bord de l'image, où il était simplement coupé. Accrochée par le bas, le menu pousse dans la place qui reste.

**C'est le coeur qui décide du sens, et la page qui obéit.** La page ignore où sur l'écran elle a été posée ; le coeur, lui, connaît l'image et le coin où la main a laissé le logo. La réponse voyage là où les deux se parlent déjà de la forme de cette fenêtre : la page dit ce qu'elle dessine, le coeur répond de quel côté ouvrir.

**Et tout ce que la page mesure se compte alors depuis le bas.** C'est la même règle que pour la largeur, pour la même raison : le dessin est mesuré dans la fenêtre telle qu'elle est et découpé dans celle qu'elle devient. Vers le haut, c'est le sommet de la fenêtre qui se déplace quand elle grandit, donc le bas est le seul bord qui ne bouge pas. Compté depuis le haut, le menu se serait décalé de toute la différence, ce qui est exactement ce qui arrivait à son bord droit avant qu'on compte la largeur depuis la droite.

**Le curseur montre enfin une main qui agrippe.** Il passait au sens interdit pendant tout le déplacement : le logo porte une image, une image se traîne toute seule sous Windows, et le système prenait la prise du bouton pour un glisser-déposer. Plus rien de ce bouton ne se traîne ni ne se sélectionne.

## D67. La touche Windows part dans la session, et un interrupteur dit laquelle des deux machines reçoit ces touches (2026-08-27, pendant M4)

**Le relevé.** « La touche Windows n'envoie pas sur la session quand je suis dessus. » Avec, la façon de faire du produit de référence : un mode immersif, hors duquel Alt+Tab et la touche Windows restent sur l'ordinateur qui regarde, et dans lequel tout part dans le flux.

**Ce qui le faisait, et c'était une moitié manquante et non une fonctionnalité absente.** Il faut deux choses pour qu'une de ces touches parte au loin : que Windows ne l'attrape pas ici, et que le moteur l'envoie là-bas. [D43](#d43-les-touches-du-système-ont-un-seul-propriétaire-et-ce-peut-être-le-moteur-2026-08-24-pendant-m4) avait réglé la première pour Tab et Échap. La seconde était intacte, et elle refusait.

**Le moteur a une porte devant la touche Windows, et elle était posée sur une question sans réponse ici.** Elle demande à sa fenêtre d'être celle que le système appelle le premier plan **et** de tenir la prise clavier de sa bibliothèque d'affichage. La première est impossible chez nous, la fenêtre de l'image étant portée dans la nôtre, donc fille, et le premier plan allant au chef de famille. La seconde est justement ce que notre mode éteint exprès, cette prise avalant Alt et Control en entier et coupant tous les raccourcis du produit. La porte répondait donc non pendant toute la session, et la touche Windows n'a jamais quitté l'ordinateur qui regarde. Tab et Échap ne s'en apercevaient pas : ils ne passent pas par cette porte.

**Décision : la porte est posée sur la même question que le crochet.** « Le clavier est-il réellement à cette fenêtre », qui est la seule réponse vraie ici et que notre correctif calcule déjà pour décider quoi avaler. Une fenêtre ne peut pas avoir deux réponses différentes à la même question dans le même processus.

**Et le crochet avale maintenant la touche Windows.** Sans cela elle partirait au loin **et** ouvrirait le menu Démarrer d'ici, ce qui est pire que les deux comportements pris séparément.

> Complétée par [D68](#d68-le-clavier-immersif-prend-tout-ce-quun-logiciel-peut-prendre-et-dit-ce-quil-ne-peut-pas-2026-08-27-pendant-m4)

**Décision : c'est un interrupteur, pas un état permanent.** Prendre ces touches tout le temps est faux dans l'autre sens : la main qui va chercher Alt+Tab veut parfois une fenêtre de l'ordinateur qui regarde. Il vit dans le menu du bouton flottant, à côté de ceux de la souris et du son, et il se lit comme eux : les deux côtés écrits, celui qui est en place allumé. Un réglage qui décide où va une touche doit dire où il en est sans qu'on ait à essayer.

**Il se bascule sans relancer l'image.** Par le chemin qui existe déjà pour la souris : ZyrDesk tape le raccourci du moteur dans la fenêtre de l'image. Relancer la session pour changer de côté aurait rendu l'interrupteur inutilisable, puisqu'on le jette justement pour deux secondes.

**Il est retenu, et il vaut « session » par défaut.** Retenu parce que c'est la règle du bouton flottant depuis le début : ce qu'on y règle ne se règle pas à chaque connexion. Et « session » par défaut parce qu'une session dont la touche Windows ne fait rien sans qu'on sache pourquoi est exactement le défaut que tout ceci répare ; l'autre côté ne surprend personne, l'interrupteur étant sous les yeux et le disant.

**Deux touches restent hors de portée, des deux côtés.** Windows+L et Ctrl+Alt+Suppr, qu'aucun crochet ne peut prendre, par construction et pour de bonnes raisons. La seconde a sa propre entrée dans le menu, qui passe par le canal du produit et par le service d'en face ([D59](#d59-ctrlaltsuppr-voyage-sur-le-canal-du-produit-pas-par-le-clavier-2026-08-26-pendant-m4)).

## D68. Le clavier immersif prend tout ce qu'un logiciel peut prendre, et dit ce qu'il ne peut pas (2026-08-27, pendant M4)

**Le relevé.** « Les raccourcis avec la touche Windows sont toujours en local, par exemple Windows+L. En fait il faut vraiment un mode immersif au clavier : quand c'est actif, absolument tout passe sur la session. »

**Ce qui manquait vraiment, et ce n'était pas la touche Windows.** Elle partait bien, et ses combinaisons avec elle : le crochet l'avale ici, donc le raccourci d'ici ne se déclenche pas, et l'ordinateur d'en face la reçoit maintenue pendant que la lettre qui suit voyage par le chemin ordinaire. Restaient deux touches, et une seule des deux était au système.

**La touche Impr. écran.** Windows la détourne vers son outil de capture depuis quelques versions. Elle rejoint les autres dans ce que le crochet avale du côté immersif ; le moteur savait déjà l'envoyer.

**Alt+F4, qui n'était volée par personne.** Ni par Windows ni par nous : c'est la bibliothèque d'affichage du moteur qui ferme sa propre fenêtre dessus, et fermer la fenêtre de l'image est terminer la session. Un raccourci qui, dans un mode dit immersif, ferme la session au lieu de fermer la fenêtre d'en face est exactement le contraire de ce qui est promis. La bibliothèque est donc priée de n'en rien faire tant que l'interrupteur est du côté immersif, et de le refaire dès qu'il en sort.

**Décision : deux touches ne seront jamais prises, et c'est écrit noir sur blanc.** Windows+L et Ctrl+Alt+Suppr sont traitées par Windows dans une partie du système qu'aucun crochet ne voit, exprès : ce sont les deux gestes qui rendent la main à la personne assise devant la machine, et un programme capable de les intercepter pourrait faire passer un faux écran de connexion pour le vrai. Aucun produit de bureau à distance ne les a. Elles ne s'envoient pas davantage : le moteur d'en face pose les touches par le mécanisme ordinaire, qui ne déclenche pas plus Windows+L là-bas qu'ici. Verrouiller l'ordinateur d'en face passe donc par l'entrée Ctrl+Alt+Suppr du menu, qui ne va pas par le clavier du tout, ou par son menu Démarrer que la touche Windows ouvre maintenant.

**Et le réglage change de nom.** « Alt+Tab, Windows / Ici / Session » énumérait des touches sans dire ce que ça fait, et la liste était déjà fausse le jour où elle a été écrite. C'est **Clavier : Partagé ou Immersif**, qui est le mot de la personne qui l'a demandé, et qui reste vrai quand une touche s'ajoute à ce que le mode emporte.

## D69. Le sens du menu appartient au dessin à l'écran, pas au souhait du coeur (2026-08-27, pendant M4)

**Le relevé, deux symptômes et une seule cause.** « Quand je déplace le bouton de haut en bas, des fois il ne suit plus mon curseur : il reste tout en haut alors que mon curseur est tout en bas. » Et, sur une capture d'écran : le retour de la croix par-dessus le bouton, celle-là même qui avait été chassée en juillet.

**Ce qui les faisait.** [D66](#d66-quatre-reprises-sur-le-bouton-flottant-2026-08-27-pendant-m4) a donné au menu le droit de s'ouvrir vers le haut, et le sens était gardé à un seul endroit. Or ce mot-là recouvre deux choses différentes : le sens que le coeur voudrait, et le sens dans lequel la page a réellement dessiné ce qui est à l'écran. Les deux diffèrent forcément un moment, le temps que la page entende la réponse et se remette en page ; et pendant un déplacement, où rien n'est demandé à la page, ils peuvent différer aussi longtemps que la main tient le bouton.

**Et tout ce qui lisait ce mot lisait le mauvais.** Une fenêtre accrochée par son bas alors que sa page dessine encore depuis le haut pose le logo une hauteur de menu au-dessus de la main qui le tient : c'est le bouton qui décroche du curseur. Et la découpe, taillée pour un dessin qui n'est pas celui-là, garde un morceau de fenêtre que la page ne peint pas, que le système remplit de son propre fond : c'est la croix. Le journal le disait déjà, `3 morceaux dessinés jusqu'à 687x0`, une hauteur nulle étant la signature d'un dessin lu à l'envers.

**Décision : la page dit dans quel sens elle a dessiné, et c'est ce sens-là qui découpe et qui pose la fenêtre.** Le souhait du coeur reste ce qu'il répond à la page ; il ne pose plus rien. Où se trouve le logo est un fait sur ce qui est dessiné, et il n'y a jamais qu'un seul dessin. Les deux se rejoignent à l'image suivante, quand la page a redessiné, et jusque-là tout est cohérent au lieu d'être à moitié dans chaque sens.

**Ce que ça change au déplacement.** Le coeur continue de calculer le sens qu'il voudrait pendant qu'une main déplace le bouton, mais il ne le pose plus ; le retournement attend le prochain dessin, qui vient à l'ouverture du menu. Le bouton suit donc le curseur du début à la fin, ce qui est la seule chose qu'on lui demande pendant ce geste.

## D70. La signature d'un pilote se lit en demandant au fichier la bonne chose (2026-08-27, pendant M4)

**Le relevé.** « Tu me dis qu'il faut mettre l'écran virtuel, mais normalement il est installé directement avec ZyrDesk : je t'avais demandé explicitement que pour l'utilisateur ce soit invisible. » Il a raison, c'est ce qui a été décidé et écrit, et le journal des deux tours disait la même ligne depuis des jours : `MttVDD.cat carries no signature`, sur un fichier qui en porte trois.

**Ce qui le faisait.** Avant de poser un pilote, le service nomme son éditeur comme attendu par la machine, sans quoi Windows ouvre une fenêtre de confirmation sur un bureau où il n'y a personne. Pour cela il faut lire qui a signé, et la lecture était demandée de la façon la plus large possible : « ce fichier, c'est quoi ? ». Or un catalogue de pilote est un message signé dont le contenu se trouve être une liste d'empreintes de fichiers ; à cette question-là Windows répond la liste, rend un magasin qui la contient, et il n'y a aucun certificat dedans. Zéro certificat, donc « aucune signature », donc pas d'installation, donc pas d'écran virtuel, sur toute machine à qui ce pilote n'avait pas été donné autrement.

**Décision : la question est posée dans l'autre sens.** Ce fichier est un message signé, rendez ce qui l'a signé. C'est la voie documentée et c'est ce que fait tout outil qui lit une signature.

**Et un seul certificat est nommé, celui de l'éditeur.** Une signature en porte plusieurs : celui de l'éditeur, et les autorités qui le cautionnent à leur tour. Le message dit lequel est le sien, et c'est le seul repris. Nommer une autorité comme attendue étendrait le laissez-passer de cette machine à tous les pilotes que cette autorité a jamais signés, ce qui est beaucoup de monde et personne de chez nous. La promesse écrite dans la documentation, « ça n'accorde rien de plus », n'était donc pas tenue non plus.

**Le journal dit maintenant quel éditeur a été nommé.** Dire à une machine d'attendre des pilotes de quelqu'un sans écrire de qui, c'est le genre de ligne qu'on ne peut pas vérifier.

## D71. Verrouiller l'ordinateur distant est une entrée du menu, comme Ctrl+Alt+Suppr (2026-08-27, pendant M4)

**La demande.** « Pour Windows+L tu peux faire une option dans le bouton flottant comme pour Ctrl+Alt+Suppr, du coup ? »

**Oui, et c'est la même réponse au même problème.** [D68](#d68-le-clavier-immersif-prend-tout-ce-quun-logiciel-peut-prendre-et-dit-ce-quil-ne-peut-pas-2026-08-27-pendant-m4) a établi que Windows+L ne peut ni être attrapée ici ni être tapée là-bas, quel que soit le mode du clavier. Une touche qui ne peut pas voyager n'est pas une touche perdue : c'est une entrée de menu qui manque. C'est exactement le raisonnement qui a donné son entrée à Ctrl+Alt+Suppr, et il ne demande pas à être refait.

**Décision : la demande prend le canal du produit, et le service d'en face lève l'écran.** Même chemin, même forme, même endroit dans le menu, juste en dessous.

**Et c'est le miroir exact de Ctrl+Alt+Suppr, jusque dans la mécanique.** Windows n'accepte cette frappe-là **que** d'un service, donc le service la presse dans son propre processus. Il n'accepte de lever un écran de verrouillage **que** d'un programme assis sur le bureau interactif, ce qu'un service n'est justement pas ; le service se relance donc une seconde dans la session qui tient l'écran, ce qu'il sait déjà faire pour deux autres besoins et qui ne demandait aucune mécanique nouvelle.

**Les deux refus de Windows disent la même chose.** Ce que vaut un écran de verrouillage tient entièrement à ce que personne ne puisse le lever ni le baisser depuis l'extérieur du bureau auquel il appartient. Ce n'est pas une gêne qu'on contourne, c'est la garantie qu'on utilise : la demande passe par le seul programme de cette machine-là à qui son propre Windows accorde ce geste, et jamais par le clavier.

**Ce qu'il faut savoir en s'en servant.** L'image ne se coupe pas : le moteur hôte tourne avec les droits du système et sait capturer l'écran de verrouillage. On voit donc l'ordinateur d'en face se verrouiller, et on peut le déverrouiller de loin si on a le mot de passe, en s'aidant de l'entrée Ctrl+Alt+Suppr quand cette machine la réclame.

## D72. Le pointeur tient dans l'image parce que ZyrDesk le dit, et le bouton suit la main pas à pas (2026-08-27, pendant M4)

**Le relevé, deux morceaux.** « Si je déplace le bouton et que je sors de mon écran à droite, lui reste en place mais ma souris se déplace hors écran à l'infini. Et ce glissement se fait même sans attraper le bouton. »

**Deux causes, une par morceau, et toutes les deux vraies.**

**Le bouton, d'abord : il additionnait sans jamais se recaler.** Le bouton est tenu contre l'image, ce qui est voulu. Mais le déplacement mesurait l'écart depuis le début du geste : une main qui continue au-delà d'un bord demandait une place que le bouton ne peut pas prendre, et chacun de ces pixels restait dans la somme. Revenir n'a alors rien bougé tant que la main n'avait pas tout rendu, et le bouton attendait au bord pendant que le curseur était à un demi-écran de là. C'est exactement ce qui a été décrit.

**Décision : le geste se suit pas à pas, et ce qu'on suit est la place où le bouton est réellement.** Chaque pas part de là où il est, pas de là où il aurait été si l'image avait été infinie. Ce que la place refusée ne prend pas est perdu tout de suite plutôt qu'accumulé.

**Le pointeur, ensuite : le moteur avait ce qu'il fallait et ne pouvait pas s'en servir.** Il sait tenir le pointeur à l'intérieur de l'image, et il le fait quand sa fenêtre occupe un écran entier. Sa fenêtre est une petite fenêtre ordinaire portée dans la nôtre pendant toute la session : la condition est fausse du début à la fin, quoi que la personne soit en train de regarder. Le pointeur pouvait donc sortir de l'image sans que rien ne l'arrête, ce qui sur une machine à deux écrans veut dire qu'il s'en va.

**C'est le même défaut que celui du clavier**, à un mois d'intervalle et pour la même raison : une condition posée sur « ma fenêtre est-elle un écran entier », dans un produit où cette fenêtre n'est jamais rien d'autre qu'un morceau de la nôtre. Toute question de cette forme répond non pour la session entière.

**Décision : c'est ZyrDesk qui répond, et il le dit avec l'interrupteur du moteur.** Le moteur cesse de décider tout seul dès qu'on jette cet interrupteur-là, ce qui est précisément ce qu'on veut : il n'y a plus qu'un avis sur le pointeur et c'est le bon.

**Et il suit le plein écran, pas autre chose.** En fenêtré le pointeur doit pouvoir sortir : les autres fenêtres de cet ordinateur sont autour de l'image, et les atteindre est toute la raison pour laquelle on n'est pas en plein écran. En plein écran il n'y a rien autour, et un pointeur qui part sur le deuxième écran est un pointeur perdu.

## D73. La cadence de l'écran immobile se demande depuis le côté qui regarde (2026-08-27, pendant M4)

**La demande.** « Il faut absolument me rajouter dans le bouton flottant l'option pour choisir si on veut le FPS constant ou pas, parce que dans les réglages c'est relou et en plus je la trouve pas. »

**Il ne la trouvait pas parce qu'elle n'est pas là où elle sert.** Elle existe, sur l'écran d'accueil, dans le bloc « ce que cet ordinateur fait quand c'est lui qu'on regarde ». C'est-à-dire sur la machine qu'il faudrait aller régler à la main, pendant qu'il est assis devant l'autre. C'est mot pour mot la faute de conception de [D64](#d64-couper-le-son-den-face-se-décide-du-côté-qui-regarde-2026-08-27-pendant-m4), et elle a été refaite.

**Ce que le réglage fait.** Le moteur d'en face n'encode que lorsque son écran change, et sa réponse propre est la moitié de la cadence demandée : sur un bureau immobile, le pointeur avance par à-coups. Lui demander de renvoyer l'écran quand même rend le pointeur fluide et coûte une image complète encodée soixante fois par seconde pour rien. Lequel des deux vaut mieux dépend de la machine et de ce qu'on y fait, donc c'est un choix et pas un défaut à défendre.

**Décision : le choix vit avec la session, du côté de celui qui l'ouvre, comme le son.** La seule personne capable de dire si l'image est fluide est celle qui la regarde ; ce que ça coûte est payé en face, donc c'est une demande et non un ordre, et un refus est écrit au journal sans jamais faire échouer la session.

**Et il est demandé à l'ouverture, jamais au milieu.** Le moteur d'en face ne lit ce réglage qu'à son démarrage : en changer le fait repartir, et un moteur qui repart au milieu d'une session est cette session qui s'en va. Il se range donc dans le menu avec la taille, le débit et le codec, qui ont exactement la même contrainte, et il part avec eux quand on applique. C'est la même relance d'image qu'on voit déjà, à ceci près que le moteur d'en face redémarre au passage.

**Ce que ça ne change pas.** L'interrupteur de l'accueil reste ce qu'il est : le réglage de cette machine-ci quand c'est elle qu'on regarde, utile pour la poser une fois d'avance. Les deux disent la même chose sur des machines différentes, et le dernier qui parle gagne, ce qui est le comportement attendu d'un réglage qu'on peut changer des deux bouts.

## D74. La cadence plancher d'un écran immobile est une période, pas un délai ajouté au travail (2026-08-27, pendant M4)

**Le relevé.** « Le mode fluide ne fonctionne pas, je suis à 37 fps sauf quand je bouge la souris, là il passe à 60. »

**Le réglage partait bien, et le moteur en face l'appliquait.** Le journal montre la demande à chaque ouverture de session. Ce n'était donc pas le chemin, c'était ce qu'il y a au bout.

**Ce qui le faisait, et le chiffre le disait.** Le moteur hôte n'encode que lorsque son écran change, et se rattrape sur un écran immobile en réencodant l'image précédente au bout d'un temps donné, ce temps étant l'inverse de la cadence plancher. Or il était passé tel quel à l'attente d'une nouvelle image : **ajouté** à tout ce que la boucle fait par ailleurs au lieu de le couvrir. Sur un bureau immobile la période devenait l'attente **plus** l'encodage, et la cadence obtenue valait `1000 / (période + encodage)`.

**Le calcul se vérifie sur les relevés du client, qui mesure lui-même le temps d'encodage de l'hôte.** Sur trois sessions indépendantes : 2,1 ms d'encodage prédisent 53,3 images par seconde, il en a été mesuré 52,98 ; 9,3 ms prédisent 38,5, il en a été rapporté 37 ; 22,2 ms prédisent 25,7 sur une machine qui en donnait 30 en moyenne, mouvements compris. La première correspond à deux décimales.

**Et bouger la souris le masquait entièrement.** Les images arrivent alors d'elles-mêmes, l'attente se termine aussitôt, et la cadence redevient celle de la capture. D'où « ça marche quand je bouge la souris », qui est la description exacte d'un délai qui ne s'applique qu'à l'arrêt.

**Décision : la boucle vise une échéance au lieu d'attendre une durée.** L'attente est ce qu'il reste du temps de cette image-là, donc l'encodage sort de l'attente au lieu de s'y ajouter. ~~Et l'échéance suivante est une période après la précédente, jamais plus d'une période après maintenant.~~ Cette seconde moitié est corrigée par [D75](#d75-la-cadence-de-la-session-est-celle-de-lécran-et-une-image-capturée-passe-avant-une-image-répétée-2026-08-27-pendant-m4) : deux grilles de même pas finissent par se toucher, et ce qu'elle produisait alors était des images en trop.

**C'est un patch du moteur hôte, et le premier depuis le renommage.** Il n'a rien de ZyrDesk : le défaut touche tout bureau distant servi par ce moteur, et il est candidat à une contribution en amont. Il porte le plafond de Sunshine à deux sur deux, ce qui est le signal que le prochain besoin de ce côté se traite en amont ou par une interface officielle.

## D75. La cadence de la session est celle de l'écran, et une image capturée passe avant une image répétée (2026-08-27, pendant M4)

**Le relevé.** « Je dépasse les 60 fps, il faudrait que ce soit aligné avec mon écran. Là je n'ai pas de déchirure, mais j'ai l'impression que les images supplémentaires cassent la fluidité. »

**Trois choses différentes se cachaient derrière ce chiffre, et une seule était déjà réglée.**

**Un : la cadence demandée était écrite en dur.** Une session s'ouvrait à soixante images par seconde, toujours, quel que soit l'écran devant lequel on est assis. C'est mot pour mot la faute que la taille faisait avant qu'on la mesure : un nombre juste sur l'écran que la plupart des gens ont, et faux sur tous les autres. Sur un écran à cent quarante-quatre, deux rafraîchissements sur trois montrent l'image précédente ; sur un écran à trente, une image sur deux est jetée sans avoir jamais été vue.

**Décision : la session demande la cadence de l'écran sur lequel elle va s'afficher, mesurée au même endroit et au même moment que sa taille.** L'écran de la fenêtre et non l'écran principal, parce qu'un cent quarante-quatre à côté d'un soixante est le cas ordinaire d'un bureau à deux écrans. Bornée entre trente et cent quarante-quatre : en dessous, c'est une lecture qui a mal tourné plutôt qu'un écran sur lequel on travaille ; au-dessus, chaque image supplémentaire est payée entièrement en face, et un bureau distant est du texte et un pointeur, pas un jeu.

**Et un écran plus rapide que le plafond reçoit une part entière de sa propre cadence, pas le plafond.** Tout l'intérêt de mesurer est que les images tombent une par rafraîchissement ; une cadence qui ne divise pas celle de l'écran en met certaines deux par rafraîchissement et laisse les autres réafficher la précédente, ce qui est exactement le défaut qu'on cherche à fermer. Un écran à deux cent quarante reçoit donc cent vingt et non cent quarante-quatre.

**Windows répond zéro ou un pour un écran dont il ne tient pas la cadence, et ça ne veut pas dire « très lent ».** Ces deux valeurs-là veulent dire « pas mesuré » et retombent sur soixante, comme un écran qu'on n'a pas pu mesurer du tout. Sans ça, un écran illisible aurait été moins bien traité qu'un écran invisible.

**Deux : la cadence plancher d'en face était écrite en dur elle aussi, et au même nombre.** Le fichier de configuration du moteur hôte demandait soixante, parce que soixante était ce que toutes les sessions demandaient. Vers un écran à cent quarante-quatre, l'écran immobile serait réémis à soixante ; vers un écran à trente, il l'aurait été à soixante, c'est-à-dire **plus vite que la session ne l'a demandé**. Un plancher au-dessus du plafond est une contradiction, et le moteur ne l'attrapait pas.

**Décision : le plancher est plafonné par ce que la session demande, et la configuration écrit le plafond du produit.** Ce fichier est écrit avant que quiconque sache à quelle cadence la session s'ouvrira ; demander le plus rapide qu'une session atteigne jamais, en sachant que le moteur ne réémettra pas au-delà de ce qui lui est demandé, est la façon exacte de dire « la cadence de la session », quel que soit l'écran.

**Trois : deux grilles de même pas finissent par se toucher, et ce jour-là tout part en double.** C'est la moitié de [D74](#d74-la-cadence-plancher-dun-écran-immobile-est-une-période-pas-un-délai-ajouté-au-travail-2026-08-27-pendant-m4) qu'il fallait reprendre. La répétition existe pour un écran qui a cessé de changer. Mais quand l'écran change, la capture avance sur une grille à elle, à la même cadence que la répétition ; les deux dérivent l'une vers l'autre jusqu'à se toucher, et à cet instant le moindre soubresaut fait expirer l'attente un cheveu avant que l'image n'arrive. Les deux partent. Celui qui regarde jette la répétition, il n'a nulle part où la montrer, mais elle a d'abord été dessinée, encodée et transportée.

**Décision : une image capturée achète une période de battement en plus de la sienne, une image répétée en achète exactement une.** Un écran immobile n'envoie rien du tout : la capture se tait, l'attente expire, et les répétitions tombent alors sur une grille fixe à la cadence demandée, ce qui était le but depuis le début. Une période de battement suffit à ce qu'une image arrivée à l'heure gagne toujours, donc une répétition ne part que lorsqu'une image manque réellement.

**Ce que ça ne change pas, et qu'il faut dire clairement.** Le moteur client cale déjà l'affichage sur l'écran et jette ce qui arrive plus vite que l'écran ne sait montrer. C'est pour cela qu'il n'y a pas de déchirure, et c'est ce que notre ligne de statistiques compte sous `dropped_jitter_pct`, resté entre 0,05 % et 0,07 % sur les relevés. Les images en trop n'étaient donc jamais affichées : ce qu'elles coûtaient, c'est du travail en face et de la place sur le lien, et c'est déjà une raison suffisante de ne pas les envoyer.

## D76. Attendre en aveugle coûte le pire cas à chaque fois (2026-08-27, pendant M4)

**Le relevé, resté ouvert depuis [D59](#d59-ctrlaltsuppr-voyage-sur-le-canal-du-produit-pas-par-le-clavier-2026-08-26-pendant-m4).** L'image gèle un instant à chaque Ctrl+Alt+Suppr.

**Ce que fait cette touche, et pourquoi le moteur perd la main.** Elle fait changer Windows de **bureau** : le bureau sécurisé n'est pas celui où la session travaille, et la duplication d'écran est retirée au moteur pendant toute la bascule. Le moteur doit donc tout reprendre. C'est normal, ce n'est pas ce qui coûte.

**Ce qui coûte, c'est qu'il attendait sans regarder.** Rien dans le moteur ne peut être prévenu que la bascule est finie : la seule façon de savoir est de redemander. Il redemandait **deux fois, avec deux cents millisecondes de sommeil entre les deux**. Une bascule qui dure trois millisecondes coûtait donc deux cents millisecondes, exactement comme une qui en dure cent quatre-vingt-dix. Le même bout de code était écrit trois fois dans le même fichier.

**Décision : il redemande toutes les cinq millisecondes jusqu'à quatre cents, et le motif est écrit une fois.** Le pire cas est intact, et même allongé : une bascule vraiment lente a maintenant plus de temps qu'avant. C'est le cas courant qui change, et c'est le seul qu'on voit.

**Et la deuxième moitié était dans la synchronisation, plus intéressante que la première.** L'événement qui dit « réinitialisation en cours » réveille les fils qui l'attendent quand on le lève, et quand on l'arrête. Pas quand on l'**efface**, c'est-à-dire au seul changement qui annonce que c'est terminé. Le fil qui encode n'avait donc aucun moyen d'être réveillé et redemandait toutes les vingt millisecondes ; il payait cette période entière après que l'image était redevenue disponible, et cette période-là était la dernière chose entre la bascule et le retour de l'image.

**Décision : effacer réveille, et le fil attend au lieu de redemander.** Une attente bornée, pas infinie : ce type ne connaît pas les raisons d'abandonner que son appelant connaît, donc l'appelant garde la main dessus. C'est un défaut du moteur et pas un besoin ZyrDesk : n'importe quel programme bâti sur ce type l'a.

**Et le moteur dit maintenant le chiffre au lieu de le laisser deviner.** Le journal écrit combien de temps la reprise a pris et laquelle des deux moitiés l'a prise. Ce n'est pas une décoration : sur les deux défauts précédents, le temps perdu l'a été à supposer un chiffre au lieu de le lire, et il n'y avait aucun moyen de le lire.

**Ça franchit le plafond de Sunshine, de deux à trois, et c'est assumé.** Rien de ce qui précède n'est atteignable de l'extérieur : ce sont deux constantes internes et une primitive de synchronisation interne. Ce n'est pas un interrupteur de confort, ce sont deux défauts, et les deux sont candidats à une contribution en amont.

## D77. L'écran virtuel dort entre les sessions (2026-08-27, pendant M4)

**La demande.** « L'écran virtuel est toujours actif, donc sur mon PC il y a deux écrans qui apparaissent en permanence : celui de mon portable et le virtuel. Ça ne devrait jamais être actif hors d'une session. »

**Il a raison, et le défaut est de conception, pas d'implémentation.** Le pilote était posé au démarrage du service, et Windows démarre un périphérique à l'instant où il est déclaré. L'écran existait donc de la minute où le produit est installé jusqu'à celle où il est retiré. Personne n'a demandé ça : une machine que personne ne regarde a les écrans que son propriétaire a branchés, et pas un de plus.

**Décision : le pilote reste installé, l'écran dort.** Deux choses différentes qu'on avait confondues. Installer le pilote est long, demande les droits administrateur et fait cliqueter Windows ; c'est fait une fois. **Endormir** le périphérique, c'est ce qu'un clic droit « Désactiver » fait dans le gestionnaire de périphériques : instantané, réversible, et l'écran disparaît complètement, comme débranché. Il dort donc partout ailleurs qu'en session.

**Il se réveille sur demande, et la demande vient du côté qui regarde.** Une nouvelle question sur le canal du produit, comme le son et la cadence : « réveille ton écran virtuel pour une image de cette taille ». Elle part à l'ouverture de la session et la réponse arrive **avant** que l'image ne s'ouvre, parce que le moteur d'en face ne peut capturer qu'un écran déjà là. La taille voyage avec, parce que le pilote lit les tailles qu'on lui a écrites au moment où il se réveille et qu'il n'y a pas de deuxième chance.

**Un refus n'a jamais fait échouer une session, et ça ne change pas.** Un ordinateur sans écran virtuel sert ce que son propre écran sait dessiner et le nôtre étire le reste, ce que faisait chaque session avant que cet écran existe. C'est écrit au journal et la session continue.

**Le rendormissement a deux chemins, et le deuxième est celui qui compte.** Une session qui se termine proprement le dit. Une session dont l'ordinateur a été fermé, débranché ou a planté ne dit rien du tout, et c'est précisément là qu'un écran resterait sur le bureau de quelqu'un sans que personne sache d'où il vient. La surveillance qui tient le moteur regarde donc, à chaque tour, s'il reste réveillé sans personne derrière, et le rendort. Le service l'endort aussi à chaque démarrage, ce qui rattrape un service tué au milieu d'une session.

**Le seul moment où il se montre sans session, et pourquoi il est inévitable.** Le moteur d'en face nomme un écran par une empreinte de son identité que rien d'autre sur la machine ne calcule pareil, et il ne la dit que des écrans qu'il voit. Un écran qui dort n'est jamais vu. Sur un ordinateur qui n'a jamais fait tourner de moteur avec lui réveillé, ce nom ne serait donc jamais appris et l'écran virtuel ne servirait jamais à rien. Il est donc réveillé pour exactement un démarrage du moteur, **une fois dans la vie d'un ordinateur**, et rendormi dès que le nom est écrit. Quelqu'un assis devant voit un deuxième écran apparaître et repartir, une fois.

**Et un défaut du moteur hôte tombait pile en travers.** Il ne cherchait l'écran nommé dans sa configuration qu'à sa toute première énumération. Un écran branché après son démarrage n'était donc jamais repris, quel que soit le nombre de fois qu'il revenait, ce qui est exactement la vie d'un écran virtuel allumé pour une session et éteint après : le moteur aurait continué de capturer l'écran du bureau pendant toute sa vie. Il le préfère maintenant à chaque énumération, et le nom est relu de sa configuration à chaque passage, ce qui dit que c'était l'intention depuis le début.

## D78. Un appairage abandonné bloquait tous les suivants (2026-08-27, pendant M4)

**Le relevé.** « Je ne peux plus me connecter au PC du SAV, c'est quoi ce merdier », avec trois tentatives d'affilée finissant toutes sur `400 Invalid uniqueid`.

**Ce n'était pas le réseau, ni le tunnel, ni le code d'appairage.** Le service a bien ouvert la voie, bien remis le code, et l'échange a bien avancé de trois étapes sur quatre à la première tentative, puis de deux, puis d'une. Cette dégradation-là est la signature du défaut.

**Le moteur hôte garde les appairages en cours dans une table indexée par l'identifiant du client, et il n'y écrivait que si la clé était absente.** Une tentative interrompue n'importe où après sa première étape laisse une entrée dont l'étape a avancé. La tentative suivante du même client se voyait rendre **cette entrée-là** : son nouveau certificat était jeté, lui remettre un code échouait au contrôle d'ordre, et ce contrôle supprime l'entrée par sécurité. L'appel suivant du client ne trouvait donc plus rien et s'entendait répondre que son identifiant était invalide. À partir de là, les deux ordinateurs ne pouvaient plus jamais s'appairer, quoi qu'ils tentent, jusqu'au redémarrage du moteur hôte.

**Décision : demander le certificat du serveur, c'est dire « je commence un appairage », donc c'est le seul appel en droit de dire de quoi il part.** Il remplace ce qui était là au lieu de le retrouver.

**Et la remise du code avait le même défaut vu de l'autre côté.** Elle allait à l'appairage que la table tenait en premier, ce qui, après une tentative abandonnée, n'était pas celui qui attendait. Elle va maintenant à un appairage qui attend vraiment, et le dit clairement quand il n'y en a aucun.

**Le contournement en attendant une recompilation** : redémarrer le service ZyrDesk de la machine d'en face vide cette table.

## D79. La résolution est une liste, et sa vraie question est « lequel des deux écrans décide » (2026-08-27, pendant M4)

**La demande.** « Pour l'option taille, change ce nom pourri et mets Résolution, ensuite mets-la en sous-menu avec un chevron, et deux options d'abord : utiliser la résolution du client, utiliser la résolution de l'hôte, puis la liste des résolutions. »

**Le nom était faux et la barre l'était aussi.** « Taille » ne dit pas de quoi, et un curseur promet une échelle : plus à droite, plus grand. Or les deux premières entrées de cette liste ne sont pas des tailles du tout, elles disent **lequel des deux ordinateurs décide**, ce qu'aucune barre ne sait exprimer. Et il y en a quinze en dessous, ce qui fait des crans qu'on ne vise plus.

**Décision : une ligne qui ouvre une liste, avec les deux façons de décider en tête et les nombres ensuite.** Chacune des deux porte, écrit dessous, ce qu'elle fait vraiment. Pas une infobulle : une infobulle demande un survol que personne ne fait sur un menu qu'on traverse.

**Le sous-menu remplace le menu au lieu de s'ouvrir à côté.** Cette fenêtre est étroite et suit ce que la page occupe : une colonne de plus la ferait déborder de l'image. C'est aussi la porte qu'on avait déjà refermée une fois, quand un sous-menu latéral décalait toutes les lignes vers la droite pour lui garder une gouttière.

**« Résolution de l'hôte » a demandé bien plus que la ligne du menu.** C'est le seul choix dont le résultat n'est pas connu de ce côté-ci : rien ici ne sait ce qui est branché là-bas. La question d'écran qui voyage déjà avec la session répond donc désormais **la taille que l'ordinateur d'en face va afficher**, et la session s'aligne dessus avant de démarrer son moteur. Sans écran virtuel demandé, cette machine répond la taille de son écran à elle ; avec, elle répond ce qu'on lui a demandé.

**Et c'est le choix qui ne réarrange rien chez l'autre.** Aucun écran virtuel réveillé, aucune résolution changée sous quelqu'un qui est peut-être assis devant. Ce qu'il coûte est de ce côté-ci : l'image est mise à l'échelle pour tenir dans l'écran qui la regarde.

**Ce que la liste offre.** Les quinze tailles sont celles auxquelles les écrans se font, du 3840x2160 au 1024x768, avec leur rapport écrit à droite. Le rapport est calculé et non écrit à côté de chaque nombre : une deuxième table s'écarterait de la première le jour où une taille s'ajoute. Et il n'est pas montré pour les deux premières entrées, dont le résultat dépend d'un écran qu'on n'a pas encore vu.

**Ce qui a été écrit avant continue de vouloir dire ce qu'il voulait dire.** Le choix « écran du client » s'écrivait `screen` dans les préférences déjà posées sur les machines. Il s'écrit `client` maintenant, et `screen` reste lu comme tel : lu et jamais écrit, donc un choix fait une fois ne change pas de sens sous les pieds de qui l'a fait.

## D80. Un écran naît à la taille qu'on lui demande, et un refus de Windows se réessaie (2026-08-27, pendant M4)

**Le relevé.** « Gros plantages depuis la maj, impossible de tenir plus de 5 s. » Les journaux des deux machines l'expliquent en trois lignes, et le défaut est bien celui que [D77](#d77-lécran-virtuel-dort-entre-les-sessions-2026-08-27-pendant-m4) avait introduit.

**Un : l'écran se réveillait à la mauvaise taille.** Un écran virtuel qui démarre porte **la première taille de sa liste**, et la liste était dans un ordre fixe qui commence au plus petit. Une session demandant 1920x1080 recevait donc un écran né en 1280x720, et le moteur d'en face devait réarranger tout le bureau une seconde fois pour le corriger. Sur une machine à trois écrans, cette seconde réorganisation tombait pendant que ce moteur éteignait déjà les autres écrans pour la session : les deux se défaisaient l'un l'autre, et l'image ne tenait pas.

**Décision : la taille demandée passe en tête de la liste.** L'écran naît à la bonne taille, il n'y a plus rien à corriger, et il ne reste qu'une seule réorganisation du bureau au lieu de deux.

**Et le service ne mentait plus qu'à moitié.** Il répondait « cet ordinateur affichera 1920x1080 » pendant que l'écran faisait 1280x720. C'est vrai maintenant.

**Deux : un refus de Windows était compté comme un succès.** Le journal de la machine hôte : `the virtual screen would not go to sleep: Windows a refusé, code 13`. Windows refuse d'arrêter un périphérique d'affichage pendant que quelque chose d'autre réorganise le bureau, ce qui à la fin d'une session est exactement ce que le moteur est en train de faire, puisqu'il remet les écrans qu'il avait éteints. Un refus est donc **ordinaire** et veut dire « recommence dans un instant ».

**Il était noté comme fait quand même.** L'écran restait allumé pour toujours, plus rien ne le regardait, et le moteur suivant le trouvait au démarrage et le capturait à la taille qu'il portait ce jour-là. C'est ce qu'on lit dans le relevé : `screens the engine sees: VDD by MTT (on at 1280x720)`, puis `Capture size : 1280x720` alors que les deux vrais écrans sont en 3840x2160.

**Décision : on réessaie jusqu'à ce que ce soit fait, toutes les deux secondes.** Ce qui est écrit dans le journal est ce qui s'est passé, pas ce qu'on espérait.

**Trois, et c'était déjà corrigé la veille : répondre trop tôt.** Windows rend la main quand le périphérique a démarré, pas quand l'écran fait partie du bureau. Le réveil attend maintenant que le bureau compte un écran de plus.

**La leçon des trois.** Réveiller un écran, c'est demander à Windows de refaire son bureau, et le moteur d'en face fait exactement la même chose au même moment pour la même session. Chaque fois qu'on lui donne moins de travail, il y a moins de risque que les deux se croisent. C'est la même famille de défaut que celle décrite au sujet des écrans qu'un moteur n'arrive pas à remettre : deux programmes qui réarrangent la même chose se défont l'un l'autre.

## D81. On demande au périphérique ce qu'il fait, pas ce qui a été écrit sur lui (2026-08-27, pendant M4)

**Le relevé, et il se contredit tout seul.** Sur la machine hôte, à une seconde d'intervalle :

```
08:11:12 virtual screen already asleep
08:11:13 screens the engine sees: VDD by MTT (on at 1280x720) ; ...
08:11:13 the engine is capturing the virtual screen
```

Le service dit que l'écran dort. Le moteur le voit allumé et le capture. L'un des deux se trompe, et c'est le nôtre.

**Ce qu'on lisait.** L'état du périphérique était lu dans les drapeaux que l'outillage de Windows écrit quand quelqu'un active ou désactive un matériel. Ces drapeaux disent ce qui a été **demandé**, pas ce qui s'est passé. Or Windows refuse d'arrêter un périphérique d'affichage pendant que quelque chose d'autre réorganise le bureau, et **quand il refuse, il a déjà écrit que le périphérique est éteint**.

**La cascade complète, et elle explique tout ce qui a été relevé depuis deux jours.** Un écran qui a continué de dessiner se lisait donc comme endormi. Alors :
- Plus rien n'essayait de l'arrêter : `virtual screen already asleep` sur un écran allumé, à chaque démarrage du service.
- Le réveil suivant le trouvait « endormi », le réveillait pour rien, puis attendait qu'un écran de plus rejoigne le bureau, ce qui n'arrivait jamais puisqu'il y était déjà : `the virtual screen was woken but has not joined the desktop after 5000 ms`.
- Et une demande d'endormissement sur un périphérique que Windows croit déjà éteint est refusée avec le code 13, qu'on lisait comme un vrai refus alors que c'est « c'est déjà écrit ».
- Pendant ce temps l'écran restait sur le bureau en 1280x720, le moteur hôte le trouvait à son démarrage et le capturait : cet ordinateur servait un bureau de 1280x720 étiré, en permanence, session ou pas.

**Décision : la question se pose au périphérique lui-même.** Un périphérique démarré est un périphérique qui dessine un écran, quoi que quiconque ait écrit à son sujet. C'est une autre question que « qu'est-ce qui a été demandé », elle a une autre réponse, et c'est celle-ci qui compte.

**Ce qu'il faut en retenir au-delà de ce défaut.** Deux fois de suite sur ce sujet, on a pris pour acquis ce qu'un appel Windows a répondu au lieu de vérifier ce qui s'était réellement produit : d'abord en croyant qu'un périphérique démarré était un écran prêt ([D80](#d80-un-écran-naît-à-la-taille-quon-lui-demande-et-un-refus-de-windows-se-réessaie-2026-08-27-pendant-m4)), puis en croyant que des drapeaux disaient un état. Tout ce qui touche à l'affichage se vérifie en le regardant, jamais en le déduisant.

## D82. Un service n'a pas de bureau, et on ne range pas un écran pendant que le moteur remet les autres (2026-08-28, pendant M4)

**Le relevé.** La lecture d'état est corrigée depuis [D81](#d81-on-demande-au-périphérique-ce-quil-fait-pas-ce-qui-a-été-écrit-sur-lui-2026-08-27-pendant-m4) et le contrôle passe, mais l'ouverture d'une session traîne cinq secondes et la machine hôte se retrouve avec des écrans rallumés que son propriétaire avait éteints.

**Un : compter les écrans depuis un service ne compte rien.** L'attente posée la veille marchait sur le nombre d'écrans du bureau, lu en parcourant les périphériques d'affichage de ce programme. Cette question-là porte sur le poste de travail de celui qui la pose, et un service est assis sur un poste **sans bureau du tout**. Les écrans sont là, la réponse ne vient pas, et l'attente attendait un changement qui ne pouvait pas se voir : `the virtual screen was woken but has not joined the desktop after 5000 ms` sur un écran parfaitement réveillé, cinq secondes ajoutées à chaque ouverture.

**Ce qui le prouve dans le même relevé.** L'endormissement qui suit réussit du premier coup, donc le périphérique tournait bien ; et le moteur d'en face, en essayant de remettre les écrans, nomme cet écran dans sa topologie, donc il y était.

**Décision : la question se pose à la configuration d'affichage que le système tient, pas au poste de travail de celui qui demande.** C'est aussi ce que lit le moteur d'en face, donc les deux moitiés du produit regardent enfin la même chose.

> Cette moitié-là de D82 n'a jamais été écrite dans le code, et la voie annoncée n'était pas la bonne. Corrigée et remplacée par [D85](#d85-une-décision-écrite-nest-pas-une-décision-appliquée-2026-08-28-pendant-m4).

**Deux : on rangeait l'écran pendant que le moteur remettait les autres.** Une session qui se termine, c'est le moteur d'en face qui remet en place les écrans qu'il avait éteints pour elle, et l'arrangement qu'il remet est celui qui contenait notre écran virtuel. Nous l'endormions dans la même seconde. Le moteur se retrouvait à restaurer un écran qui n'existe plus :

```
Error: Device {5eb52002-...} does not exist in the available path source data!
Failed to change topology to: [...]
Warning: Failed to revert display device configuration. Enabling all of the available devices
```

**Et « allumer tous les écrans disponibles » est exactement ce qu'il ne faut pas faire chez quelqu'un.** Sur la machine du relevé, un écran éteint par son propriétaire s'est rallumé, et le service a dû redémarrer le moteur par-dessus. Nous avons cassé la disposition d'écrans de quelqu'un en rangeant la nôtre.

**Décision : on attend que le bureau cesse de changer avant d'endormir.** Pas un délai choisi au hasard : le moteur change le compte d'écrans à chaque écran qu'il déplace, donc une période sans aucun changement est une période où plus rien ne se passe. Dix secondes de patience, huit dixièmes de calme pour conclure, et ce que ça coûte est quelques secondes que personne ne regarde.

**Et c'est la troisième fois que le même piège se referme.** Croire qu'un périphérique démarré est un écran prêt, croire que des drapeaux disent un état, croire qu'une énumération répond depuis un service. À chaque fois, une réponse de Windows prise pour la réalité. Tout ce qui touche à l'affichage se vérifie en le regardant, et depuis l'endroit d'où il est visible.

## D83. On ne devine pas par où passer, on essaie (2026-08-28, pendant M4)

**Le relevé, et il est net cette fois.** « Ça marche sans Mullvad, mais dès que je le remets, ça casse le flux au bout de deux secondes. Dans exactement les mêmes conditions, le logiciel de référence n'a pas ce problème. »

**Le chiffre qui l'accompagne** : `Average network latency: 63 ms (variance: 0 ms)` entre deux ordinateurs posés sur le même bureau. Variance nulle : ce n'est pas de l'encombrement, c'est un chemin qui ajoute systématiquement soixante-trois millisecondes. Une session est encore possible, agréable non.

**Ce que nous faisions, et c'était un tirage au sort.** Une machine avec plusieurs cartes répond à un appel sur chacune, et chaque réponse arrive à part. Nous écrivions le pair et **remplacions** son adresse à chaque réponse : celle qui restait était celle de la dernière arrivée. Sur la machine du relevé, deux chemins existent vers le même ordinateur, et le tirage tombait sur celui qui traverse un réseau virtuel, lui-même repris par le VPN.

**Et rien ne permet de les distinguer en regardant.** Une adresse, ce sont quatre nombres. Lequel de ces nombres mène à travers un tunnel n'est écrit nulle part, ni dans l'adresse, ni dans le nom de la carte, ni dans ce que Windows en dit. Choisir « la première » ou « la plus petite » revient au même : deviner.

**Décision : toutes les adresses sont gardées, et la voie s'ouvre vers toutes à la fois. La première qui répond gagne.** Ce n'est pas une astuce, c'est la seule façon d'avoir la réponse : le chemin le plus rapide est celui qui répond le premier, par définition, et il n'y a rien à interpréter. Les autres tentatives sont abandonnées dès qu'il y a un gagnant.

**Le journal dit qui a gagné et en combien de temps**, parce que le jour où quelqu'un se demandera par où passe sa session, c'est cette ligne qui répondra : `192.168.1.20:47000 answered first, after 3 ms`.

**Ce que ça règle au-delà du VPN.** Toute machine avec une deuxième carte, un adaptateur virtuel, une machine virtuelle ou un réseau maillé était exposée au même tirage, et le perdait silencieusement une fois sur deux. Le symptôme n'était pas une panne mais une session simplement moins bonne, ce qui est le genre de défaut que personne ne signale et que tout le monde subit.

## D84. La course n'a servi à rien parce qu'il n'y avait qu'un concurrent (2026-08-28, pendant M4)

**Le relevé, et il tient en une ligne.** Sur la machine qui appelle, au moment d'ouvrir la session :

```
opening a way to 192.168.2.20:47000, expecting 0829cc7e...
```

**Cette phrase-là n'est écrite que dans un seul cas : quand il n'y a qu'une adresse à essayer.** La course décidée en [D83](#d83-on-ne-devine-pas-par-où-passer-on-essaie-2026-08-28-pendant-m4) écrit une autre phrase, qui nomme les adresses en lice. Elle n'a jamais eu lieu. Les chiffres du client le confirment sans discuter : `Average network latency: 59 ms`, inchangé.

**Pourquoi une seule.** La machine d'en face a quatre adresses, son propre journal les nomme toutes. Celle qui appelle n'en connaissait qu'une, et il n'y avait rien à garder en plus : sur cette machine, la découverte par annonce mDNS ne passe pas, et l'autre chemin de découverte, celui qui appelle directement, **n'apprenait qu'une adresse par nature**. Celle d'où la réponse est arrivée. C'est-à-dire celle où la question avait été envoyée : on redécouvrait ce qu'on savait déjà.

**Le défaut n'était donc pas dans le rangement, il était dans ce qui se dit.** Une machine qui se présente disait son port, son empreinte et son nom. Elle ne disait pas où elle répond. Une machine à quatre cartes répond sur celle que sa table de routage a choisie et se tait sur les trois autres ; celle qui l'écoute connaît une porte sur quatre, n'a aucun moyen de deviner les autres, et ouvre toutes ses sessions par celle dont on lui a parlé.

**Décision : un ordinateur qui se présente nomme toutes les adresses auxquelles il répond.** C'est ce que l'annonce mDNS a toujours fait ; c'est désormais aussi ce que dit l'appel direct, qui est le chemin qui marche quand mDNS ne passe pas. Huit adresses au plus, parce que chacune est une porte de plus à essayer et que la ligne doit tenir dans un seul petit datagramme.

**Ce que ça oblige à changer d'autre.** Le numéro de version de ce dialogue passe de 1 à 2. Deux versions différentes ne se parlent plus au lieu de se mécomprendre : lue à l'ancienne, la nouvelle ligne prendrait la liste d'adresses pour une empreinte et l'empreinte pour un nom, ce qui est exactement le genre d'accord faux qu'un numéro de version existe pour empêcher.

**Et le journal dit enfin ce qu'il fallait.** `PC-VICTOR at 192.168.2.20, also answering at 192.168.1.20, 10.141.87.37` : la ligne qui manquait, celle qui dit qu'il y avait un choix à faire.

**La leçon, et c'est la deuxième fois cette semaine.** Une correction qui repose sur une donnée doit se demander d'où vient cette donnée. La course était juste, son entrée était vide, et rien dans le code ne disait qu'une liste d'adresses pouvait n'en contenir qu'une pour une raison structurelle.

## D85. Une décision écrite n'est pas une décision appliquée (2026-08-28, pendant M4)

**Le relevé.** Sur la machine hôte, la ligne que [D82](#d82-un-service-na-pas-de-bureau-et-on-ne-range-pas-un-écran-pendant-que-le-moteur-remet-les-autres-2026-08-28-pendant-m4) déclarait réglée, mot pour mot :

```
the virtual screen was woken but has not joined the desktop after 5000 ms
```

**Le diagnostic de D82 était bon, sa correction n'a jamais été écrite.** Le texte annonçait que le comptage passerait par la configuration d'affichage du système. Le code, lui, comptait toujours les périphériques d'affichage du programme, c'est-à-dire la question aveugle depuis un service. Seule une ligne de dépendance avait bougé, sans que rien ne l'utilise. Cinq secondes perdues à chaque ouverture de session, sur un écran parfaitement réveillé.

**Et la réponse annoncée n'était pas la bonne non plus.** Toutes les questions d'affichage de Windows passent par le poste de travail de celui qui demande, la configuration d'affichage comprise. Un service n'en a pas. Choisir cette voie, c'était remplacer une question aveugle par une autre en espérant.

**Ce dont nous avons la preuve, en revanche**, c'est que les périphériques répondent depuis le service : c'est déjà comme ça que l'écran virtuel est trouvé, réveillé, interrogé et endormi, et ces opérations-là marchent. Un périphérique appartient à la machine, pas à une session.

**Décision : les écrans se comptent au périphérique.** Un écran qui se réveille, c'est un moniteur qui arrive dans la liste des moniteurs de la machine, et cette liste-là répond de partout. C'est la même mesure pour les deux attentes qui en dépendent : celle qui attend qu'un écran devienne un écran, et celle qui attend que le bureau cesse de bouger avant de ranger le nôtre.

**La règle qui manquait.** Une décision qui décrit un changement de code se relit contre le code. Écrite mais pas appliquée, elle est pire qu'absente : elle raye le défaut de la liste.

## D86. Une taille qu'on ne peut plus changer se prend sur le plancher, pas sur le plafond (2026-08-28, pendant M4)

**Le mot qui a tout débloqué.** « Dès que la session est établie, au bout de deux secondes, elle se fige totalement. » Pas une coupure : un **gel**. La connexion tient, la fenêtre répond, le clavier part, et l'image s'arrête net.

**Ce que ça élimine d'un coup.** Une connexion perdue coupe tout et laisse une raison derrière elle. Un gel avec la connexion vivante veut dire que ce qui passe par les canaux fiables continue de passer et que ce qui passe par les datagrammes ne passe plus. Chez nous, les canaux fiables portent le contrôle et le RTSP ; les datagrammes portent la vidéo et le son. Le journal du moteur client le dit sans ambiguïté : quelques images décodées, puis plus rien, et le contrôle encore debout quinze secondes plus tard.

**Le mécanisme, vérifié dans le transport et pas supposé.** Le transport part d'un paquet prudent et sonde vers le haut. Quand les gros paquets commencent à disparaître, il conclut que le chemin ne les porte plus et **retombe d'un coup au plus petit paquet que QUIC impose à tout chemin**, puis attend une minute avant de resonder. Cette bascule prend une seconde ou deux, et elle arrive exactement sur les chemins où elle compte : un tunnel privé porté à l'intérieur d'un autre, où la première sonde passe et où plus rien ne passe une fois la vidéo lancée.

**Et le moteur ne peut pas suivre.** On lui dit une taille de paquet une fois, à son démarrage, et il la garde toute la session. Nous la calculions sur la mesure du moment : 1290 octets. À la seconde où le transport retombait au plancher, **chaque paquet vidéo devenait trop gros pour être envoyé**, et chacun était jeté. En silence.

**Décision : la taille demandée au moteur se calcule sur ce que le chemin ne peut plus cesser d'offrir, pas sur ce qu'il offre à l'instant.** Une valeur qu'on ne pourra plus corriger se prend au pire cas, c'est la seule lecture qui vaille quelque chose. Le surcoût mesuré est de 1101 octets par paquet au lieu de 1290, soit quinze pour cent de paquets en plus pour la même image, et ce que ça achète est une session que le chemin ne peut plus tuer en se rétrécissant sous elle. Le plancher garanti n'est pas un chiffre choisi par nous : c'est celui en dessous duquel une connexion QUIC ne peut pas exister du tout.

**Effet de bord agréable** : les deux secondes d'attente qui servaient à laisser la découverte se faire disparaissent de l'ouverture de chaque session.

**Et la vraie faute, celle qui a coûté quatre jours : le silence.** Le tunnel jette des paquets à deux endroits, et les deux étaient muets. Un paquet trop gros pour le chemin, jeté plutôt que découpé ; et la file d'envoi qui déborde, où le transport sacrifie les plus anciens. Aucun des deux ne termine la session, **et c'est précisément pour ça qu'il faut les dire** : une session qui meurt laisse une raison, une session qui se tait ne laisse rien. Les deux comptes existaient en mémoire depuis le début et personne ne les lisait. Ils sont écrits dans le journal maintenant, une fois par voie et par espèce, avec la place qui reste sur le chemin et le nombre de fois où il s'est rétréci.

**La règle.** Un endroit du produit qui jette quelque chose sans le dire est un défaut à part entière, même quand jeter est la bonne décision. Ce qui se perd en silence finit par se payer en journées.

## D87. La longueur de la route se dit, parce qu'elle change en cours de session (2026-08-28, pendant M4)

**Le relevé, et il vient de l'usage.** Le gel est réglé ([D86](#d86-une-taille-quon-ne-peut-plus-changer-se-prend-sur-le-plancher-pas-sur-le-plafond-2026-08-28-pendant-m4)). Reste ceci : « le ping est multiplié par au moins deux quand je réactive le VPN commercial sur la machine hôte ».

**Ce que notre code fait, et il ne fait rien d'autre.** La session est ouverte vers une adresse du réseau privé maillé de son propriétaire, et elle y reste : nous remettons un paquet au système pour cette adresse-là, avant comme après. Ce qui a bougé n'est pas notre destination, c'est la route que prennent les paquets **du maillage lui-même** une fois qu'un autre VPN a pris la route par défaut de la machine. Le maillage part alors faire un détour avant de sortir. La destination est identique, le trajet a doublé.

**Ce n'est donc pas un défaut du produit, et ce n'en est pas moins un défaut du produit.** Nous ne choisissons pas cette route et nous ne pouvons pas la choisir. Mais la session a doublé de longueur pendant qu'elle tournait, et **rien nulle part ne l'a dit** : le seul endroit où ça se voyait était un compteur dans une fenêtre de statistiques que personne n'a ouverte. C'est la même faute que celle du paquet jeté en silence, une semaine plus tôt, sous une autre forme.

**Décision : la longueur de la route est écrite à l'ouverture de chaque voie, et réécrite chaque fois qu'elle double ou qu'elle est divisée par deux.** Pas à chaque lecture : un réseau respire, et un journal qui rapporte la respiration ne rapporte rien. Et pas en dessous de cinq millisecondes, parce que sur un câble la route passe d'un tiers de milliseconde à une milliseconde sans que personne ne sente quoi que ce soit.

**La règle, générale.** Ce que le produit ne peut pas corriger, il doit au moins le dire. Une dégradation invisible coûte plus cher qu'une panne, parce qu'une panne se raconte et qu'une dégradation se subit.

## D88. Le même sommeil en aveugle, un cran plus haut (2026-08-28, pendant M4)

**Le relevé, obtenu parce qu'on l'a enfin mesuré.** L'image se fige une à deux secondes au verrouillage de l'ordinateur d'en face. Le chemin du verrouillage a été chronométré des deux côtés ([R55](testing/M4-PROTOCOLE.md)), et notre moitié est hors de cause : soixante et une millisecondes, puis quarante-neuf. Le moteur, lui, dit ceci :

```
Capture reinitialized after 702ms (92ms waiting for the encoders to let the display go, 610ms finding it again)
Capture reinitialized after 489ms (61ms ..., 428ms finding it again)
Capture reinitialized after 486ms (30ms ..., 456ms finding it again)
```

**Trois réinitialisations pour un seul verrouillage, et toujours la même moitié qui coûte.** Retrouver l'écran prend quatre cent vingt-huit à six cent dix millisecondes ; l'attente des encodeurs, trente à quatre-vingt-douze.

**Et l'arithmétique est lisible à l'œil nu.** Le moteur redemandait l'écran deux fois, avec deux cents millisecondes de sommeil entre les deux : quatre cents millisecondes de sommeil pur, et vingt-huit de travail réel dans le cas à quatre cent vingt-huit.

**C'est exactement le défaut corrigé par [P-S4](../patches/MANIFEST.md), une copie plus haut, et manquée.** P-S4 a réparé trois copies du motif dans le fichier de la duplication d'écran. Il y en avait une quatrième dans la boucle de capture, sur l'écran lui-même, et elle n'a pas été cherchée.

**Décision : on redemande l'écran au lieu de dormir dessus**, toutes les vingt-cinq millisecondes jusqu'à cinq cents. Vingt-cinq et non cinq comme un cran plus bas : construire un écran n'est pas gratuit, c'est la raison d'être du sommeil d'origine, et tourner à vide dessus serait remplacer un défaut par un autre.

**La leçon, et elle est de méthode.** Un défaut corrigé à un endroit se cherche partout ailleurs dans la même forme, tout de suite, dans le même lot. « Le même motif était écrit trois fois dans le même fichier » aurait dû être une question et pas une conclusion : trois fois dans ce fichier-là, et combien dans les autres.

**Et ce qui l'a rendu trouvable en une soirée** : le chronomètre posé la veille sur le verrouillage. La ligne du moteur ne disait pas seulement « ça a pris sept cents millisecondes », elle disait laquelle des deux moitiés les avait prises. Sans cette séparation, le relevé aurait désigné le moteur sans désigner l'endroit.

## D89. Une dette qu'on ne peut pas payer n'est pas une dette, c'est un piège (2026-08-29, pendant M4)

**Le relevé, et il est sans appel.** « Quand je lance ZyrDesk, ça me fout en l'air mes paramétrages d'écran par défaut, c'est insupportable de devoir tout refaire à chaque fois. » Dans le journal du moteur, à chacun de ses démarrages :

```
Trying to revert applied display device settings. API is available: true
Error: Device {5eb52002-...} does not exist in the available path source data!
Error: Failed to change topology to: [["{5eb52002-...}"]]
Warning: Failed to revert display device configuration (will retry once devices are added
or removed). Enabling all of the available devices:
  ["{7db8b83a-...} - SAMSUNG", "{a1676a5f-...} - U28G2G6B", "{aed131a5-...} - U28G2G6B"]
```

**Ce numéro-là est celui de notre écran virtuel.** Le moteur garde l'arrangement d'écrans qu'il a trouvé avant qu'une session ne le change, et il le remet à chacune de ses vies jusqu'à y arriver. C'est juste, et c'est exactement le seul cas où ça ne peut pas marcher.

**Notre écran virtuel dort entre les sessions.** Un arrangement qui le nomme nomme donc un écran qui n'existe pas au moment où le moteur essaie. L'essai échoue, et ce que le moteur fait quand il échoue est **rallumer tous les écrans qu'il trouve**. Puis il garde l'arrangement et recommence au démarrage suivant. Et à tous les suivants. L'écran que son propriétaire avait éteint se rallumait à chaque lancement du produit, pour toujours, sans rien pour briser le cercle.

**Et le moteur ne peut rien y comprendre.** Il n'a aucun moyen de distinguer un écran parti d'un écran endormi, ni la moindre raison de soupçonner que l'un des deux va revenir. Ce côté-ci le sait, puisque c'est lui qui l'endort.

**Décision : au démarrage du service, un arrangement qui nomme notre écran virtuel est jeté.** Au démarrage et nulle part ailleurs : c'est le seul instant où aucune session ne peut être en cours, donc le seul où un arrangement en attente vient forcément d'une exécution qui ne s'est pas terminée.

**Et seulement celui-là.** Un arrangement qui ne nomme que de vrais écrans est une vraie dette envers une vraie personne : il est laissé exactement où il est.

**L'autre moitié, dans le même lot.** L'écran virtuel laissé allumé par une exécution interrompue était rangé au démarrage du service, **juste avant** que le moteur ne démarre. C'était lui retirer un périphérique d'affichage une seconde avant qu'il n'essaie de remettre un arrangement qui le nomme, et c'était en même temps l'événement qui le fait réessayer, puisqu'il réessaie à chaque ajout ou retrait de périphérique. Il est rangé après, une fois que le moteur a dit ce qu'il avait à dire, et le rangement attend déjà que le bureau cesse de changer avant de toucher à quoi que ce soit.

**La règle.** Quand un moteur tient une promesse qu'il ne pourra jamais honorer, ce n'est pas à lui de s'en apercevoir : c'est à celui qui a créé la condition. Nous endormons cet écran, donc c'est nous qui savons qu'un arrangement qui le nomme est mort-né.

## D90. Un écran se décrit par sa taille et par la taille de ce qu'on y écrit (2026-08-29, pendant M4)

**Le relevé.** « Je suis en mode résolution client, il a bien mis la résolution mais pas le scaling : je suis à 125 % sur mon portable et du coup l'écran n'est pas scalé. » La session portait 1920x1200, l'écran virtuel naissait en 1920x1200, et tout ce qui était écrit dessus arrivait deux fois plus petit qu'à la maison.

**Une taille toute seule ne décrit pas un écran.** Le même panneau à la même définition écrit un texte deux fois plus petit à cent pour cent qu'à deux cents. « La résolution du client » promet le bureau de la personne qui regarde, et son bureau c'est autant l'agrandissement que le nombre de pixels : la moitié qui manquait était la moitié qu'on voit.

**Le moteur hôte n'a rien pour ça.** Ses options d'écran couvrent la définition, la fréquence, le HDR et l'arrangement des écrans, et s'arrêtent là. Il n'y a pas de réglage à lui passer, donc rien à demander en amont : c'est ZyrDesk qui pose l'agrandissement, sur l'écran qu'il a lui-même fait pousser.

**Windows ne publie qu'un seul chemin pour le faire**, et c'est un message privé sur l'appel qui lit la configuration d'affichage. Sa propre page de paramètres l'utilise, tous les outils qui déplacent ce chiffre l'utilisent, et il n'a pas bougé depuis Windows 8.1. Il se demande par numéro et non par nom, il parle en pas le long de la liste que Windows offre plutôt qu'en pour cent, et il compte ces pas depuis celui que Windows recommande pour cet écran-là. Les trois sont écrits dans le fichier qui l'appelle, avec ce qu'ils veulent dire.

**Décision : l'agrandissement voyage collé à la taille, dans une seule valeur.** Séparés, les deux dérivent au premier changement, et un écran qui a l'un sans l'autre est le bureau de quelqu'un d'autre à la bonne résolution. La demande d'écran qui traverse le tunnel porte donc « largeur x hauteur @ agrandissement », d'un bout à l'autre.

**Zéro veut dire « aucun demandé », et prend ce que Windows recommande.** Deux cas le disent : une session qui n'a pas su mesurer l'écran qu'elle regarde, et une session qui a demandé une taille à la main. Cette taille n'est l'écran de personne, il n'y a donc rien à copier, et un agrandissement pris sur un autre panneau vaut moins que la recommandation de Windows. C'est posé à chaque réveil et pas seulement quand un chiffre est nommé : cet écran n'appartient qu'aux sessions, le laisser là où la session précédente l'a mis serait une machine qui se souvient du bureau d'une autre.

**Et « résolution de l'hôte » ne touche toujours à rien**, agrandissement compris : aucun écran n'est demandé du tout dans ce cas, ce qui est exactement ce que cette entrée promet.

**Où ça tourne, et pourquoi ce détour.** Tout ce que Windows dit de l'arrangement des écrans est répondu pour le poste de travail de celui qui demande, et un service siège sur un poste sans le moindre écran : depuis là, il n'y a rien à agrandir. Le service envoie donc cette course dans la session qui tient l'écran, exactement comme celle qui lève l'écran de verrouillage et celle qui coupe les enceintes. C'est la quatrième, et la forme était déjà là.

**Et ça ne fait jamais échouer une session.** Une session sur un écran écrit à la mauvaise taille reste une session ; une session refusée pour ça n'en est plus une. Ce qui s'est passé part dans le journal de l'hôte, en une phrase, et la session continue.

## D91. On ne touche pas aux écrans de quelqu'un, on lui rend son bureau (2026-08-29, pendant M4)

**Le relevé, et il annule une partie de [D77](#d77-lécran-virtuel-dort-entre-les-sessions-2026-08-27-pendant-m4).** « Il m'a coupé mes écrans physiques pour s'afficher sur un seul côté client, c'est pas ce que je veux, faut qu'il laisse les écrans physiques comme ils sont. Là actuellement ZyrDesk désactive mes écrans, c'est pas bon. » Trois écrans 4K éteints pour qu'un seul porte une image de 1920x1200, et une télé rallumée à chaque démarrage du service.

**Ce qu'on avait construit, et pourquoi c'était trop.** L'écran virtuel résolvait un vrai problème : servir une image plus grande que l'écran de l'hôte sans déformer, et sans déranger la personne assise devant. Mais pour que le bureau se déplace dessus, il fallait éteindre les autres, et c'est cette moitié-là qui est insupportable. On avait résolu un problème de netteté en en créant un plus grand.

**Décision : une session règle la taille de l'écran principal de l'hôte, et rien d'autre ne bouge.** Rien n'est éteint, rien ne change de place, rien ne pousse. C'est ce que font les bureaux distants qui ne fabriquent pas d'écran, et c'est ce que Victor demande explicitement en le comparant à ce qu'il connaît.

**L'écran virtuel garde le seul usage pour lequel il est réellement bon** : une machine qui n'a aucun écran branché, dans un placard, où il est la seule chose à filmer. C'est décidé au démarrage du moteur, qui est le seul moment où il lit quel écran filmer, et il est réveillé depuis le service parce que démarrer un périphérique d'affichage demande les droits administrateur.

**Le moteur ne s'occupe plus des écrans du tout, et c'est la moitié qui compte.** Il sait le faire et le faisait : cinq lignes de sa configuration lui demandaient de poser la taille, d'éteindre le reste et de tout remettre à la fin. Il remet un arrangement relevé à son propre démarrage, il abandonne dès que quelque chose d'autre a bougé un écran entre-temps, et **ce qu'il fait quand il abandonne est rallumer tous les écrans qu'il trouve**. C'est exactement ce qu'on lisait dans les journaux de Victor. Ces cinq lignes sont parties.

**C'est donc le produit qui promet, et qui tient.** Avant qu'une session ne touche à quoi que ce soit, tout le bureau est écrit sur le disque : pour chaque écran, allumé ou éteint, sa place par rapport aux autres, sa taille, sa cadence, son agrandissement, son orientation, et lequel est le principal. À la fin, tout est remis, écrans éteints compris. Une ligne par écran, lisible à l'oeil : ce fichier est ce qu'on ouvre quand un bureau est revenu de travers.

**Et il survit à la machine.** La note est écrite avant de toucher au bureau et effacée seulement une fois tout remis, donc elle survit au service, et c'est tout l'intérêt : les sessions qui laissent un bureau derrière elles sont celles dont l'ordinateur a été fermé, débranché ou a planté, et celles-là ne disent rien à personne. Elle est relue au démarrage du moteur et honorée là.

**Un bureau et une dalle sont deux tailles, et Windows le sait depuis longtemps.** Le relevé sur le portable de Victor : sa carte graphique n'offre réellement rien au-delà de sa dalle, la liste s'arrête à 1920x1200, et pourtant le produit de référence lui met un bureau 3840x2160, capture d'écran à l'appui. Il ne demande pas la même chose que nous. **L'ancienne interface d'affichage ne connaît qu'une taille par écran, et cette taille est le signal envoyé à la dalle** : elle s'arrête donc là où la dalle s'arrête. La moderne sépare les deux, le bureau a une taille, la dalle en a une autre, et la carte graphique réduit le premier dans la seconde. Un portable dessine alors un vrai bureau 4K, que son propriétaire voit en petit avec des bandes là où les formats diffèrent.

**Décision : c'est la seconde tentative, jamais la première.** Un écran qui offre la taille demandée est réglé par l'ancien appel, qui est simple, éprouvé et porte déjà toutes les sessions du produit. La moderne ne sert qu'à l'écran qui ne l'offre pas, où le choix est entre un bureau réduit et une image floue. Le chemin qui marche n'est pas touché, et c'est délibéré.

**Deux choses qu'on ne devine pas en lisant la documentation.** Les deux appels doivent être prévenus qu'un bureau peut différer de sa dalle, sans quoi ils décrivent un monde où les deux tailles sont égales ; et l'indice qui dit où un écran range sa taille cesse alors d'être un nombre pour devenir deux moitiés, dont seule la haute compte. Lire le mot entier nomme un écran qui n'a rien à voir. Windows garde par ailleurs, **par écran**, un interrupteur qui refuse tout bureau plus grand que la dalle, et il faut le lever : c'est la différence entre marcher et ne pas marcher, et c'est ce qui produit la boîte « la résolution choisie n'est pas prise en charge ».

**On demande à Windows un mode qu'il offre, on n'en fabrique pas un.** Le refus qui a lancé tout ça, « that screen does not have that size » sur un portable à qui on demandait 3840x2160, ne venait pas de l'écran : il venait de nous. Un mode d'affichage construit ici à partir d'une largeur et d'une hauteur ne porte ni profondeur de couleur ni fréquence, et Windows le compare à ce que le pilote offre. La bonne façon est de **lire la liste** des modes de l'écran, d'y prendre celui qui a la taille voulue, et de le poser tel quel. La fréquence courante l'emporte, puis la plus rapide : une session n'a pas à faire tomber en soixante hertz un écran qui en fait cent quarante-quatre.

**Et il y a deux listes, pas une.** La première est ce que le moniteur dit de lui-même, et elle s'arrête à la taille de la dalle. La seconde, qu'il faut demander explicitement, est ce que la carte graphique sait réellement produire, et sur presque tous les portables elle contient des tailles plus grandes que la dalle, dessinées en entier puis réduites dedans. C'est cette seconde liste qui permet à un portable 1920x1200 de servir un bureau 4K, et c'est exactement la capacité pour laquelle on avait fait pousser un écran virtuel entier. Elle était là depuis le début.

**Quand la taille n'y est vraiment pas, le journal dit ce que l'écran offre.** Huit tailles, les plus grandes d'abord. C'est la question que se pose la personne qui lit : est-ce que ce que j'ai demandé était déraisonnable pour cette machine.

**Un agrandissement appartient à une taille, et pas à une session.** Le relevé qui suit, dans l'autre sens : depuis un écran 4K à 175 % vers un portable 1920x1200, l'hôte refuse la taille, garde la sienne, et prenait quand même les 175 %. Un agrandissement demandé au nom d'une taille qui n'a pas été obtenue n'appartient plus à rien : il n'est donc posé que si la taille est réellement là, et la taille est relue plutôt que supposée. Et il fait partie de ce qui est remis, au même titre que la taille : une session qui n'avait changé que lui ne remettait rien du tout, puisque rien n'avait changé de taille.

**« Résolution de l'hôte » veut dire le bureau de son propriétaire, pas celui que la session d'avant a laissé.** Le relevé : passer une session en cours de « Résolution du client » à « Résolution de l'hôte » laissait l'hôte 4K en 1920x1200 pour le reste de la soirée. Basculer ferme la voie qui portait la taille et en ouvre une autre dans la même seconde ; entre les deux, plus personne ne regarde, mais la remise en place tourne sur le fil qui tient le moteur et n'a pas encore eu son tour. La session suivante demandait alors l'écran de l'hôte et recevait ce que la précédente avait posé. Une session qui ne nomme aucune taille rend donc le bureau elle-même avant de répondre, et elle ne le fait que si personne d'autre ne regarde : une autre session en cours est servie à la taille qu'elle a demandée, et la lui retirer est précisément la chose que tout ceci interdit.

**Et le basculement en cours de session est le moment fragile, pas le cas rare.** C'est ce que Victor fait le plus souvent, et c'est là que ça cassait : le rendu de bureau n'était fait que si aucune autre session n'était ouverte, or une session qui se ferme et la suivante qui s'ouvre dans la même seconde sont comptées ensemble tant que la première n'a pas fini de sortir. Cela donnait un tirage au sort, les mêmes trois clics marchant un soir et ne faisant rien le lendemain. Une session qui demande l'écran de l'hôte l'obtient donc maintenant sans condition : ce que cela coûte à un second spectateur est une image qui change de taille, contre un premier spectateur servi sur le mauvais écran. Que les deux puissent se chevaucher est O1, encore ouverte.

**Le service cesse enfin d'être aveugle sur les écrans.** Tout ce que Windows dit de l'arrangement des écrans est répondu pour le poste de travail de celui qui demande, et un service siège sur un poste qui n'en a aucun. Interrogé sur ce qu'il montrait, l'hôte répondait qu'il ne savait pas mesurer son propre écran, et la session gardait la taille qu'elle avait supposée : c'est de là que venait un client en 1920x1200 persuadé qu'un hôte en 3840x2160 lui montrait du 1920x1200. La session qui tient l'écran écrit ce qu'elle voit, le service le lit. C'est la quatrième course qu'on envoie là-bas, après le verrouillage, les enceintes et l'agrandissement.

## D92. Un agrandissement n'est pas un pourcentage, c'est un pas compté depuis une recommandation qui bouge (2026-08-29, pendant M4)

**Le relevé, et il a fallu quatre essais pour en venir à bout.** « Le PC portable reste à 100 %, il ne revient pas à 125 %. » La taille revenait bien, l'agrandissement non, et le journal n'avait qu'une seule chose à dire : cet écran ne répond pas. Il disait vrai, et c'était la mauvaise question.

**Ce que Windows garde n'est pas ce qu'il montre.** La page des paramètres affiche « 125 % », mais ce qui est réellement écrit pour l'écran est un **pas** le long d'une liste fixe, compté depuis celui que Windows recommande **pour cet écran à la taille qu'il a en ce moment**. Changer la taille du bureau déplace la recommandation : le pas n'a pas bougé, ce qu'il veut dire a changé. C'est ainsi qu'un écran 4K à 175 % passé en 1920x1200 se retrouve à 125 % sans que rien n'ait été écrit dessus, et qu'il revient à 175 % tout seul quand le bureau rentre à la maison.

**Le piège, lui, n'a rien d'accidentel.** Poser 175 % pendant que le bureau est en 3840x2160, puis ramener ce bureau à 1920x1200, laisse l'écran sur un pas qui n'est plus dans la liste annoncée. Le journal a fini par le dire mot pour mot : « il est au pas -2 d'une liste comptée depuis le pas 1, qui va de -1 à 3 ». Un pas hors de la liste ne vaut aucun pourcentage, et l'écran interrogé ne répond plus rien du tout.

**De là partait un cercle dont on ne sort pas tout seul.** Un écran qu'on ne peut plus lire ne met rien dans le relevé fait avant la session suivante ; ce qui n'est pas relevé n'est pas remis ; donc il reste dans cet état, indéfiniment. La seule chose qui en sortait un portable était d'ouvrir les paramètres d'affichage à la main. Deux choses cassent ce cercle. Un pas qui ne veut plus rien dire n'est pas une raison d'abandonner, c'est le seul cas où écrire sans lire est exactement juste ; et l'agrandissement est remis pour **tout écran allumé**, pas seulement pour ceux dont la taille est revenue de travers.

**Un troisième oubli tenait à la même famille.** Les appels qui décrivent les écrans doivent être prévenus qu'un bureau peut différer de sa dalle, sans quoi ils ne savent pas décrire un écran dans cet état et répondent que cette machine n'a aucun écran. Un portable dont le bureau avait été agrandi pour une session n'était donc plus trouvé du tout : ni son agrandissement remis, ni son bureau compté comme rendu.

**Décision : ce qu'on remet est l'agrandissement de la personne, jamais un défaut.** Victor l'a posé en une phrase : « si ton code c'est juste se baser sur la recommandation de Windows ça me convient pas, il faut que ça reprenne le scaling d'avant la session, si des users n'utilisent pas le scaling recommandé après ils sont aussi dans la merde. » C'est exactement le contraire de ce que le code faisait alors, et c'est lui qui a raison.

**Donc une mémoire, et elle survit à tout.** Un fichier à côté du relevé, `data/screen/screen-scales.txt`, une ligne par écran désigné par l'identité qui survit à un redémarrage et non par sa place dans la liste. Il retient le dernier agrandissement qui a pu être lu, il est mis à jour dès qu'il change, et il n'est relu que pour un écran qui refuse de répondre. La recommandation de Windows existe toujours, mais elle est descendue au tout dernier rang : elle ne sert plus qu'à un écran jamais lu une seule fois, c'est-à-dire à une machine où ce fichier n'existe pas encore.

**Et cette mémoire n'écoute que le propriétaire.** Elle n'est mise à jour que tant que le relevé d'avant session n'existe pas, c'est-à-dire tant que le bureau est encore celui de son propriétaire. Dès qu'une session le tient, ce que les écrans dessinent est l'oeuvre de cette session : le retenir reviendrait à servir, à la prochaine session incapable de lire un écran, l'agrandissement qu'un inconnu avait demandé.

## D93. On nomme toujours l'écran que le moteur filme (2026-08-30, pendant M4)

**Le relevé.** « Ça change bien côté physique mais côté client non, et en plus il m'affiche mon écran de gauche alors qu'avant de switcher de mode de résolution j'étais sur l'écran principal. » L'écran physique de l'hôte prenait bien la taille demandée et la rendait bien à la fin ; ce que le client recevait était l'autre écran, aplati dans la taille demandée.

**Le moteur n'était jamais dit quel écran filmer, et ce n'était pas un oubli anodin.** Sans nom dans sa configuration, il filme celui que la carte graphique énumère en premier, ce qui n'est déjà pas l'écran principal sur toutes les machines. Pire, il reprend le premier écran qui répond chaque fois qu'il doit recommencer à filmer, et il doit recommencer précisément quand on change une définition. Or **un écran dont on change la définition disparaît de cette énumération pendant tout le changement** : celui que la session venait de régler était exactement celui que le moteur laissait tomber, au profit du voisin, pour le reste de la session.

**Décision : l'écran filmé est nommé, sur toute machine.** L'écran principal de l'hôte quand la machine a des écrans, celui qu'elle fait pousser quand elle n'en a aucun. C'est la même mécanique dans les deux cas, et elle existait déjà pour le second : le moteur est la seule chose qui sache nommer un écran d'une façon que sa propre configuration accepte, donc son nom est **lu dans son journal** et jamais recalculé, écrit à côté du service, et le moteur redémarre une fois quand ce nom change. Il ne lit ce réglage qu'à son démarrage.

**Et la moitié qui reste est dans le moteur.** Nommer l'écran ne suffit pas tant que n'importe quel autre fait l'affaire au moment où il manque. Les deux boucles de reprise du moteur prenaient le premier écran qui répondait ; elles redemandent maintenant l'écran nommé pendant trois secondes avant de se rabattre, ce qui est long devant un changement de définition et court devant un écran vraiment débranché. C'est le complément de P-S5, dans la fonction que P-S5 avait déjà corrigée à moitié, et ce n'est toujours pas une fonctionnalité ZyrDesk : c'est un défaut qui touche tout bureau distant servi par ce moteur.

## D94. Un « oui » de Windows sur l'affichage se relit, il ne se croit pas (2026-08-30, pendant M4)

**Le relevé.** Sur un troisième ordinateur, écran 1920x1080, une session en résolution du client demande 1920x1200. Le journal écrit coup sur coup deux phrases qui se contredisent : « `\\.\DISPLAY1` dessine un bureau 1920x1200 réduit dans sa dalle », puis « `\\.\DISPLAY1` ne sait pas dessiner 1920x1200, donc il garde sa taille ». La seconde est la vraie. Windows avait répondu « c'est fait » sans rien faire.

**Deux causes, et la première est la nôtre.** L'interrupteur qui autorise un bureau plus grand que la dalle se pose écran par écran, et ce qu'un chemin d'affichage dit de lui-même est calculé **au moment où on le lit**. On le lisait avant de poser l'interrupteur : le chemin ne portait donc pas la marque qui autorise un bureau différent de sa dalle, et toute la demande était écrite dans des termes que Windows ignore poliment. Sur le portable, où l'interrupteur était déjà posé, cela ne se voyait pas. L'interrupteur est maintenant posé d'abord, et le bureau relu ensuite.

**La seconde est un mot de trop dans la demande.** On disait à Windows qu'il avait le droit d'ajuster ce qu'on lui demandait, et ce qu'il fait d'une demande qu'il ne peut pas satisfaire du tout est répondre « oui » en n'ajustant rien. On demande maintenant **exactement**, ce qui l'oblige à faire ou à dire pourquoi, et l'autorisation d'ajuster n'est donnée qu'en second recours, sur une demande déjà refusée telle quelle.

**Décision : sur cet appel, on relit toujours.** Ce que la fonction annonce est ce que l'écran dessine après coup, jamais ce qu'on lui a demandé. Une taille qui n'a pas été prise le dit avec les deux chiffres, et le journal nomme la raison : un écran qui ne sait pas porter de bureau différent de sa dalle, un chemin qui ne porte pas le bloc décrivant où le bureau se pose, ou, quand la demande portait tout ce que Windows réclame, une carte graphique qui ne dessine rien de plus grand que la dalle sur cette sortie. C'est la différence entre « cette machine ne peut pas » et « on a mal demandé », et c'est la première question de qui lit cette ligne.

**Et les deux corrections faites, la réponse est la troisième.** Sur ce PC, le chemin porte bien la marque et bien le bloc, la demande est faite exactement, et Windows répond oui en laissant l'écran où il est. Les deux machines qui marchent ont une carte Intel et une dalle interne ; celle qui ne marche pas a une carte d'un autre fabricant et un écran externe, dont la liste de modes ne contient rien au-delà de la dalle. Un bureau plus grand que la dalle n'est donc pas une chose qu'on obtient partout, et ce n'est pas la demande qui décide.

## D95. L'écran virtuel revient, en dernier recours et sans rien éteindre (2026-08-30, pendant M4)

**Le relevé, et il ferme la question de [D94](#d94-un-oui-de-windows-sur-laffichage-se-relit-il-ne-se-croit-pas).** Un PC dont la carte graphique ne dessine aucun bureau plus grand que sa dalle, quelle que soit la forme demandée : essai fait depuis un client 16:10 puis depuis un client 16:9, même refus. Sur cette machine, résolution de l'hôte et résolution du client donnaient la même image, et rien dans notre code ne pouvait y changer quoi que ce soit.

**Le mur est réel et il n'est pas le nôtre.** Pour qu'un client reçoive un bureau plus grand, il faut que le bureau de l'hôte soit plus grand ; si sa dalle ne sait pas l'afficher, la personne assise devant ne peut pas le voir. Le produit de référence a le même mur et s'en sort de la même façon : là où il ne peut pas étirer la dalle, il pose un écran que personne ne regarde.

**Décision : la machine qui ne peut pas est filmée sur l'écran qu'elle fait pousser.** C'est le second usage de cet écran, à côté de celui de [D91](#d91-on-ne-touche-pas-aux-écrans-de-quelquun-on-lui-rend-son-bureau) qui le réservait à la machine sans aucun écran branché.

**Et pendant cette session, c'est le seul écran de la machine.** Premier essai : les écrans physiques restaient allumés, personne n'éteignait rien, et le résultat a été jugé en trois mots. « On se retrouve avec 2 écrans et c'est à chier. » Il avait raison, et la raison est mécanique : un écran laissé allumé à côté n'est pas un écran neutre, c'est la seconde moitié d'un bureau que personne ne voit. Les fenêtres y atterrissent et disparaissent de la session, le pointeur sort par le bord de l'image et se perd, Windows range tout à cheval sur les deux. **Vu de loin, une machine à un écran doit ressembler à une machine à un écran**, quoi qu'il ait fallu faire ici pour y arriver ; une machine à deux écrans montre son écran principal. C'est la règle, et c'est celle du produit de référence.

Les écrans physiques sont donc éteints le temps de la session. Ce n'est pas ce que [D91](#d91-on-ne-touche-pas-aux-écrans-de-quelquun-on-lui-rend-son-bureau) refusait : ce qui était insupportable là-bas, c'était de les éteindre **sans les rendre**, sur une machine qui n'en avait aucun besoin, et de les voir revenir de travers au démarrage suivant. Ici la machine ne peut pas servir la session autrement, et Victor a posé la règle lui-même : « ZyrDesk fait ce qu'il veut quand je demande la session mais une fois la session finie il doit absolument tout remettre comme c'était avant. » Ce qui rend cela tenable n'est pas cette décision, c'est le relevé pris avant d'y toucher : chaque écran y est écrit, allumé ou éteint, et remis à la fin de la session ou au démarrage suivant du service si cet ordinateur n'a pas eu le temps de finir.

**Ce n'est appris qu'en essayant, et retenu.** Un écran qui a refusé une fois un bureau plus grand que lui refusera toujours, alors son nom est écrit à côté du service. Le moteur ne lit quel écran filmer qu'à son démarrage, et le redémarrer emporte le tunnel et avec lui toutes les sessions en cours : la bascule ne peut donc pas se faire au milieu d'une session. Elle se fait quand plus personne ne regarde. Concrètement, la première session qui découvre le mur est servie comme avant, et c'est la suivante qui profite de l'écran virtuel.

**Les garde-fous, parce que c'est la seule partie qui compte vraiment.** Le bureau ne déménage et rien ne s'éteint que si le relevé d'avant session existe : sans papier disant comment revenir, on ne touche à rien. Tout est écrit d'un coup et appliqué d'un coup, jamais moitié par moitié : la moitié d'un tel arrangement serait un ordinateur avec deux écrans l'un sur l'autre, ou pire, aucun. À la fin d'une session, **le bureau est remis d'abord et l'écran virtuel endormi ensuite**, dans cet ordre : l'inverse laisserait Windows décider où poser le bureau et l'arrangement remis un instant plus tard se battrait contre sa décision. Et un écran resté allumé par une exécution qui n'a pas fini est endormi au démarrage suivant du service, comme il l'était déjà.

## D96. Le journal d'un ordinateur se lit d'où l'on est (2026-08-30, pendant M4)

**Le besoin, dit par Victor.** « Pour faire l'aller-retour c'est hyper relou. » Une panne de session se diagnostique sur les deux journaux à la fois, celui de la machine qui regarde et celui de la machine regardée, et jusqu'ici le second se copiait en marchant jusqu'à elle. C'est exactement l'aller-retour qu'un bureau à distance existe pour supprimer, et c'était le dernier qui restait.

**Décision : un seul auteur du journal, deux lecteurs.** C'est le **service** qui rassemble la page, pas la fenêtre. La moitié de ce qu'un journal dit n'est connue que de lui : l'empreinte de la machine, ce qui empêche l'accès distant, la confiance au réseau local, les ordinateurs vus, les sessions ouvertes. Une fenêtre qui lirait les quatre fichiers toute seule aurait une page amputée précisément des lignes que personne ne peut recalculer.

Ce qui compte n'est pas l'économie de code, c'est que **la page lue de loin soit la même que la page lue sur place**. Deux versions de la même page, écrites à deux endroits, se seraient mises à diverger au premier ajout, et deux journaux qu'on ne peut plus comparer ligne à ligne ne servent à rien.

**Et quand le service se tait, la fenêtre rassemble ce qu'elle peut.** C'est justement le moment où l'on ouvre un journal. Elle écrit alors les quatre fichiers et le silence du service à la place des lignes manquantes, plutôt que de laisser un blanc ou de refuser la page.

**Qui a le droit de lire.** Ceux que cet ordinateur laisse déjà entrer, et personne d'autre. La question mérite d'être posée franchement, parce que c'est une permission accordée sans que la personne assise devant la machine soit consultée. La réponse est qu'elle est **plus petite que celle qu'ils ont déjà** : ces ordinateurs-là peuvent prendre l'écran, le clavier et la souris de cette machine. Une page de ce qu'elle a écrit est moins que cela. La lecture laisse en outre une ligne dans le journal de la machine lue, comme tout ce qu'un ordinateur distant demande ici.

**Vider se fait de loin aussi, et c'est une correction.** La première version de cette décision réservait le vidage à la machine sous la main, au motif qu'effacer le journal de quelqu'un qui est en train de le lire jetterait les lignes qu'il regarde. Victor a tranché en une phrase : « bah non faut pouvoir vider aussi hein », et il a raison, parce que l'argument était le mauvais.

Un journal ne se lit pas, il se compare. La manière dont on trouve une panne est : vider les deux journaux, refaire ce qui ne marche pas, lire les deux. Une page qui porte trois semaines de lignes sans rapport est une page que personne ne lit jusqu'au bout, et c'est justement pour ça que **Vider** existe. Pouvoir vider seulement celui des deux qui est sur le bureau où l'on se trouve laisse la marche jusqu'à l'autre machine exactement là où elle était, c'est-à-dire annule la moitié de ce que cette décision apporte. Le seul risque écarté était celui d'un geste de travers, et la double confirmation le couvrait déjà.

**Ouvrir le dossier reste chez soi**, en revanche : ces fichiers sont sur l'autre machine, et ouvrir une fenêtre sur le bureau de quelqu'un d'autre n'est pas ce qu'on demandait.

**Deux plafonds au lieu d'un, sur les deux canaux.** C'est le premier message du produit qui pèse une page et non une ligne, et les deux canaux avaient jusqu'ici une seule limite pour les deux sens. Un plafond protège **celui qui écoute de celui qui parle**, et les deux côtés ne sont pas exposés à la même chose : le service écoute n'importe quel programme que la personne peut lancer, un programme n'écoute que le service qu'il a appelé ; cet ordinateur prend des questions de tous ceux qu'il laisse entrer, et des réponses seulement de la machine où il est allé. Les questions gardent donc leur limite ancienne, et seules les réponses ont le droit de peser une page.

## D97. Le bouton flottant n'est pas transparent, il est découpé, et tout ce qui est blanc autour de lui vient de là (2026-08-30, pendant M4)

**Le symptôme, dit par Victor.** « Cet artefact blanc avec la croix par-dessus le FAB », qui apparaît presque à chaque fois après **Appliquer** sur un changement de résolution, plus « un effet aliasing autour ». Deux plaintes, une seule cause.

**Ce que le bouton est vraiment.** Sa fenêtre est demandée transparente, et l'une des couches sous la page refuse de l'être. Ce qui a été fait à la place est une **découpe** : la page mesure ce qu'elle dessine, le cœur taille la fenêtre à cette forme avec une région du système, et rien n'est dessiné hors d'une région. Ça marche, mais ça a deux conséquences qu'il faut regarder en face.

La première : **une région est un masque à un bit**. Elle n'a pas de demi-pixels. La page, elle, dessine des coins arrondis lissés. Le masque coupe donc la courbe en escalier, et là où il ne la coupe pas, les pixels à demi transparents du bord sont mélangés non pas à l'image derrière, mais **au fond de la fenêtre**, puisque la fenêtre est opaque. Ce n'est pas de l'aliasing : c'est un bord lissé posé sur le mauvais fond.

La seconde : **tout pixel que la page n'a pas encore peint montre ce fond**, et ce fond était blanc.

**Trois endroits fabriquaient du blanc, et c'étaient les trois plaintes.**

1. **La découpe demandait un redessin.** Poser une région en demandant le redessin fait effacer la fenêtre au pinceau de sa classe *avant* que quoi que ce soit d'autre arrive, et la vue web repeint quand elle peut. Ce qui se voyait entre les deux, c'était la forme elle-même remplie de blanc, c'est-à-dire un logo blanc posé sur l'image, et il restait là jusqu'à ce que quelque chose bouge. **Appliquer** est le pire moment possible pour ça : le menu se referme, donc la forme change, donc on découpe, à la seconde exacte où la machine est la plus occupée à relancer une session vidéo. La demande de redessin est retirée ; il y en avait déjà un juste après, écrit exprès pour ne pas effacer le fond.

2. **Le masque réclamait un pixel de plus que la page n'en peint.** La page rogne ses mesures vers l'intérieur, exprès, pour ne jamais réclamer une colonne qu'elle n'a pas peinte. Le cœur, écrit cinq jours plus tôt, rajoutait un pixel à droite et en bas de chaque morceau. Personne n'était revenu enlever le second en ajoutant le premier : les deux se battaient, et il restait un liseré clair sur deux bords.

3. **Le fond lui-même, et la demande de transparence qui l'empêchait d'exister.** La fenêtre demandait à être transparente, se le voyait refuser, et c'est exactement pour ça qu'elle est découpée : rien n'est dessiné hors d'une découpe, donc la découpe fait le travail que la transparence aurait fait. Ce que la demande continuait de faire, en revanche, c'était **empêcher qu'on donne une couleur au fond sous la page**. On ne la demande plus, et ce fond est maintenant peint du bleu très sombre du contour du logo. Le logo est cerné de cette couleur sur tout son tour, donc un bord lissé qui s'appuie dessus s'appuie sur lui-même et disparaît ; et un pixel pas encore peint devient le noir du logo au lieu d'un bloc blanc sur l'image de quelqu'un.

**Et le menu se referme avant de relancer, plus après.** « Appliquer » fait partir l'image et revenir, ce qui prend des secondes et met un écran de chargement à la place. Le menu attendait la fin pour se replier : il restait donc posé par-dessus ce chargement tout du long, la fenêtre découpée à sa taille à lui. Un refus le rouvre pour porter sa raison, qui vit dedans.

**Quatrième endroit, trouvé au journal : la découpe courait devant le dessin.** Le liseré blanc du bord gauche, pendant l'animation du survol et jamais au repos. Les nombres le disent sans ambiguïté : le bord gauche de la découpe passe de 1019 à 1013 pendant que le logo grandit, soit six colonnes découvertes. Le logo grandit depuis son coin haut droit, donc c'est bien par la gauche qu'il s'étend.

La page mesure dans le rappel d'animation, c'est-à-dire **avant que l'image mesurée soit peinte**, et la découpe est posée dans la foulée. Pendant au moins une image, elle montre donc six colonnes que la vue web n'a pas encore touchées.

Poser la découpe sur l'image d'avant plutôt que sur celle qui vient ne règle rien : ça déplace le défaut du grandissement au rétrécissement. **La règle juste est l'intersection des deux** : par morceau, le plus petit de ce qui vient d'être peint et de ce qui va l'être. Elle est toujours à l'intérieur de ce qui est peint, dans les deux sens. Elle coûte un ou deux pixels rognés sur un bord lissé tant que ça bouge, ce qui ne se voit pas, et rien du tout dès que c'est immobile, les deux dessins étant alors le même. Un morceau qui vient d'apparaître, la carte du menu par exemple, attend une image pour la même raison : personne ne l'a encore peint.

**Ce qui reste, et il faut le dire.** La vraie réponse à l'escalier des coins n'est pas une meilleure couleur de fond, c'est une transparence par pixel qui marche, ce qui veut dire aller voir pourquoi la couche du dessous la refuse. Tant qu'on découpe au masque, les coins seront durs. C'est le prochain travail sur ce bouton, pas celui-ci.

## D98. Un codec que la machine d'en face ne sait pas faire ne s'offre pas (2026-08-31, pendant M4)

**Le symptôme, dit par Victor.** Il choisit AV1 en sachant que sa machine hôte ne sait pas l'encoder. La session s'ouvre très bien, en HEVC, parce que les deux moteurs s'entendent entre eux sur ce qu'ils savent faire. Mais **AV1 reste coché dans le menu**, pour toute la session, et rien ne dit que ce choix n'a pas été honoré. « C'est un peu mal foutu. »

Il a raison, et c'est même le pire genre de défaut : rien n'échoue, rien ne se voit, et le menu ment tranquillement.

**Qui sait, et qui ne sait pas.** Le codec est choisi par l'ordinateur qui regarde et encodé par l'ordinateur regardé. Le second est **le seul** à savoir s'il en est capable : ça dépend de sa carte graphique, pas de la nôtre. Rien du côté qui choisit ne peut le deviner.

**Décision : on demande, on ne devine pas.** Une question de plus sur le canal du produit, posée pendant la session : « qu'est-ce que ton moteur sait encoder ? » L'hôte répond en lisant ce que **son propre moteur a écrit à son démarrage**, exactement comme pour les écrans (D93) : ce moteur essaie chaque encodeur que la machine pourrait avoir et note ceux qui ont répondu. Recalculer ça de notre côté voudrait dire recopier son idée de ce qu'une carte sait faire, et une copie pareille est fausse sur la première machine que personne n'a testée.

Ce qui n'est pas dans la réponse est barré dans le menu, avec le mot qui explique au survol. **Barré et pas effacé** : une possibilité qui disparaît d'un ordinateur à l'autre laisse croire à un menu qui change d'avis, là où c'est la machine d'en face qui n'a pas la même carte.

**Une réponse vide veut dire « il n'a rien dit », jamais « il ne sait rien faire ».** Hors session, ou pendant que son moteur démarre, la question n'a pas de réponse ; et un ordinateur qui n'encoderait rien ne pourrait pas être regardé du tout. Une question sans réponse laisse donc le menu exactement comme il était plutôt que d'en griser la moitié.

**« Automatique » n'est jamais hors de portée**, puisque c'est le choix de ne pas choisir. Et c'est déjà ce que le produit fait par défaut, ce qui répond à l'autre moitié de la demande : le réglage retenu est celui de la personne, et tant qu'elle n'a rien dit, c'est aux deux moteurs de s'entendre.

**Lequel des journaux du moteur, et c'est une correction.** La première version lisait `engine-console.log`, la sortie console que le service capte à côté du moteur. Elle n'a rien donné du tout, et pour deux raisons qui se cumulent. Ce fichier est **vide sur certaines machines** : sur le PC de Victor, la section « Le moteur hôte » du journal ne contient rien, alors que le moteur tourne parfaitement. Et c'est un **des quatre fichiers que le bouton « Vider » efface**, y compris depuis l'autre bout d'un tunnel depuis D96 : vider son journal aurait retiré la réponse jusqu'au redémarrage suivant du moteur.

Ce qui est lu est donc `engine.log`, **le journal que le moteur écrit lui-même**. C'est déjà celui d'où sortent les écrans (D93), il n'est pas rassemblé par le journal du produit, et personne ne l'efface. La règle qui en sort vaut pour tout ce qui viendra : **ce que le produit relit du moteur se relit dans le journal du moteur, jamais dans une copie que le produit tient à côté.**

## D99. On ne fournit une photo de sa propre fenêtre que quand Windows ne peut pas la prendre (2026-08-31, pendant M4)

**Le symptôme, dit par Victor.** Dans le Win+Tab du client, pendant une session, la vignette de ZyrDesk est nettement plus petite que celles de toutes les autres fenêtres, alors que la fenêtre est agrandie. Et pareil en plein écran.

**D'où ça vient.** Ce que Win+Tab, Alt+Tab et la barre des tâches montrent est une photo que le système prend d'une fenêtre. ZyrDesk lui disait de ne pas la prendre : il levait les deux attributs qui veulent dire « cette fenêtre fournit elle-même son image », et répondait au message par lequel le système la réclame.

C'était juste au moment où ça a été écrit. L'image de la session est dans une fenêtre à elle, posée par-dessus la nôtre ; le système photographiant **une** fenêtre, il ramenait la page d'accueil que la session cache. Fournir l'image nous-mêmes était la seule réponse.

Ça a cessé d'être vrai. Depuis que l'image est **portée** par notre fenêtre pendant toute la session, c'est-à-dire adoptée comme fenêtre fille, elle est dessinée dans notre composition à nous : la photo du système la contient déjà, et de plein droit. L'ancien réflexe est resté, et il coûtait exactement ce que Victor voit. Une fenêtre qui fournit son image n'est plus jamais photographiée en direct, et cette image ne peut pas dépasser la taille que le système réclame dans son message, laquelle est bien plus petite que la carte que Win+Tab dessine. La vignette était donc plafonnée à cette taille-là, quelle que soit la taille de la fenêtre.

**Décision.** La question « qui photographie la session ? » se pose à un seul endroit et se lit sur un seul fait : est-ce que l'image est portée par notre fenêtre ? Si oui, le système prend sa photo lui-même, et il la prend à la taille qu'il veut. Si non, ZyrDesk fournit la sienne comme avant.

Le « si non » n'est pas théorique et c'est pour ça qu'il reste : Windows peut refuser l'adoption, ce qui arrive quand les deux fenêtres ne mesurent pas l'écran de la même façon, et le journal le dit quand ça arrive. Sur une machine comme celle-là, la session est de nouveau deux fenêtres posées l'une sur l'autre, et la photo du système redeviendrait fausse. La règle vaut donc pour les deux cas au lieu d'en supposer un.

**Ce qu'on en retient.** Une réponse au système qui remplace ce qu'il sait faire tout seul se paye toujours quelque part, ici en taille. Quand la raison qui l'a justifiée disparaît, ce n'est pas neutre de la laisser : c'est un défaut qui vieillit tout seul.

## D100. La session dit quel écran de l'hôte elle regarde, et celui qui ne dit rien regarde le principal (2026-08-31, pendant M4)

**La demande de Victor.** Son PC a deux écrans allumés et une télé éteinte en troisième. Il veut pouvoir basculer entre les deux écrans allumés depuis le bouton flottant, et il veut que par défaut ce soit **l'écran principal** qui soit pris, pas le secondaire.

**Le fait qui commande tout le reste.** Le moteur de l'hôte lit quel écran filmer **une seule fois, à son démarrage**. Il n'y a pas de moyen de le lui redire en cours de route : ce n'est pas un choix de conception de notre côté, c'est ainsi que le moteur est fait. Changer d'écran veut donc dire redémarrer ce moteur, et redémarrer ce moteur emporte le tunnel, donc la session.

Le moteur sait bien basculer d'écran en direct, par une frappe réservée venue du client, mais il le fait **par numéro dans l'énumération de la carte graphique**, et cette énumération est précisément celle dont D93 dit qu'elle n'est pas fiable : elle change d'un appel à l'autre, et un écran en train d'être redimensionné en sort. Ce chemin-là a donc été écarté.

**Décision : la session porte l'écran, comme elle porte déjà la taille, le débit et le codec.** À chaque ouverture, elle dit à l'hôte de quel écran elle veut être servie ; rien de nommé veut dire son écran principal. L'hôte l'écrit, et son moteur redémarre si ce n'est pas celui qu'il filme.

Il **le dit** plutôt que de laisser l'autre bout le découvrir sur une voie qui casse : la réponse est soit « j'y suis déjà », soit « je redémarre ». Sur la seconde, le client lâche la voie, laisse au moteur d'en face le temps de revenir, rouvre une voie et repose la question, jusqu'à huit fois. L'écran d'ouverture le dit à la personne, parce que c'est plusieurs secondes qu'on lui a fait attendre.

La session ordinaire ne fait jamais ce tour-là : elle demande l'écran principal, qui est celui que l'hôte filme déjà, et la réponse arrive du premier coup.

**Pourquoi l'écran choisi ne va pas dans les réglages.** Il nomme **un** écran d'**un** ordinateur : l'identifiant est une empreinte que le moteur de cette machine-là est seul à calculer. Écrit dans les préférences, il partirait demander à une autre machine un écran qui n'est pas le sien. Il vit donc le temps de la session, et une nouvelle session s'ouvre sur l'écran principal d'en face.

**Et c'est ça, la réponse à « que ça prenne l'écran principal par défaut ».** Une session qui ne dit rien demande l'écran principal, donc une machine laissée sur un écran secondaire par une session précédente revient au principal à la session suivante, en payant un redémarrage de moteur que le journal explique. Le service oublie aussi ce choix à son démarrage : un ordinateur qui redémarre ne doit pas revenir en servant l'écran que quelqu'un avait choisi la semaine d'avant, sans personne pour le voir.

**Ce qui est offert et ce qui ne l'est pas.** Seuls les écrans sur lesquels la machine affiche vraiment. Un écran éteint, la télé de Victor par exemple, ne montrerait qu'une image noire ; et l'écran que le produit fait pousser n'est un écran pour personne assis devant la machine. Une machine à un seul écran n'a pas de choix à offrir : la ligne du menu ne s'affiche pas du tout, plutôt que de proposer de choisir entre une chose et elle-même.

**Et la liste vient de la machine d'en face, comme les codecs (D98) et les écrans (D93).** C'est son moteur qui nomme ses écrans, avec une empreinte que rien d'autre ne calcule pareil. Une liste vide veut dire « il n'a rien dit », jamais « il n'a pas d'écran ».

## D101. La transparence du bouton flottant n'était pas refusée, elle était demandée à moitié (2026-08-31, pendant M4)

**Le symptôme, dit par Victor.** Sur un fond blanc, la bordure noire autour du logo du bouton flottant est « toute saccadée », pas lisse. C'est l'escalier que D97 avait nommé et laissé ouvert.

**Pourquoi aucune couleur de fond ne peut y répondre.** La fenêtre du bouton est découpée à la forme que la page dessine, et une découpe du système est **un masque à un bit par pixel**. Elle n'a pas de demi-pixel. Une fenêtre opaque a donc un bord dur, quelle que soit sa couleur : le contraste change, la marche reste. La seule réponse est que les pixels à demi couverts se mélangent à l'image derrière plutôt qu'à un fond, c'est-à-dire une vraie transparence par pixel.

**Ce qui manquait, et c'est établi et non deviné.** La boîte à outils demande la transparence en disant au compositeur d'honorer l'alpha de chaque pixel, avec une région de flou vide. C'est tout ce qu'elle fait. Deux choses le disent :

- La **documentation du compositeur** elle-même prévient que « certaines opérations GDI ne préservent pas l'alpha, donc attention aux fenêtres filles, dont l'alpha est imprévisible ». Notre fenêtre est un cadre qui porte une vue web, laquelle est exactement une fenêtre fille.
- Les autres boîtes à outils qui ont eu cette panne l'ont corrigée **au même endroit** : il faut en plus que la fenêtre se déclare *layered* et annonce une opacité constante de 255, ce qui est la façon documentée de dire « prends l'alpha de chaque pixel et pas un seul nombre pour tous ».

**Décision.** On redemande la transparence, et on ajoute nous-mêmes la moitié manquante juste après la construction de la fenêtre, au même endroit où ses autres styles sont posés. C'est deux appels, sur notre propre fenêtre, comme les coins arrondis le sont déjà.

**Et la couleur de fond s'en va avec, ce n'est pas un oubli.** Les deux semblent s'exclure : la boîte à outils peint cette couleur sur toute la fenêtre avant que la page soit dessinée, donc une fenêtre qui en aurait une serait de cette couleur et de rien d'autre. Ce qu'elle achetait paraît disparaître sans rien coûter : un pixel que la page n'a pas encore peint ne montrerait plus du blanc mais rien, le fond de la vue web devenant transparent en même temps.

**Cette phrase est fausse sur ses deux moitiés, et c'est elle qui a coûté le plus cher.** Voir « le fond noir pur » plus bas : les deux ne s'excluent pas, et un pixel que personne n'a peint ne montre pas rien.

**Ce qui reste découpé, et pourquoi.** La découpe n'est pas retirée. Elle ne sert plus à faire le bord, elle sert à laisser passer les clics : hors de la forme, la souris atteint l'image de la session.

**Et la découpe arrondit maintenant vers le dehors, ce qui est la seconde moitié.** Victor : « c'est mieux, mais la bordure noire n'est pas du tout homogène, elle est plus épaisse à gauche. » Le journal confirme au passage que la transparence a bien pris : aucune ligne de refus.

La mesure arrondissait chaque morceau **vers l'intérieur**, et pas de la même quantité sur les quatre bords : l'origine était remontée d'une fraction de pixel et la taille rabotée d'une autre, donc le bord opposé bougeait de la somme des deux. Un pochoir n'ayant pas de demi-pixel, ça rognait le bord lissé d'un pixel d'un côté et de deux de l'autre, et le contour ressortait épais ici et fin là. Sur fond blanc, ça se voit.

Chaque bord est désormais arrondi vers le dehors, et les quatre de la même façon : la découpe contient tout ce que la page a dessiné, donc le contour qu'on voit est celui que la page a peint, avec son lissage, partout pareil. Ce que ça réclame en plus est une frange d'un pixel que personne n'a peinte, et **c'est précisément ce qui ne coûtait rien qu'une fois la transparence acquise** : cette frange était blanche avant, c'était le liseré de D97, et elle n'est plus rien du tout. Les bords sont arrondis un par un et non l'origine puis la taille, sinon l'erreur se cumule sur le bord opposé et la bordure redevient inégale dans l'autre sens.

**Troisième et dernière prudence tombée pour la même raison : la découpe ne reste plus une image en arrière.** Victor : « au survol ça zoome sans la bordure noire, et c'est pas super lisse quand même. »

C'est la règle de l'intersection posée par D97, qui découpait sur le plus petit du dessin d'avant et du dessin qui vient. Elle existait pour la même raison que les deux autres : la page mesure dans le rappel d'animation, donc avant que l'image mesurée soit peinte, et un pixel réclamé trop tôt était blanc.

Ce que D97 estimait qu'elle coûtait, « un ou deux pixels rognés pendant que ça bouge », était faux d'un ordre de grandeur. Le logo grandit de six pour cent au survol depuis son coin haut droit : sur un logo de soixante-dix-sept pixels, ça fait près de cinq pixels, et ils tombent entièrement sur le bord gauche et le bord bas. La découpe coupait donc en plein milieu du dessin pendant toute l'animation : elle enlevait le contour noir de ces deux bords et le remplaçait par l'escalier franc du pochoir. C'est exactement les deux plaintes en une.

Elle est retirée : la découpe suit le dessin qui vient. Un pixel réclamé une image trop tôt ne montre plus rien, ce qui ne se voit pas, là où une bordure absente sur deux côtés se voit tout de suite.

**Ce qu'il faut retenir des trois.** La fenêtre était opaque, donc chaque prudence évitait du blanc et coûtait de la netteté ; personne ne pouvait payer le second prix tant que le premier existait. La transparence acquise, les trois prudences n'achetaient plus rien et ne coûtaient plus que leur prix. Elles sont tombées dans l'ordre où Victor les a vues, ce qui est la seule façon honnête de les enlever : une à la fois, en regardant.

**Ce qui restait après les trois, et c'est de l'arithmétique, pas une prudence.** Victor : « au premier survol ça refait pareil et après c'est bon, et sans survol il y a toujours ce tour pixelisé ». Deux défauts encore, sans rapport l'un avec l'autre.

Le premier survol : le suivi du dessin s'arrête dès que deux images de suite dessinent la même forme, ce qui est juste au repos et faux au démarrage d'une animation. Entre la souris qui entre et la transition que le navigateur crée vraiment, il passe une image ou deux où rien n'a encore bougé ; le suivi s'y arrêtait, et la découpe restait celle du repos pendant tout le grandissement. Les survols suivants trouvaient le style déjà calculé et la transition démarrée à l'image d'après, d'où « et après c'est bon ». Le suivi écoute maintenant les événements de transition eux-mêmes, qui disent exactement quand elle commence et quand elle finit.

Le tour pixelisé au repos : arrondir vers le dehors laisse **zéro** de marge quand un bord tombe pile sur un pixel, et ce zéro se paye dans les coins. Un coin arrondi n'est pas un bord : au plus loin de l'angle, un rayon r ne dépasse de sa boîte que de 0,29 r, et le rayon de la découpe est arrondi au pixel alors que celui du dessin ne l'est pas. La marge y tombait au dixième de pixel, donc sous le lissage, donc le pochoir coupait dedans. La découpe prend désormais **un pixel plein** au-delà de l'arrondi, et son rayon est arrondi vers le bas plutôt qu'au plus proche, un coin moins rond débordant de sa boîte là où un coin plus rond y rentre. La marge du coin ne peut donc plus être plus petite que celle des bords.

**Et le journal dit ces nombres-là**, morceau par morceau : le dessin en vrais pixels non arrondis, la découpe, les quatre marges et celle du coin. Une marge négative est le pochoir qui coupe dans le dessin, et c'est la seule chose qu'on ne pouvait pas lire jusqu'ici, puisqu'elle se joue sur des fractions de pixel que les nombres arrondis avaient déjà perdues. La fenêtre dit aussi, une fois à sa construction, les styles qu'elle porte vraiment et l'alpha relu : poser un style et l'avoir ne sont pas la même phrase, et la différence entre les deux est toute la différence entre un bord lisse et un bouton posé sur une plaque.

**Et une leçon sur le débogage lui-même, payée sur le premier journal.** Ces lignes étaient écrites à chaque découpe, donc dix fois par seconde tant qu'une main restait sur le bouton. Un journal garde les cent vingt dernières lignes d'un fichier : douze secondes de survol ont chassé tout le reste, **y compris la ligne des styles**, dans le seul journal qui avait été rassemblé pour la lire. Un débogage qui noie sa propre réponse ne débogue rien. Première correction : dites une fois par session, plus chaque fois qu'une marge passait **sous le pixel**. Elle ne valait rien non plus, et pour une raison bête : au repos la marge du coin vaut 0,93, donc ce cas-là est permanent, donc la ligne revenait à chaque découpe comme avant. **Un seuil qui est franchi en permanence n'est pas un seuil.** Le seul nombre qui veuille dire « le pochoir coupe dans le dessin » est une marge **négative**, et c'est celui-là qui réveille le journal, plus une fois toutes les lignes à la construction du bouton. Et par-dessus, dans les deux cas : jamais deux fois les mêmes nombres, un morceau qui n'a pas bougé n'ayant rien à ajouter. La ligne de la découpe elle-même suit la même règle, sur le nombre de morceaux, la taille de la fenêtre et les refus.

**Ce que les nombres ont répondu.** Toutes les marges entre 1,0 et 2,0, dans les coins entre 0,82 et 1,6 : **le pochoir ne mord plus nulle part**. Ce qui reste n'est donc pas la découpe.

**Et ils ont répondu autre chose, que personne ne cherchait.** Victor : « c'est bizarre, de temps en temps il se met nickel et après il rechie. » Une géométrie ne va pas et vient : cinquante-cinq pixels sont cinquante-cinq pixels. Ce qui va et vient, dans le journal, c'est **la taille du dessin lui-même**, morceau par morceau : 43,16 quand le bouton est appuyé, 47,17 au survol, et jamais 44,50, qui est la valeur au repos. Rapportées à 44,50, ces trois-là sont exactement 0,97, 1,06 et 1,00, c'est-à-dire les trois échelles du survol et de la prise en main.

Or **une mise à l'échelle CSS ne redessine rien**. Le navigateur rasterise le logo une fois, à sa taille, et le compositeur étire ensuite cette image. Un dessin de cinquante-cinq pixels réels rendu à cinquante-huit ou à cinquante-trois est donc **rééchantillonné**, ce qui est précisément un tour qui n'est pas net. Et comme il ne l'est que quand l'échelle n'est pas un, ça donne mot pour mot « de temps en temps c'est nickel et après ça rechie » : net quand la souris est ailleurs, mou dès qu'on l'approche ou qu'on appuie.

**Le bouton a donc grandi par sa taille et non par une mise à l'échelle.** Changer la taille refait la mise en page et redessine le dessin vectoriel à la taille voulue, là où l'échelle étirait une image déjà faite. Le coin d'ancrage était déjà donné par le bloc, qui est collé en haut à droite et aligne ses enfants à droite.

**La règle qui en sort, et elle vaut pour tout ce produit.** Un dessin vectoriel qu'on anime s'anime par sa taille. Une mise à l'échelle est une loupe posée sur une photo : elle est gratuite pour le compositeur et elle coûte la netteté, ce qui est exactement le mauvais côté du marché pour une marque de cinquante pixels posée sur le travail de quelqu'un.

**Et redessiner net ne sert à rien si on redessine à côté de la grille.** Victor : « un peu mieux, mais là sur la droite c'est pas lisse. »

Cette fois la réponse a été **mesurée et non raisonnée** : le logo a été dessiné pour de vrai dans un navigateur, à 125 %, sur fond blanc, et les pixels de ses bords ont été comptés. Un bord est franc quand le premier pixel peint est déjà le noir plein du contour ; il est gris quand c'est une teinte intermédiaire. Coins arrondis exclus, puisque là le dégradé est le dessin lui-même :

| Taille du logo | Bords droits d'un noir franc |
|---|---|
| 55,00 px (repos) | 52 % |
| 53,35 px (appuyé, -3 %) | 28 % |
| 58,30 px (survol, +6 %) | **2 %** |

**La taille de repos n'est pas une taille parmi d'autres, c'est la bonne.** 55 sur les 440 du dessin tombent juste : un huitième. Chaque nombre du dessin y atterrit donc sur un pixel entier, un demi ou un quart, et deux de ses quatre bords verticaux tombent pile sur la grille. À 58,30 les mêmes nombres tombent n'importe où, et un trait noir qui s'arrête au milieu d'un pixel y est gris. Le grandissement du survol coûtait donc très exactement ce qu'on lui demandait d'apporter.

**Trois échappatoires ont été essayées et mesurées, aucune ne tient.**

- *Caler la boîte du bouton sur un pixel entier.* Déjà le cas, et sans effet : décalée d'un demi-pixel, elle échange simplement les bords francs contre les autres (58 bords francs contre 48), et tout décalage vertical la fait chuter à 12.
- *Arrondir la taille du survol à un pixel entier*, 58 au lieu de 58,30. On remonte un peu, on reste très loin du repos : ce qui compte n'est pas que la boîte tombe sur la grille, c'est le **rapport** de la taille aux 440 unités du dessin.
- *Caler le dessin lui-même* sur des multiples de 8 unités. Ça marche, et bien : 62 % de bords francs au repos au lieu de 52. Mais uniquement à 125 % avec un bouton de 44 points. Pour que tous les nombres d'un dessin tombent juste à 100, 125, 150, 175 et 200 % à la fois, chacun doit être un multiple d'un **onzième** de la boîte, ce qui pour un contour d'un quinzième est impossible. Ce serait un réglage pour une machine, donc un bricolage.

**Le grandissement a donc été retiré, et il a été remis le lendemain.** Victor : « c'est encore pire, il n'y a plus l'animation au hover. » C'était son bouton et son choix, il l'a fait, et le mesuré ne le remplace pas : la netteté du contour et le retour sous la souris sont deux qualités, et laquelle vaut le plus n'est pas une question qui a une réponse dans un fichier de nombres.

**Et le retirer avait un second coût, que personne n'avait vu.** Cette animation était aussi, sans que ça ait jamais été décidé, **la seule chose qui faisait redessiner ce bouton** entre deux ouvertures du menu. Le cœur ne redécoupe que quand la forme change, parce qu'une découpe n'est ni gratuite ni silencieuse : le système redessine la fenêtre à chaque fois. Le survol faisait donc changer la forme dix fois par seconde, donc dix redessins, et tout ce qui pouvait traîner sur ce bouton était balayé au passage. Sans animation, plus rien ne bouge tant que le menu n'est pas ouvert, et ce qui apparaît reste. Le journal du produit le disait déjà, à propos d'un autre défaut : la trace « restait là jusqu'au survol suivant ».

**Ce qu'il faut en retenir, et c'est la vraie leçon de ce tour-ci.** Une animation qui n'existait que pour l'oeil tenait aussi un rôle que personne ne lui avait donné. Enlever quelque chose de décoratif n'est jamais seulement enlever de la décoration tant qu'on n'a pas cherché ce qui s'appuyait dessus.

**Ce qui reste vrai malgré tout**, et il faut le dire : sur le portable de Victor, à 125 %, le logo entier fait **55 pixels réels** et son contour **3,5**. Un trait de trois pixels et demi ne peut pas s'arrêter net des deux côtés : il fera toujours trois pixels pleins et un demi. Le repos reste le mieux que ce dessin puisse donner à cette taille. Si ça ne suffit pas, la suite n'est plus une correction : c'est un choix de taille de bouton ou de dessin, et c'est à Victor de le dire.

**Et la croix, enfin nommée, parce qu'on a fini par regarder ses pixels.** Victor l'a montrée depuis juillet et elle a été expliquée deux fois, différemment, sans jamais être mesurée. La capture est désormais rangée dans le dépôt, à `docs/testing/captures/bouton-flottant-croix.png`, parce qu'elle est pénible à reprendre : l'outil de capture de Windows la fait disparaître le temps de son propre voile.

L'or du logo vaut `239,181,54` dans le dessin. Dans la capture il vaut `131,99,30` là où le fond derrière est noir, et `228,201,139` là où il est clair. 131 / 239 = **0,548**, et le second cas le confirme. C'est l'opacité au repos, `0.55`, écrite dans la feuille de style.

**Première conclusion, et elle était fausse : « c'est l'écran d'en face vu au travers ».** Victor l'a refusée tout de suite, et il avait raison : « ça me le fait sur les trois PC exactement pareil, donc il y a forcément quelque chose qui amène cette cochonnerie ». Trois machines qui montrent la même chose ne regardent pas un contenu, elles montrent un défaut.

**La preuve tient en une phrase, et elle était sous les yeux depuis le début.** La chose claire **s'arrête pile au bord du logo**, et le noir tout autour d'elle est pur. Or ce qui serait derrière la fenêtre se verrait aussi à côté du bouton, la capture étant deux fois plus large que lui. Donc elle n'est pas derrière la fenêtre : elle est **dedans, sous la page**.

**Ce qu'il y a dessous, obtenu en retirant le logo par le calcul.** Chaque pixel de la capture vaut 0,55 fois le dessin plus 0,45 fois ce qu'il y a dessous ; le dessin est connu, donc le dessous se calcule. Ce qui sort est sans ambiguïté : **une barre de titre claire avec un bouton de fermeture**, carré blanc et croix noire, et la zone client sombre en dessous. C'est le cadre qu'un système peint dans une fenêtre à sa naissance.

**Et c'est bien cette décision-ci qui l'a révélé.** Tant que la fenêtre était opaque, la boîte à outils repeignait son fond sur toute sa surface avant chaque dessin de la page, donc ce cadre était couvert et personne ne l'avait jamais vu. Devenue vraiment transparente, elle ne repeint plus rien, et un logo à 55 % ne couvre pas ce qu'il y a dessous : il s'y mélange. C'est exactement la mesure.

**Et ça explique les deux détails que personne n'arrivait à relier.** « Ça part quand j'approche la souris » : le survol rendait le logo entier, donc opaque, donc il couvrait le cadre. « La capture d'écran ne la prend pas » : le voile de l'outil de capture fait la même chose.

**Décision : le bouton est franc en permanence.** Le fondu au repos couvrait mal quelque chose qui ne devrait pas être là, et il n'y avait pas de demi-mesure possible : s'effacer, c'est se mélanger à ce qu'il y a dessous, quel que soit ce dessous. Opaque, le logo couvre le cadre entièrement, ce que le survol prouvait déjà sur les machines de Victor. Le bouton reste discret par sa taille, et le survol continue de le faire grandir.

**Et le journal dit désormais le style ordinaire de cette fenêtre**, à côté de ses styles étendus : barre de titre, menu système, bordure, cadre redimensionnable. C'est la moitié qui manquait pour nommer le peintre de ce cadre, et couvrir n'est pas guérir : tant qu'on ne sait pas qui le peint, il est là.

La lecture de la boîte à outils dit déjà quoi attendre : elle crée toute fenêtre avec `WS_CAPTION | WS_CLIPSIBLINGS | WS_SYSMENU` et, sans décorations, n'enlève que `WS_CAPTION` et `WS_THICKFRAME`. **Le menu système reste**, et `WS_EX_WINDOWEDGE` avec lui, ce que les styles étendus déjà relevés confirment au bit près. La ligne dira si un cadre y est resté pour autant.

**Et la même règle a servi une troisième fois, pour que cette ligne-là arrive.** Le premier journal envoyé après la correction ne la contenait pas : elle avait été chassée par trente lignes « le premier plan passe à ZyrDesk » écrites d'affilée, le système envoyant cet avis à chaque passation, y compris celles qui rendent le premier plan à qui l'avait déjà. Une douzaine de clics sur le bouton suffisaient. Elle ne se dit plus que quand la fenêtre au premier plan change vraiment.

**Et c'est Victor qui a donné le dernier mot, sans le savoir : « ça n'apparaît que quand on clique dessus, ça fait un flash ».** Un clic est très exactement le moment où le système repeint le cadre d'une fenêtre, puisque c'est là qu'elle change d'activation. Le cadre n'est donc pas peint une fois à la naissance et oublié : il est **repeint à chaque activation**, dans la fenêtre elle-même, sous une page qui ne le couvre pas. Le logo opaque cachait le résultat au repos ; le flash est le même cadre pris entre le coup de pinceau du système et celui de la page.

**Décision : cette fenêtre n'a plus de cadre à peindre.** Là où ses styles sont déjà repris pour la transparence, on lui retire tout ce qui lui en donne un : barre de titre, bordure, cadre de dialogue, cadre redimensionnable, menu système, boutons réduire et agrandir, et les quatre bords relevés côté styles étendus. Un style n'est relu qu'au recalcul du cadre, donc on le demande dans la foulée, sans bouger ni retailler ni réactiver la fenêtre. Un bouton découpé à la forme d'un dessin n'a pas de cadre à montrer, donc on ne lui en donne pas à peindre.

Ce n'est pas une supposition : la boîte à outils a été lue, elle bâtit toute fenêtre avec `WS_CAPTION | WS_CLIPSIBLINGS | WS_SYSMENU` et n'ôte que `WS_CAPTION` et `WS_THICKFRAME` quand on demande « sans décorations ». Les styles étendus relevés dans le journal de Victor, `0x80c0190`, portent `WS_EX_WINDOWEDGE` au bit près. Le journal dira désormais quatre « false » si la reprise a pris.

**Et le journal a répondu quatre « false » et l'artefact était toujours là.** `style ordinaire 0x4000000`, styles étendus passés de `0x80c0190` à `0x80c0090` : la reprise a bien pris, la fenêtre n'a plus rien qui lui donne un cadre, et ça n'a rien changé. La piste était fausse. Elle reste juste sur un point, et c'est le seul qu'on garde : une fenêtre découpée sur un dessin n'a pas de cadre à montrer, donc on ne lui en laisse pas.

**Le fond noir pur, qui est la vraie réponse, et elle était dans la boîte à outils depuis le début.** Cette boîte à outils ne demande pas au système une fenêtre transparente. Elle allume **le flou derrière la fenêtre sur une région vide**, qui est le vieux tour dont toute la règle tient en une phrase : **un pixel peint en noir pur y devient entièrement transparent**.

Or, sans couleur de fond, elle **n'efface rien du tout**. Son traitement de l'effacement se lit en douze lignes : s'il y a une couleur, elle remplit toute la zone client avec ; sinon elle passe la main, et la classe de la fenêtre n'a pas de pinceau. Ce qui n'est jamais effacé, c'est **la mémoire tampon de la fenêtre**, que personne n'a nettoyée et qui contient ce qui s'y trouvait avant. C'est ça, la croix : de la mémoire jamais peinte, montrée là où la page ne peint pas.

**Et ça explique le clic, ce que ni le fondu ni le cadre n'expliquaient.** Ouvrir le menu **retaille la fenêtre**, une fenêtre qui grandit fait grandir ce tampon, et la bande neuve n'a jamais été peinte par personne. D'où un artefact qui apparaît au clic et repart à l'image suivante, quand la page a fini de peindre par-dessus.

**Décision : la fenêtre reçoit un fond, et ce fond est le noir pur.** La boîte à outils efface alors toute la zone client à chaque fois, et chacun de ces pixels est transparent puisqu'il est noir pur. Rien du dessin de ce produit n'est noir pur, le contour du logo étant 9,13,22, donc rien de ce qu'on peint n'est rendu translucide au passage. La tentative de la première moitié de cette décision avait pris justement ce 9,13,22 comme fond et conclu que les deux s'excluaient : il n'était pas assez noir, donc il restait opaque, donc le bouton se retrouvait posé sur une plaque. **Quatre unités de rouge séparaient une plaque d'une fenêtre propre.**

L'alpha vaut zéro et il est lu : la couche fenêtre l'ignore et prend le noir, la couche vue web l'honore et reste transparente. C'est écrit dans la documentation des deux couches.

**Et ça n'a rien changé non plus, ce qui fait trois.** L'écran d'en face vu au travers, le cadre de la fenêtre, le tampon jamais effacé : trois réponses raisonnées, trois fois faux. Chacune des trois a été plaidée à partir d'une **capture d'écran**, et une capture d'écran d'une fenêtre découpée ne montre que ce que la découpe laisse passer. Ce que personne n'a jamais regardé, c'est **la fenêtre elle-même**, la part que la découpe cache comprise, qui est pourtant le seul endroit d'où l'artefact puisse venir.

**Décision : le bouton se photographie.** À chaque fois que sa fenêtre change de taille, qui est le moment où l'artefact se montre, le coin haut droit de cette fenêtre est écrit en image à côté du journal, découpe non appliquée. Huit au plus par session, et le journal dit où. Le seul appel de copie qui atteigne une fenêtre dessinée sur la carte graphique est déjà employé dans ce produit pour la vignette de session : c'est la même surface qui sert, rendue commune plutôt que réécrite.

Ce n'est pas une correction, c'est un instrument, et c'est ce qui manquait depuis le début : **trois raisonnements de suite valent moins qu'une mesure.**

**Et l'instrument a répondu à la première session.** La septième photo, prise à l'instant où le menu s'ouvre, est gardée dans le dépôt à `docs/testing/captures/bouton-flottant-decoupe-decalee.png`. On y voit, sans rien avoir à déduire : **la carte du menu dessinée dix-huit pixels à gauche de la forme gardée pour elle.**

**Ce que ça veut dire.** Les morceaux de la forme sont comptés depuis le bord droit de la fenêtre, et la page les compte dans la fenêtre **telle qu'elle est**. Le cœur, lui, découpait contre la taille que la fenêtre **allait avoir**, au motif qu'une forme plus large que sa fenêtre est simplement rognée par elle et que ça ne coûtait donc rien. Ça coûtait dix-huit pixels : ouvrir le menu élargit cette fenêtre d'autant, et découper contre la nouvelle largeur pose toute la forme dix-huit pixels à droite de ce qui est réellement peint. Une bande de fenêtre que personne n'a peinte apparaît le long d'un bord de la carte, et le logo est rogné de l'autre. **C'est le flash blanc, et c'est ici qu'il se fabrique.**

**Décision : on découpe contre la taille que la fenêtre a, pas contre celle qu'elle prend.** Une image de l'ancienne forme sur l'ancien dessin est exactement ce qu'on veut, puisque les deux vont ensemble. La page redemande à l'image suivante, mise en page à la nouvelle largeur cette fois, et c'est celle-là qui est découpée contre la nouvelle largeur, la fenêtre l'ayant alors pour de bon.

**Et la leçon sur l'instrument.** Une capture d'écran d'une fenêtre découpée ne montre que ce que la découpe laisse passer, c'est-à-dire précisément la partie qui a l'air juste. Trois explications ont été bâties sur ces captures et les trois étaient fausses. La première photo prise de l'intérieur a suffi.

**Et un refus se dit.** Si Windows refuse ces deux appels, le journal l'écrit. C'est la différence entre un bouton au bord lisse et un bouton posé sur une plaque, et rien d'autre à l'écran ne dirait lequel des deux on regarde.

**Et ça n'a rien changé non plus, ce qui fait quatre.** La correction est juste en elle-même, la photo la prouve, et l'artefact est resté. Une cinquième explication a suivi, tirée du blanc de la carte du menu : le bouton serait dans le mauvais thème. Victor a répondu « claire ». Le blanc était donc la bonne couleur, et ça faisait cinq.

**L'instrument ne pouvait pas répondre, et c'est de sa faute.** `PrintWindow` ne copie rien : il **demande à la fenêtre de se redessiner**, et une fenêtre qui se redessine dessine ce que la page dit, qui est juste par définition. Deux archives de photos sont revenues vides pour cette seule raison, et ce n'était pas une réponse rassurante, c'était un instrument aveugle au défaut qu'il devait montrer. Le défaut n'est pas dans ce que la page dit. Il est dans ce qui arrive à l'écran, et l'écran est la seule surface où le compositeur, la découpe et la page sont déjà additionnés.

**Alors on a mesuré l'image que Victor a filmée, et elle porte deux choses dont une seule est un défaut.** La tache blanche ronde posée sur le coin bas droit de l'écran or est **le pointeur de la souris**, la main fermée que la page demande sur le logo pendant qu'on le tient. Victor l'avait déjà dit d'une capture précédente, et c'est aussi pourquoi aucune photo de la fenêtre ne l'a jamais montrée : une copie de fenêtre ne prend pas le pointeur. Cette tache-là a coûté une session entière à courir après.

**Le défaut est un liseré pâle le long du bord gauche, dehors, avec du noir pur au-delà**, et il suit toute la silhouette gauche du dessin, les deux écrans et l'arrondi entre eux compris. Sur une ligne au milieu de l'écran or : noir jusqu'à 1,5,10, puis 117,122,129, puis **203,209,216**, puis 115,121,129, puis le contour du dessin à 9,13,20. Large de deux pixels, dehors, collé au dessin.

**Et la même marge sur les trois autres bords est noire** : 5,8,15 en haut, 6,9,15 à droite, 8,11,15 en bas, sur la même image. La page rendue telle quelle dans un navigateur, au même agrandissement et en thème clair, n'a ce liseré nulle part : il ne vient donc pas de ce qu'elle dessine.

Cette marge est celle que la page prend au-delà de ce qu'elle peint pour que la découpe ne morde jamais dans le bord lissé du dessin. C'est **le seul endroit de cette fenêtre que la page ne peint pas**, et le commentaire qui l'installe affirme qu'elle ne coûte rien depuis que la fenêtre est transparente. La mesure dit le contraire, et à gauche seulement. Le pourquoi du « à gauche seulement » n'est pas su, et on ne l'invente pas : c'est le seul bord qui bouge, la fenêtre étant accrochée par son coin haut droit et ne grandissant que vers la gauche.

**Décision : l'instrument copie l'écran.** Au lieu de demander à la fenêtre de se redessiner, on recopie le rectangle de l'écran au coin haut droit de cette fenêtre. Ce qui en revient est exactement ce que l'oeil voit, découpe et compositeur compris, donc le liseré y sera, en vraie taille, à côté des chiffres de la découpe que le journal écrit déjà. Le reste ne bouge pas : mêmes deux moments, mêmes huit photos en anneau, même endroit.

L'image de Victor est gardée dans le dépôt à `docs/testing/captures/bouton-flottant-liseret-gauche.png`, avec sa mesure. C'est la deuxième pièce à conviction rangée là, et pour la même raison que la première : elle coûte une manipulation à refaire.

**Et l'instrument, recopiant l'écran, a rendu deux réponses du premier coup.**

**La première est dans le journal et se lit au pixel près.** En plein survol, la découpe passe de `(1328, 2, 1403, 76)` à `(1326, 2, 1403, 78)` pendant que le dessin passe de 77 à 78 pixels de large. La découpe gagne deux pixels à gauche là où le dessin en gagne un : elle est **posée une image trop tôt**.

Le pourquoi tient en une phrase, et le commentaire qui installe le suivi énonçait déjà la règle sans la tenir : **une mesure prise dans une image n'y est pas encore peinte.** Le navigateur avance l'animation, appelle le suivi, et ne peint qu'ensuite ; la forme envoyée est donc celle de l'image à venir, posée sur un écran qui montre encore la précédente. Tant que le dessin rétrécit ça ne se voit pas, une découpe plus petite ne faisant que cacher des pixels déjà peints. Dès qu'il grandit, elle découvre une bande de fenêtre que la page n'a pas encore peinte, **à gauche**, puisque le logo est accroché par son coin haut droit. Deux pixels, à gauche, pendant une image : c'est le liseré mesuré, et c'est l'éclair au clic, relâcher rendant au logo sa taille donc le faisant grandir.

**Décision : on mesure à chaque image et on pose la mesure de l'image d'avant**, qui est celle que l'écran montre. La découpe ne peut alors plus rien découvrir que la page n'ait déjà peint, ce qui est la règle écrite depuis le début. Le suivi s'arrête toujours sur deux images identiques, donc la forme finale part quand même.

**La seconde réponse est la photo 8, et elle se voit sans rien mesurer.** Le bouton entier y est dessiné **dix-huit pixels à gauche** de sa place, le temps d'une image, juste après la première ouverture du menu. Le journal donne la cause : la fenêtre passe de 1405 à 1423 pixels de large à cet instant précis. Elle est accrochée par son coin haut droit, donc l'élargir déplace son bord gauche, et la vue web garde le temps d'une image son dessin d'avant collé au nouveau bord.

**Ce qui l'élargit est la barre des mesures.** Elle est bâtie à la première lecture, c'est-à-dire à la première ouverture du menu, et c'est elle qui décide de la largeur de la carte. Tout le reste du menu est déjà mesuré au chargement, les raccourcis compris, précisément pour que la fenêtre ait sa taille avant qu'on clique.

**Décision : la barre est posée dès le chargement, remplie de tirets.** C'est déjà ce que la lecture écrit pour un nombre manquant, donc rien de neuf n'est inventé ; la barre a sa largeur définitive avant que le menu s'ouvre, et la fenêtre n'a plus jamais à grandir de la session. Mesuré dans un navigateur : dix-huit pixels avant, zéro après.

**Le saut est parti, le liseré est resté, et l'instrument avait un défaut de plus.** Le journal de la session suivante le confirme sur les deux points : la fenêtre vaut 1423 pixels de large avant que le menu s'ouvre et ne bouge plus, donc plus de saut ; et Victor voit toujours l'éclair.

**Onze photos irréprochables pendant que l'oeil voit un défaut ne veulent pas dire qu'il n'y en a pas, elles veulent dire que la photo est prise trop tôt.** Elle l'était : au bout de `floating_size`, une milliseconde après avoir posé la forme. La forme est remise au système et la fenêtre seulement marquée comme voulant être dessinée ; ni le compositeur ni la page n'ont bougé à cet instant, donc la photo montre l'image d'avant, celle qui est juste.

**Décision : la photo de ce que la découpe a découvert est prise une image plus tard**, au tout début de l'appel suivant, que la page fait à son image suivante. Et seulement quand la découpe **grandit** : une découpe qui rétrécit ne peut que cacher des pixels déjà peints, elle n'a rien à découvrir. Et une seule par série : l'animation du logo fait grandir la découpe à chacune de ses sept images, et sept copies de l'écran par passage de main se paieraient sur la session, ce qui n'est pas un prix à faire payer à un produit pour un instrument.

Le calcul de ce que la forme atteint depuis les deux bords qui ne bougent pas était écrit deux fois à l'identique dans ce fichier ; il l'est une seule, et sert aussi à savoir si la découpe a grandi.

**Et l'appareil braqué au bon instant n'a rien vu non plus.** Vingt-quatre photos, dont six prises exactement une image après un agrandissement de la découpe : toutes donnent la même chose au bord gauche, noir pur, puis le contour à 9,13,22, puis l'or à 239,181,54. Pas un pixel pâle. Ce qui ferme une porte de plus : au moment visé, le produit qui se regarde lui-même voit un bouton irréprochable.

**Ce qui a débloqué la suite est venu d'un autre modèle, appelé en renfort par Victor, et il faut lui rendre deux points sur trois.**

Le point faux d'abord : il affirmait voir dans `bouton-8.bmp` le contour blanc d'un ancien logo dépasser derrière le nouveau. Ce fichier a été relu pixel par pixel et classé en entier : fond noir, contour `#090D16`, or, blanc du dessin, et les lissages entre eux. Rien d'autre. Il n'y a pas de fantôme dans cette image.

**Le premier point juste : `SetLayeredWindowAttributes(…, 255, LWA_ALPHA)` ne veut pas dire « chaque pixel porte son alpha ».** Il règle **une seule opacité pour toute la fenêtre**, la documentation du système est explicite, et l'alpha par pixel de cette fenêtre vient d'ailleurs, du flou-derrière sur région vide que pose la boîte à outils. Le journal de ce produit imprimait donc une phrase fausse depuis le jour où cette ligne a été écrite. **Décision : la phrase est corrigée, l'appel est gardé.** La fenêtre a été mesurée avec et sans, la plaque est partie avec lui, et ce qu'un compositeur fait d'une fenêtre une fois qu'elle est en calque n'est pas quelque chose que les deux documentations tranchent entre elles. Retirer un appel dont on ne comprend pas l'effet parce que son commentaire était mal écrit serait échanger une erreur d'énoncé contre une erreur de code.

**Le second point juste, et c'est celui qui compte : il ne faut pas mieux synchroniser une découpe animée, il faut cesser de l'animer.** Le rendu du navigateur, l'appel vers le coeur et la composition de Windows sont trois files d'attente indépendantes ; le décalage d'une image posé plus haut met les deux premières dans le bon ordre et ne garantit rien sur la troisième. C'est une critique juste de cette correction-là.

**Décision : le bouton ne change plus de taille.** Sa boîte porte en permanence la plus grande des trois tailles et c'est l'image dedans qui grandit et rétrécit. Mesuré dans un navigateur : la boîte du bouton et celle du bloc entier valent le même nombre au centième près au repos, au survol et enfoncé, dans les deux sens d'ouverture. La forme envoyée au coeur ne bouge donc plus pendant l'animation, et le coeur ne redécoupant que quand la forme change, **plus un seul appel de découpe n'est fait entre l'ouverture et la fermeture du menu**. La fenêtre cesse aussi de grandir d'un pixel par image de survol.

Le décalage d'une image est gardé, non pour ce qu'il devait corriger et n'a pas corrigé, mais parce qu'il reste le bon ordre pour les formes qui changent encore, le menu et la barre des mesures. Son commentaire, qui lui attribuait le liseré pâle, est corrigé : cette explication a été démentie par l'essai suivant et une explication fausse laissée dans le code vaut moins que pas d'explication du tout.

**Ce que ça coûte et qu'il faut regarder en premier.** La découpe est celle du plus grand des trois dessins, donc au repos elle dépasse de deux ou trois pixels en bas et à gauche. Ces pixels sont censés ne rien montrer. Si un liseré pâle apparaît désormais **en permanence** au lieu de faire un éclair, c'est la réponse que six photos n'ont pas su donner : la marge est le défaut, et cette moitié-là se défait en un commit.

**Et c'est exactement ce qui s'est passé. « En effet c'est pire ».**

La capture est gardée à `docs/testing/captures/bouton-flottant-marge-non-peinte.png`. Le liseré est passé d'un clignotement de deux pixels au clic à **une bande permanente de quatre pixels** le long de tout le bord gauche et du coin haut gauche. Sur une ligne au milieu du logo, sur fond de bureau brun : `49,39,29`, puis **`215,228,241` quatre fois**, puis `102,110,121`, puis le contour du dessin à `9,13,22`.

**Un pixel de fenêtre que la page n'a pas peint n'est pas vide.** C'est la réponse, et il aura fallu rendre le défaut permanent pour l'obtenir, après avoir couru toute une soirée derrière un éclair d'une image. Sur fond noir la même bande valait `203,209,216`, sur fond brun `215,228,241` : elle n'a pas de couleur à elle, **elle éclaircit ce qu'il y a derrière**. C'est du verre dépoli, celui que la boîte à outils allume en posant un flou-derrière sur région vide pour obtenir la transparence par pixel, et il se voit partout où la page ne peint rien.

**Et ça explique enfin la dernière anomalie**, celle qui rendait toutes les mesures contradictoires : vingt-quatre photos prises par le bouton lui-même, dont six pile au bon instant, montraient un bouton irréprochable pendant que l'oeil voyait le défaut. Ce qu'un compositeur ajoute **au moment de composer** n'est pas dans ce qu'on recopie de l'écran. L'instrument n'était pas mal braqué, il est aveugle à cette classe de défaut, et il fallait le savoir plutôt que de continuer à l'affiner.

**Décision : la découpe ne prend plus un seul pixel au-delà de ce que la page peint.** `MARGE` passe de un à zéro. Les bords restent arrondis vers le dehors, ce qui suffit et ne coûte rien : `Math.floor` d'un bord gauche prend le pixel qui **contient** ce bord, et ce pixel-là est peint, en partie, par le lissage du dessin. Un pixel à moitié peint se voit comme un demi-pixel de dessin ; un pixel pas peint du tout se voit comme du verre. Le premier est le lissage qu'on veut, le second était le défaut.

La crainte qui avait fait ajouter ce pixel était le coin, où un rayon arrondi au pixel pouvait mordre dans le dessin. Le calcul a été rejoué sur les nombres réels des deux machines, ceux que le journal écrit : à `MARGE = 0`, les cinq marges restent positives partout, la plus petite valant cinq centièmes de pixel et celle des coins quinze. La découpe contient donc toujours le dessin entier, sans rien prendre autour.

**Et la boîte du bouton redevient celle du dessin.** La fixer à la plus grande des trois tailles était le geste qui a rendu le défaut permanent ; il a servi à le nommer, il n'a plus de raison d'être. Ce qui reste de cette tentative est le bon ordre entre mesurer et poser, qui vaut pour les formes qui changent encore.

**« Très fortement réduit mais il y en a encore », et cette fois c'est le journal qui a donné la suite.** Débarrassé du pixel de marge, il s'est mis à écrire ses lignes de marges à chaque découpe, avec une marge de coin négative partout, de -0,02 à -0,24. Or **ce signe était à l'envers depuis le jour où cette ligne a été écrite.**

La géométrie tient en une phrase : le point d'un coin arrondi le plus proche du coin de sa boîte est celui à quarante-cinq degrés, et il est en retrait de 0,29 fois le rayon sur chaque axe. **Un rayon plus petit est donc moins en retrait, donc il va plus loin dehors.** Le rayon de la découpe étant arrondi vers le bas, il était le plus petit des deux, donc la découpe dépassait du dessin à chaque coin ; et le journal, qui soustrayait là où il fallait ajouter, appelait ça une marge négative, c'est-à-dire l'inverse exact de ce qui se passait. Vérifié sur un cas du journal : il imprimait -0,24 là où la géométrie donne +0,24.

**Cette erreur de signe est ce qui a fait poser le pixel de marge sur les quatre bords**, celui qui s'est révélé être le défaut lui-même. Une ligne de journal fausse coûte plus cher qu'une ligne de journal absente.

**Décision : le signe est corrigé, et le rayon de la découpe est arrondi vers le haut.** Un coin plus rond rentre dans le dessin au lieu d'en dépasser, ce qui est la même règle que `MARGE` appliquée là où elle manquait. Rejoué sur les vingt-trois morceaux que le journal de NOTEBOOK-VICTOR a écrits : vers le bas, la découpe dépasse du dessin aux vingt-trois coins sur vingt-trois ; vers le haut, à douze, et ce qui reste ne vient plus du rayon mais de la marge du bord, qui est la situation ordinaire d'un pochoir posé sur un dessin dont les bords tombent entre deux pixels. Ce que ça coûte de l'autre côté est un quart de pixel mordu à la pointe du coin.

**Et le seuil de la ligne change de sens avec elle.** Elle ne se dit plus quand une marge devient négative, ce qui est désormais l'état normal du coin, mais **quand une marge atteint un pixel entier**, qui est la seule chose qui ne doit jamais arriver : un pixel entier de marge est un pixel de fenêtre que la page ne peint pas, et on sait maintenant ce qu'on y voit.

**Décision : l'appareil photo du bouton est retiré.** Il a servi une soirée, d'abord en demandant à la fenêtre de se redessiner, ce qui ne montre jamais qu'une page juste, puis en recopiant l'écran. Trente-cinq photos, dont une quinzaine prises pile au bon instant, ont toutes montré un bouton irréprochable pendant que l'oeil voyait le défaut, et on sait maintenant pourquoi : ce qu'un compositeur ajoute au moment de composer n'est pas dans ce qu'on recopie de l'écran. **Un instrument aveugle au défaut qu'il vise n'a rien à faire dans un produit**, surtout au prix d'une copie de trois cent vingt pixels de côté sur le fil qui dessine, à chaque agrandissement de la découpe, c'est-à-dire plusieurs fois par ouverture de menu. Ce qui reste de lui est ce qu'il a appris, écrit ici et dans le protocole, et les deux images qu'il a produites, rangées dans `docs/testing/captures`.

**Et un défaut voisin, rapporté en même temps et qui n'est pas celui-ci : le bouton se téléporte et revient.** Les journaux des quatre derniers builds portent tous le même saut, avant comme après les corrections de ce soir : le coin haut droit de cette fenêtre, qui n'est censé jamais bouger, change une fois par session.

**Deux règles décident où le bouton est accroché et elles ne sont pas d'accord.** Celle du coeur, quand la page redemande une taille, dit « là où il est déjà » et la lit sur la fenêtre elle-même. Celle qui suit l'image, appelée à chaque fois que l'image est posée, dit « le coin de l'image plus le décalage rangé ». Tant que les deux coïncident on ne voit rien ; dès qu'elles divergent, le bouton fait l'aller-retour entre les deux.

**Cette piste était la mauvaise, et le journal suivant a donné la vraie.** Les positions relevées ne sont pas des sauts : cette fenêtre fait la largeur du menu et n'en montre que le logo, donc elle est normalement à moitié hors de l'écran, et un bouton traîné à gauche la met à `-853` sans que rien n'aille mal.

**Le vrai mécanisme est le sens d'ouverture, et il tient en trois lignes.** Le menu s'ouvre vers le bas ou vers le haut selon la place ; le logo, lui, ne doit pas bouger d'un pixel. Il est donc en haut de la fenêtre dans un cas et en bas dans l'autre, et **basculer d'un sens à l'autre déplace la fenêtre de toute sa hauteur moins le logo**, neuf cent douze pixels ici, pour que le logo reste où il est. La page et le coeur ne peuvent pas faire ce demi-tour dans la même image : celle qui redessine la première montre le logo à neuf cents pixels de sa place avant que l'autre ne rattrape. **C'est ça, « il se téléporte et il revient ».**

**Et ce qui déclenchait la bascule était le survol.** Le logo grandit de six pour cent sous la souris, le bloc grandit avec lui, la fenêtre gagne trois pixels de hauteur, et trois pixels suffisent à faire changer la réponse à « est-ce que le menu tient en dessous ». D'où une bascule, donc un saut de neuf cents pixels, à chaque fois qu'une main passe sur le bouton posé près du bord.

**Décision : le bouton garde en permanence la plus grande de ses trois tailles, et c'est le dessin dedans qui respire.** La mise en page ne bouge plus, donc la fenêtre ne change plus de hauteur, donc le sens ne bascule plus tout seul. Mesuré dans un navigateur : le bloc et le bouton donnent le même nombre au centième près au repos, au survol et enfoncé, pendant que le dessin passe de 44 à 46,63 puis 42,67.

**Et la découpe, elle, est prise sur le dessin et non sur la boîte.** C'est l'erreur de la première version de cette idée, qui avait rendu le liseré permanent : la boîte étant plus grande que le dessin, la découpe contenait trois pixels que la page ne peint jamais. Les deux choses sont séparées maintenant, chacune mesurée sur ce qui la concerne.

**Et le décalage d'une image posé plus tôt est retiré.** Il devait empêcher la découpe de courir devant ce que la page peint ; il n'a pas enlevé le liseré, qui venait d'ailleurs, et il retardait d'une image de plus le rattrapage du sens d'ouverture, donc il allongeait le saut. Ce qu'il protégeait est protégé autrement et mieux : la découpe ne prend plus un seul pixel que la page ne peigne pas, ni sur les bords ni dans les coins, donc une découpe posée une image trop tôt ne montre que le lissage du dessin une image trop tôt, ce qui ne se voit pas.

**« Il y en a encore », sur un bouton dont la découpe ne prend pourtant plus rien de trop. La cause est de la virgule flottante, et il a fallu se dédire pour l'attraper.**

Le journal de NOTEBOOK-VICTOR l'écrit noir sur blanc : `découpé (-47, 2, -1, 37) ; marges g 0.50 h 0.50 d 1.00 b 0.25`. Une marge de **1,00 pixel exactement** au bord droit, et `b 1.00` sous la carte du menu. Un pixel entier de marge est précisément ce qui ne doit jamais arriver, et on sait maintenant ce qu'on y voit.

**Le bord en question tombe pourtant pile sur un vrai pixel.** L'écran du fond du logo a son bord droit à 424/440 du dessin ; à 125 % le logo fait 55 pixels réels, et 424/440 x 55 = 53 tout rond. Mais le calcul passe par 44/440, qui n'existe pas en binaire, donc ce bord sort à **-1,9999999999999574** au lieu de -2, et `Math.ceil` réclame là-dessus **une colonne entière** que la page ne peindra jamais. Quatre nombres de journal, tous les quatre sortis d'un seul écart de quatre à la puissance moins quatorze.

**Décision : les bords et le rayon sont calés sur un grain d'un 1024e de pixel avant d'être arrondis.** Le grain est choisi entre deux bornes. Au-dessus du bruit : celui du calcul est de l'ordre de 1e-13, mais `getBoundingClientRect` rend ses nombres en simple précision, donc un bord réellement posé sur le pixel 943 revient à 943,0000305, et jusqu'à 2,4e-4 sur les grandes coordonnées d'un écran 4K ; un millionième de pixel ne rattraperait pas la rangée sous la carte, un 1024e, qui vaut 9,8e-4, est quatre fois au-dessus. En dessous de la vraie géométrie : la mise en page ne connaît pas plus fin que le 64e de pixel de page, ce qui fait 20, 24 et 28 /1024es de pixel réel à 125, 150 et 175 %, donc tous les vrais bords sont déjà des multiples exacts du grain et le calage ne leur fait rien. Une puissance de deux parce qu'elle est exacte en binaire : le calage lui-même n'ajoute pas d'erreur.

**Et il faut dire qu'on s'est dédit.** Cette piste a été trouvée, puis abandonnée, et Victor a été prévenu qu'elle était fausse. Ce qui l'avait « réfutée » était un balayage de trois millions de positions bâti sur une **expression réécrite**, plus propre que celle du code, et dont les erreurs s'annulaient autrement. Le contre-essai a repris l'expression réelle et a **reproduit le journal de Victor chiffre pour chiffre**, les `-1,9999999999999574` et la découpe `(-47, 2, -1, 37)` compris. Une réfutation bâtie sur une expression réécrite ne réfute rien : ce n'est pas le code qu'elle mesure. La leçon est la même que celle des captures d'écran, un cran plus loin.

**Deux mesures sont ajoutées avec, parce qu'il en reste.**

La première : **le journal ne disait jamais rien de la carte du menu.** Sa ligne de marges ne s'écrit qu'à la construction du bouton, où la carte n'existe pas encore, puis seulement si une marge atteint un pixel entier. Or c'est elle qui portait le `b 1.00`, sur le plus long bord du dessin. Elle se dit maintenant **la première fois qu'un morceau apparaît**, et pas seulement à la toute première découpe.

La seconde : **tout ce que la page mesure se compte depuis `clientWidth`**, qui est un entier de pixels de page, tandis que le coeur repose la forme depuis le bord droit réel de la fenêtre, qui est un entier de vrais pixels. Les deux ne tombent au même endroit que si la largeur divisée par l'agrandissement tombe juste ; sinon toute la découpe glisse d'une fraction de pixel, des pixels à demi peints deviennent des pixels pas peints du tout, et un halo devient un trait franc. C'est la seule chose qui expliquerait qu'un même bouton paraisse propre sur une machine et sale sur l'autre. La page envoie donc le bord droit qu'elle peint pour de bon, et le journal écrit l'écart.

**Ce qui reste, et qui demandera un choix.** Le calage enlève environ quatre cents pixels de verre sur les scènes du journal, dont la colonne permanente que Victor voit et la rangée sous la carte du menu. Après lui, le bouton est irréprochable à 125 % au repos et très propre à 150 % ; à 175 % il ne l'est pas encore. Deux chemins restent et ils ne coûtent pas la même chose. Le premier est un **essai de diagnostic** : rendre le défaut permanent une seconde fois avec huit pixels de marge, retirer l'attribut de calque, puis bâtir la fenêtre sans transparence, en mesurant à chaque fois avec un instrument qui voit ce que le compositeur ajoute, Windows.Graphics.Capture ou DXGI, et jamais une recopie d'écran, qui est aveugle à cette classe de défaut. Il dit si ce verre peut être éteint du tout. Le second est un **repli** : une jupe opaque d'un pixel et demi tout autour du dessin, qui ne laisse aucun verre nulle part mais durcit le bord et le rend inégal, c'est-à-dire qui échange le défaut d'aujourd'hui contre celui du tout début.

**Décision : les trois réglages soupçonnés sont éteints un par un, et on regarde.** C'est la première des deux routes ci-dessus, choisie par Victor, et elle est bâtie comme un instrument à part, `trial.rs`, pour qu'elle s'enlève d'un seul geste comme l'appareil photo avant elle.

**Ce qui la rend lisible tient en un mot : le carré.** Le défaut fait deux pixels le long d'un dessin, et huit explications ont été plaidées sur des photographies de ces deux pixels, les huit fausses. La fenêtre est donc découpée, pour l'expérience, sur un **carré de quatre logos de côté** au lieu du dessin. Tout ce qui est dedans et qui n'est pas le logo est de la fenêtre que personne ne peint : deux cents pixels au lieu de deux, et plus personne n'a à plisser les yeux.

**Cinq façons de bâtir la fenêtre, une par session, dans l'ordre, et le journal dit laquelle tourne.** Le bouton tel qu'il est, qui sert de témoin ; le carré ; le carré sans le calque ; le carré sans la transparence ; le carré sans le fond noir. Une session par essai parce que la fenêtre du bouton naît et meurt avec la session : c'est le seul endroit où ces réglages se posent, et les changer sur une fenêtre vivante est justement le genre de manipulation dont le résultat ne prouve rien.

**Un seul réglage change d'un essai à l'autre**, ce qui est toute la différence entre une expérience et une bidouille. Et le nom écrit dans le journal est **calculé à partir des réglages** plutôt qu'écrit à côté d'eux : un nom et une liste d'interrupteurs tenus séparément finissent par décrire la mauvaise session, et la seule valeur de ces cinq lignes est qu'on puisse les croire.

**Ce que chaque réponse voudra dire**, écrit d'avance pour ne pas le décider après coup : un carré pâle au deuxième essai, c'est le défaut reproduit en grand ; un carré qui devient franchement transparent quand on éteint un réglage, c'est le coupable ; un carré qui devient franchement opaque, c'est un réglage qui n'est pas le coupable mais qui est ce qui rend la fenêtre transparente du tout ; et trois essais identiques au deuxième, c'est qu'aucun des trois n'est en cause et que le verre vient d'ailleurs. Ce dernier cas est une réponse lui aussi, et il ferme trois portes d'un coup.

**Et l'expérience a répondu du premier coup, contre ce qui était écrit ici.**

Cinq captures, dans l'ordre, sur PC-VICTOR à 175 % : **rien du tout aux essais 2, 3 et 5**, et un carré **noir et opaque** au quatrième. Le carré du quatrième essai se lit au pixel près : la partie qui déborde sur la fenêtre blanche d'en face fait cent quatre-vingts pixels de large, ce qui est exactement ce que quatre logos donnent une fois la partie posée sur la bande noire retirée. **La découpe en carré a donc bien été appliquée aux quatre essais**, et aux trois qui gardent la transparence elle ne se voit pas.

**Un pixel de fenêtre que la page ne peint pas ne montre rien.** C'est l'inverse de ce que ce journal affirme trois paragraphes plus haut, et c'est mesuré au lieu d'être raisonné : deux cents pixels de fenêtre nue, sur fond noir, invisibles. **La thèse du verre dépoli est fausse**, et avec elle l'explication vendue à Victor après la capture `bouton-flottant-marge-non-peinte.png`. Ce qui restait de cette capture est son fait brut, une bande pâle apparue quand la découpe a été élargie de trois pixels ; ce qui tombe est l'explication qu'on en avait tirée.

**Ce que l'essai ne dit pas encore, et c'est un défaut de sa conduite.** Le bouton était posé sur la **bande noire** que l'image laisse à droite. Sur du noir, une fenêtre opaque au fond noir et une fenêtre transparente donnent le même pixel : les essais 3 et 5, qui éteignent le calque et le fond, ne prouvent donc rien. Le protocole le dit désormais : **le bouton doit être posé sur du clair**. Seul l'essai 2 est concluant tel quel, et c'est celui qui compte.

**Et la page a été mesurée de son côté, dans un navigateur, ce qui ferme la porte suivante.** Le dessin rendu à 125, 150 et 175 %, sur fond noir et sur fond blanc, ligne par ligne au bord gauche et au bord droit : à l'extérieur du dessin, **le fond, exactement, sans un pixel intermédiaire**, puis le lissage du contour, puis le contour à `9,13,22`, puis l'or. Rien de pâle nulle part. Le premier pixel peint tombe à deux pixels du bord de la boîte à 125 %, ce qui est au centième près ce que le dessin annonce, les traits de contour du SVG débordant de quatorze unités sur quatre cent quarante. **La page ne peint donc rien autour du dessin**, et le liseré n'est pas un halo du logo.

**Et la question a changé de nature, parce que Victor a dit quand il voit le défaut : « le liseré n'apparaît que quand je clique sur le FAB ou que je le déplace ».** Jamais au repos. Ce n'est pas un détail, c'est un autre défaut que celui qu'on cherchait depuis deux jours : il ne se montre qu'aux moments où la fenêtre est **redécoupée**. Le protocole disait de ne pas cliquer, donc les cinq captures ne pouvaient pas le montrer, et l'accord entre les cinq ne voulait pas dire ce qu'on en avait tiré.

**Décision : un sixième essai, et c'est peut-être aussi la correction.** La fenêtre est découpée **une seule fois**, sur un rectangle contenant tout ce que la page peut dessiner, et plus jamais pendant que le logo grandit, rétrécit ou voyage. Si le liseré part avec, la découpe répétée est la faute.

**Et cette idée n'est pas un pis-aller, elle découle de la mesure.** Une découpe ne sert plus qu'à laisser passer les clics : ce qu'elle contient et que personne ne peint est invisible, c'est mesuré. Toute la raison qui l'obligeait à épouser le dessin au centième de pixel, la crainte du verre, n'existe plus. Un masque n'a qu'à contenir ce que la page peint. Il reste à le changer entre menu fermé et menu ouvert, sinon le bouton avalerait les clics sur toute la surface du menu replié, mais cela fait deux découpes par session au lieu de plusieurs par image de survol.

**Et l'essai a répondu, et c'est enfin le bon défaut.** Découpée sur un rectangle au lieu du dessin, la chose n'est plus un liseré : c'est **un carré blanc derrière le logo, pendant le clic**. Le liseré n'a donc jamais été un liseré. **C'est toute la fenêtre qui devient blanche le temps d'une découpe**, et une découpe collée au dessin n'en laissait passer qu'un trait de deux pixels. Deux jours à mesurer un trait qui était le bord d'une plaque.

**Et ça referme tout ce qui était contradictoire.** Un pixel non peint est invisible au repos, c'est mesuré ; le défaut ne se montre qu'au clic et au déplacement, c'est dit ; les trente-cinq photos prises par le bouton lui-même ne montraient rien parce qu'aucune n'a été prise pendant une découpe ; et le carré de deux cents pixels du premier essai était invisible parce que rien ne le découpait pendant qu'on le regardait.

**Ce qui reste à trouver est quelle couche peint ce blanc.** Le fond de cette fenêtre est un noir pur, donc ce n'est pas elle. Restent la vue web qu'elle porte, qui est une fenêtre à part avec son propre pinceau de fond, et le calque que Windows tient d'elle. **Décision : les essais changent de cible.** Ils ne portent plus sur ce qui rend la fenêtre transparente, question close, mais sur la seule chose que le produit fait à l'instant précis où le blanc paraît : la découpe et le redessin qui la suit. Une découpe vraiment figée, une par forme de la page ; le redessin en disant cette fois que rien ne doit être effacé ; le redessin sans la vue web ; pas de redessin du tout ; et le calque, la seule des trois anciennes pistes qui n'ait jamais été mesurée sur autre chose que du noir.

**Et l'essai figé de la veille ne figeait rien**, ce qui est à écrire : il prenait la boîte du dessin, et la boîte du dessin suit le logo qui grandit et rétrécit. Il redécoupait donc autant qu'avant. Il a servi quand même, et beaucoup, en montrant le blanc en grand.

**Décision : la ligne de découpe dit aussi le rectangle que le système tient.** Elle ne se disait que quand le nombre de morceaux, le sens, l'acceptation ou la taille de la fenêtre changeaient. Une découpe dont la géométrie bouge sans que son nombre de morceaux bouge ne disait donc **rien du tout**, ce qui est exactement ce qui s'est produit la première fois que la fenêtre a été découpée sur une boîte : quatre ouvertures de menu, pas une ligne. Une ligne qui se tait quand ce qu'elle rapporte a changé vaut moins que pas de ligne.

**Et les six essais ont tous répondu non, ce qui est la réponse.** Sans effacement au redessin : le blanc. Sans redessiner la vue web : le blanc. **Sans redessiner du tout** : le blanc. Sans calque : le blanc. Découpe figée : le blanc, en carré. Le blanc n'est donc ni la découpe, ni le redessin qu'on demande après elle, ni le calque, ni le fond de cette fenêtre, ni un pixel que la page ne peint pas. Cinq pistes éteintes une par une, et il tenait bon.

**Il vient d'ailleurs, et de deux fichiers qu'on peut lire.** `wry` pose un sous-classement sur la fenêtre qui porte la vue web et, **à chaque message de redimensionnement**, il rend à cette vue ses limites, par `ICoreWebView2Controller::SetBounds` puis un `SetWindowPos` sur sa propre fenêtre. `tao`, lui, laisse `DefWindowProc` traiter le changement de position, ce qui envoie ce message de redimensionnement à chaque pose de fenêtre où on n'a pas dit que la taille tenait.

**Or la fenêtre du bouton est reposée cent vingt fois par seconde pendant qu'une main la déplace, toujours à la même taille.** Cent vingt fois par seconde, la vue web se voit donc rendre ses limites et rebâtit sa surface ; et une surface en train d'être bâtie montre le fond de la vue web, qui est blanc, jusqu'à ce que le dessin de la page revienne.

**Tout le reste en découle.** Le blanc au clic et au déplacement et nulle part ailleurs : ce sont les deux seuls moments où cette fenêtre bouge ou change de taille. Rien au repos : Windows n'envoie alors rien, position et taille étant inchangées. Un liseré plutôt qu'une plaque : la découpe collée au dessin n'en laisse voir que la marge, et la découpe figée sur un rectangle en a montré le carré entier. Les trente-cinq photos irréprochables : aucune n'a été prise pendant un déplacement. Et le carré de deux cents pixels resté invisible : rien ne bougeait pendant qu'on le regardait.

**Décision : on dit à Windows ce qui est vrai, à savoir que la taille n'a pas changé.** La fenêtre est posée avec le mot qui le dit dès que sa taille est celle qu'elle a déjà. Ce n'est pas un contournement, c'est la vérité qui manquait : le geste demandait un redimensionnement à chaque pas d'un déplacement qui n'en contient aucun. Ça enlève aussi cent vingt remises de limites par seconde à une vue web pendant qu'on traîne le bouton, ce qui n'était bon pour personne.

**Et l'instrument reste, réduit à deux essais**, pour que la différence se voie sur la même machine à la suite : le bouton corrigé, puis le bouton retaillé à chaque pas comme avant. Il part quand Victor a vu les deux.

**Et ça n'a rien changé non plus : le liseré est là dans les deux essais.** L'explication était pourtant lisible dans les deux bibliothèques, et elle était fausse quand même. Ce qui reste d'elle est gardé pour ce qu'il vaut tout seul : sans ce mot, Windows envoie un redimensionnement à chaque pas d'un déplacement, et la boîte à outils rend à la vue web ses limites à chaque fois, cent vingt fois par seconde, pour une fenêtre dont la taille n'a pas bougé. C'est du travail que personne n'a demandé. Mais le commentaire qui lui attribuait le blanc est réécrit : **une explication fausse laissée dans le code vaut moins que pas d'explication du tout**, et ce fichier l'a payé cinq fois.

**Sept pistes éteintes, sept fois non.** La découpe refaite à chaque image ; la découpe figée pour toute la session ; le redessin sans effacement ; le redessin sans la vue web ; pas de redessin du tout ; le calque ; le message de redimensionnement. Chacune dans une session à elle, chez Victor, et le blanc à chaque fois.

**Et la page est hors de cause, mesurée et non supposée.** Le dessin rendu dans un navigateur à 125 et 175 %, sur fond noir et sur fond blanc, au repos **et arrêté net à sept endroits de son animation de survol** : le pixel le plus clair des quatre pixels qui entourent le dessin vaut `0,0,0` dans tous les cas. La page ne peint rien autour du logo, jamais, même en plein mouvement. Ce qui ferme aussi l'idée d'un halo blanc que produirait le redimensionnement d'une image à transparence, qui était la piste suivante.

**Décision : il ne reste qu'une chose, et c'est le déplacement.** C'est la seule chose que le produit fait pendant qu'un bouton est cliqué ou traîné et qu'il ne fait pas au repos : la fenêtre est posée cent vingt fois par seconde tant qu'une main la tient. Quatre essais, quatre façons de la déplacer, la dernière étant de ne pas la déplacer du tout.

**Et une capture du défaut, demandée en fichier, vaudrait plus que les quatre.** Quatre explications de suite ont été bâties sur une description et les quatre étaient fausses. Personne n'a encore mesuré un seul pixel de ce liseré : ce qui a été mesuré, c'est tout ce qu'il n'est pas.

**Décision : la ligne du bord droit est corrigée, parce qu'elle comparait deux instants.** Elle tenait le bord peint par la page contre la largeur que la fenêtre **allait** prendre, alors que la page mesure toujours dans la fenêtre telle qu'elle **est**. Elle a donc écrit `1344.00 px de fenêtre que la page n'atteint pas` sur un bouton parfaitement placé. Elle dit maintenant les deux nombres, la fin de la page et la fin de la fenêtre, sans rien en conclure, et elle se dit quand la paire change au lieu de se dire quand la taille change, ce qui est le seul moyen de l'obtenir une fois tout posé. Une ligne de journal fausse coûte plus cher qu'une ligne absente, et c'est la cinquième fois que ce fichier l'écrit.

**Et le déplacement non plus, ce qui fait onze.** Le dernier essai clouait la fenêtre sur place, le bouton refusait de suivre la main, et le liseré était toujours là. Onze pistes, onze fois non.

**Décision : la vue web sort du logo.** C'est la seule couche que ces onze essais n'ont jamais éteinte, parce qu'ils portaient tous sur ce que **nous** faisions à la fenêtre et jamais sur ce que le navigateur y faisait. C'est aussi la seule dont le fond à elle est blanc. Victor l'a demandé en ces termes, après avoir tout essayé : « fais moi le FAB en natif pour voir ».

**Et ce qui part avec elle est bien plus qu'un navigateur.** La fenêtre du logo est désormais **habillée par l'image qu'on lui donne** : on remet à Windows un rectangle de pixels portant chacun sa transparence, et c'est toute la fenêtre. Il n'y a plus de forme à découper, la forme étant la transparence ; plus de fond à effacer, puisque rien n'est jamais effacé ; plus de cadre ; et plus de clics à laisser passer, le système les laissant déjà passer partout où l'image est claire. **Quatre des défauts que ce bouton porte depuis sa naissance ne peuvent plus se produire du tout**, et la moitié du code qui les combattait s'en va avec.

**Le dessin est calculé, pas redimensionné.** Chaque pixel est déduit de la géométrie du fichier `zyrdesk.svg`, dont les coordonnées sont reprises telles quelles : un rectangle arrondi sait exactement quelle part de chaque pixel il couvre, et c'est ce qu'est un bord lisse. Il n'y a plus nulle part de pochoir à un bit par pixel, qui était la seule chose qui ait jamais donné un bord en escalier à ce bouton.

**Ce qui reste dans la vue web est le menu**, qui est de la vraie interface et n'a rien à faire dessiné à la main. Il ne se déplace jamais et n'existe que pendant qu'il est ouvert. Sa fenêtre garde la place du logo dans sa mise en page, pour que le menu s'ouvre exactement où il s'ouvrait, mais elle ne peint plus rien dedans : menu fermé, elle ne dessine rien, elle est découpée sur rien, et les clics la traversent en entier.

**Et une seule ancre pour les deux fenêtres**, posées dans le même geste : c'est ce qui les empêche de jamais être en désaccord sur l'endroit où se trouve le bouton.

**Il n'y a pas survécu. « ENFIN CA MARCHE ».**

**C'était donc bien la vue web**, et l'élimination avait raison là où onze raisonnements s'étaient trompés. Ce qui se voyait n'était pas un liseré mais le fond blanc d'un navigateur, laissé une image à l'écran chaque fois qu'il refaisait sa surface, et la découpe collée au dessin n'en laissait passer qu'un trait de deux pixels. Trois jours à mesurer le bord d'une plaque.

**Deux fautes de plus, corrigées le même jour, et elles valent d'être écrites.** La première : une forme vide est une forme, et c'est celle que la fenêtre du menu porte le plus souvent depuis que le logo est ailleurs. Lue comme « rien à faire », elle laissait cette fenêtre sans découpe du tout, ce qui pour une fenêtre ne veut pas dire rien mais tout : quatorze cents pixels de rectangle nu en travers de la session. La seconde : les trois tailles du logo étaient lues comme des fractions de sa taille de repos alors que sa fenêtre est bâtie à la plus grande, si bien qu'il était dessiné six pour cent trop gros et perdait son côté droit sous la souris.

**Et ce que la chasse laisse comme leçon, en une ligne : les mesures ont eu raison, les raisonnements ont eu tort.** Chaque explication bâtie sur une description a été démentie ; chaque chose éteinte une par une a fini par désigner la bonne. Ce qui a coûté le plus cher n'est aucune des douze pistes, c'est le temps passé à en plaider avant d'en éteindre.

**Décision : les explications démenties sortent du code.** Le commentaire qui installait `MARGE` racontait le verre dépoli, celui du grain disait tenir « le liseré que Victor voit », et la ligne de journal des marges annonçait rattraper le défaut. Les trois gestes restent, tous justes, mais pour la raison qui est vraie : une découpe n'a qu'un travail, contenir ce que la page peint et rien de plus, parce que ce qu'elle contient au-delà attrape les clics.

## D102. L'interface sort de la vue web, morceau par morceau, en commençant par le menu du bouton flottant (2026-09-01, pendant M4)

**Demandé par Victor, dans ces termes** : « maintenant qu'on a commencé à faire ça en natif fais moi aussi le menu du fab en natif on va retirer bout par bout le webview ça me fait chier ce webview depuis le début ». Et pour la manière : « pars du principe que tout sera migré en natif donc anticipe si y'a besoin ».

**Pourquoi maintenant et pas plus tard.** L'interface d'aujourd'hui a été faite vite pour qu'il y ait quelque chose à cliquer, et elle sera refaite de toute façon avant la sortie. Un déménagement fait pendant qu'on refait de toute façon coûte le déménagement seul ; fait après, il coûte deux fois l'écran.

**Ce que ça n'est pas.** Ce n'est pas un doute sur la vue web en général : c'est la conclusion de D101, où trois jours ont été passés sur un défaut qui n'appartenait ni au produit ni à Windows mais au navigateur embarqué, et sur une fenêtre où le produit avait besoin de choses qu'un navigateur ne rend pas : la transparence par pixel, un clic qui traverse, une fenêtre qui ne redessine jamais rien qu'on n'ait pas peint.

**Décision : une seule couche de dessin pour tout le produit.** `paint.rs` n'est pas taillée sur le premier écran qui la demande : elle sait remplir, contourner, ombrer, écrire et poser une icône, et le logo n'en emploie que deux gestes. Une couche taillée sur son premier client se rouvre à chaque suivant, et une couche qu'on rouvre est une couche dont personne ne connaît plus les règles.

**Dessinée par le processeur, et c'est voulu.** Victor : « dans ma tête ça résonne par perte de performances ». La carte graphique décode déjà de la vidéo en quatre mille par soixante ; lui ajouter le dessin d'une carte de menu serait mettre un client de plus dans la file la plus longue du produit. Une carte coûte deux ou trois millisecondes de processeur, et seulement quand quelque chose change : à l'ouverture, au passage de la souris d'une ligne à l'autre, à la seconde qui fait bouger les chiffres. Zéro le reste du temps. C'est aussi ce qui évite d'avoir à survivre à la perte d'un appareil graphique, ce qui arrive précisément quand un pilote redémarre, c'est-à-dire au pire moment d'une session.

**Décision : le système de design reste écrit une seule fois, dans `design.css`, et Rust le lit à la compilation.** Deux copies d'une palette, ce sont deux palettes, et la première couleur changée dans l'une est le jour où le produit cesse de se ressembler. La règle s'entretient toute seule : **la palette est exactement ce que le thème clair redit**, et tout ce qu'il ne redit pas devient une constante commune. Le jour où la dernière page s'en va, la source de ces valeurs revient dans Rust et rien d'autre ne bouge.

**Décision : les icônes sont reprises telles quelles, pas redessinées.** Le lecteur de chemins comprend ce dont les icônes du produit se servent et rien de plus : aller à, tracer jusqu'à, à l'horizontale, à la verticale, un arc, refermer. Une lettre inconnue arrête la lecture plutôt que d'être sautée : une icône à moitié dessinée ressemble à un défaut, une icône absente à un oubli, et le second se cherche. Une icône transcrite à la main est une icône qui finit par ne plus être la même.

**Décision : une seule promenade décrit la carte, lue par le dessin et par la souris.** Chaque ligne y reçoit sa place une fois. Une carte dont les lignes sont dessinées à un endroit et cliquées à un autre est une carte qui rend le mauvais menu, et c'est le genre de faute qui n'apparaît qu'à un agrandissement d'écran donné.

**Décision : les combinaisons de touches sont écrites comme elles sont gravées.** Le produit retient la **place** d'une touche et non le signe dessus, parce que la touche à gauche des chiffres porte « ² » en France et « ` » ailleurs. La page refait le chemin en sens inverse en demandant au navigateur ce qu'il sait du clavier ; le menu dessiné le demande à Windows. Écrire `Alt+Backquote` dans un menu serait exact et illisible.

**Ce qui reste dans la vue web à cette étape**, dit dans le journal à chaque ouverture plutôt que masqué : les lignes à interrupteur, le curseur du débit, les deux sous-menus, et la ligne rouge qui porte un refus. Tant qu'elle est là, un refus venu de la carte dessinée ne va qu'au journal : deux endroits pour la même phrase, ce serait deux phrases.

**Et le menu web reste ouvert au clic gauche pendant tout le déménagement**, la carte dessinée s'ouvrant au clic droit. C'est ce qui permet de les comparer côte à côte sur la même machine, ce qui a déjà servi : la carte tombait vingt pixels trop bas et vingt trop à gauche, la fenêtre étant plus grande que la carte de tout ce que l'ombre déborde.

**Deux fautes à écrire, toutes deux du même genre.** Les quatre mesures de la barre ont d'abord été **inventées** au lieu d'être lues dans la page : « Latence, Réseau, Débit, Images » là où le produit dit « Décodage, Encodage, Réseau, Débit ». Et le trait de séparation allait d'un bord à l'autre parce que sa marge latérale n'avait pas été relue : un trait qui traverse coupe la carte en deux au lieu de séparer deux groupes. Les deux fois, la faute est d'avoir écrit de mémoire ce qui était déjà écrit ailleurs.

**Ce que ça dit pour Linux, puisque la question viendra.** La couche de dessin est le seul morceau à réécrire par système : ce qui décrit l'interface, ses lignes, ses icônes, ses couleurs et ses mesures, ne connaît pas Windows. Et tout ce qui entoure ce bouton, la fenêtre qui flotte, les clics qui traversent, la fenêtre du moteur portée dans la nôtre, les raccourcis pris au système, est déjà propre à Windows et l'aurait été avec ou sans navigateur.

## D103. Une icône à moitié dessinée est un défaut qui ne se dit pas, donc elle se dit (2026-09-01, pendant M4)

**Le symptôme, en photo.** L'icône « Masquer ce bouton » du menu dessiné ne ressemblait à rien : un trait en biais et un point. Victor : « l'icône masquer est mal faite, on comprend pas ce que c'est comme logo. »

**La cause, mesurée et non supposée.** Le lecteur de chemins comprenait sept commandes du langage SVG. Les icônes du produit en emploient huit : celle qui manquait est la **courbe de Bézier**, et une seule icône s'en sert, le contour de l'oeil barré. Un passage sur les dix-sept dessins de la page le dit en une ligne : `non lu : Cc`, une fois, sur ce chemin-là et sur aucun autre.

**Ce qui a permis à ce défaut de passer.** Une icône est faite de plusieurs traits, chacun un chemin à part. Celui qui ne se lisait pas disparaissait, **les autres restaient**, et ce qui s'affichait était une icône méconnaissable dont rien nulle part ne disait qu'elle était incomplète. Le commentaire du lecteur promettait pourtant le contraire : « une icône absente à un oubli, et le second se cherche ». La promesse n'était pas tenue.

**Décision : la courbe est ajoutée, et un chemin illisible se dit dans le journal.** Une fois, et retenu comme illisible pour ne pas se redire à chaque image. Les deux vont ensemble : la première moitié répare l'icône d'aujourd'hui, la seconde répare la façon dont on trouvera la prochaine.

**Et ce que ça confirme, une fois de plus dans ce fichier : ce qui n'est pas dit ne se cherche pas.** Trois jours de chasse au liseré blanc tenaient à la même chose.

## D104. Un rôle employé et jamais défini ressemble à un rôle défini (2026-09-01, pendant M4)

**Trouvé en portant les interrupteurs du menu.** La feuille du bouton écrit `color: var(--sur-accent)` sur le côté allumé d'un interrupteur, et **`--sur-accent` n'existe nulle part**. Le navigateur remplace alors par ce qui est hérité, c'est-à-dire la couleur du texte ordinaire : du texte clair sur du bleu clair en thème sombre, du texte sombre sur du bleu foncé en thème clair. Ça se lit mal dans les deux, et rien ne signale une faute.

**Décision : le rôle est créé dans le système de design, pas contourné.** Une couleur d'accent n'est pas un fond de texte tant qu'on n'a pas dit ce qui se lit dessus ; sans ce rôle, chaque écran le devine à sa façon. Défini dans les deux thèmes, il rejoint la palette tout seul, la règle étant que **la palette est exactement ce que le thème clair redit**. La page et le menu dessiné en profitent du même coup, ce qui est le but de n'avoir qu'une source.

## D105. Ce qui vit dans le menu ne vit que pendant qu'on le regarde (2026-09-01, pendant M4)

**Les quatre chiffres.** Le moteur écrit une ligne par seconde dans un fichier ; la carte la lit tant qu'elle est ouverte, et pas une seconde de plus. Des chiffres que personne ne regarde ne valent ni le fichier ni le réveil. La lecture se fait hors du fil qui dessine, et ce fil-là ne reçoit que du texte déjà mis en forme : la mise en forme est ainsi faite une fois par seconde et non une fois par image.

**Le tour de veille porte un numéro**, qui change à chaque ouverture et à chaque fermeture. Sans lui, ouvrir et refermer vite laisserait deux veilles derrière la même carte, et une carte disparue avec sa session en laisserait une pour toute la vie du programme.

**Les trois interrupteurs se relisent, ils ne se retiennent pas.** Le mode de la souris et les touches système sont ce que ce programme croit, parce que c'est lui qui les bascule et que le moteur ne dit jamais où il en est ; le son se demande au mélangeur de Windows, qui le sait et qui est ouvert à tout le monde. Relus à chaque ouverture de la carte et après chaque bascule : un interrupteur qui montre ce qu'il croit plutôt que ce qui est est un interrupteur qu'on ne croit pas deux fois.

**Et la carte reste ouverte quand on bascule**, là où une action la referme. On regarde l'image après avoir basculé, et rouvrir le menu pour la ligne d'à côté ferait deux gestes pour un réglage.

## D106. Une mesure prise dans une boîte démesurée ne vaut rien (2026-09-01, pendant M4)

**Le symptôme, en photo.** Les interrupteurs du menu dessiné étaient écrasés à la largeur de leur seule marge : « Bureau » s'y coupait en « Bur / eau », et la carte était pourtant assez large, avec du vide à gauche d'eux.

**Deux fautes en une, et la même racine.** La première : `ecris` réglait le calage du texte **sur la police partagée**, celle-là même dont les mesures se servent. Une mesure prise juste après un texte aligné à droite était donc prise dans une boîte alignée à droite. La seconde : cette boîte faisait la moitié du plus grand nombre représentable, où un mot de soixante pixels ne pèse plus rien du tout, le calcul ayant perdu à cette échelle-là toute précision.

**Décision : une police par taille, par graisse et **par calage**, réglée à sa fabrication et jamais après.** Ce qui la partage ne la change plus. Et les mesures se prennent dans une boîte large mais finie, assez pour qu'aucun mot n'aille à la ligne et pas plus.

**Et l'écriture d'une ligne se mesure, elle ne se déduit pas.** La barre des mesures était plus tassée que celle de la page : le texte y était empilé sur sa **taille** de caractère quand une page l'empile sur la **hauteur de sa ligne**, laquelle vaut environ quatre tiers de la taille et est décidée par la police. Elle est maintenant demandée à la police, une fois, à l'ouverture.

## D107. La fenêtre suit la carte, ce qu'une vue web ne savait pas faire (2026-09-01, pendant M4)

**Le problème posé par les lignes qui vont et viennent.** Le menu porte trois lignes qui ne sont pas toujours là : l'écran de l'hôte quand la machine d'en face en a plusieurs, la liste des tailles, et « Appliquer » quand ce qui est choisi n'est pas ce qui est à l'écran. La vue web y répondait en bâtissant sa fenêtre à la plus grande taille jamais nécessaire et en ne la réduisant plus jamais, parce que **chaque changement de taille découvrait une bande que la page n'avait pas encore peinte** : c'était le clignotement.

**Décision : la fenêtre est remesurée à chaque dessin et suit ce que la carte demande.** Ça ne peut rien faire clignoter : l'image et la taille sont remises à Windows dans le même geste, donc il n'existe pas d'instant où la fenêtre soit grande sans être peinte. C'est précisément ce qu'une vue web ne sait pas faire, et c'est la deuxième chose que le dessin natif rend possible après la transparence par pixel.

**Elle est accrochée par son bord droit et par celui d'où le menu s'ouvre**, qui sont les deux seuls que personne ne doit voir bouger, et ce sont ceux-là mêmes que la pose calcule : les deux tombent d'accord sans se parler.

**Et les sous-menus s'ouvrent à gauche, dans la même fenêtre.** Pas de deuxième fenêtre : ce qui n'est pas dessiné ne se voit pas et n'attrape aucun clic, donc un panneau fermé ne coûte rien du tout. La fenêtre est mesurée pour le **plus large** des panneaux et non pour celui qui est ouvert, sinon ouvrir une liste déplacerait tout le reste au même instant.

## D108. L'accent du produit est l'or du logo (2026-09-01, pendant M4)

**Demandé par Victor** : « les couleurs y'a du bleu, essaie plutôt de partir sur le thème du logo ZyrDesk, avec ce jaune / blanc / noir ».

**Décision : `--accent` devient l'or de `zyrdesk.svg`, repris au signe près.** Un produit dont la marque dit une couleur et dont l'interface en dit une autre est un produit qui se présente deux fois. Comme tout passe par le système de design, les cinq rôles de la famille changent en un seul endroit et tout le produit suit, l'accueil comme le menu dessiné.

**Le noir du logo est ce qui s'écrit dessus.** Le blanc ne tient pas sur cet or-là. C'est le rôle `--sur-accent` de D104, et il y avait déjà deux endroits qui écrivaient `#ffffff` en dur sur l'accent : ils nomment le rôle maintenant.

**Le thème clair descend l'or jusqu'à se lire sur du blanc.** L'or du logo y est trop clair pour porter du texte, et un accent qu'on ne lit pas n'est plus un accent.

**Et l'attention penche vers l'orange, ce qui n'est pas un détail.** Elle était un jaune ambré, à un cheveu du nouvel accent. Deux jaunes voisins dont l'un appelle le clic et l'autre prévient ne se distinguent plus, et c'est justement celui qui prévient qu'il ne faut pas rater.

## D109. Le menu web s'en va, et avec lui mille sept cents lignes (2026-09-01, pendant M4)

**Demandé par Victor**, une fois les deux menus comparés côte à côte : « je vois pas d'autre raison de garder l'ancien menu, tu peux le supprimer et mettre le nouveau sur le clic gauche ».

**Ce qui part avec la page.** La fenêtre du bouton n'existait que pour porter ce menu. Elle emportait avec elle : la découpe et tout ce qui la mesurait, la disait et la refaisait ; la conversation où la page mesurait son propre dessin et où le coeur redimensionnait la fenêtre autour ; l'attente qu'une page finisse par parler et les trois tentatives quand elle ne parlait pas ; la transparence demandée en deux moitiés ; sept commandes ; et deux notions du sens d'ouverture, ce qu'on voulait et ce que la page avait dessiné, qu'il fallait tenir séparées parce qu'elles étaient en désaccord le temps d'une image. **Mille sept cents lignes de moins pour la même interface.**

**Et un souci de moins qui ne se répare pas, il disparaît.** Le menu ne prend plus le clavier : la carte n'est jamais activée et ne porte aucune page qui prendrait le focus. Toute la mécanique qui rendait le clavier à l'image en refermant le menu n'a plus d'objet.

**Le clic gauche ouvre le menu, et le geste du logo est intact** : un clic simple ouvre, un déplacement déplace, et l'animation de la prise est celle qu'elle a toujours été. Le clic droit qui servait à comparer s'en va avec ce qu'il comparait.

**Le sous-menu perd son titre**, demandé aussi : la ligne qui l'ouvre est en face, son chevron s'est retourné vers lui, et la cliquer à nouveau referme. Un titre qui redit le mot d'à côté prend une ligne pour n'apprendre rien.

**Et la question au loin se repose jusqu'à ce qu'elle soit répondue.** Ce que la machine d'en face sait encoder n'est connu qu'une fois son moteur démarré et le chemin en train de servir la session : posée une seule fois à l'ouverture du bouton, la question tombait toujours avant, et le menu proposait un codec que cette machine-là ne sait pas faire. Le journal du service le montre à la seconde près. Elle est reposée une fois par seconde tant que la réponse manque, et plus du tout ensuite.

## D110. L'accueil sort de la vue web, et il ne reste plus de navigateur nulle part (2026-09-01, pendant M4)

**Demandé par Victor**, une fois la vue web réduite au seul accueil : « si il reste que l'accueil vas y ducoup migre en full natif ».

**Ce qui reste de la boîte à outils.** Une fenêtre, sa boucle d'événements et l'icône de zone de notification. Elle est maintenant construite **nue** : plus aucun navigateur n'est ouvert dans le produit, et son dedans est une toile que ce programme peint, comme le logo et le menu de la session. Les vingt-cinq commandes qui existaient pour qu'une page pose ses questions n'existent plus : ce sont des appels. Les événements qu'une page écoutait sont des appels aussi.

**Ce qui a été gagné en route.** La couche de dessin a appris ce qu'une vraie page demande : une plume qui porte ensemble la taille, la graisse, le calage, la famille et ce qu'un mot fait quand il ne tient pas ; l'espace entre les signes, que le système de design réclame pour les étiquettes en capitales et pour le code d'appairage ; la hauteur d'un bloc replié à une largeur donnée, sans quoi on n'empile pas des paragraphes ; le trait en pointillés de ce qui attend d'être rempli ; et de quoi se verser dans une fenêtre ordinaire, encadrée et opaque, là où une fenêtre à calque reçoit son image et sa place en un seul geste.

**Mesurer et dessiner sont la même marche.** Ce qu'un dialogue prend de haut est ce que ses lignes prennent : la marche se fait une fois muette pour compter, une fois pour poser. Une deuxième arithmétique écrite à côté se répondrait juste jusqu'au premier mot rallongé.

**Les trois champs de saisie sont ceux de Windows**, et c'est le seul endroit du produit qui emprunte un objet du système. Écrire du texte est aussi le seul endroit où le système fait mieux que nous : le curseur, la sélection, le presse-papiers, les claviers qui composent leurs signes. Ils vivent le temps du dialogue qui les porte, dans le cadre que nous dessinons, et la tabulation, Entrée et Échap leur sont reprises pour que le dialogue se comporte comme un dialogue.

**Le thème ne vit plus dans le navigateur.** Le choix vivait dans le magasin de la page, qui s'en va avec elle : il est dans un fichier, à côté de la place du bouton flottant, relu avant que la fenêtre s'ouvre. Et ce que Windows demande est relu par le fil qui le surveille et par personne d'autre : une question au registre par bascule, et non une par image dessinée.

**Le système de design n'a pas bougé.** Il est toujours écrit une seule fois et lu à la compilation. Le fichier garde sa notation, que plus aucun navigateur ne lit : elle écrit deux thèmes côte à côte, et la relire à la compilation est ce qui vérifie que les deux disent bien les mêmes rôles. Recopier quarante valeurs à la main serait exactement ce que tout ce mécanisme existe pour éviter.

**~~Ce que ça coûte~~ réglé le 2026-09-01 par [D111](#d111-la-fenêtre-la-boucle-et-licône-deviennent-les-nôtres-et-il-ne-reste-plus-de-boîte-à-outils-2026-09-01-pendant-m4).** Il était écrit ici que la fenêtre nue vivait derrière la porte « unstable » de la boîte à outils, et que le jour où celle-ci ne fournirait plus que ça, la fenêtre deviendrait la nôtre. Ce jour est le lendemain.

## D111. La fenêtre, la boucle et l'icône deviennent les nôtres, et il ne reste plus de boîte à outils (2026-09-01, pendant M4)

**Demandé par Victor**, à la lecture de ce qui restait : « et tu peux pas aussi migrer ça en natif ? ».

**Ce qui restait** était la fenêtre, sa boucle d'événements et l'icône près de l'horloge. C'est peu de choses en nombre, et c'est la plus importante des trois : **c'est la même fenêtre qui porte l'accueil et l'image d'une session**, et tout ce que `picture` fait de délicat se joue dans les messages qu'elle reçoit. Une couche qui vise autre chose entre nous et ces messages-là est exactement l'endroit où l'on ne veut pas d'intermédiaire.

**La fenêtre.** Bâtie ici, avec sa classe, son cadre, son plancher de taille, son plein écran, son agrandi et son suivi de l'agrandissement d'écran. Ce que `picture` posait devant ses messages continue de s'y poser : un gardien se met devant celle-ci comme il se mettait devant l'autre.

**La boucle.** Le fil principal prend les messages et les rend. Ce qui devait être fait sur ce fil-là passe par une **boîte aux lettres** : une fenêtre qui ne montre rien, dont le seul rôle est de porter du travail. Une fenêtre et non un message au fil lui-même, et ce n'est pas un détail : Windows jette les messages adressés à un fil pendant qu'il déplace une fenêtre, et c'est précisément pendant qu'on la déplace que l'image d'une session doit la suivre.

**L'icône.** Elle est **dessinée**, comme tout le reste du produit, à la taille exacte que la barre demande. Il n'y a donc plus rien à réduire ni à agrandir, et les six images qu'elle embarquait s'en vont : c'est la même marque que celle du bouton flottant et de l'accueil, tracée par le même dessin. Pâlie plutôt que remplacée quand l'ordinateur n'est pas joignable, comme avant.

**Un seul ZyrDesk à la fois** tient maintenant dans un verrou nommé et un message : le second trouve la fenêtre du premier, lui demande de se montrer, et s'arrête.

**Ce que ça enlève.** Trois cent vingt et une caisses dans le verrou du projet, cinq cent soixante-neuf devenues deux cent quarante-huit. Plus aucune dépendance à une couche web, ni au moment de compiler ni au moment de tourner : la CI de Linux n'installe plus rien pour bâtir l'interface. Et le fichier de configuration, les capacités et les schémas que la boîte à outils demandait s'en vont avec elle.

**Ce que ça coûte.** Ce que la boîte à outils faisait dans l'ombre est maintenant écrit et doit être maintenu : la conscience des écrans, dite au démarrage et redite dans le manifeste ; les contrôles modernes du système, sans lesquels le mot en filigrane d'un champ de saisie ne s'affiche pas ; et l'icône du programme, gravée dans la ressource sous le numéro que Windows y cherche. Trois choses, écrites une fois, dans deux fichiers de vingt lignes.

## D112. Chaque programme du produit dit dans le gestionnaire des tâches lequel il est (2026-09-01, pendant M4)

**Le constat, fait par Victor.** Trois lignes dans le gestionnaire des tâches : « ZyrDesk », « ZyrDesk.exe » et « zyrdeskd ». La première est le moteur hôte, la deuxième l'application, la troisième le service, et rien à l'écran ne le dit. Quelqu'un qui ouvre cette liste pour savoir ce qui tourne sur sa machine n'apprend rien, et deux des trois noms sont des noms de fichiers.

**Pourquoi il y en a trois, et pourquoi il en faut trois.** L'application ne peut pas être le service : le service tourne avant qu'une session Windows soit ouverte, et c'est ce qui permet de se connecter à un ordinateur où personne n'est connecté. Le moteur ne peut pas être l'application : c'est un programme d'un autre projet, piloté de l'extérieur, et le faire vivre dans notre processus reviendrait à le forker pour de bon.

**Décision : ce que cette liste affiche est écrit dans chaque programme, et dit lequel il est.** Windows n'y montre pas le nom du fichier mais la description gravée dans l'exécutable. Elle est désormais posée pour les cinq :

| Programme | Ce que la liste affiche |
|---|---|
| `ZyrDesk.exe` | ZyrDesk : Application |
| `zyrdeskd.exe` | ZyrDesk : Service de connexion distante |
| `zyrdesk-host-engine.exe` | ZyrDesk : Moteur de diffusion |
| moteur client | ZyrDesk : Moteur d'affichage |
| `zyr-cli.exe` | ZyrDesk : Outil en ligne de commande |

Tous commencent par le nom du produit, donc la liste les range ensemble et personne n'a à chercher lequel appartient à quoi. Et tous sont écrits sans article : c'est ainsi que se nomment les programmes d'un produit, et l'article n'apprend rien à qui lit la liste.

**Et ce nom est écrit une seule fois par programme**, dans la description de son paquet, d'où la ressource Windows le lit à la compilation. Le nom du moteur hôte est passé par notre script de compilation, le moteur ne portant que ce qu'on lui donne.

**Sans accent, et c'est mesuré et non supposé.** Le compilateur de ressources lit ce nom dans le jeu de caractères du système et non en Unicode, même avec la ligne qui lui demande le contraire : essayé, « le service d'accès distant » arrive dans l'exécutable coupé à « le service d'acc ». Et le nom du moteur hôte voyage en plus par une ligne de commande, où une apostrophe casse la compilation, également essayé. Les cinq noms sont donc écrits sans accent et sans apostrophe là où c'est nécessaire.

## D113. Le menu du bouton flottant se pose là où il tient, et le débit se règle au mégabit (2026-09-01, pendant M4)

**Trois constats de Victor, sur la même carte.**

**Un.** Le bouton posé vers le milieu de l'image ouvrait un menu coupé par le bas. [D66](#d66-quatre-reprises-sur-le-bouton-flottant-2026-08-27-pendant-m4) ne connaissait que deux sens, dessous et dessus, et il en manquait un : à mi-hauteur, ni l'un ni l'autre n'a la place, et le choix se faisait alors entre deux sens qui coupent tous les deux.

**Décision : un troisième sens, à gauche du bouton.** La carte y a toute la hauteur de l'image pour elle. Elle part du haut du bouton et glisse vers le haut de ce qu'il faut pour tenir en entier, ce qui est le seul moment où elle ne commence pas exactement au bouton : c'est ce glissement qui la garde entière, et c'est toute la raison d'être de ce sens-là. L'ordre reste dessous, dessus, à côté : c'est l'ordre dans lequel une carte se lit le plus naturellement depuis le bouton qui l'ouvre. Une image trop courte pour la carte quel que soit le sens la garde du côté où il reste le plus de place, comme avant : à côté elle serait coupée aussi, et elle y perdrait en plus de partir du bouton.

**Ce que le logo en sait.** Rien de neuf. Il ne connaît que deux coins, et à côté il garde celui qu'il a quand la carte est dessous : ce sens-là ne le concerne pas, la carte ne partant plus de lui.

**Deux.** Un sous-menu de deux lignes s'ouvrait tout en haut de la carte pendant qu'on cliquait une ligne du bas. Il était aligné sur le bord d'où le menu s'ouvre, ce qui est juste pour la liste des résolutions, trop haute pour partir d'ailleurs, et faux pour toutes les autres.

**Décision : un sous-menu s'ouvre en face de la ligne qui l'ouvre**, sa première valeur sur elle. Il descend ou remonte de ce qu'il faut pour tenir dans sa fenêtre, qui est bâtie assez haute pour le plus grand d'entre eux. La liste des résolutions remonte donc toujours, et c'est le seul cas où le sous-menu ne part pas de sa ligne.

**Trois.** Le curseur du débit sautait de cinq mégabits, puis de dix, puis de vingt : huit crans ronds pour toute l'échelle. C'était une échelle à viser du regard, pas à régler.

**Décision : un cran par mégabit, de 5 à 80 Mb/s.** Les bornes ne bougent pas, l'espacement si. Le curseur est la manière dont on cherche le débit que sa propre liaison porte vraiment, en regardant l'image pendant qu'on le pousse, et un curseur qui saute dix mégabits n'est pas quelque chose avec quoi on cherche. Rien d'autre ne change : ce sont toujours des rangs qui sont poussés et non des nombres, et rien n'est écrit tant que la main tient le pouce.

## D114. Ce que le moteur d'en face ne lit qu'à son démarrage se règle avant d'ouvrir l'image, jamais après (2026-09-01, pendant M4)

**Le constat, relevé par Victor, journaux à l'appui.** Passer « Écran d'en face » en Économe pendant une session coupait la session. Se reconnecter marchait, et le réglage était bien pris en compte.

**Ce qui se passait, à la seconde près.** Appliquer relance l'image : le lecteur s'arrête, une voie s'ouvre vers l'ordinateur d'en face, la cadence lui est demandée, et le lecteur repart. Or cette demande écrit un fichier chez lui, et la veille qui tient son moteur voit ce fichier bouger et **redémarre le moteur**, ce qui emporte le tunnel et donc la voie qu'on vient d'ouvrir. Le lecteur, lancé une seconde plus tôt, se retrouvait à parler à un moteur en train de mourir : `closed by peer: 0`, lu ici comme un refus d'appairage, et la session tombait. La reconnexion d'après marchait pour la seule raison que le moteur avait fini de redémarrer et que la cadence était déjà écrite.

**Ce n'est pas nouveau et ce n'était pas oublié.** [D73](#d73-la-cadence-de-lécran-immobile-se-demande-depuis-le-côté-qui-regarde-2026-08-27-pendant-m4) dit exactement ça : « un moteur qui repart au milieu d'une session est cette session qui s'en va », d'où une demande faite à l'ouverture et jamais au milieu. Elle **était** faite à l'ouverture. Ce qui manquait est que le redémarrage tombe alors au milieu de **cette ouverture-là**.

**L'écran à filmer, lui, avait déjà la réponse.** Il a la même contrainte, il l'a rencontrée avant, et il a été réglé pour de bon : l'ordinateur d'en face **répond** s'il est déjà comme on le demande ou s'il redémarre pour l'être, et celui qui demande lâche la voie, attend, et redemande sur une voie neuve jusqu'à ce que la réponse soit « déjà ». Rien de tout ça n'est deviné depuis une voie qui casse.

**Décision : les deux demandes que le moteur d'en face ne lit qu'à son démarrage sont une seule affaire, réglée avant que l'image s'ouvre.** L'écran à filmer et la cadence de l'écran immobile voyagent maintenant dans la même boucle, sur la même voie, avec la même réponse en deux mots : « déjà » ou « je redémarre ». Une session ordinaire n'y passe qu'une fois, les deux réponses étant « déjà ».

**Et la réponse est pesée contre le moteur qui tourne, pas contre le fichier.** Le fichier est ce que le **prochain** moteur lira : répondre d'après lui dirait « tu l'as » à une session dont le moteur n'a pas encore redémarré. Ce que le moteur en marche a lu à son démarrage voyage donc jusqu'à la porte, comme l'écran filmé le fait déjà.

**Ce que ça donne à l'écran.** L'écran d'ouverture dit « L'ordinateur distant change sa façon d'envoyer un écran immobile, il redémarre… », comme il le dit déjà pour un changement d'écran. Quelques secondes de plus, dites, au lieu d'une session qui tombe.

**Le dialecte change des deux côtés**, celui du tunnel et celui du canal de commande : deux moitiés du produit installées à des dates différentes le disent au lieu de se mécomprendre. Les deux ordinateurs se mettent à jour ensemble.

## D115. La croix annule une ouverture, à n'importe quel moment de l'ouverture (2026-09-01, pendant M4)

**Le constat, de Victor.** Sur l'écran d'ouverture, la croix ne fait rien : « ça met juste en pause la barre de chargement pendant une seconde et c'est tout ».

**Ce qui se passait.** La croix demandait la fin de la session, et cette demande cherche une session à terminer : d'abord auprès du service, ce qui prend la seconde en question, puis dans ce que la fenêtre a écrit quand elle a lancé le lecteur. Pendant une ouverture, il n'y a ni l'un ni l'autre la plupart du temps, et la demande s'arrêtait là, sur une ligne de journal que personne ne lit. Plus loin dans l'ouverture, quand le lecteur venait de partir, elle l'arrêtait bien, mais la fenêtre restait vingt secondes de plus à attendre une image qui n'arriverait jamais.

**Et l'ouverture ne demandait qu'à la fin si on la voulait encore.** La question existait, mais elle n'était posée que pendant les six secondes de veille qui suivent l'image. Tout ce qui précède, qui est là où une ouverture passe son temps (la course aux adresses, un moteur d'en face qui redémarre, deux ordinateurs qu'on présente), ne la posait jamais.

**Décision : la croix pendant une ouverture est une annulation, et elle est dite avant tout le reste.** Elle n'attend plus qu'on lui trouve une session : elle marque l'ouverture comme lâchée, et c'est cette marque que l'ouverture lit **à chacun de ses pas**, du premier au dernier. Ce qui avait été lancé est arrêté en sortant, y compris un lecteur parti une seconde plus tôt, et l'appairage, qui est la plus longue attente de toutes, la lit à chacun de ses tours au lieu de tenir trente secondes.

**Une annulation n'est pas un échec.** Elle revient à l'accueil sans ligne rouge : celui qui l'a demandée n'a rien à apprendre de sa propre demande. Le journal la garde, l'écran non.

## D116. Changer d'écran chez l'hôte ne relance plus rien (2026-09-01, pendant M4)

**La demande de Victor.** « Pour switcher d'écran c'est beaucoup trop long, je ne voudrais pas avoir besoin de recharger la session pour pouvoir switcher, ça devrait être instantané comme Parsec, et vraiment pouvoir switcher entre les deux sans problèmes. »

**Ce que ça coûtait.** Le moteur d'en face lit dans sa configuration quel écran filmer, une fois, à son démarrage. En changer voulait donc dire le redémarrer, ce qui emporte son tunnel, et le tunnel emporte toutes les sessions qui passent dedans : une bascule d'écran coûtait une session qui tombe, un moteur qui repart, un client qui se reconnecte. Une dizaine de secondes, plus l'écran d'ouverture, pour un changement qui devrait être un battement de cils.

**Ce qui existait déjà, et qu'il suffisait de nommer.** La capture du moteur sait changer d'écran là où elle est : un écran choisi à la main, par le raccourci que le client peut envoyer, pose un rang et la capture se réinitialise dessus. C'est exactement le chemin qu'emprunte un changement de bureau, celui de Ctrl+Alt+Suppr, et une session le traverse sans s'en apercevoir. Ce qui manquait n'était pas le mécanisme, c'était de pouvoir demander un écran **par son nom**, qui est ce que le produit connaît.

**Décision : le moteur hôte apprend à filmer un autre écran sans redémarrer, et c'est le produit qui le lui demande.** Le nom demandé prime sur la configuration tant que ce moteur-là vit ; il est cherché à chaque réénumération et attendu pendant qu'un écran change de mode, exactement comme l'écran de la configuration, parce qu'il passe par la même porte. La demande arrive par le serveur de configuration du moteur, sur l'interface locale, avec les identifiants que le produit lui a donnés.

**Le service écrit toujours la note, et il la garde.** Elle est ce que le **prochain** moteur lira : un moteur qui redémarre pour une autre raison revient sur le bon écran. Ce qui change est ce qu'il fait ensuite : il demande au moteur en marche, et la veille qui le tient sait alors que le moteur est là où il doit être, donc elle ne le redémarre pas. Une seule réponse à un seul endroit, partagée entre les deux, faute de quoi ce serait un moteur qui redémarre pour toujours ou un qui ne redémarre jamais.

**Deux chemins finissent encore par un redémarrage, et la réponse le dit** : un moteur d'une compilation antérieure, qui ne sait pas qu'on peut le lui demander, et un ordinateur dont l'écran principal n'a jamais été nommé, c'est-à-dire un qui n'a jamais fini de démarrer un moteur. C'est exactement ce qui se passait avant, donc rien ne casse sur une machine à moitié à jour.

**Du côté qui regarde, il n'y a plus rien à appliquer.** L'écran de l'hôte quitte la liste des réglages qui attendent « Appliquer les changements » : on le choisit, la carte se referme, et l'image est sur l'autre écran. Les autres réglages y restent, parce qu'eux sont dits au moteur à son démarrage.

**Et un raccourci pour ne plus ouvrir le menu du tout.** « Écran suivant de l'hôte » passe d'un écran au suivant et revient au premier après le dernier. Une machine à un seul écran n'a nulle part où aller, et la touche y est simplement muette. Sans combinaison par défaut : c'est un choix, comme les deux autres raccourcis facultatifs.

**Ce que ça ne change pas.** La résolution de la session. Le moteur d'en face redimensionne l'écran qu'il filme dans l'image que la session a demandée, comme il le fait déjà pour un écran plus grand qu'elle : basculer sur un écran de taille différente ne renégocie rien et ne coûte rien. Le pointeur, lui, suit : les encodeurs redisent au moteur les dimensions et la position de l'écran filmé à chaque réinitialisation, donc la souris tombe au bon endroit sur le nouvel écran.

## D117. Tout se règle en direct, et il n'y a plus rien à appliquer (2026-09-02, pendant M4)

**La demande de Victor.** « Ce bouton appliquer en fait il me fait chier, j'aimerais faire exactement comme Parsec, c'est-à-dire tout peut être modifié en direct de la session sans avoir besoin de recharger la session, que ce soit le changement de résolution, de débit, d'encodage, bref la totalité des options disponibles. »

**Ce que ça coûtait.** [D27](#d27-la-taille-le-débit-et-le-codec-sappliquent-en-relançant-limage-2026-08-21-pendant-m4) avait posé une ligne « Appliquer les changements » : la taille, le débit et le codec sont dits au lecteur à son démarrage et jamais après, la cadence de l'écran immobile est dite au moteur d'en face au sien, donc en changer un voulait dire arrêter le lecteur et le relancer, écran d'ouverture compris, et parfois redémarrer le moteur d'en face. Quelques secondes à chaque essai, alors que régler un débit est une chasse : on pousse le curseur en regardant l'image, et une image qui disparaît à chaque cran est une chasse impossible. L'écran de l'hôte avait déjà quitté ce bouton ([D116](#d116-changer-décran-chez-lhôte-ne-relance-plus-rien-2026-09-01-pendant-m4)) ; celui-ci fait quitter tout le reste, et retire le bouton.

**Trois routes, une par nature du réglage.** Le débit et la cadence de l'écran immobile sont l'affaire du moteur d'en face : ils lui sont demandés là où il est, par la porte que D116 a ouverte pour l'écran, et qui prend maintenant les trois. Le codec et la résolution sont l'affaire du lecteur, qui refait son flux dans sa propre fenêtre. La résolution est les deux à la fois : l'ordinateur d'en face est d'abord prié de mettre son écran à la taille voulue, ou de garder le sien, exactement comme à l'ouverture, et ce qu'il répond qu'il affichera est ce que le lecteur est prié de devenir. La fenêtre de ZyrDesk reprend ensuite la forme de l'image.

**Chez le moteur hôte : un débit ou un plancher coûtent un encodeur, jamais une capture.** L'écran filmé passe par une réinitialisation de la capture, comme un changement de bureau. Le débit et le plancher n'ont pas besoin de ça : la boucle d'encodage lit ce qu'on lui demande au moment où elle bâtit un encodeur, avec un numéro de tour, et lâche son encodeur quand le tour a bougé ; le suivant est bâti sur la nouvelle demande. Ni événement à lever, ni à consommer, et deux flux ne peuvent pas se voler le leur. Le débit demandé en cours de route est ajusté comme le débit demandé à l'ouverture, part de la correction d'erreurs et du son retirée : l'ajustement, qui vivait dans le traitement RTSP, vit maintenant chez les encodeurs et sert aux deux lectures, et ce que coûte le son voyage avec la configuration du flux. Un débit est demandé au flux qui tourne ; un flux qui démarre repart de ce qu'il a négocié, qui est la parole la plus fraîche du client. Le plancher, lui, est celui du moteur pour toute sa vie, comme l'écran.

**Chez le moteur client : un fichier suivi, et le flux refait sur place.** Tout ce dont un flux est fait lui arrivait sur sa ligne de commande et n'était lu qu'une fois. Il suit maintenant un fichier, nommé par une option de sa ligne de commande, que ZyrDesk remplace entier à chaque changement et que sa boucle de session relit quatre fois par seconde, sur un rappel d'une minuterie de SDL, parce que la boucle de la boîte à outils n'est pas pompée pendant qu'un flux tourne. Une ligne, `clé=valeur` comme la ligne de statistiques : largeur, hauteur, cadence, débit, codec. Un débit seul ne change rien chez lui, le moteur d'en face ayant été prévenu directement ; il le garde pour le prochain flux qu'il annoncera. Une taille, une cadence ou un codec fait refaire le flux là où il est : le décodeur part sous son verrou, la connexion est arrêtée, l'hôte est interrogé sur ce qu'il fait tourner pour savoir s'il faut lancer ou reprendre, le flux est recalculé depuis les préférences exactement comme au premier démarrage, la connexion repart de la même façon, et le décodeur est rebâti par la boucle sur le chemin qu'elle prend déjà pour un rendu perdu. Même fenêtre, même processus, même session. Une seconde environ d'image figée, contre les secondes et l'écran d'ouverture d'avant. Un flux qui ne peut pas être refait termine la session comme une connexion perdue le ferait : le lecteur sort avec son code d'échec, et ZyrDesk le dit.

**Ce qui reste à relancer, et pourquoi.** Un moteur d'en face qui ne peut pas être prié, parce qu'il est d'une compilation antérieure ou qu'il ne répond plus. Le service d'en face le dit, « déjà » ou « je redémarre » pour la cadence, un refus pour le débit, et ZyrDesk relance alors l'image comme avant. Ce n'est pas un chemin de secours bricolé : c'est l'ancien chemin, laissé entier, et emprunté seulement quand le nouveau ne peut pas l'être.

**Les moteurs restent séparés, et chacun ne porte qu'un point de contact de plus.** Chez Sunshine, une porte, `POST /api/zyr/serve`, et une fonction qui lit trois demandes facultatives ; chez Moonlight, une option, `--follow-settings`, et une classe qui lit un fichier. Rien de ZyrDesk ne vit dans un moteur : l'un ne sait pas qui le prie, l'autre ne sait pas qui écrit le fichier. Les deux correctifs sont un commit chacun sur la branche `zyr/` de leur fork, et ils se rebasent sur une version amont comme les autres : quelques lignes touchées dans le code amont, tout le reste dans des fonctions et des fichiers à nous.

**Ce que ZyrDesk retient d'une session.** Ce que le lecteur affiche, tel qu'il a démarré puis tel qu'on le lui a dit depuis, parce que le lecteur est prié avec la ligne entière à chaque fois : prié d'un débit seul, il lirait une taille qu'on ne lui a jamais donnée. Ce qu'il affiche est ce que l'ordinateur d'en face a répondu qu'il afficherait, et non ce qui avait été demandé, et c'est l'ouverture qui le rend maintenant.

**Les dialectes changent des deux côtés**, le tunnel passe en version 14 et le canal de commande en 27 : le débit se demande au milieu d'une session, ce qu'aucune version d'avant ne savait dire. Les deux ordinateurs et les deux moteurs se mettent à jour ensemble.

**Des essais de logique sont écrits** là où il y a de la logique à casser : la ligne que le lecteur suit et son remplacement entier, les nouveaux messages du tunnel et du canal de commande, le corps de la demande faite au moteur hôte, et le plancher demandé au moteur en marche, qui doit être le nombre même que son fichier de démarrage porte.

## D118. Le serveur est facultatif, et les deux façons de se joindre cohabitent (2026-09-02, avant M5)

**Décision.** ZyrDesk se joint de deux façons, dans le même produit. Sans serveur : réseau local, VPN, adresse publique avec le port UDP 47000 renvoyé, deux machines qui s'écrivent l'une l'autre par empreinte ; aucun compte, aucune infrastructure, et rien de ce qui existe ne change. Avec un serveur : un compte, ses appareils, la présence, des contacts, des partages, la connexion en un clic d'où qu'on soit, et un relais en secours. Un service sans lien de compte ne contient aucun code qui parle au serveur : ce n'est pas un réglage sur « non », c'est une brique absente. Étude complète : [SERVER.md](SERVER.md).

**Comment les deux tiennent ensemble.** Les ordinateurs viennent de plusieurs sources et sont fondus par empreinte, ce que l'accueil fait déjà pour ce qui s'annonce et ce qui est écrit à la main ; le compte est une troisième source, et la carte le dit. Les autorisations s'additionnent : la liste écrite, la confiance au réseau local, et ce que le serveur présente par un ticket signé, dans la même fonction qui décide aujourd'hui. Le chemin le plus court gagne sans demander à personne : vers une machine vue sur le réseau local, le serveur n'est pas consulté. Un serveur injoignable dégrade et ne casse pas.

**Ce que ça remplace.** L'architecture initiale dessinait le broker comme le passage obligé d'une session ([ARCHITECTURE.md](ARCHITECTURE.md) §6) ; le jalon M4 a montré que le réseau local n'en a pas besoin, et Victor demande que personne n'en ait besoin. C'est la contrainte C5 lue jusqu'au bout.

## D119. Le chemin se choisit sous QUIC : quinn reste, l'aiguilleur migre, iroh est écarté (2026-09-02, clôture du réexamen prévu par D13)

**Décision.** La migration du relais vers le direct, et le retour, se font dans une couche à nous, sous la connexion QUIC : une prise virtuelle, l'aiguilleur, qui donne à chaque ordinateur d'en face une adresse fictive et stable, tient la liste des chemins réels vers lui (adresses directes, relais), envoie chaque paquet par le chemin élu et remet chaque paquet reçu comme venant de l'adresse fictive. QUIC ne voit jamais un chemin changer : mêmes clés, même fenêtre de congestion, aucune reconnexion. Le transport reste quinn, et le seul fichier qui le nomme ne bouge pas.

**Ce qui a été vérifié.** quinn 0.11 sait qu'un client change d'adresse locale et que le serveur le suive, et rien d'autre : aucune API pour envoyer à une autre adresse du pair, aucun multichemin (demande ouverte depuis 2019, aucune fusion en 2026), une adresse préférée qui ne se dit qu'à la poignée de main. iroh 1.1.0 (2026-08-25) a le multichemin, la perforation et la découverte d'adresse dans QUIC, et un relais éprouvé ; mais tout cela repose sur noq, son fork de quinn (1.2.0, dix versions depuis février 2026), sur un relais en WebSocket et TCP, et porte des défauts ouverts qui nous regardent : des datagrammes remis en rafales avec des trous de 100 à 800 ms sur un flux audio temps réel (n0-computer/iroh#4309), des paquets de poignée de main jetés sous charge (#4325). Il n'existe pas de « quinn plus la perforation d'iroh ».

**Pourquoi l'aiguilleur.** C'est le mécanisme de Tailscale sous WireGuard, et celui d'iroh lui-même avant 0.96 ; la seule chose qu'iroh lui reprochait, des à-coups à la bascule, venait d'un contrôleur de congestion relancé à chaque changement, et le nôtre ne se relance pas et ne réagit pas aux pertes. Un millier de lignes à nous, testables sans réseau, contre un changement de transport pour une fonction qu'on obtient sans en changer. Le jour où le multichemin QUIC sera standard et noq mûr, l'aiguilleur est précisément l'endroit d'où il pourra entrer.

**Ce que ça coûte.** Une taille de paquet figée à 1200 octets sur les connexions qui passent par l'aiguilleur, découverte de MTU désactivée, pour que tout paquet tienne dans un datagramme du relais : quelques dizaines d'octets par paquet de moins qu'un chemin direct n'en permettrait. Et une estimation d'aller-retour fausse pendant un ou deux échanges après une bascule vers un chemin plus long, qui ne touche que les flux fiables, minuscules.

## D120. Le relais est le nôtre : des paquets opaques entre deux empreintes, en datagrammes QUIC sur UDP 443, en secours seulement (2026-09-02, avant M6)

**Décision.** Le relais reprend le modèle de confiance de DERP et d'iroh, des paquets déjà chiffrés adressés par clé publique, transportés de l'aveugle, et change leur transport : une connexion QUIC extérieure par session et par appareil, en datagrammes, dans laquelle chaque datagramme porte un paquet entier du tunnel, remis tel quel à l'autre bout de la même session. Le premier flux porte un laissez-passer signé par le broker, qui nomme les deux empreintes ; le relais ne transmet qu'entre elles. Il vit dans le même binaire que le broker, se débraye, et le même port UDP répond au miroir qui dit à un appareil son adresse vue de l'extérieur.

**Pourquoi des datagrammes et pas TCP.** DERP et iroh transportent sur une connexion HTTPS passée en WebSocket, ce qui passe le mieux les pare-feu et se comporte le plus mal pour de la vidéo : une perte y bloque tout ce qui suit. Sur des datagrammes, une perte vers le relais est une perte, absorbée comme sur un chemin direct par la correction d'erreurs des moteurs. Le repli TCP sur 443 reste hors périmètre, à la même place dans l'aiguilleur, pour le jour où un réseau qui coupe tout UDP se présentera assez pour être mesuré.

**Pourquoi seulement en secours, et pourquoi d'abord quand même.** Un chemin direct validé gagne toujours sur le relais, quel que soit l'aller-retour ; mais la session part par le relais dès qu'il est prêt tant qu'aucun direct n'est validé, parce qu'on ne fait pas attendre une session pendant qu'on cherche mieux. Dans le cas courant le direct est validé avant la fin de la poignée de main, et le relais n'a rien porté. La branche de relais est gardée chaude toute la session, pour qu'un direct qui meurt ne coûte pas une reconnexion.

**Ce que ça coûte.** Un aller-retour de plus, celui vers le relais, et un chiffrement de plus par paquet sur les deux branches, ce qui est voulu : la couche extérieure authentifie et protège le relais lui-même. Le critère de M6 reste « surcoût d'au plus un aller-retour vers le relais ».

## D121. Le serveur ne parle jamais en clair, et un certificat auto-signé s'épingle (2026-09-02, avant M5)

**Décision.** Le serveur refuse au démarrage tout point d'écoute HTTP sans TLS qui ne soit pas sur une adresse de boucle locale, la seule exception étant un mandataire inverse sur la même machine qui termine TLS ; et le service refuse une adresse de serveur en `http://`. Trois façons d'avoir TLS à l'installation : un mandataire existant avec un certificat valide, un certificat auto-signé généré par le script, un certificat fourni. Un auto-signé s'épingle par l'empreinte de sa clé publique, affichée par le script et confirmée par une personne dans l'application, puis exigée à chaque connexion ; un serveur qui change de clé est refusé avec une phrase qui le dit.

**Ce que ce n'est pas.** Un client qui accepte tout. La signature du certificat est toujours vérifiée, TLS 1.3 seulement, un certificat public passe par la voie ordinaire, et l'épinglage n'intervient que quand cette voie a échoué, pour demander à un humain de comparer deux nombres. C'est la sémantique de SSH et celle de l'appairage des appareils déjà en place. Épingler la clé et non le certificat permet de renouveler le second sans toucher au premier.

**Un détail qui a été vérifié.** Sur Debian, un `openssl req -x509` nu produit une autorité de certification (`CA:TRUE`), ce qui est faux pour un serveur ; le script écrit explicitement les extensions d'une feuille, avec le nom et l'adresse IP en noms alternatifs.

## D122. Un contact n'ouvre rien : les partages sont explicites, nommés, et se retirent (2026-09-02, clôture de O2)

**Décision.** Accepter un contact ne lui donne que de se voir mutuellement dans une liste et de recevoir des partages. Un partage nomme UNE machine, porte des permissions et une expiration facultative, se retire d'un clic, session en cours comprise. Ses propres appareils se joignent sans approbation, parce que c'est le sens d'un compte. Le MVP accorde toutes les permissions et n'en fait respecter qu'une, l'accès ; les autres (clavier, souris, son) sont écrites dans le modèle dès le premier jour et se feront respecter par retenue des entrées dans le tunnel, sans toucher aux moteurs. L'approbation à chaque session, comme la touche Ctrl+F1 de Parsec pour un invité, est une option par partage prévue et absente du MVP, faute d'invite sur l'hôte.

**Ce qu'on garde de Parsec, et ce qu'on fait autrement.** Gardé : la demande symétrique, « un ami n'a rien par défaut », les permissions par relation, l'approbation à la session comme option. Autrement : chez nous le partage nomme une machine, là où Parsec ouvre tous les PC d'un compte dès qu'un ami a une permission. C'est la ligne posée par Victor : un contact ne reçoit pas automatiquement tous les droits.

**Ce que ça clôt.** O2, dont le défaut prévoyait une approbation au premier appairage de chaque paire : entre appareils du même compte elle n'apporte rien que le compte ne garantisse, et entre comptes le partage est cette approbation. Le reste de O2 tient : activation de l'hôte réservée aux administrateurs, TOTP obligatoire à la bêta.

## D123. Le serveur s'installe par un script dans l'esprit de Proxmox-Tools, aux couleurs de ZyrDesk (2026-09-02, avant M5)

**Décision.** `bash install.sh` sur un Debian 12 ou 13, en conteneur LXC non privilégié de Proxmox ou non : bannière, vérifications qui expliquent au lieu d'échouer, questions pré-remplies de ce qui est détecté, récapitulatif, étapes derrière une roue réécrites en `✓` ou `✗`, résumé final avec ce qu'il faut savoir, relance qui met à jour ou reconfigure, désinstallation à deux paliers. Le style est relevé sur les sources des scripts Proxmox-Tools de Victor (panneaux ouverts, glyphes, défauts entre crochets, « oui » tapé en entier avant ce qui ne se défait pas, français si la machine est en français) ; la palette est celle de `design.css` (l'or du logo en accent, ses états vert, orange, rouge), et non l'orange de Proxmox.

**Ce que le script ne fait pas.** Il ne configure pas le mandataire inverse, n'ouvre pas la box, n'installe pas de pare-feu (dans un conteneur non privilégié, c'est Proxmox ou la box qui décident, et `nftables.service` y échoue), ne compile pas sauf sur demande : il télécharge le binaire publié par l'intégration continue et vérifie son empreinte. L'unité systemd n'emploie que ce qui survit à un conteneur non privilégié ; le durcissement fondé sur les montages est un complément activé hors conteneur, parce que le profil AppArmor de Proxmox et le systemd 257 de Debian 13 le refusent.

## D124. Les cartes de l'aiguilleur vivent en 240.0.0.0/4, sa prise parle les deux versions d'IP, et le miroir ne signe rien (2026-09-02, M5 tranche 2)

**Ce qui était prévu.** SERVER.md §4.1 donnait à chaque ordinateur d'en face une adresse de carte dans un préfixe IPv6 privé, et §4.2 laissait la réponse du miroir sans signature sans le dire tout à fait.

**Ce qui a été fait, et pourquoi.** La carte est une adresse IPv4 de `240.0.0.0/4`, tirée de l'empreinte de l'ordinateur : le transport refuse une adresse IPv6 sur une prise qui ne parle qu'IPv4, et il y a des machines où IPv6 est coupé. Une carte IPv4 passe partout, y compris sur la prise à double pile que l'aiguilleur ouvre dès que le système l'accepte, où le transport l'écrit lui-même sous sa forme IPv6 mappée ; ce bloc n'est routé nulle part et ne se pose sur aucune carte réseau, si bien qu'une carte ne peut jamais être prise pour un lieu. Les paquets partis vers une carte avant qu'aucun chemin ne réponde, ceux de la poignée de main, sont gardés, huit au plus, et partent d'un coup au premier écho : QUIC les aurait réémis, mais une seconde plus tard. Le miroir répond sans signer : sa réponse n'est qu'une adresse à essayer, et une adresse mensongère est une sonde de plus qui ne reviendra pas ; signer aurait fait porter la clé du serveur à un port que n'importe qui atteint, pour rien. Quand la box a changé le port en sortant, l'hôte nomme aussi son adresse vue sur son propre port 47000, là où mène un port renvoyé à la main.

**Ce qui reste de la tranche.** Le mappage de port chez l'hôte (UPnP, NAT-PMP, PCP) n'est pas fait : la perforation par sondes couvre les box ordinaires, un port renvoyé à la main couvre le reste, et le mappage viendra en complément, avec sa préférence, plutôt que d'alourdir le MVP d'une pile de dépendances avant d'avoir mesuré ce que la perforation laisse passer.

## D125. La file d'envoi tient une image entière, et le transport est tenu à la version dont la comptabilité est juste (2026-09-02, pendant M5)

**Le relevé, et il vient de l'usage.** Deux sessions d'affilée, sur deux ordinateurs distants différents et deux réseaux différents : sur l'une l'image s'affiche puis meurt au bout de douze secondes, sur l'autre elle ne s'affiche jamais. Le lecteur dit la même chose des deux côtés : `Unrecoverable frame 1: 61+23=84 received < 112 needed`, puis `Waiting for IDR frame` indéfiniment, puis `Control stream received unexpected disconnect event` à douze ou treize secondes. Le journal du service, lui, ne dit rien du tout après l'ouverture de la voie.

**Première cause : la file d'envoi était plus courte qu'une image.** Elle faisait 128 Kio fixes ; une image clé en 1080p à 80 Mb/s en fait 123, et elle sort de l'encodeur d'un bloc que la pompe pousse dans la file bien plus vite que le transport ne le met sur le fil. La file déborde donc à chaque image clé, sur le meilleur des réseaux, et le transport jette les plus anciens paquets, c'est-à-dire le début de l'image en cours. Un quart d'image manquant ne se répare pas : la correction d'erreurs du protocole vidéo couvre quelques paquets, pas trente. Le lecteur demande alors une image clé, qui est coupée à son tour, et l'image ne s'établit jamais. La file est maintenant taillée sur le profil du flux, six images avec un plancher de 256 Kio, au même endroit que la fenêtre de congestion et à partir du même calcul : c'est la même arithmétique, elle ne vit qu'une fois. Sur un chemin qui ne peut vraiment pas prendre le débit, ce que ça met en attente est borné par ces mêmes six images.

**Seconde cause, et c'est elle qui tuait la session : le transport soustrait deux fois.** `quinn-proto` 0.11.17 a déplacé la comptabilité de la file d'envoi dans la file elle-même, sans retirer la soustraction que faisait l'appelant. Chaque paquet jeté pour faire de la place est donc décompté deux fois, la taille de la file passe sous zéro, et le transport se met à émettre depuis une file dont il ne sait plus la longueur, avant de paniquer en plein envoi. La panique tombe dans la tâche qui pilote la connexion : la session meurt d'un coup, à l'autre bout, sans qu'une ligne de journal existe pour le dire. Le produit est tenu à la 0.11.16, la dernière dont la comptabilité est juste, avec un essai qui envoie une rafale plus grosse que la file et vérifie que la connexion y survit. Cet essai échoue en 0.11.17 : le jour où la correction est publiée, il dira lui-même que la version peut être relevée.

**Et, une fois de plus, le silence.** [D86](#d86-une-taille-quon-ne-peut-plus-changer-se-prend-sur-le-plancher-pas-sur-le-plafond-2026-08-28-pendant-m4) posait la règle : ce qui se jette sans le dire se paye en journées. Elle n'était appliquée qu'à moitié. La voie ouverte par l'ordinateur qui regarde n'attendait son tunnel nulle part, à la différence de la porte : une pompe qui meurt y laissait une voie qui ne portait plus rien et n'en disait rien, et la session mourait plusieurs secondes plus tard à l'autre bout, d'un silence. La ronde des voies lit maintenant l'état du tunnel, écrit pourquoi il s'est arrêté, panique comprise, et referme la voie. Les paquets jetés faute de place se comptent en les demandant avant de les remettre, et non en déduisant après coup un écart qui contenait aussi ce qui attendait son tour : la ligne précédente criait au loup dès le premier paquet en attente. Et les deux côtés écrivent, quand une session se termine, tout ce qu'elle a porté : ce qui est entré dans le tunnel, ce qui en est sorti sur le fil, ce qui a été jeté et pour quelle raison, la place restante et l'aller-retour.

## D126. La fenêtre d'envoi se mesure en temps de flux, pas en kilo-octets (2026-09-03, pendant M5)

**Le relevé, et il vient de l'usage.** Une session de onze minutes entre deux machines du même réseau local, à sept millisecondes de route et 15 Mb/s. L'ordinateur regardé a jeté 4331 paquets vidéo faute de place dans sa file d'envoi, dont 835 d'un seul coup ; à la même seconde, celui qui regardait n'a reçu que deux des onze paquets d'une image, n'a pas pu la reconstituer, a demandé une image de reprise en boucle, et la session est morte treize secondes plus tard. Ces chiffres n'existaient pas la veille : ce sont les compteurs de [D125](#d125-la-file-denvoi-tient-une-image-entière-et-le-transport-est-tenu-à-la-version-dont-la-comptabilité-est-juste-2026-09-02-pendant-m5) qui les ont écrits, des deux côtés.

**La cause.** La fenêtre valait `2 x débit x aller-retour + une image`, avec un plancher de 64 Kio. Sur un réseau local, les deux premiers termes ne pèsent presque rien et c'est le plancher qui décide : 64 Kio, soit un trentième de seconde de flux à 15 Mb/s. Or cette fenêtre est ce qui peut être en vol sans réponse, et rien ne part au-delà. Un ordinateur d'en face occupé ailleurs pendant deux dixièmes de seconde — un portable qui compile, un navigateur qui charge — cesse d'accuser réception pendant ce temps, la fenêtre est pleine au bout de trente millisecondes, la file se remplit, et tout le reste est jeté. Le transport jette les plus anciens, c'est-à-dire l'image en cours de sortie : elle arrive en morceaux, ne se reconstitue pas, et l'image suivante attend une image clé.

**Ce qui est fait.** Le plancher devient un temps : une demi-seconde de flux, quel que soit le débit. À 15 Mb/s c'est environ 915 Kio au lieu de 64. Et ça ne coûte rien tant que le chemin va bien : ce qui est en vol est sur le fil, pas en attente quelque part, et le débit reste celui de l'encodeur, que ce contrôleur n'a jamais eu pour rôle de limiter ([D13](#d13-transport--quinn-maintenant-iroh-reconsidéré-à-m6-2026-08-07-clôture-de-o5)). Le nombre fixe était le vrai défaut : 64 Kio ne veut rien dire tant qu'on ne sait pas combien de secondes de vidéo ça représente.

**Ce que le journal dit maintenant.** La fenêtre, en octets, sur la ligne des pertes et dans le résumé de fin de session : une session qui jette des paquets avec une fenêtre pleine est une session dont l'autre bout s'est tu, une qui en jette avec de la place est un chemin qui ne les prend vraiment pas, et rien ne distinguait les deux. S'ajoute une ligne quand plus rien n'arrive pendant plusieurs relevés d'affilée : la voie tient, la connexion tient, la route est aussi courte qu'avant, et pas un paquet ne vient. C'est la forme que prend ici toute panne d'en face, et c'est la seule qui ne se voyait nulle part.

**Confirmation d'hier, au passage.** Le journal de la machine regardée porte, deux fois, `session ended: task ... panicked with message "datagrams.outgoing.payload_bytes desynchronized"`. C'est mot pour mot la panique de quinn-proto 0.11.17 décrite en D125, prise sur le fait sur une troisième machine.

## D127. Sur un aiguilleur, le paquet ne change plus de taille (2026-09-03, M6)

**Le problème naît le jour où il y a deux routes.** QUIC cherche la plus grosse taille de paquet que le chemin porte, en sondant vers le haut ; quand ces gros paquets se mettent à disparaître, il décide que le chemin s'est bouché et retombe au plancher. C'est ce que [D86](#d86-une-taille-quon-ne-peut-plus-changer-se-prend-sur-le-plancher-pas-sur-le-plafond-2026-08-28-pendant-m4) racontait déjà pour la vidéo. Sous un aiguilleur, ce mécanisme devient franchement dangereux : la découverte trouve ce que porte la route du moment, l'aiguilleur bascule sur une autre, et cette autre reçoit des paquets qu'elle ne prend pas. Le transport voit alors des paquets s'évanouir, croit à un chemin bouché, et retombe — en silence, au milieu d'une session, et pour une raison qui n'a rien à voir avec le chemin. Avec le relais, le cas cesse d'être théorique : sa branche porte 1200 octets et pas un de plus.

**Ce qui est fait.** Un point d'accès posé sur un aiguilleur fige sa taille de paquet à 1200 octets, le plancher que QUIC exige de tout chemin, et coupe la découverte. Toute route porte donc tout paquet, par définition, la route relayée comprise. Ça coûte quelques dizaines d'octets par paquet sur les flux fiables, qui ne portent rien de gros ; la vidéo, elle, était déjà taillée sur ce plancher et ne perd rien. La branche vers le relais fait l'inverse et le doit : elle part de 1280 octets, le plancher d'IPv6, et découvre au-dessus, parce qu'il lui faut porter ces 1200 octets plus son enveloppe. Un chemin qui n'y arrive pas rend le relais inutilisable, et le service le dit plutôt que d'essayer.

## D128. Un laissez-passer de relais vaut tant qu'il vit, un ticket vaut une fois (2026-09-03, M6)

**Deux choses signées, deux règles.** Un ticket de session est consommé au premier usage : rejoué, il est refusé, et c'est ce qui empêche de rouvrir une session avec un ticket ramassé au passage. Un laissez-passer de relais ne l'est pas, et ses cinq minutes de validité disaient déjà pourquoi ([SERVER.md](SERVER.md) §3.7, « le temps d'atteindre le relais, avec de la place pour un essai ») : un appareil dont la connexion au relais a lâché — une box qui change de port, un réseau qui hoquette — revient avec le même papier. Le consommer aurait fait de la moindre coupure une session perdue.

**Ce qui protège le laissez-passer n'est pas une mémoire, c'est un certificat.** Il nomme son porteur par empreinte, et le relais ne le lit qu'après avoir vérifié que l'appareil en face présente ce certificat-là : TLS a déjà prouvé qu'il en détient la clé. Un laissez-passer volé sur le canal vivant ne sert donc à personne d'autre. Au passage, le relais n'a plus rien à retenir de qui est passé, ce qui est exactement ce qu'on lui demande.

## D129. Un ordinateur du compte se joint par une rencontre, même quand on le voit sur le réseau local (2026-09-03, pendant M6)

**Le relevé.** Le premier essai du relais entre deux PC côte à côte a échoué là où personne ne regardait. Le direct coupé au pare-feu, la session ne s'est pas ouverte du tout : `racing 3 addresses: 192.168.2.5:47000, 192.168.1.5:47000, 192.168.56.1:47000`, puis trois fois « timed out ». Pas une ligne sur le relais, et pour cause : il n'y en avait pas.

**La cause.** Une carte par ordinateur, et son adresse était celle du réseau local, parce que l'annonce locale l'avait vu. Un ordinateur nommé par une adresse est joint à cette adresse : pas de rencontre, pas d'aiguilleur, pas de relais. Le compte ne servait qu'à colorer la carte. Sur le réseau local ça marchait, et c'était ce qu'on voulait au jalon M5 ; mais une adresse est un chemin unique, sans moyen d'en changer et sans retour possible quand elle cesse de porter, et ça n'est pas resté vrai une fois qu'il y avait mieux à offrir.

**Ce qui est fait.** Un ordinateur du compte se joint par une rencontre dès qu'une rencontre peut être obtenue : le serveur répond, l'autre est en ligne et prêt. La rencontre ne retire rien et ajoute tout : elle donne à l'aiguilleur les adresses que cette machine connaissait déjà de lui, sondées les premières et donc élues les premières sur un réseau local, plus celles que l'autre nomme, plus le relais. Ce qui reste au knock à l'adresse est ce qui n'a pas mieux : un ordinateur d'aucun compte, un serveur injoignable, un ordinateur pas prêt. Le serveur ne porte toujours rien d'une session locale, ce que son compteur dit tout seul.

**Ce que ça coûte.** Un aller-retour vers le serveur avant de frapper, une dizaine de millisecondes chez soi, contre un chemin qui se répare et un relais en secours. Et si le serveur ne répond pas, le comportement d'avant revient tel quel : c'est la même règle qui gouverne les deux, « une rencontre quand elle est possible, l'adresse quand elle ne l'est pas », et pas deux cas écrits séparément.

## D130. La patience d'un aiguilleur se compte du dernier chemin qui a répondu (2026-09-03, pendant M6)

**Le relevé.** Première session tenue de bout en bout par le relais : onze minutes de 1080p60 entre deux lignes différentes, image irréprochable (0,00 % d'images perdues par le réseau, 0,02 % par la gigue, 1 ms de latence réseau moyenne). Puis, à onze minutes, tout s'arrête d'un coup. Les deux côtés écrivent la même ligne à la même seconde : `card 240.…:47000: nobody answered, forgotten`.

**La cause, et elle n'a rien à voir avec le relais.** L'aiguilleur oubliait un ordinateur attendu quand il n'avait plus aucun chemin **et** que l'attente avait plus de deux minutes, ces deux minutes étant comptées depuis l'ouverture de la session. Autrement dit : passé deux minutes, la première seconde où tous les chemins meurent, la session était jetée. La carte disparaissait, et tout ce que le transport confiait ensuite pour cet ordinateur partait à la poubelle en silence, sans retour possible même si le chemin revenait. Ces deux minutes voulaient dire « personne n'a jamais répondu, on arrête d'appeler » ; elles disaient en fait « toute session de plus de deux minutes meurt au premier hoquet ».

**Ce qui est fait.** La patience se compte du dernier chemin qui a répondu. Une session dont tous les chemins meurent est gardée deux minutes de plus, ce qui laisse largement le temps à un relais qui hoquette ou à une box qui lâche sa traduction de revenir. Et le rythme des sondes repart de zéro avec elle : les secondes qui suivent la mort du dernier chemin valent exactement les secondes qui suivent l'ouverture d'une session, donc une sonde toutes les 200 ms pendant cinq secondes, et non une toutes les quinze secondes.

**Et le silence, encore.** Une branche de relais qui casse ne le disait nulle part : la tâche qui la lisait se terminait sans un mot, et l'ordinateur d'en face mourait plusieurs secondes plus tard d'une absence. Elle écrit maintenant qu'elle est partie. C'est la même règle que [D86](#d86-une-taille-quon-ne-peut-plus-changer-se-prend-sur-le-plancher-pas-sur-le-plafond-2026-08-28-pendant-m4) et [D125](#d125-la-file-denvoi-tient-une-image-entière-et-le-transport-est-tenu-à-la-version-dont-la-comptabilité-est-juste-2026-09-02-pendant-m5) posaient déjà, appliquée à la route qui restait muette.

## D131. Le nom d'un relais mène à plusieurs adresses, et on les essaie toutes en même temps (2026-09-03, pendant M6)

**Le relevé, et il vient de l'intégration continue.** L'essai qui vérifie que deux ordinateurs d'un compte se rencontrent et ouvrent leur branche de relais échouait sur les machines de GitHub, Linux et Windows, et passait partout ailleurs. C'est la forme la plus trompeuse qu'un défaut puisse prendre : le journal de l'essai s'arrête sur « présenté par le serveur », et plus rien après. Pas d'erreur, pas de refus, pas de branche.

**La cause.** Le serveur donne son relais par son nom, celui de son adresse publique. L'appareil demandait au système à quoi ce nom correspond, prenait la première adresse rendue et n'essayait que celle-là. Sur ces machines, `localhost` mène d'abord à `::1` puis à `127.0.0.1`, et le relais de l'essai n'écoute qu'en IPv4 : la branche partait vers une adresse où personne ne répond et attendait là les trente secondes de patience du transport, pendant que l'essai, lui, en attendait huit. Ce n'est pas un défaut d'essai. Une machine dont l'IPv6 est configuré et cassé, ce qui court les rues, se voit donner l'adresse IPv6 de tous les noms qu'elle résout et n'atteint rien derrière ; et le relais existe précisément pour les réseaux qui vont mal.

**Ce qui est fait.** Toutes les adresses du nom sont essayées en même temps, et la première branche ouverte gagne ; les autres sont abandonnées là où elles en sont. Les prendre l'une après l'autre aurait coûté la patience entière du transport pour chaque adresse qui ne mène nulle part, et cette attente-là tombe sur les sessions qui n'ont que le relais. Chaque échec s'écrit avec son adresse, si bien que le cas se lit dans le journal au lieu de se deviner.

**Et un essai qui demandait une promesse que personne ne fait.** Le même relevé portait un second échec, sous Windows seulement. L'essai de la rafale de [D125](#d125-la-file-denvoi-tient-une-image-entière-et-le-transport-est-tenu-à-la-version-dont-la-comptabilité-est-juste-2026-09-02-pendant-m5) envoyait un datagramme après avoir fait déborder toutes les files exprès, et attendait qu'il arrive. Or un datagramme a le droit de se perdre, c'est toute la différence avec un flux fiable, et une rafale faite pour tout faire déborder est le meilleur moyen de le perdre. Ce que cet essai doit prouver est que la connexion survit, pas que ce paquet-là arrive : il le demande maintenant à un flux fiable, qui est renvoyé jusqu'à ce qu'il passe.

## D132. Une route qui meurt le dit, et cesse d'être empruntée (2026-09-03, pendant M6)

**Le relevé.** Session relayée entre deux fibres, trente-trois secondes. Image nette, route à sept millisecondes, puis tout s'arrête. À la même seconde, les deux ordinateurs écrivent la même chose : leur file d'envoi déborde, puis plus rien n'arrive de l'autre bout. Le lecteur note `Frames dropped by your network connection: 0.00%` et meurt sur `Control stream received unexpected disconnect event`. Le relais du serveur n'a rien à redire : il a porté 17 Mo et personne n'a approché son quota, qui est à 60 Mb/s. Et les deux aiguilleurs n'écrivent pas une seule ligne pendant toute la panne.

**Trois silences, trouvés en lisant le code et non les journaux, ce qui est bien le problème.**

Le premier : l'aiguilleur abandonne une route après trois sondes sans écho, soit six secondes, et ne le dit nulle part. La seule ligne qu'il écrive jamais à ce sujet arrive deux minutes plus tard, quand il oublie l'ordinateur tout entier. Or le moment où la route meurt est précisément la nouvelle.

Le deuxième, et c'est un vrai défaut : la route élue n'était jamais rendue. L'élection ne choisit rien quand il ne reste aucun chemin, et laissait donc la route morte en place. Tout ce que le transport confiait ensuite partait dans un chemin abandonné, sans un mot et sans retour possible, alors que le mécanisme qui garde les derniers paquets en attendant qu'une route réponde existe déjà et ne servait qu'à l'ouverture d'une session. La route élue est maintenant rendue quand elle est abandonnée : ce qui suit est gardé, et repart entier dès qu'une route répond.

Le troisième : la branche vers le relais répond « porté » ou « pas porté » à chaque paquet, et cette réponse était jetée. Une route relayée est deux routes à la suite, chacune avec sa file, et rien ne disait ce que la première refusait. La branche compte maintenant ce qu'elle porte et ce pour quoi elle n'avait pas de place, en demandant la place avant de confier le paquet, exactement comme la pompe le fait depuis [D125](#d125-la-file-denvoi-tient-une-image-entière-et-le-transport-est-tenu-à-la-version-dont-la-comptabilité-est-juste-2026-09-02-pendant-m5).

**Ce que ça ne fait pas.** Ça ne dit pas pourquoi cette session-là est morte. Ça fait que la prochaine le dira : soit la branche écrit qu'elle n'avait plus de place, et la saturation est du côté de l'ordinateur qui envoie ; soit elle ne dit rien, la route élue est abandonnée avec sa ligne, et la perte est sur le fil. Les deux se ressemblaient trait pour trait, et c'est pour ça qu'on ne tranchait pas.

**L'essai suivant, le soir même, a répondu à moitié.** Deux minutes et demie de session relayée, puis la route s'arrête de porter pendant six secondes : la ligne existe maintenant, `the relay at … stopped answering and is given up`, suivie une seconde plus tard de la même route reprise à sept millisecondes. Aucune ligne de branche saturée, donc les paquets sont bien partis des deux ordinateurs et la perte est sur le fil. L'aiguilleur et le tunnel ont tous les deux survécu à la coupure ; ce sont les moteurs qui ont renoncé, le canal de contrôle du lecteur ayant déclaré l'autre bout mort au bout de huit secondes de silence.

**Et un quatrième silence, au milieu.** Le relais est le seul endroit qui voit les deux moitiés d'une route relayée, et il ne disait rien de ce qu'il voyait : sur les deux ordinateurs, une moitié de route qui lâche et une session qui s'arrête s'écrivent exactement pareil. Il écrit maintenant, quand un côté se remet à parler, combien de temps il s'était tu, et il dit en fin de session ce que chacun des deux a envoyé. C'est ce qui manque pour savoir laquelle des deux lignes cède, et il n'y avait aucun autre endroit d'où le savoir.

## D133. Une carte se rend à la session qui la tient, et à aucune autre (2026-09-03, pendant M6)

**Le relevé, et c'est le relais qui l'a donné.** Quatre sessions mortes dans la soirée, à trente-trois secondes, deux minutes trente, trente-six secondes et vingt-trois secondes, toutes de la même façon : plus rien ne traverse, les deux files débordent, les moteurs renoncent. Les lignes ajoutées le même soir ont tranché en une fois :

```
session h6XMjvdgxgqnS0jc: relayed, 17731 kB carried
  0829cc… (PC-VICTOR) sent 1165 kB,  last heard 587 ms ago,   0 packet(s) could not be handed to it
  1aed56… (PC-SAV)     sent 16566 kB, last heard 10359 ms ago, 0 packet(s) could not be handed to it
```

L'ordinateur regardé n'avait plus rien envoyé au relais depuis dix secondes et demie, pendant que l'autre parlait encore. Le relais n'a rien refusé, rien perdu, et les compteurs du système sur le serveur (`/proc/net/snmp`, `ip -s link`) ne bougent pas d'un paquet sur toute la durée de l'essai. La panne était donc entièrement chez l'ordinateur regardé, qui avait cessé d'émettre, sondes de l'aiguilleur comprises, sans que son propre journal en dise un mot.

**La cause.** Une carte vaut pour un ordinateur, pas pour une session : elle est tirée de l'empreinte, donc deux sessions de suite vers la même machine la partagent. Or l'interface ouvre une vraie session pour chaque question qu'elle pose au loin, ne serait-ce que pour lire un journal, et le tunnel de ces sessions-là ne se referme pas tout de suite : il tient trente secondes avant de renoncer, faute de nouvelles. Le déroulé était donc immanquable. Une session de question s'ouvre et prend la carte. La vraie session s'ouvre dix secondes plus tard et prend la même carte. Trente secondes après la première, son tunnel abandonne, et la porte rendait alors la carte, celle de la session en cours. À partir de cet instant l'aiguilleur ne connaît plus personne derrière cette carte : les sondes ne partent plus, tout ce que le transport confie est jeté en silence, et l'ordinateur d'en face meurt d'une absence.

**Ce qui est fait.** Rendre une carte se fait au nom d'une session, et n'a d'effet que si c'est bien cette session-là qui la tient. Et la porte ne rend plus rien du tout : la carte appartient au compte, qui l'a prise quand le serveur a présenté la session et qui la rend quand le serveur dit qu'elle est finie. Un tunnel qui renonce une demi-minute après son dernier paquet n'est pas ce qui décide de la fin d'une session, et n'avait aucune raison de disposer de quoi que ce soit.

**La leçon, la même que d'habitude.** Trois soirées à chercher dans le réseau ce qui était dans une table en mémoire, parce que la seule chose que faisait le défaut, c'était de se taire. Ce sont les lignes du relais, écrites deux heures plus tôt pour une tout autre hypothèse, qui ont montré du doigt la bonne machine en une seule lecture.

## D134. Le tunnel de l'ordinateur regardé suit le débit de la session, et pas un débit nominal (2026-09-04, pendant M6)

**Le relevé.** Deux journaux de la même session, à quatre-vingts mégabits, la même seconde :

```
PC-SAV     (celui qui regarde) : 5000000 bytes may be out unanswered at once
PC-VICTOR  (celui qu'on regarde) : 1250000 bytes may be out unanswered at once
             1575571 packets into the tunnel, 20729 thrown away for want of room
```

Les deux ordinateurs portaient la même session et ne tenaient pas la même fenêtre. Cinq millions d'octets, c'est une demi-seconde de flux à quatre-vingts mégabits, ce que le contrôleur de congestion est écrit pour tenir. Un million deux cent cinquante mille, c'est une demi-seconde à vingt : le profil par défaut. L'ordinateur qui envoie toute la vidéo tenait donc un huitième de seconde là où il devait en tenir une demie, et jetait vingt mille paquets faute de place.

**La cause.** Le côté client construit son tunnel au moment où la session s'ouvre, et connaît son débit : `Reach` le lui apporte. Le côté hôte n'a pas ce moment-là. Sa porte s'ouvre au démarrage du service, une fois pour toutes, bien avant qu'une session existe, et rien ne la lui décrivait ensuite. Elle prenait donc le profil nominal, quelle que soit la session, et le changement de débit en cours de route ne l'atteignait pas davantage.

**Ce qui est fait.** La forme du flux devient vivante. Le contrôleur ne garde plus un profil figé à la construction de la connexion : il lit, à chaque calcul de fenêtre, ce que la porte est en train de servir. La porte l'apprend de deux endroits, et ce sont les deux seuls qui le savent : le premier mot d'une session, la question des ports, qui porte désormais le débit et la cadence demandés ; et la demande de changement de débit en cours de session, qui existait déjà pour le moteur et prévient maintenant le tunnel avant lui. Quand plus aucune session n'est ouverte, la porte revient à ce avec quoi elle a été construite, pour qu'une session n'hérite jamais de la précédente. La branche de relais de l'ordinateur regardé partage la même forme vivante : c'est elle qui porte la vidéo quand la route est relayée, et elle était fausse pour la même raison.

**La file d'envoi, elle, ne peut pas suivre.** Le transport la fixe à la création de la connexion et ne la rouvre jamais. Elle est donc taillée sur le flux le plus rapide que le produit propose, lu là où la liste des débits est écrite, et sur rien d'autre : trop courte, elle coupe chaque image clé et l'image ne s'établit jamais ; trop longue, elle coûte un mégaoctet et un peu de retard après un arrêt que la session n'aurait pas passé du tout. Le relais du serveur est taillé de la même façon, pour la même raison en plus forte : il porte ce qu'on lui donne et n'a aucune session à lui.

**Ce que ça ne prétend pas être.** Ce n'est pas l'explication du silence d'une seconde qui revient sur l'une des deux machines. C'est ce qui fait que le tunnel tient ce silence-là au lieu de céder au premier hoquet de plus de cent vingt-cinq millisecondes, comme il le faisait depuis le début du côté regardé.

## Décisions ouvertes (défauts proposés, à confirmer avant le jalon concerné)

- O1 (avant M5). Concurrence de sessions : défaut = 1 spectateur entrant actif avec reprise possible (takeover), plusieurs sessions sortantes autorisées.
- ~~O2 (avant M5). Modèle de confiance.~~ Clos le 2026-09-02 par D122 : appareils du même compte sans approbation, partages explicites par machine, approbation à la session plus tard ; activation de l'hôte réservée aux administrateurs et TOTP obligatoire à la bêta, inchangés.
- ~~O3 (avant M6). Politique du relais hébergé.~~ Close le 2026-09-03 par le relais lui-même : il vit dans le binaire du serveur (D120), s'auto-héberge, se débraye par `[relay] enabled`, et porte ses quotas par session. Un serveur officiel, s'il vient, sera une instance de ce même binaire avec des quotas par compte, et son hébergement le seul coût d'infrastructure du projet.
- O4 (avant M4). Posture de crédit : défaut = moteurs invisibles dans l'expérience, crédités clairement dans « À propos » et la documentation.
- ~~O5 (avant M2). Choix final iroh contre quinn.~~ Clos le 2026-08-07 par D13.
