; Set the compression algorithm.
!if "{{compression}}" == ""
  SetCompressor /SOLID lzma
!else
  SetCompressor /SOLID "{{compression}}"
!endif

Unicode true

!include MUI2.nsh
!include FileFunc.nsh
!include x64.nsh
!include WordFunc.nsh
!include "FileAssociation.nsh"
!include "StrFunc.nsh"
!include "StrFunc.nsh"
${StrCase}
${StrLoc}

!define MANUFACTURER "{{manufacturer}}"
!define PRODUCTNAME "{{product_name}}"
!define VERSION "{{version}}"
!define VERSIONWITHBUILD "{{version_with_build}}"
!define SHORTDESCRIPTION "{{short_description}}"
!define INSTALLMODE "{{install_mode}}"
!define LICENSE "{{license}}"
!define INSTALLERICON "{{installer_icon}}"
!define SIDEBARIMAGE "{{sidebar_image}}"
!define HEADERIMAGE "{{header_image}}"
!define MAINBINARYNAME "{{main_binary_name}}"
!define MAINBINARYSRCPATH "{{main_binary_path}}"
!define IDENTIFIER "{{identifier}}"
!define COPYRIGHT "{{copyright}}"
!define OUTFILE "{{out_file}}"
!define ARCH "{{arch}}"
!define PLUGINSPATH "{{additional_plugins_path}}"
!define ALLOWDOWNGRADES "{{allow_downgrades}}"
!define DISPLAYLANGUAGESELECTOR "{{display_language_selector}}"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCTNAME}"
!define MANUPRODUCTKEY "Software\${MANUFACTURER}\${PRODUCTNAME}"
!define UNINSTALLERSIGNCOMMAND "{{uninstaller_sign_cmd}}"
!define ESTIMATEDSIZE "{{estimated_size}}"

; Variables for service installation
Var InstallServiceCheckbox
Var InstallServiceState

; Variables for logging
Var LogFile
Var TempDir

; Force verbose installation details to always be visible
ShowInstDetails show
ShowUnInstDetails show

; Enable detailed logging
!define MUI_INSTFILESPAGE_FINISHHEADER_TEXT "Installation Complete - Check logs for details"
!define MUI_INSTFILESPAGE_FINISHHEADER_SUBTEXT "Installation logs are available in the temp directory"

Name "${PRODUCTNAME}"
BrandingText "${COPYRIGHT}"
OutFile "${OUTFILE}"

VIProductVersion "${VERSIONWITHBUILD}"
VIAddVersionKey "ProductName" "${PRODUCTNAME}"
VIAddVersionKey "FileDescription" "${SHORTDESCRIPTION}"
VIAddVersionKey "LegalCopyright" "${COPYRIGHT}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"

; Plugins path, currently exists for linux only
!if "${PLUGINSPATH}" != ""
    !addplugindir "${PLUGINSPATH}"
!endif

; Handle code signing
; This is based on the instructions for self-signed certificates from
; https://docs.microsoft.com/en-us/windows/win32/msi/digital-signatures-and-windows-installer
; and the NSIS documentation for !uninstfinalize and !finalize
; https://nsis.sourceforge.io/Docs/Chapter4.html#flags
!if "${UNINSTALLERSIGNCOMMAND}" != ""
  !uninstfinalize '${UNINSTALLERSIGNCOMMAND}'
!endif

; Handle install mode, `perUser`, `perMachine` or `both`
!if "${INSTALLMODE}" == "perMachine"
  RequestExecutionLevel highest
!endif

!if "${INSTALLMODE}" == "currentUser"
  RequestExecutionLevel user
!endif

!if "${INSTALLMODE}" == "both"
  !define MULTIUSER_MUI
  !define MULTIUSER_INSTALLMODE_INSTDIR "${PRODUCTNAME}"
  !define MULTIUSER_INSTALLMODE_COMMANDLINE
  !if "${ARCH}" == "x64"
    !define MULTIUSER_USE_PROGRAMFILES64
  !else if "${ARCH}" == "arm64"
    !define MULTIUSER_USE_PROGRAMFILES64
  !endif
  !define MULTIUSER_INSTALLMODE_DEFAULT_REGISTRY_KEY "${UNINSTKEY}"
  !define MULTIUSER_INSTALLMODE_DEFAULT_REGISTRY_VALUENAME "CurrentUser"
  !define MULTIUSER_INSTALLMODEPAGE_SHOWUSERNAME
  !define MULTIUSER_INSTALLMODE_FUNCTION RestorePreviousInstallLocation
  !define MULTIUSER_EXECUTIONLEVEL Highest
  !include MultiUser.nsh
!endif

; installer icon
!if "${INSTALLERICON}" != ""
  !define MUI_ICON "${INSTALLERICON}"
!endif

; installer sidebar image
!if "${SIDEBARIMAGE}" != ""
  !define MUI_WELCOMEFINISHPAGE_BITMAP "${SIDEBARIMAGE}"
!endif

; installer header image
!if "${HEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE
  !define MUI_HEADERIMAGE_BITMAP  "${HEADERIMAGE}"
!endif

; Define registry key to store installer language
!define MUI_LANGDLL_REGISTRY_ROOT "HKCU"
!define MUI_LANGDLL_REGISTRY_KEY "${MANUPRODUCTKEY}"
!define MUI_LANGDLL_REGISTRY_VALUENAME "Installer Language"

; Installer pages, must be ordered as they appear
; 1. Welcome Page
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_WELCOME

; 2. License Page (if defined)
!if "${LICENSE}" != ""
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
  !insertmacro MUI_PAGE_LICENSE "${LICENSE}"
!endif

; 3. Install mode (if it is set to `both`)
!if "${INSTALLMODE}" == "both"
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
  !insertmacro MULTIUSER_PAGE_INSTALLMODE
!endif


