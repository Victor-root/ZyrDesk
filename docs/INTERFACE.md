# Le nouvel accueil (interface)

Le plan de la fenêtre principale, refaite depuis une page vierge ([D280](DECISIONS.md)). La direction visuelle vient d'une maquette choisie par Victor : le résultat en est très proche, sans la recopier au pixel.

**À lire avec la maquette.** Pour retoucher l'accueil, demander la maquette à Victor (une capture d'écran) : c'est elle qui fait foi pour le visuel (placement, proportions, formes, ambiance), et ce document pour le contenu et ce qui marche ou non. Le code vit dans `crates/zyr-ui/src/home/`, rangé comme [D280](DECISIONS.md) le décrit.

Règle de ce plan : **tout ce qui est prévu se voit dès le premier jour.** Ce qui ne marche pas encore est dessiné à sa place, grisé, avec une bulle « Bientôt » au survol. L'accueil sert ainsi de carte de ce qui reste à faire.

## 1. Principes

- **Thèmes clair et sombre**, par défaut « Automatique » : l'accueil suit le réglage de Windows, et change en direct si Windows change (par exemple en mode automatique jour et nuit). Un choix forcé Clair ou Sombre reste possible dans les paramètres.
- **Couleur d'accentuation : le doré, fixe.** Il n'est écrit qu'à un seul endroit, la charte graphique (`crates/zyr-draw/design.css`) ; tout le reste le lit là. Aucune couleur en dur.
- **Icônes : [Tabler Icons](https://tabler.io/icons)** (licence MIT), au trait, taille unique par contexte. Elles rejoignent `crates/zyr-draw/src/icons.rs`, où un essai lit chaque tracé. La brique de dessin doit savoir tracer un trait aux bouts arrondis, comme Tabler le demande.
- **Tous les textes passent par `zyr-i18n`**, anglais d'abord, français ensuite.
- **Les données séparées du dessin** : ce que l'accueil affiche lui est donné (par le service, le compte, la session) ; le dessin ne va rien chercher lui-même.
- **Fenêtre étroite** : la barre de gauche se replie derrière un bouton « menu » (trois traits) en haut à gauche ; les rangées de cartes passent à la ligne.
- **Barre de titre : celle de Windows, comme la fenêtre d'aujourd'hui**, avec ses boutons réduire, agrandir et fermer, et non une barre dessinée par ZyrDesk (contrairement à la maquette). C'est Windows qui la dessine et la colore selon ses propres réglages : la couleur d'accentuation de Windows quand la personne l'a choisie pour les barres de titre, une barre sombre en thème sombre (`shell/main_window.rs` le fait déjà).
- **Images** : la page est unie. Le bandeau et la barre de gauche sont habillés d'un paysage de dunes fait pour ZyrDesk : sous un soleil voilé en clair, sous la lune en sombre ([D303](DECISIONS.md)) ; dans la barre de gauche, les pages et le pied (la personne, l'état, la version) sont posés sur deux plaques de verre dépoli ([D305](DECISIONS.md)), détachées des dunes par une ombre, un contour et un reflet ([D315](DECISIONS.md)).

Légende des tableaux : **Oui** marche dès le premier jour ; **Grisé** est dessiné mais attend sa fonction.

## 2. Barre de gauche

| Élément | État | Note |
|---|---|---|
| Logo ZyrDesk et nom | Oui | |
| Accueil | Oui | La page de ce plan |
| Ordinateurs | Oui | Tous les ordinateurs, en grille ou en liste ([D310](DECISIONS.md)) |
| Contacts | Oui | Les demandes, et pour chaque contact un interrupteur par ordinateur ; un chiffre compte les demandes reçues ([D319](DECISIONS.md)) |
| Activité | Oui | Les sessions terminées, jour par jour ([D311](DECISIONS.md)) |
| Paramètres | Oui | Les réglages d'aujourd'hui, remis au style de l'accueil |
| À propos | Oui | Juste sous Paramètres (§8) |
| En bas : la personne (initiales, nom, adresse du compte) | Oui | Sans compte : le nom de la session Windows, sans adresse. Avec un compte, un point sur les initiales dit si le serveur répond : vert, il répond, et le nom du serveur est dessous ; orange, « Hors ligne » à sa place, et la bulle dit pourquoi ([D322](DECISIONS.md), [D323](DECISIONS.md)) |
| En bas : « Tout est à jour » | Grisé | Attend les mises à jour automatiques |

## 3. Barre du haut

| Élément | État | Note |
|---|---|---|
| Recherche « Rechercher un ordinateur… » avec Ctrl+K | Oui | Par le nom ou l'adresse, sur l'accueil et la page « Ordinateurs » |
| Filtre « Tous » | Oui | Tous, en ligne, hors ligne, réseau local, Internet |
| Vue en grille ou en liste | Oui | Le choix est gardé d'une ouverture à l'autre |
| Réduire, agrandir, fermer | Oui | Ceux de la barre de titre de Windows (§1) |

## 4. Le bandeau

