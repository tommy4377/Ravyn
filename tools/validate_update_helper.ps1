param(
  [string]$WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
)

$ErrorActionPreference = 'Stop'
$helperPath = Join-Path ([System.IO.Path]::GetTempPath()) 'ravyn-helper-script.ps1'
$testRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("ravyn-update-lifecycle-" + [guid]::NewGuid().ToString('N'))

function New-TestExecutable {
  param(
    [Parameter(Mandatory = $true)][string]$Source,
    [Parameter(Mandatory = $true)][string]$Output
  )
  Add-Type -TypeDefinition $Source -Language CSharp -OutputAssembly $Output -OutputType ConsoleApplication
}

function Write-GeneratedHelper {
  param(
    [Parameter(Mandatory = $true)][string]$ScenarioRoot,
    [Parameter(Mandatory = $true)][string]$FromVersion,
    [Parameter(Mandatory = $true)][string]$ToVersion
  )
  $env:RAVYN_HELPER_TEST_ROOT = $ScenarioRoot
  $env:RAVYN_HELPER_FROM_VERSION = $FromVersion
  $env:RAVYN_HELPER_TO_VERSION = $ToVersion
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
  return $script
}

function Assert-HelperGuards {
  param([Parameter(Mandatory = $true)][string]$Script)
  $requiredMarkers = @(
    'Write-Journal',
    'Restore-Installation',
    'The updated version did not reach readiness',
    'reg.exe export',
    'reg.exe import',
    'completed_at_unix_ms'
  )
  foreach ($marker in $requiredMarkers) {
    if (-not $Script.Contains($marker, [System.StringComparison]::Ordinal)) {
      throw "The generated updater helper is missing the required marker: $marker"
    }
  }
}

function Invoke-LifecycleScenario {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][string]$OldExecutable,
    [Parameter(Mandatory = $true)][string]$NewExecutable,
    [Parameter(Mandatory = $true)][string]$InstallerExecutable,
    [Parameter(Mandatory = $true)][ValidateSet('success', 'failure')][string]$ReadinessMode,
    [Parameter(Mandatory = $true)][string]$FromVersion,
    [Parameter(Mandatory = $true)][string]$ToVersion,
    [Parameter(Mandatory = $true)][ValidateSet('succeeded', 'rolled_back')][string]$ExpectedOutcome
  )

  $scenarioRoot = Join-Path $testRoot $Name
  $installDir = Join-Path $scenarioRoot 'install'
  New-Item -ItemType Directory -Path $installDir -Force | Out-Null
  $installed = Join-Path $installDir 'Ravyn.exe'
  $installer = Join-Path $scenarioRoot 'update.exe'
  Copy-Item -LiteralPath $OldExecutable -Destination $installed -Force
  Copy-Item -LiteralPath $InstallerExecutable -Destination $installer -Force
  Set-Content -LiteralPath (Join-Path $scenarioRoot 'pending.json') -Value '{}' -Encoding UTF8
  Set-Content -LiteralPath (Join-Path $scenarioRoot 'transaction.json') -Value '{}' -Encoding UTF8

  $script = Write-GeneratedHelper -ScenarioRoot $scenarioRoot -FromVersion $FromVersion -ToVersion $ToVersion
  Assert-HelperGuards -Script $script

  $env:RAVYN_TEST_UPDATE_PAYLOAD = $NewExecutable
  $env:RAVYN_TEST_INSTALLED = $installed
  $env:RAVYN_TEST_READY_MODE = $ReadinessMode
  $env:RAVYN_TEST_READY_FILE = Join-Path $scenarioRoot 'ready.marker'

  & powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $helperPath
  if ($LASTEXITCODE -ne 0) {
    throw "$Name helper exited with code $LASTEXITCODE."
  }

  $resultPath = Join-Path $scenarioRoot 'result.json'
  if (-not (Test-Path $resultPath)) {
    throw "$Name did not persist an update result."
  }
  $result = Get-Content $resultPath -Raw | ConvertFrom-Json
  if ($result.outcome -ne $ExpectedOutcome) {
    throw "$Name produced outcome '$($result.outcome)' instead of '$ExpectedOutcome'."
  }
  if ($result.from_version -ne $FromVersion -or $result.to_version -ne $ToVersion) {
    throw "$Name persisted incorrect version metadata."
  }
  if (Test-Path (Join-Path $scenarioRoot 'pending.json')) {
    throw "$Name did not remove pending update state."
  }
  if (Test-Path (Join-Path $scenarioRoot 'transaction.json')) {
    throw "$Name did not remove transaction state."
  }

  $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $installed).Hash
  $expectedFile = if ($ExpectedOutcome -eq 'succeeded') { $NewExecutable } else { $OldExecutable }
  $expectedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $expectedFile).Hash
  if ($actualHash -ne $expectedHash) {
    throw "$Name left the wrong executable installed."
  }
  Write-Host "Validated updater lifecycle scenario: $Name ($ExpectedOutcome)."
}

