<#
.SYNOPSIS
  Run swing-trade for daily use: production builds, reachable from your own
  devices (e.g. your phone over Tailscale), with the API kept private.

.DESCRIPTION
  - The API (port 8080) listens on 127.0.0.1 only; just the web app on this
    PC talks to it, so the API token never leaves the machine.
  - The web app (port 3000) listens on every interface, so other devices can
    open http://<this-pc>:3000. Use Tailscale (or your home Wi-Fi) for that,
    never port forwarding: the site has no login.
  - Builds once, then reuses the builds; pass -Rebuild after pulling changes.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\serve.ps1
  powershell -ExecutionPolicy Bypass -File scripts\serve.ps1 -Rebuild
#>
param([switch]$Rebuild)
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$EnvFile = Join-Path $Root '.env'
$ApiDir = Join-Path $Root 'services\api'
$WebDir = Join-Path $Root 'apps\web'
$ApiExe = Join-Path $ApiDir 'target\release\swing-api.exe'
$WebBuild = Join-Path $WebDir '.next\BUILD_ID'

if (-not (Test-Path $EnvFile)) {
    throw 'No .env found. Copy .env.example to .env and set API_TOKEN first.'
}

# Load KEY=value lines from .env into this process; the new windows inherit them.
Get-Content $EnvFile | ForEach-Object {
    $line = $_.Trim()
    if ($line -and -not $line.StartsWith('#') -and $line.Contains('=')) {
        $key, $value = $line.Split('=', 2)
        $value = $value.Trim()
        if ($value.Length -ge 2 -and $value[0] -eq $value[-1] -and @('"', "'") -contains [string]$value[0]) {
            $value = $value.Substring(1, $value.Length - 2)
        }
        [Environment]::SetEnvironmentVariable($key.Trim(), $value, 'Process')
    }
}
# The API is for this PC only; the web app reaches it on loopback.
$env:BIND_ADDR = '127.0.0.1:8080'
$env:API_URL = 'http://127.0.0.1:8080'
$env:NEXT_TELEMETRY_DISABLED = '1'

foreach ($port in 3000, 8080) {
    if (Get-NetTCPConnection -State Listen -LocalPort $port -ErrorAction SilentlyContinue) {
        throw "Port $port is already in use. Stop the other swing-trade windows first (dev.ps1 or an earlier serve.ps1)."
    }
}

if ($Rebuild -or -not (Test-Path $ApiExe)) {
    Write-Host 'Building the API (release)...'
    Push-Location $ApiDir
    try { cargo build --release -p api-server; if ($LASTEXITCODE) { throw 'API build failed' } } finally { Pop-Location }
}
if ($Rebuild -or -not (Test-Path $WebBuild)) {
    Write-Host 'Building the web app...'
    Push-Location $WebDir
    try {
        if (-not (Test-Path 'node_modules')) { npm ci; if ($LASTEXITCODE) { throw 'npm ci failed' } }
        npm run build; if ($LASTEXITCODE) { throw 'Web build failed' }
    } finally { Pop-Location }
}

$Shell = if (Get-Command pwsh -ErrorAction SilentlyContinue) { 'pwsh' } else { 'powershell' }
# The API runs from services\api so its relative .data paths resolve as in development.
Start-Process $Shell -WorkingDirectory $ApiDir -ArgumentList @(
    '-NoExit', '-Command', "`$Host.UI.RawUI.WindowTitle = 'swing-trade API'; & '$ApiExe' serve"
)
Start-Process $Shell -WorkingDirectory $WebDir -ArgumentList @(
    '-NoExit', '-Command', "`$Host.UI.RawUI.WindowTitle = 'swing-trade web'; npx next start -H 0.0.0.0 -p 3000"
)

Write-Host ''
Write-Host 'On this PC:      http://localhost:3000'
$ts = Get-Command tailscale -ErrorAction SilentlyContinue
if ($ts) {
    $name = (tailscale status --json 2>$null | ConvertFrom-Json).Self.DNSName
    if ($name) { Write-Host ("On your phone:   http://{0}:3000  (Tailscale)" -f $name.TrimEnd('.')) }
} else {
    Write-Host ("On your phone:   install Tailscale, then http://{0}:3000" -f $env:COMPUTERNAME.ToLower())
}
Write-Host 'Close the two windows (or press Ctrl+C in each) to stop.'
