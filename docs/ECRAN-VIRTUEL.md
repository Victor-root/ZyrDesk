# Écran virtuel : faire pousser un écran sur l'ordinateur hôte

## Où il sert, et où il ne sert plus

**Sur une machine qui n'a aucun écran branché** ([D91](DECISIONS.md)).
Un serveur dans un placard, une tour dont on a débranché le moniteur :
il n'y a rien à filmer, et l'écran qu'on fait pousser est la seule chose
qui existe.

**Et, en dernier recours, sur une machine dont l'écran refuse la taille
demandée** ([D175](DECISIONS.md)). Une dalle 1920x1080 ne dessine pas
un bureau de 1920x1200. Le bureau passe alors sur l'écran poussé le
temps de la session, les écrans de la machine sont éteints, et tout est
remis comme avant à la fin.

**Ou sur une machine dont l'écran principal est allumé aux yeux de
Windows et ne donne aucune image** ([D245](DECISIONS.md)). Un écran relié
par un commutateur (KVM) tourné vers un autre ordinateur, un moniteur
éteint dont le câble reste branché : Windows le liste toujours, et la
capture ne dit pas qu'elle échoue, elle ne rend rien. Le moteur le voit
et le dit au service, le bureau passe sur l'écran poussé comme ci-dessus,
après quelques secondes de noir.

Partout ailleurs, une session règle la taille de l'écran principal de
l'hôte et ne touche à rien d'autre. Ni écran éteint, ni écran déplacé, ni
écran créé. Ce document décrit donc la moitié de secours du produit, pas
son chemin ordinaire.

**Pourquoi ce recul.** Faire pousser un écran ne suffit pas : pour que le
bureau se déplace dessus, il faut éteindre tous les autres, sans quoi la
session ne montre qu'un fond vide. C'est cette moitié-là qui a été jugée
inacceptable, et à juste titre : trois écrans 4K éteints pour qu'un seul
porte une image de portable, et une télé rallumée à chaque démarrage. On
avait réglé un problème de netteté en en créant un plus gros. Ce que la
netteté coûte maintenant, c'est la taille de l'écran principal de l'hôte
pendant la session, et il est remis après.

## Le problème

Un ordinateur ne peut envoyer que ce qu'il dessine.

Quand la session demande une image plus grande que tout ce que l'hôte
sait afficher, le moteur hôte envoie la taille de son écran et pas un
pixel de plus (`crates/zyr-host/src/picture.rs`) : agrandir avant
d'encoder coûterait du débit sans ajouter un seul détail. À l'arrivée,
le client étire l'image pour remplir l'écran, et elle est floue.

Symétriquement, demander moins que l'écran du client coûte deux
agrandissements successifs : l'hôte rétrécit son image à la taille
demandée, et le client la réétire à la taille de son écran. Aucun des
deux ne rend un pixel qui n'a jamais été envoyé.

Le tableau, avec un client 4K et un portable hôte en 1080p :

| Hôte | Ce qui arrive | Résultat |
|---|---|---|
| Sans écran virtuel, on demande 1080p | 1080p réel, étiré ×2 à l'arrivée | Flou |
| Sans écran virtuel, on demande 4K | Les 1080p de l'hôte, rien de plus, étirés ×2 à l'arrivée | Flou aussi |
| Avec écran virtuel en 4K | 4K réellement dessiné | Net, un pixel envoyé pour un pixel affiché |

## Les deux moitiés

Il en faut deux, et l'une sans l'autre ne sert à rien.

1. **Le client demande la bonne taille.** Une qualité n'est plus une
   taille absolue mais un plafond ; la taille demandée est celle de
   l'écran sur lequel l'image va être posée, mesurée en pixels réels
   (`crates/zyr-proto/src/session.rs`, `crates/zyr-ui/src/session/picture.rs`).
2. **L'hôte sait la fournir.** Windows permet à un pilote de déclarer un
   écran vers lequel aucun câble ne va. Le bureau est alors réellement
   dessiné à cette taille, et le moteur le capture réellement
   (`crates/zyr-screen/`).

## Le pilote, et pourquoi celui-là

