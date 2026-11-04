!include "MUI2.nsh"
!ifndef VERSION
!define VERSION "0.5.0"
!endif
!define ROOTDIR "${__FILEDIR__}\.."
Name "RustBridge"
OutFile "${ROOTDIR}\dist\rustbridge_${VERSION}-setup-windows-x64.exe"
InstallDir "$LOCALAPPDATA\Programs\RustBridge"
RequestExecutionLevel user
Unicode True
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_LANGUAGE "English"
Section "Install RustBridge"
    SetOutPath "$INSTDIR"
    File "${ROOTDIR}\target\release\rustbridge.exe"
    File "${ROOTDIR}\LICENSE"
    File "${ROOTDIR}\README.md"
    WriteUninstaller "$INSTDIR\Uninstall.exe"
    CreateDirectory "$SMPROGRAMS\RustBridge"
    CreateShortCut "$SMPROGRAMS\RustBridge\RustBridge.lnk" "$INSTDIR\rustbridge.exe"
SectionEnd
Section "Uninstall"
    Delete "$SMPROGRAMS\RustBridge\rustbridge.exe"
    Delete "$SMPROGRAMS\RustBridge\RustBridge.lnk"
    RMDir "$SMPROGRAMS\RustBridge"
    Delete "$INSTDIR\rustbridge.exe"
    Delete "$INSTDIR\README.md"
    Delete "$INSTDIR\LICENSE"
    Delete "$INSTDIR\Uninstall.exe"
    RMDir "$INSTDIR"
SectionEnd
