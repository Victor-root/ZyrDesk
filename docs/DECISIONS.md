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

**L'appairage se refait tout seul quand l'autre l'a oublié.** Ce que le client retient d'un appairage n'est qu'une note qu'il s'est écrite à lui-même, et l'hôte est le seul à décider : réinstallé, remis à zéro ou simplement vidé de son état, il ne reconnaît plus personne. Le moteur client repart alors en moins d'une seconde, dans un journal que personne ne lit, et la fenêtre n'a qu'une session terminée à montrer. La session est donc surveillée le temps qu'elle prenne : un moteur qui abandonne aussitôt déclenche une nouvelle présentation, et la session s'ouvre. Cette surveillance ne coûte rien à la première session, où les présentations viennent d'être faites, et se confond avec le démarrage du moteur pour les suivantes.

## D18. Le moteur hôte n'est plus une condition d'existence du service (2026-08-18, pendant M4)

**Décision.** Le service tourne, ouvre les voies sortantes, s'annonce sur le réseau et répond à l'interface, que le moteur hôte soit là ou non. Un moteur absent ou qui ne tient pas rend cet ordinateur injoignable, et rien de plus. Ce qui empêche d'être joignable voyage jusqu'à la fenêtre, qui le dit en clair.

**Ce que ça corrige.** Le service s'arrêtait net quand le moteur hôte manquait. Un ordinateur qui ne sert que de client perdait donc le tunnel, la découverte du réseau et son interface, pour une moitié du produit dont il n'a pas l'usage. Et la fenêtre n'ayant que « joignable ou non » à afficher, un moteur absent s'y lisait « démarrage en cours » indéfiniment.

**Ce que ça coûte.** Un service qui ne s'arrête plus de lui-même. Il se relit toutes les cinq secondes plutôt que d'insister, et le retour en arrière après un moteur qui ne tient pas est l'interrupteur d'accès distant, coupé puis rallumé.

## Décisions ouvertes (défauts proposés, à confirmer avant le jalon concerné)

- O1 (avant M5). Concurrence de sessions : défaut = 1 spectateur entrant actif avec reprise possible (takeover), plusieurs sessions sortantes autorisées.
- O2 (avant M5). Modèle de confiance : défaut = connexion automatique entre appareils du même compte + approbation au premier appairage de chaque paire + activation de l'hôte réservée aux administrateurs + TOTP obligatoire à la bêta.
- O3 (avant M6). Politique du relais hébergé : défaut = auto-hébergement documenté dès le premier jour ; service officiel avec quotas par compte. Le coût d'hébergement du broker/relais officiel (un petit serveur) est le seul coût d'infrastructure du projet.
- O4 (avant M4). Posture de crédit : défaut = moteurs invisibles dans l'expérience, crédités clairement dans « À propos » et la documentation.
- ~~O5 (avant M2). Choix final iroh contre quinn.~~ Clos le 2026-08-07 par D13.
