# Pilote de l'écran virtuel

Ces fichiers ne sont pas écrits par ZyrDesk. Ils viennent du projet
**Virtual Display Driver**, sous licence MIT (voir `LICENSE` à côté), et
ils sont recopiés ici tels quels.

## Pourquoi ils sont là

Un ordinateur ne peut envoyer que ce qu'il dessine. Si l'écran d'en face
est plus grand que le sien, il agrandit ce qu'il a avant d'encoder : le
flux porte bien le nombre de pixels demandé, mais pas un détail de plus.
Un portable en 1920x1080 ne peut donc pas servir correctement un écran
4K, quel que soit le débit qu'on y met.

La seule chose qui corrige ça, c'est de donner à l'ordinateur hôte un
écran de la taille demandée. Windows permet à un pilote de déclarer un
écran vers lequel aucun câble ne va ; le bureau est alors réellement
dessiné à cette taille, et le moteur le capture réellement.

## Pourquoi celui-ci

Windows refuse de charger un pilote que personne n'a cautionné, et se
faire cautionner coûte plusieurs centaines d'euros par an. ZyrDesk ne
paie rien. Ce pilote-là est signé gratuitement par la fondation
SignPath, qui signe des projets libres, et sa signature remonte à une
autorité que Windows connaît déjà. **Rien n'est ajouté aux racines de
confiance de la machine**, ce qui écarte les autres pilotes du même
genre : ils sont auto-signés et exigent qu'on injecte un certificat
racine, ce qui n'est pas acceptable dans un produit.

Sa licence MIT autorise à le redistribuer, ce qui est ce qui permet de
le livrer avec ZyrDesk plutôt que de demander un téléchargement de plus.

## Ce que ZyrDesk en fait

Rien de ce dossier n'est modifié, renommé ni recompressé : les trois
fichiers sont signés **comme un tout**, et toucher à l'un d'eux fait
perdre la signature de l'ensemble. C'est le service qui les met en place
dans Windows puis les retire (voir `crates/zyr-screen/`).

Le produit les cherche à côté de son programme. Depuis le dépôt, cela
tombe sur ce dossier-ci ; installé, sur le même dossier posé à côté de
l'exécutable par l'installateur. Il n'y a donc rien à recopier pour
qu'une compilation faite sur sa machine les trouve.

Le seul écart : le fichier de la signature s'appelait `mttvdd.cat` dans
l'archive d'origine et il est renommé `MttVDD.cat` ici, qui est le nom
que le fichier de description attend. Windows ne distingue pas les
majuscules dans un nom de fichier, donc c'est le même fichier pour lui,
et son contenu n'a pas été touché.

Le fichier `vdd_settings.xml` de l'archive d'origine n'est pas repris :
ZyrDesk écrit le sien dans son propre dossier et dit au pilote d'aller
l'y chercher.

## Provenance exacte

| | |
|---|---|
| Projet | https://github.com/VirtualDrivers/Virtual-Display-Driver |
| Version | 25.7.23 |
| Archive | `VirtualDisplayDriver-x86.Driver.Only.zip` |
| Empreinte de l'archive | `e24210692b442b39af763536330ce78b423f19342b7a7792c26de3944e418b3a` |
| Licence | MIT |
| Signé par | SignPath Foundation |

Empreintes des fichiers repris, à vérifier après toute mise à jour :

```
08a0093fc9b2e32b287a6f8a77ca4de0a31830d29fc33d2b13a918dc859468f6  MttVDD.cat
c9ca837f57a98fbd43bc416a7f535a95843626e7759eaf85cf0cd7ce334dbb05  MttVDD.dll
550d211fe481e74dfe3f9d724ed78be48b3a9113405965d683d9373e8d672f5d  MttVDD.inf
```

## Mettre à jour le pilote

1. Récupérer l'archive « Driver Only » de la version voulue sur la page
   des publications du projet.
2. Remplacer les trois fichiers, en gardant le renommage de `.cat`.
3. Relire `crates/zyr-screen/src/mtt.rs` : l'identifiant matériel
   (`Root\MttVDD`), le nom sous lequel l'écran se présente
   (`VDD by MTT`, qui vient du bloc d'identité que le pilote publie) et
   la forme du fichier de réglages y sont écrits. Une version qui change
   l'un des trois demande de changer ce fichier avec.
4. Mettre à jour les empreintes ci-dessus.
