param([Parameter(Mandatory = $true)][string]$Assets)
$ErrorActionPreference = 'Stop'
$fixture = (Resolve-Path -LiteralPath $Assets).Path
$testVersion = (Get-Content -Raw "$PSScriptRoot/../packages/selfhost/package.json" | ConvertFrom-Json).version
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('selfhost-installer-test-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $testRoot
$binDir = Join-Path $testRoot 'bin with spaces'
$corruptChecksum = $false
function Invoke-WebRequest {
    param([switch]$UseBasicParsing, [string]$Uri, [string]$OutFile)
    if (-not $Uri.StartsWith("https://github.com/Obiente/selfhost/releases/download/v$testVersion/")) { throw 'Unexpected download URL.' }
    $name = ([uri]$Uri).Segments[-1]
    if ($corruptChecksum -and $name -eq 'BINARY-SHA256SUMS') {
        (Get-Content -LiteralPath (Join-Path $fixture $name)) -replace '^[a-f0-9]{64}', ('0' * 64) | Set-Content -LiteralPath $OutFile
    } else { Copy-Item -LiteralPath (Join-Path $fixture $name) -Destination $OutFile }
}
try {
    & "$PSScriptRoot/install.ps1" -Version $testVersion -BinDir $binDir -NoModifyPath
    $binary = Join-Path $binDir 'selfhost.exe'
    $hash = (Get-FileHash -LiteralPath $binary).Hash
    $refused = $false
    try { & "$PSScriptRoot/install.ps1" -Version $testVersion -BinDir $binDir -NoModifyPath } catch { $refused = $true }
    if (-not $refused) { throw 'Installer overwrote an existing installation without -Force.' }
    $corruptChecksum = $true
    $refused = $false
    try { & "$PSScriptRoot/install.ps1" -Version $testVersion -BinDir $binDir -NoModifyPath -Force } catch { $refused = $true }
    if (-not $refused -or (Get-FileHash -LiteralPath $binary).Hash -ne $hash) { throw 'Checksum failure did not preserve the installation.' }
    $corruptChecksum = $false
    & "$PSScriptRoot/install.ps1" -Version $testVersion -BinDir $binDir -NoModifyPath -Force
    $backups = @(Get-ChildItem -LiteralPath $binDir -Filter '*.backup.*')
    if ($backups.Count -ne 1 -or (Get-FileHash -LiteralPath $backups[0].FullName).Hash -ne $hash) { throw 'Backup verification failed.' }
    Write-Output 'Installer passed: fresh install, explicit replacement, corrupt download rejection and backup.'
} finally {
    $resolvedTestRoot = (Resolve-Path -LiteralPath $testRoot).Path
    if ([IO.Path]::GetDirectoryName($resolvedTestRoot).TrimEnd('\') -ne ([IO.Path]::GetTempPath()).TrimEnd('\')) { throw 'Unexpected test directory.' }
    Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force
}
