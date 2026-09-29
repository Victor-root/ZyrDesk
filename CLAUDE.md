# ZyrDesk

Bureau à distance open source en Rust, pour Windows, à très faible latence. Un seul programme sert d'hôte (le PC que l'on contrôle), de client (le PC d'où l'on se connecte), ou des deux. Son propre moteur filme l'écran, le compresse par la carte graphique (FFmpeg), le fait passer par un tunnel QUIC chiffré de bout en bout et l'affiche dans la fenêtre de ZyrDesk. Un serveur facultatif (`zyr-server`, sur un Debian) apporte les comptes, la présence et le relais.

## À lire avant tout travail

- `README.md`, section « Conventions du code ».
- `docs/ARCHITECTURE.md` : les processus et les flux, et surtout le §10 (organisation du dépôt, « Qui peut utiliser qui ») et le §11 (ce que les composants se disent).
- `docs/DECISIONS.md` : la raison de chaque choix. Y chercher le sujet avant de le toucher : ce qui a l'air d'un oubli est souvent une décision.
- Selon le sujet : `docs/MOTEUR.md` (le moteur), `docs/NETWORK.md` (tunnel, relais), `docs/SECURITY.md` (identités, chiffrement), `docs/SERVER.md` et `server/README.md` (le serveur), `docs/CLAVIER.md` (touches réservées), `docs/ECRAN-VIRTUEL.md`, `docs/UI-UX.md` (direction visuelle, ton), `docs/TESTING.md` et `perf/GATES.md` (essais et seuils), `docs/TECH-CHOICES.md`, `docs/COMPLIANCE.md` (licences), `docs/ROADMAP.md` (jalons).

## Le mainteneur

- Réponses à Victor : claires et sans jargon, courtes pendant les itérations ; les explications longues, seulement s'il les demande.
- Il compile et essaie lui-même sur ses deux PC Windows à chaque commit poussé, et rapporte les journaux. Un journal se lit en entier avant de conclure.
- Toute demande qui brouillerait une frontière (une brique qui en utiliserait une plus haute, une décision posée dans l'interface, le moteur touché pour une raison qui n'est pas du moteur) se signale avant d'être faite, avec l'endroit où le code aurait sa place. Elle ne s'applique pas d'office.

## Git

- Branche de travail : `develop`. Ne jamais toucher à `main` sans demande explicite. Ne jamais pousser une branche `claude/...`.
- Un gros chantier se découpe en étapes, livrées une à une. Un commit par étape terminée, poussé aussitôt : une interruption ne perd jamais un travail fini.
- Messages de commit en français, comme tout l'historique : un titre qui dit ce qui change, puis pourquoi, et ce qui se voit à l'usage.

## Langues

- **Code en anglais** : fichiers, modules, types, fonctions, variables, constantes, noms et messages des essais, commentaires, scripts, CI, jetons de `design.css`, aide `--help` des outils.
- **Journaux en anglais**, comme les erreurs techniques.
- **En français** : la documentation de `docs/`, les messages de commit, les réponses à Victor.
- **Ce que la personne lit passe par `zyr-i18n`**, jamais en dur : l'anglais d'abord (`crates/zyr-i18n/words/en.txt`), le français ensuite (`fr.txt`), avec les mêmes valeurs entre accolades. La ligne de commande parle anglais. Seule exception, l'accueil actuel, qui garde ses textes français jusqu'à ce qu'il soit refait.
- **Ce qui est écrit sur le disque ou passe sur le réseau garde son nom exact**, même français : valeurs de préférences (`systeme`, `clair`, `sombre`), clés de `install.env`, champs échangés avec le serveur, verbes du tube, noms des règles de pare-feu et des classes de fenêtres, `vendor/ecran-virtuel`. Le renommer casserait une installation existante ou le dialogue entre deux versions.
- **Le tiret long (em dash) est banni partout** : code, commentaires, textes, documentation, commits.

## Les couches

Chaque brique n'utilise que des briques de sa couche ou des couches du dessous (D233) :

| Couche | Briques | Rôle |
|---|---|---|
| Programmes | `zyr-ui`, `zyrdeskd`, `zyr-cli`, `zyr-server` | Assemblent le reste ; rien ne les utilise |
| Produit | `zyr-control`, `zyr-session`, `zyr-clipboard`, `zyr-launch`, `zyr-service` | Le produit qui se parle à lui-même : fenêtre et service, ouverture et reprise d'une session, presse-papiers, mise en route des programmes, logique du service |
| Réseau et comptes | `zyr-transport`, `zyr-tunnel`, `zyr-lan`, `zyr-broker`, `zyr-account` | La connexion entre deux ordinateurs, le réseau local, le serveur |
| Moteur | `zyr-media`, `zyr-codec`, `zyr-link`, `zyr-host`, `zyr-player` | L'image, le son et les entrées, de l'écran filmé à l'image affichée |
| Plateforme | `zyr-draw`, `zyr-screen`, `zyr-sound`, `zyr-system`, `zyr-win32` | Ce que Windows fait pour le produit, et ce avec quoi il est dessiné. Elles répondent, elles ne décident rien pour le produit |
| Base | `zyr-proto`, `zyr-i18n` | Les valeurs que tout le monde partage, et les mots |

