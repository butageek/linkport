# Linkport installer (per-user, no admin rights required).
#
# Easiest install or upgrade - open PowerShell and paste (downloads the
# latest release from GitHub and runs this script as a file; piping it into
# iex breaks on Windows PowerShell 5.1 because of the param block below):
#   powershell -c "irm https://raw.githubusercontent.com/butageek/linkport/main/tools/install.ps1 -OutFile $env:TEMP\linkport-install.ps1; powershell -ExecutionPolicy Bypass -File $env:TEMP\linkport-install.ps1"
#
# More control:
#   powershell -ExecutionPolicy Bypass -File tools\install.ps1 -BinDir .\target\x86_64-pc-windows-gnu\release
#   powershell -ExecutionPolicy Bypass -File tools\install.ps1 -DownloadUrl https://github.com/butageek/linkport/releases/download/v0.5.0/linkport-0.5.0-win64.zip
#   powershell -ExecutionPolicy Bypass -File tools\install.ps1 -InstallDir D:\Apps\Linkport -NoPath
#
# Source resolution order:
#   1. -BinDir (a local build with both exes) / -DownloadUrl (a release zip)
#   2. target\...\release next to this script (local builds)
#   3. the latest linkport-*-win64.zip asset of the GitHub releases of -Repo
#
# What it does:
#   1. Stops any running daemon (gracefully through `linkport-cli stop`,
#      then by force) and copies linkport.exe + linkport-cli.exe to
#      %LOCALAPPDATA%\Programs\Linkport - the per-user location modern
#      Windows apps use: no admin rights, no UAC prompt on upgrade, and
#      program files kept apart from the data in %APPDATA%\linkport
#   2. Creates the Start Menu shortcut and (optionally) adds the install
#      dir to the user PATH (so `linkport-cli` works from any terminal)
#   3. Registers Linkport as a browser candidate - only when it is not
#      registered yet; otherwise the existing registration is pointed at
#      this install by rewriting its values in place (re-registering can
#      reset Windows' hashed default-browser choice)
#   4. Points the sign-in auto-start entry at this install when one exists
#   5. Starts the daemon (the portal opens by itself on the very first run)
#
# Settings never live in the install dir: rules, history and the portal
# token are in %APPDATA%\linkport and are not touched, and the Windows
# default-browser choice survives the in-place repair in step 3.
#Requires -Version 5.1
[CmdletBinding()]
param(
    # Directory holding a built linkport.exe + linkport-cli.exe. Default:
    # searched next to the script, then downloaded from GitHub releases.
    [string]$BinDir = '',

    # Where to install. Default: %LOCALAPPDATA%\Programs\Linkport
    [string]$InstallDir = '',

    # Direct URL to a linkport-<version>-win64.zip release asset.
    [string]$DownloadUrl = '',

    # GitHub repo (owner/name) for the latest release when no local build
    # is given.
    [string]$Repo = 'butageek/linkport',

    # Skip the registry registration step.
    [switch]$NoRegister,

    # Do not add the install dir to the user PATH.
    [switch]$NoPath,

    # Do not start the daemon afterwards.
    [switch]$NoLaunch
)

$ErrorActionPreference = 'Stop'
# The progress bar makes Invoke-WebRequest crawl on Windows PowerShell 5.1.
$ProgressPreference = 'SilentlyContinue'

