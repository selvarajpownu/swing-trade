<#
.SYNOPSIS
  Start the Rust API and the Next.js dev server in two PowerShell windows (Windows).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\dev.ps1
#>
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$EnvFile = Join-Path $Root '.env'
$ApiDir = Join-Path $Root 'services\api'
$WebDir = Join-Path $Root 'apps\web'

if (-not (Test-Path $EnvFile)) {
    Write-Warning 'No .env found. Creating one from .env.example - set API_TOKEN before going further.'
    Copy-Item (Join-Path $Root '.env.example') $EnvFile
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

# Prefer PowerShell 7 if installed, otherwise Windows PowerShell.
$Shell = if (Get-Command pwsh -ErrorAction SilentlyContinue) { 'pwsh' } else { 'powershell' }

if (-not (Test-Path (Join-Path $WebDir 'node_modules'))) {
    Write-Host 'Installing web dependencies...'
    Push-Location $WebDir
    try {
        if (Test-Path 'package-lock.json') { npm ci } else { npm install }
    } finally {
        Pop-Location
    }
}

Start-Process $Shell -WorkingDirectory $ApiDir -ArgumentList @(
    '-NoExit', '-Command', '$Host.UI.RawUI.WindowTitle = ''swing-trade API''; cargo run -p api-server -- serve'
)
Start-Process $Shell -WorkingDirectory $WebDir -ArgumentList @(
    '-NoExit', '-Command', '$Host.UI.RawUI.WindowTitle = ''swing-trade web''; npm run dev'
)

Write-Host 'API -> http://localhost:8080   Web -> http://localhost:3000'
Write-Host 'Close the two windows (or press Ctrl+C in each) to stop.'
