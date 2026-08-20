# Écran virtuel : faire pousser un écran sur l'ordinateur hôte

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

**Pendant une session.** Le moteur est configuré avec
`output_name = <l'écran virtuel>` et
`dd_configuration_option = ensure_only_display` : il allume l'écran
virtuel et **éteint les autres** le temps de la session.

Éteindre les autres n'est pas une brutalité, c'est le seul comportement
correct. Un écran devant lequel personne n'est assis est un bureau vide :
la barre des tâches, les fenêtres et les icônes sont sur l'écran de la
personne d'en face. Une session à qui on montrerait l'écran vide
montrerait une copie blanche de l'ordinateur. Tout éteindre sauf lui
déplace le bureau entier dessus. L'écran de la personne assise devant
s'éteint pendant ce temps, et `dd_config_revert_on_disconnect = enabled`
remet tout en place à la fin.

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
