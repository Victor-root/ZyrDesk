# Sécurité

Principe directeur : le serveur met en relation, il ne peut pas espionner. Les clés de session ne quittent jamais les appareils. Un produit d'accès non supervisé donne le contrôle total du PC : le modèle de confiance est traité comme une fonctionnalité de premier rang.

## 1. Identités

- Compte utilisateur : nom d'utilisateur + mot de passe (haché Argon2id côté broker, paramètres OWASP), e-mail facultatif, jeton de compte d'une heure pour les gestes du compte, jeton d'appareil de 90 jours renouvelé sans geste et révoqué avec l'appareil. Double authentification TOTP hors MVP, OBLIGATOIRE avant toute bêta publique (compromission du compte = contrôle des PC). Détails : [SERVER.md](SERVER.md) §3.
- Identité d'appareil : à l'installation, chaque appareil génère une paire de clés. La clé privée ne quitte JAMAIS l'appareil. La clé publique est enregistrée auprès du broker à l'enrôlement (appareil rattaché au compte) et sert d'identité réseau.

  État au jalon M2 : l'identité est un certificat auto-signé, produit à la première demande et conservé dans `data/identity`. Sa clé est ECDSA P-256, celle que produit `rcgen` par défaut, et non Ed25519 comme ce document l'annonçait ; Ed25519 est la courbe de la clé de signature du serveur. Ce qui compte n'est pas le certificat mais son empreinte : chaque extrémité connaît d'avance celle du pair et refuse tout autre certificat, dans les deux sens. Aucune autorité de certification n'entre en jeu, et le nom porté par le certificat n'est jamais vérifié : il n'y a pas de nom de domaine à valider, seulement deux ordinateurs qui doivent se reconnaître. L'empreinte ne change plus une fois créée, sans quoi chaque ordinateur qui l'a admise la refuserait ; une identité dont un fichier manque est refusée plutôt que refaite en silence.

  Limite assumée à ce stade : la clé privée est écrite en clair sous le dossier du projet, sur une machine que son propriétaire administre. La protection du système, hors de portée des autres comptes locaux, reste à poser (voir §4).

  D'où vient l'empreinte attendue : sur un réseau local, de l'annonce mDNS que chaque service émet (voir §1.1). Le ticket de session la fournit quand le broker met les deux appareils en relation, ce qui ne change rien au mécanisme de vérification. Elle reste recopiable à la main quand l'annonce ne passe pas, l'interface l'affichant sur chaque machine.

  Côté hôte, depuis le jalon M3, les empreintes admises sont une liste et non une seule : un ordinateur en sert plusieurs au fil du temps. Elle vit dans `data/authorized-devices.conf`, s'écrit depuis la fenêtre par « Ajouter un ordinateur », et le service la relit toutes les cinq secondes. Autoriser une machine de plus ne coupe donc pas la session en cours, et une liste devenue illisible ne révoque personne : elle est signalée dans le journal, l'ensemble précédent restant en vigueur. Une empreinte mal recopiée est refusée avec son numéro de ligne plutôt qu'ignorée en silence, faute de quoi une autorisation absente passerait longtemps pour une panne réseau. Ce fichier ne contient aucun secret : une empreinte est publique et n'ouvre rien à elle seule.

  Un second fichier, `data/known-computers.conf`, garde l'adresse et le nom des ordinateurs saisis à la main pour qu'ils restent sur l'accueil. Il est tenu à part exprès : il ne décide de rien, personne n'entre parce qu'il y figure, et une ligne qu'il ne comprend pas est ignorée là où la liste des appareils admis, elle, refuse tout le fichier. Oublier un ordinateur depuis la fenêtre le retire des deux.

### 1.1 Confiance au réseau local (jalon M4)

Le service admet, en plus de cette liste, les ZyrDesk qui s'annoncent sur le réseau local. Un interrupteur des réglages le décide, activé par défaut, et l'état s'affiche dans le journal comme sur l'écran d'accueil.

