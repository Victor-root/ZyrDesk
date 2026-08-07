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
- D5. Sunshine intégré en processus enfant piloté par config/CLI/REST, objectif zéro patch ; Moonlight en processus enfant piloté par CLI + état portable, six micro-patchs maximum ; règle absolue : aucune fonctionnalité ZyrDesk dans les moteurs.
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

## Décisions ouvertes (défauts proposés, à confirmer avant le jalon concerné)

- O1 (avant M5). Concurrence de sessions : défaut = 1 spectateur entrant actif avec reprise possible (takeover), plusieurs sessions sortantes autorisées.
- O2 (avant M5). Modèle de confiance : défaut = connexion automatique entre appareils du même compte + approbation au premier appairage de chaque paire + activation de l'hôte réservée aux administrateurs + TOTP obligatoire à la bêta.
- O3 (avant M6). Politique du relais hébergé : défaut = auto-hébergement documenté dès le premier jour ; service officiel avec quotas par compte. Le coût d'hébergement du broker/relais officiel (un petit serveur) est le seul coût d'infrastructure du projet.
- O4 (avant M4). Posture de crédit : défaut = moteurs invisibles dans l'expérience, crédités clairement dans « À propos » et la documentation.
- ~~O5 (avant M2). Choix final iroh contre quinn.~~ Clos le 2026-08-07 par D13.
