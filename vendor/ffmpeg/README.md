# FFmpeg du moteur MZ

Ces fichiers ne sont pas écrits par ZyrDesk. Ce sont les bibliothèques
du projet **FFmpeg** 9.0.2, compilées pour Windows 64 bits avec
seulement ce dont le moteur MZ se sert, par le script
`packaging/ffmpeg/build.sh`. L'ensemble est sous licence GPL (voir plus
bas et le dossier `licenses/`).

## Pourquoi elles sont là

Le moteur doit compresser l'image de l'écran et le son avant de les
envoyer, puis décompresser ce qu'il reçoit. FFmpeg sait le faire avec
les puces vidéo des trois fabricants de cartes graphiques (NVIDIA, AMD,
Intel), avec celle que Windows propose de lui-même, et en logiciel
quand aucune n'est disponible, le tout derrière une seule façon de
faire. Écrire chacun de ces chemins à la main serait des mois de
travail.

Les versions toutes faites qu'on trouve en ligne contiennent des
centaines de formats dont le moteur n'a pas l'usage, pèsent plusieurs
dizaines de Mo et demandent parfois d'autres fichiers à livrer avec.
Celle-ci ne garde que ce qui sert, tient en moins de 10 Mo et ne
demande rien d'autre que Windows.

## Ce que ZyrDesk en fait

Le moteur ouvre ces bibliothèques lui-même quand il démarre. La
compilation de ZyrDesk (`cargo build --release`) n'en a pas besoin et
ne demande ni FFmpeg ni aucun outil de plus sur la machine.

Ce qu'elles savent faire, et rien d'autre :

- **Décompresser** : H.264, HEVC et AV1 pour l'image, Opus pour le son.
  H.264, HEVC et AV1 peuvent passer par la carte graphique (Direct3D 11).
  Pour AV1, c'est même la seule voie : FFmpeg n'a pas de décodeur AV1
  logiciel à lui.
- **Compresser l'image** : x264 en logiciel (H.264, 8 bits), et par la
  carte graphique NVENC (NVIDIA), AMF (AMD) et QSV (Intel) en H.264,
  HEVC et AV1, plus Media Foundation (Windows) en H.264 et HEVC.
- **Compresser le son** : Opus.
- **Convertir le son** d'un format à un autre (bibliothèque
  swresample).

Rien pour lire ou écrire des fichiers, rien pour le réseau, pas de
filtres ni de mise à l'échelle d'image.

Les pilotes des cartes graphiques ne sont pas inclus : ils sont
cherchés sur la machine au moment où on s'en sert, et leur absence ne
gêne rien d'autre. Il s'agit de `nvEncodeAPI64.dll` et `nvcuda.dll`
(NVIDIA), `amfrt64.dll` (AMD), du moteur Intel que trouve le
répartiteur libvpl, et de `mfplat.dll`, `d3d11.dll` et `dxgi.dll`
(Windows).

**NVENC demande un pilote NVIDIA 570 ou plus récent.** Les en-têtes
NVIDIA sont volontairement ceux de la version 13.0 du kit NVIDIA et non
de la 13.1, la plus récente, qui exigerait un pilote 610 : les GeForce
10 (Pascal), dont les pilotes s'arrêtent à la branche R580, perdraient
leur encodeur matériel, tout comme les machines pas encore passées au
pilote 610, et la 13.1 n'apporte rien dont le moteur se sert (voir
D221 dans `docs/DECISIONS.md`). Avec un pilote plus ancien que 570,
NVENC refuse de démarrer et le moteur doit passer par un autre
encodeur.

## Fichiers

| Fichier | Taille | Empreinte SHA-256 |
|---|---|---|
| `avcodec-63.dll` | 7 273 984 octets | `dc77a3fe6e89911728f79aa8b39d894b1df6efdf1e28f95a10607f6b04439bce` |
| `avutil-61.dll` | 2 363 392 octets | `5a4937b2d3d56700b9e27cb89aa803675a7e50d7ddaeb878eed97a9c035db70b` |
| `swresample-7.dll` | 139 264 octets | `06245d16116ecf1fd0a9f65ef03ff1c0c366eaa5765326d07ed16af1f86200c4` |

