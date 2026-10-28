Unicode true
!include "MUI2.nsh"

!define APPNAME   "Coverly"
!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!define PUBLISHER "Coverly"
!define EXE       "coverly.exe"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}"

Name "${APPNAME} ${VERSION}"
OutFile "..\dist\Coverly-Setup-${VERSION}.exe"
InstallDir "$PROGRAMFILES64\${APPNAME}"
InstallDirRegKey HKLM "Software\${APPNAME}" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
BrandingText "${APPNAME} ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName"     "${APPNAME}"
VIAddVersionKey "FileVersion"     "${VERSION}"
VIAddVersionKey "ProductVersion"  "${VERSION}"
VIAddVersionKey "CompanyName"     "${PUBLISHER}"
VIAddVersionKey "FileDescription" "${APPNAME} installer"
VIAddVersionKey "LegalCopyright"  "MIT License"

!define MUI_ICON   "..\assets\app.ico"
!define MUI_UNICON "..\assets\app.ico"
!define MUI_ABORTWARNING

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "Run ${APPNAME}"
!define MUI_FINISHPAGE_RUN_FUNCTION LaunchAsUser
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Farsi"

Function LaunchAsUser
  Exec '"$WINDIR\explorer.exe" "$INSTDIR\${EXE}"'
FunctionEnd

Section "${APPNAME} (required)" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "..\target\release\${EXE}"
  File "..\assets\app.ico"

  SetOutPath "$INSTDIR\templates"
  File /r "..\templates\*.*"

  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateDirectory "$SMPROGRAMS\${APPNAME}"
  CreateShortcut "$SMPROGRAMS\${APPNAME}\${APPNAME}.lnk" "$INSTDIR\${EXE}" "" "$INSTDIR\app.ico"
  CreateShortcut "$SMPROGRAMS\${APPNAME}\Uninstall ${APPNAME}.lnk" "$INSTDIR\uninstall.exe"

  WriteRegStr HKLM "Software\${APPNAME}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "${UNINSTKEY}" "DisplayName"     "${APPNAME}"
  WriteRegStr HKLM "${UNINSTKEY}" "DisplayVersion"  "${VERSION}"
  WriteRegStr HKLM "${UNINSTKEY}" "Publisher"       "${PUBLISHER}"
  WriteRegStr HKLM "${UNINSTKEY}" "DisplayIcon"     "$INSTDIR\app.ico"
  WriteRegStr HKLM "${UNINSTKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegDWORD HKLM "${UNINSTKEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINSTKEY}" "NoRepair" 1
SectionEnd

Section "Desktop shortcut" SecDesktop
  CreateShortcut "$DESKTOP\${APPNAME}.lnk" "$INSTDIR\${EXE}" "" "$INSTDIR\app.ico"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\${EXE}"
  Delete "$INSTDIR\app.ico"
  Delete "$INSTDIR\uninstall.exe"
  RMDir /r "$INSTDIR\templates"
  RMDir "$INSTDIR"

  Delete "$DESKTOP\${APPNAME}.lnk"
  RMDir /r "$SMPROGRAMS\${APPNAME}"

  DeleteRegKey HKLM "${UNINSTKEY}"
  DeleteRegKey HKLM "Software\${APPNAME}"
SectionEnd
