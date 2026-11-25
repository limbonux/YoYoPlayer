# Builds the release and packs it into dist\YoYoPlayer-<version>-win-x64.zip.
# Paths of the build machine are remapped, so the program does not carry them.
param([string]$Cargo = 'cargo')
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$Version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$CargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$RustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE '.rustup' }
$env:RUSTFLAGS = "--remap-path-prefix=$CargoHome=cargo --remap-path-prefix=$RustupHome=rustup --remap-path-prefix=$Root=yoyo"

$Dll = 'third_party\mpv\libmpv-2.dll'
if (-not (Test-Path $Dll)) { throw 'libmpv is missing; run scripts\fetch-deps.ps1 first' }

$ErrorActionPreference = 'Continue'  # cargo writes progress to stderr
& $Cargo build --release --locked -p yoyo-app
$ErrorActionPreference = 'Stop'
if ($LASTEXITCODE -ne 0) { throw "cargo failed with exit code $LASTEXITCODE" }

$Exe = 'target\release\yoyo.exe'
$Text = [Text.Encoding]::GetEncoding(28591).GetString([IO.File]::ReadAllBytes((Resolve-Path $Exe)))
foreach ($Private in @($env:USERPROFILE, $Root)) {
    if ($Text.IndexOf($Private, [StringComparison]::OrdinalIgnoreCase) -ge 0) { throw "the program still contains $Private" }
}

$Name = "YoYoPlayer-$Version-win-x64"
$Stage = Join-Path 'dist' $Name
if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Force $Stage | Out-Null
Copy-Item $Exe, $Dll, 'LICENSE', 'ATTRIBUTION.md' $Stage
$Zip = Join-Path 'dist' "$Name.zip"
if (Test-Path $Zip) { Remove-Item -Force $Zip }
Compress-Archive -Path $Stage -DestinationPath $Zip -CompressionLevel Optimal
$Hash = (Get-FileHash $Zip -Algorithm SHA256).Hash.ToLower()
"$Hash  $Name.zip" | Out-File -Encoding ascii "$Zip.sha256"
Write-Host "$Zip"
Write-Host "sha256 $Hash"