Chaque compilation inscrit sa date et son dossier de travail dans les
fichiers : une nouvelle compilation donne donc d'autres empreintes, même
sans rien changer.

Ce que chaque DLL demande à Windows au chargement, en plus des deux
autres :

| Fichier | Bibliothèques Windows |
|---|---|
| `avcodec-63.dll` | `ADVAPI32.dll`, `KERNEL32.dll`, `msvcrt.dll`, `ole32.dll` |
| `avutil-61.dll` | `ADVAPI32.dll`, `bcrypt.dll`, `KERNEL32.dll`, `msvcrt.dll`, `ole32.dll` |
| `swresample-7.dll` | `KERNEL32.dll`, `msvcrt.dll` |

Toutes font partie de Windows. `avcodec-63.dll` a besoin des deux
autres DLL à côté d'elle, `swresample-7.dll` de `avutil-61.dll`. Le
script refuse de terminer si une autre dépendance apparaît.

## Provenance exacte

| Source | Version | Référence |
|---|---|---|
| FFmpeg | 9.0.2 | `ffmpeg-9.0.2.tar.xz`, SHA-256 `8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e`, signature du projet vérifiée (clé `FCF9 86EA 15E6 E293 A564 4F10 B432 2F04 D676 58D8`) |
| x264 | branche `stable` (version 165) | commit `b35605ace3ddf7c1a5d67a2eb553f034aef41d55` de https://code.videolan.org/videolan/x264 |
| Opus | 1.6.1 | `opus-1.6.1.tar.gz`, SHA-256 `6ffcb593207be92584df15b32466ed64bbec99109f007c82205f0194572411a1` (identique à celle publiée par Xiph) |
| nv-codec-headers (NVIDIA) | n13.0.19.1 | commit `88fee5c37318c991a8762d423530f91681e32e3a` de https://github.com/FFmpeg/nv-codec-headers |
| En-têtes AMF (AMD) | 1.5.2 | `AMF-headers-v1.5.2.tar.gz`, SHA-256 `d3c12eb324edf05e214608b6a395a51dd95770ed9d45520185d6c3a206811c99` ; licence prise au commit `eadd00804d5f7e5cd8c85d540073198312870776` de https://github.com/GPUOpen-LibrariesAndSDKs/AMF |
| libvpl (Intel) | 2.17.0 | commit `d77f9195cf495b937631607333288fd917ae8939` de https://github.com/intel/libvpl, répartiteur seul |

Chacune était la dernière version stable au moment de la compilation,
sauf les en-têtes NVIDIA, restés en 13.0 pour la raison dite plus haut.
Les adresses exactes et les empreintes sont écrites en tête du script,
qui refuse toute source qui ne correspond pas.

Compilé sous Ubuntu 24.04 avec la chaîne mingw-w64 d'Ubuntu (GCC 13.2,
mingw-w64 11.0.1, fils d'exécution natifs de Windows), nasm 2.16.01 et
CMake 3.28.3. x264, Opus et libvpl sont intégrés aux DLL, de même que
les bibliothèques d'exécution du compilateur.

Options passées à FFmpeg, telles que les DLL les rapportent
(`avcodec_configuration()`) :

```
--pkg-config=pkg-config --pkg-config-flags=--static --enable-gpl
--enable-shared --disable-static --disable-programs --disable-doc
--disable-debug --disable-autodetect --disable-everything
--disable-avformat --disable-avdevice --disable-avfilter
--disable-swscale --disable-network --enable-libx264 --enable-libopus
--enable-decoder='h264,hevc,av1,opus' --enable-parser='h264,hevc,av1,opus'
--enable-encoder='libx264,libopus' --target-os=mingw32 --arch=x86_64
--cross-prefix=x86_64-w64-mingw32-
--extra-cflags=-I/tmp/zyrdesk-ffmpeg.gtN1RG/deps/include
--extra-ldflags=-static --extra-libs=-lstdc++ --enable-w32threads
--enable-d3d11va --enable-ffnvcodec --enable-nvenc --enable-amf
--enable-libvpl --enable-mediafoundation
--enable-encoder='h264_nvenc,hevc_nvenc,av1_nvenc,h264_amf,hevc_amf,av1_amf,h264_qsv,hevc_qsv,av1_qsv,h264_mf,hevc_mf'
--enable-hwaccel='h264_d3d11va,h264_d3d11va2,hevc_d3d11va,hevc_d3d11va2,av1_d3d11va,av1_d3d11va2'
```

