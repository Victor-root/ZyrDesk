# ZyrDesk

Bureau à distance open source très faible latence, pensé pour un usage réellement fluide : Blender, CAO, Unreal Engine, bureaux très animés, et éventuellement le jeu. Objectif d'expérience utilisateur : une fluidité et une simplicité comparables aux meilleures solutions commerciales du marché.

Une seule application à installer. Le même ZyrDesk sert d'hôte (le PC que l'on contrôle), de client (le PC depuis lequel on se connecte), ou des deux à la fois.

```text
Mes ordinateurs

● PC-BUREAU        [ Se connecter ]
● PC-PORTABLE      [ Se connecter ]
○ PC-ATELIER       Hors ligne
```

## Principes

- Performance d'abord : 1080p60 et 1440p60 réels, encodage et décodage matériels, frame pacing soigné, latence minimale. Jamais sacrifiés pour simplifier l'architecture.
- Moteurs éprouvés, invisibles : ZyrDesk s'appuie sur les projets officiels Sunshine (capture et encodage côté hôte) et Moonlight (décodage et affichage côté client), utilisés comme moteurs techniques internes. L'utilisateur ne les voit jamais : aucune interface, aucun logo, aucune configuration manuelle de ces moteurs. Ils sont crédités dans « À propos » et la documentation.
- Connexion directe prioritaire : le flux vidéo va d'un PC à l'autre sans intermédiaire chaque fois que possible. Un petit serveur (broker) sert uniquement à la mise en relation : comptes, liste des appareils, présence, échange des informations de connexion. En dernier recours, un relais transporte des paquets chiffrés qu'il ne peut pas lire.
- Chiffré de bout en bout : les clés de session ne quittent jamais les appareils. Ni le broker ni le relais ne peuvent déchiffrer la vidéo, l'audio ou les entrées clavier/souris.
- Un vrai produit : interface moderne, premium, minimaliste. Windows 11 d'abord, scénario NVIDIA vers NVIDIA en premier.
- Maintenable dans la durée : les moteurs upstream restent quasi intacts (objectif : zéro modification de Sunshine, six micro-modifications maximum de Moonlight), pour pouvoir suivre leurs nouvelles versions facilement.

## État du projet

Jalon en cours : **M0, fondations**. Le dépôt est opérationnel : ossature Rust, moteurs upstream épinglés, outil de diagnostic, squelette d'installateur Windows, intégration continue. Aucune session distante n'est encore possible : c'est l'objet du jalon M1. La feuille de route complète est dans [docs/ROADMAP.md](docs/ROADMAP.md).

## Construire

Prérequis : Rust stable. Les moteurs sont des submodules et ne sont pas nécessaires pour compiler la partie ZyrDesk.

```bash
git clone https://github.com/Victor-root/ZyrDesk
cd ZyrDesk
cargo test --workspace
cargo run -p zyr-cli -- doctor
```

Pour récupérer aussi les moteurs upstream (volumineux, utiles à partir du jalon M1) :

```bash
git submodule update --init --recursive
```

Construction de l'installateur Windows : voir [packaging/windows/README.md](packaging/windows/README.md).

## Documentation

| Document | Contenu |
|---|---|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Faisabilité, composants, processus Windows, flux de connexion, cycle de vie |
| [docs/engines/STRATEGY.md](docs/engines/STRATEGY.md) | Frontières avec Sunshine et Moonlight, liste des points de contact, politique de patchs |
| [docs/engines/UPGRADING.md](docs/engines/UPGRADING.md) | Procédure de mise à niveau des moteurs upstream |
| [docs/NETWORK.md](docs/NETWORK.md) | Tunnel, transport QUIC, traversée NAT, relais, budget latence et MTU |
| [docs/SECURITY.md](docs/SECURITY.md) | Identités, tickets de session, chiffrement, stockage Windows, modèle de menace |
| [docs/UI-UX.md](docs/UI-UX.md) | Direction visuelle, écrans, design system |
| [docs/TECH-CHOICES.md](docs/TECH-CHOICES.md) | Choix de technologies et alternatives rejetées |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Jalons M0 à M10 avec critères de sortie mesurables |
| [docs/TESTING.md](docs/TESTING.md) | Niveaux de tests, seuils de performance, banc de mesure |
| [docs/COMPLIANCE.md](docs/COMPLIANCE.md) | Licences, obligations, marques, brevets codecs |
| [docs/DECISIONS.md](docs/DECISIONS.md) | Décisions actées et décisions ouvertes |
| [patches/MANIFEST.md](patches/MANIFEST.md) | Versions de moteurs épinglées et adaptations appliquées |
| [perf/GATES.md](perf/GATES.md) | Seuils de performance chiffrés et protocoles de mesure |

## Licences

L'application ZyrDesk est sous GPLv3 (voir [LICENSE](LICENSE)), en cohérence avec les moteurs Sunshine et Moonlight (GPLv3). Les composants serveur (broker, relais) seront publiés sous AGPLv3. Détails et obligations : [docs/COMPLIANCE.md](docs/COMPLIANCE.md).