# Windows PowerShell 5.1 defaults may not include TLS 1.2 (GitHub requires it).
[Net.ServicePointManager]::SecurityProtocol = `
    [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

# Canonicalize TEMP to its long form: profiles with dots in the username
# (C:\Users\hendry.chou) can get an 8.3 short-form TEMP
# (C:\Users\HENDRY~1.CHO\AppData\Local\Temp), which trips Remove-Item's
# path handling in Windows PowerShell 5.1.
$TempRoot = (Resolve-Path $env:TEMP).Path

# Same for the default install dir: LOCALAPPDATA can be short-formed too,
# and we don't want 8.3 paths baked into the registry or shortcuts.
if ($InstallDir -eq '') {
    $InstallDir = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\Linkport'
}

function Write-Step([string]$msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Write-Note([string]$msg) { Write-Host "    $msg" -ForegroundColor DarkGray }

# Every process named linkport (the daemon, the URL handler and `restart`
# flavors - never linkport-cli). They all share %APPDATA%\linkport and so
# the portal port: any one of them must go before the new daemon starts.
function Get-LinkportProcesses {
    try {
        return @(Get-Process -Name 'linkport' -ErrorAction SilentlyContinue)
    }
    catch { return @() }
}

# Run linkport-cli and wait for it, capturing its output. Console binary,
# but the call operator + `$ErrorActionPreference = 'Stop'` turn any stderr
# line into a terminating error before `$LASTEXITCODE` can be checked.
function Invoke-LinkportCli {
    param([string]$CliPath, [string[]]$CliArgs)
    $outLog = Join-Path $TempRoot ("linkport-cli-out-" + [guid]::NewGuid().ToString('N') + '.txt')
    $errLog = Join-Path $TempRoot ("linkport-cli-err-" + [guid]::NewGuid().ToString('N') + '.txt')
    $proc = Start-Process -FilePath $CliPath -ArgumentList $CliArgs `
        -Wait -PassThru -RedirectStandardOutput $outLog -RedirectStandardError $errLog
    $text = ''
    foreach ($log in @($outLog, $errLog)) {
        if (Test-Path -LiteralPath $log) { $text += (Get-Content -LiteralPath $log -Raw -ErrorAction SilentlyContinue) }
    }
    # Cleanup is cosmetic: -ErrorAction alone cannot suppress terminating
    # PSArgumentExceptions in PS 5.1, so guard with try/catch too.
    try { Remove-Item -LiteralPath $outLog, $errLog -ErrorAction SilentlyContinue } catch { }
    return [pscustomobject]@{ Code = $proc.ExitCode; Output = $text }
}

# Stop the running daemon(s) so the file can be replaced (Windows locks a
# running executable). Graceful first - `linkport-cli stop` is the tray's
# Quit over HTTP, which removes the tray icon cleanly - then
# force-terminate after a short grace period. Releases before 0.5.0 have no
# `stop`; the force path covers them.
function Stop-LinkportDaemon {
    param([string]$CliPath)
    $procs = @(Get-LinkportProcesses)
    if ($procs.Count -eq 0) { return }

    Write-Note "stopping the running Linkport ($($procs.Count) process(es))"
    if (Test-Path -LiteralPath $CliPath) {
        try { $null = Invoke-LinkportCli -CliPath $CliPath -CliArgs @('stop') } catch { }
    }
    $deadline = (Get-Date).AddSeconds(5)
    while ((Get-Date) -lt $deadline -and @(Get-LinkportProcesses).Count -gt 0) {
        Start-Sleep -Milliseconds 250
    }
    $left = @(Get-LinkportProcesses)
    if ($left.Count -gt 0) {
        try { $left | Stop-Process -Force -ErrorAction SilentlyContinue } catch { }
    }
    Start-Sleep -Milliseconds 300 # let the OS release the file handles
}

# Point the existing registration at this install by rewriting its values
# in place. Only existing values are overwritten - no key is created or
# deleted - so the user's default-browser choice (hashed over the ProgId
# name, not its command) survives, exactly like the daemon's own self-heal.
# Needed after a manual copy elsewhere was removed, and harmless on
# upgrades in place.
function Repair-Registration {
    param([string]$TargetExe)
    $handler = "`"$TargetExe`" `"%1`""
    $icon = "$TargetExe,0"
    $updates = @(
        @{ Path = 'HKCU:\Software\Classes\Linkport.URL\shell\open\command'; Value = $handler },
        @{ Path = 'HKCU:\Software\Classes\Linkport.URL\DefaultIcon'; Value = $icon },
        @{ Path = 'HKCU:\Software\Clients\StartMenuInternet\Linkport\shell\open\command'; Value = $handler },
        @{ Path = 'HKCU:\Software\Clients\StartMenuInternet\Linkport\DefaultIcon'; Value = $icon }
    )
    foreach ($u in $updates) {
        if (Test-Path -Path $u.Path) {
            try { Set-ItemProperty -Path $u.Path -Name '(default)' -Value $u.Value } catch { Write-Warning "could not update $($u.Path): $_" }
        }
    }
}

function Get-GithubLatestAsset {
    param([string]$OwnerRepo)
    Write-Step "Looking up the latest release of $OwnerRepo"
    $headers = @{ 'User-Agent' = 'linkport-installer' }
    if ($env:GITHUB_TOKEN) { $headers['Authorization'] = "token $env:GITHUB_TOKEN" }
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$OwnerRepo/releases/latest" -Headers $headers
    $asset = $release.assets |
        Where-Object { $_.name -match '^linkport-[\w.\-]+-win64\.zip$' } |
        Select-Object -First 1
    if (-not $asset) {
        throw "No linkport-*-win64.zip asset found on the latest release of $OwnerRepo ($($release.tag_name))."
    }
    return $asset.browser_download_url
}

