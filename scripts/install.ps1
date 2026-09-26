[CmdletBinding()]
param(
    [string]$Version = 'latest',
    [string]$BinDir = (Join-Path $env:LOCALAPPDATA 'Selfhost\bin'),
    [switch]$Force,
    [switch]$NoModifyPath
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Use install.sh on Linux or macOS.' }
if ($Version -eq 'latest') {
    $release = 'latest/download'
} else {
    $Version = $Version -replace '^v', ''
    if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Expected a stable version such as 0.1.1.' }
    $release = "download/v$Version"
}
$machine = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
$arch = switch ($machine) {
    'AMD64' { 'x64' }
    'ARM64' { 'arm64' }
    default { throw 'Supported architectures: x64 and ARM64.' }
}
$asset = "selfhost-win32-$arch.exe"
$base = "https://github.com/Obiente/selfhost/releases/$release"
$null = New-Item -ItemType Directory -Path $BinDir -Force
$installRoot = (Resolve-Path -LiteralPath $BinDir).Path
$target = Join-Path $installRoot 'selfhost.exe'
if (Test-Path -LiteralPath $target) {
    $existing = Get-Item -LiteralPath $target
    if ($existing.PSIsContainer -or ($existing.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'Refusing to replace a link or non-file.'
    }
}
$stage = Join-Path $installRoot ('.selfhost-install-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $stage
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $download = Join-Path $stage 'selfhost.exe'
    $checksums = Join-Path $stage 'checksums'
    Invoke-WebRequest -UseBasicParsing -Uri "$base/BINARY-SHA256SUMS" -OutFile $checksums
    Invoke-WebRequest -UseBasicParsing -Uri "$base/$asset" -OutFile $download
    $matches = @(Get-Content -LiteralPath $checksums | Where-Object { $_ -cmatch ('^[a-f0-9]{64}  ' + [regex]::Escape($asset) + '$') })
    if ($matches.Count -ne 1) { throw 'Missing or ambiguous release checksum.' }
    $expected = $matches[0].Substring(0, 64)
    if ((Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expected) {
        throw 'Checksum mismatch. Existing installation was not changed.'
    }
    $installedVersion = & $download --version
    if ($LASTEXITCODE -ne 0 -or $installedVersion -notmatch '^selfhost \d+\.\d+\.\d+$') { throw 'Downloaded binary did not start correctly.' }
    if ($Version -ne 'latest' -and $installedVersion -cne "selfhost $Version") { throw 'Downloaded binary reports a different version.' }
    $unchanged = (Test-Path -LiteralPath $target) -and -not $Force -and ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant() -ceq $expected)
    if ($unchanged) {
        Write-Output "$installedVersion is already up to date at $target"
    } elseif (Test-Path -LiteralPath $target) {
        $existing = Get-Item -LiteralPath $target
        if ($existing.PSIsContainer -or ($existing.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'Refusing to replace a link or non-file.'
        }
        $backup = $target + '.backup.' + [guid]::NewGuid().ToString('N')
        [IO.File]::Replace($download, $target, $backup)
        Write-Output "Previous binary kept at $backup"
    } else {
        [IO.File]::Move($download, $target)
    }
    if (-not $NoModifyPath) {
        $userPath = [string][Environment]::GetEnvironmentVariable('Path', 'User')
        $userEntries = @($userPath -split ';' | Where-Object { $_ -and $_.TrimEnd('\') -ine $installRoot.TrimEnd('\') })
        $updatedUserPath = (@($installRoot) + $userEntries) -join ';'
        if ($userPath -cne $updatedUserPath) {
            [Environment]::SetEnvironmentVariable('Path', $updatedUserPath, 'User')
        }
        $sessionEntries = @($env:PATH -split ';' | Where-Object { $_ -and $_.TrimEnd('\') -ine $installRoot.TrimEnd('\') })
        $env:PATH = (@($installRoot) + $sessionEntries) -join ';'
    }
    if (-not $unchanged) { Write-Output "Installed $installedVersion at $target" }
    if (-not $NoModifyPath) { Write-Output 'PATH is configured for this PowerShell session and future terminals.' }
    Write-Output 'Run selfhost --help or selfhost serve. No services were started.'
} finally {
    $resolvedStage = (Resolve-Path -LiteralPath $stage).Path
    if ([IO.Path]::GetDirectoryName($resolvedStage) -ne $installRoot -or [IO.Path]::GetFileName($resolvedStage) -notlike '.selfhost-install-*') { throw 'Unexpected installer staging path.' }
    Remove-Item -LiteralPath $resolvedStage -Recurse -Force
}
