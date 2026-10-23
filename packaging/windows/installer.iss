#ifndef AppVersion
  #error AppVersion must be passed by package.ps1
#endif
#ifndef RepoDir
  #error RepoDir must be passed by package.ps1
#endif
#ifndef BinaryDir
  #error BinaryDir must be passed by package.ps1
#endif
#ifndef OutputPath
  #error OutputPath must be passed by package.ps1
#endif

[Setup]
; Keep this ID stable across versions so installing again performs an upgrade.
AppId={{79464B2B-E131-4C56-BD17-2D595182914D}
AppName=VoxelCraft
AppVersion={#AppVersion}
AppPublisher=BrendanH18
AppPublisherURL=https://github.com/BrendanH18/VoxelCraft
AppSupportURL=https://github.com/BrendanH18/VoxelCraft/issues
AppUpdatesURL=https://github.com/BrendanH18/VoxelCraft/releases
DefaultDirName={localappdata}\Programs\VoxelCraft
DefaultGroupName=VoxelCraft
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#OutputPath}
OutputBaseFilename=VoxelCraft-{#AppVersion}-windows-x64-Setup
SetupIconFile={#RepoDir}\packaging\icons\VoxelCraft.ico
UninstallDisplayIcon={app}\voxelcraft.exe
LicenseFile={#RepoDir}\packaging\LICENSE.txt
InfoBeforeFile={#RepoDir}\packaging\INSTALL.txt
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#BinaryDir}\voxelcraft.exe"; DestDir: "{app}"; Flags: ignoreversion
; Command-line client for hosted players (see docs/agents.md).
Source: "{#BinaryDir}\voxelcraft-agent.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoDir}\packaging\INSTALL.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoDir}\packaging\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoDir}\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoDir}\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoDir}\packaging\THIRD-PARTY-LICENSES.html"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\VoxelCraft"; Filename: "{app}\voxelcraft.exe"; WorkingDir: "{app}"
; Hosts command-line players on this computer only (loopback, no cheats).
Name: "{group}\VoxelCraft (host agent players)"; Filename: "{app}\voxelcraft.exe"; Parameters: "--agent-listen 127.0.0.1:4242"; WorkingDir: "{app}"
Name: "{autodesktop}\VoxelCraft"; Filename: "{app}\voxelcraft.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\voxelcraft.exe"; Description: "Launch VoxelCraft"; Flags: nowait postinstall skipifsilent
; The in-game updater installs silently with /UPDATE=1; reopen the game afterwards.
Filename: "{app}\voxelcraft.exe"; Parameters: "{code:RestartParams}"; WorkingDir: "{app}"; Flags: nowait; Check: IsUpdate

; Player saves/logs live in LocalAppData\VoxelCraft, outside {app}.
; Deliberately omit UninstallDelete: uninstalling must preserve player data.

[Code]
const
  SYNCHRONIZE = $00100000;

function OpenProcess(DesiredAccess: Cardinal; InheritHandle: Integer; ProcessId: Cardinal): THandle;
  external 'OpenProcess@kernel32.dll stdcall';
function WaitForSingleObject(Handle: THandle; Milliseconds: Cardinal): Cardinal;
  external 'WaitForSingleObject@kernel32.dll stdcall';
function CloseHandle(Handle: THandle): Integer;
  external 'CloseHandle@kernel32.dll stdcall';

// The in-game updater passes /WAITPID=<game process>: wait (up to a minute)
// for the game to exit so its files can be replaced.
function InitializeSetup(): Boolean;
var
  Pid: Cardinal;
  Process: THandle;
begin
  Pid := StrToIntDef(ExpandConstant('{param:WAITPID|0}'), 0);
  if Pid <> 0 then
  begin
    Process := OpenProcess(SYNCHRONIZE, 0, Pid);
    if Process <> 0 then
    begin
      WaitForSingleObject(Process, 60000);
      CloseHandle(Process);
    end;
  end;
  Result := True;
end;

function IsUpdate(): Boolean;
begin
  Result := ExpandConstant('{param:UPDATE|0}') = '1';
end;

// The game's data folder and update source, passed as /DATADIR= and
// /UPDATESOURCE=, so an updated game reopens with the same data.
function RestartParams(Param: String): String;
var
  Value: String;
begin
  Result := '';
  Value := ExpandConstant('{param:DATADIR|}');
  if Value <> '' then
    Result := '--data-dir "' + Value + '"';
  Value := ExpandConstant('{param:UPDATESOURCE|}');
  if Value <> '' then
    Result := Result + ' --update-source "' + Value + '"';
end;
