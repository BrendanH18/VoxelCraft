$ErrorActionPreference = "Stop"
if ($env:CI -ne "true") { throw "Run this installation test only on a disposable CI runner" }
$repo = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
$installer = @(Get-ChildItem (Join-Path $repo "dist/*-Setup.exe"))
if ($installer.Count -ne 1) { throw "Expected exactly one installer" }
$installDir = Join-Path $env:TEMP "VoxelCraft installer test $PID"
$fixture = Join-Path $env:LOCALAPPDATA "VoxelCraft/saves/installer-smoke-test-$PID"
$shortcut = Join-Path $env:APPDATA "Microsoft/Windows/Start Menu/Programs/VoxelCraft/VoxelCraft.lnk"
$hostShortcut = Join-Path $env:APPDATA "Microsoft/Windows/Start Menu/Programs/VoxelCraft/VoxelCraft (host agent players).lnk"
if (Test-Path $fixture) { throw "Refusing to overwrite a pre-existing save fixture" }
New-Item -ItemType Directory -Path $fixture | Out-Null
Set-Content -Path (Join-Path $fixture "level.txt") -Value "seed=424242"

function Run-And-Wait([string]$File, [string[]]$Arguments) {
    $process = Start-Process -FilePath $File -ArgumentList $Arguments -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "$File exited with $($process.ExitCode)" }
}

try {
    if ((Get-AuthenticodeSignature $installer[0].FullName).Status -ne "NotSigned") { throw "Expected an unsigned installer" }
    # Installing twice exercises the existing installation/upgrade path.
    foreach ($attempt in 1..2) {
        Run-And-Wait $installer[0].FullName @("/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-", "/DIR=`"$installDir`"")
        $exe = Join-Path $installDir "voxelcraft.exe"
        if (-not (Test-Path $exe)) { throw "Installed executable is missing" }
        if (-not (Test-Path $shortcut)) { throw "Start Menu shortcut is missing" }
        if (-not (Test-Path $hostShortcut)) { throw "Agent host shortcut is missing" }
        $agent = Join-Path $installDir "voxelcraft-agent.exe"
        if (-not (Test-Path $agent)) { throw "Installed agent client is missing" }
        $help = & $agent --help
        if ($LASTEXITCODE -ne 0 -or -not ($help -match "--connect")) { throw "Agent client did not run" }
        if ((Get-Item $exe).VersionInfo.ProductName -ne "VoxelCraft") { throw "Executable metadata is missing" }
        if (-not (Test-Path (Join-Path $installDir "THIRD-PARTY-LICENSES.html"))) { throw "License notices are missing" }
        Run-And-Wait $exe @("--bench", "--rd", "2")
        if ((Get-Content (Join-Path $fixture "level.txt")) -ne "seed=424242") { throw "Installer changed player data" }
    }
    Run-And-Wait (Join-Path $installDir "unins000.exe") @("/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART")
    if (Test-Path (Join-Path $installDir "voxelcraft.exe")) { throw "Uninstall did not remove the executable" }
    if (Test-Path (Join-Path $installDir "voxelcraft-agent.exe")) { throw "Uninstall did not remove the agent client" }
    if (Test-Path $hostShortcut) { throw "Uninstall did not remove the agent host shortcut" }
    if (Test-Path $shortcut) { throw "Uninstall did not remove the shortcut" }
    if ((Get-Content (Join-Path $fixture "level.txt")) -ne "seed=424242") { throw "Uninstall changed player data" }
    Write-Host "Install, reinstall, game and agent client launch and uninstall passed; saves preserved."
} finally {
    Remove-Item $fixture -Recurse -Force
}
