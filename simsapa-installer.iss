; Simsapa Dhamma Reader - Inno Setup Installer Script
; This script creates a Windows installer for the Simsapa application

#ifndef AppVersion
  #define AppVersion "0.1.8"
#endif

#ifndef DistDir
  #define DistDir ".\dist"
#endif

#define AppName "Simsapa"
#define AppPublisher "Profound Labs"
#define AppURL "https://simsapa.github.io/"
#define AppExeName "simsapadhammareader.exe"
#define AppId "{{B8F8C8A0-9F8E-4E5A-8C9D-1E2F3A4B5C6D}"

[Setup]
; NOTE: The value of AppId uniquely identifies this application.
; Do not use the same AppId value in installers for other applications.
AppId={#AppId}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}
AppUpdatesURL={#AppURL}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
AllowNoIcons=yes
; Always show the "Select Destination Location" page and do not reuse a folder
; remembered from a previous install. Without these, when a prior install of the
; same AppId is detected Inno auto-hides the directory page and silently reuses
; the old path - which meant a Portable re-install never offered the folder
; (Desktop/USB/External SSD) chooser and CurPageChanged's per-mode default never applied.
DisableDirPage=no
UsePreviousAppDir=no
; Uncomment the following line if you have a LICENSE file
;LicenseFile=LICENSE
; Run with the user's own privileges by default: no UAC prompt and no "Select
; Setup Install Mode" dialog at startup, in any launch (normal or "Run as
; administrator"). The install location is chosen explicitly on the custom mode
; page instead. Only the "Standard - all users" option needs administrator
; rights; the user supplies those by launching the installer with "Run as
; administrator" (the mode page warns and blocks that option otherwise). The
; installed app never needs administrator rights at runtime.
PrivilegesRequired=lowest
; Portable installs register no uninstaller (no Add/Remove Programs entry);
; Standard installs remain fully uninstallable. ShouldRunStandard is a [Code]
; Boolean function (= not IsPortable), evaluated after the mode page.
Uninstallable=ShouldRunStandard
OutputDir=.
OutputBaseFilename=Simsapa-Setup-{#AppVersion}
SetupIconFile=assets\icons\appicons\simsapa.ico
Compression=lzma
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Uninstaller should remove user data (app databases downloaded to user folder)
UninstallDisplayIcon={app}\{#AppExeName}
UninstallFilesDir={app}\uninst

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
; Standard-only: Portable mode creates its launcher in the parent folder and
; would otherwise show a desktop-icon checkbox that does nothing.
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked; Check: ShouldRunStandard

[Files]
; Main executable
Source: "{#DistDir}\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion

; Qt libraries and dependencies (deployed by windeployqt)
Source: "{#DistDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs; Excludes: "{#AppExeName}"

; Application icon
Source: "assets\icons\appicons\simsapa.ico"; DestDir: "{app}"; Flags: ignoreversion

; Visual C++ Redistributable (bundled for offline installation)
; Download from: https://aka.ms/vs/17/release/vc_redist.x64.exe
; Place in redist\ folder before building installer
Source: "redist\vc_redist.x64.exe"; DestDir: "{tmp}"; Flags: ignoreversion deleteafterinstall; Check: VCRedistNeedsInstall

[Icons]
; Standard-only icons. Portable mode creates its launcher in the parent folder
; (see [Code]) and registers no uninstaller, so these are gated to Standard.
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExeName}"; IconFilename: "{app}\simsapa.ico"; Check: ShouldRunStandard
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"; Check: ShouldRunStandard
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; IconFilename: "{app}\simsapa.ico"; Tasks: desktopicon; Check: ShouldRunStandard

[Run]
; Install Visual C++ Redistributable silently if needed (runs before app launch)
Filename: "{tmp}\vc_redist.x64.exe"; Parameters: "/install /quiet /norestart"; StatusMsg: "Installing Visual C++ Redistributable..."; Flags: waituntilterminated; Check: VCRedistNeedsInstall
; Launch application after installation
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(AppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[Code]
var
  ShouldDeleteUserData: Boolean;
  // Install-mode selection page, shown after Welcome. Built from custom controls
  // so the option titles can be shown in bold and each can list its default path.
  // Three options: Standard (all users), Standard (this user only), Portable.
  ModePage: TWizardPage;
  AllUsersRadio: TNewRadioButton;
  ThisUserRadio: TNewRadioButton;
  PortableRadio: TNewRadioButton;
  // True when the user chose Portable install on ModePage. The two Standard
  // options are distinguished by AllUsersRadio.Checked / ThisUserRadio.Checked.
  IsPortable: Boolean;
  // Launcher-type page (Portable only): .lnk shortcut vs .cmd relative launcher.
  LauncherPage: TInputOptionWizardPage;
  // True when the user chose the .cmd launcher; False for the .lnk shortcut.
  LauncherIsCmd: Boolean;

// Helper used by [Files]/[Icons]/[Run] Check: parameters to gate entries on the
// chosen install mode. Returns True only for a Portable install.
function ShouldRunPortable: Boolean;
begin
  Result := IsPortable;
end;

// Inverse of ShouldRunPortable, for entries that apply only to Standard install.
function ShouldRunStandard: Boolean;
begin
  Result := not IsPortable;
end;

// The portable data folder: a sibling of the install ({app}) folder, named by
// appending 'Data' to the install folder name (e.g. ...\Simsapa -> ...\SimsapaData).
function GetPortableDataDir: String;
var
  AppDir: String;
begin
  AppDir := ExpandConstant('{app}');
  Result := ExtractFileDir(AppDir) + '\' + ExtractFileName(AppDir) + 'Data';
end;

// Check if Visual C++ Redistributable is installed
function VCRedistNeedsInstall: Boolean;
var
  Version: String;
begin
  // Check for Visual C++ 2015-2022 Redistributable (x64)
  // The registry key varies by version, but we check for the unified runtime
  if RegQueryStringValue(HKLM64, 'SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64', 'Version', Version) then
    Result := False
  else if RegQueryStringValue(HKLM, 'SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64', 'Version', Version) then
    Result := False
  else
    Result := True;
end;

// Check if VC++ Redistributable installer is bundled
function VCRedistBundled: Boolean;
begin
  Result := FileExists(ExpandConstant('{src}\redist\vc_redist.x64.exe'));
end;

// Add one install-mode option to ModePage: a bold radio-button title, a wrapped
// normal-weight description, and (when Note <> '') a bold note line below it
// (used to flag that the option's default folder already holds an install).
// Advances NextTop past the block so the next option stacks below it.
procedure AddModeOption(var Radio: TNewRadioButton; var NextTop: Integer; Title, Desc, Note: String);
var
  DescLabel: TNewStaticText;
  NoteLabel: TNewStaticText;
  BlockBottom: Integer;
begin
  Radio := TNewRadioButton.Create(ModePage);
  Radio.Parent := ModePage.Surface;
  Radio.Caption := Title;
  Radio.Font.Style := [fsBold];
  Radio.Left := 0;
  Radio.Top := NextTop;
  Radio.Width := ModePage.SurfaceWidth;
  Radio.Height := ScaleY(17);

  DescLabel := TNewStaticText.Create(ModePage);
  DescLabel.Parent := ModePage.Surface;
  DescLabel.AutoSize := True;
  DescLabel.WordWrap := True;
  DescLabel.Left := ScaleX(16);
  DescLabel.Top := Radio.Top + Radio.Height + ScaleY(2);
  DescLabel.Width := ModePage.SurfaceWidth - ScaleX(16);
  // Caption set last so the wrapped height is computed against the final Width.
  DescLabel.Caption := Desc;

  BlockBottom := DescLabel.Top + DescLabel.Height;

  if Note <> '' then
  begin
    NoteLabel := TNewStaticText.Create(ModePage);
    NoteLabel.Parent := ModePage.Surface;
    NoteLabel.AutoSize := True;
    NoteLabel.WordWrap := True;
    NoteLabel.Font.Style := [fsBold];
    NoteLabel.Left := ScaleX(16);
    NoteLabel.Top := BlockBottom + ScaleY(2);
    NoteLabel.Width := ModePage.SurfaceWidth - ScaleX(16);
    NoteLabel.Caption := Note;
    BlockBottom := NoteLabel.Top + NoteLabel.Height;
  end;

  NextTop := BlockBottom + ScaleY(16);
end;

// Build the "already installed here" note for a Standard option's default
// folder, or '' when no install is present there. Helps a re-installing user
// recognise which location already has Simsapa.
function ExistingInstallNote(DefaultDir: String): String;
begin
  if FileExists(DefaultDir + '\{#AppExeName}') then
    Result := 'There is already a Simsapa version in ' + DefaultDir +
              ' - installing will update it.'
  else
    Result := '';
end;

// Initialize the wizard with custom pages
procedure InitializeWizard;
var
  WarningPage: TOutputMsgWizardPage;
  NextTop: Integer;
begin
  // Install-type selection page, shown early (after Welcome, before the
  // directory page). Built from custom controls so the option titles are bold
  // and each option lists the default folder it installs to, which helps the
  // user pick the same location when re-installing. A Standard option is always
  // the default so users who do not expect portable mode are not surprised.
  ModePage := CreateCustomPage(wpWelcome,
    'Installation Type',
    'Choose how Simsapa should be installed.');

  NextTop := ScaleY(8);
  AddModeOption(AllUsersRadio, NextTop,
    'Standard Install - all users',
    'Installs to ' + ExpandConstant('{commonpf}\Simsapa') + ' for everyone on ' +
    'this computer. Requires administrator rights: launch the installer with ' +
    '"Run as administrator" to use this option.',
    ExistingInstallNote(ExpandConstant('{commonpf}\Simsapa')));
  AddModeOption(ThisUserRadio, NextTop,
    'Standard Install - this user only',
    'Installs to ' + ExpandConstant('{localappdata}\Programs\Simsapa') + ' for ' +
    'your account only. No administrator rights required.',
    ExistingInstallNote(ExpandConstant('{localappdata}\Programs\Simsapa')));
  AddModeOption(PortableRadio, NextTop,
    'Portable Install',
    'Installs into any folder you choose (default ' +
    ExpandConstant('{userdesktop}\Simsapa') + ') and keeps ' +
    'all data in a folder next to the app, so it can travel with you. No administrator rights required. ' +
    'For using Simsapa from an external USB drive (SSD recommended for disk read speed), it is faster to install to a folder in the Desktop first and selecting the .cmd launcher option, starting the app so that it downloads the databases, and then move the files to the USB drive (Simsapa/ and SimsapaData/ folders and the Simsapa.cmd launcher).',
    '');

  // Default to all-users when the installer was launched elevated, otherwise to
  // this-user (always works without admin). Portable is never the default.
  if IsAdmin() then
    AllUsersRadio.Checked := True
  else
    ThisUserRadio.Checked := True;
  IsPortable := False;

  // Launcher-type page (shown after the directory page, Portable only via
  // ShouldSkipPage). The launcher is placed in the parent of the install
  // folder (e.g. the Desktop or USB root) and starts the installed exe.
  LauncherPage := CreateInputOptionPage(wpSelectDir,
    'Portable Launcher',
    'Choose how to create the launcher next to your install folder.',
    'The launcher is placed in the parent folder (e.g. your Desktop or the USB ' +
    'drive root) and starts Simsapa. Select a type, then click Next.',
    True {Exclusive radio buttons}, False {not a list box});
  LauncherPage.Add(
    'Batch launcher (.cmd) - recommended for USB drives. Resolves the app ' +
    'relative to its own location, so it keeps working when the drive letter ' +
    'changes on another computer.');
  LauncherPage.Add(
    'Shortcut (.lnk) - simplest, uses the app icon. May stop working if a USB ' +
    'drive is given a different drive letter on another computer.');
  LauncherPage.SelectedValueIndex := 0;
  LauncherIsCmd := True;

  // Add a warning page if VC++ Redistributable is needed but NOT bundled
  // If bundled, it will be installed silently - no warning needed
  // Placed after ModePage so the wizard order is Welcome -> Mode -> (VC warning).
  if VCRedistNeedsInstall and not VCRedistBundled then
  begin
    WarningPage := CreateOutputMsgPage(ModePage.ID,
      'Visual C++ Redistributable Required',
      'Additional runtime required',
      'This application requires the Microsoft Visual C++ Redistributable to run.' + #13#10#13#10 +
      'If you encounter errors starting the application after installation, ' +
      'please download and install the Visual C++ Redistributable from:' + #13#10#13#10 +
      'https://aka.ms/vs/17/release/vc_redist.x64.exe' + #13#10#13#10 +
      'Click Next to continue with the installation.');
  end;
end;

// Capture the install-mode choice when leaving the mode page.
function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if CurPageID = ModePage.ID then
  begin
    IsPortable := PortableRadio.Checked;
    // "All users" installs to Program Files and needs administrator rights. If
    // the installer was not launched elevated, warn and keep the user on the
    // page so they can re-run as administrator or pick a no-admin option.
    if AllUsersRadio.Checked and (not IsAdmin()) then
    begin
      MsgBox('Installing for all users places Simsapa in ' +
        ExpandConstant('{commonpf}\Simsapa') + ' and requires administrator ' +
        'rights. Please re-run the installer using "Run as administrator", or ' +
        'choose "Standard Install - this user only" or "Portable Install".',
        mbInformation, MB_OK);
      Result := False;
    end;
  end;
  if CurPageID = LauncherPage.ID then
    LauncherIsCmd := (LauncherPage.SelectedValueIndex = 0);
end;

// Decide which wizard pages to skip per mode:
// - The launcher-type page only applies to Portable installs.
// - In Portable mode the "Select Start Menu Folder" page is meaningless (no
//   group icons are created, and the launcher goes in the parent folder), so
//   skip it to avoid confusing the user.
function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := False;
  if PageID = LauncherPage.ID then
    Result := not IsPortable;
  if PageID = wpSelectProgramGroup then
    Result := IsPortable;
end;

// Build the "Ready to Install" memo. In Portable mode the Start Menu folder
// section is omitted: no Start Menu group is created (its page is skipped and
// the [Icons] are gated to Standard), so listing it would be misleading.
function UpdateReadyMemo(Space, NewLine, MemoUserInfoInfo, MemoDirInfo, MemoTypeInfo, MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
var
  S: String;
begin
  S := '';
  if MemoUserInfoInfo <> '' then
    S := S + MemoUserInfoInfo + NewLine + NewLine;
  if MemoDirInfo <> '' then
    S := S + MemoDirInfo + NewLine + NewLine;
  if MemoTypeInfo <> '' then
    S := S + MemoTypeInfo + NewLine + NewLine;
  if MemoComponentsInfo <> '' then
    S := S + MemoComponentsInfo + NewLine + NewLine;
  if (not IsPortable) and (MemoGroupInfo <> '') then
    S := S + MemoGroupInfo + NewLine + NewLine;
  if MemoTasksInfo <> '' then
    S := S + MemoTasksInfo + NewLine + NewLine;
  Result := S;
end;

// When entering the directory page, suggest the default folder for the chosen
// mode (matching the paths shown on the mode page). Explicit folder constants
// are used rather than {autopf} because, in the lowest-privileges install mode,
// {autopf} would always resolve to the per-user location.
procedure CurPageChanged(CurPageID: Integer);
begin
  if CurPageID = wpSelectDir then
  begin
    if PortableRadio.Checked then
      WizardForm.DirEdit.Text := ExpandConstant('{userdesktop}\Simsapa')
    else if AllUsersRadio.Checked then
      WizardForm.DirEdit.Text := ExpandConstant('{commonpf}\Simsapa')
    else
      WizardForm.DirEdit.Text := ExpandConstant('{localappdata}\Programs\Simsapa');
  end;
end;

// After files are installed, for Portable mode: create the sibling data folder
// (reusing it as-is if it already exists, preserving any downloaded databases)
// and write config.txt next to the exe pointing SIMSAPA_DIR at that folder via
// a relative, unquoted, forward-slash path (e.g. SIMSAPA_DIR=../SimsapaData).
procedure CurStepChanged(CurStep: TSetupStep);
var
  AppDir: String;
  DataDir: String;
  ParentDir: String;
  BaseName: String;
  ConfigPath: String;
  ConfigContent: String;
  CmdContent: String;
  LinkResult: String;
begin
  if (CurStep = ssPostInstall) and IsPortable then
  begin
    AppDir := ExpandConstant('{app}');
    ParentDir := ExtractFileDir(AppDir);
    BaseName := ExtractFileName(AppDir);

    DataDir := GetPortableDataDir;
    // Reuse an existing data folder as-is; only create it when absent.
    if not DirExists(DataDir) then
      CreateDir(DataDir);

    // Relative path must match the sibling folder name and use forward slashes
    // (Rust accepts '/' on Windows; dotenvy treats '\' as an escape char).
    ConfigContent := 'SIMSAPA_DIR=../' + BaseName + 'Data';
    ConfigPath := AppDir + '\config.txt';
    SaveStringToFile(ConfigPath, ConfigContent + #13#10, False);

    // Create the launcher in the parent folder. Neither launcher relies on a
    // "Start in"/CWD value: the app locates config.txt via the exe's own
    // directory, so the launcher only needs to start the exe.
    if LauncherIsCmd then
    begin
      // %~dp0 expands to the launcher's own drive+path (with trailing '\'),
      // so the exe is resolved relative to the .cmd's location -> survives
      // USB drive-letter changes. The install subfolder name is BaseName.
      CmdContent :=
        '@echo off' + #13#10 +
        'start "" "%~dp0' + BaseName + '\{#AppExeName}"' + #13#10;
      SaveStringToFile(ParentDir + '\' + BaseName + '.cmd', CmdContent, False);
    end
    else
    begin
      // Standard Windows shortcut pointing at the installed exe, using the app
      // icon. Working directory left empty (config discovery is exe-relative).
      LinkResult := CreateShellLink(
        ParentDir + '\' + BaseName + '.lnk',
        '{#AppName}',
        AppDir + '\{#AppExeName}',
        '', '',
        AppDir + '\simsapa.ico',
        0, SW_SHOWNORMAL);
    end;
  end;
end;

// Get the user data directory where app databases are stored
// Uses app_dirs2 crate convention: AppInfo{name: "simsapa", author: "profound-labs"}
// From backend/src/lib.rs:45: APP_INFO: AppInfo = AppInfo{name: "simsapa", author: "profound-labs"}
// From backend/src/lib.rs:274: get_app_root(AppDataType::UserData, &APP_INFO)
// On Windows, app_dirs2 creates: %LOCALAPPDATA%\{author}\{name}
// Result: %LOCALAPPDATA%\profound-labs\simsapa
// This directory contains:
//   - app-assets/ (appdata.sqlite3, dictionaries, downloaded language databases)
//   - logs/ (application logs)
function GetUserDataDir: String;
begin
  Result := ExpandConstant('{localappdata}\profound-labs\simsapa');
end;

// Called before uninstall begins - ask user about deleting user data
function InitializeUninstall(): Boolean;
var
  UserDataDir: String;
  MsgResult: Integer;
begin
  Result := True;
  ShouldDeleteUserData := False;
  
  if not UninstallSilent then
  begin
    UserDataDir := GetUserDataDir;
    
    if DirExists(UserDataDir) then
    begin
      MsgResult := MsgBox(
        'Simsapa stores downloaded language databases, user settings, and annotations in your user data folder.' + #13#10#13#10 +
        'Location: ' + UserDataDir + #13#10#13#10 +
        'Do you want to delete this data as well?' + #13#10#13#10 +
        'Click Yes to remove the Simsapa app and all user data.' + #13#10 +
        'Click No to remove the Simsapa app but keep user data (you can reinstall the app later without re-downloading).' + #13#10 +
        'Click Cancel to exit and abort uninstallation.',
        mbConfirmation, MB_YESNOCANCEL);
      
      case MsgResult of
        IDYES: ShouldDeleteUserData := True;
        IDNO: ShouldDeleteUserData := False;
        IDCANCEL: Result := False;  // Abort uninstall
      end;
    end;
  end;
end;

// Custom uninstall process to remove user data if requested
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  UserDataDir: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    // Perform deletion if user requested it
    if ShouldDeleteUserData then
    begin
      UserDataDir := GetUserDataDir;
      if DirExists(UserDataDir) then
      begin
        if DelTree(UserDataDir, True, True, True) then
        begin
          if not UninstallSilent then
            MsgBox('User data has been successfully deleted.', mbInformation, MB_OK);
        end
        else
        begin
          if not UninstallSilent then
            MsgBox('Could not delete all user data. Some files may still remain at:' + #13#10#13#10 +
                   UserDataDir + #13#10#13#10 +
                   'You may need to delete them manually.',
                   mbError, MB_OK);
        end;
      end;
    end;
  end;
end;
