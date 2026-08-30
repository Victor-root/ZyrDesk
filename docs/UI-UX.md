# Direction UI/UX

L'interface est une priorité de premier rang, pas un habillage. Cible : la qualité perçue d'un logiciel commercial haut de gamme. Anti-modèles explicites : panneau d'administration, formulaires denses, composants par défaut sans identité, esthétique « outil technique open source ».

## 1. Personnalité visuelle

Calme, précise, premium. L'application inspire la même confiance qu'un bon matériel : sobre au repos, réactive à l'usage, jamais bavarde.

- Sombre d'abord : fond quasi noir profond légèrement bleuté, surfaces en élévations subtiles (2 à 3 niveaux), jamais de gris boueux. Thème clair soigné disponible.
- Une seule couleur d'accent : bleu électrique, réservée aux actions principales et aux états « en ligne / connecté ». Les états système utilisent une palette réduite : vert doux (en ligne), ambre (dégradé, relais), rouge calme (erreur), gris (hors ligne).
- Typographie : Inter (ou Geist), 4 tailles seulement (titre, sous-titre, corps, légende), interlignage généreux, chiffres tabulaires pour les métriques.
- Grille de 8 px, rayons 10 à 12 px, ombres douces et courtes, pas de bordures dures.
- Mouvement : 150 à 220 ms, courbes en sortie douce, uniquement au service de la compréhension (apparition de cartes, changement d'état, transitions de pages). Jamais d'animation gratuite. Squelettes de chargement pour les listes, pastilles de présence avec pulsation très discrète.

## 2. Écrans

Accueil :

```text
┌─────────────────────────────────────────────────────┐
│  ZyrDesk                                   ▤  ⚙     │
│                                                     │
│  CET ORDINATEUR                                     │
│  ┌───────────────────────────────────────────────┐  │
│  │  PC-BUREAU                    Accès distant   │  │
│  │  ● Prêt à être contrôlé            [ ◉ ON ]   │  │
│  │  Empreinte de cet ordinateur       [ Copier ] │  │
│  └───────────────────────────────────────────────┘  │
│  ┌───────────────────────────────────────────────┐  │
│  │  Le moteur hôte n'est pas installé…  [Ouvrir] │  │
│  └───────────────────────────────────────────────┘  │
│                                                     │
│  MES ORDINATEURS                                    │
│  ┌─────────────────────┐  ┌─────────────────────┐   │
│  │ ● PC-PORTABLE       │  │ ○ PC-ATELIER        │   │
│  │ 192.168.1.31        │  │ Hors ligne · 3 j    │   │
│  │   Se connecter      │  │                     │   │
│  └─────────────────────┘  └─────────────────────┘   │
│                                                     │
│           ZyrDesk 0.1.0 (599c1c4 2026-08-18)        │
└─────────────────────────────────────────────────────┘
```

- La carte « Cet ordinateur » porte l'interrupteur Accès distant et son état en langage humain (« Prêt », « Désactivé », « Moteur hôte absent », « Démarrage en cours »). Ce qui empêche d'être joignable est dit, jamais laissé à deviner ([D18](DECISIONS.md)).
- Sous la carte, ce qu'il reste à faire pour que le produit marche, avec de quoi le faire : le service qui ne tourne pas se démarre d'un bouton, un moteur qui manque ouvre son dossier. Rien de tout cela ne demande de ligne de commande.
- Les cartes machines : pastille de présence, nom, adresse, bouton Se connecter proéminent au survol. Clic simple = connexion. Elles se remplissent seules à partir des annonces du réseau local ; « Ajouter un ordinateur » ne sert qu'aux réseaux où l'annonce ne passe pas. Ce geste-là écrit l'ordinateur dans les deux sens, et se fait donc sur les deux machines : l'adresse n'est demandée que si on veut aussi le contrôler depuis ici. Elle est ce qui le garde à l'écran, sur une carte comme les autres, à une pastille près : grise, avec « ajouté à la main » écrit à côté, parce que ce réseau ne porte pas son annonce et non parce qu'il serait éteint. Le retirer se fait dans le dialogue d'ajout, là où il a été écrit.
- Une carte porte un second geste et un seul : l'icône du journal, en haut à droite, qui ouvre le journal de **cette machine-là** lu d'ici ([D96](DECISIONS.md)). Le grand clic reste la carte entière, une surface qui la recouvre, et l'icône se pose par-dessus : c'est ce qui permet à une carte d'être un bouton entier sans interdire tout autre bouton dessus. Elle ne s'efface pas pendant une session, alors que le reste de la carte s'estompe : c'est justement à ce moment-là qu'on veut lire ce que la machine d'en face a écrit.
- La version tient au bas de l'écran, discrète. Quand la fenêtre et le service ne datent pas du même jour, elle le dit en ambre : c'est la panne que personne ne pense à vérifier.
- Le journal (icône ▤) rassemble tout ce que le produit a écrit, sous la compilation qui l'a produit, avec un bouton qui copie l'ensemble. Rapporter un problème est un clic et un collage. La même fenêtre sert aux deux journaux, et son titre dit lequel. **Vider** vide celui qui est affiché, y compris celui d'en face : une panne se cherche en vidant les deux, en refaisant l'essai, puis en lisant les deux. Seul **Ouvrir le dossier** ne vaut que chez soi.
- L'icône dans la zone de notification est la seule chose qui reste quand la fenêtre est fermée, et elle est là dès que ZyrDesk tourne ([D20](DECISIONS.md)). Nette quand cet ordinateur peut être pris en main, atténuée sinon, et son infobulle le dit en toutes lettres : un état ne se lit jamais à la couleur seule. Pendant une session, l'infobulle dit cela avant tout le reste, parce que la fenêtre peut être réduite et que l'icône est alors la seule chose à l'écran qui sache que l'ordinateur d'en face est tenu. Sur l'accueil, la croix de la fenêtre range la fenêtre et n'arrête rien, et un clic sur l'icône la rappelle ; sur une session, elle quitte la session et rend l'accueil, l'image étant dans cette fenêtre-là ([D23](DECISIONS.md)). « Quitter » dans le menu de l'icône arrête le service, la session en cours et le produit : c'est le seul geste qui arrête tout.

Fiche machine (panneau latéral, pas une page) : statut, GPU, résolution native, chemin réseau (« Direct disponible » / « Via relais »), latence estimée, réglages propres à cette machine (qualité par défaut, écran cible), actions secondaires (renommer, révoquer).

Session (l'écran le plus important) : l'image est native et s'affiche dans la fenêtre de ZyrDesk elle-même, qui prend l'écran entier ou reste une fenêtre selon le réglage ([D21](DECISIONS.md)). Une session n'ouvre donc jamais de deuxième fenêtre. L'image remplit exactement ce qu'on lui donne, sans bande noire et sans déformation : la fenêtre prend la forme de l'image, et l'ordinateur d'en face met son bureau à la forme demandée ([D22](DECISIONS.md)). Aucune décoration permanente. Une seule marque de ZyrDesk reste posée dessus, en haut à droite : le logo, en petit. Un clic dessus déplie le menu de la session.

```text
                                        ┌────┐
                                        │ ZD │
                                        └────┘
                     ┌────────────────────┐
                     │  ✓ Écran, 3840x2160│  ┌───────────────────────────────────────┐
                     │    2560 x 1440     │  │ ⛶  Fenêtré ou plein écran   Ctrl+Alt+F │
                     │    1920 x 1080     │  │ ▥  Statistiques        Ctrl+Alt+Maj+S │
                     │    1280 x 720      │  │ ⌖  Souris bureau ou jeu Ctrl+Alt+Maj+M│
                     └────────────────────┘  │ ────────────────────────────────────── │
                                             │ ▭  Taille       Écran, 3840x2160    ‹  │
                                             │ ∿  Débit                  20 Mb/s   ‹  │
                                             │ ⬚  Codec              Automatique   ‹  │
                                             │ ⟳  Appliquer les changements           │
                                             │ ────────────────────────────────────── │
                                             │ ⦸  Masquer ce bouton             Alt+² │
                                             │ ⏻  Terminer la session      Ctrl+Alt+W │
                                             └───────────────────────────────────────┘
```

Deux groupes, et une seule façon de finir : la distinction des moteurs entre partir et fermer ne remonte pas jusqu'ici ([D24](DECISIONS.md)).

En haut, ce qui se fait tout de suite. Chaque entrée affiche le raccourci clavier qui fait la même chose : en mode souris de jeu, le pointeur appartient à l'ordinateur distant et le bouton n'est pas cliquable. Celles qui parlent au moteur portent ses combinaisons à lui ; les nôtres se choisissent dans les réglages, et ce sont celles qui s'affichent.

En bas, les trois nombres qu'une session demande. Ils sont là et pas dans l'écran des réglages parce qu'on les change en regardant l'image qu'ils changent, et que revenir sur un écran de réglages pour en essayer un, c'est s'éloigner de la seule chose qui dit si ça a marché. Chacun ouvre sa liste sur le côté, avec une coche sur la valeur en place ; la taille dit à quoi « Écran » revient sur cet ordinateur-ci, le mot seul ne disant pas si on demande du 4K ou du 1080p. La ligne « Appliquer les changements » n'apparaît que quand ce qui est choisi n'est plus ce qui est à l'écran, et relance l'image sans fermer la session ([D27](DECISIONS.md)).

Posé en M4 ([D16](DECISIONS.md)). À venir : latence et chemin réseau en un coup d'œil, statistiques détaillées (fps capturés/reçus/affichés, débit, pertes, jitter, temps de décodage), changement d'écran.

Réglages : ce qui ne se règle pas en regardant l'image. Le thème, la confiance aux ordinateurs du réseau local, la fenêtre de la session, la souris, les statistiques, le démarrage avec Windows, le dossier des journaux, et ce que cet ordinateur fait quand c'est lui qu'on regarde : renvoyer ou non un écran immobile, et la façon de filmer l'écran. Une ligne y rappelle ce qu'une session demanderait maintenant, sans qu'on puisse la changer là : c'est le menu de la session qui la porte. À venir : taille de paquet, décodeur, choix du relais, mode paranoïaque. Le jargon reste rangé dans « Avancé ».

Un seul de ces réglages s'écrit aussi tout seul : « fenêtre de la session ». Basculer entre plein écran et fenêtre pendant une session est un choix comme un autre, et il se retrouve écrit ici, donc la session suivante s'ouvre comme la précédente a été laissée. Personne ne doit avoir à dire deux fois la même chose, une fois dans l'image et une fois dans un écran de réglages.

Premier lancement : trois choix clairs : créer un compte, se connecter, « utiliser uniquement en réseau local » (sans compte). Deux minutes maximum jusqu'au premier succès. Sur un réseau local, aucun code ni empreinte n'est demandé à personne ([D17](DECISIONS.md)) : les ordinateurs apparaissent, on clique.

États vides et erreurs : illustrés sobrement, avec l'action suivante évidente (« Aucun ordinateur pour l'instant : installez ZyrDesk sur l'autre PC et connectez-vous au même compte »). Les erreurs réseau disent ce qui se passe et ce que ZyrDesk fait (« Chemin direct indisponible, connexion via relais... »).

## 3. Design system (construit au jalon M4, pas après coup)

- Tokens : couleurs (sémantiques, pas de valeurs en dur dans les vues), espacements, typographie, rayons, ombres, durées et courbes d'animation. Déclinés sombre/clair.
- Composants : boutons (3 variantes x 3 tailles), carte machine, pastille de présence, interrupteur, champs, menus, menus contextuels, dialogues, toasts, badges d'état, squelettes, barres de progression, pilule de session, états vides.
- Chaque composant définit ses états : normal, survol, focus visible (accessibilité clavier), actif, désactivé, chargement.
- Accessibilité : contrastes AA minimum, navigation clavier complète, cibles de clic de 32 px minimum, focus toujours visible, texte des états jamais porté par la couleur seule.
- Iconographie : un seul set cohérent (type Lucide), trait fin, taille unique par contexte.

## 4. Ton et langage

Français naturel, humain, sans jargon dans le parcours principal (« Prêt à être contrôlé », pas « Service actif : streaming host initialisé »). Les termes techniques n'apparaissent que dans Avancé et Diagnostic. Jamais de fenêtre modale bloquante pour une information non critique : les événements passent par des toasts discrets.

## 5. Ce que l'utilisateur ne verra jamais

Aucun logo, nom, écran ou réglage Sunshine, Moonlight ou GameStream dans le parcours. Les crédits complets et licences vivent dans « À propos » (obligation légale et reconnaissance méritée), formulés comme « ZyrDesk s'appuie sur les projets open source Sunshine et Moonlight ».
