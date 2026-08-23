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

**Décision.** Pendant qu'une session tient le clavier, ZyrDesk intercepte Alt+Tab, Alt+Échap et Ctrl+Échap avant que Windows n'agisse dessus, et les porte telles quelles à la fenêtre de l'image. Partout ailleurs, sur toutes les autres touches, et pour les frappes que ZyrDesk envoie lui-même, rien n'est touché. L'option du moteur client qui faisait ça, demandée par [D28](#d28-alt-tab-et-la-touche-windows-agissent-sur-lordinateur-distant-2026-08-23-pendant-m4), est retirée : l'intention de D28 tient, le moyen change.

**Ce qui l'a rendu nécessaire.** Deux choses dites par Victor, à deux tours d'écart. D'abord « Non ça a rien changé » : le moteur ne peut pas se servir de son option ici. Il décide qu'il tient le clavier en comparant sa propre fenêtre à celle que le système appelle celle du premier plan ; la sienne est portée dans la nôtre pendant toute la session, donc c'est une fenêtre fille, et une fenêtre fille n'est jamais celle-là ([D31](#d31-le-clavier-de-la-session-se-rend-par-le-focus-et-jamais-par-le-premier-plan-2026-08-23-pendant-m4)). Au premier message de focus qui lui parvient, quelques secondes après le début, il conclut qu'il a perdu le clavier et relâche ces touches pour le reste de la session.

Ensuite : « je perdais mes raccourcis clavier de zyrdesk comme par exemple alt + & pour switcher plein ecran/fenetré ». C'est la seconde moitié, et elle condamne l'option pour de bon. La façon dont le moteur reprend ces touches est de se mettre devant chaque frappe de tout l'ordinateur et d'avaler **Alt et Ctrl en entier**, avant que quiconque les voie. Tous les raccourcis du produit sont des combinaisons Alt : tant que le moteur tenait ces touches, aucun ne fonctionnait, et on ne pouvait plus sortir du plein écran au clavier.

**Pourquoi ce n'est pas une fonctionnalité mise dans un moteur.** Elle n'y est pas mise : rien n'est ajouté au moteur, et rien de nouveau ne lui est demandé. Ce qu'il reçoit est une frappe ordinaire à sa propre fenêtre, qu'il transmet comme il transmet tout le reste. La fenêtre que le système appelle celle du premier plan est la nôtre, donc le programme qui peut reprendre ces touches est le nôtre, et il se contente de les passer.

**Ce qui garde ça sûr.** Trois conditions, toutes les trois exigées, et lues à chaque touche : une session est à l'écran, le premier plan appartient à cette session, et l'image est bien la fenêtre à qui le clavier va. Le menu du bouton flottant ouvert, la session terminée ou le programme fermé, la réponse est non et Windows fait ce qu'il a toujours fait. Tab et Échap seules ne sont jamais reprises : ce sont des touches ordinaires, et seule la compagnie qu'elles gardent en fait des touches du système. Une touche reprise à l'appui l'est aussi au relâchement, quoi qu'il arrive entre les deux, sans quoi une session qui s'arrête au mauvais moment laisserait Windows voir un relâchement qu'il n'a jamais vu s'enfoncer.

**Ce que ça coûte.** Chaque touche de cet ordinateur passe par un test de ZyrDesk tant qu'une session dure. Le test est court par construction, et rien ne s'écrit dans le journal depuis là : le journal est un verrou et un fichier vidé sur le disque, sur la route que chaque frappe de chaque programme emprunte, et un système qui trouve cette route lente décroche le tout sans prévenir. Ce qui est repris est compté dans deux nombres, et la surveillance de session l'écrit une seconde plus tard.

**Ce qui va avec : plus aucune touche coincée.** Si une modificatrice est enfoncée et que le premier plan part ailleurs avant qu'elle ne remonte, l'ordinateur distant ne voit jamais le relâchement et croit la touche tenue pour toujours ; tout ce qui est tapé ensuite y arrive en Alt et une lettre, ce qui ne fait rien et ressemble trait pour trait à un clavier mort. Dit par Victor : « j'ai même carrément perdu le clavier dans la session », puis, un tour plus tard, « ce bug tu me le ramène toujours ». ZyrDesk relâche donc là-bas, à chaque tour de la surveillance de session, toutes les modificatrices qu'aucun doigt ne tient, lues sur le clavier physique et nulle part ailleurs.

**Sans condition, et c'est là que le premier essai a échoué.** Ce relâchement n'était demandé que lorsque le clavier revenait à l'image après en être parti, et il n'a jamais eu lieu une seule fois : ce qui abandonne une modificatrice, c'est le premier plan qui s'en va, et le clavier n'est pas obligé de le suivre. ZyrDesk gardait donc le clavier et répondait « oui, toujours là » pendant qu'Alt restait coincée au loin. Rien n'est envoyé pour une touche qu'un doigt tient réellement, donc demander à chaque tour ne coûte rien, et une seule modificatrice touchée depuis le dernier passage suffit à déclencher le suivant.

**Et l'état d'Alt et Ctrl est compté sur le flux lui-même**, plus demandé au système au moment où une touche arrive. Cette question-là, posée de l'intérieur du traitement d'une autre touche par le système, sur une touche qu'il n'a pas fini de traiter, n'est pas une chose sur laquelle asseoir une fonctionnalité : une Alt+Tab sur quatre était lue comme un Tab tout seul et laissée passer, et Windows changeait de fenêtre sur cet ordinateur-ci. Le flux est seul juge de ce que le flux transporte. Il est amorcé au démarrage de la session par une lecture du clavier physique, pour le cas d'une session ouverte un doigt déjà sur Alt.

**Ce qui reste ouvert : la touche Windows.** Elle n'est pas reprise, et c'est la seule que ce chemin ne peut pas servir. Le moteur refuse de la transmettre à l'ordinateur distant tant que sa propre capture des touches du système ne tourne pas, ce qui dans ce produit n'arrive jamais. La reprendre ici n'ouvrirait donc de menu nulle part, ni là-bas ni ici, ce qui est pire que de la laisser tranquille : laissée tranquille, elle fait ce qu'elle a toujours fait sur cet ordinateur. Ctrl+Maj+Échap, le gestionnaire des tâches, part au loin comme les autres ; si ça gêne à l'usage, c'est une ligne à retirer de la liste.

## Décisions ouvertes (défauts proposés, à confirmer avant le jalon concerné)

- O1 (avant M5). Concurrence de sessions : défaut = 1 spectateur entrant actif avec reprise possible (takeover), plusieurs sessions sortantes autorisées.
- O2 (avant M5). Modèle de confiance : défaut = connexion automatique entre appareils du même compte + approbation au premier appairage de chaque paire + activation de l'hôte réservée aux administrateurs + TOTP obligatoire à la bêta.
- O3 (avant M6). Politique du relais hébergé : défaut = auto-hébergement documenté dès le premier jour ; service officiel avec quotas par compte. Le coût d'hébergement du broker/relais officiel (un petit serveur) est le seul coût d'infrastructure du projet.
- O4 (avant M4). Posture de crédit : défaut = moteurs invisibles dans l'expérience, crédités clairement dans « À propos » et la documentation.
- ~~O5 (avant M2). Choix final iroh contre quinn.~~ Clos le 2026-08-07 par D13.
