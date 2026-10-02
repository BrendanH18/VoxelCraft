param(
    [string]$Target = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
Push-Location $repo
try {
    $metadata = cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed" }
    $version = ($metadata.packages | Where-Object name -eq "voxelcraft").version
    if ($Target -ne "x86_64-pc-windows-msvc") { throw "This installer packages Windows x64 only" }
    $binaryDir = Join-Path $metadata.target_directory "$Target/release"
    $binary = Join-Path $binaryDir "voxelcraft.exe"
    $agent = Join-Path $binaryDir "voxelcraft-agent.exe"
    if (-not (Test-Path $binary) -or -not (Test-Path $agent)) { throw "Build first: cargo build --release --locked --target $Target" }
    if (-not (Test-Path "packaging/THIRD-PARTY-LICENSES.html")) { throw "Generate third-party notices with cargo about (see docs/releases.md)" }
    $output = Join-Path $repo "dist"
    New-Item -ItemType Directory -Path $output -Force | Out-Null
    $iscc = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($iscc) {
        $compiler = $iscc.Source
    } else {
        $compiler = Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6/ISCC.exe"
        if (-not (Test-Path $compiler)) { throw "Install Inno Setup 6 before packaging" }
    }
    & $compiler "/DAppVersion=$version" "/DRepoDir=$repo" "/DBinaryDir=$binaryDir" "/DOutputPath=$output" "packaging/windows/installer.iss"
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed" }
    Write-Host "Built $(Join-Path $output "VoxelCraft-$version-windows-x64-Setup.exe")"
} finally {
    Pop-Location
}
