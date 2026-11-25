# Downloads libmpv into third_party/mpv and verifies its SHA256.
# Source: https://github.com/shinchiro/mpv-winbuild-cmake (the Windows builds linked from mpv.io)
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Dest = Join-Path $Root 'third_party\mpv'
$Name = 'mpv-dev-x86_64-20261111-git-bcde6b93cc.7z'
$Url = "https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/20261111/$Name"
$Sha = '273c079b11ea5575822f972756f4c9a71149a5c10130353abeb8586738938452'
$SevenZip = 'C:\Program Files\7-Zip\7z.exe'

New-Item -ItemType Directory -Force $Dest | Out-Null
$Archive = Join-Path $Dest 'mpv-dev.7z'
if (-not (Test-Path $Archive)) {
    Write-Host "Downloading $Name"
    Invoke-WebRequest -Uri $Url -OutFile $Archive -UseBasicParsing
}
$got = (Get-FileHash $Archive -Algorithm SHA256).Hash.ToLower()
if ($got -ne $Sha) { Remove-Item $Archive; throw "SHA256 mismatch: $got" }
if (-not (Test-Path $SevenZip)) { throw '7-Zip is required: winget install 7zip.7zip' }
& $SevenZip x -y "-o$Dest" $Archive | Out-Null
Write-Host "libmpv ready in $Dest"
