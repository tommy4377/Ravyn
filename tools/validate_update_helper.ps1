param(
  [string]$WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
)

$ErrorActionPreference = 'Stop'
$helperPath = Join-Path ([System.IO.Path]::GetTempPath()) 'ravyn-helper-script.ps1'

Push-Location $WorkspaceRoot
try {
  Remove-Item $helperPath -Force -ErrorAction SilentlyContinue

  cargo test --locked -p ravyn-desktop dump_helper_script_for_parser_validation -- --ignored
  if ($LASTEXITCODE -ne 0) {
    throw "The updater helper generation test failed with exit code $LASTEXITCODE."
  }
  if (-not (Test-Path $helperPath)) {
    throw "The updater helper generation test did not create $helperPath."
  }

  $script = Get-Content $helperPath -Raw
  [void][scriptblock]::Create($script)

  $requiredMarkers = @(
    'Write-Journal',
    'Restore-Installation',
    'The updated version did not reach readiness',
    'reg.exe export',
    'reg.exe import',
    'completed_at_unix_ms'
  )
  foreach ($marker in $requiredMarkers) {
    if (-not $script.Contains($marker, [System.StringComparison]::Ordinal)) {
      throw "The generated updater helper is missing the required marker: $marker"
    }
  }

  Write-Host 'Generated updater helper parsed successfully and contains all recovery guards.'
} finally {
  Remove-Item $helperPath -Force -ErrorAction SilentlyContinue
  Pop-Location
}
