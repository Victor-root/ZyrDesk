# Compiler les moteurs sur sa propre machine

Les moteurs sont normalement compilés par le workflow **Moteurs** sur GitHub, et récupérés en artefact. C'est pratique mais lent : la machine de GitHub a quatre coeurs, et le moteur hôte y prend une demi-heure.

Les compiler soi-même est possible, et sur une machine récente c'est plusieurs fois plus rapide. Le prix à payer est une installation à faire une fois.

Ce document décrit cette installation. Le workflow reste la référence : c'est lui qui dit si le projet compile ailleurs que sur une seule machine, et c'est de lui que sortent les moteurs qu'on distribue.

---

## Ce qu'il faut savoir avant

Les deux moteurs ne se compilent pas avec les mêmes outils, et c'est important :

| Moteur | Compilateur | Ce qui sert |
|---|---|---|
| Client (fenêtre vidéo) | Visual Studio | Visual Studio + Qt |
| Hôte (capture, encodage) | GCC sous MSYS2 | ni Visual Studio, ni Rust |

Rust ne sert à aucun des deux : il compile notre propre code (`zyr-cli`, `zyrdeskd`), ce qui est une étape séparée que vous faites déjà avec `cargo build --release`.

Compter environ **10 Go** pour l'ensemble, dont la moitié en sources téléchargées une seule fois.

Les deux moteurs peuvent être compilés sur la même machine, quel que soit le PC où ils serviront ensuite : le résultat est un dossier à copier.

---

## Préparation commune : les sources des moteurs

Les moteurs vivent dans des dépôts séparés, rattachés au projet. Ils ne sont pas téléchargés par un simple `git pull`. Une seule fois, dans le dossier du projet :

```
git submodule update --init --recursive
```

C'est long au premier passage (plusieurs Go, dont des bibliothèques déjà compilées). Les fois suivantes, après un `git pull`, il suffit de :

```
git submodule update --init --recursive
```

La même commande : elle ne retélécharge que ce qui a bougé.

---

## Moteur client

### Installation, une seule fois

**1. Visual Studio, avec le nécessaire pour le C++.**

Si Visual Studio est déjà installé, ouvrir **Visual Studio Installer**, cliquer **Modifier**, et vérifier que la case **Développement Desktop en C++** est cochée. Sans elle, le compilateur lui-même est absent.

**2. Python.**

Il ne sert à rien dans le projet : il n'est là que pour installer Qt à l'étape suivante, sans avoir à créer de compte Qt. Vérifier d'abord s'il est déjà là :

```
py --version
```

Si la commande répond un numéro de version, passer à l'étape suivante. Sinon :

```
winget install --id Python.Python.3.13 -e
```

Puis **fermer et rouvrir la fenêtre PowerShell**, sans quoi Windows ne connaît pas encore la commande.

**3. Qt 6.7.3.**

C'est la version sur laquelle le moteur est construit et testé en amont ; une autre version peut compiler mais ne sera pas ce qui est vérifié.

```
py -m pip install aqtinstall
py -m aqt install-qt windows desktop 6.7.3 win64_msvc2019_64 --outputdir C:\Qt
```

Qt se retrouve dans `C:\Qt\6.7.3\msvc2019_64`. Compter quelques minutes et environ 2 Go.

La forme `py -m` n'est pas une coquetterie : elle évite de dépendre de l'endroit où Python range ses commandes, qui n'est pas toujours connu de Windows.

### Compiler

Ouvrir **Developer PowerShell for VS 2022** depuis le menu Démarrer (pas PowerShell normal : c'est ce raccourci qui met le compilateur à disposition), puis :

```
$env:PATH = "C:\Qt\6.7.3\msvc2019_64\bin;$env:PATH"
cd D:\Temporaire\ZyrDesk
.\packaging\engines\build-client-engine.ps1
```

Le moteur se retrouve directement dans `data\engines\client\`, prêt à l'emploi. Rien à décompresser, rien à renommer.

---

## Moteur hôte

### Installation, une seule fois

**1. MSYS2.**

À télécharger sur [msys2.org](https://www.msys2.org/) et installer dans son dossier par défaut (`C:\msys64`). C'est un environnement de compilation à part, qui ne touche à rien d'autre sur la machine.

**2. Les outils du moteur.**

Ouvrir **MSYS2 UCRT64** depuis le menu Démarrer (attention : plusieurs raccourcis MSYS2 existent, il faut bien celui-là), puis :

```
pacman -Syu
```

Il demandera peut-être de fermer la fenêtre à la fin ; dans ce cas la rouvrir et relancer la même commande. Ensuite :

```
pacman -S --needed git mingw-w64-ucrt-x86_64-{boost,cmake,cppwinrt,curl-winssl,gcc,MinHook,miniupnpc,ninja,nlohmann-json,onevpl,openssl,opus,toolchain}
```

**3. Node.js.**

Le moteur hôte embarque une petite interface web construite par un outil JavaScript. Installer Node.js depuis [nodejs.org](https://nodejs.org/) (version LTS, installateur Windows classique).

### Compiler

Dans la fenêtre **MSYS2 UCRT64** :

```
export PATH="$PATH:/c/Program Files/nodejs"
cd /d/Temporaire/ZyrDesk
./packaging/engines/build-host-engine.sh
```

Noter la façon d'écrire les chemins dans cette fenêtre : `/d/Temporaire/...` au lieu de `D:\Temporaire\...`.

La ligne `export` est à retaper à chaque nouvelle fenêtre MSYS2.

Le moteur se retrouve dans `data\engines\host\`.

---

## Avant de compiler un moteur qui tourne déjà

Le script vide son dossier de sortie avant d'y écrire. Si le moteur est en train de tourner, Windows refuse de supprimer son fichier et la compilation échoue à la fin, après tout le travail.

Donc, avant de compiler le moteur hôte :

```
zyrdeskd stop
```

Et pour le moteur client, s'assurer qu'aucune session n'est ouverte.

---

## Ce que la compilation vérifie toute seule

Les deux scripts ne se contentent pas de compiler : ils relisent ce qu'ils ont produit et refusent de rendre un moteur incomplet. Une bibliothèque oubliée ne se voit sinon qu'au premier lancement, sur la machine de quelqu'un d'autre.

Un script qui se termine sur « Moteur ... assemblé dans ... » a donc produit quelque chose de complet.

---

## En cas de problème

**« pip n'est pas reconnu ».** Python n'est pas installé, ou pas connu de Windows : voir l'étape Python. Une fois installé, utiliser `py -m pip` plutôt que `pip` tout court.

**« qmake n'est pas reconnu ».** La ligne `$env:PATH` n'a pas été passée, ou Qt n'est pas là où elle le dit. Vérifier que `C:\Qt\6.7.3\msvc2019_64\bin\qmake.exe` existe.

**« cl n'est pas reconnu » ou une erreur de compilateur introuvable.** La fenêtre ouverte est PowerShell normal et non **Developer PowerShell for VS 2022**, ou la case C++ n'est pas cochée dans Visual Studio.

**Une erreur qui parle d'un fichier absent dans `third-party` ou `libs`.** Les sources des moteurs ne sont pas complètes : relancer `git submodule update --init --recursive`.

**« npm : command not found » dans MSYS2.** La ligne `export PATH` n'a pas été passée dans cette fenêtre, ou Node.js n'est pas installé.

**L'assemblage échoue sur un accès refusé.** Le moteur tourne encore : voir la section plus haut.
