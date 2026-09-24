; ZyrDesk installer for Windows.
;
; Installs the existing binaries, registers the service, and uninstalls
; without leaving anything behind. The components that do not exist yet
; (engines, interface) are added at their own milestone, where marked.
;
; Build: makensis -DVERSION=<version> zyrdesk-setup.nsi

Unicode true
SetCompressor /SOLID lzma

!ifndef VERSION
  !define VERSION "0.1.0"
!endif
!ifndef BIN_DIR
  !define BIN_DIR "..\..\target\release"
!endif

!define PRODUCT "ZyrDesk"
!define PUBLISHER "Projet ZyrDesk"
!define SITE "https://github.com/Victor-root/ZyrDesk"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT}"

Name "${PRODUCT} ${VERSION}"
OutFile "${PRODUCT}-Setup-${VERSION}.exe"
InstallDir "$PROGRAMFILES64\${PRODUCT}"
InstallDirRegKey HKLM "Software\${PRODUCT}" "InstallDir"
RequestExecutionLevel admin
ShowInstDetails show
ShowUnInstDetails show

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${PRODUCT}"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "FileDescription" "Installateur ${PRODUCT}"
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

; The product keeps its data in a "data" subfolder of its own folder: the
; installer has nothing to create anywhere else, and uninstalling has only
; one place to clean.
!define DATA_DIR "$INSTDIR\data"

; The virtual screen: its signed files, and the place they are put. The
; destination must stay equal to what paths::virtual_screen_driver_dir()
; returns in crates/zyr-proto/src/paths.rs, which looks next to the
; program: NSIS cannot read Rust code.
!ifndef SCREEN_DIR
  !define SCREEN_DIR "..\..\vendor\ecran-virtuel"
!endif
!define SCREEN_DRIVER_DIR "$INSTDIR\vendor\ecran-virtuel"

; The only port open on the machine. Must stay equal to TUNNEL_PORT in
; crates/zyr-proto/src/net.rs: NSIS cannot read Rust code.
!define TUNNEL_PORT "47000"
!define FIREWALL_RULE "ZyrDesk (tunnel)"

Section "ZyrDesk" SEC_MAIN
  SectionIn RO
  SetOutPath "$INSTDIR"

  File "${BIN_DIR}\zyr-cli.exe"
  File "${BIN_DIR}\zyrdeskd.exe"
  File "..\..\LICENSE"

  ; M4: ZyrDesk.exe (interface) and rebranded engines.

  ; The virtual screen travels with the product: nothing to download,
  ; nothing to install separately. Its files are signed as a whole, so
  ; they are put down as they are, neither renamed nor touched.
  ;
  ; It is the service that then installs them into Windows, when it
  ; registers, because it is the one that knows what it does with them and
  ; the one that knows how to remove them.
  SetOutPath "${SCREEN_DRIVER_DIR}"
  File /nonfatal "${SCREEN_DIR}\MttVDD.inf"
  File /nonfatal "${SCREEN_DIR}\MttVDD.cat"
  File /nonfatal "${SCREEN_DIR}\MttVDD.dll"
  ; Its MIT licence travels with it: it requires keeping its copyright
  ; notice in any redistribution.
  File /nonfatal /oname=LICENSE-ecran-virtuel "${SCREEN_DIR}\LICENSE"
  IfFileExists "${SCREEN_DRIVER_DIR}\MttVDD.inf" screen_present 0
  DetailPrint "Pilote d'écran virtuel absent de la construction : les sessions \
    demandant un écran plus grand que celui de cet ordinateur seront agrandies."
  screen_present:
  SetOutPath "$INSTDIR"

  ; A single rule, for a single program and a single port: everything a
  ; session carries goes through the tunnel, and the engines can only be
  ; reached from the machine itself.
  DetailPrint "Ouverture du port ${TUNNEL_PORT} pour ZyrDesk..."
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="${FIREWALL_RULE}"'
  Pop $0
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="${FIREWALL_RULE}" \
    dir=in action=allow protocol=UDP localport=${TUNNEL_PORT} \
    program="$INSTDIR\zyrdeskd.exe" description="Accès distant ZyrDesk"'
  Pop $0
  ${If} $0 <> 0
    MessageBox MB_OK|MB_ICONEXCLAMATION \
      "La règle de pare-feu n'a pas pu être créée (code $0).$\n$\n\
       Les autres ordinateurs ne pourront pas joindre celui-ci tant que \
       le port UDP ${TUNNEL_PORT} restera fermé."
  ${EndIf}

  ; The service registers itself: the installer does not need to know its
  ; internal name or its account.
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

  WriteRegStr HKLM "Software\${PRODUCT}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "Software\${PRODUCT}" "Version" "${VERSION}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "URLInfoAbout" "${SITE}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKLM "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoRepair" 1

  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "EstimatedSize" "$0"
SectionEnd

Section "Uninstall"
  ; The service holds the program file while it runs: it is stopped and
  ; removed before anything else is touched.
  DetailPrint "Retrait du service ZyrDesk..."
  ExecWait '"$INSTDIR\zyrdeskd.exe" uninstall'

  DetailPrint "Fermeture du port ${TUNNEL_PORT}..."
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="${FIREWALL_RULE}"'
  Pop $0

  ; The data is only deleted if the user asks for it. In silent mode, it
  ; is kept.
  IfSilent keep_data
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Supprimer aussi les données ZyrDesk (moteurs, réglages, journaux, appairages) ?" \
    /SD IDNO IDNO keep_data
  RMDir /r "${DATA_DIR}"
  Goto data_handled
  keep_data:
  DetailPrint "Données conservées dans ${DATA_DIR}"
  data_handled:

  ; The service has just removed the virtual screen driver from Windows;
  ; its files are no longer of any use. They are not the user's data, so
  ; they go in every case.
  RMDir /r "${SCREEN_DRIVER_DIR}"
  RMDir "$INSTDIR\vendor"

  Delete "$INSTDIR\zyr-cli.exe"
  Delete "$INSTDIR\zyrdeskd.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKLM "${UNINSTALL_KEY}"
  DeleteRegKey HKLM "Software\${PRODUCT}"
SectionEnd
