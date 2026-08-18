# Sécurité

Principe directeur : le serveur met en relation, il ne peut pas espionner. Les clés de session ne quittent jamais les appareils. Un produit d'accès non supervisé donne le contrôle total du PC : le modèle de confiance est traité comme une fonctionnalité de premier rang.

## 1. Identités

- Compte utilisateur : e-mail + mot de passe (haché Argon2id côté broker), jetons d'accès courts + jeton de rafraîchissement. Double authentification TOTP disponible dès que les comptes existent, OBLIGATOIRE avant toute bêta publique (compromission du compte = contrôle des PC).
- Identité d'appareil : à l'installation, chaque appareil génère une paire de clés. La clé privée ne quitte JAMAIS l'appareil. La clé publique est enregistrée auprès du broker à l'enrôlement (appareil rattaché au compte) et sert d'identité réseau.

  État au jalon M2 : l'identité est un certificat auto-signé, produit à la première demande et conservé dans `data/identity`. Ce qui compte n'est pas le certificat mais son empreinte : chaque extrémité connaît d'avance celle du pair et refuse tout autre certificat, dans les deux sens. Aucune autorité de certification n'entre en jeu, et le nom porté par le certificat n'est jamais vérifié : il n'y a pas de nom de domaine à valider, seulement deux ordinateurs qui doivent se reconnaître. L'empreinte ne change plus une fois créée, sans quoi tous les appairages seraient rompus ; une identité dont un fichier manque est refusée plutôt que refaite en silence.

  Limite assumée à ce stade : la clé privée est écrite en clair sous le dossier du projet, sur une machine que son propriétaire administre. Le service du jalon M3 la mettra sous la protection du système, hors de portée des autres comptes locaux (voir §4).

  D'où vient l'empreinte attendue : sur un réseau local, de l'annonce mDNS que chaque service émet (voir §1.1). Le ticket de session la fournira une fois le broker en place, ce qui ne change rien au mécanisme de vérification. Elle reste recopiable à la main quand l'annonce ne passe pas, l'interface l'affichant sur chaque machine.

  Côté hôte, depuis le jalon M3, les empreintes admises sont une liste et non une seule : un ordinateur en sert plusieurs au fil du temps. Elle vit dans `data/authorized-devices.conf`, se gère par `zyr-cli host authorize`, `revoke` et `devices`, et le service la relit toutes les cinq secondes. Autoriser une machine de plus ne coupe donc pas la session en cours, et une liste devenue illisible ne révoque personne : elle est signalée dans le journal, l'ensemble précédent restant en vigueur. Une empreinte mal recopiée est refusée avec son numéro de ligne plutôt qu'ignorée en silence, faute de quoi une autorisation absente passerait longtemps pour une panne réseau. Ce fichier ne contient aucun secret : une empreinte est publique et n'ouvre rien à elle seule.

### 1.1 Confiance au réseau local (jalon M4)

Le service admet, en plus de cette liste, les ZyrDesk qui s'annoncent sur le réseau local. Un interrupteur des réglages le décide, activé par défaut, et l'état s'affiche dans le journal comme sur l'écran d'accueil.

Ce que cela suppose : que le réseau local soit celui de son propriétaire. C'est la même hypothèse que celle de la découverte mDNS elle-même, et que celle de l'imprimante ou du partage de fichiers de la même machine. Ce que cela n'ouvre pas : rien qui vienne d'ailleurs que du réseau local. Seule une machine capable de parler sur ce réseau peut s'y annoncer, et une annonce ne donne accès qu'à ce que l'accès distant laisse passer par ailleurs.

Ce que cela remplace : l'obligation de recopier une empreinte de soixante-quatre caractères d'un ordinateur à l'autre avant la première session. Cette étape n'apportait aucune garantie que le réseau local ne donnait pas déjà, et elle coûtait à chaque installation.

