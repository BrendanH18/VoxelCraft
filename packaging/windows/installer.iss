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

; Player saves/logs live in LocalAppData\VoxelCraft, outside {app}.
; Deliberately omit UninstallDelete: uninstalling must preserve player data.
