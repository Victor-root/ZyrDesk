# Le nouvel accueil (interface)

Le plan de la fenêtre principale, refaite depuis une page vierge quand Victor lancera le chantier. La direction visuelle vient d'une maquette choisie par Victor : le résultat doit en être très proche, sans la recopier au pixel.

**À lire avec la maquette.** Au lancement du chantier, Victor redonnera la maquette (une capture d'écran) dans la conversation : c'est elle qui fait foi pour le visuel (placement, proportions, formes, ambiance), et ce document pour le contenu et ce qui marche ou non. Travailler en l'ayant sous les yeux, et la redemander si elle manque.

Règle de ce plan : **tout ce qui est prévu se voit dès le premier jour.** Ce qui ne marche pas encore est dessiné à sa place, grisé, avec une bulle « Bientôt » au survol. L'accueil sert ainsi de carte de ce qui reste à faire.

## 1. Principes

- **Thèmes clair et sombre**, par défaut « Automatique » : l'accueil suit le réglage de Windows, et change en direct si Windows change (par exemple en mode automatique jour et nuit). Un choix forcé Clair ou Sombre reste possible dans les paramètres.
- **Couleur d'accentuation : le doré, fixe.** Il n'est écrit qu'à un seul endroit, la charte graphique (`crates/zyr-draw/design.css`) ; tout le reste le lit là. Aucune couleur en dur.
- **Icônes : [Tabler Icons](https://tabler.io/icons)** (licence MIT), au trait, taille unique par contexte. Elles rejoignent `crates/zyr-draw/src/icons.rs`, où un essai lit chaque tracé. La brique de dessin doit savoir tracer un trait aux bouts arrondis, comme Tabler le demande.
- **Tous les textes passent par `zyr-i18n`**, anglais d'abord, français ensuite.
- **Les données séparées du dessin** : ce que l'accueil affiche lui est donné (par le service, le compte, la session) ; le dessin ne va rien chercher lui-même.
- **Fenêtre étroite** : la barre de gauche se replie derrière un bouton « menu » (trois traits) en haut à gauche ; les rangées de cartes passent à la ligne.
- **Barre de titre : celle de Windows, comme la fenêtre d'aujourd'hui**, avec ses boutons réduire, agrandir et fermer, et non une barre dessinée par ZyrDesk (contrairement à la maquette). C'est Windows qui la dessine et la colore selon ses propres réglages : la couleur d'accentuation de Windows quand la personne l'a choisie pour les barres de titre, une barre sombre en thème sombre (`shell/main_window.rs` le fait déjà).
- **Images** : aucune photo d'illustration. Le bandeau porte une image abstraite dessinée pour ZyrDesk, déclinée en clair et en sombre.

Légende des tableaux : **Oui** marche dès le premier jour ; **Grisé** est dessiné mais attend sa fonction.

## 2. Barre de gauche

| Élément | État | Note |
|---|---|---|
| Logo ZyrDesk et nom | Oui | |
| Accueil | Oui | La page de ce plan |
| Ordinateurs | Grisé | Page faite après l'accueil, dans son style |
| Contacts | Grisé | Idem |
| Activité | Grisé | Idem |
| Paramètres | Oui | Les réglages d'aujourd'hui, remis au style de l'accueil |
| À propos | Oui | Juste sous Paramètres (§8) |
| En bas : la personne (initiales, nom, adresse du compte) | Oui | Sans compte : le nom de la session Windows, sans adresse |
| En bas : « Tout est à jour » | Grisé | Attend les mises à jour automatiques |

## 3. Barre du haut

| Élément | État | Note |
|---|---|---|
| Recherche « Rechercher un ordinateur, un contact… » avec Ctrl+K | Grisé | |
| Filtre « Tous » | Grisé | |
| Vue en grille ou en liste | Grisé | La grille est la vue du premier jour |
| Réduire, agrandir, fermer | Oui | Ceux de la barre de titre de Windows (§1) |

## 4. Le bandeau

| Élément | État | Note |
|---|---|---|
| « Bonjour {prénom} ! » | Oui | Le nom du compte ZyrDesk ; sans serveur, celui de la session Windows ou Linux |
| « Prenez le contrôle de vos ordinateurs, où que vous soyez. » | Oui | |
| Poste local : la version de Windows | Oui | |
| Réseau : connecté ou non | Oui | |
| ZyrDesk : « Tout est opérationnel », ou ce qui ne va pas | Oui | L'état du service, avec le fait qui l'explique |
| Bloc « Accès distant » : l'interrupteur | Oui | Active ou coupe l'accès distant |
| Bloc « Accès distant » : « Votre ordinateur est prêt à être contrôlé », ou ce qui l'empêche | Oui | |
| Image abstraite et devise « Plus loin ensemble » | Oui | |

L'empreinte de l'ordinateur **ne figure pas** sur l'accueil : elle ne sert qu'à ajouter un ordinateur à la main. Elle vit dans « Ajouter un ordinateur » et dans les détails de « Cet ordinateur ».

## 5. Cet ordinateur

| Élément | État | Note |
|---|---|---|
| Fond d'écran de cet ordinateur | Oui | Le sien, lu sur place |
| Nom, avec le crayon pour le renommer | Oui | Renommer passe par le compte quand il y en a un, sinon grisé |
| « Prêt à être contrôlé » ou ce qui l'empêche | Oui | |
| Système, processeur, mémoire, carte graphique | Oui | Lus sur place |
| « Voir les détails » | Oui | Ouvre les détails, empreinte comprise |

## 6. Mes ordinateurs

Une carte par ordinateur connu (réseau local et compte), avec « Voir tout » et le nombre.

| Élément de la carte | État | Note |
|---|---|---|
| Fond d'écran de l'ordinateur | Grisé | L'ordinateur d'en face enverra le sien. D'ici là, et tant qu'on ne l'a pas, une image abstraite de ZyrDesk. Plus tard : un fond choisi par la personne |
| Pastille en ligne ou hors ligne | Oui | |
| Nom, adresse ou « En ligne », compte | Oui | |
| Étiquette LAN ou Internet, et la latence | Oui | |
| « Se connecter » | Oui | Ouvre la session |
| « Réveiller » (ordinateur éteint) | Grisé | Réveil par le réseau, à faire |
| Poignée pour réordonner les cartes | Grisé | |

## 7. Le bas de la page

| Bloc | État | Note |
|---|---|---|
| Contacts et accès partagés, avec les rôles Admin, Contrôle, Lecture | Grisé | Le partage entre comptes est à faire ; la liste reste vide et grisée |
| « Inviter un contact » | Grisé | |
| Sessions récentes : ordinateur, date, durée | Grisé | Il faudra garder l'historique des sessions |
| Action rapide « Ajouter un ordinateur » | Oui | La fenêtre d'aujourd'hui, avec l'empreinte |
| Action rapide « Inviter un contact » | Grisé | |
| Action rapide « Voir l'activité » | Grisé | |
| Action rapide « Ouvrir les paramètres » | Oui | |
| Version et compilation, en bas à droite | Oui | |

## 8. À propos

Une page à elle, sous Paramètres dans la barre de gauche. Une partie de son contenu est une obligation de licence ([COMPLIANCE.md](COMPLIANCE.md)).

- La version, la compilation, le lien vers le code source et la licence GPLv3.
- Les remerciements, chacun avec sa licence et son lien : FFmpeg, x264, Opus, les en-têtes NVIDIA et AMD, le répartiteur Intel, le pilote d'écran virtuel, Tabler Icons, et les principales bibliothèques Rust (réseau, chiffrement, découverte locale).
- Sunshine et Moonlight, dont l'étude a guidé la conception du moteur.
- Les textes complets des licences, consultables.

## 9. Ce qui vient après l'accueil

Les pages Ordinateurs, Contacts et Activité, faites une fois l'accueil terminé, dans son style. Puis, bloc par bloc, les fonctions grisées : chacune qui arrive retire son gris.