# Extract a release zip and return the directory that holds linkport.exe
# (the zip wraps everything in linkport-<version>-win64\).
function Expand-LinkportArchive {
    param([string]$ArchivePath)
    $tmp = Join-Path $TempRoot ("linkport-install-" + [guid]::NewGuid().ToString('N'))
    Expand-Archive -LiteralPath $ArchivePath -DestinationPath $tmp -Force
    $exe = Get-ChildItem -Path $tmp -Recurse -Filter 'linkport.exe' | Select-Object -First 1
    if (-not $exe) { throw 'linkport.exe not found inside the downloaded archive.' }
    return $exe.DirectoryName
}

function Save-ReleaseZip {
    param([string]$Url)
    Write-Step "Downloading $Url"
    $archive = Join-Path $TempRoot ("linkport-download-" + [guid]::NewGuid().ToString('N') + '.zip')
    Invoke-WebRequest -Uri $Url -OutFile $archive -UseBasicParsing
    return $archive
}

# ------------------------------------------------------------ source -------
$tempDirs = @()
if ($DownloadUrl -ne '') {
    $archive = Save-ReleaseZip $DownloadUrl
    $BinDir = Expand-LinkportArchive $archive
    $tempDirs += $BinDir, $archive
}
elseif ($BinDir -ne '') {
    if (-not (Test-Path -LiteralPath (Join-Path $BinDir 'linkport.exe'))) {
        throw "linkport.exe not found in -BinDir '$BinDir'."
    }
}
elseif ($PSScriptRoot) {
    $candidates = @(
        (Join-Path $PSScriptRoot '..\target\x86_64-pc-windows-gnu\release'),
        (Join-Path $PSScriptRoot '..\target\release')
    )
    $found = $candidates | Where-Object { Test-Path -LiteralPath (Join-Path $_ 'linkport.exe') } | Select-Object -First 1
    if ($found) { $BinDir = (Resolve-Path -LiteralPath $found).Path }
}

if ($BinDir -eq '') {
    # No local build: fetch the latest release zip from GitHub.
    $archive = Save-ReleaseZip (Get-GithubLatestAsset $Repo)
    $BinDir = Expand-LinkportArchive $archive
    $tempDirs += $BinDir, $archive
}

$BinDir = (Resolve-Path -LiteralPath $BinDir).Path
foreach ($name in @('linkport.exe', 'linkport-cli.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path $BinDir $name))) {
        throw "$name not found in '$BinDir'. Pass -BinDir, -DownloadUrl, or build with: cargo build --release --target x86_64-pc-windows-gnu"
    }
}
Write-Step "Using build: $BinDir"

# ----------------------------------------------------------- install -------
$registered = Test-Path 'HKCU:\Software\Clients\StartMenuInternet\Linkport\Capabilities\URLAssociations'
$isUpgrade = Test-Path -LiteralPath (Join-Path $InstallDir 'linkport.exe')

if ($isUpgrade) { Write-Step "Upgrading $InstallDir" }
else { Write-Step "Installing to $InstallDir" }
New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
$InstallDir = (Resolve-Path -LiteralPath $InstallDir).Path
$targetExe = Join-Path $InstallDir 'linkport.exe'
$targetCli = Join-Path $InstallDir 'linkport-cli.exe'

Stop-LinkportDaemon -CliPath $targetCli

Copy-Item -LiteralPath (Join-Path $BinDir 'linkport.exe') -Destination $targetExe -Force
Copy-Item -LiteralPath (Join-Path $BinDir 'linkport-cli.exe') -Destination $targetCli -Force
# Future-proofing: if a release ever ships helper DLLs next to the exes,
# bring them along (the mingw build links everything statically today).
Get-ChildItem -Path $BinDir -Filter '*.dll' -ErrorAction SilentlyContinue | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination $InstallDir -Force
}
foreach ($doc in @('QUICK-START.txt', 'LICENSE')) {
    $src = Join-Path $BinDir $doc
    if (Test-Path -LiteralPath $src) { Copy-Item -LiteralPath $src -Destination (Join-Path $InstallDir $doc) -Force }
}

