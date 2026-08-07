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
- D4. Transport : QUIC ; pari principal iroh, plan B quinn + relais maison, derrière un trait d'abstraction ; contrôleur de congestion média sur mesure OBLIGATOIRE (critère GO/NO-GO du banc M2) ; décision finale gravée dans ce registre à l'issue de M2.
- D5. Sunshine intégré en processus enfant piloté par config/CLI/REST, objectif zéro patch ; Moonlight en processus enfant piloté par CLI + état portable, six micro-patchs maximum ; règle absolue : aucune fonctionnalité ZyrDesk dans les moteurs.
- D6. Dépôt : monorepo + deux forks légers en submodules, pile de commits rebasée sur tags épinglés, miroirs de patchs exportés, suite contrat moteur, répétition mensuelle de mise à niveau. Version plancher Sunshine : v2026.516.143833 (sécurité).
- D7. Interface : Tauri v2 (web + cœur Rust) ; la vidéo ne traverse jamais la WebView (fenêtre native du lecteur). Choix retenu par défaut après examen de Slint, Flutter, Qt, egui/iced (détails dans TECH-CHOICES.md) ; réversible tant que M4 n'est pas engagé.
- D8. Licences : application et forks en GPLv3 ; broker et relais en AGPLv3. Retenu par défaut ; alternatives documentées (tout GPLv3, serveur Apache) si Victor préfère.
- D9. Écran virtuel sans coût : pilote tiers open source déjà signé (Virtual-Display-Driver, MIT) en installation optionnelle consentie, testé dès M1 ; repli = fonction désactivée (écran branché requis ; adaptateur du commerce facultatif à la charge de l'utilisateur final qui veut un PC sans écran). Aucun certificat ni compte payant, conformément à C8.
- D10. Périmètre v1 : clavier, souris, audio, 1080p60/1440p60, direct + relais + reprise, accès non supervisé, presse-papiers texte, mode LAN sans compte, un spectateur actif. Hors v1 : manettes, transfert de fichiers, coupure audio côté hôte, HDR, 120 FPS, partage entre comptes, Wake-on-LAN.
- D11. Pas de bitrate adaptatif en v1 (limite du protocole) : sonde de débit pré-session + préréglages + bascule de qualité rapide + plafond en relais ; assumé et documenté.
- D12. Apollo (fork Sunshine avec écran virtuel intégré) rejeté comme base : divergence et retard sur upstream incompatibles avec C3.

## Décisions ouvertes (défauts proposés, à confirmer avant le jalon concerné)

- O1 (avant M5). Concurrence de sessions : défaut = 1 spectateur entrant actif avec reprise possible (takeover), plusieurs sessions sortantes autorisées.
- O2 (avant M5). Modèle de confiance : défaut = connexion automatique entre appareils du même compte + approbation au premier appairage de chaque paire + activation de l'hôte réservée aux administrateurs + TOTP obligatoire à la bêta.
- O3 (avant M6). Politique du relais hébergé : défaut = auto-hébergement documenté dès le premier jour ; service officiel avec quotas par compte. Le coût d'hébergement du broker/relais officiel (un petit serveur) est le seul coût d'infrastructure du projet.
- O4 (avant M4). Posture de crédit : défaut = moteurs invisibles dans l'expérience, crédités clairement dans « À propos » et la documentation.
- O5 (avant M2). Choix final iroh contre quinn : tranché par les mesures du banc M2 (mémo à consigner ici).
