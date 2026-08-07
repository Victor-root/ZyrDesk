# Sécurité

Principe directeur : le serveur met en relation, il ne peut pas espionner. Les clés de session ne quittent jamais les appareils. Un produit d'accès non supervisé donne le contrôle total du PC : le modèle de confiance est traité comme une fonctionnalité de premier rang.

## 1. Identités

- Compte utilisateur : e-mail + mot de passe (haché Argon2id côté broker), jetons d'accès courts + jeton de rafraîchissement. Double authentification TOTP disponible dès que les comptes existent, OBLIGATOIRE avant toute bêta publique (compromission du compte = contrôle des PC).
- Identité d'appareil : à l'installation, chaque appareil génère une paire de clés Ed25519. La clé privée ne quitte JAMAIS l'appareil. La clé publique est enregistrée auprès du broker à l'enrôlement (appareil rattaché au compte) et sert d'identité réseau.
- Identités internes moteurs : le certificat client RSA de Moonlight et le certificat de Sunshine existent toujours (protocole d'appairage officiel conservé), mais ils vivent en loopback derrière le tunnel et sont gérés automatiquement ; ils constituent une couche interne supplémentaire, pas la frontière de sécurité principale.

## 2. Tickets de session

1. Le client demande au broker une session vers un appareil du même compte.
2. Le broker vérifie compte, appareil non révoqué, politique (voir §6), puis émet aux DEUX extrémités un ticket signé de courte durée (60 s, nonce unique, identités des deux appareils, indices de chemin réseau).
3. Les deux services établissent la connexion QUIC avec certificats auto-signés liés à leur clé Ed25519 ; chacun vérifie que la clé du pair correspond EXACTEMENT à celle annoncée dans le ticket (épinglage mutuel).
4. Anti-rejeu : nonce à usage unique + expiration courte + tolérance d'horloge de ±5 minutes + protections de la poignée de main QUIC.

Résultat : le chiffrement (TLS 1.3 de QUIC) est négocié directement entre les deux appareils. Le broker sait QUI parle à QUI et QUAND (métadonnées de mise en relation), jamais le contenu. Le relais transporte des paquets qu'il ne peut pas déchiffrer.

## 3. Chiffrement des flux

- Sur le réseau : tout passe dans le tunnel QUIC (TLS 1.3, AEAD par paquet). Vidéo, audio, entrées, presse-papiers, appairage : une seule enveloppe chiffrée de bout en bout.
- À l'intérieur des machines : les moteurs parlent en loopback. Le chiffrement interne GameStream est désactivé (mode 0) pour éviter un double chiffrement inutile : le trafic en clair n'existe que sur 127.0.0.1, jamais sur un fil. Un mode « paranoïaque » (chiffrement interne en mode obligatoire, en plus du tunnel) reste disponible dans les réglages avancés.
- Le PIN d'appairage initial transite par le canal de contrôle DU TUNNEL authentifié : le broker ne le voit jamais.

## 4. Stockage des secrets sous Windows

| Secret | Où | Protection |
|---|---|---|
| Clé privée d'appareil | Profil du service | DPAPI dans le profil SYSTEM (PAS DPAPI « machine », que tout utilisateur local peut déchiffrer) + ACL SYSTEM et Administrateurs |
| Identifiants de l'interface web Sunshine | Profil du service | Aléatoires 32 octets, régénérés à chaque démarrage du service, DPAPI profil SYSTEM |
| Jetons de compte (interface) | Session utilisateur | Gestionnaire d'identifiants Windows de l'utilisateur |
| État Moonlight (certificats d'appairage internes) | `%ProgramData%\ZyrDesk\devices\<id>` | ACL restreintes |

## 5. Surface locale

- Named pipe `\\.\pipe\zyrdesk` : chaque message est autorisé selon le SID Windows de l'appelant. Activer/désactiver l'hôte et modifier sa configuration : administrateurs uniquement. Consulter l'état et lancer des sessions sortantes : utilisateur standard. Clients distants du pipe refusés.
- Interface web Sunshine : impossible à désactiver (elle porte l'API d'appairage), donc verrouillée : accès loopback uniquement + identifiants aléatoires inconnus de l'utilisateur. Elle n'est jamais exposée ni mentionnée.
- Les moteurs écoutent uniquement sur loopback : aucune surface réseau externe en dehors du port UDP unique du tunnel.

## 6. Politiques de confiance (défauts proposés, ajustables)

- Appareils d'un même compte : connexion automatique autorisée, avec approbation explicite au tout premier appairage de chaque paire d'appareils.
- Session entrante : indicateur visible côté hôte (icône + notification de début de session). Un seul spectateur actif ; une nouvelle connexion propose la reprise (takeover) ou est refusée, selon le réglage.
- Révocation : depuis n'importe quel appareil connecté au compte, révoquer un appareil perdu ou volé ; le broker pousse la révocation (les services la reçoivent immédiatement) et la vérifie à chaque émission de ticket. Les tickets étant courts, la fenêtre d'exposition après révocation est de l'ordre de la minute.
- Partage entre comptes (inviter un ami) : hors périmètre v1 ; le modèle de tickets l'anticipe (autorisations par appareil et durée limitée).

## 7. Modèle de menace (résumé)

| Acteur | Capacités | Défense |
|---|---|---|
| Écoute réseau (FAI, Wi-Fi public) | Voit des paquets | QUIC chiffré de bout en bout, rien d'exploitable |
| Relais compromis | Voit les paquets qu'il relaie, peut les jeter | Ne peut ni déchiffrer ni s'insérer (épinglage mutuel des clés) ; au pire, déni de service = bascule de chemin |
| Broker compromis | Métadonnées, comptes ; peut mentir sur les correspondances | Ne peut PAS s'insérer dans une session (il ne connaît aucune clé privée) ; il pourrait refuser le service ou mettre en relation avec un appareil du même compte uniquement (les clés des pairs sont vérifiées de part et d'autre contre le ticket ET la liste d'appareils signée connue localement) |
| Voleur du PC client | Accès aux secrets locaux | DPAPI + session Windows ; révocation immédiate depuis un autre appareil ; TOTP protège le compte |
| Utilisateur local non privilégié sur l'hôte | Accès au pipe, tentative sur l'interface web du moteur | Actions sensibles réservées aux administrateurs (SID) ; interface web verrouillée par identifiants aléatoires |
| Rejeu d'un ticket intercepté | Réutilisation | Nonce unique, expiration 60 s, liaison aux clés des deux appareils |

## 8. Hygiène de base

- Version plancher de Sunshine : v2026.516.143833 (corrige un contournement critique de la validation des certificats clients, CVSS 9.8). Aucune version antérieure, jamais.
- Dépendances épinglées, mise à jour régulière outillée (la répétition mensuelle de mise à niveau couvre aussi les correctifs de sécurité moteurs).
- Journaux sans secrets (les bundles de diagnostic sont expurgés : pas de jetons, pas de clés, pas d'adresses complètes si non nécessaires).
- Passe de sécurité dédiée avant bêta (jalon M10) : ACL du pipe, permissions des fichiers, quotas relais, revue des surfaces.
