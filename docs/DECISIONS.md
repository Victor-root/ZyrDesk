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
- D7. Interface : Tauri v2 (web + cœur Rust) ; la vidéo ne traverse jamais la WebView (fenêtre native du lecteur). Choix retenu par défaut après examen de Slint, Flutter, Qt, egui/iced (détails dans TECH-CHOICES.md) ; réversible tant que M4 n'est pas engagé.
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

**La demande, et la comparaison qui la fonde.** « Quand je quitte la session, l'écran de l'hôte doit se remettre nickel comme avant d'en prendre le contrôle. Parsec y arrive. » Il y arrive parce qu'il ne touche jamais à l'écran de l'ordinateur qu'il montre : il le filme tel quel et met l'image à l'échelle de son côté. Rien à remettre, donc rien qui puisse rater.

**Ce que faisait ZyrDesk, et pourquoi ça finit mal.** Sur une machine sans écran virtuel, nous demandions au moteur hôte de mettre l'écran physique à la taille de la session, et de le remettre après. Remettre est une chose qui peut rater, et elle rate précisément quand quelque chose d'autre a bougé les écrans entre temps : un deuxième bureau à distance, un moniteur qui se réveille, un câble. Ce qui suit est pire que ce qu'on évitait. Le code du moteur, lu ligne à ligne : il échoue à revenir à ce qu'il avait trouvé, **il rallume alors tous les écrans qu'il voit**, ce qui est en soi un changement d'écrans, ce qui est exactement la condition qui le fait réessayer. La boucle s'entretient toute seule et ne s'arrête jamais. Un relevé l'a montrée sur vingt secondes, entre la fin d'une session et l'arrêt du service, la personne entendant sa tour cliquer à travers ses moniteurs.

**Corrigé en ne touchant plus rien.** Une seule question décide, et ce n'est pas celle de la session : cet ordinateur a-t-il un écran à lui à donner ? Un écran que ZyrDesk a fait pousser existe pour prendre la forme qu'une session demande, et les vrais sont éteints le temps de la session puis rendus. Un ordinateur qui n'en a pas n'a que des écrans réels, et un écran réel appartient à qui est assis devant. Sur celui-là, `dd_configuration_option = disabled` et rien d'autre : pas « touché avec précaution », pas « touché puis remis », **pas touché**. Un écran auquel on n'a jamais touché revient en n'étant jamais parti.

**Ce que ça coûte, et ce qui reste à faire.** L'image arrive dans la forme de l'ordinateur d'en face et non dans la nôtre, donc une session qui demande une autre forme reçoit des bandes noires gravées à la source. Le remède n'est pas de déplacer les meubles d'en face : c'est de demander l'image dans **sa** forme à lui. Le moteur hôte ne publie sa résolution nulle part, donc elle doit voyager par le canal entre les deux services, et le lecteur de zyr-screen sait déjà la lire dans la liste d'écrans du moteur. En attendant, choisir une taille de la bonne forme dans le menu de la session suffit à supprimer les bandes.

## Décisions ouvertes (défauts proposés, à confirmer avant le jalon concerné)

- O1 (avant M5). Concurrence de sessions : défaut = 1 spectateur entrant actif avec reprise possible (takeover), plusieurs sessions sortantes autorisées.
- O2 (avant M5). Modèle de confiance : défaut = connexion automatique entre appareils du même compte + approbation au premier appairage de chaque paire + activation de l'hôte réservée aux administrateurs + TOTP obligatoire à la bêta.
- O3 (avant M6). Politique du relais hébergé : défaut = auto-hébergement documenté dès le premier jour ; service officiel avec quotas par compte. Le coût d'hébergement du broker/relais officiel (un petit serveur) est le seul coût d'infrastructure du projet.
- O4 (avant M4). Posture de crédit : défaut = moteurs invisibles dans l'expérience, crédités clairement dans « À propos » et la documentation.
- ~~O5 (avant M2). Choix final iroh contre quinn.~~ Clos le 2026-08-07 par D13.