Windows refuse de charger un pilote que personne n'a cautionné. Se faire
cautionner soi-même coûte un certificat à plusieurs centaines d'euros
par an, plus un compte chez Microsoft. **ZyrDesk ne paie ni l'un ni
l'autre**, ce qui ne laisse qu'une porte : un pilote que quelqu'un
d'autre publie déjà signé, sous une licence qui autorise à le
redistribuer.

| Candidat | Verdict |
|---|---|
| **Virtual Display Driver** (MIT, signé par la fondation SignPath) | **Retenu.** Sa signature remonte à une autorité que Windows connaît déjà, donc rien n'est ajouté aux racines de confiance de la machine. Sa licence autorise la redistribution |
| SudoVDA (MIT/CC0) | Écarté : auto-signé, exige d'injecter un certificat racine dans la machine, ce qui reviendrait à faire confiance à tout ce que ce certificat signera un jour |
| Le pilote du concurrent | Écarté : propriétaire, redistribution interdite |
| En écrire un | Écarté : c'est le certificat qui coûte, pas le code |

Ses fichiers sont dans `vendor/ecran-virtuel/`, recopiés tels quels avec
leur licence. Ils sont signés **comme un tout** : en modifier, renommer
ou recompresser un seul fait perdre la signature de l'ensemble.

Le moteur, lui, ne sait rien de ce pilote : l'écran poussé est pour lui
un écran comme un autre, qu'il filme quand le service le lui désigne.

## Ce qui se passe, dans l'ordre

**À l'installation.** Le produit cherche les fichiers du pilote à côté de
son programme : depuis le dépôt cela tombe sur `vendor/ecran-virtuel/`,
installé, sur le même dossier posé à côté de l'exécutable. Rien à
recopier dans un cas comme dans l'autre. Le service, au moment où il
s'enregistre, les met en place. Ce moment-là et pas un autre : les droits
administrateur y sont déjà en main, et personne n'est en session.

Le service, dans l'ordre : écrit les tailles que l'écran devra offrir,
dit au pilote de garder ses papiers dans `data/screen/` plutôt que dans
un dossier à lui à la racine du disque, désigne l'éditeur du pilote comme
attendu par cette machine, dépose le paquet dans la réserve de pilotes de
Windows, déclare l'appareil, et installe le pilote dessus.

L'étape « éditeur attendu » mérite un mot, parce qu'elle ressemble à ce
qui a fait écarter les autres candidats et n'en est pas : Windows fait
déjà confiance à ce pilote. Ce qu'il ignore, c'est si cette machine
**s'attend** à en recevoir un de cet éditeur. Ne le sachant pas, il pose
la question dans une fenêtre. Personne ne peut y répondre : l'installation
tourne depuis un service, sur un bureau où il n'y a personne. Désigner
l'éditeur y répond d'avance, et n'accorde rien de plus : un pilote non
signé, ou signé par quelqu'un d'autre, reste refusé exactement comme
avant. C'est repris au retrait du produit.

**Rien de tout cela ne fait échouer l'installation.** Un ordinateur sans
écran virtuel ouvre toujours des sessions et affiche toujours une image ;
ce qu'il perd, c'est de pouvoir servir correctement un écran plus grand
que le sien. Chaque étape est écrite dans le journal du service.

**Quand le moteur d'une session démarre.** Le service lance un moteur
par session. Avant la première image, ce moteur lui dit quels écrans il
peut filmer, chacun sous un identifiant qui survit à un redémarrage (le
chemin du moniteur que Windows donne, celui sous lequel le produit note
déjà les écrans) et sous le nom par lequel l'écran se présente. Le
service reconnaît l'écran virtuel à ce nom, celui que le pilote publie,
et c'est lui qui dit au moteur quel écran filmer. Rien n'est recalculé,
rien n'est écrit sur le disque, et changer d'écran filmé se fait en
pleine session, sans rien relancer.

**Pendant une session, sur une machine sans écran.** Le service réveille
l'écran poussé à la taille demandée quand la session arrive, puis dit au
moteur de le filmer dès qu'il le voit : c'est le seul écran de cette
machine, il n'y a rien à éteindre et rien à déplacer. Il le rendort
quand plus personne ne regarde, même sans aucun bureau à rendre
([D246](DECISIONS.md)).