Push-Location $WorkspaceRoot
try {
  New-Item -ItemType Directory -Path $testRoot -Force | Out-Null
  $oldExecutable = Join-Path $testRoot 'old-ravyn.exe'
  $newExecutable = Join-Path $testRoot 'new-ravyn.exe'
  $installerExecutable = Join-Path $testRoot 'mock-installer.exe'

  New-TestExecutable -Output $oldExecutable -Source @'
using System;
public static class OldRavynProgram {
    public static int Main(string[] args) { return 0; }
}
'@
  New-TestExecutable -Output $newExecutable -Source @'
using System;
using System.IO;
using System.Threading;
public static class NewRavynProgram {
    public static int Main(string[] args) {
        string mode = Environment.GetEnvironmentVariable("RAVYN_TEST_READY_MODE");
        string marker = Environment.GetEnvironmentVariable("RAVYN_TEST_READY_FILE");
        if (String.Equals(mode, "success", StringComparison.Ordinal) && !String.IsNullOrEmpty(marker)) {
            File.WriteAllText(marker, "ready");
            Thread.Sleep(1500);
            return 0;
        }
        Thread.Sleep(10000);
        return 0;
    }
}
'@
  New-TestExecutable -Output $installerExecutable -Source @'
using System;
using System.IO;
public static class MockInstallerProgram {
    public static int Main(string[] args) {
        string source = Environment.GetEnvironmentVariable("RAVYN_TEST_UPDATE_PAYLOAD");
        string destination = Environment.GetEnvironmentVariable("RAVYN_TEST_INSTALLED");
        if (String.IsNullOrEmpty(source) || String.IsNullOrEmpty(destination)) return 2;
        File.Copy(source, destination, true);
        return 0;
    }
}
'@

  Invoke-LifecycleScenario -Name 'upgrade-success' -OldExecutable $oldExecutable -NewExecutable $newExecutable -InstallerExecutable $installerExecutable -ReadinessMode success -FromVersion '0.2.0' -ToVersion '0.3.0' -ExpectedOutcome succeeded
  Invoke-LifecycleScenario -Name 'upgrade-rollback' -OldExecutable $oldExecutable -NewExecutable $newExecutable -InstallerExecutable $installerExecutable -ReadinessMode failure -FromVersion '0.2.0' -ToVersion '0.3.0' -ExpectedOutcome rolled_back
  Invoke-LifecycleScenario -Name 'same-version-repair' -OldExecutable $oldExecutable -NewExecutable $newExecutable -InstallerExecutable $installerExecutable -ReadinessMode success -FromVersion '0.3.0' -ToVersion '0.3.0' -ExpectedOutcome succeeded

  Write-Host 'Generated updater helper parsed successfully and passed upgrade, rollback, and repair lifecycle tests.'
} finally {
  foreach ($name in @(
    'RAVYN_HELPER_TEST_ROOT',
    'RAVYN_HELPER_FROM_VERSION',
    'RAVYN_HELPER_TO_VERSION',
    'RAVYN_TEST_UPDATE_PAYLOAD',
    'RAVYN_TEST_INSTALLED',
    'RAVYN_TEST_READY_MODE',
    'RAVYN_TEST_READY_FILE'
  )) {
    Remove-Item "Env:$name" -ErrorAction SilentlyContinue
  }
  Remove-Item $helperPath -Force -ErrorAction SilentlyContinue
  Remove-Item $testRoot -Recurse -Force -ErrorAction SilentlyContinue
  Pop-Location
}