Ce que `crates/zyr-layers` vérifie à chaque `cargo test` :

- Le moteur est à part : il n'utilise que le moteur, la base et la plateforme. Rien de ce qu'on fait à la fenêtre, au service ou au réseau n'atteint une ligne du moteur.
- FFmpeg (`zyr-codec`) n'est connu que des deux moitiés du moteur, `zyr-host` et `zyr-player` : les autres passent par elles.
- Seuls `zyr-ui` et `zyr-cli` utilisent `zyr-i18n`.
- Rien ne s'appuie sur un programme.
- La carte de `crates/zyr-layers/src/lib.rs` est exactement le dépôt : une dépendance entre briques ajoutée ou retirée s'y écrit dans le même commit, à découvert. Ce qu'une brique n'utilise que pour ses essais n'y figure pas.

Une nouvelle brique ne naît que pour une responsabilité qui a sa propre raison d'exister, jamais pour ranger un fichier. Elle entre dans le `Cargo.toml` du workspace, dans la carte de `zyr-layers`, dans l'arbre et le tableau du §10 d'`ARCHITECTURE.md`, et dans une décision de `DECISIONS.md`.

## Où va le code

| Ce qu'on ajoute | Où ça va |
|---|---|
| Une valeur, un type, un chemin ou un réglage partagé par plusieurs briques | `zyr-proto` |
| Un texte que la personne lit | une clé dans `words/en.txt` puis `fr.txt`, dite par `zyr-ui` ou `zyr-cli` |
| Un appel à Windows dont le service, la session ou la mise en route ont besoin | la brique de plateforme du sujet (`zyr-system`, `zyr-screen`, `zyr-sound`), qui répond « pas ici » ailleurs que sous Windows |
| Un petit outil Windows commun (texte pour Windows, poignée, code d'erreur, processus) | `zyr-win32` |
| Du dessin : toile, charte graphique, icônes, marque, rythme de ce qui bouge | `zyr-draw` |
| Une décision sur une session : l'ouvrir, la reprendre, allumer un voyant | `zyr-session` |
| Ce que fait le service | `zyr-service` ; `zyrdeskd` ne fait qu'assembler, installer et aiguiller |
| Mettre en route un programme du produit | `zyr-launch`, qui le demande à `zyr-system` |
| Une question de la fenêtre au service | un message de `zyr-control` ; jamais un fichier écrit par l'un et lu par l'autre |
| Ce qu'un service demande à l'ordinateur d'en face | le canal ZyrDesk du tunnel (`zyr-tunnel`, `aside.rs`) |
| La connexion, le relais, la course entre plusieurs adresses | `zyr-transport`, seul à nommer `quinn` |
| Ce que le service et le serveur se disent | `zyr-broker` ; le lien de compte vivant dans `zyr-account` |
| L'image, le son, le clavier et la souris en route | le moteur, et seulement pour une raison de moteur |
| Ce qui se voit dans la fenêtre | `zyr-ui` : `shell/` (le programme, sa fenêtre, l'icône près de l'horloge, le journal, le thème, les réglages, les raccourcis, les questions au service), `home/` (l'accueil), `session/` (l'interface de session) |

## Organisation du code

- Chaque fichier a une responsabilité qu'on peut dire en une phrase, écrite dans son commentaire de tête (`//!`). Du code nouveau va dans le fichier qui porte cette responsabilité, pas dans celui qu'on était en train de modifier.
- Découper quand des choses sans rapport se croisent dans un fichier, jamais pour le seul nombre de lignes : beaucoup de fichiers sont longs à cause de leurs essais, et c'est voulu.
- L'interface affiche et transmet, elle ne décide pas. Un calcul pur (lecture, formule, décision sans effet) vit dans la brique basse qui le concerne, avec ses essais, là où il s'essaie sur tout système.
- Avant d'écrire une aide, chercher si elle existe. Un même morceau écrit deux fois est fusionné.
- Garder privé ce que les autres briques ne nomment pas ; n'exporter que ce qui sert.
- Pas de code mort, de contournement, de solution temporaire ni de reste de tentative. Pas d'`allow(dead_code)` pour faire taire le compilateur, sauf dans les fichiers de `zyr-ui` qui n'existent que sous Windows (`cfg_attr(not(windows), ...)`).
- Vérifier les usages réels avant de supprimer, déplacer ou remplacer du code.
- Un commentaire dit pourquoi, en phrases courtes, jamais ce que le code dit déjà. Chaque bloc `unsafe` porte une note `// SAFETY:` qui dit pourquoi il est sûr.
- Des noms en anglais simple, qui disent ce que la chose est pour le produit (`see_it_through`, `how_far`), plutôt que du jargon.

## Les essais

- Un essai porte le nom d'une phrase anglaise qui dit ce qui est vrai (`a_lost_link_lets_go_of_what_is_held`). Il vit à côté du code qu'il essaie ; les préparations communes à plusieurs fichiers s'écrivent une fois (`zyr-session/src/testing.rs`).
- Une décision sortie de l'interface arrive avec ses essais, qui tournent sur tout système.
- Un essai de bout en bout qui compte sur une cadence ou un délai tourne seul, jamais en même temps qu'un autre (`one_at_a_time` dans `zyr-player/tests/end_to_end.rs`).
- Les essais tournent aussi sous Windows, sous un compte ordinaire et non sous le compte système : un tuyau ouvert par un essai prend l'accès prévu pour les essais dans `zyr-link`, jamais `Access::SystemOnly`, sans quoi l'essai attend pour toujours.
- Les essais du moteur chargent le vrai FFmpeg : sous Linux, `bash packaging/ffmpeg/build.sh linux <dossier>` une fois, puis `ZYR_FFMPEG_DIR=<dossier>/lib`. Un essai qui ne trouve pas FFmpeg échoue, il ne passe jamais sans avoir tourné.

## Faits, mots et journaux

- Les moteurs, le service, le compte et le serveur ne composent jamais de phrase pour la personne. Ils disent un fait, `zyr_proto::fact::Fact` : un code (mots anglais en minuscules reliés par des points, le premier nommant le sujet : `reach.silent`, `session.link_lost`) et des valeurs nommées. Un fait peut porter sa cause (`because`).
- Seules la fenêtre et la ligne de commande choisissent les mots. Le code d'un fait est la clé de son texte.
- Les essais de `zyr-i18n` refusent une langue qui ne dit pas tout ce que dit l'anglais, une clé demandée sans texte anglais et un texte que rien ne demande : retirer un fait, c'est retirer ses textes.
- Le journal : `zyr_proto::log::Log`, horodaté en UTC, une étiquette par module (une constante `TAG`, un mot anglais court, posé par `log.about(TAG)`). `log.write` dit ce que le produit fait, refuse ou trouve ; `log.debug` raconte la plomberie qui marche.
- **Les lignes de débogage s'écrivent toujours**, dans toutes les versions, et se trient à la lecture (`level:debug`, `-level:debug`) : Victor copie le journal entier (D191). Ne jamais les conditionner à `cfg(debug_assertions)`, à un fichier ou à un réglage. Cette règle du projet remplace ici la règle générale de protéger les journaux de débogage.

## Les dialogues entre deux versions

| Dialogue | Son numéro |
|---|---|
| Fenêtre et service, sur le tube `\\.\pipe\ZyrDesk` | `PROTOCOL` dans `zyr-control/src/message.rs` |
| Service et service, sur le canal ZyrDesk du tunnel | `VERSION` dans `zyr-tunnel/src/aside.rs` |
| Lecteur et moteur hôte | `MEDIA_VERSION` dans `zyr-media/src/lib.rs` |
| Service et serveur | `PROTOCOL` dans `zyr-broker/src/lib.rs` |

- Un message ou un champ ajouté, retiré ou qui change de sens fait monter le numéro dans le même commit, et la décision le dit : les deux PC se mettent à jour ensemble. Les formats qui portent leur propre `VERSION` (paquets de `zyr-media`, appel direct de `zyr-lan`, tickets de `zyr-broker`) suivent la même règle.
- Un champ inconnu est ignoré, un champ ajouté après coup se lit avec un défaut, un verbe inconnu se nomme : une moitié plus ancienne perd ce qu'elle ne connaît pas, pas la conversation.
- Tout message nouveau entre dans l'essai d'aller-retour de son dialogue : écrit, puis relu à l'identique.
- Le presse-papiers ne passe jamais par le tube de la fenêtre.

## L'interface

- **Aucune couleur en dur.** Tout ce qui est coloré passe par les rôles de la charte, `crates/zyr-draw/design.css`, dans ses deux thèmes. Le doré n'est que la valeur par défaut de la couleur d'accentuation, qui deviendra un choix de la personne dans les réglages du nouvel accueil : rien ne suppose qu'elle est dorée. Seuls l'icône du programme et le logo livrés dans `packaging/brand` restent dorés.
- Les icônes vivent dans `zyr-draw/src/icons.rs`, et un essai lit chaque tracé.
- La fenêtre ne parle au lecteur que par des appels qui n'attendent jamais : rien de l'interface ne se met sur le trajet d'une image.
- L'interface de session est définitive (D238) : on la retouche à la demande, sans la réorganiser.
- **L'accueil sera refait de zéro**, à partir d'une page vierge, quand Victor le demandera. D'ici là, on ne l'améliore pas : on le garde en état de marche. Le nouvel accueil naîtra avec l'accès aux données séparé du dessin, tous ses textes par `zyr-i18n`, les jetons de la charte et la couleur d'accentuation dans ses réglages.
- Ton, accessibilité et ce que la personne ne voit jamais (noms de bibliothèques, de codecs, de pilotes) : `docs/UI-UX.md`.

## Windows et les autres systèmes

- Tout se compile et s'essaie sous Linux comme sous Windows : la CI fait les deux.
- Les briques de plateforme répondent honnêtement hors de Windows, par un « pas ici », pour que la logique qui s'en sert soit essayée partout.
- Aucun code propre à Windows dans la logique du produit et du réseau (`zyr-service`, `zyr-session`, `zyr-launch`, `zyr-tunnel`, `zyr-transport`, `zyr-account`, `zyr-broker`, `zyr-lan`) : pas de fonction écrite en deux exemplaires, une pour Windows et une pour ailleurs.

## Performance et sécurité

- La performance d'abord : 1080p60 et 1440p60 réels, latence minimale, jamais sacrifiées pour simplifier l'architecture. Un changement qui touche la fluidité se juge sur les mesures (étiquettes `pace`, `tunnel` et `flow` du journal, `zyr-cli bench`, seuils de `perf/GATES.md`), pas à l'œil.
- Chiffré de bout en bout : les clés de session ne quittent jamais les appareils, et ni le serveur ni le relais ne peuvent lire l'image, le son ou les entrées (`docs/SECURITY.md`). Un changement qui affaiblirait cela se signale avant tout.
- Rien de personnel dans le dépôt : les noms de machines, adresses, empreintes et comptes tirés des journaux de Victor ne vont ni dans le code, ni dans les essais, ni dans la documentation, ni dans les commits. Les exemples utilisent des valeurs inventées.

## Dépendances

- La dernière version stable de Rust, de chaque bibliothèque et de FFmpeg (D221). Un retard se justifie par écrit, dans le manifeste concerné et dans `DECISIONS.md`, jamais par commodité.
- Aucun programme tiers piloté de l'extérieur : ce dont le produit a besoin, il le fait lui-même ou par une bibliothèque qu'il charge.
- Une bibliothèque extérieure vit dans la seule brique qui en a besoin : `quinn` n'est nommé que dans `zyr-transport`, FFmpeg que dans `zyr-codec`.
- `vendor/ffmpeg` ne se retouche qu'en le recompilant par `packaging/ffmpeg/build.sh` (`vendor/ffmpeg/README.md`). Le pilote de `vendor/ecran-virtuel`, signé par son auteur, ne se modifie pas.

## Avant chaque commit

- `cargo fmt --all`.
- `cargo clippy --workspace --all-targets -- -D warnings`, pour Linux et pour Windows (`--target x86_64-pc-windows-gnu`).
- Les essais, dosés selon ce qui bouge : ceux de la brique touchée au minimum ; la suite entière (`cargo test --workspace --all-targets`) dès que plusieurs briques, le moteur, le réseau, le service ou un dialogue changent.
- Relire le diff complet : aucune modification sans rapport, aucun reste, aucun tiret long.
- La CI refait tout à chaque poussée (format, analyse statique Linux et Windows, essais Linux et Windows, `zyr-cli doctor`, installateur) : elle reste verte.

## Documenter

- Une décision d'architecture ou un changement visible s'écrit dans `docs/DECISIONS.md`, au plus tard dans le dernier commit de l'étape : une entrée `## Dnnn. Titre (date, pendant <jalon>)`, ajoutée juste avant « Décisions ouvertes », qui dit ce qui est fait, pourquoi, et ce qui se voit.
- L'historique ne se réécrit jamais : une décision qui en change une autre la cite.
- `docs/ARCHITECTURE.md` suit dans le même commit quand une brique, un rôle, un processus ou une interface change ; le `README.md`, quand une convention ou l'état du projet change.
