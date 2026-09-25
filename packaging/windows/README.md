# Empaquetage Windows

## Construire l'installateur

Prérequis : Rust stable et [NSIS](https://nsis.sourceforge.io/) (`makensis` dans le PATH).

```powershell
cargo build --release
cd packaging\windows
makensis "-DVERSION=0.1.0" zyrdesk-setup.nsi
```

Produit `ZyrDesk-Setup-0.1.0.exe` dans le dossier courant.

Les guillemets autour de la version sont nécessaires sous PowerShell, qui couperait sinon l'argument en deux.

## Ce que l'installateur pose

- `zyrdeskd.exe`, le service. C'est aussi lui qui fait tourner le moteur quand quelqu'un prend la main sur l'ordinateur.
- `zyr-cli.exe`, la ligne de commande.
- `vendor\ecran-virtuel`, le pilote d'écran virtuel.
- `vendor\ffmpeg`, les trois fichiers de FFmpeg dont le moteur se sert pour compresser et décompresser l'image et le son, avec le dossier `licenses` qui doit les accompagner (voir `vendor/ffmpeg/README.md`).
- `LICENSE`, la licence de ZyrDesk.

La fenêtre de ZyrDesk (`ZyrDesk.exe`) n'y est pas encore : elle arrive avec le jalon M4 (voir plus bas).

## Vérifier une installation propre (critère de sortie du jalon M0)

1. Installer : lancer l'exécutable produit, accepter la licence, installer.
2. Vérifier : `C:\Program Files\ZyrDesk\zyr-cli.exe` existe, `C:\Program Files\ZyrDesk\vendor\ffmpeg` contient les trois fichiers de FFmpeg et leur dossier `licenses`, et l'entrée « ZyrDesk » apparaît dans Applications installées.
3. Diagnostiquer : ouvrir une invite de commandes et lancer `"C:\Program Files\ZyrDesk\zyr-cli.exe" doctor`. La ligne « FFmpeg » doit dire « présent et chargeable ».
4. Vérifier le service : « ZyrDesk » apparaît dans la console des services (`services.msc`), en démarrage automatique.
5. Désinstaller depuis Applications installées, répondre « Oui » à la suppression des données.
6. Vérifier l'absence de résidu : le dossier d'installation a disparu, la clé de registre `HKLM\Software\ZyrDesk` n'existe plus, et « ZyrDesk » a disparu de la console des services.

Le produit range tout ce qu'il écrit (réglages, journaux, appairages) dans un sous-dossier `data` de son dossier d'installation. La désinstallation propose de le supprimer.

Désinstallation silencieuse : `"C:\Program Files\ZyrDesk\Uninstall.exe" /S` (conserve les données).

## FFmpeg et sa licence

Le FFmpeg livré avec ZyrDesk est sous GPL. Publier l'installateur, c'est donc aussi publier FFmpeg : chaque publication doit rendre disponibles les sources exactes qui l'ont produit, comme l'explique la partie « Licence » de `vendor/ffmpeg/README.md`.

## Signature

Les binaires ne sont pas signés : Windows SmartScreen affiche un avertissement « application non reconnue » au premier lancement. C'est le comportement attendu pour un projet open source jeune sans certificat payant. Voir `docs/COMPLIANCE.md`.

## Composants à ajouter

| Jalon | Ajout |
|---|---|
| M3 | `zyrdeskd.exe`, enregistrement du service Windows, règle de pare-feu UDP entrante, arrêt et nettoyage à la désinstallation |
| M4 | `ZyrDesk.exe` (interface), raccourcis menu Démarrer |
| M9 | Installation optionnelle et consentie du pilote d'écran virtuel tiers |
