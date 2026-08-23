; NSIS installer for persistex. Per-user install, so no admin prompt.
!include "MUI2.nsh"

!ifndef VERSION
  !define VERSION "0.1.0"
!endif
!ifndef OUTDIR
  !define OUTDIR "..\dist"
!endif

Name "persistex ${VERSION}"
OutFile "${OUTDIR}\persistex-${VERSION}-windows-setup.exe"
Unicode True
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\persistex"
InstallDirRegKey HKCU "Software\persistex" "InstallDir"

!define MUI_ICON "assets\icon.ico"
!define MUI_UNICON "assets\icon.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\persistex.exe"

!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "persistex"
  SetOutPath "$INSTDIR"
  File "${OUTDIR}\persistex.exe"
  WriteRegStr HKCU "Software\persistex" "InstallDir" "$INSTDIR"

  CreateShortCut "$SMPROGRAMS\persistex.lnk" "$INSTDIR\persistex.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; Add/Remove Programs entry
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\persistex" \
    "DisplayName" "persistex"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\persistex" \
    "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\persistex" \
    "DisplayIcon" "$INSTDIR\persistex.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\persistex" \
    "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\persistex" \
    "NoModify" 1
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\persistex.exe"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$SMPROGRAMS\persistex.lnk"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\persistex"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\persistex"
SectionEnd
