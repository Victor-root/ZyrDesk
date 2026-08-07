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
│  ZyrDesk                                   ⚙  👤    │
│                                                     │
│  CET ORDINATEUR                                     │
│  ┌───────────────────────────────────────────────┐  │
│  │  PC-BUREAU                    Accès distant   │  │
│  │  ● Prêt à être contrôlé            [ ◉ ON ]   │  │
│  └───────────────────────────────────────────────┘  │
│                                                     │
│  MES ORDINATEURS                                    │
│  ┌─────────────────────┐  ┌─────────────────────┐   │
│  │ ● PC-PORTABLE       │  │ ○ PC-ATELIER        │   │
│  │ RTX 4070 · 1440p    │  │ Hors ligne · 3 j    │   │
│  │   [ Se connecter ]  │  │                     │   │
│  └─────────────────────┘  └─────────────────────┘   │
└─────────────────────────────────────────────────────┘
```

- La carte « Cet ordinateur » porte l'interrupteur Accès distant et son état en langage humain (« Prêt », « Désactivé », « Écran de connexion visible », états dégradés expliqués).
- Les cartes machines : pastille de présence, nom, GPU, dernière connexion, bouton Se connecter proéminent au survol. Clic simple = connexion (le détail est secondaire, accessible par clic droit ou icône).

Fiche machine (panneau latéral, pas une page) : statut, GPU, résolution native, chemin réseau (« Direct disponible » / « Via relais »), latence estimée, réglages propres à cette machine (qualité par défaut, écran cible), actions secondaires (renommer, révoquer).

Session (l'écran le plus important) : la fenêtre vidéo est native et plein écran par défaut. Aucune décoration permanente. Une pilule discrète apparaît en haut au survol du bord :

```text
        ┌──────────────────────────────────────────┐
        │  PC-BUREAU · 8 ms · Direct   ⛶  ⚙  ⏻    │
        └──────────────────────────────────────────┘
```

Latence et chemin en un coup d'œil ; au clic : statistiques détaillées (fps capturés/reçus/affichés, débit, pertes, jitter, temps de décodage), changement d'écran, préréglage de qualité, déconnexion. Raccourci clavier global pour capturer/libérer la souris et pour la pilule.

Réglages : deux niveaux. Simple par défaut (qualité en préréglages : Fluide / Équilibré / Qualité, audio, démarrage avec Windows, thème). « Avancé » replié : codec, débit manuel, taille de paquet, décodeur, choix du relais, mode paranoïaque, diagnostic. Le jargon reste rangé là.

Premier lancement : trois choix clairs : créer un compte, se connecter, « utiliser uniquement en réseau local » (sans compte). Deux minutes maximum jusqu'au premier succès.

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
