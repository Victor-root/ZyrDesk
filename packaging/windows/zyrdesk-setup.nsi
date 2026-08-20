; Installateur ZyrDesk pour Windows.
;
; Installe les binaires existants, enregistre le service, et se désinstalle
; sans laisser de résidu. Les composants qui n'existent pas encore (moteurs,
; interface) sont ajoutés à leur jalon respectif aux emplacements marqués.
;
; Construction : makensis -DVERSION=<version> zyrdesk-setup.nsi

Unicode true
SetCompressor /SOLID lzma

!ifndef VERSION
  !define VERSION "0.1.0"
!endif
!ifndef BIN_DIR
  !define BIN_DIR "..\..\target\release"
!endif

!define PRODUIT "ZyrDesk"
!define EDITEUR "Projet ZyrDesk"
!define SITE "https://github.com/Victor-root/ZyrDesk"
!define CLE_DESINSTALL "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUIT}"

Name "${PRODUIT} ${VERSION}"
OutFile "${PRODUIT}-Setup-${VERSION}.exe"
InstallDir "$PROGRAMFILES64\${PRODUIT}"
InstallDirRegKey HKLM "Software\${PRODUIT}" "InstallDir"
RequestExecutionLevel admin
ShowInstDetails show
ShowUnInstDetails show

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${PRODUIT}"
VIAddVersionKey "CompanyName" "${EDITEUR}"
VIAddVersionKey "FileDescription" "Installateur ${PRODUIT}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "GPLv3"

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_LICENSE "..\..\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "French"

; Le produit range ses données dans un sous-dossier « data » de son
; propre dossier : l'installateur n'a rien à créer ailleurs, et la
; désinstallation n'a qu'un endroit à nettoyer.
!define DOSSIER_DONNEES "$INSTDIR\data"

; L'écran virtuel : ses fichiers signés, et l'endroit où ils sont posés.
; Doit rester égal à paths::virtual_screen_driver_dir() dans
; crates/zyr-proto/src/paths.rs : NSIS ne sait pas lire le code Rust.
!ifndef ECRAN_DIR
  !define ECRAN_DIR "..\..\vendor\ecran-virtuel"
!endif
!define DOSSIER_PILOTE_ECRAN "${DOSSIER_DONNEES}\screen\driver"

; Le seul port ouvert sur la machine. Doit rester égal à TUNNEL_PORT
; dans crates/zyr-proto/src/net.rs : NSIS ne sait pas lire le code Rust.
!define PORT_TUNNEL "47000"
!define REGLE_PARE_FEU "ZyrDesk (tunnel)"