« Sans écran » veut dire qu'aucun écran de la machine n'est allumé aux
yeux de Windows, un écran débranché par exemple. Le service le sait par
la note que la session de l'écran écrit à l'ouverture de chaque session
(`showing.txt`), et qu'elle écrit aussi quand elle ne trouve rien
d'allumé : sans cela, la note de la dernière session qui avait un écran
continuerait à dire que la machine en a un ([D244](DECISIONS.md)). Et
c'est vrai quelle que soit la résolution demandée : en « Résolution de
l'hôte », qui ne demande aucune taille, l'écran naît en 1920x1080, la
taille commune, mais seulement si la note dit que rien n'est allumé. Une
note absente dit seulement que personne n'a encore regardé, et ne fait
rien pousser sous quelqu'un qui pourrait avoir un écran.

**Pendant une session, sur une machine dont l'écran refuse la taille.**
Trois pas, chacun défait si le suivant ne passe pas : le service réveille
l'écran poussé à la taille demandée, la session qui tient l'écran y
déplace le bureau, puis le moteur est prié de filmer celui-là. À la fin,
le bureau revient d'où il vient et l'écran poussé se rendort.

**Pendant une session, sur une machine dont l'écran allumé ne donne
rien.** Aucune note ne peut le dire à l'avance : pour Windows, l'écran est
allumé. C'est le moteur qui le voit. Un écran qui marche donne son image
à l'instant où on le filme ; un écran qui n'a rendu ni image ni mouvement
de curseur au bout d'une seconde et demie est dit au service, une fois
par écran visé (`Silent`). Le service fait alors ce qu'il fait pour un
écran qui refuse la taille : il note le bureau si personne ne l'avait
fait, puis réveille l'écran poussé (à la taille demandée, ou à celle que
le moteur filmait), y déplace le bureau et prie le moteur de le filmer.
Le bureau est noté avant le réveil et pas après : réveillé, l'écran poussé
fait partie du bureau, et la fin de la session le remettrait allumé
([D246](DECISIONS.md)). Une seule fois par session, et seulement quand
l'écran filmé est l'écran principal : un écran choisi à la main dans le
menu n'est pas touché.

Rien n'en est retenu d'une session à l'autre. Un commutateur peut revenir
vers la machine à tout moment, et la session suivante regarde de nouveau.
Pour que l'écran poussé, levé en pleine session, soit proposé au service
aussitôt, le moteur demande aussi toutes les demi-secondes, pendant que
les images partent, si la liste des écrans a changé.

**À la fin.** Le service ne laisse rien de prêté. Quand plus aucune
session n'est ouverte, il remet le bureau comme il a été noté, puis
rendort l'écran poussé, dans cet ordre, et recommence tant que l'un ou
l'autre ne passe pas : toutes les deux secondes d'abord, puis de moins en
moins souvent jusqu'à une fois par minute. Un écran absent, un moniteur
commuté sur un autre ordinateur, refuse aussi longtemps qu'il est parti,
et réessayer sans cesse pendant des heures, c'est demander à Windows de
réorganiser le bureau trente fois par minute. Ni l'un ni l'autre n'est
tenu pour fait tant qu'il ne l'est pas : un bureau revenu dont l'écran
poussé n'a pas voulu dormir, c'est un deuxième écran que personne n'a
demandé, à côté du premier quand le moniteur revient.

Ailleurs, une machine qui a un écran à elle n'entend jamais parler de
l'écran virtuel : le moteur filme l'écran principal, celui où est le
bureau, ou celui qu'on a choisi dans le menu de la session. L'écran
poussé n'y est jamais proposé, puisque personne devant la machine ne le
voit.

**Le moteur n'arrange jamais les écrans.** Il filme, et rien d'autre.
C'est le produit qui relève le bureau, le règle pour la session et le
remet ensuite ([D91](DECISIONS.md), `crates/zyr-screen/src/arrangement.rs`).

## L'agrandissement

Une taille toute seule ne décrit pas un écran. Le même panneau à la même
définition écrit un texte deux fois plus petit à cent pour cent qu'à deux
cents, et « la résolution du client » promet le bureau de la personne qui
regarde, agrandissement compris ([D90](DECISIONS.md)).

Le chiffre est posé par ZyrDesk sur l'écran principal de l'hôte, juste
après la taille et seulement si cette taille est réellement arrivée.

