# Linkport uninstaller: stops the daemon and removes what tools/install.ps1
# created - the browser registration, the sign-in auto-start entry, the Start
# Menu shortcut, the user PATH entry and the install dir. Your rules and
# history (%APPDATA%\linkport) stay unless you pass -PurgeData.
#
#   powershell -c "irm https://raw.githubusercontent.com/butageek/linkport/main/tools/uninstall.ps1 -OutFile $env:TEMP\linkport-uninstall.ps1; powershell -ExecutionPolicy Bypass -File $env:TEMP\linkport-uninstall.ps1"
#
#   powershell -ExecutionPolicy Bypass -File tools\uninstall.ps1 -InstallDir D:\Apps\Linkport -PurgeData
#Requires -Version 5.1
[CmdletBinding()]
param(
    # Where Linkport was installed. Default: %LOCALAPPDATA%\Programs\Linkport
    [string]$InstallDir = '',

    # Also delete %APPDATA%\linkport (config.toml, events.jsonl, token).
    [switch]$PurgeData
)

$ErrorActionPreference = 'Stop'

# Canonicalize to long paths: profiles with dots in the username get 8.3
# short-form env dirs (C:\Users\HENDRY~1.CHO\...), which trip Remove-Item
# in Windows PowerShell 5.1.
$TempRoot = (Resolve-Path $env:TEMP).Path
if ($InstallDir -eq '') {
    $InstallDir = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\Linkport'
}
elseif (-not (Test-Path -LiteralPath $InstallDir)) {
    # Default may have been given in short form; resolve via LOCALAPPDATA.
    $long = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\Linkport'
    if (Test-Path -LiteralPath $long) { $InstallDir = $long }
}

function Write-Step([string]$msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Write-Note([string]$msg) { Write-Host "    $msg" -ForegroundColor DarkGray }

# Every process named linkport (the daemon, the URL handler and `restart`
# flavors - never linkport-cli, so the cli running `stop` never kills
# itself).
function Get-LinkportProcesses {
    try {
        return @(Get-Process -Name 'linkport' -ErrorAction SilentlyContinue)
    }
    catch { return @() }
}

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
    try { Remove-Item -LiteralPath $outLog, $errLog -ErrorAction SilentlyContinue } catch { }
    return [pscustomobject]@{ Code = $proc.ExitCode; Output = $text }
}

# Graceful first (`linkport-cli stop` = the tray's Quit, clean tray-icon
# removal), force-terminate after a short grace period.
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

$targetExe = Join-Path $InstallDir 'linkport.exe'
$targetCli = Join-Path $InstallDir 'linkport-cli.exe'
Stop-LinkportDaemon -CliPath $targetCli

# ------------------------------------------------------ unregister --------
if (Test-Path -LiteralPath $targetCli) {
    Write-Step 'Removing the browser registration'
    $r = Invoke-LinkportCli -CliPath $targetCli -CliArgs @('unregister')
    if ($r.Code -ne 0) { Write-Warning "linkport-cli unregister exited with code $($r.Code); continuing." }
}
else {
    Write-Warning 'linkport-cli.exe not found; skipping registry cleanup (keys stay in HKCU).'
}

# --------------------------------------------------------- auto-start -----
Write-Step 'Removing the sign-in auto-start entry'
try {
    Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'Linkport' -ErrorAction SilentlyContinue
}
catch { }

# ------------------------------------------------------------ shortcut ----
Write-Step 'Removing Start Menu shortcut'
$shortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) 'Linkport.lnk'
if (Test-Path -LiteralPath $shortcutPath) {
    try { Remove-Item -LiteralPath $shortcutPath -Force } catch { Write-Warning "could not remove the Start Menu shortcut: $_" }
}

# ------------------------------------------------------------ PATH entry ---
Write-Step 'Removing PATH entry'
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($null -ne $userPath) {
    $kept = @($userPath -split ';' | Where-Object { $_ -ne '' -and -not [string]::Equals($_.TrimEnd('\'), $InstallDir.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase) })
    if ($kept.Count -ne ($userPath -split ';').Count) {
        [Environment]::SetEnvironmentVariable('Path', ($kept -join ';'), 'User')
    }
}

# ---------------------------------------------------------- install dir ---
Write-Step "Removing $InstallDir"
if (Test-Path -LiteralPath $InstallDir) {
    try { Remove-Item -LiteralPath $InstallDir -Recurse -Force } catch { Write-Warning "could not remove $InstallDir (delete it manually): $_" }
}

# ---------------------------------------------------------------- data ----
if ($PurgeData) {
    $dataDir = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'linkport'
    Write-Step "Removing $dataDir"
    if (Test-Path -LiteralPath $dataDir) {
        try { Remove-Item -LiteralPath $dataDir -Recurse -Force } catch { Write-Warning "could not remove $dataDir: $_" }
    }
}
else {
    Write-Note 'rules and history kept in %APPDATA%\linkport (pass -PurgeData to delete them)'
}

Write-Host 'Linkport uninstalled.' -ForegroundColor Green
Write-Host 'Note: if Linkport was your default browser, pick another one under'
Write-Host 'Settings > Apps > Default apps - Windows keeps the stale choice until you do.'
