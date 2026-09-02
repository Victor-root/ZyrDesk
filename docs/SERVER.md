# Le serveur ZyrDesk : comptes, mise en relation et relais

Ce document conçoit ce qui manque encore à ZyrDesk pour se joindre à travers Internet sans rien recopier : un serveur facultatif qui tient les comptes, dit qui est en ligne, met deux ordinateurs en relation, et transporte en dernier recours des paquets qu'il ne peut pas lire. Il complète [NETWORK.md](NETWORK.md), qui décrit le tunnel, et [SECURITY.md](SECURITY.md), qui décrit les identités. Rien de ce qui suit n'est codé : c'est l'étude, à valider avant le jalon M5 ([ROADMAP.md](ROADMAP.md)). Les décisions qu'elle prend sont consignées de D118 à D123 dans [DECISIONS.md](DECISIONS.md), et les questions qu'elle laisse à Victor sont rassemblées au §13.

État de l'art vérifié le 2026-09-02 sur les sources citées ; les chiffres datés sont ceux de ce jour-là.

## 0. En dix lignes

1. Le serveur est **facultatif**. Sans lui, ZyrDesk continue de faire ce qu'il fait aujourd'hui : réseau local, VPN, adresse publique avec un port ouvert, deux machines réglées à la main. Aucun compte, aucune dépendance.
2. Avec lui, une personne a un compte, y rattache ses PC, les voit en ligne ou non, ajoute des contacts, leur partage une machine, et se connecte en un clic d'où qu'elle soit.
3. Le serveur **met en relation et ne regarde jamais passer l'image** : en fonctionnement normal, le flux va d'un PC à l'autre. Il ne connaît aucune clé de session et ne peut rien déchiffrer.
4. Quand aucun chemin direct n'existe, un **relais** transporte des paquets chiffrés de bout en bout, adressés par empreinte, sans les ouvrir. Seulement en secours, et la session **revient en direct toute seule** dès qu'un chemin direct apparaît, sans coupure.
5. Le tunnel actuel ne change pas : la migration se fait **sous** lui, dans une couche qui choisit le chemin, de sorte que ni le chiffrement, ni le contrôle de congestion, ni les moteurs ne s'en aperçoivent. On reste sur quinn.
6. Chaque appareil garde sa clé privée chez lui ; le serveur ne détient que des clés publiques, des mots de passe hachés en Argon2id et des jetons courts. Un appareil se révoque en une minute, de n'importe quel autre.
7. Un contact n'ouvre rien : il faut lui partager une machine, explicitement, et le partage se retire. Les permissions fines et l'accès temporaire ont leur place dans le modèle dès le premier jour.
8. Le serveur **refuse le clair** : HTTPS ou rien. Trois façons d'y arriver à l'installation (mandataire inverse existant, certificat auto-signé généré, certificat fourni), et un auto-signé s'épingle dans l'application au lieu de désactiver la vérification.
9. Il s'installe sur un Debian dans un conteneur LXC de Proxmox par `bash install.sh`, un script interactif dans l'esprit des scripts Proxmox-Tools de Victor, aux couleurs de ZyrDesk.
10. Le MVP tient en deux jalons : M5 le serveur, les comptes et le direct à travers Internet ; M6 le relais et la bascule automatique.

## 1. Deux façons de se joindre, un seul produit

### 1.1 Le mode autonome, tel qu'il existe

Tout ce que le produit sait faire aujourd'hui reste, intact, sans compte ni serveur :

- **Réseau local.** Les deux services s'annoncent en mDNS et s'appellent directement en UDP quand le multicast ne passe pas (D19) ; le service hôte admet ce qui s'annonce sous l'interrupteur de confiance au réseau local (D17) ; le code d'appairage des moteurs voyage dans le tunnel. Rien à recopier.
- **VPN, adresse publique avec port ouvert, deux machines réglées à la main.** Le chemin existe déjà, c'est l'ajout par empreinte : sur le PC qui regarde, « Ajouter un ordinateur » avec l'empreinte de l'hôte et son adresse ; sur l'hôte, la même carte avec l'empreinte du PC qui regarde. L'adresse peut être celle d'un VPN, ou une adresse publique derrière laquelle la box de l'hôte renvoie le port **UDP 47000** vers lui. Un seul port, et rien d'autre à ouvrir : les moteurs n'écoutent que sur la boucle locale ([NETWORK.md](NETWORK.md) §8). La liste des appareils admis est relue toutes les cinq secondes, donc rien à redémarrer.

Ce mode ne dépend d'aucune infrastructure et n'en dépendra jamais : c'est la contrainte C5 lue jusqu'au bout, et c'est ce qui permet à quelqu'un de refuser le serveur sans rien perdre de ce qui marche.

Ce que la conception lui apporte en passant : la couche des chemins du §4 sert aussi aux sessions sans serveur. Une adresse tapée à la main devient un candidat parmi d'autres, et un ordinateur qui a plusieurs adresses (VPN et réseau local, par exemple) est joint par la meilleure. C'est ce que fait déjà la course entre adresses annoncées (`Ways::open`), sur un mécanisme plus général.

### 1.2 Le mode compte, ce qu'il ajoute

Un **lien de compte** est un réglage du service : l'adresse d'un serveur ZyrDesk, et ce que cet appareil a obtenu en s'y rattachant. Éteint par défaut. Une fois le lien fait :

- l'accueil montre **Mes ordinateurs** (les appareils du compte, avec une pastille verte quand l'un d'eux est en ligne et joignable) et **Partagés avec moi** (les machines que des contacts ont partagées) ;
- un clic sur l'un d'eux ouvre la session, où qu'il soit : le serveur présente les deux services l'un à l'autre, et ils se joignent en direct si le réseau le permet, par le relais sinon, puis en direct dès que possible ;
- l'écran des réglages gagne une section **Compte** : créer un compte ou s'y connecter, nommer cet appareil, voir et révoquer les appareils du compte, gérer les contacts et les partages ;
- la présence circule dans les deux sens : cet appareil dit s'il accepte l'accès distant, et apprend qui est là.

### 1.3 Comment les deux cohabitent

Les règles sont peu nombreuses, et chacune évite une confusion précise.

- **Les ordinateurs viennent de plusieurs sources et sont fondus par empreinte.** Aujourd'hui l'accueil fond déjà deux sources, ce qui s'annonce et ce qui est écrit à la main, avec pour chaque carte d'où elle vient (`Machine::on_screen`, `Peer { seen, written }`). Le compte est une troisième source, et la carte le dit. Un même PC vu sur le réseau local et rattaché au compte est une seule carte, et le réseau local, qui donne son adresse la plus fraîche, l'emporte.
- **Les autorisations s'additionnent.** Un service admet la réunion de ce qui est écrit dans `authorized-devices.conf`, de ce qui s'annonce sur le réseau local si la confiance lui est accordée, et de ce que le serveur présente par un ticket signé (§3.6). La fonction qui décide existe déjà (`let_in`, dans le service) ; le compte y ajoute un troisième ensemble.
- **Le chemin le plus court gagne, sans demander à personne.** Vers un ordinateur présent sur le réseau local, la session s'ouvre comme aujourd'hui, sans que le serveur soit consulté : ce serait un aller-retour de plus pour apprendre ce que le réseau vient de dire. Le serveur n'entre en jeu que quand la machine visée n'est pas là où on la voit.
- **Sans lien de compte, aucun code ne touche au serveur.** Ce n'est pas un réglage qui vaut « off » : c'est une brique absente. Le service ne connaît pas d'adresse de serveur, n'ouvre aucune connexion sortante, et l'accueil n'a pas la section. On peut désinstaller le serveur du monde entier, ZyrDesk en mode autonome ne le remarque pas.
- **Un serveur injoignable dégrade, il ne casse pas.** Le lien reste écrit, la liste des ordinateurs du compte reste affichée avec ce qu'on en savait, en gris, et tout ce qui ne passe pas par lui continue : réseau local, adresses tapées, sessions en cours. Le service réessaie de se rattacher avec un délai croissant (5 s, puis jusqu'à 2 min) et le dit dans le journal et sur l'accueil.

| Situation | Ce qui se passe |
|---|---|
| Deux PC du même compte sur le même réseau local | Session par le réseau local, comme aujourd'hui. Le serveur n'est pas consulté |
| Même chose, confiance au réseau local coupée sur l'hôte | Le client demande un ticket au serveur ; l'hôte admet l'empreinte présentée. Sans serveur joignable, seule la liste écrite compte |
| Deux PC du même compte, l'un en 4G, l'autre à la maison | Rendez-vous par le serveur, chemin direct par perforation, relais en secours, retour en direct dès que possible |
| Un PC partagé par un contact | Pareil, une fois que le partage existe et tant qu'il n'est pas retiré ni expiré |
| Serveur arrêté, lien de compte présent | Réseau local et adresses écrites marchent ; les cartes du compte passent en gris « injoignable » ; réessais en fond |
| Aucun lien de compte | Le produit d'aujourd'hui, à l'octet près sur le réseau |

## 2. Ce que le serveur fait, et ce qu'il ne fait jamais

Un seul programme, `zyrdesk-server`, deux rôles dans le même processus, le second débrayable :

