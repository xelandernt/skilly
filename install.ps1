<#
.SYNOPSIS
    skilly installer for Windows.

.DESCRIPTION
    Downloads the prebuilt skilly binary from GitHub releases, verifies its
    SHA256 checksum, installs it, and adds the install directory to PATH.

.EXAMPLE
    irm https://raw.githubusercontent.com/xelandernt/skilly/main/install.ps1 | iex

.EXAMPLE
    & ([scriptblock]::Create((irm https://raw.githubusercontent.com/xelandernt/skilly/main/install.ps1))) -Version 0.0.32
#>
[CmdletBinding()]
param(
    [string]$Version = $env:SKILLY_VERSION,
    [string]$InstallDir = $env:SKILLY_INSTALL_DIR,
    [switch]$NoModifyPath,
    [switch]$DryRun,
    [switch]$Help
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Repo = 'xelandernt/skilly'
$Binary = 'skilly'

function Write-Info { param([string]$Message) Write-Host $Message }
function Write-Warn { param([string]$Message) Write-Warning $Message }

function Show-Usage {
    @"
skilly installer (Windows)

Usage:
  irm https://raw.githubusercontent.com/$Repo/main/install.ps1 | iex

  # With options:
  & ([scriptblock]::Create((irm https://raw.githubusercontent.com/$Repo/main/install.ps1))) -Version 0.0.32

Options:
  -Version <version>   Version to install (default: latest, e.g. 0.0.32)
  -InstallDir <dir>    Install directory (default: %LOCALAPPDATA%\skilly\bin)
  -NoModifyPath        Do not add the install directory to your user PATH
  -DryRun              Print what would happen without installing
  -Help                Show this help

Environment variables:
  SKILLY_VERSION         Same as -Version
  SKILLY_INSTALL_DIR     Same as -InstallDir
  SKILLY_GITHUB_TOKEN    GitHub token for higher API rate limits (latest lookup)
"@
}

if ($Help) {
    Show-Usage
    return
}

if ([string]::IsNullOrWhiteSpace($Version)) { $Version = 'latest' }
if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $InstallDir = Join-Path $env:LOCALAPPDATA 'skilly\bin'
}

function Get-Target {
    $arch = $env:PROCESSOR_ARCHITECTURE
    if ($env:PROCESSOR_ARCHITEW6432) { $arch = $env:PROCESSOR_ARCHITEW6432 }
    switch ($arch) {
        'AMD64' { return 'x86_64-pc-windows-msvc' }
        default {
            throw "no prebuilt skilly binary for architecture '$arch'. Supported: x86_64 (AMD64). Install via 'uvx skilly', 'npx @xelandernt/skilly', or 'cargo install' instead."
        }
    }
}

function Get-AuthHeaders {
    $token = $env:SKILLY_GITHUB_TOKEN
    if ([string]::IsNullOrWhiteSpace($token)) { return @{} }
    return @{ Authorization = "Bearer $token" }
}

function Resolve-Version {
    param([string]$Requested)
    if ($Requested -ne 'latest') { return $Requested }
    $api = "https://api.github.com/repos/$Repo/releases/latest"
    $headers = Get-AuthHeaders
    $headers['Accept'] = 'application/vnd.github+json'
    try {
        $release = Invoke-RestMethod -Uri $api -Headers $headers -UseBasicParsing
    }
    catch {
        throw "failed to query latest release: $($_.Exception.Message)"
    }
    if ([string]::IsNullOrWhiteSpace($release.tag_name)) {
        throw 'could not determine latest version from GitHub API'
    }
    return $release.tag_name
}

function Test-Checksum {
    param([string]$ArchivePath, [string]$ChecksumsPath, [string]$Name)
    $line = Select-String -Path $ChecksumsPath -Pattern ([regex]::Escape($Name)) | Select-Object -First 1
    if (-not $line) { throw "no checksum found for $Name" }
    $expected = ($line.Line -split '\s+')[0].ToLower()
    $actual = (Get-FileHash -Path $ArchivePath -Algorithm SHA256).Hash.ToLower()
    if ($expected -ne $actual) {
        throw "checksum mismatch for ${Name}: expected $expected, got $actual"
    }
}

function Add-ToUserPath {
    param([string]$Dir)
    if ($NoModifyPath) { return }
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($null -eq $userPath) { $userPath = '' }
    $entries = $userPath -split ';' | Where-Object { $_ -ne '' }
    if ($entries -contains $Dir) { return }
    $newPath = if ([string]::IsNullOrEmpty($userPath)) { $Dir } else { "$userPath;$Dir" }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    $env:Path = "$env:Path;$Dir"
    Write-Info "Added $Dir to your user PATH. Restart your terminal to pick it up."
}

$target = Get-Target
$Version = Resolve-Version -Requested $Version

$base = "https://github.com/$Repo/releases/download/$Version"
$archiveName = "$Binary-$Version-$target.zip"
$archiveUrl = "$base/$archiveName"
$checksumsUrl = "$base/$Binary-sha256sums.txt"
$binaryName = "$Binary.exe"

Write-Info "Installing $Binary $Version ($target) to $InstallDir"

if ($DryRun) {
    Write-Info "[dry-run] would download: $archiveUrl"
    Write-Info "[dry-run] would verify against: $checksumsUrl"
    Write-Info "[dry-run] would install to: $(Join-Path $InstallDir $binaryName)"
    return
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) "skilly-install-$(Get-Random)"
New-Item -ItemType Directory -Path $tmp -Force | Out-Null
try {
    $archivePath = Join-Path $tmp $archiveName
    $checksumsPath = Join-Path $tmp "$Binary-sha256sums.txt"

    try {
        Invoke-WebRequest -Uri $archiveUrl -OutFile $archivePath -UseBasicParsing
    }
    catch {
        throw "failed to download ${archiveUrl} (does version $Version exist?): $($_.Exception.Message)"
    }
    Invoke-WebRequest -Uri $checksumsUrl -OutFile $checksumsPath -UseBasicParsing

    Test-Checksum -ArchivePath $archivePath -ChecksumsPath $checksumsPath -Name $archiveName

    $extractDir = Join-Path $tmp 'extract'
    Expand-Archive -Path $archivePath -DestinationPath $extractDir -Force
    $extracted = Join-Path $extractDir $binaryName
    if (-not (Test-Path $extracted)) {
        throw "archive did not contain $binaryName"
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path $extracted -Destination (Join-Path $InstallDir $binaryName) -Force

    Write-Info "Installed $Binary to $(Join-Path $InstallDir $binaryName)"
    Add-ToUserPath -Dir $InstallDir
    Write-Info "Run '$Binary --help' to get started."
}
finally {
    if (Test-Path $tmp) { Remove-Item -Path $tmp -Recurse -Force -ErrorAction SilentlyContinue }
}