# Start Menu shortcut (puts Linkport in the system app list; the daemon
# recreates it when missing, this just makes it exist before the first start).
Write-Step 'Creating Start Menu shortcut'
$shortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) 'Linkport.lnk'
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $targetExe
$shortcut.Arguments = 'serve'
$shortcut.WorkingDirectory = $InstallDir
$shortcut.IconLocation = "$targetExe,0"
$shortcut.Description = 'Linkport - rule-based browser router'
$shortcut.Save()

# User PATH (lets you run `linkport-cli test <url>` from any terminal).
if (-not $NoPath) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($null -eq $userPath) { $userPath = '' }
    if (-not (($userPath -split ';') | Where-Object { [string]::Equals($_.TrimEnd('\'), $InstallDir, [StringComparison]::OrdinalIgnoreCase) })) {
        Write-Step "Adding $InstallDir to the user PATH"
        $newPath = ($userPath.TrimEnd(';'), $InstallDir) -join ';'
        [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    }
}

# ------------------------------------------------------------ register ----
if (-not $NoRegister -and -not $registered) {
    Write-Step 'Registering Linkport with Windows (HKCU only, no admin needed)'
    $r = Invoke-LinkportCli -CliPath $targetCli -CliArgs @('register')
    if ($r.Output.Trim()) { Write-Note ($r.Output.Trim() -replace "`r?`n", "`n    ") }
    if ($r.Code -ne 0) { throw "linkport-cli register failed with exit code $($r.Code)." }
}
elseif ($registered) {
    Write-Step 'Pointing the existing registration at this install'
    Repair-Registration -TargetExe $targetExe
}

# The sign-in auto-start entry follows the install when it exists (the
# user's choice is kept, only the path changes); "off" stays off.
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$runValue = $null
try { $runValue = (Get-ItemProperty -Path $runKey -Name 'Linkport' -ErrorAction SilentlyContinue).Linkport } catch { }
if ($runValue) {
    $wanted = "`"$targetExe`" serve"
    if ($runValue -ne $wanted) {
        Write-Step 'Pointing the sign-in auto-start entry at this install'
        Set-ItemProperty -Path $runKey -Name 'Linkport' -Value $wanted
    }
}

# Best-effort version confirmation (never fails the install).
$versionText = ''
try {
    $v = Invoke-LinkportCli -CliPath $targetCli -CliArgs @('--version')
    if ($v.Code -eq 0) { $versionText = $v.Output.Trim() -replace '^\S+\s+', '' }
} catch { }

# Cleanup is cosmetic: -ErrorAction alone cannot suppress terminating
# PSArgumentExceptions in PS 5.1, so guard with try/catch too.
foreach ($tmp in $tempDirs) {
    if ($tmp -and (Test-Path -LiteralPath $tmp)) {
        try { Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue } catch { }
    }
}

# ------------------------------------------------------------- launch -----
if (-not $NoLaunch) {
    Write-Step 'Starting Linkport'
    Start-Sleep -Milliseconds 500 # let the old daemon's port go
    # GUI-subsystem exe: Start-Process returns at once and no window shows.
    Start-Process -FilePath $targetExe -ArgumentList 'serve'
}

Write-Host ''
if ($versionText) { Write-Host "Linkport $versionText installed." -ForegroundColor Green }
elseif ($isUpgrade) { Write-Host 'Linkport upgraded.' -ForegroundColor Green }
else { Write-Host 'Linkport installed.' -ForegroundColor Green }
if (Test-Path (Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'linkport\config.toml')) {
    Write-Note 'rules, history and the portal token in %APPDATA%\linkport are untouched'
}

if (-not $registered -and -not $NoRegister) {
    Write-Host 'Next: Windows Settings > Apps > Default apps > Linkport > set the default for HTTP and HTTPS.'
    Write-Host '      (Windows requires this by hand - no app can do it for you.)'
}
if (-not $NoLaunch) {
    Write-Host 'The portal opens in your browser on the first run; otherwise left-click the tray icon.'
    Write-Host 'Recommended: right-click the tray icon > "Start Linkport when I sign in".'
}
else {
    Write-Host "Start it with: `"$targetExe`" serve  (or Start menu > Linkport)"
}
if (-not $NoPath) {
    Write-Host 'Open a new terminal and try:  linkport-cli test https://docs.google.com/'
}