Section "ZyrDesk" SEC_PRINCIPAL
  SectionIn RO
  SetOutPath "$INSTDIR"

  File "${BIN_DIR}\zyr-cli.exe"
  File "${BIN_DIR}\zyrdeskd.exe"
  File "..\..\LICENSE"

  ; M4 : ZyrDesk.exe (interface) et moteurs rebrandés.

  ; L'écran virtuel voyage avec le produit : rien à télécharger, rien à
  ; installer à part. Ses fichiers sont signés comme un tout, donc ils
  ; sont posés tels quels, sans être renommés ni retouchés.
  ;
  ; C'est le service qui les pose ensuite dans Windows, à son
  ; enregistrement, parce que c'est lui qui sait ce qu'il en fait et lui
  ; qui sait le retirer.
  SetOutPath "${DOSSIER_PILOTE_ECRAN}"
  File /nonfatal "${ECRAN_DIR}\MttVDD.inf"
  File /nonfatal "${ECRAN_DIR}\MttVDD.cat"
  File /nonfatal "${ECRAN_DIR}\MttVDD.dll"
  ; Sa licence MIT voyage avec lui : elle exige de conserver son avis de
  ; copyright dans toute redistribution.
  File /nonfatal /oname=LICENSE-ecran-virtuel "${ECRAN_DIR}\LICENSE"
  IfFileExists "${DOSSIER_PILOTE_ECRAN}\MttVDD.inf" ecran_present 0
  DetailPrint "Pilote d'écran virtuel absent de la construction : les sessions \
    demandant un écran plus grand que celui de cet ordinateur seront agrandies."
  ecran_present:
  SetOutPath "$INSTDIR"

  ; Une seule règle, pour un seul programme et un seul port : tout ce
  ; qu'une session transporte passe par le tunnel, et les moteurs ne
  ; sont joignables que depuis la machine elle-même.
  DetailPrint "Ouverture du port ${PORT_TUNNEL} pour ZyrDesk..."
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="${REGLE_PARE_FEU}"'
  Pop $0
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="${REGLE_PARE_FEU}" \
    dir=in action=allow protocol=UDP localport=${PORT_TUNNEL} \
    program="$INSTDIR\zyrdeskd.exe" description="Accès distant ZyrDesk"'
  Pop $0
  ${If} $0 <> 0
    MessageBox MB_OK|MB_ICONEXCLAMATION \
      "La règle de pare-feu n'a pas pu être créée (code $0).$\n$\n\
       Les autres ordinateurs ne pourront pas joindre celui-ci tant que \
       le port UDP ${PORT_TUNNEL} restera fermé."
  ${EndIf}

  ; Le service s'enregistre lui-même : l'installateur n'a pas à
  ; connaître son nom interne ni son compte.
  DetailPrint "Enregistrement du service ZyrDesk..."
  ExecWait '"$INSTDIR\zyrdeskd.exe" install' $0
  ${If} $0 <> 0
    MessageBox MB_OK|MB_ICONEXCLAMATION \
      "Le service ZyrDesk n'a pas pu être enregistré (code $0).$\n$\n\
       ZyrDesk est installé, mais l'ordinateur ne sera pas accessible \
       avant l'ouverture d'une session. Vous pouvez réessayer plus tard \
       avec « zyrdeskd install » dans une fenêtre administrateur."
  ${EndIf}

  WriteUninstaller "$INSTDIR\Uninstall.exe"

  WriteRegStr HKLM "Software\${PRODUIT}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "Software\${PRODUIT}" "Version" "${VERSION}"
  WriteRegStr HKLM "${CLE_DESINSTALL}" "DisplayName" "${PRODUIT}"
  WriteRegStr HKLM "${CLE_DESINSTALL}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${CLE_DESINSTALL}" "Publisher" "${EDITEUR}"
  WriteRegStr HKLM "${CLE_DESINSTALL}" "URLInfoAbout" "${SITE}"
  WriteRegStr HKLM "${CLE_DESINSTALL}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${CLE_DESINSTALL}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKLM "${CLE_DESINSTALL}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKLM "${CLE_DESINSTALL}" "NoModify" 1
  WriteRegDWORD HKLM "${CLE_DESINSTALL}" "NoRepair" 1

  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKLM "${CLE_DESINSTALL}" "EstimatedSize" "$0"
SectionEnd

Section "Uninstall"
  ; Le service tient le fichier programme tant qu'il tourne : il est
  ; arrêté et retiré avant qu'on touche à quoi que ce soit.
  DetailPrint "Retrait du service ZyrDesk..."
  ExecWait '"$INSTDIR\zyrdeskd.exe" uninstall'

  DetailPrint "Fermeture du port ${PORT_TUNNEL}..."
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="${REGLE_PARE_FEU}"'
  Pop $0

  ; Les données ne sont supprimées que si l'utilisateur le demande.
  ; En mode silencieux, elles sont conservées.
  IfSilent conserver_donnees
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Supprimer aussi les données ZyrDesk (moteurs, réglages, journaux, appairages) ?" \
    /SD IDNO IDNO conserver_donnees
  RMDir /r "${DOSSIER_DONNEES}"
  Goto donnees_traitees
  conserver_donnees:
  DetailPrint "Données conservées dans ${DOSSIER_DONNEES}"
  donnees_traitees:

  ; Le service vient de retirer le pilote de l'écran virtuel de Windows ;
  ; ses fichiers ne servent plus à rien. Ce ne sont pas des données de
  ; l'utilisateur, donc ils partent dans tous les cas.
  RMDir /r "${DOSSIER_PILOTE_ECRAN}"

  Delete "$INSTDIR\zyr-cli.exe"
  Delete "$INSTDIR\zyrdeskd.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKLM "${CLE_DESINSTALL}"
  DeleteRegKey HKLM "Software\${PRODUIT}"
SectionEnd