Ce que cela ne couvrira pas : les sessions passant par Internet. Le jour où le broker existe (jalon M5), les appareils d'un compte se reconnaissent par leur enregistrement auprès de lui, et cette confiance-là cesse de s'appliquer au-delà du réseau local. L'interrupteur permet, dès aujourd'hui, de s'en passer sur un réseau dont on ne répond pas : la liste écrite reprend alors seule la main.
- Identités internes moteurs : le certificat client RSA de Moonlight et le certificat de Sunshine existent toujours (protocole d'appairage officiel conservé), mais ils vivent en loopback derrière le tunnel et sont gérés automatiquement ; ils constituent une couche interne supplémentaire, pas la frontière de sécurité principale.

## 2. Tickets de session

1. Le client demande au broker une session vers un appareil du même compte.
2. Le broker vérifie compte, appareil non révoqué, politique (voir §6), puis émet aux DEUX extrémités un ticket signé de courte durée (60 s, nonce unique, identités des deux appareils, indices de chemin réseau).
3. Les deux services établissent la connexion QUIC avec certificats auto-signés liés à leur clé Ed25519 ; chacun vérifie que la clé du pair correspond EXACTEMENT à celle annoncée dans le ticket (épinglage mutuel).
4. Anti-rejeu : nonce à usage unique + expiration courte + tolérance d'horloge de ±5 minutes + protections de la poignée de main QUIC.

Résultat : le chiffrement (TLS 1.3 de QUIC) est négocié directement entre les deux appareils. Le broker sait QUI parle à QUI et QUAND (métadonnées de mise en relation), jamais le contenu. Le relais transporte des paquets qu'il ne peut pas déchiffrer.

Asymétrie du protocole, à ne pas confondre avec une faille : le client présente son certificat en dernier, et l'hôte ne le juge qu'ensuite. Un appareil refusé voit donc sa connexion réussir avant d'être rompue aussitôt. Rien n'y circule, mais l'interface ne doit jamais annoncer une session établie avant le premier échange réussi. Un test le vérifie dans les deux sens : l'hôte refuse l'inconnu, et l'inconnu perd sa connexion. En pratique, ce premier échange est la salutation du canal ZyrDesk, que le client attend avant de lancer quoi que ce soit : un ordinateur non autorisé s'en va avec un message qui le dit, et non sur un délai d'attente inexpliqué.

## 3. Chiffrement des flux

- Sur le réseau : tout passe dans le tunnel QUIC (TLS 1.3, AEAD par paquet). Vidéo, audio, entrées, presse-papiers, appairage : une seule enveloppe chiffrée de bout en bout.
- À l'intérieur des machines : les moteurs parlent en loopback. Le chiffrement interne GameStream est désactivé (mode 0) pour éviter un double chiffrement inutile : le trafic en clair n'existe que sur 127.0.0.1, jamais sur un fil. Un mode « paranoïaque » (chiffrement interne en mode obligatoire, en plus du tunnel) reste disponible dans les réglages avancés.
- Le code d'appairage initial des moteurs transite par le canal ZyrDesk DU TUNNEL authentifié : le broker ne le voit jamais, et personne ne le lit non plus. Depuis le jalon M4, il est tiré au sort par le client, envoyé dans le tunnel, et remis par le service hôte à son moteur ; il n'est plus jamais affiché ni saisi. Le tunnel ayant reconnu les deux ordinateurs à leur empreinte avant qu'un seul octet ne passe, ce code ne prouve rien que le tunnel n'ait déjà prouvé : il n'est là que parce que les moteurs le réclament.

## 4. Stockage des secrets sous Windows

| Secret | Où | Protection |
|---|---|---|
| Clé privée d'appareil | Profil du service | DPAPI dans le profil SYSTEM (PAS DPAPI « machine », que tout utilisateur local peut déchiffrer) + ACL SYSTEM et Administrateurs. Jalon M3 ; jusque-là en clair dans `data\identity` |
| Identifiants de l'interface web Sunshine | Profil du service | Aléatoires 32 octets, régénérés à chaque démarrage du service, DPAPI profil SYSTEM |
| Jetons de compte (interface) | Session utilisateur | Gestionnaire d'identifiants Windows de l'utilisateur |
| État Moonlight (certificats d'appairage internes) | dossier de données, `devices\<id>` | ACL restreintes une fois le service en place |

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
