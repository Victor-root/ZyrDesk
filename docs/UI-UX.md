# Direction UI/UX

L'interface est une priorité de premier rang, pas un habillage. Cible : la qualité perçue d'un logiciel commercial haut de gamme. Anti-modèles explicites : panneau d'administration, formulaires denses, composants par défaut sans identité, esthétique « outil technique open source ».

## 1. Personnalité visuelle

Calme, précise, premium. L'application inspire la même confiance qu'un bon matériel : sobre au repos, réactive à l'usage, jamais bavarde.

- Sombre d'abord : fond quasi noir profond légèrement bleuté, surfaces en élévations subtiles (2 à 3 niveaux), jamais de gris boueux. Thème clair soigné disponible.
- Une seule couleur d'accent : le doré, fixe, écrit à un seul endroit, la charte graphique `crates/zyr-draw/design.css` ([D242](DECISIONS.md)). Il est réservé aux actions principales, à ce qui est choisi et à ce que la main ou le clavier désigne. Les états système utilisent une palette réduite : vert doux (en ligne), ambre (dégradé, relais), rouge calme (erreur), gris (hors ligne).
- Typographie : Segoe UI Variable, la police de Windows, en peu de tailles (message d'accueil, titre, sous-titre, corps, légende), interlignage généreux ; une seule ligne à empattements, la devise du bandeau.
- Grille de 8 px, rayons 10 à 12 px, ombres douces et courtes, pas de bordures dures.
- Mouvement : 150 à 220 ms, courbes en sortie douce, uniquement au service de la compréhension (apparition de cartes, changement d'état, transitions de pages). Jamais d'animation gratuite. Squelettes de chargement pour les listes, pastilles de présence avec pulsation très discrète.

## 2. Écrans

Accueil : son plan est [INTERFACE.md](INTERFACE.md), tiré d'une maquette choisie par Victor, et posé par [D280](DECISIONS.md). Une barre à gauche (les pages, et en bas la personne), une barre en haut (recherche, filtre, vues), puis la page.

- Le bandeau salue la personne par son prénom, présente cet ordinateur, et porte l'interrupteur de l'accès distant avec son état en langage humain (« Cet ordinateur accepte les connexions à distance », « L'accès distant est coupé »). Ce qui empêche d'être joignable est dit, jamais laissé à deviner ([D18](DECISIONS.md)), avec de quoi le réparer : le service qui ne tourne pas se démarre d'un bouton, et FFmpeg, s'il manque, ouvre son dossier. Rien de tout cela ne demande de ligne de commande. Quand un autre ordinateur contrôle celui-ci, le bandeau le dit et offre de reprendre la main.
- Cet ordinateur vit dans le bandeau, sous le message d'accueil, et non dans une carte à part qui répétait le bandeau ([D283](DECISIONS.md)) : son vrai fond d'écran, son nom (renommé par le compte quand il y en a un), ce qu'il est (système, processeur, mémoire, carte graphique), comment vont son réseau et ZyrDesk, et « Voir les détails », qui ouvre tout le reste, dont l'empreinte, qui ne vit plus que là et dans « Ajouter un ordinateur ».
- Les autres ordinateurs viennent dans l'ordre où ils servent : d'abord ceux qui acceptent une session, puis ceux ajoutés manuellement, ceux qui refusent, et enfin ceux qui sont hors ligne.
- Les cartes des autres ordinateurs : une image, la pastille de présence, le nom, ce qu'on en sait, la route (LAN, Internet, manuelle) et son délai, et « Se connecter ». La carte entière est le bouton. Son menu, en haut à droite de l'image, offre de se connecter uniquement sur le réseau local ([D148](DECISIONS.md)), de lire le journal de **cette machine-là** d'ici ([D96](DECISIONS.md)), de renvoyer celui qui contrôle cet ordinateur, et d'oublier un ordinateur ajouté à la main. Elles se remplissent seules à partir des annonces du réseau local et du compte ; « Ajouter un ordinateur » ne sert qu'aux réseaux où l'annonce ne passe pas, se fait sur les deux machines, et donne d'abord l'empreinte et l'adresse de celle-ci, à écrire sur l'autre.
- Ce qui est prévu et ne marche pas encore se voit à sa place, grisé, avec « Bientôt » au survol : les pages Ordinateurs, Contacts et Activité, la recherche, les contacts, les sessions récentes, le réveil d'un ordinateur éteint.
- La version tient au bas de la page, discrète. Quand la fenêtre et le service ne datent pas de la même compilation, elle le dit en ambre : c'est la panne que personne ne pense à vérifier.
- Les dialogues s'ouvrent par-dessus la page assombrie, tous du même cadre : un titre et ce dont il s'agit, la croix, ce qu'ils portent, et leurs boutons en bas, le principal à droite et ce qui détruit à part, à gauche. Ce qui détruit demande un second clic, et la demande s'éteint d'elle-même.
- Le journal rassemble tout ce que le produit a écrit, sous la compilation qui l'a produit, avec un bouton qui copie l'ensemble. Rapporter un problème est un clic et un collage. Le même dialogue sert aux deux journaux, et son titre dit lequel. **Vider** vide celui qui est affiché, y compris celui d'en face : une panne se cherche en vidant les deux, en refaisant l'essai, puis en lisant les deux. Seul **Ouvrir le dossier** ne vaut que chez soi.
- L'icône dans la zone de notification est la seule chose qui reste quand la fenêtre est fermée, et elle est là dès que ZyrDesk tourne ([D20](DECISIONS.md)). Nette quand cet ordinateur peut être pris en main, atténuée sinon, et son infobulle le dit en toutes lettres : un état ne se lit jamais à la couleur seule. Pendant une session, l'infobulle dit cela avant tout le reste, parce que la fenêtre peut être réduite et que l'icône est alors la seule chose à l'écran qui sache que l'ordinateur d'en face est tenu. Sur l'accueil, la croix de la fenêtre range la fenêtre et n'arrête rien, et un clic sur l'icône la rappelle ; sur une session, elle quitte la session et rend l'accueil, l'image étant dans cette fenêtre-là ([D23](DECISIONS.md)). « Quitter » dans le menu de l'icône arrête le service, la session en cours et le produit : c'est le seul geste qui arrête tout.

Fiche d'un autre ordinateur (à venir, un panneau latéral et non une page) : statut, GPU, résolution native, chemin réseau (« Direct disponible » / « Via relais »), latence estimée, réglages propres à cette machine (qualité par défaut, écran cible), actions secondaires (renommer, révoquer).

Session (l'écran le plus important) : l'image est native et s'affiche dans la fenêtre de ZyrDesk elle-même, qui prend l'écran entier ou reste une fenêtre selon le réglage ([D21](DECISIONS.md)). Une session n'ouvre donc jamais de deuxième fenêtre. L'image remplit exactement ce qu'on lui donne, sans bande noire et sans déformation : la fenêtre prend la forme de l'image, et l'ordinateur d'en face met son bureau à la forme demandée ([D22](DECISIONS.md)). Aucune décoration permanente. Une seule marque de ZyrDesk reste posée dessus, en haut à droite : le logo, en petit. Un clic dessus déplie le menu de la session.

```text
                                                 ┌────┐
                                                 │ ZD │
                                                 └────┘
 ┌────────────────────────────────────────────────────┐
 │ Décodage    Encodage    Réseau    Débit            │
 │ 0,84 ms     2,10 ms     1 ms      18,52 Mb/s       │
 │ ────────────────────────────────────────────────── │
 │ ⛶  Fenêtré ou plein écran                          │
 │ ▥  Statistiques                                    │
 │ ◎  Voyants                      [Au besoin]  Tenus │
 │ ⌖  Souris                            [Bureau]  Jeu │
 │ ♪  Son                              [Actif]  Coupé │
 │ ⌨  Clavier                     Partagé  [Immersif] │
 │ ⎘  Presse-papiers        Chacun le sien  [Partagé] │
 │ ⌘  Ctrl+Alt+Suppr         sur l'ordinateur distant │
 │ ⚿  Verrouiller                l'ordinateur distant │
 │ ────────────────────────────────────────────────── │
 │ ▭  Résolution                 client, 3840x2160  ‹ │
 │ ▣  Écran de l'hôte                 DELL U2723QE  ‹ │
 │ ∿  Débit                                   20 Mb/s │
 │    ─────────●──────────────────────────────────    │
 │ ⬚  Codec           [Automatique]  H.264  HEVC  AV1 │
 │ ◐  Cadence de l'écran distant    [Fluide]  Économe │
 │ ────────────────────────────────────────────────── │
 │ ⦸  Masquer ce bouton                         Alt+² │
 │ ⏻  Terminer la session      rend le bureau distant │
 └────────────────────────────────────────────────────┘
```

Trois groupes, et une seule façon de finir : terminer la session, qui rend son bureau à l'ordinateur distant ([D24](DECISIONS.md)). En tête, quatre chiffres lus en direct ; entre crochets sur le dessin, le côté allumé de chaque interrupteur.

En haut, ce qui se fait tout de suite. Chaque entrée affiche le raccourci clavier qui fait la même chose : en mode souris de jeu, le pointeur appartient à l'ordinateur distant et le bouton n'est pas cliquable. Ces combinaisons se choisissent dans les réglages, et ce sont celles qui s'affichent ; une entrée qui n'en a pas n'en montre aucune, le lecteur jouant dans ZyrDesk même et n'ayant plus de combinaisons à lui.

Au milieu, ce qu'une session demande : la résolution, l'écran de l'ordinateur distant quand il en a plusieurs, le débit, le codec, et ce que fait un écran immobile. Ils sont là et pas dans l'écran des réglages parce qu'on les change en regardant l'image qu'ils changent, et que revenir sur un écran de réglages pour en essayer un, c'est s'éloigner de la seule chose qui dit si ça a marché. La résolution et l'écran ouvrent leur liste sur le côté, avec une coche sur la valeur en place ; la résolution dit à quoi « ici » revient sur cet ordinateur, le mot seul ne disant pas si on demande du 4K ou du 1080p. Le débit se pousse sur une barre, d'un mégabit par cran, de 5 à 80 Mb/s. Les codecs sont des boutons, et ceux que l'ordinateur distant ne sait pas produire sont barrés. Chacun agit tout de suite, sans rien à appliquer : la session le prend là où elle est ([D117](DECISIONS.md)), et rien n'est jamais relancé.

Posé en M4 ([D16](DECISIONS.md)). L'entrée « Statistiques » pose en haut de l'image un bandeau relu cinq fois par seconde, sur une ligne quand l'image est assez large et sur plusieurs sinon, avec le bouton et les voyants juste en dessous ([D232](DECISIONS.md)) : le codec, la taille et les images par seconde, puis le temps passé chez l'hôte, le réseau, le décodage, l'affichage, le débit, les pertes, et la latence de bout en bout, de la capture là-bas à l'affichage ici ([MOTEUR.md](MOTEUR.md) §9). À venir : le chemin réseau en un coup d'œil.

Paramètres : une page de l'accueil, ce qui ne se règle pas en regardant l'image, en cartes. Général (le thème, le démarrage avec Windows, la confiance aux ordinateurs du réseau local), le compte et ses ordinateurs, les raccourcis clavier, les journaux, et, repliés dans « Réglages avancés », le codec, la fenêtre de la session, la souris, le silence des enceintes de l'ordinateur distant, les statistiques et les essais réseau. Une ligne y rappelle ce qu'une session demanderait maintenant, sans qu'on puisse la changer là : c'est le menu de la session qui la porte. À venir : taille de paquet, décodeur, choix du relais, mode paranoïaque. Le jargon reste rangé dans « Réglages avancés ».

À propos : une page sous Paramètres. La version, le code source, la licence, et ce dont ZyrDesk est fait, chacun avec sa licence lisible en entier ([COMPLIANCE.md](COMPLIANCE.md)).

Un seul de ces réglages s'écrit aussi tout seul : « fenêtre de la session ». Basculer entre plein écran et fenêtre pendant une session est un choix comme un autre, et il se retrouve écrit ici, donc la session suivante s'ouvre comme la précédente a été laissée. Personne ne doit avoir à dire deux fois la même chose, une fois dans l'image et une fois dans un écran de réglages.

Premier lancement : trois choix clairs : créer un compte, se connecter, « utiliser uniquement en réseau local » (sans compte). Deux minutes maximum jusqu'au premier succès. Sur un réseau local, aucun code ni empreinte n'est demandé à personne ([D17](DECISIONS.md)) : les ordinateurs apparaissent, on clique.

États vides et erreurs : illustrés sobrement, avec l'action suivante évidente (« Aucun ordinateur pour l'instant : installez ZyrDesk sur l'autre PC et connectez-vous au même compte »). Les erreurs réseau disent ce qui se passe et ce que ZyrDesk fait (« Chemin direct indisponible, connexion via relais... »).

## 3. Design system (construit au jalon M4, pas après coup)

- Tokens : couleurs (sémantiques, pas de valeurs en dur dans les vues), espacements, typographie, rayons, ombres, durées et courbes d'animation. Déclinés sombre/clair.
- Composants : boutons (3 variantes x 3 tailles), carte machine, pastille de présence, interrupteur, champs, menus, menus contextuels, dialogues, toasts, badges d'état, squelettes, barres de progression, pilule de session, états vides.
- Chaque composant définit ses états : normal, survol, focus visible (accessibilité clavier), actif, désactivé, chargement.
- Accessibilité : contrastes AA minimum, navigation clavier complète, cibles de clic de 32 px minimum, focus toujours visible, texte des états jamais porté par la couleur seule.
- Iconographie : un seul set cohérent, [Tabler Icons](https://tabler.io/icons) pour l'accueil, au trait, taille unique par contexte ([D242](DECISIONS.md)).

## 4. Ton et langage

Une langue professionnelle, sobre et précise, sans jargon dans le parcours principal (« Accepte les connexions », pas « Service actif : streaming host initialisé »), dans chacune des langues du produit : l'anglais d'abord, le français ensuite, la fenêtre parlant la langue d'affichage de Windows ([D236](DECISIONS.md)). Les termes techniques n'apparaissent que dans Avancé et Diagnostic. Jamais de fenêtre modale bloquante pour une information non critique : les événements passent par des toasts discrets. Le français vouvoie, et une même chose porte le même mot partout (« ordinateur distant », « accès distant »), sans registre familier.

## 5. Ce que l'utilisateur ne verra jamais

Aucun nom de bibliothèque, de codec ou de pilote dans le parcours principal : FFmpeg, NVENC ou Direct3D n'apparaissent que dans Avancé, dans le journal, et là où il faut dire quoi réparer (« FFmpeg manque »). Les crédits complets et les licences vivent dans « À propos » (obligation légale et reconnaissance méritée) : FFmpeg, x264, Opus, l'écran virtuel, et Sunshine et Moonlight, dont l'étude a guidé la conception du moteur.