| Élément | État | Note |
|---|---|---|
| « Bienvenue {prénom} » | Oui | Le nom du compte ZyrDesk ; sans serveur, celui de la session Windows ou Linux |
| Cet ordinateur, sous le message d'accueil | Oui | Voir la section 5 : il vit dans le bandeau ([D283](DECISIONS.md)) |
| Bloc « Accès distant » : l'interrupteur | Oui | Active ou coupe l'accès distant |
| Bloc « Accès distant » : « Cet ordinateur accepte les connexions à distance », ou ce qui l'empêche | Oui | |
| Paysage de dunes | Oui | Remplace l'image abstraite qu'il portait ([D303](DECISIONS.md)) |

L'empreinte de l'ordinateur **ne figure pas** sur l'accueil : elle ne sert qu'à ajouter un ordinateur à la main. Elle vit dans « Ajouter un ordinateur » et dans les détails de « Cet ordinateur ».

## 5. Cet ordinateur

Dans le bandeau, sous le message d'accueil : une carte à part le répétait ([D283](DECISIONS.md)).

| Élément | État | Note |
|---|---|---|
| Fond d'écran de cet ordinateur | Oui | Le sien, lu sur place |
| Nom, avec le crayon pour le renommer | Oui | Renommer passe par le compte quand il y en a un, sinon grisé |
| Système, processeur, mémoire, carte graphique | Oui | Lus sur place |
| Réseau : connecté ou non | Oui | |
| ZyrDesk : « Tout est opérationnel », ou ce qui ne va pas | Oui | L'état du service, avec le fait qui l'explique |
| « Voir les détails », à côté du nom | Oui | Ouvre les détails, empreinte comprise |

## 6. Mes ordinateurs

Une carte par ordinateur connu (réseau local et compte), ou une ligne en vue liste, avec « Voir tout », qui mène à la page « Ordinateurs », et le nombre. Ceux qui acceptent une session viennent d'abord, ceux qui sont hors ligne en dernier, tant que la personne ne les a pas rangés elle-même à la poignée. La recherche et le filtre de la barre du haut s'y appliquent.

| Élément de la carte | État | Note |
|---|---|---|
| Fond d'écran de l'ordinateur | Oui | Entier, au format d'un écran (16:9) ([D315](DECISIONS.md)). Celui de l'ordinateur distant, rapporté par chaque session avec lui ([D312](DECISIONS.md)). Avant une première session, une vue des dunes du paysage de ZyrDesk, propre à chaque ordinateur ([D306](DECISIONS.md)). Plus tard : un fond choisi par la personne |
| Pastille en ligne ou hors ligne | Oui | Devant le nom, pour laisser le fond d'écran libre ([D317](DECISIONS.md)) |
| Nom, adresse ou « En ligne », compte | Oui | |
| Étiquette LAN ou Internet, et la latence | Oui | |
| « Se connecter » | Oui | Ouvre la session |
| « Réveiller » (ordinateur éteint) | Oui | Réveil par le réseau local, une fois sa carte réseau connue ; grisé avant ([D313](DECISIONS.md)) |
| Poignée pour réordonner les cartes | Oui | Visible seulement quand « Réorganiser » est allumé dans la barre du haut ; glisser une carte, ou une ligne en vue liste ; l'ordre est gardé ([D314](DECISIONS.md), [D317](DECISIONS.md)) |
| Ce que l'ordinateur offre d'autre | Oui | À côté de « Se connecter », un bouton à icône par geste, nommé au survol : par le réseau local seulement, son journal, mettre fin à sa session, l'oublier. Les deux derniers demandent un second clic ([D315](DECISIONS.md)) |

## 7. Le bas de la page

| Bloc | État | Note |
|---|---|---|
| Contacts et accès partagés | Oui | Les contacts et les demandes, chacun avec ce qu'il est au compte ; les rôles de la maquette deviennent des interrupteurs par ordinateur, « Admin » est retiré ([D319](DECISIONS.md)) |
| « Inviter un contact » | Oui | Par nom d'utilisateur |
| Sessions récentes : ordinateur, date, durée | Oui | Gardées par le service, dans les deux sens ([D311](DECISIONS.md)) |
| Action rapide « Ajouter un ordinateur » | Oui | La fenêtre d'aujourd'hui, avec l'empreinte |
| Action rapide « Inviter un contact » | Oui | |
| Action rapide « Voir l'activité » | Oui | |
| Action rapide « Ouvrir les paramètres » | Oui | |
| Version et compilation | Oui | En bas de la barre de gauche, la compilation sous le numéro ([D293](DECISIONS.md)) |

## 8. À propos

Une page à elle, sous Paramètres dans la barre de gauche. Une partie de son contenu est une obligation de licence ([COMPLIANCE.md](COMPLIANCE.md)).

- La version, la compilation, le lien vers le code source et la licence GPLv3.
- Les remerciements, chacun avec sa licence et son lien : FFmpeg, x264, Opus, les en-têtes NVIDIA et AMD, le répartiteur Intel, le pilote d'écran virtuel, Tabler Icons, et les principales bibliothèques Rust (réseau, chiffrement, découverte locale).
- Sunshine et Moonlight, dont l'étude a guidé la conception du moteur.
- Les textes complets des licences, consultables.

## 9. Ce qui vient après l'accueil

Les pages Ordinateurs, Contacts et Activité sont faites ([D310](DECISIONS.md), [D319](DECISIONS.md), [D311](DECISIONS.md)). Restent, bloc par bloc, les fonctions grisées : chacune qui arrive retire son gris.