; 4. Custom page to ask user if he wants to reinstall/uninstall
;    only if a previous installtion was detected
Var ReinstallPageCheck
Page custom PageReinstall PageLeaveReinstall
Function PageReinstall
  ; Uninstall previous WiX installation if exists.
  ;
  ; A WiX installer stores the isntallation info in registry
  ; using a UUID and so we have to loop through all keys under
  ; `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall`
  ; and check if `DisplayName` and `Publisher` keys match ${PRODUCTNAME} and ${MANUFACTURER}
  ;
  ; This has a potentional issue that there maybe another installation that matches
  ; our ${PRODUCTNAME} and ${MANUFACTURER} but wasn't installed by our WiX installer,
  ; however, this should be fine since the user will have to confirm the uninstallation
  ; and they can chose to abort it if doesn't make sense.
  StrCpy $0 0
  wix_loop:
    EnumRegKey $1 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall" $0
    StrCmp $1 "" wix_done ; Exit loop if there is no more keys to loop on
    IntOp $0 $0 + 1
    ReadRegStr $R0 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1" "DisplayName"
    ReadRegStr $R1 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1" "Publisher"
    StrCmp "$R0$R1" "${PRODUCTNAME}${MANUFACTURER}" 0 wix_loop
    ReadRegStr $R0 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1" "UninstallString"
    ${StrCase} $R1 $R0 "L"
    ${StrLoc} $R0 $R1 "msiexec" ">"
    StrCmp $R0 0 0 wix_done
    StrCpy $R7 "wix"
    StrCpy $R6 "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1"
    Goto compare_version
  wix_done:

  ; Check if there is an existing installation, if not, abort the reinstall page
  ReadRegStr $R0 SHCTX "${UNINSTKEY}" ""
  ReadRegStr $R1 SHCTX "${UNINSTKEY}" "UninstallString"
  ${IfThen} "$R0$R1" == "" ${|} Abort ${|}

  ; Compare this installar version with the existing installation
  ; and modify the messages presented to the user accordingly
  compare_version:
  StrCpy $R4 "$(older)"
  ${If} $R7 == "wix"
    ReadRegStr $R0 HKLM "$R6" "DisplayVersion"
  ${Else}
    ReadRegStr $R0 SHCTX "${UNINSTKEY}" "DisplayVersion"
  ${EndIf}
  ${IfThen} $R0 == "" ${|} StrCpy $R4 "$(unknown)" ${|}

  nsis_tauri_utils::SemverCompare "${VERSION}" $R0
  Pop $R0
  ; Reinstalling the same version
  ${If} $R0 == 0
    StrCpy $R1 "$(alreadyInstalledLong)"
    StrCpy $R2 "$(addOrReinstall)"
    StrCpy $R3 "$(uninstallApp)"
    !insertmacro MUI_HEADER_TEXT "$(alreadyInstalled)" "$(chooseMaintenanceOption)"
    StrCpy $R5 "2"
  ; Upgrading
  ${ElseIf} $R0 == 1
    StrCpy $R1 "$(olderOrUnknownVersionInstalled)"
    StrCpy $R2 "$(uninstallBeforeInstalling)"
    StrCpy $R3 "$(dontUninstall)"
    !insertmacro MUI_HEADER_TEXT "$(alreadyInstalled)" "$(choowHowToInstall)"
    StrCpy $R5 "1"
  ; Downgrading
  ${ElseIf} $R0 == -1
    StrCpy $R1 "$(newerVersionInstalled)"
    StrCpy $R2 "$(uninstallBeforeInstalling)"
    !if "${ALLOWDOWNGRADES}" == "true"
      StrCpy $R3 "$(dontUninstall)"
    !else
      StrCpy $R3 "$(dontUninstallDowngrade)"
    !endif
    !insertmacro MUI_HEADER_TEXT "$(alreadyInstalled)" "$(choowHowToInstall)"
    StrCpy $R5 "1"
  ${Else}
    Abort
  ${EndIf}

  Call SkipIfPassive

  nsDialogs::Create 1018
  Pop $R4
  ${IfThen} $(^RTL) == 1 ${|} nsDialogs::SetRTL $(^RTL) ${|}

  ${NSD_CreateLabel} 0 0 100% 24u $R1
  Pop $R1

  ${NSD_CreateRadioButton} 30u 50u -30u 8u $R2
  Pop $R2
  ${NSD_OnClick} $R2 PageReinstallUpdateSelection

  ${NSD_CreateRadioButton} 30u 70u -30u 8u $R3
  Pop $R3
  ; disable this radio button if downgrading and downgrades are disabled
  !if "${ALLOWDOWNGRADES}" == "false"
    ${IfThen} $R0 == -1 ${|} EnableWindow $R3 0 ${|}
  !endif
  ${NSD_OnClick} $R3 PageReinstallUpdateSelection

  ; Check the first radio button if this the first time
  ; we enter this page or if the second button wasn't
  ; selected the last time we were on this page
  ${If} $ReinstallPageCheck != 2
    SendMessage $R2 ${BM_SETCHECK} ${BST_CHECKED} 0
  ${Else}
    SendMessage $R3 ${BM_SETCHECK} ${BST_CHECKED} 0
  ${EndIf}

  ${NSD_SetFocus} $R2
  nsDialogs::Show
FunctionEnd
Function PageReinstallUpdateSelection
  ${NSD_GetState} $R2 $R1
  ${If} $R1 == ${BST_CHECKED}
    StrCpy $ReinstallPageCheck 1
  ${Else}
    StrCpy $ReinstallPageCheck 2
  ${EndIf}
