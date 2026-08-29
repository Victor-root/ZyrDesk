# Écran virtuel : faire pousser un écran sur l'ordinateur hôte

## Où il sert, et où il ne sert plus

**Sur une machine qui n'a aucun écran branché, et là seulement**
([D91](DECISIONS.md)). Un serveur dans un placard, une tour dont on a
débranché le moniteur : il n'y a rien à filmer, et l'écran qu'on fait
pousser est la seule chose qui existe.

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
sait afficher, le moteur hôte capture ce qu'il a et l'agrandit avant
d'encoder (`engines/sunshine/src/video.cpp`). Le flux porte bien le
nombre de pixels demandé, mais pas un détail de plus : c'est du 1080p
étiré, encodé au prix du 4K. À l'arrivée, l'image remplit l'écran et
elle est floue.

Symétriquement, demander moins que l'écran du client coûte deux
agrandissements successifs : l'hôte rétrécit son bureau à la taille
demandée, et le client réétire l'image à la taille de son écran. Aucun
des deux ne rend un pixel qui n'a jamais été envoyé.

Le tableau, avec un client 4K et un portable hôte en 1080p :

| Hôte | Ce qui arrive | Résultat |
|---|---|---|
| Sans écran virtuel, on demande 1080p | 1080p réel, étiré ×2 à l'arrivée | Flou |
| Sans écran virtuel, on demande 4K | 1080p agrandi côté hôte, encodé en 4K | Flou **et** quatre fois plus lourd |
| Avec écran virtuel en 4K | 4K réellement dessiné | Net, un pixel envoyé pour un pixel affiché |

## Les deux moitiés

Il en faut deux, et l'une sans l'autre ne sert à rien.

1. **Le client demande la bonne taille.** Une qualité n'est plus une
   taille absolue mais un plafond ; la taille demandée est celle de
   l'écran sur lequel l'image va être posée, mesurée en pixels réels
   (`crates/zyr-proto/src/session.rs`, `crates/zyr-ui/src/picture.rs`).
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

Sunshine, lui, n'a aucun écran virtuel à lui. Il sait en revanche
piloter ceux qui existent, et c'est ce qui est utilisé
(`docs/engines/STRATEGY.md`).

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

**Au premier démarrage du moteur.** Le moteur nomme les écrans par un
condensé de leur identité et de l'endroit où ils sont branchés. Le
recalculer ici et se tromper d'un octet donnerait un nom qui ne désigne
rien, sur lequel le moteur retomberait silencieusement sur l'écran
principal. Il n'est donc pas recalculé : il est **lu**. Le moteur écrit sa
liste complète d'écrans dans son propre journal à chaque démarrage. Le
service la lit, y reconnaît l'écran virtuel au nom qu'il publie, l'écrit
dans `data/screen/engine-screen.txt`, et redémarre le moteur une fois,
parce que le moteur ne lit ce réglage qu'à son démarrage. Les démarrages
suivants sont renseignés d'avance.

**Pendant une session, sur une machine sans écran.** Le moteur est
configuré avec `output_name = <l'écran virtuel>`, et rien d'autre : c'est
le seul écran de cette machine, donc il n'y a rien à éteindre et rien à
déplacer. Le service le réveille à la taille demandée quand la session
arrive, et le rendort quand plus personne ne regarde.

Le nom est écrit au démarrage du moteur, qui est le seul moment où il lit
quel écran filmer, et il n'est écrit **que** si la machine n'a aucun
écran à elle. Une machine qui en a un n'entend jamais parler de l'écran
virtuel : le moteur filme l'écran principal, celui où est le bureau.

**Ce que le moteur ne fait plus du tout.** Cinq lignes de sa configuration
lui demandaient d'arranger les écrans : poser la taille, éteindre le
reste, tout remettre à la fin. Elles sont parties. Le moteur remet un
arrangement relevé à son propre démarrage, abandonne dès que quelque
chose d'autre a bougé un écran entre-temps, et rallume alors tout ce
qu'il trouve. C'est le produit qui relève le bureau et qui le remet
maintenant ([D91](DECISIONS.md), `crates/zyr-screen/src/arrangement.rs`).

## L'agrandissement

Une taille toute seule ne décrit pas un écran. Le même panneau à la même
définition écrit un texte deux fois plus petit à cent pour cent qu'à deux
cents, et « la résolution du client » promet le bureau de la personne qui
regarde, agrandissement compris ([D90](DECISIONS.md)).

Le moteur hôte n'a rien pour ça : ses options d'écran couvrent la
définition, la fréquence, le HDR et l'arrangement, et s'arrêtent là. Le
chiffre est donc posé par ZyrDesk, sur l'écran que ZyrDesk a lui-même
fait pousser, juste après son réveil et avant que le moteur n'ouvre
dessus.

Windows ne publie qu'un seul chemin pour l'écrire, un message privé sur
l'appel qui lit la configuration d'affichage, et `magnify.rs` est le seul
fichier qui le connaisse. Il parle en pas le long de la liste que Windows
offre plutôt qu'en pour cent, et compte ces pas depuis celui que Windows
recommande pour l'écran en question.

Deux choses en découlent, et elles valent d'être sues avant d'y toucher.
La course tourne dans la session qui tient l'écran et jamais dans le
service : tout ce que Windows dit de l'arrangement des écrans est répondu
pour le poste de travail de celui qui demande, et celui d'un service n'a
aucun écran dessus. Et rien de tout cela ne fait échouer une session :
ce qui s'est passé part en une phrase dans le journal de l'hôte, et la
session continue.

Une session qui ne nomme aucun agrandissement, parce qu'elle n'a pas su
mesurer son écran ou parce qu'une taille a été choisie à la main, reçoit
celui que Windows recommande pour cette taille-là. C'est posé à chaque
réveil : cet écran n'appartient qu'aux sessions, et le laisser où la
session précédente l'a mis serait une machine qui se souvient du bureau
d'une autre.

## La frontière dans le code

Elle est tenue au même endroit que celle des moteurs, et pour la même
raison : un pilote qu'il faudra remplacer un jour doit coûter un fichier.

```
crates/zyr-screen/
  src/lib.rs        ce que le produit demande : pose, retire, offre une taille
  src/driver.rs     LA frontière : un trait, et rien de propre à un pilote
  src/mtt.rs        le seul pilote livré, et tout ce qui lui est propre
  src/place.rs      la mécanique Windows, la même pour n'importe quel pilote
  src/vouching.rs   désigner l'éditeur comme attendu, idem
  src/magnify.rs    la taille à laquelle Windows écrit sur cet écran
  src/engine.rs     lire la liste d'écrans du moteur
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

## Ce qui reste ouvert

Une session dont la taille demandée n'est ni celle d'un écran du
commerce, ni l'une des tailles offertes d'avance, n'a aucun moyen de le
faire savoir à l'hôte avant de s'ouvrir. Cela arrive quand l'écran du
client est rétréci pour tenir sous le plafond d'une qualité : un écran
très large en qualité « Équilibré » donne par exemple 1920x802, qui n'est
la taille d'aucun écran. Le moteur garde alors la taille courante et
agrandit, ce qui redonne une image un peu molle sans rien casser.

La brique est prête pour le corriger (`zyr_screen::offer`), il manque à
la session le moyen de dire sa taille à l'hôte avant de s'ouvrir. Le
canal existe : celui où les deux ZyrDesk se parlent déjà
(`crates/zyr-tunnel/src/aside.rs`).
