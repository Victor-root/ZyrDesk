# Empaquetage Windows

## Construire l'installateur

Prérequis : Rust stable et [NSIS](https://nsis.sourceforge.io/) (`makensis` dans le PATH).

```powershell
cargo build --release
cd packaging\windows
makensis -DVERSION=0.1.0 zyrdesk-setup.nsi
```

Produit `ZyrDesk-Setup-0.1.0.exe` dans le dossier courant.

## Vérifier une installation propre (critère de sortie du jalon M0)

1. Installer : lancer l'exécutable produit, accepter la licence, installer.
2. Vérifier : `C:\Program Files\ZyrDesk\zyr-cli.exe` existe et l'entrée « ZyrDesk » apparaît dans Applications installées.
3. Diagnostiquer : ouvrir une invite de commandes et lancer `"C:\Program Files\ZyrDesk\zyr-cli.exe" doctor`.
4. Vérifier le service : « ZyrDesk » apparaît dans la console des services (`services.msc`), en démarrage automatique.
5. Désinstaller depuis Applications installées, répondre « Oui » à la suppression des données.
6. Vérifier l'absence de résidu : le dossier d'installation a disparu, la clé de registre `HKLM\Software\ZyrDesk` n'existe plus, et « ZyrDesk » a disparu de la console des services.

Le produit range tout ce qu'il écrit (moteurs, réglages, journaux, appairages) dans un sous-dossier `data` de son dossier d'installation. La désinstallation propose de le supprimer.

Désinstallation silencieuse : `"C:\Program Files\ZyrDesk\Uninstall.exe" /S` (conserve les données).

## Signature

Les binaires ne sont pas signés : Windows SmartScreen affiche un avertissement « application non reconnue » au premier lancement. C'est le comportement attendu pour un projet open source jeune sans certificat payant. Voir `docs/COMPLIANCE.md`.

## Composants à ajouter

| Jalon | Ajout |
|---|---|
| M3 | `zyrdeskd.exe`, enregistrement du service Windows, règle de pare-feu UDP entrante, arrêt et nettoyage à la désinstallation |
| M4 | `ZyrDesk.exe` (interface), moteurs rebrandés et leurs dépendances, raccourcis menu Démarrer |
| M9 | Installation optionnelle et consentie du pilote d'écran virtuel tiers |