FunctionEnd
Function PageLeaveReinstall
  ${NSD_GetState} $R2 $R1

  ; $R5 holds whether we are reinstalling the same version or not
  ; $R5 == "1" -> different versions
  ; $R5 == "2" -> same version
  ;
  ; $R1 holds the radio buttons state. its meaning is dependant on the context
  StrCmp $R5 "1" 0 +2 ; Existing install is not the same version?
    StrCmp $R1 "1" reinst_uninstall reinst_done ; $R1 == "1", then user chose to uninstall existing version, otherwise skip uninstalling
  StrCmp $R1 "1" reinst_done ; Same version? skip uninstalling
  reinst_uninstall:
    HideWindow
    ClearErrors    ; Always stop and remove any existing Blue Onyx Service during reinstall/modify
    DetailPrint "=== INSTALLER: Comprehensive service cleanup ==="

    ; First, try to stop the service gracefully
    DetailPrint "Attempting to stop Blue Onyx Service gracefully..."
    nsExec::ExecToStack 'net stop BlueOnyxService'
    Pop $0
    Pop $1
    DetailPrint "Service stop result: exit code $0"
    DetailPrint "Service stop output: $1"

    ; Wait a moment for graceful shutdown
    Sleep 2000

    ; Force stop if still running
    DetailPrint "Force stopping any remaining Blue Onyx processes..."
    nsExec::ExecToStack 'taskkill /f /im blue_onyx_service.exe 2>nul'
    Pop $0
    Pop $1
    DetailPrint "Force stop result: exit code $0"

    ; Check if service exists and remove it
    DetailPrint "Checking for existing Blue Onyx Service..."
    nsExec::ExecToStack 'sc.exe query BlueOnyxService'
    Pop $0
    Pop $1
    ${If} $0 == 0
      DetailPrint "Blue Onyx Service found, removing..."
      nsExec::ExecToStack 'sc.exe delete BlueOnyxService'
      Pop $0
      Pop $1
      DetailPrint "Service delete result: exit code $0"
      ${If} $0 == 0
        DetailPrint "Blue Onyx Service removed successfully"
      ${Else}
        DetailPrint "Service deletion warning (code: $0): $1"
      ${EndIf}
    ${Else}
      DetailPrint "No existing Blue Onyx Service found"
    ${EndIf}

    ; Clean up any orphaned event log sources
    DetailPrint "Cleaning up event log sources..."
    nsExec::ExecToStack 'powershell.exe -Command "try { Remove-EventLog -Source BlueOnyxService -ErrorAction SilentlyContinue; Write-Output \"Event log source cleaned\" } catch { Write-Output \"No event log source found\" }"'
    Pop $0
    Pop $1
    DetailPrint "Event log cleanup: $1"

    DetailPrint "=== INSTALLER: Service cleanup completed ==="

    ; Remove the old installation without running the uninstaller
    ; This leaves files in place but cleans up registry and shortcuts
    ${If} $R7 == "wix"
      ReadRegStr $R1 HKLM "$R6" "UninstallString"
      ExecWait '$R1' $0
    ${Else}
      ; Instead of running the full uninstaller, just clean up registry and shortcuts
      ; but leave files in place to be overwritten
      DetailPrint "Cleaning up previous installation registry entries and shortcuts..."

      ; Remove old shortcuts if they exist
      ReadRegStr $R2 SHCTX "${MANUPRODUCTKEY}" ""
      ${If} $R2 != ""
        IfFileExists "$DESKTOP\${PRODUCTNAME}.lnk" 0 +2
          Delete "$DESKTOP\${PRODUCTNAME}.lnk"

        ; Try to find and remove start menu shortcuts
        ReadRegStr $R3 HKCU "${MANUPRODUCTKEY}" "Start Menu Folder"
        ${If} $R3 != ""
          Delete "$SMPROGRAMS\$R3\${PRODUCTNAME}.lnk"
          RMDir "$SMPROGRAMS\$R3"
        ${EndIf}
      ${EndIf}

      ; Clean up old registry entries but keep the install path
      DeleteRegKey SHCTX "${UNINSTKEY}"
    ${EndIf}

    BringToFront

    ${IfThen} ${Errors} ${|} StrCpy $0 2 ${|} ; ExecWait failed, set fake exit code

    ${If} $0 <> 0
    ${AndIf} $0 <> 1619  ; 1619 = package not found, which is OK for our case
      ${If} $0 = 1 ; User aborted uninstaller?
        StrCmp $R5 "2" 0 +2 ; Is the existing install the same version?
          Quit ; ...yes, already installed, we are done
        Abort
      ${EndIf}
      ; Don't show error for file existence since we're not running full uninstaller
      ${IfNot} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
        Goto reinst_done
      ${EndIf}
    ${EndIf}
  reinst_done:
FunctionEnd

; 5. Choose install directoy page
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_DIRECTORY

; 6. Custom page for service installation option
Page custom ServiceInstallPage ServiceInstallPageLeave
Function ServiceInstallPage
  Call SkipIfPassive

  !insertmacro MUI_HEADER_TEXT "Service Installation" "Choose whether to install Blue Onyx as a Windows Service"

  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 24u "Blue Onyx can be installed as a Windows Service to run automatically in the background."
  Pop $0

  ${NSD_CreateLabel} 0 30u 100% 24u "Services start automatically when Windows boots and run without requiring a user to be logged in."
  Pop $0

  ${NSD_CreateCheckbox} 0 60u 100% 12u "$(installService)"
  Pop $InstallServiceCheckbox

  ${NSD_CreateLabel} 0 80u 100% 24u "Note: When installing as a service, desktop shortcuts and start menu entries will be skipped."
  Pop $0

  ; Default to checked
  ${NSD_SetState} $InstallServiceCheckbox ${BST_CHECKED}

  nsDialogs::Show
FunctionEnd

Function ServiceInstallPageLeave
  ${NSD_GetState} $InstallServiceCheckbox $InstallServiceState
FunctionEnd

; 7. Start menu shortcut page
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
Var AppStartMenuFolder
!insertmacro MUI_PAGE_STARTMENU Application $AppStartMenuFolder

; 8. Installation page
; Force details to be expanded by default
!define MUI_INSTFILESPAGE_PROGRESSBAR colored
!define MUI_PAGE_CUSTOMFUNCTION_SHOW ShowInstFilesDetails
!insertmacro MUI_PAGE_INSTFILES

; 9. Finish page
;
; Don't auto jump to finish page after installation page,
; because the installation page has useful info that can be used debug any issues with the installer.
!define MUI_FINISHPAGE_NOAUTOCLOSE
; Use show readme button in the finish page as a button create a desktop shortcut
!define MUI_FINISHPAGE_SHOWREADME
!define MUI_FINISHPAGE_SHOWREADME_TEXT "$(createDesktop)"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION CreateDesktopShortcutConditional
; Show run app after installation only if not installing as service
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "Run ${PRODUCTNAME}"
!define MUI_FINISHPAGE_RUN_FUNCTION RunAppConditional
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_FINISH