Windows ne publie qu'un seul chemin pour l'écrire, un message privé sur
l'appel qui lit la configuration d'affichage, et `magnify.rs` est le seul
fichier qui le connaisse. Il ne parle pas en pour cent mais en pas le
long d'une liste fixe, comptés depuis celui que Windows recommande pour
cet écran **à la taille qu'il a en ce moment**. Changer la taille du
bureau déplace donc la recommandation sans toucher au pas, et le même pas
ne veut alors plus dire le même pourcentage ([D92](DECISIONS.md)).

Trois choses en découlent, et elles valent d'être sues avant d'y toucher.
Un écran laissé sur un pas qui n'est plus dans la liste ne répond plus
rien du tout, et c'est le seul cas où on lui écrit sans le lire. Ce que
chaque écran dessine est retenu d'une session à l'autre dans
`data/screen/screen-scales.txt`, pour qu'un écran devenu muet reprenne
l'agrandissement de son propriétaire et non celui que Windows
recommande. Et la course tourne dans la session qui tient l'écran et
jamais dans le service : tout ce que Windows dit de l'arrangement des
écrans est répondu pour le poste de travail de celui qui demande, et
celui d'un service n'a aucun écran dessus.

Rien de tout cela ne fait échouer une session : ce qui s'est passé part
en une phrase dans le journal de l'hôte, et la session continue.

Une session qui ne nomme aucun agrandissement, parce qu'elle n'a pas su
mesurer son écran ou parce qu'une taille a été choisie à la main, reçoit
celui que Windows recommande pour cette taille-là : cette taille n'est
l'écran de personne, il n'y a donc rien à copier.

## Le curseur

Le pilote est prié de porter lui-même le curseur de son écran, comme le
fait une carte graphique pour un vrai écran. Laissé à son réglage par
défaut, il n'en porte aucun : Windows dessine alors le curseur dans
chaque image de l'écran poussé, la capture le reçoit déjà dans les
pixels, et une session en souris « Bureau » montre deux curseurs, le
sien et celui d'en face, un aller-retour en retard. Porté par le pilote,
il reste hors de l'image et la capture le reçoit à part
([D226](DECISIONS.md)).

## La frontière dans le code

Un pilote qu'il faudra remplacer un jour doit coûter un fichier.

```
crates/zyr-screen/
  src/lib.rs        ce que le produit demande : pose, retire, offre une taille
  src/driver.rs     LA frontière : un trait, et rien de propre à un pilote
  src/mtt.rs        le seul pilote livré, et tout ce qui lui est propre
  src/place.rs      la mécanique Windows, la même pour n'importe quel pilote
  src/vouching.rs   désigner l'éditeur comme attendu, idem
  src/magnify.rs    la taille à laquelle Windows écrit sur cet écran
  src/arrangement.rs  relever le bureau, et le remettre tel qu'il était
```

`mtt.rs` est le seul fichier qui connaisse l'identifiant matériel
(`Root\MttVDD`), les noms des fichiers du paquet, le nom sous lequel
l'écran se présente (`VDD by MTT`), la clé de registre où le pilote va
chercher son dossier, et la forme de son fichier de réglages. Le reste du
produit ne connaît qu'un dossier, un identifiant et une liste de tailles.

Changer de pilote = écrire un fichier à côté de `mtt.rs` et changer ce
que renvoie `zyr_screen::shipped()`.

## Les tailles offertes

L'écran offre d'avance les tailles sous lesquelles les écrans sont
réellement vendus (`ALWAYS_OFFERED` dans `crates/zyr-screen/src/lib.rs`).
Une taille déjà offerte ne coûte rien à une session ; une taille absente
coûte un redémarrage de l'écran, que la personne assise devant l'hôte
voit. La liste est donc longue exprès.

## Une taille qu'aucun écran n'a

Une session peut demander une taille qui n'est ni celle d'un écran du
commerce, ni l'une des tailles offertes d'avance, 1920x802 par exemple.
Elle la dit à l'hôte avant de s'ouvrir, par le canal où les deux ZyrDesk
se parlent (`crates/zyr-tunnel/src/aside.rs`), et le service la met en
tête de la liste de l'écran poussé avant de le réveiller
(`zyr_screen::wake_up`) : l'écran naît à cette taille-là. S'il est déjà
réveillé, il n'est arrêté puis relancé que si cette taille manque à sa
liste.
