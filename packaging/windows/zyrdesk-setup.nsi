; Installateur ZyrDesk pour Windows.
;
; Jalon M0 : installe les binaires existants et se désinstalle sans laisser
; de résidu. Les composants qui n'existent pas encore (service, moteurs,
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

!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_LICENSE "..\..\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "French"

; Répertoire de données commun, hors Program Files.
; SetShellVarContext all fait pointer $APPDATA sur C:\ProgramData.
Var DossierDonnees

Function .onInit
  SetShellVarContext all
  StrCpy $DossierDonnees "$APPDATA\${PRODUIT}"
FunctionEnd

Section "ZyrDesk" SEC_PRINCIPAL
  SectionIn RO
  SetOutPath "$INSTDIR"

  File "${BIN_DIR}\zyr-cli.exe"
  File "..\..\LICENSE"

  ; M3 : zyrdeskd.exe et enregistrement du service Windows.
  ; M4 : ZyrDesk.exe (interface) et moteurs rebrandés.

  CreateDirectory "$DossierDonnees"
  CreateDirectory "$DossierDonnees\logs"

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
  ; M3 : arrêt et suppression du service avant de toucher aux fichiers.
  ; M3 : suppression des règles de pare-feu créées à l'installation.

  Delete "$INSTDIR\zyr-cli.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  ; Les données ne sont supprimées que si l'utilisateur le demande.
  ; En mode silencieux, elles sont conservées.
  IfSilent conserver_donnees
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Supprimer aussi les données ZyrDesk (réglages, journaux, appairages) ?" \
    /SD IDNO IDNO conserver_donnees
  RMDir /r "$DossierDonnees"
  Goto donnees_traitees
  conserver_donnees:
  DetailPrint "Données conservées dans $DossierDonnees"
  donnees_traitees:

  DeleteRegKey HKLM "${CLE_DESINSTALL}"
  DeleteRegKey HKLM "Software\${PRODUIT}"
SectionEnd

Function un.onInit
  SetShellVarContext all
  StrCpy $DossierDonnees "$APPDATA\${PRODUIT}"
FunctionEnd