; Uninstaller Pages
; 1. Confirm uninstall page
{{#if appdata_paths}}
!define /ifndef WS_EX_LAYOUTRTL         0x00400000
!define MUI_PAGE_CUSTOMFUNCTION_SHOW un.ConfirmShow
Function un.ConfirmShow
    FindWindow $1 "#32770" "" $HWNDPARENT ; Find inner dialog
    ${If} $(^RTL) == 1
      System::Call 'USER32::CreateWindowEx(i${__NSD_CheckBox_EXSTYLE}|${WS_EX_LAYOUTRTL},t"${__NSD_CheckBox_CLASS}",t "$(deleteAppData)",i${__NSD_CheckBox_STYLE},i 50,i 100,i 400, i 25,i$1,i0,i0,i0)i.s'
    ${Else}
      System::Call 'USER32::CreateWindowEx(i${__NSD_CheckBox_EXSTYLE},t"${__NSD_CheckBox_CLASS}",t "$(deleteAppData)",i${__NSD_CheckBox_STYLE},i 0,i 100,i 400, i 25,i$1,i0,i0,i0)i.s'
    ${EndIf}
    Pop $DeleteAppDataCheckbox
    SendMessage $HWNDPARENT ${WM_GETFONT} 0 0 $1
    SendMessage $DeleteAppDataCheckbox ${WM_SETFONT} $1 1
FunctionEnd
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE un.ConfirmLeave
Function un.ConfirmLeave
    SendMessage $DeleteAppDataCheckbox ${BM_GETCHECK} 0 0 $DeleteAppDataCheckboxState
FunctionEnd
{{/if}}
!insertmacro MUI_UNPAGE_CONFIRM

; 2. Uninstalling Page
!define MUI_PAGE_CUSTOMFUNCTION_SHOW un.ShowUninstFilesDetails
!insertmacro MUI_UNPAGE_INSTFILES

;Languages
{{#each languages}}
!insertmacro MUI_LANGUAGE "{{this}}"
{{/each}}
!insertmacro MUI_RESERVEFILE_LANGDLL
{{#each language_files}}
  !include "{{this}}"
{{/each}}

; Custom language strings for service installation
LangString installService ${LANG_ENGLISH} "Install Blue Onyx as Windows Service"
LangString serviceInstallSuccess ${LANG_ENGLISH} "Blue Onyx Service installed successfully!"
LangString serviceInstallFailed ${LANG_ENGLISH} "Failed to install Blue Onyx Service. You can install it manually later using install_service.ps1"
LangString serviceUninstallSuccess ${LANG_ENGLISH} "Previous Blue Onyx Service uninstalled successfully!"
LangString serviceUninstallFailed ${LANG_ENGLISH} "Failed to uninstall previous Blue Onyx Service. Continuing with installation..."

!macro SetContext
  !if "${INSTALLMODE}" == "currentUser"
    SetShellVarContext current
  !else if "${INSTALLMODE}" == "perMachine"
    SetShellVarContext all
  !endif

  ${If} ${RunningX64}
    !if "${ARCH}" == "x64"
      SetRegView 64
    !else if "${ARCH}" == "arm64"
      SetRegView 64
    !else
      SetRegView 32
    !endif
  ${EndIf}
!macroend

Var PassiveMode

Function .onInit
  ${GetOptions} $CMDLINE "/P" $PassiveMode
  IfErrors +2 0
    StrCpy $PassiveMode 1
  ; Initialize logging
  GetTempFileName $LogFile "$TEMP"
  StrCpy $TempDir "$TEMP"
  Delete $LogFile
  StrCpy $LogFile "$TEMP\BlueOnyx_Install.log"
  DetailPrint "=== Blue Onyx Installation Started ==="
  DetailPrint "Log file: $LogFile"
  DetailPrint "Installation directory: $INSTDIR"
  DetailPrint "Command line: $CMDLINE"

  !if "${DISPLAYLANGUAGESELECTOR}" == "true"
    !insertmacro MUI_LANGDLL_DISPLAY
  !endif

  !insertmacro SetContext

  ${If} $INSTDIR == ""
    ; Set default install location
    !if "${INSTALLMODE}" == "perMachine"
      ${If} ${RunningX64}
        !if "${ARCH}" == "x64"
          StrCpy $INSTDIR "$PROGRAMFILES64\${PRODUCTNAME}"
        !else if "${ARCH}" == "arm64"
          StrCpy $INSTDIR "$PROGRAMFILES64\${PRODUCTNAME}"
        !else
          StrCpy $INSTDIR "$PROGRAMFILES\${PRODUCTNAME}"
        !endif
      ${Else}
        StrCpy $INSTDIR "$PROGRAMFILES\${PRODUCTNAME}"
      ${EndIf}
    !else if "${INSTALLMODE}" == "currentUser"
      StrCpy $INSTDIR "$LOCALAPPDATA\${PRODUCTNAME}"
    !endif

    Call RestorePreviousInstallLocation
  ${EndIf}


  !if "${INSTALLMODE}" == "both"
    !insertmacro MULTIUSER_INIT
  !endif
FunctionEnd


Section EarlyChecks
  ; Abort silent installer if downgrades is disabled
  !if "${ALLOWDOWNGRADES}" == "false"
  IfSilent 0 silent_downgrades_done
    ; If downgrading
    ${If} $R0 == -1
      System::Call 'kernel32::AttachConsole(i -1)i.r0'
      ${If} $0 != 0
        System::Call 'kernel32::GetStdHandle(i -11)i.r0'
        System::call 'kernel32::SetConsoleTextAttribute(i r0, i 0x0004)' ; set red color
        FileWrite $0 "$(silentDowngrades)"
      ${EndIf}
      Abort
    ${EndIf}
  silent_downgrades_done:
  !endif

SectionEnd

{{#if preinstall_section}}
{{unescape_newlines preinstall_section}}
{{/if}}

!macro CheckIfAppIsRunning
  nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
  Pop $R0
  ${If} $R0 = 0
      IfSilent kill 0
      ${IfThen} $PassiveMode != 1 ${|} MessageBox MB_OKCANCEL "$(appRunningOkKill)" IDOK kill IDCANCEL cancel ${|}
      kill:
        nsis_tauri_utils::KillProcess "${MAINBINARYNAME}.exe"
        Pop $R0
        Sleep 500
        ${If} $R0 = 0
          Goto app_check_done
        ${Else}
          IfSilent silent ui
          silent:
            System::Call 'kernel32::AttachConsole(i -1)i.r0'
            ${If} $0 != 0
              System::Call 'kernel32::GetStdHandle(i -11)i.r0'
              System::call 'kernel32::SetConsoleTextAttribute(i r0, i 0x0004)' ; set red color
              FileWrite $0 "$(appRunning)$\n"
            ${EndIf}
            Abort
          ui:
            Abort "$(failedToKillApp)"
        ${EndIf}
      cancel:
        Abort "$(appRunning)"
  ${EndIf}
  app_check_done:
!macroend

Section Install
  SetDetailsPrint both
  DetailPrint "=== STARTING FILE INSTALLATION ==="
  DetailPrint "Target directory: $INSTDIR"

  ; Clean up any existing service before installation (for modify/upgrade scenarios)
  DetailPrint "=== INSTALLER: Pre-installation service cleanup ==="
  nsExec::ExecToStack 'sc.exe query BlueOnyxService'
  Pop $0
  Pop $1
  ${If} $0 == 0
    DetailPrint "Existing Blue Onyx Service detected, cleaning up..."

    ; Stop the service
    nsExec::ExecToStack 'net stop BlueOnyxService'
    Pop $0
    Pop $1
    DetailPrint "Service stop attempt: exit code $0"

    Sleep 2000

    ; Force stop processes
    nsExec::ExecToStack 'taskkill /f /im blue_onyx_service.exe 2>nul'
    Pop $0
    Pop $1

    ; Remove service
    nsExec::ExecToStack 'sc.exe delete BlueOnyxService'
    Pop $0
    Pop $1
    DetailPrint "Service deletion: exit code $0"
  ${Else}
    DetailPrint "No existing Blue Onyx Service found"
  ${EndIf}
  DetailPrint "=== INSTALLER: Pre-installation service cleanup completed ==="

  SetOutPath $INSTDIR

  !insertmacro CheckIfAppIsRunning

  ; Copy main executable
  DetailPrint "Installing main executable: ${MAINBINARYNAME}"
  File "${MAINBINARYSRCPATH}"
  DetailPrint "Main executable installed successfully"
  ; Create resources directory structure
  DetailPrint "Creating resource directories..."
  {{#each resources_dirs}}
    DetailPrint "Creating directory: $INSTDIR\\{{this}}"
    CreateDirectory "$INSTDIR\\{{this}}"
  {{/each}}

  ; Copy resources
  DetailPrint "Installing resource files..."
  {{#each resources}}
    DetailPrint "Installing resource: {{this}} from {{@key}}"
    File /a "/oname={{this}}" "{{@key}}"
  {{/each}}
  DetailPrint "Resource files installation complete"

  ; Copy external binaries
  DetailPrint "Installing additional binaries..."
  {{#each binaries}}
    DetailPrint "Installing binary: {{this}} from {{@key}}"
    File /a "/oname={{this}}" "{{@key}}"
  {{/each}}
  DetailPrint "Additional binaries installation complete"

   ; Create file associations
  {{#each file_associations as |association| ~}}
    {{#each association.ext as |ext| ~}}
       !insertmacro APP_ASSOCIATE "{{ext}}" "{{or association.name ext}}" "{{association-description association.description ext}}" "$INSTDIR\${MAINBINARYNAME}.exe,0" "Open with ${PRODUCTNAME}" "$INSTDIR\${MAINBINARYNAME}.exe $\"%1$\""
    {{/each}}
  {{/each}}

  ; Register deep links
  {{#each deep_link_protocols as |protocol| ~}}
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}" "URL Protocol" ""
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}" "" "URL:${BUNDLEID} protocol"
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}\DefaultIcon" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}\shell\open\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
  {{/each}}

  ; Create uninstaller
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; Save $INSTDIR in registry for future installations
  WriteRegStr SHCTX "${MANUPRODUCTKEY}" "" $INSTDIR

  !if "${INSTALLMODE}" == "both"
    ; Save install mode to be selected by default for the next installation such as updating
    ; or when uninstalling
    WriteRegStr SHCTX "${UNINSTKEY}" $MultiUser.InstallMode 1
  !endif

  ; Registry information for add/remove programs
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayName" "${PRODUCTNAME}"
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayIcon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\""
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr SHCTX "${UNINSTKEY}" "Publisher" "${MANUFACTURER}"
  WriteRegStr SHCTX "${UNINSTKEY}" "InstallLocation" "$\"$INSTDIR$\""
  WriteRegStr SHCTX "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegDWORD SHCTX "${UNINSTKEY}" "NoModify" "1"
  WriteRegDWORD SHCTX "${UNINSTKEY}" "NoRepair" "1"
  WriteRegDWORD SHCTX "${UNINSTKEY}" "EstimatedSize" "${ESTIMATEDSIZE}"  ; Create shortcuts only if not installing as service only
  ${If} $InstallServiceState != 1
    ; Create start menu shortcut (GUI)
    !insertmacro MUI_STARTMENU_WRITE_BEGIN Application
      Call CreateStartMenuShortcut
      ; Save the start menu folder for future cleanup
      WriteRegStr SHCTX "${MANUPRODUCTKEY}" "Start Menu Folder" $AppStartMenuFolder
    !insertmacro MUI_STARTMENU_WRITE_END

    ; Create shortcuts for silent and passive installers, which
    ; can be disabled by passing `/NS` flag
    ; GUI installer has buttons for users to control creating them
    IfSilent check_ns_flag 0
    ${IfThen} $PassiveMode == 1 ${|} Goto check_ns_flag ${|}
    Goto shortcuts_done
    check_ns_flag:
      ${GetOptions} $CMDLINE "/NS" $R0
      IfErrors 0 shortcuts_done
        Call CreateDesktopShortcut
        Call CreateStartMenuShortcut
        ; Save info for silent installs too
        WriteRegStr SHCTX "${MANUPRODUCTKEY}" "Start Menu Folder" $AppStartMenuFolder
    shortcuts_done:
  ${EndIf}  ; Auto close this page for passive mode
  ${IfThen} $PassiveMode == 1 ${|} SetAutoClose true ${|}

  ; Debug: Show the state of InstallServiceState
  DetailPrint "Debug: InstallServiceState = $InstallServiceState"
  ; Install service if checkbox was checked
  ${If} $InstallServiceState == 1
    DetailPrint "Debug: Service installation requested, calling InstallBlueOnyxService"
    Call InstallBlueOnyxService
  ${Else}
    DetailPrint "Debug: Service installation not requested (InstallServiceState = $InstallServiceState)"
  ${EndIf}

  ; Final installation logging
  DetailPrint "=== INSTALLATION COMPLETE ==="
  DetailPrint "Installation directory: $INSTDIR"
  DetailPrint "Service installation requested: $InstallServiceState"
  DetailPrint "Installation logs are displayed above"
  DetailPrint "For detailed troubleshooting, review the installation details above"
SectionEnd

; Install service function
Function InstallBlueOnyxService
  DetailPrint "=== STARTING SERVICE INSTALLATION ==="
  DetailPrint "Install directory: $INSTDIR"

  ; Check admin privileges
  System::Call 'shell32::IsUserAnAdmin()i.r0'
  ${If} $0 == 0
    DetailPrint "WARNING: Not running as administrator - service installation may fail"
  ${Else}
    DetailPrint "Running with administrator privileges - good!"
  ${EndIf}

  ; First check if the service executable exists
  DetailPrint "Checking for service executable..."
  IfFileExists "$INSTDIR\blue_onyx_service.exe" service_file_found service_file_missing

  service_file_missing:
    DetailPrint "ERROR: blue_onyx_service.exe not found in $INSTDIR"
    DetailPrint "Directory contents:"
    FindFirst $0 $1 "$INSTDIR\*.*"
    loop:
      StrCmp $1 "" done
      DetailPrint "  - $1"
      FindNext $0 $1
      Goto loop
    done:
    FindClose $0
    DetailPrint "Service installation aborted."
    Goto service_install_done

  service_file_found:
    DetailPrint "SUCCESS: Service executable found at $INSTDIR\blue_onyx_service.exe"

    ; Get file size and version info
    ${GetSize} "$INSTDIR\blue_onyx_service.exe" "/S=0K" $0 $1 $2
    DetailPrint "Service executable size: $0 KB"

    ; Set service timeout to 10 minutes for model loading
    DetailPrint "Setting service timeout to 10 minutes..."
    nsExec::ExecToStack 'reg add "HKLM\SYSTEM\CurrentControlSet\Control" /v ServicesPipeTimeout /t REG_DWORD /d 600000 /f'
    Pop $0
    Pop $1
    DetailPrint "Registry timeout command exit code: $0"
    DetailPrint "Registry timeout command output: $1"

    ; Create event log source
    DetailPrint "Creating event log source for BlueOnyxService..."
    nsExec::ExecToStack 'powershell.exe -Command "try { New-EventLog -LogName Application -Source BlueOnyxService -ErrorAction SilentlyContinue; Write-Host OK } catch { Write-Host ERROR: Failed to create event log }"'
    Pop $0
    Pop $1
    DetailPrint "Event log creation exit code: $0"
    DetailPrint "Event log creation output: $1"

    ; Check if service already exists and remove it
    DetailPrint "Checking if BlueOnyxService already exists..."
    nsExec::ExecToStack 'sc.exe query BlueOnyxService'
    Pop $0
    Pop $1
    ${If} $0 == 0
      DetailPrint "Existing service found - removing it first..."
      nsExec::ExecToStack 'net stop BlueOnyxService'
      Pop $0
      Pop $1
      DetailPrint "Service stop exit code: $0"

      nsExec::ExecToStack 'sc.exe delete BlueOnyxService'
      Pop $0
      Pop $1
      DetailPrint "Service delete exit code: $0"

      ; Wait a moment for service to be fully removed
      Sleep 2000
    ${Else}
      DetailPrint "No existing service found - proceeding with fresh installation"
    ${EndIf}    ; Install the service
    DetailPrint "Creating Blue Onyx Service..."
    StrCpy $R1 '$INSTDIR\blue_onyx_service.exe'
    DetailPrint "Service executable path: $R1"
    DetailPrint "Running: sc.exe create BlueOnyxService binPath= $R1 start= auto DisplayName= Blue_Onyx_Service obj= LocalSystem"
    nsExec::ExecToStack 'sc.exe create BlueOnyxService binPath= "$R1" start= auto DisplayName= Blue_Onyx_Service obj= LocalSystem'
    Pop $0
    Pop $1
    DetailPrint "Service creation exit code: $0"
    DetailPrint "Service creation output: $1"

    ${If} $0 == 0
      DetailPrint "SUCCESS: $(serviceInstallSuccess)"      ; Configure service type
      DetailPrint "Configuring service type to 'own'..."
      nsExec::ExecToStack 'sc.exe config BlueOnyxService type= own'
      Pop $0
      Pop $1
      DetailPrint "Service config exit code: $0"
      DetailPrint "Service config output: $1"

      ; Verify service was created
      DetailPrint "Verifying service was created..."
      nsExec::ExecToStack 'sc.exe query BlueOnyxService'
      Pop $0
      Pop $1
      DetailPrint "Service query exit code: $0"
      DetailPrint "Service query output: $1"

      ; Start the service
      DetailPrint "Starting Blue Onyx Service..."
      nsExec::ExecToStack 'net start BlueOnyxService'
      Pop $0
      Pop $1
      DetailPrint "Service start exit code: $0"
      DetailPrint "Service start output: $1"
      ${If} $0 == 0
        DetailPrint "SUCCESS: Blue Onyx Service started successfully!"

        ; Double-check service status
        DetailPrint "Verifying service is running..."
        nsExec::ExecToStack 'sc.exe query BlueOnyxService'
        Pop $0
        Pop $1
        DetailPrint "Final service status: $1"
      ${Else}
        DetailPrint "WARNING: Service created but failed to start (exit code: $0)"
        DetailPrint "Start error details: $1"
        DetailPrint "You can start the service manually later using: net start BlueOnyxService"
      ${EndIf}
    ${Else}
      DetailPrint "ERROR: $(serviceInstallFailed)"
      DetailPrint "Service creation failed with exit code: $0"
      DetailPrint "Error details: $1"
      DetailPrint "You can try installing the service manually using the install_service.ps1 script"
    ${EndIf}

  service_install_done:
    DetailPrint "=== SERVICE INSTALLATION COMPLETE ==="
FunctionEnd

; Remove the old InstallService section since we're now using a function
; Section InstallService - REMOVED

Function .onInstSuccess
  ; Check for `/R` flag only in silent and passive installers because
  ; GUI installer has a toggle for the user to (re)start the app
  IfSilent check_r_flag 0
  ${IfThen} $PassiveMode == 1 ${|} Goto check_r_flag ${|}
  Goto run_done
  check_r_flag:
    ${GetOptions} $CMDLINE "/R" $R0
    IfErrors run_done 0
      Exec '"$INSTDIR\${MAINBINARYNAME}.exe"'
  run_done:
FunctionEnd

Function un.ShowUninstFilesDetails
  ; Automatically expand the details section when the uninstall page is shown
  ; This ensures users always see the verbose logging output
  SetDetailsView show
FunctionEnd

Function un.onInit
  !insertmacro SetContext

  !if "${INSTALLMODE}" == "both"
    !insertmacro MULTIUSER_UNINIT
  !endif

  !insertmacro MUI_UNGETLANGUAGE
FunctionEnd

Section Uninstall
  SetDetailsPrint both
  !insertmacro CheckIfAppIsRunning
  ; Always stop and remove the Blue Onyx Service during uninstall (complete cleanup)
  DetailPrint "=== UNINSTALLER: Starting service cleanup ==="
  DetailPrint "Checking for Blue Onyx Service..."
  nsExec::ExecToStack 'sc.exe query BlueOnyxService'
  Pop $0
  Pop $1
  DetailPrint "Service query result: exit code $0"
  ${If} $0 == 0
    DetailPrint "Blue Onyx Service found! Proceeding with cleanup..."
    DetailPrint "Service status output: $1"

    DetailPrint "Attempting to stop Blue Onyx Service..."
    nsExec::ExecToStack 'net stop BlueOnyxService'
    Pop $0
    Pop $1
    DetailPrint "Service stop result: exit code $0"
    ${If} $0 == 0
      DetailPrint "Blue Onyx Service stopped successfully"
    ${Else}
      DetailPrint "Service stop failed or service was already stopped (code: $0)"
      DetailPrint "Stop output: $1"
    ${EndIf}

    DetailPrint "Attempting to remove Blue Onyx Service..."
    nsExec::ExecToStack 'sc.exe delete BlueOnyxService'
    Pop $0
    Pop $1
    DetailPrint "Service deletion result: exit code $0"
    ${If} $0 == 0
      DetailPrint "Blue Onyx Service removed successfully!"
    ${Else}
      DetailPrint "Service deletion failed (code: $0)"
      DetailPrint "Deletion output: $1"
    ${EndIf}
  ${Else}
    DetailPrint "No Blue Onyx Service found (exit code: $0)"
  ${EndIf}
  DetailPrint "=== UNINSTALLER: Service cleanup completed ==="
  ; Remove event log source if it exists
  DetailPrint "=== UNINSTALLER: Cleaning up event log source ==="
  nsExec::ExecToStack 'powershell.exe -Command "try { Remove-EventLog -Source BlueOnyxService -ErrorAction SilentlyContinue; Write-Output \"Event log source removed\" } catch { Write-Output \"No event log source found\" }"'
  Pop $0
  Pop $1
  DetailPrint "Event log cleanup result: exit code $0"
  DetailPrint "Event log cleanup output: $1"

  ; Delete all Blue Onyx files and directories
  DetailPrint "Removing Blue Onyx files..."

  ; Delete the main executable
  Delete "$INSTDIR\${MAINBINARYNAME}.exe"

  ; Delete all resources
  {{#each resources}}
    Delete "$INSTDIR\\{{this}}"
  {{/each}}

  ; Delete all external binaries
  {{#each binaries}}
    Delete "$INSTDIR\\{{this}}"
  {{/each}}  ; Delete any additional files that might exist (but preserve config files)
  Delete "$INSTDIR\*.dll"
  Delete "$INSTDIR\*.pdb"
  Delete "$INSTDIR\*.log"
  ; Do NOT delete *.yaml, *.json, or *.onnx files to preserve user configurations and models
  Delete "$INSTDIR\*.md"
  Delete "$INSTDIR\*.txt"
  Delete "$INSTDIR\uninstall.exe"

  ; Remove all subdirectories and their contents
  {{#each resources_dirs}}
  RMDir /r /REBOOTOK "$INSTDIR\\{{this}}"
  {{/each}}
  ; Remove scripts directory if it exists
  RMDir /r /REBOOTOK "$INSTDIR\scripts"

  ; Do NOT remove models directory to preserve ONNX files and user models

  ; Remove any other subdirectories (except models and config)
  RMDir /r /REBOOTOK "$INSTDIR\assets"
  ; Do NOT remove config directory to preserve user configurations
  RMDir /r /REBOOTOK "$INSTDIR\logs"

  ; Finally remove the main installation directory
  RMDir /REBOOTOK "$INSTDIR"

  ; Remove ALL shortcuts and start menu entries
  DetailPrint "Removing shortcuts and start menu entries..."

  ; Remove desktop shortcut
  IfFileExists "$DESKTOP\${PRODUCTNAME}.lnk" 0 +3
    Delete "$DESKTOP\${PRODUCTNAME}.lnk"
    DetailPrint "Removed desktop shortcut"

  ; Remove start menu shortcuts (check both current and saved folder)
  !insertmacro MUI_STARTMENU_GETFOLDER Application $AppStartMenuFolder
  ${If} $AppStartMenuFolder != ""
    Delete "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
    RMDir "$SMPROGRAMS\$AppStartMenuFolder"
    DetailPrint "Removed start menu folder: $AppStartMenuFolder"
  ${Else}
    ; Try to get it from registry if not available
    ReadRegStr $R3 SHCTX "${MANUPRODUCTKEY}" "Start Menu Folder"
    ${If} $R3 != ""
      Delete "$SMPROGRAMS\$R3\${PRODUCTNAME}.lnk"
      RMDir "$SMPROGRAMS\$R3"
      DetailPrint "Removed start menu folder from registry: $R3"
    ${EndIf}
  ${EndIf}

  ; Also check for common start menu locations in case folder name changed
  Delete "$SMPROGRAMS\${PRODUCTNAME}\${PRODUCTNAME}.lnk"
  RMDir "$SMPROGRAMS\${PRODUCTNAME}"
  Delete "$SMPROGRAMS\Blue Onyx\Blue Onyx.lnk"
  RMDir "$SMPROGRAMS\Blue Onyx"

  ; Remove file associations
  {{#each file_associations as |association| ~}}
    {{#each association.ext as |ext| ~}}
      !insertmacro APP_UNASSOCIATE "{{ext}}" "{{or association.name ext}}"
    {{/each}}
  {{/each}}

  ; Remove deep link protocols
  {{#each deep_link_protocols as |protocol| ~}}
    ReadRegStr $R7 SHCTX "Software\Classes\\{{protocol}}\shell\open\command" ""
    StrCmp $R7 "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\"" 0 +2
      DeleteRegKey SHCTX "Software\Classes\\{{protocol}}"
  {{/each}}

  ; Remove ALL registry entries for add/remove programs
  DetailPrint "Cleaning up registry entries..."
  !if "${INSTALLMODE}" == "both"
    DeleteRegKey SHCTX "${UNINSTKEY}"
  !else if "${INSTALLMODE}" == "perMachine"
    DeleteRegKey HKLM "${UNINSTKEY}"
  !else
    DeleteRegKey HKCU "${UNINSTKEY}"
  !endif

  ; Remove manufacturer/product registry keys completely
  DeleteRegKey SHCTX "${MANUPRODUCTKEY}"
  DeleteRegValue HKCU "${MANUPRODUCTKEY}" "Installer Language"

  ; Remove Blue Onyx registry keys from both HKLM and HKCU to be thorough
  DeleteRegKey HKLM "Software\blue-onyx"
  DeleteRegKey HKCU "Software\blue-onyx"
  DeleteRegKey HKLM "Software\Blue Onyx"
  DeleteRegKey HKCU "Software\Blue Onyx"

  ; Clean up app data if user chose to
  {{#if appdata_paths}}
  ${If} $DeleteAppDataCheckboxState == 1
      SetShellVarContext current
      DetailPrint "Removing application data..."
      {{#each appdata_paths}}
      RmDir /r "{{unescape_dollar_sign this}}"
      {{/each}}
  ${EndIf}
  {{/if}}  ; Clean up common Blue Onyx data locations
  DetailPrint "Cleaning up additional data locations..."
  RmDir /r "$LOCALAPPDATA\blue-onyx"
  RmDir /r "$APPDATA\blue-onyx"

  ${GetOptions} $CMDLINE "/P" $R0
  IfErrors +2 0
    SetAutoClose true

  DetailPrint "Blue Onyx has been completely removed from your system"
SectionEnd

Function RestorePreviousInstallLocation
  ReadRegStr $4 SHCTX "${MANUPRODUCTKEY}" ""
  StrCmp $4 "" +2 0
    StrCpy $INSTDIR $4
FunctionEnd

Function ShowInstFilesDetails
  ; Automatically expand the details section when the installation page is shown
  ; This ensures users always see the verbose logging output
  SetDetailsView show
FunctionEnd

Function SkipIfPassive
  ${IfThen} $PassiveMode == 1  ${|} Abort ${|}
FunctionEnd

Function CreateDesktopShortcut
  CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
  ApplicationID::Set "$DESKTOP\${PRODUCTNAME}.lnk" "${IDENTIFIER}"
FunctionEnd

Function CreateDesktopShortcutConditional
  ; Only create desktop shortcut if not installing as service only
  ${If} $InstallServiceState != 1
    Call CreateDesktopShortcut
  ${EndIf}
FunctionEnd

Function RunAppConditional
  ; Check if installing as service
  ${If} $InstallServiceState == 1    ; Installing as service - open browser to web interface
    ; Check for service config file to get the port
    IfFileExists "$INSTDIR\blue_onyx_config_service.json" ConfigExists UseDefaultPort

    ConfigExists:
      ; Write debug info to a log file for troubleshooting
      FileOpen $3 "$INSTDIR\port_debug.log" w
      FileWrite $3 "=== PORT PARSING DEBUG ===$\r$\n"
      FileWrite $3 "Install directory: $INSTDIR$\r$\n"
      FileWrite $3 "Looking for: blue_onyx_config_service.json$\r$\n"
      FileClose $3
        ; Use PowerShell to extract port from service config with detailed logging
      DetailPrint "Found blue_onyx_config_service.json, parsing port..."
      nsExec::ExecToStack 'powershell.exe -Command "try { $$configFile = \"$INSTDIR\\blue_onyx_config_service.json\"; Add-Content \"$INSTDIR\\port_debug.log\" \"PowerShell: Checking file: $$configFile\"; $$exists = Test-Path $$configFile; Add-Content \"$INSTDIR\\port_debug.log\" \"PowerShell: File exists: $$exists\"; if ($$exists) { $$content = Get-Content $$configFile -Raw; $$contentLen = $$content.Length; Add-Content \"$INSTDIR\\port_debug.log\" \"PowerShell: File content length: $$contentLen\"; $$config = $$content | ConvertFrom-Json; $$portValue = $$config.port; Add-Content \"$INSTDIR\\port_debug.log\" \"PowerShell: Parsed port: $$portValue\"; Write-Output $$portValue.ToString().Trim() } else { Add-Content \"$INSTDIR\\port_debug.log\" \"PowerShell: File not found, using default\"; Write-Output \"32168\" } } catch { Add-Content \"$INSTDIR\\port_debug.log\" \"PowerShell: Error: $$_\"; Write-Output \"32168\" }"'
      Pop $0 ; Exit code
      Pop $1 ; Output (port number)

      ; Log the results
      FileOpen $3 "$INSTDIR\port_debug.log" a
      FileWrite $3 "PowerShell exit code: $0$\r$\n"
      FileWrite $3 "PowerShell output: '$1'$\r$\n"
      FileClose $3

      DetailPrint "Port parsing result: exit code $0, port: '$1'"
      ${If} $0 == 0
      ${AndIf} $1 != ""
        ; Successfully got port from config
        DetailPrint "Using port $1 from service config"
        ExecShell "open" "http://127.0.0.1:$1"
        Goto RunAppDone
      ${EndIf}

    UseDefaultPort:
      ; Fallback to default service port
      DetailPrint "Using default service port 32168"
      ExecShell "open" "http://127.0.0.1:32168"
      Goto RunAppDone
  ${Else}
    ; Not installing as service - run the main executable directly
    DetailPrint "Running Blue Onyx CLI application"
    Exec '"$INSTDIR\${MAINBINARYNAME}.exe"'
  ${EndIf}

  RunAppDone:
FunctionEnd

Function CreateStartMenuShortcut
  CreateDirectory "$SMPROGRAMS\$AppStartMenuFolder"
  CreateShortcut "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
  ApplicationID::Set "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "${IDENTIFIER}"
FunctionEnd