Ce que cela suppose : que le réseau local soit celui de son propriétaire. C'est la même hypothèse que celle de la découverte mDNS elle-même, et que celle de l'imprimante ou du partage de fichiers de la même machine. Ce que cela n'ouvre pas : rien qui vienne d'ailleurs que du réseau local. Seule une machine capable de parler sur ce réseau peut s'y annoncer, et une annonce ne donne accès qu'à ce que l'accès distant laisse passer par ailleurs.

Ce que cela remplace : l'obligation de recopier une empreinte de soixante-quatre caractères d'un ordinateur à l'autre avant la première session. Cette étape n'apportait aucune garantie que le réseau local ne donnait pas déjà, et elle coûtait à chaque installation.

Ce que cela ne couvrira pas : les sessions passant par Internet. Le jour où le broker existe (jalon M5), les appareils d'un compte se reconnaissent par leur enregistrement auprès de lui, et cette confiance-là cesse de s'appliquer au-delà du réseau local. L'interrupteur permet, dès aujourd'hui, de s'en passer sur un réseau dont on ne répond pas : la liste écrite reprend alors seule la main.

## 2. Tickets de session

1. Le client demande au broker une session vers un appareil du même compte, ou vers une machine qu'un contact lui a partagée.
2. Le broker vérifie compte, appareil non révoqué, partage en cours de validité, politique (voir §6), puis émet aux DEUX extrémités un ticket signé Ed25519 de courte durée (60 s, nonce unique, empreintes des deux appareils, titre d'accès) ; les candidats de chemin voyagent à part, au fur et à mesure ([SERVER.md](SERVER.md) §3.6 et §4.4).
3. L'hôte admet l'empreinte du client pour la durée du ticket, dans la même liste relue à chaud que les empreintes écrites et annoncées ; les deux services établissent la connexion QUIC avec leurs certificats d'appareil, et chacun refuse toute autre empreinte que celle du ticket (épinglage mutuel, exactement comme avec une empreinte tapée).
4. Anti-rejeu : nonce à usage unique gardé en mémoire le temps de sa validité + expiration courte + tolérance d'horloge de ±5 minutes + protections de la poignée de main QUIC.

Résultat : le chiffrement (TLS 1.3 de QUIC) est négocié directement entre les deux appareils. Le broker sait QUI parle à QUI et QUAND (métadonnées de mise en relation), jamais le contenu. Le relais transporte des paquets qu'il ne peut pas déchiffrer, et seulement entre les deux empreintes qu'un laissez-passer signé lui nomme.

Asymétrie du protocole, à ne pas confondre avec une faille : le client présente son certificat en dernier, et l'hôte ne le juge qu'ensuite. Un appareil refusé voit donc sa connexion réussir avant d'être rompue aussitôt. Rien n'y circule, mais l'interface ne doit jamais annoncer une session établie avant le premier échange réussi. Un test le vérifie dans les deux sens : l'hôte refuse l'inconnu, et l'inconnu perd sa connexion. En pratique, ce premier échange est la question d'ouverture du canal ZyrDesk, dont le client attend la réponse avant de lancer quoi que ce soit : un ordinateur non autorisé s'en va avec un message qui le dit, et non sur un délai d'attente inexpliqué.

## 3. Chiffrement des flux

- Sur le réseau : tout passe dans le tunnel QUIC (TLS 1.3, AEAD par paquet). Image, son, clavier et souris, presse-papiers, fichiers, journal : une seule enveloppe chiffrée de bout en bout.
- Un seul chiffrement, celui du tunnel. Le moteur n'en ajoute aucun : chaque moitié ne parle qu'au service de sa propre machine, par un tube nommé que le réseau ne peut pas atteindre, et ce qu'elle y dit n'existe en clair qu'entre ces deux programmes. Chiffrer une seconde fois ne protégerait rien de plus et coûterait du temps sur chaque image.
- Pas d'appairage : le tunnel a reconnu les deux ordinateurs à leur empreinte avant qu'un seul octet ne passe, et le moteur ne demande rien de plus ([D222](DECISIONS.md)). Il n'y a ni code à afficher, ni code à taper, ni secret propre au moteur à garder.

## 4. Stockage des secrets sous Windows

| Secret | Où | Protection |
|---|---|---|
| Clé privée d'appareil | Profil du service | Prévue : DPAPI dans le profil SYSTEM (PAS DPAPI « machine », que tout utilisateur local peut déchiffrer) + ACL SYSTEM et Administrateurs. Aujourd'hui en clair dans `data\identity` |
| Jeton d'appareil (lien de compte) | Profil du service, `data/account.conf` | Même protection que la clé privée d'appareil, et même limite tant que la DPAPI n'est pas en place ; jalon M5 |
| Jeton de compte (une heure, gestes du compte) | Service, en mémoire le temps d'une action | Jamais écrit ; la fenêtre ne tient aucun jeton |

Le moteur ne garde aucun secret : il n'a ni certificat, ni code, ni identifiant à lui.

## 5. Surface locale

- Tube de commande `\\.\pipe\ZyrDesk` : le compte système et les administrateurs en contrôle total, la personne connectée à la machine en lecture et écriture, rien pour personne d'autre ; les clients distants sont refusés. Qui parle se décide à la création du tube, pas à chaque message : consulter l'état, ouvrir une session et activer l'accès distant sont donc aujourd'hui permis à toute personne connectée à la machine, administrateur ou non. Réserver l'activation de l'hôte et sa configuration aux administrateurs, selon l'identité Windows de l'appelant, reste à faire avant la bêta (§8).
- Tubes du moteur : un par session et par machine, créés par le service sous un nom tiré au sort (plus de 160 bits), pour une seule connexion ; seule la première instance d'un nom le réclame, et les clients distants sont refusés.
  - Chez l'hôte, le tube du moteur n'admet que le compte système (`D:P(A;;GA;;;SY)`). Le service y attend le moteur qu'il vient de lancer et vérifie, par son numéro de processus, que c'est bien lui qui s'y est connecté ; sinon il lâche le tube, arrête le moteur et refuse la session.
  - Chez le client, le tube du lecteur admet le compte système et les personnes connectées à la machine, en lecture et écriture (`D:P(A;;GA;;;SY)(A;;GRGW;;;IU)`) : exactement celles qui peuvent déjà ouvrir une session par le tube de commande.
- Le moteur hôte tourne sous le compte système, dans la session qui tient l'écran. C'est la seule façon de filmer et de piloter l'écran de connexion, l'écran de verrouillage et les invites d'administration : seul ce compte peut rejoindre ces bureaux sécurisés. Il naît dans un objet de tâche qui l'emporte si le service s'arrête, et ne vit que le temps d'une session.
- Les touches et la souris ne sont injectées que par le moteur hôte, et ne lui arrivent que par son tube, que seul le compte système peut ouvrir. Aucun programme de la machine, hors du compte système, ne peut donc lui faire taper quoi que ce soit, pas même sur l'écran de connexion : ce qui arrive par ce tube vient de l'ordinateur d'en face, à travers le tunnel authentifié. Ctrl+Alt+Suppr, que Windows refuse à tout programme qui tape et n'accepte que d'un service ou d'un programme signé à cet effet, est pressé par le service lui-même, à la demande du canal ZyrDesk.
- Aucune touche n'est écrite dans un journal : le moteur hôte compte combien il en a joué, jamais lesquelles.
- Le moteur n'ouvre aucune prise : aucune surface réseau en dehors du port UDP du tunnel et des deux ports du réseau local, tenus par le service.

## 6. Politiques de confiance (défauts proposés, ajustables)

- Appareils d'un même compte : connexion automatique, sans approbation, parce que c'est le sens d'un compte (D122). L'approbation à chaque session est une option par partage prévue dans le modèle et absente du MVP.
- Session entrante : indicateur visible côté hôte (icône + notification de début de session). Un seul spectateur actif ; une nouvelle connexion propose la reprise (takeover) ou est refusée, selon le réglage.
- Révocation : depuis n'importe quel appareil connecté au compte, révoquer un appareil perdu ou volé ; le broker pousse la révocation (les services la reçoivent immédiatement) et la vérifie à chaque émission de ticket. Les tickets étant courts, la fenêtre d'exposition après révocation est de l'ordre de la minute.
- Partage entre comptes : un contact n'ouvre rien ; un partage nomme UNE machine, porte des permissions et une expiration facultative, et se retire d'un clic, session en cours comprise ([SERVER.md](SERVER.md) §5). Le MVP n'en fait respecter que l'accès ; les permissions fines suivront par retenue des entrées dans le tunnel.

## 7. Modèle de menace (résumé)

| Acteur | Capacités | Défense |
|---|---|---|
| Écoute réseau (FAI, Wi-Fi public) | Voit des paquets | QUIC chiffré de bout en bout, rien d'exploitable |
| Relais compromis | Voit les paquets qu'il relaie, peut les jeter | Ne peut ni déchiffrer ni s'insérer (épinglage mutuel des clés) ; au pire, déni de service = bascule de chemin |
| Inconnu qui joint le relais | Peut lui parler en UDP | Rien ne passe sans un laissez-passer signé par le broker ; nouvelles connexions plafonnées par adresse |
| Broker compromis | Métadonnées, comptes ; peut mentir sur la présence et les correspondances | Ne peut PAS s'insérer dans une session (il ne connaît aucune clé privée) ni se faire passer pour un appareil ; il peut refuser le service, ou présenter à un hôte un appareil qu'il a lui-même rattaché au compte. Garde-fous du MVP : chaque session entrante est affichée et journalisée avec le nom de l'appareil et du compte, et la liste des appareils du compte se lit partout ; la chaîne de signatures entre appareils, qui lui retirerait ce pouvoir, est prévue dans le modèle ([SERVER.md](SERVER.md) §3.10) |
| Écoute entre un appareil et le broker | Voit du TLS | HTTPS et WSS seulement, jamais de clair ; un certificat auto-signé est épinglé par sa clé publique, confirmée par une personne ([SERVER.md](SERVER.md) §8) |
| Voleur du PC client | Accès aux secrets locaux | DPAPI + session Windows ; révocation immédiate depuis un autre appareil ; TOTP protège le compte |
| Utilisateur local non privilégié sur l'hôte | Accès au tube de commande ; tentative d'ouvrir le tube du moteur pour taper sur l'écran de connexion | Le tube du moteur n'admet que le compte système, et le service vérifie que c'est son moteur qui s'y connecte ; le tube de commande ne distingue pas encore l'administrateur (§5) |
| Programme local sur le PC client | Tente de prendre le tube du lecteur avant lui | Nom tiré au sort, une seule connexion, réservée au compte système et aux personnes connectées, qui peuvent de toute façon ouvrir une session par le tube de commande |
| Ordinateur d'en face malveillant, mais admis | Envoie des paquets et des messages mal formés au moteur ou au lecteur | Tout est relu sans jamais croire une longueur ; un paquet mal formé est compté et jeté, une image irréparable coûte une image clé |
| Rejeu d'un ticket intercepté | Réutilisation | Nonce unique, expiration 60 s, liaison aux clés des deux appareils |

## 8. Hygiène de base

- Dépendances à leur dernière version stable ([D221](DECISIONS.md)), un retard n'étant admis que justifié par écrit. Le transport suit les correctifs de quinn : le produit exige `quinn-proto` 0.11.18 au moins, qui garde les trois épuisements de mémoire fermés en 0.11.17 et corrige le double décompte de sa file d'envoi ([D221](DECISIONS.md)). FFmpeg, qui lit ce qui arrive de l'autre ordinateur, suit aussi sa dernière version stable : ses DLL sont refaites par `packaging/ffmpeg/build.sh`, depuis des sources dont l'empreinte est vérifiée, et ne contiennent que ce que le moteur emploie.
- Journaux sans secrets (les bundles de diagnostic sont expurgés : pas de jetons, pas de clés, pas d'adresses complètes si non nécessaires).
- Passe de sécurité dédiée avant bêta (jalon M10) : autorisation par message sur le tube de commande, permissions des fichiers, quotas relais, revue des surfaces.