## Vérification

Avant d'être déposées ici, les DLL ont été chargées sous Wine par un
petit programme de test qui les ouvre à l'exécution, comme le moteur :
tous les encodeurs et décodeurs ci-dessus sont présents, 30 images
compressées par x264 ressortent toutes du décodeur H.264, et 20 ms de
son stéréo passent par Opus dans les deux sens. Le même test passe sur
la version Linux (voir plus bas).

## Licence

FFmpeg est sous LGPL, mais x264 est sous GPL : une fois x264 dedans,
l'ensemble est sous **GPL version 2 ou ultérieure**, comme FFmpeg le
rapporte lui-même (`avcodec_license()`). C'est compatible avec la GPL
version 3 de ZyrDesk. Les autres morceaux sont sous des licences plus
souples (BSD pour Opus, MIT pour libvpl, AMF et les en-têtes NVIDIA),
qui demandent seulement de garder leur texte avec les fichiers.

Le dossier `licenses/` contient ces textes :

| Fichier | Pour |
|---|---|
| `FFmpeg-LICENSE.md`, `FFmpeg-COPYING.GPLv2` | FFmpeg |
| `x264-COPYING` | x264 (GPL version 2) |
| `opus-COPYING` | Opus (BSD) |
| `libvpl-LICENSE` | libvpl (MIT) |
| `AMF-LICENSE.txt` | en-têtes AMF (MIT) |
| `nv-codec-headers-LICENSE.txt` | en-têtes NVIDIA (MIT, texte repris de chaque fichier) |

Ils doivent accompagner les DLL partout où elles sont livrées.

**Obligation de fournir le code source.** La GPL demande que quiconque
reçoit ces DLL puisse obtenir le code source exact qui les a produites,
avec de quoi le recompiler. Ce code, ce sont les sources listées
ci-dessus, dans ces versions exactes, plus `packaging/ffmpeg/build.sh`.
Chaque publication de ZyrDesk qui contient ces DLL doit donc rendre ces
sources disponibles, et le rester aussi longtemps que la publication
est distribuée. Le plus sûr est de joindre les archives de ces sources
à la publication elle-même, plutôt que de compter sur les sites
d'origine, qui peuvent changer ou disparaître.

## Recompiler ou mettre à jour

La compilation se fait sous Linux (Ubuntu 24.04, ou Ubuntu dans WSL sous
Windows) :

1. Installer les outils :
   `sudo apt install mingw-w64 nasm cmake pkg-config make gcc git curl xz-utils`
2. Depuis la racine du dépôt :
   `bash packaging/ffmpeg/build.sh windows vendor/ffmpeg`
   Le script télécharge les sources, vérifie leurs empreintes, compile,
   contrôle les dépendances des DLL, puis remplace les DLL et le dossier
   `licenses/`. Ce fichier-ci n'est pas touché.
3. Mettre à jour dans ce fichier les tailles, les empreintes et les
   options de compilation.

Pour passer à une nouvelle version d'une source, on change son numéro
et son empreinte (ou son commit) en tête du script, en les prenant sur
le site officiel du projet, puis on recompile et on met à jour ce
fichier. Si FFmpeg change de version majeure, le nom des DLL change
aussi (`avcodec-64.dll` par exemple) et le moteur doit suivre.

`bash packaging/ffmpeg/build.sh linux <dossier>` construit la même
chose pour Linux, avec les mêmes sources, pour les tests du moteur. Ce
résultat-là n'est pas gardé dans le dépôt.