- **Le broker** : comptes, appareils, présence, contacts, partages, tickets de session, rendez-vous (l'échange des adresses candidates entre deux services), laissez-passer de relais. Il parle HTTPS pour ce qui est demande et réponse, et tient un canal WebSocket chiffré (WSS) par appareil pour ce qui est vivant : présence et rendez-vous.
- **Le relais** : reçoit des paquets d'un appareil et les remet à l'autre, sans les ouvrir, seulement entre deux appareils que le broker a présentés. Il écoute en UDP. Le même port répond aussi à un « miroir » : « voici l'adresse d'où tu m'écris », ce qui est le seul renseignement dont un appareil a besoin pour se faire joindre à travers sa box (§4.2).

Ce qu'il ne fait jamais, et que rien dans son code ne saura faire :

- transporter le flux en fonctionnement normal : quand un chemin direct existe, le serveur ne voit pas un octet de session ;
- détenir une clé de session ou une clé privée d'appareil : le tunnel se négocie entre les deux appareils, en TLS 1.3, avec les certificats qu'ils ont chacun chez eux ;
- lire, décoder ou transcoder quoi que ce soit : le relais copie des paquets qu'il ne peut pas déchiffrer, et n'a pas de GPU ;
- s'insérer dans une session : chaque bout vérifie l'empreinte de l'autre contre le ticket, et le ticket contre la clé du serveur ; un serveur qui mentirait sur l'un ne peut pas mentir sur l'autre sans être vu.

Ce qu'il sait, et qu'il faut dire honnêtement : qui a un compte, quels appareils, lesquels sont en ligne, qui a demandé une session vers qui et quand, combien d'octets le relais a portés pour chaque session relayée. Des métadonnées, jamais le contenu. C'est le même partage des rôles que chez Tailscale (le coordinateur ne voit que des clés publiques, DERP relaie de l'aveugle) et chez iroh, et c'est ce que Parsec revendique de son propre serveur.

## 3. Identités, confiance et secrets

### 3.1 L'appareil, tel qu'il est déjà

Rien ne change à l'identité d'un appareil, et c'est voulu : elle est en place depuis le jalon M2, épinglée par les deux bouts du tunnel, et tout repose dessus.

- Un certificat auto-signé, produit à la première demande et jamais refait, dans `data/identity` (`device.crt`, `device.key`). La clé est **ECDSA P-256**, celle que produit `rcgen` par défaut ; [SECURITY.md](SECURITY.md) disait Ed25519, ce qui était le projet et non le fait, et il est corrigé. Ce qui compte n'est pas la courbe mais l'empreinte.
- L'**empreinte** est le SHA-256 du certificat entier, écrite en 64 caractères hexadécimaux. C'est le nom réseau de la machine : dans les annonces mDNS, dans les listes admises, dans les tickets, dans les trames du relais.
- Le certificat ne se régénère jamais en silence : une identité dont un fichier manque est refusée, parce que changer d'empreinte romprait tous les appairages. Un compte ne change rien à cela : rattacher un appareil, c'est enregistrer l'empreinte qu'il a déjà.

La clé privée est aujourd'hui en clair sous le dossier du produit, ce que [SECURITY.md](SECURITY.md) §4 nomme comme limite assumée ; la protection DPAPI dans le profil SYSTEM reste due, et le lien de compte lui donne une raison de plus d'arriver au jalon M5 : le jeton d'appareil (§3.5) ira au même endroit.

### 3.2 Le serveur

Le serveur a deux clés, et l'application en épingle deux choses.

- Son **certificat TLS**, celui de l'API et du canal WSS. Public et valide quand un mandataire inverse ou un certificat fourni le porte ; auto-signé sinon. Ce que l'application épingle dans le second cas est l'**empreinte de la clé publique** de ce certificat (SHA-256 du `SubjectPublicKeyInfo`, 64 hexadécimaux comme celles des appareils), et non le certificat entier, parce qu'un certificat se renouvelle et qu'une clé peut rester : le script régénère un certificat sur la même clé sans que personne ait à réépingler quoi que ce soit. Le §8 dit comment l'épinglage se fait et se voit.
- Sa **clé de signature**, Ed25519, générée à l'installation dans `/var/lib/zyrdesk-server/keys/`. Elle signe les tickets de session et les laissez-passer de relais. Sa clé publique est remise à l'appareil au rattachement, par le canal TLS déjà vérifié, et épinglée avec le lien : un serveur qui changerait de clé de signature serait refusé avec un message qui le dit, et le chemin pour lui refaire confiance passe par un rattachement neuf.
- Le **relais** a son propre certificat, auto-signé, produit à l'installation. Il n'a pas besoin d'être public : c'est le broker qui donne au client l'adresse du relais et l'empreinte de ce certificat, dans le laissez-passer. Un relais ne se joint jamais sans qu'un broker l'ait nommé.

Perdre `keys/`, c'est perdre l'identité du serveur : tous les appareils devraient se rattacher à nouveau. Le script le dit dans son résumé, et `zyrdesk-server backup` emporte les clés avec la base.

### 3.3 Le compte

- Un **nom d'utilisateur** (3 à 32 caractères, lettres, chiffres, point, tiret, souligné ; unique, insensible à la casse) et un **mot de passe**. Le nom est l'identifiant public : c'est lui qu'un contact tape pour vous trouver. L'adresse e-mail est facultative dans le MVP, gardée pour le jour où le serveur saura envoyer un courrier de réinitialisation ; tant qu'il ne le sait pas, un mot de passe oublié se remet par l'administrateur du serveur, en ligne de commande.
- Le mot de passe est haché en **Argon2id** avec les paramètres par défaut de la caisse `argon2` 0.6.0 (19 MiB de mémoire, 2 passes, 1 fil), qui sont ceux que l'OWASP recommande. Sel aléatoire par compte, comparaison en temps constant. Il n'est jamais journalisé, jamais transmis ailleurs qu'au serveur en TLS, jamais gardé sur l'appareil : le service l'envoie une fois au rattachement et l'oublie.
- Politique de mot de passe : douze caractères au moins, pas de règle de composition (NIST SP 800-63B). Les tentatives de connexion sont ralenties par adresse et par compte, en délai croissant, et jamais bloquées définitivement, un blocage étant une façon d'empêcher quelqu'un d'entrer chez lui.
- Trois politiques d'inscription, choisies à l'installation et modifiables dans la configuration : **ouverte** (n'importe qui crée un compte), **sur invitation** (un code à usage unique, produit par l'administrateur), **fermée** (l'administrateur crée les comptes). Le défaut proposé par le script est l'invitation : un serveur à la maison n'a pas vocation à accueillir Internet.
- La double authentification TOTP n'est pas dans le MVP ; elle reste obligatoire avant une bêta publique ([SECURITY.md](SECURITY.md) §1), et le modèle lui réserve sa colonne.

### 3.4 Prouver l'appareil

Un jeton volé ne doit pas suffire. Un appareil prouve donc qu'il détient sa clé privée, à deux moments :

- **au rattachement** : le service demande un défi au serveur (32 octets aléatoires, valables 60 s), signe avec sa clé d'appareil le défi, l'identifiant du serveur et le mot « link », et envoie son certificat, sa signature, le nom de l'appareil et le mot de passe du compte. Le serveur vérifie la signature avec la clé publique lue dans le certificat, calcule l'empreinte, et rattache ;
- **à chaque ouverture du canal WSS** : le serveur envoie un défi en premier message, le service répond par sa signature avant toute autre chose. Sans elle, le canal se ferme.

La signature se fait avec `ring`, que le transport tire déjà par `rustls`, sur la clé PKCS#8 que `data/identity` contient : aucune caisse de plus côté appareil.

### 3.5 Jetons

- Un **jeton d'appareil**, remis au rattachement : 32 octets aléatoires, présenté en `Authorization: Bearer`, gardé côté serveur sous forme de SHA-256 seulement. Lié à l'appareil, valable 90 jours, renouvelé sans geste tant que l'appareil se présente (le serveur en rend un neuf quand il reste moins de 30 jours), révoqué avec l'appareil.
- Un **jeton de compte**, remis à la connexion par mot de passe, pour les gestes du compte (contacts, partages, appareils) : une heure, opaque, haché de même, rendu au service qui le tient pour la fenêtre le temps d'une action. Le service demande un nouveau mot de passe quand il a expiré ; il n'y a pas de jeton de rafraîchissement dans le MVP, parce que le lien d'appareil suffit à tout ce qui est quotidien, et que les gestes du compte sont rares.
- Aucun jeton dans les journaux, ni côté serveur ni côté appareil. Côté appareil, le jeton d'appareil vit dans `data/account.conf`, à côté de la clé, sous les mêmes limites et la même promesse que la clé (§3.1).

### 3.6 Tickets de session

Le ticket est la parole du serveur sur une mise en relation, et la seule chose que les deux services croient de lui.

```text
{
  "v": 1,
  "kind": "session",
  "session": "b7c1…",             identifiant tiré au sort
  "from": "<empreinte du client>",
  "to": "<empreinte de l'hôte>",
  "issued": 1788307800,
  "expires": 1788307860,          60 secondes
  "grant": "owner" | "share:<id>",
  "nonce": "…"
}
```

L'objet est sérialisé de façon canonique (clés triées, sans espaces), signé en Ed25519 par le serveur, et voyage comme `{ ticket, signature }` en base64. Le broker en remet un aux **deux** services : le client l'utilise pour savoir qui joindre et par où ; l'hôte l'utilise pour **admettre** l'empreinte `from`, dans l'ensemble qu'il tient déjà et relit à chaud (`AllowedPeers::replace_with`), pour la durée du ticket ou jusqu'à ce que la session soit ouverte. Chaque bout vérifie : la signature contre la clé épinglée, `to` ou `from` contre sa propre empreinte, la fenêtre de temps avec cinq minutes de tolérance d'horloge, et le nonce contre un cache des nonces déjà vus pendant cette fenêtre. Un ticket rejoué est jeté avant d'avoir ouvert quoi que ce soit.

Ce que le ticket ne remplace pas : l'authentification mutuelle du tunnel. Le client se présente avec son certificat, l'hôte avec le sien, et chacun refuse toute autre empreinte que celle du ticket, exactement comme aujourd'hui avec une empreinte tapée. Le serveur peut donc présenter, il ne peut pas se faire passer pour l'un ou l'autre.

### 3.7 Laissez-passer de relais

Même forme, autre sujet : `kind = "relay"`, l'empreinte du porteur, celle de l'autre bout, l'identifiant de session, cinq minutes de validité. Le relais vérifie la signature avec la clé du broker (la sienne, dans le même processus, ou celle configurée s'il tourne ailleurs), et **ne transmet qu'entre les deux empreintes nommées**. Un relais n'est jamais ouvert à qui le connaît, ce que Tailscale documente comme le risque d'un DERP sans vérification des clients ; ici le laissez-passer est la règle, pas une option.

### 3.8 Révocation

- **Un appareil**, depuis n'importe quel appareil du compte connecté : ses jetons sont effacés, son canal WSS fermé, les autres appareils prévenus par le canal vivant ; les hôtes le retirent de leur admission ; plus aucun ticket ne le nomme. Effet en quelques secondes pour ce qui est en ligne, et au pire à l'expiration d'un ticket déjà émis, soit une minute : c'est le critère de sortie de M5.
- **Un contact** : la relation passe à « retirée » des deux côtés ; tous les partages entre les deux comptes tombent avec elle.
- **Un partage** : retiré par son propriétaire, ou expiré ; une session en cours sous ce partage est terminée par l'hôte, prévenu par le canal vivant.
- **Le serveur lui-même**, côté appareil : « Détacher cet appareil » efface le lien, le jeton et les épinglages, et l'appareil redevient purement autonome.

Une révocation ne touche jamais aux deux autres sources d'autorisation : un appareil révoqué du compte mais écrit à la main dans `authorized-devices.conf`, ou présent sur un réseau local à qui la confiance est accordée, entre encore par là. C'est cohérent avec ce que chaque source promet, et le journal dit par laquelle un appareil est entré. Une liste de refus explicite, consultée en dernier, est notée comme extension (§12) : elle réglerait aussi le cas, déjà connu, d'un ordinateur annoncé sur le réseau local qu'on ne peut pas oublier.

### 3.9 Qui sait quoi

| | Clé privée d'appareil | Clés de session | Contenu (image, son, clavier, souris) | Qui parle à qui, quand | Adresses réseau |
|---|---|---|---|---|---|
| Les deux appareils | La leur | Oui | Oui | Oui | Oui |
| Le broker | Non | Non | Non | Oui | Les candidats qu'il transmet, non conservés |
| Le relais | Non | Non | Non, paquets opaques | Les deux empreintes d'une session, et les octets comptés | Celles des deux bouts, le temps de la session |
| Un observateur du réseau | Non | Non | Non | Deux adresses qui se parlent | Oui |

### 3.10 Un serveur compromis

Il peut : refuser le service, mentir sur la présence, mettre en relation deux appareils qu'il a lui-même enregistrés sur un compte qu'il contrôle, compter et dater. Il ne peut pas : déchiffrer, s'insérer, ni se faire passer pour un appareil dont il n'a pas la clé.

Ce qu'il peut faire de pire est d'ajouter un appareil à lui sur votre compte et de le présenter à votre hôte par un ticket valide. Deux garde-fous dans le MVP : l'hôte affiche et journalise chaque session entrante avec le nom de l'appareil et du compte, et la liste des appareils du compte se lit sur chaque appareil. Le garde-fou complet, où le serveur ne peut plus rien ajouter du tout, est connu et n'est pas dans le MVP : chaque appareil nouveau est approuvé et **signé** par un appareil déjà rattaché, et un hôte n'admet qu'un appareil dont la chaîne de signatures remonte à un appareil qu'il connaît. Le modèle de données réserve les colonnes (`signed_by`, `signature`) pour que ce jour-là soit une migration et non une refonte.

## 4. Se joindre à travers Internet

### 4.1 Le principe : l'aiguilleur sous QUIC

Le tunnel est une connexion QUIC entre les deux services, et le problème de la migration se pose ainsi : QUIC tel que quinn le fournit sait qu'un **client** change d'adresse (le serveur valide le nouveau chemin et le suit), mais aucune API de quinn 0.11 ne permet de dire « envoie désormais à cette autre adresse du pair » ; le multichemin QUIC n'y existe pas (la demande est ouverte depuis 2019, aucune fusion), et l'adresse préférée de la RFC 9000 ne se dit qu'une fois, à la poignée de main.

La réponse n'est pas de migrer QUIC, c'est de ne pas lui laisser voir de chemin. C'est ce que Tailscale fait sous WireGuard, et ce que iroh faisait sous QUIC avant de réécrire son transport : une prise virtuelle.

- Le transport ouvre son point d'accès sur une prise **à nous**, qui implémente l'interface de prise qu'attend quinn (`AsyncUdpSocket`, déjà employée ici : la prise dégradée du banc de mesure en est une). Appelons-la **l'aiguilleur**.
- Chaque ordinateur d'en face reçoit une **adresse de carte**, stable et fictive, dans un préfixe IPv6 privé réservé au produit (`fd7a:7972:6465:736b::/64`, qui n'est jamais posé sur une carte réseau). C'est à cette adresse que quinn croit parler.
- L'aiguilleur tient, pour chaque adresse de carte, la liste des **chemins** réels vers cet ordinateur : des adresses UDP directes (réseau local, adresse publique, adresse mappée par la box) et, s'il y en a, un **relais**. Il envoie chaque paquet par le chemin élu du moment, et remet chaque paquet reçu, d'où qu'il vienne, comme venant de l'adresse de carte.
- Quand l'élection change, quinn ne voit rien : même adresse, même connexion, mêmes clés, même fenêtre de congestion. La migration n'est plus un événement du protocole, c'est une écriture dans une table.

Ce qui reste intact, et c'est le point : la connexion QUIC, TLS 1.3 mutuel épinglé par empreinte, l'ALPN `zyrdesk/1`, le contrôleur de congestion média mesuré au jalon M2, les pompes du tunnel, le canal ZyrDesk, et les moteurs. Le seul fichier du produit qui nomme la bibliothèque de transport (`crates/zyr-transport/src/endpoint.rs`) reste le seul, et l'aiguilleur vit à côté de lui dans le même crate.

Un effet à connaître : après une bascule, l'estimation d'aller-retour de la connexion est celle de l'ancien chemin pendant quelques échanges. Vers un chemin plus court (relais vers direct, le cas voulu), c'est sans conséquence. Vers un chemin plus long (un direct qui meurt, retour au relais), QUIC compte pendant un ou deux allers-retours des pertes qui n'en sont pas, ce qui ne touche que les flux fiables, minuscules ; les datagrammes vidéo ne sont jamais retransmis. Le contrôleur média, lui, ne réagit pas aux pertes par construction et recalcule sa fenêtre sur l'aller-retour mesuré. iroh avait constaté des à-coups sur ses bascules d'avant 0.96 parce qu'il relançait son contrôleur de congestion à chaque changement ; nous ne le relançons pas.

Une taille de paquet, et une seule : QUIC exige que tout chemin porte 1200 octets, et le moteur client fige sa taille de paquet au démarrage sur ce plancher moins nos en-têtes ([NETWORK.md](NETWORK.md) §4). La découverte de MTU est donc **désactivée** sur le point d'accès qui passe par l'aiguilleur, et chaque paquet du tunnel fait au plus 1200 octets, ce qui tient dans un datagramme du relais (§4.5). C'est quelques dizaines d'octets de moins par paquet qu'un chemin direct ordinaire n'en permettrait, et c'est le prix d'une bascule sans que personne s'en aperçoive.

### 4.2 Les chemins possibles et leurs candidats

Un candidat est une adresse UDP à laquelle il vaut la peine d'essayer de joindre l'autre. Chaque service en collecte de quatre provenances, et les envoie au fur et à mesure (§4.4) :

1. **Ses adresses locales**, carte par carte, celles que la découverte mDNS et l'appel direct connaissent déjà. IPv4 et IPv6 : le tunnel n'écoute aujourd'hui qu'en IPv4, il passera en double pile, parce qu'une adresse IPv6 globale des deux côtés est un chemin direct sans aucune box à traverser.
2. **Son adresse vue de l'extérieur**, apprise du **miroir** : depuis sa prise de tunnel (port 47000 côté hôte), le service envoie une sonde au port UDP du serveur, qui répond « tu m'écris depuis telle adresse et tel port ». C'est le rôle que STUN joue ailleurs, dans notre dialecte plutôt qu'un protocole de plus, et sans état côté serveur. iroh l'a d'ailleurs remplacé par la même chose intégrée à QUIC (QAD) ; avec quinn, le miroir est à nous. Cette adresse-là n'a de sens que depuis la prise qui fera le tunnel, parce que la box associe une traduction à une prise : la sonde part donc de la prise du tunnel, jamais d'une autre.
3. **Un mappage de port** demandé à la box, en UPnP, NAT-PMP ou PCP, par la caisse `portmapper` (n0, 0.19.3, en production chez iroh). Quand la box l'accorde, l'adresse publique renvoie directement le port 47000 vers l'hôte : le chemin direct est acquis pour la durée du mappage, renouvelé en fond. Une préférence permet de ne pas le demander.
4. **Une adresse écrite** : celle d'un ordinateur ajouté à la main, ou celle que quelqu'un a renvoyée sur sa box une fois pour toutes.

Un candidat n'est qu'une hypothèse ; la sonde du §4.3 décide.

### 4.3 Les sondes

Les deux services se parlent directement, sur leur prise de tunnel, par de petits datagrammes signés, en plus des paquets QUIC. Ils ont un préfixe magique dont le premier octet ne peut pas être celui d'un paquet QUIC (le bit fixe de QUIC est à un ; celui des sondes à zéro), donc l'aiguilleur les trie avant de rien remettre à quinn.

- `sonde { session, de, vers, numéro, émis }` et `écho { session, de, vers, numéro, adresse vue }`, l'écho renvoyant l'adresse d'où la sonde est arrivée. Les deux sont signés avec la clé d'appareil de l'émetteur, et la signature est vérifiée contre l'empreinte que le ticket nomme : personne d'autre sur Internet ne peut faire valider un chemin, ni faire croire à une adresse.
- Une sonde à tout candidat connu, des deux côtés en même temps, à la réception du ticket : c'est la **perforation**. Chaque box voit sortir un paquet vers l'autre et laisse alors rentrer celui qui vient d'en face. L'écho qui revient **valide** un chemin ; l'adresse vue qu'il rapporte est un candidat de plus. Rythme : toutes les 200 ms les cinq premières secondes, puis toutes les 2 s pendant une minute, puis toutes les 15 s tant que la session est sur le relais.
- Sur le chemin élu, une sonde toutes les 2 s mesure l'aller-retour et tient la traduction de la box ouverte ; trois échos manqués, et le chemin est déclaré mort. Les autres chemins validés, deux au plus, reçoivent une sonde toutes les 5 s pour rester chauds.

Ce que ça ne fait pas dans le MVP : deviner les ports d'une box « symétrique », qui change de port à chaque destination. Tailscale y arrive par la force des grands nombres (des centaines de sondes) ; ce sera une amélioration quand le relais aura montré combien de sessions y restent. Deux box symétriques l'une en face de l'autre, ou une opérateur partagée (CGNAT) sans mappage, sont les cas où le relais est la réponse : Tailscale annonce plus de 90 % de chemins directs, libp2p mesure 70 % sur des millions d'essais, Parsec revendique 97 %. Le reste passe par un relais, chez tout le monde.

### 4.4 Le rendez-vous

La séquence, du clic à la première image, pour deux PC qui ne se voient pas sur un réseau local :

1. La fenêtre demande au service une voie vers l'appareil, par le canal de commande existant (`reach`), avec pour cible l'empreinte de l'appareil du compte.
2. Le service client dit au broker, sur son canal vivant : `session.open { to }`. Le broker vérifie le droit (même compte, ou un partage en cours de validité), que l'appareil visé est en ligne et accepte l'accès distant, qu'aucun des deux n'est révoqué, et tire un identifiant de session.
3. Le broker envoie aux **deux** services `session.start { ticket, pair : { empreinte, nom, compte }, relais : { adresse, empreinte du certificat, laissez-passer } }`. Le relais est absent si le serveur n'en a pas.
4. Chaque service vérifie son ticket. L'hôte **admet** l'empreinte du client pour 60 s. Chacun collecte ses candidats (§4.2), les envoie au broker par `session.candidates` au fur et à mesure qu'ils arrivent, et le broker les retransmet à l'autre. Chacun ouvre en parallèle sa **branche de relais** (§4.5) avec son laissez-passer, ce qui prend un aller-retour vers le serveur.
5. Dès que les premiers candidats d'en face arrivent, les sondes partent des deux côtés (§4.3).
6. Le service client ouvre la connexion QUIC vers l'adresse de carte de l'hôte, **tout de suite**, sans attendre qu'un chemin soit validé : l'aiguilleur l'envoie par la branche de relais dès qu'elle est prête, ou par le premier chemin direct validé si c'est lui qui arrive d'abord, ce qui est le cas courant avec une box ordinaire (une sonde et son écho prennent un aller-retour ; la branche de relais deux). Un paquet parti avant qu'aucun chemin n'existe est simplement perdu, et QUIC le réémet.
7. À partir de là, tout est comme aujourd'hui : première question du canal ZyrDesk qui prouve l'autorisation, écran d'en face, appairage des moteurs dans le tunnel, lancement du lecteur sur les adresses de boucle locale. Rien de ce chemin ne sait qu'un serveur a servi.
8. L'aiguilleur bascule en direct dès qu'un chemin direct est validé, si la session est partie par le relais, et le journal l'écrit (`chemin : relais, puis direct après 340 ms`). Le menu de la session dit le chemin en cours à côté de l'aller-retour.
9. À la fin, le service client dit `session.end` au broker, qui clôt la ligne de session ; les branches de relais se ferment ; l'admission accordée pour le ticket est retirée si elle n'avait pas servi.

Un ordinateur du compte qui est aussi sur le réseau local ne passe jamais par là : ses adresses annoncées sont des candidats connus avant tout ticket, et la session s'ouvre comme aujourd'hui.

### 4.5 Le relais

Un relais qui transporte de l'aveugle a deux modèles éprouvés : DERP, de Tailscale, et son dérivé chez iroh. Les deux adressent les paquets par clé publique, font prouver au client qu'il détient sa clé, et transportent des paquets déjà chiffrés qu'ils ne peuvent pas ouvrir. Les deux, en revanche, transportent sur **TCP** (une connexion HTTPS passée en WebSocket), ce qui est ce qui passe le mieux les pare-feu, et ce qui est le pire pour un flux vidéo : une perte y bloque tout ce qui suit jusqu'à la retransmission. Tailscale s'en accommode parce qu'un relais n'est pour lui qu'un secours ; pour nous aussi, mais un secours qui doit rester regardable.

Le relais ZyrDesk reprend leur modèle de confiance et change le transport :

- **Une connexion QUIC par session et par appareil**, en UDP, vers le port du relais, avec quinn des deux côtés, sous un ALPN à part (`zyrdesk-relay/1`). Le relais présente son certificat, que le client vérifie contre l'empreinte reçue dans le laissez-passer ; le client présente son certificat d'appareil, que le relais accepte tel quel pour lire son empreinte.
- **Le premier flux porte le laissez-passer**. Le relais le vérifie (signature, empreintes, validité), et n'accepte plus rien d'un client qui ne l'a pas envoyé dans les trois secondes. À partir de là, chaque **datagramme** QUIC reçu de l'un est remis tel quel à l'autre bout de la même session, s'il est connecté ; sinon il est jeté, comme sur n'importe quel chemin qui n'est pas encore là. Aucune trame de plus : la session est nommée à l'ouverture, donc les paquets n'ont pas besoin d'adresse, contrairement à DERP où un client en sert plusieurs.
- **Ce qui est transporté est un paquet QUIC entier du tunnel**, chiffré avec les clés que seuls les deux appareils ont. Le relais ne voit ni le tunnel ni ses clés ; il voit du QUIC dans du QUIC, dont il tient la couche extérieure seulement. C'est un double chiffrement sur les deux branches vers le relais, et c'est voulu : la couche extérieure authentifie et protège le relais lui-même. Le coût est celui d'AES-GCM, matériel partout.
- **Datagrammes et non flux** sur cette connexion extérieure, donc pas de blocage en tête de ligne ni de retransmission : une perte entre un appareil et le relais reste une perte, absorbée comme sur un chemin direct par la correction d'erreurs du protocole des moteurs. Le contrôle de congestion de cette connexion extérieure est le même contrôleur média que celui du tunnel, pour la même raison.
- **Une taille qui tient** : les paquets du tunnel font 1200 octets au plus (§4.1), et il faut que le chemin vers le relais porte ces 1200 octets plus l'enveloppe extérieure, une quarantaine d'octets. C'est vrai partout où le MTU dépasse 1280, ce qui est le cas de tout Internet ordinaire, y compris derrière un WireGuard ; un chemin qui n'y arrive pas rend le relais inutilisable, ce que le service dit plutôt que de tenter.
- **Le miroir** vit sur le même port UDP, et répond aux sondes `qui-suis-je` sans rien garder : c'est ce qui donne à un appareil son adresse vue de l'extérieur (§4.2). Un serveur sans relais garde le miroir : il ne coûte rien et c'est lui qui rend le direct possible.
- **Limites et quotas**, parce que le port du relais est la seule chose du serveur que n'importe qui peut atteindre sans compte : un plafond de nouvelles connexions par adresse et par seconde, un plafond de débit par session relayée (60 Mb/s par défaut, réglable) au-delà duquel les datagrammes sont jetés, un nombre maximal de sessions relayées en même temps, et le compte des octets par session, écrit dans la base à la fin pour les quotas par compte à venir. Le relais ne garde rien d'autre.
- **Repli TCP** : hors périmètre, comme [NETWORK.md](NETWORK.md) §6 le disait déjà. Un réseau qui coupe tout UDP coupe le tunnel lui-même, et la réponse propre sera une branche de relais en TLS sur 443, au même endroit de l'aiguilleur, le jour où ce cas se présente assez pour être mesuré.

Le port par défaut du relais est **UDP 443** : c'est celui que les réseaux d'entreprise laissent passer le plus souvent, HTTP/3 l'employant. Il se change à l'installation, et il ne gêne pas un mandataire inverse qui tient TCP 443 sur la même machine.

### 4.6 Choisir et changer de chemin

La règle est celle que Victor a posée : le relais n'est qu'un secours. Traduite dans l'aiguilleur :

- **Un chemin direct validé et vivant gagne toujours** sur le relais, quel que soit l'aller-retour mesuré. Entre plusieurs chemins directs, le plus court en aller-retour, avec une marge de 3 ms avant d'en changer, pour ne pas osciller entre deux chemins équivalents.
- **Le relais sert dès qu'il est prêt et tant qu'aucun direct n'est validé.** C'est « relais d'abord, direct en parallèle », la leçon de Tailscale, et ce n'est pas contraire à la règle : on ne choisit pas le relais, on ne fait pas attendre la session pendant qu'on cherche mieux. Dans le cas courant, le direct est validé avant que la poignée de main QUIC soit finie, et le relais n'a rien porté du tout.
- **La bascule vers le direct est immédiate** à la validation, sans coupure, sans que la session le sache. La bascule inverse, si le direct meurt (trois sondes sans écho, six secondes), revient au relais s'il est encore là. La branche de relais est **gardée chaude** toute la session, par les maintiens de la connexion extérieure, exactement pour que ce retour ne coûte pas une reconnexion. iroh garde la sienne de même.
- **Ce que la personne voit** : le menu de la session dit « Chemin : direct, 12 ms » ou « Chemin : relais, 38 ms ». Le journal note chaque bascule avec sa durée. Aucun réglage ne permet de préférer le relais : ce serait choisir la latence.

### 4.7 Ce qui se passe selon le réseau

| Réseau | Chemin | Ce que ça coûte |
|---|---|---|
| Même réseau local, avec ou sans compte | Direct, par le réseau local, sans serveur | Rien de nouveau |
| Deux box ordinaires (traduction stable par prise) | Direct par perforation, en un aller-retour ; souvent avant la fin de la poignée de main | Un aller-retour vers le serveur pour le rendez-vous |
| Une box ordinaire et un port renvoyé ou mappé chez l'hôte | Direct d'emblée, sans même perforer | Idem |
| IPv6 global des deux côtés | Direct, sans traduction à traverser | Idem |
| Une box symétrique ou une opérateur partagée d'un côté, ordinaire de l'autre | Souvent direct, la sonde du côté ordinaire trouvant le port ouvert par celle d'en face ; sinon relais | Sondes pendant une minute, puis toutes les 15 s |
| Symétrique des deux côtés, ou CGNAT sans mappage | Relais pour toute la session | Un aller-retour de plus, celui vers le relais, et le plafond de débit du relais |
| Réseau qui coupe UDP | Rien, dit clairement | Repli TCP, plus tard |
| Serveur sans relais, aucun direct possible | Rien, dit clairement : « aucun chemin direct, et ce serveur n'a pas de relais » | |

### 4.8 Ce qu'on a envisagé, et pourquoi pas

**Passer à iroh.** iroh 1.1.0 (2026-08-25) est solide : multichemin QUIC réel, perforation dans QUIC, découverte d'adresse dans QUIC, relais éprouvé sur des centaines de millions de points d'accès, MIT ou Apache-2.0. Mais l'adopter, c'est adopter **noq**, son fork de quinn (1.2.0, dix versions depuis février 2026), et non quinn : il n'existe aucune API « quinn plus la perforation d'iroh ». C'est aussi adopter son relais, sur WebSocket et TCP, quand le nôtre reste en datagrammes ; et son transport neuf porte encore des défauts ouverts qui nous concernent directement (des datagrammes remis en rafales avec des trous de 100 à 800 ms sur un flux audio temps réel, n0-computer/iroh#4309, ouvert depuis juin 2026 ; des paquets de poignée de main jetés en silence sous charge, #4325). Ce que nous demanderions à iroh, la migration sans coupure, l'aiguilleur le donne sans changer de transport, pour un millier de lignes à nous et un fichier de transport qui ne bouge pas. D13 prévoyait de réexaminer iroh au jalon M6 sur exactement ces critères ; l'examen est fait, et il est clos par D119. Le jour où noq sera mûr et où le multichemin QUIC sera standard, l'aiguilleur sera précisément l'endroit d'où il pourra entrer.

**TURN et ICE (WebRTC).** TURN alloue une adresse relayée par client, et la migration devient alors un changement d'adresse du pair, ce que QUIC ne sait pas faire de notre côté ; ICE apporte une machine à états entière pour un problème que nous avons des deux bouts. Les caisses Rust correspondantes visent les serveurs de visioconférence, pas un tunnel un-à-un.

**Un relais qui termine QUIC.** Un relais qui déchiffrerait pour rechiffrer verrait tout : contraire à C5 et à la promesse du produit.

**Migrer QUIC lui-même.** L'adresse préférée ne se dit qu'à la poignée de main ; le multichemin n'est pas dans quinn ; changer l'adresse distante n'y est pas exposé. Ce serait une seconde connexion QUIC en direct et une bascule applicative, ce qui coûte une poignée de main de plus et un moment sans image ; l'aiguilleur fait mieux pour moins.

**Le relais sur TCP.** Le plus compatible, le moins regardable. Gardé comme repli futur, à la même place dans l'aiguilleur.

**Un WireGuard embarqué ou Tailscale.** Un réseau virtuel complet pour un tunnel qu'on a déjà, et une couche de plus sous chaque paquet.

### 4.9 Ce que ça coûte, et ce qui le mesurera

Un chemin direct par l'aiguilleur coûte une table de correspondance par paquet et une sonde signée toutes les deux secondes : rien de mesurable au regard du seuil G-lat, et le banc de mesure le vérifiera, la prise dégradée qu'il emploie déjà étant un aiguilleur à un seul chemin. Un chemin relayé coûte un aller-retour supplémentaire, celui de chaque appareil vers le relais, et un chiffrement de plus par paquet : le critère de sortie de M6 reste « surcoût de latence d'au plus un aller-retour vers le relais », et le compteur d'octets du broker doit rester à zéro en direct, ce qui est le critère de M5.

## 5. Comptes, appareils, contacts et partages

### 5.1 Le modèle

```text
compte ──< appareil          (un compte a des appareils ; chacun a une empreinte,
   │                          un nom, un état en ligne, et dit s'il accepte l'accès distant)
   │
   ├──< contact >── compte    (une demande, acceptée, refusée ou retirée ; symétrique
   │                          une fois acceptée)
   │
   └──< partage >── appareil  (un compte propriétaire partage UN de ses appareils
              │               avec UN contact ; permissions, expiration facultative,
              └── contact     retrait à tout moment)
```

Trois idées qui tiennent le modèle droit :

- **Un contact n'est qu'une porte fermée.** Accepter quelqu'un ne lui donne rien d'autre que de pouvoir se voir mutuellement dans une liste et de recevoir des partages. C'est ce que Victor demande, et c'est aussi ce que Parsec fait : un nouvel ami n'a par défaut que la manette.
- **Un partage nomme une machine, pas un compte.** On partage « le PC de l'atelier », pas « tous mes PC ». Il porte ses permissions et, s'il en a une, sa date de fin ; il se retire d'un clic et l'autre bout le voit disparaître.
- **Ses propres appareils se joignent sans approbation**, parce que c'est le sens d'un compte : la décision ouverte O2 est close ainsi. Une approbation à chaque session, comme la touche Ctrl+F1 de Parsec pour un invité, est une option par partage prévue dans le modèle et absente du MVP, parce qu'elle demande une invite sur l'hôte que rien n'affiche encore.

### 5.2 Les gestes

- **Créer un compte** : nom d'utilisateur, mot de passe, e-mail facultatif, code d'invitation si le serveur l'exige. Depuis la fenêtre, dans la section Compte, ou en ligne de commande sur le serveur.
- **Rattacher cet appareil** : adresse du serveur (le §8 dit ce qui est vérifié à ce moment-là), nom d'utilisateur, mot de passe, nom de l'appareil pré-rempli avec le nom de la machine. Le service prouve sa clé (§3.4), reçoit son jeton et la clé de signature du serveur, ouvre son canal vivant, et l'accueil se remplit.
- **Mes ordinateurs** : tous les appareils du compte, cet ordinateur-ci compris, avec leur état : en ligne et joignable, en ligne sans accès distant (avec la raison, que le service connaît déjà : moteur absent, accès coupé), hors ligne depuis telle heure. Renommer et révoquer se font ici.
- **Ajouter un contact** : par nom d'utilisateur. L'autre reçoit la demande sur tous ses appareils et l'accepte ou la refuse ; une demande refusée peut être refaite, une demande en attente s'annule. La liste des contacts montre qui est en ligne, sans dire sur quel appareil.
- **Partager une machine** : choisir un de ses appareils, un contact, et, si on veut, une durée (une heure, un jour, une semaine, une date) ; les permissions sont « contrôle complet » dans le MVP, avec la case « demander mon accord à chaque session » visible mais grisée jusqu'à ce que l'hôte sache l'afficher. Le contact voit la machine dans **Partagés avec moi** dès que l'hôte est en ligne.
- **Retirer** : un partage, un contact (ce qui retire ses partages), un appareil (ce qui ferme ses sessions et ses partages en tant qu'hôte), le lien de cet appareil au serveur.

### 5.3 Permissions, aujourd'hui et demain

Un partage porte un ensemble de permissions écrit dès le premier jour : `connect`, `keyboard`, `mouse`, `audio`, plus tard `clipboard` et `gamepad`. Le MVP les accorde toutes et n'en fait respecter qu'une, `connect` : la session s'ouvre ou ne s'ouvre pas, et elle se ferme quand le partage expire.

Les autres ne sont pas hors de portée, et sans toucher aux moteurs : le flux de contrôle du protocole des moteurs, où voyagent le clavier et la souris, passe en clair sur la boucle locale de l'hôte, dans le tunnel, sous nos pompes. Un service hôte qui connaît les permissions d'une session peut y **retenir** les événements de clavier ou de souris avant qu'ils n'atteignent le moteur, ce qui est un mode « regarder sans toucher » écrit chez nous. C'est l'extension la plus demandée d'un partage (Parsec l'offre par ami : clavier, souris, manette), et elle vient dans un jalon de confort, une fois le relais posé.

### 5.4 Ce que Parsec fait, et ce qu'on en garde

Chez Parsec (documentation de 2026) : des amis ajoutés par nom d'utilisateur ou identifiant, une demande à accepter, un « retirer » ; par ami, des interrupteurs Manette (seul actif par défaut), Clavier, Souris et « peut se connecter sans mon accord (attention) » ; côté hôte, une session d'un ami attend « Accepter » ou Ctrl+F1 sinon elle est refusée ; des équipes avec des groupes, des rôles et des règles de connexion que le poste ne peut pas contourner.

Gardé : la demande de contact symétrique, le principe « un ami n'a rien par défaut », le partage par machine, les permissions par partage, l'approbation à la session comme option. Écarté du MVP : les équipes et les règles, qui sont un produit d'entreprise ; l'approbation à la session, faute d'invite sur l'hôte. Différent par choix : chez nous le partage est explicite et nommé, là où Parsec donne l'accès à tous ses PC dès qu'un ami a une permission ; c'est la ligne « un contact ne reçoit pas automatiquement tous les droits ».

## 6. Le dialogue avec le serveur

Tout ce que le service dit au serveur est **JSON**, versionné dans le chemin (`/v1/`) et dans le premier message du canal vivant. JSON et non le dialecte `verbe clé=valeur` des canaux internes : c'est une API que d'autres programmes pourront appeler, elle se lit à `curl`, et les valeurs y sont typées. Les erreurs rendent un code stable (`invalid_password`, `device_revoked`, `registration_closed`, `upgrade_needed`, ...) et une phrase en anglais neutre ; c'est l'application qui parle français à la personne, comme pour tout le reste.

### 6.1 Ce qui se demande et se répond (HTTPS)

| Méthode et chemin | Qui | Ce que c'est |
|---|---|---|
| `GET /v1/server` | tout le monde | Nom du serveur, version, politique d'inscription, présence d'un relais, clé publique de signature. Lu au rattachement |
| `POST /v1/accounts` | tout le monde, selon la politique | Créer un compte (nom, mot de passe, e-mail facultatif, code d'invitation) |
| `POST /v1/login` | tout le monde | Nom et mot de passe, rend un jeton de compte d'une heure |
| `POST /v1/devices/challenge` | tout le monde | Rend un défi de 60 s |
| `POST /v1/devices` | jeton de compte | Rattacher cet appareil : certificat, signature du défi, nom. Rend le jeton d'appareil |
| `GET /v1/devices` | jeton d'appareil ou de compte | Les appareils du compte, avec leur état |
| `PATCH /v1/devices/{id}` | jeton de compte | Renommer |
| `DELETE /v1/devices/{id}` | jeton de compte | Révoquer |
| `GET /v1/contacts`, `POST /v1/contacts` | jeton de compte | Lister, demander (par nom d'utilisateur) |
| `POST /v1/contacts/{id}/accept`, `.../decline`, `DELETE /v1/contacts/{id}` | jeton de compte | Répondre, retirer |
| `GET /v1/shares`, `POST /v1/shares`, `DELETE /v1/shares/{id}` | jeton de compte | Les partages reçus et donnés ; en créer un (appareil, contact, permissions, expiration) ; en retirer un |
| `GET /v1/live` | jeton d'appareil | Le canal vivant, passé en WebSocket |

### 6.2 Le canal vivant (WSS)

Un par appareil rattaché, ouvert par le service, tenu ouvert par un battement toutes les 30 s ; un silence de 90 s ferme et fait rouvrir. Messages du service vers le serveur : `hello { v, build }`, `proof { signature }` en réponse au défi, `state { hosting, holdup }` à chaque changement d'accès distant, `session.open`, `session.candidates`, `session.end`. Du serveur vers le service : `challenge`, `welcome { devices, contacts, shares }` puis les deltas `presence`, `device.added`, `device.revoked`, `contact.requested`, `contact.answered`, `share.given`, `share.removed`, et les messages de rendez-vous `session.start`, `session.candidates`, `session.end`, `session.refused { code }`.

### 6.3 Versions

Le serveur annonce la sienne dans `GET /v1/server` et dans `welcome` ; le service annonce la sienne dans `hello`. Une paire incompatible est refusée par `upgrade_needed`, avec la version attendue, et la fenêtre dit lequel des deux mettre à jour : c'est la politique d'interopérabilité posée pour M5 dans [ROADMAP.md](ROADMAP.md), appliquée au premier contact.

## 7. Le serveur lui-même

- **Code** : un crate `zyr-server` sous `server/`, dans le même dépôt et le même espace de travail Rust, sous **AGPLv3** (D8 : un service hébergé modifié doit publier ses modifications). Un binaire, `zyrdesk-server`, compilé en statique (cible musl) par un flux de travail « Serveur » de l'intégration continue et publié dans les versions du dépôt, pour que le script d'installation le télécharge et vérifie son empreinte plutôt que de compiler sur place.
- **Briques** : axum 0.8 pour HTTPS et WebSocket, rustls 0.23 (certificat lu en PEM), rusqlite avec SQLite embarqué en mode WAL et des migrations numérotées (`rusqlite_migration`), `argon2` 0.6, `ed25519-dalek` 3, quinn 0.11 pour le relais et le miroir. Un fichier de base, sauvegardable en une commande ; Postgres reste une possibilité quand un besoin réel le demandera ([TECH-CHOICES.md](TECH-CHOICES.md)).
- **Configuration** : `/etc/zyrdesk-server/server.toml`, écrit par le script, relu au démarrage.

```toml
name = "Maison"                       # ce que l'application affiche
data_dir = "/var/lib/zyrdesk-server"

[api]
listen = "0.0.0.0:443"                # ou "127.0.0.1:8443" derrière un mandataire
tls_cert = "/etc/zyrdesk-server/tls/server.crt"   # absents derrière un mandataire
tls_key = "/etc/zyrdesk-server/tls/server.key"
public_url = "https://zyr.exemple.fr" # ce que les appareils tapent

[registration]
policy = "invitation"                 # open | invitation | closed

[relay]
enabled = true
listen = "0.0.0.0:443"                # UDP
max_sessions = 10
max_kbps_per_session = 60000

[limits]
login_attempts_per_minute = 10
```

- **Données** : `zyrdesk.db` (comptes, appareils, jetons hachés, contacts, partages, journal des sessions : identifiants, dates, chemin, octets relayés, gardé 30 jours), `keys/signing.key`, `keys/relay.crt`, `keys/relay.key`. Propriété `zyrdesk:zyrdesk`, mode 700.
- **Ligne de commande d'administration**, sur la machine, sans passer par l'API : `zyrdesk-server status` (comptes, appareils, en ligne, sessions relayées en cours), `user create|list|reset-password|delete`, `invite new|list|revoke`, `fingerprint` (l'empreinte à comparer dans l'application), `check` (le serveur se joint lui-même : API, miroir, relais), `backup <dossier>` (copie cohérente de la base par `VACUUM INTO`, plus la configuration et les clés).
- **Journal** : sur la sortie standard, donc dans journald, une ligne par événement, jamais de secret, jamais d'adresse de candidat.
- **Mise à jour** : relancer le script, qui télécharge la version nouvelle, applique les migrations et redémarre ; ou `zyrdesk-server --version` et le paquet à la main.

## 8. Jamais en clair : TLS et épinglage

La règle, écrite dans le code du serveur et non dans sa documentation : **un point d'écoute HTTP sans TLS n'est accepté que sur une adresse de boucle locale**. `listen = "0.0.0.0:8080"` sans certificat est une erreur de configuration qui empêche le démarrage, avec la phrase qui l'explique. La seule exception est `127.0.0.1:port`, parce qu'un mandataire inverse sur la même machine termine TLS et ne fait circuler le clair que dans la mémoire de cette machine. Et côté application, le service **refuse** une adresse de serveur en `http://` : une adresse tapée sans schéma est comprise en `https://`, et `ws://` n'existe pas.

Trois façons d'y arriver, que le script propose dans cet ordre :

1. **Un mandataire inverse déjà en place** (Caddy, nginx, Traefik, le Nginx Proxy Manager de tant de Proxmox) avec un certificat valide, Let's Encrypt en général. Le serveur écoute en clair sur `127.0.0.1:8443`, le mandataire lui renvoie `https://zyr.exemple.fr`, WebSocket compris (`Upgrade` et `Connection` transmis, délai de lecture long). Le script imprime les lignes exactes pour Caddy et nginx. Rien à épingler : le certificat est public et l'application le vérifie comme n'importe quel navigateur.
2. **Un certificat auto-signé généré par le script**, clé P-256, valable dix ans, portant en noms alternatifs le nom de domaine et l'adresse IP saisis, marqué feuille et non autorité, réservé au rôle serveur. Ce n'est pas un `openssl req -x509` nu : sur Debian, celui-ci produit une autorité (`CA:TRUE`), ce qui est faux pour un serveur et a été vérifié. Le script écrit l'empreinte de la clé publique dans son résumé.
3. **Un certificat fourni** : le script demande le chemin du certificat (chaîne complète) et de la clé, vérifie qu'ils vont ensemble, les copie sous `/etc/zyrdesk-server/tls/` avec les bons droits. Un certificat public se vérifie normalement ; un certificat d'autorité interne se comporte comme l'auto-signé.

**Comment un auto-signé s'épingle sans désactiver quoi que ce soit.** Au rattachement, le service ouvre TLS vers le serveur avec un vérificateur à nous, qui fait deux choses dans l'ordre : vérifier le certificat comme le ferait n'importe quel client (chaîne jusqu'aux racines du système, nom), et, si cela échoue, calculer l'empreinte de la clé publique présentée. La fenêtre l'affiche alors, en huit groupes de huit caractères, avec la phrase : « Ce serveur présente un certificat que personne ne garantit. Compare cette empreinte à celle que l'installation a affichée sur le serveur (`zyrdesk-server fingerprint`) et confirme si elles sont identiques. » Confirmée, l'empreinte est écrite dans le lien de compte, et **chaque** connexion suivante l'exige : un certificat qui la contredit est refusé, avec la même fenêtre pour dire que le serveur a changé de clé et proposer de refaire le rattachement. C'est la sémantique de SSH (`StrictHostKeyChecking=ask`) et celle de l'appairage des appareils déjà en place : la confiance se donne une fois, à un humain qui compare, puis ne se rediscute plus. Ce que ça n'est pas : un client qui accepte tout. La signature du certificat est toujours vérifiée, TLS 1.3 seulement, et un certificat public passe par la voie ordinaire.

Les durées de vie imposées par les navigateurs (200 jours à partir de mars 2026, 47 en 2029) ne s'appliquent qu'aux racines publiques préinstallées ; un certificat épinglé chez nous n'y est pas soumis, et dix ans épargnent un renouvellement à quelqu'un qui n'a pas de raison d'y penser. Un renouvellement, s'il vient, se fait sur la même clé, donc sans réépinglage.

## 9. Sur Debian, dans un conteneur LXC de Proxmox, sous systemd

Cibles : Debian 12 (support LTS jusqu'en juin 2028) et Debian 13 (13.6 est la version stable), en conteneur non privilégié sur Proxmox VE 8 ou 9 (9.2 est la version courante), ou sur toute machine Debian ordinaire. Architectures x86_64 et aarch64.

- **Utilisateur et dossiers** : un compte système `zyrdesk`, sans shell ; `/etc/zyrdesk-server/` (configuration, TLS) lisible par lui seul ; `/var/lib/zyrdesk-server/` (base, clés) à lui, mode 700.
- **Unité systemd** `zyrdesk-server.service` : `User=zyrdesk`, `AmbientCapabilities=CAP_NET_BIND_SERVICE` pour les ports 443, `Restart=on-failure`, `LimitNOFILE=65536`, `RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX`, `NoNewPrivileges` partout où il tient. Le durcissement fondé sur les montages (`ProtectSystem`, `PrivateTmp`, `PrivateDevices`) est écrit dans un fichier de complément que le script n'active que hors conteneur : dans un LXC non privilégié, le profil AppArmor de Proxmox le refuse (« Failed to set up mount namespacing »), et systemd 257, celui de Debian 13, échoue de même sur les identifiants d'unité (`243/CREDENTIALS`) tant que la fonction d'imbrication n'est pas activée. L'unité n'emploie donc ni `LoadCredential=` ni rien qui ne survive pas à un conteneur ordinaire ; le script le détecte, et le dit au lieu d'échouer.
- **Ports** : TCP 443 pour l'API et le canal vivant (ou le port de boucle locale derrière un mandataire), UDP 443 pour le relais et le miroir. Depuis Internet, la box renvoie les deux vers le conteneur ; c'est la seule chose que le script ne peut pas faire à la place de la personne, et il l'écrit dans son résumé avec les numéros.
- **Pare-feu** : le script n'en installe pas. Dans un LXC, c'est le pare-feu de Proxmox (par interface du conteneur) ou celui de la box qui décide, et `nftables.service` dans un conteneur non privilégié est connu pour échouer sur le même espace de montage. Si `ufw` ou `nftables` sont présents et actifs, le script propose les règles ; sinon il les explique.
- **Sauvegarde** : `zyrdesk-server backup` dans un dossier que la sauvegarde du conteneur emporte ; Proxmox sauvegarde de toute façon le conteneur entier.
- **Performance** : un relais n'est que de la copie de paquets ; un conteneur en `veth` derrière un pont porte des dizaines de Mb/s sans y penser. Le seul chiffre publié sur la perte de débit en petits paquets dans LXC date de 2017 et de noyaux anciens ; le critère de M6 le mesurera sur le conteneur réel.

## 10. Le script d'installation

`server/install.sh`, lancé par `bash install.sh` sur le conteneur, ou par `bash <(curl -fsSL …/install.sh)` comme les scripts Proxmox-Tools, une fois publié.

### 10.1 Ce qu'il fait

1. **Bannière** : le logo ZyrDesk en lettres bloc, or ; le sous-titre « Serveur ZyrDesk · installation » ; un trait sur la largeur ; un panneau de contexte : machine, Debian, conteneur LXC ou non, adresse IP, langue.
2. **Vérifications** : root, Debian 12 ou 13, systemd, architecture, `curl` et `openssl`, une installation déjà en place (auquel cas il propose mettre à jour, reconfigurer ou désinstaller). Chaque manque est dit avec la commande qui le règle, et rien n'est modifié tant qu'un préalable manque.
3. **Questions**, chacune pré-remplie avec ce qu'il a détecté ou avec le défaut, validées et redemandées plutôt que refusées : nom affiché du serveur ; adresse publique (domaine ou IP, l'adresse publique détectée en défaut, avec l'avertissement CGNAT si elle est en 100.64/10) ; mode TLS (1 mandataire existant, 2 auto-signé, 3 certificat fourni) et ce qui en découle (port de boucle locale ; ou fichiers) ; port de l'API et port UDP du relais (443 et 443) ; dossier des données ; politique d'inscription (invitation) ; relais activé, plafond de débit et de sessions ; premier compte administrateur (nom et mot de passe, saisi sans écho).
4. **Récapitulatif** dans un panneau or, puis « Lancer l'installation maintenant ? [Entrée=oui / non] ».
5. **Étapes** derrière une roue, chacune réécrite en `✓` ou `✗` : paquets (`ca-certificates`, `curl`, `openssl`), utilisateur et dossiers, téléchargement du binaire et vérification de son empreinte, écriture de la configuration, génération des clés du serveur et du certificat du relais, certificat auto-signé si demandé, unité systemd, activation et démarrage, attente que le serveur réponde, `zyrdesk-server check`, création du compte administrateur et, en politique d'invitation, d'un premier code.
6. **Résumé** dans un panneau vert : adresse à taper dans l'application, mode TLS, **empreinte du serveur** (auto-signé), ports à renvoyer sur la box, dossiers, commandes utiles, code d'invitation, et la ligne sur les clés à sauvegarder. Puis, pour un mandataire, le panneau avec les lignes de Caddy et de nginx.

### 10.2 À quoi il ressemble

```text
███████╗██╗   ██╗██████╗ ██████╗ ███████╗███████╗██╗  ██╗
╚══███╔╝╚██╗ ██╔╝██╔══██╗██╔══██╗██╔════╝██╔════╝██║ ██╔╝
  ███╔╝  ╚████╔╝ ██████╔╝██║  ██║█████╗  ███████╗█████╔╝
 ███╔╝    ╚██╔╝  ██╔══██╗██║  ██║██╔══╝  ╚════██║██╔═██╗
███████╗   ██║   ██║  ██║██████╔╝███████╗███████║██║  ██╗
╚══════╝   ╚═╝   ╚═╝  ╚═╝╚═════╝ ╚══════╝╚══════╝╚═╝  ╚═╝

  Serveur ZyrDesk · installation
────────────────────────────────────────────────────────────

┌ Où l'on est
│ Machine : zyr (Debian 13.6, conteneur LXC non privilégié)
│ Adresse : 192.168.1.40, publique 82.64.12.7
│ Ce script installe le serveur ZyrDesk : comptes, mise en relation, relais.
└

? Nom affiché du serveur [Maison] :
? Adresse publique (domaine ou IP) [82.64.12.7] : zyr.exemple.fr
? Chiffrement de l'API :
   1) J'ai déjà un mandataire inverse avec un certificat valide
   2) Générer un certificat auto-signé (à confirmer dans l'application)
   3) J'ai mes propres fichiers de certificat
? Votre choix [2] :
? Port UDP du relais [443] :
? Dossier des données [/var/lib/zyrdesk-server] :
? Inscriptions : 1) ouvertes  2) sur invitation  3) fermées [2] :
? Activer le relais (secours quand aucun chemin direct n'existe) [Entrée=oui / non] :
? Nom du premier compte [victor] :
? Mot de passe (12 caractères au moins) :

┌ Récapitulatif avant d'installer
│ Serveur        : Maison, https://zyr.exemple.fr
│ TLS            : auto-signé, DNS zyr.exemple.fr + IP 82.64.12.7
│ API            : TCP 443     Relais et miroir : UDP 443
│ Données        : /var/lib/zyrdesk-server
│ Inscriptions   : sur invitation
│ Premier compte : victor
└
? Lancer l'installation maintenant ? [Entrée=oui / non] :

✓ Paquets nécessaires
✓ Utilisateur zyrdesk et dossiers
✓ Téléchargement de zyrdesk-server 0.5.0 (empreinte vérifiée)
✓ Configuration écrite
✓ Clés du serveur et certificat du relais
✓ Certificat auto-signé
✓ Service systemd installé et démarré
✓ Le serveur répond
✓ Compte victor créé, code d'invitation prêt

┌ Serveur ZyrDesk installé
│ Adresse à taper dans l'application : zyr.exemple.fr
│ Empreinte du serveur (à comparer dans l'application) :
│   3f9a1c02 8b7d4e55 0c1a9e77 d2f3a4b1 6e8c0d9f 5a7b2c3d 4e1f6a8b 9c0d2e3f
│ Ports à renvoyer sur la box vers 192.168.1.40 : TCP 443, UDP 443
│ Configuration : /etc/zyrdesk-server/server.toml
│ Données       : /var/lib/zyrdesk-server   (sauvegarde : zyrdesk-server backup <dossier>)
│ Code d'invitation pour un second compte : Q7KD-3MZP
│ Les clés de /var/lib/zyrdesk-server/keys font l'identité du serveur : à sauvegarder.
└
› Relancer ce script met à jour ou reconfigure. « zyrdesk-server status » dit où il en est.
```

### 10.3 L'identité visuelle

L'esprit des scripts Proxmox-Tools, relevé sur leurs sources : un logo en lettres bloc dans une seule couleur d'accent, un sous-titre et un trait, des panneaux ouverts (`┌ Titre`, `│ lignes`, `└`) dont la couleur dit le sens, une ligne par message qui commence par un glyphe (`›` information, `✓` fait, `⚠` attention, `✗` erreur, `?` question), une roue braille sur les étapes longues dont la ligne se réécrit en `✓` ou `✗`, la sortie des commandes cachée sauf en cas d'échec, des valeurs détectées en défaut entre crochets, `[Entrée=oui / non]` écrit en toutes lettres, un « oui » tapé en entier avant ce qui ne se défait pas, un récapitulatif avant d'agir, un panneau vert avec des valeurs alignées à la fin, le français quand la machine est en français et l'anglais sinon, aucune émoticône, jamais de couleur quand la sortie n'est pas un terminal ou que `NO_COLOR` est posé.

Ce qui change, pour que ce soit ZyrDesk et non Proxmox : la palette, tirée de [design.css](../crates/zyr-ui/design.css) plutôt que de l'orange de Proxmox ou du rouge de WireGuard.

| Rôle | design.css | Terminal (256 couleurs) | Où |
|---|---|---|---|
| Accent | `#efb536` | 214 | Logo, titres des panneaux de récapitulatif et d'état |
| Accent vif | `#f8cd6a` | 221 | Sous-titre, chevron `›`, `?` des questions, roue |
| Accent sourd | `#6a4a12` | 94 | Le trait sous la bannière |
| Texte doux | `#a0a7b8` | 248 | Panneaux de contexte, mentions secondaires, défauts entre crochets |
| Texte faible | `#6b7385` | 243 | Indications, « · par Victor-root » |
| En ligne | `#34d399` | 78 | `✓`, panneau final |
| Attention | `#f97316` | 208 | `⚠`, panneaux d'avertissement, `?` d'une confirmation qui engage |
| Erreur | `#f87171` | 203 | `✗` |

Valeurs et chemins en gras plutôt qu'en cyan, parce que ZyrDesk n'a pas de bleu ; sur un terminal qui annonce les couleurs vraies, les teintes exactes du fichier sont employées, et les indices ci-dessus sont le repli.

### 10.4 Ce qu'il ne fait pas

Il ne configure pas le mandataire inverse (il en imprime les lignes), n'ouvre pas la box, n'installe pas de pare-feu, ne compile rien (sauf sur demande, `--from-source`, pour une architecture sans binaire publié), et ne touche jamais à une installation existante sans l'avoir dit et fait confirmer.

### 10.5 Relancer, mettre à jour, désinstaller

Les réponses sont gardées dans `/etc/zyrdesk-server/install.env` et proposées en défaut à la relance. Relancer avec une installation en place ouvre un menu : mettre à jour (télécharge la version publiée, migre, redémarre), reconfigurer (les questions, avec les réponses d'avant), afficher l'état, désinstaller. La désinstallation a deux paliers : arrêter et retirer le service et le binaire, puis, sur un « oui » tapé en entier après un panneau orange qui dit ce que ça détruit, effacer données et clés.

## 11. Ce que ça change dans ZyrDesk

Rien dans les moteurs, et c'est vérifié par construction : tout ce qui suit vit dans nos crates, et les deux moteurs continuent de parler à une boucle locale.

- **`zyr-transport`** : l'aiguilleur (la prise virtuelle, les adresses de carte, la table des chemins, l'élection) ; les sondes (dialecte, signature, vérification, rythmes) ; la branche de relais côté client (connexion QUIC extérieure, laissez-passer, datagrammes) ; le miroir côté client ; la prise en double pile IPv4 et IPv6. `endpoint.rs` reste le seul fichier à nommer quinn ; il apprend seulement à ouvrir un point d'accès sur la prise de l'aiguilleur et à désactiver la découverte de MTU dessus.
- **`zyr-broker`**, nouveau, sans entrée-sortie : ce que le service et le serveur se disent (messages JSON, tickets, laissez-passer, signatures, canonicalisation), écrit une fois et lu des deux côtés, comme `zyr-control` entre la fenêtre et le service. Ses tests sont ceux du dialecte : un ticket rejoué, expiré, mal signé, pour un autre appareil, est refusé.
- **`zyr-account`**, nouveau, côté appareil : le lien de compte (adresse, épinglages, jeton), le rattachement, le canal vivant et sa reconnexion, la présence, le rendez-vous. Un seul crate touche au réseau vers le serveur.
- **`zyrdeskd`** : tenir le lien de compte quand il existe ; alimenter `let_in` d'un troisième ensemble, les empreintes admises par ticket avec leur expiration ; fournir les candidats à l'aiguilleur ; dire au broker l'état de l'accès distant ; ouvrir une voie vers un appareil du compte par le rendez-vous. Le registre des voies gagne le chemin en cours et l'écrit dans `sessions`.
- **`zyr-control`** : des demandes de plus (`account status|link|unlink|login`, `devices`, `contacts`, `shares`, avec leurs réponses en liste terminées par `done`), une provenance de plus sur `Peer` (`account`, `shared`), le chemin sur `Session`, et le numéro de protocole qui monte.
- **`zyr-ui`** : la section Compte des réglages, les deux groupes de l'accueil, les cartes avec leur provenance, la fenêtre d'épinglage, le chemin dans le menu de la session. Le design system ne change pas.
- **`zyr-proto`** : `paths::account()` pour `data/account.conf`, deux préférences (`port_mapping`, et l'adresse du serveur qui, elle, vit dans le lien).
- **`zyr-cli`** : `account status` et `account link` pour diagnostiquer sans fenêtre, `net observe` pour lire son adresse vue par un miroir.
- **Tests** : l'aiguilleur se teste sans réseau (deux prises en mémoire, des chemins qui apparaissent et meurent, la bascule vérifiée paquet par paquet) ; le serveur se teste en mémoire, comme [TESTING.md](TESTING.md) le prévoyait ; le banc de mesure gagne un chemin relayé pour chiffrer le surcoût.

## 12. Le MVP, par tranches, et ce qui vient après

**M5, le serveur et le direct à travers Internet**, en trois tranches livrables l'une après l'autre :

1. Le serveur : comptes, appareils, jetons, canal vivant, présence, `install.sh`, TLS strict et épinglage ; côté appareil, le rattachement et **Mes ordinateurs**. Critère : deux PC du même compte se voient en ligne depuis deux réseaux différents, et l'un révoque l'autre en moins d'une minute.
2. Le rendez-vous et le direct : candidats, miroir, mappage de port, sondes, aiguilleur en double pile. Critère : client en partage de connexion 4G, hôte à la maison derrière une box ordinaire, session en un clic, **zéro octet de session sur le serveur**, G-start Internet tenu (8 s), 30 minutes de 1080p60 stables.
3. Contacts et partages : demande, réponse, retrait, partage d'une machine avec expiration, **Partagés avec moi**. Critère : un contact se connecte à la machine partagée, et plus après le retrait ou l'expiration, session en cours comprise.

**M6, le relais et la bascule** : branche de relais, laissez-passer, quotas, migration dans les deux sens, chemin affiché, banc de mesure relayé. Critère : UDP direct bloqué entre les deux PC, la session s'établit quand même ; on débloque, elle passe en direct sans coupure perceptible ; surcoût du relais d'au plus un aller-retour vers lui.

**Après, dans l'ordre où le besoin se présentera** : TOTP ; courrier de réinitialisation (SMTP) ; permissions fines par retenue dans le tunnel (regarder sans toucher) ; approbation à la session côté hôte ; accès temporaire par lien ; liste de refus consultée en dernier ; plusieurs serveurs par appareil ; chaîne de signatures entre appareils (§3.10) ; devinette des ports pour les box symétriques ; branche de relais en TLS sur 443 ; quotas par compte ; Postgres ; métriques ; équipes.

## 13. Décisions à valider avant de coder

Chacune a un défaut proposé ; dire « d'accord » suffit.

1. **Rester sur quinn et écrire l'aiguilleur et le relais nous-mêmes**, plutôt que de passer à iroh et noq. Défaut : quinn (D119).
2. **Le relais parle QUIC en UDP sur le 443**, en datagrammes, repli TCP plus tard. Défaut : oui (D120).
3. **Le code du serveur vit dans ce dépôt**, sous `server/`, en AGPLv3, compilé et publié par l'intégration continue. Défaut : oui.
4. **Inscriptions sur invitation** par défaut à l'installation ; e-mail facultatif ; réinitialisation par l'administrateur tant qu'il n'y a pas de courrier. Défaut : oui.
5. **Le nom d'utilisateur est l'identifiant public**, celui qu'un contact tape. Défaut : oui.
6. **Ses propres appareils se joignent sans approbation** ; un partage est l'approbation ; « demander mon accord à chaque session » plus tard. Défaut : oui (clôt O2).
7. **Le lien de compte est tenu par le service**, pas par la fenêtre : c'est lui qui a l'identité et le canal vivant, et la fenêtre reste sans état (D2). Défaut : oui.
8. **Debian 12 et 13**, conteneur non privilégié, unité systemd sans les options qui n'y survivent pas ; pas de pare-feu installé par le script. Défaut : oui.
9. **Le serveur officiel** : la décision ouverte O3 reste en l'état, auto-hébergement documenté dès le premier jour ; un serveur hébergé par le projet, s'il vient, sera une instance du même binaire avec des quotas.
