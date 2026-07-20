//! PowerShell update transaction helper generation.

use super::*;

pub(super) fn build_installer_helper_script(
    transaction: &PendingUpdateTransaction,
    parent_pid: u32,
) -> String {
    build_installer_helper_script_with_timeout(transaction, parent_pid, READINESS_TIMEOUT_SECS)
}

pub(super) fn build_installer_helper_script_with_timeout(
    transaction: &PendingUpdateTransaction,
    parent_pid: u32,
    readiness_timeout_secs: u64,
) -> String {
    use std::fmt::Write as _;

    let shortcuts = transaction
        .shortcuts
        .iter()
        .map(|path| powershell_literal(path))
        .collect::<Vec<_>>()
        .join(",");
    let shortcuts = if shortcuts.is_empty() {
        "@()".to_owned()
    } else {
        format!("@({shortcuts})")
    };

    let mut script = String::new();
    writeln!(&mut script, "$ErrorActionPreference='Stop';").unwrap();
    writeln!(&mut script, "$parentPid={parent_pid};").unwrap();
    for (name, path) in [
        ("installDir", &transaction.install_dir),
        ("installed", &transaction.installed_exe),
        ("backupDir", &transaction.backup_dir),
        ("shortcutBackupDir", &transaction.shortcuts_backup_dir),
        ("regUninstallBackup", &transaction.registry_uninstall_backup),
        ("regRunBackup", &transaction.registry_run_backup),
        ("journal", &transaction.journal_path),
        ("installer", &transaction.installer_path),
        ("ready", &transaction.readiness_marker),
        ("transactionPath", &transaction.transaction_path),
        ("pendingStatePath", &transaction.pending_state_path),
        ("resultPath", &transaction.result_path),
    ] {
        writeln!(&mut script, "${name}={};", powershell_literal(path)).unwrap();
    }
    writeln!(&mut script, "$shortcuts={shortcuts};").unwrap();
    writeln!(
        &mut script,
        "$fromVersion={};",
        powershell_string(&transaction.from_version)
    )
    .unwrap();
    writeln!(
        &mut script,
        "$toVersion={};",
        powershell_string(&transaction.to_version)
    )
    .unwrap();
    writeln!(&mut script, "$timeoutSeconds={readiness_timeout_secs};").unwrap();
    writeln!(
        &mut script,
        "$regUninstallKey={};",
        powershell_string(REGISTRY_UNINSTALL_KEY)
    )
    .unwrap();
    writeln!(
        &mut script,
        "$regRunKey={};",
        powershell_string(REGISTRY_RUN_KEY)
    )
    .unwrap();
    script.push_str(
        "function Write-Journal([string]$phase) { try { Set-Content -LiteralPath $journal -Value $phase -Force } catch {} };\n\
         function Restore-Installation {\n\
           Write-Journal 'rollback';\n\
           if (Test-Path -LiteralPath $backupDir) {\n\
             Get-ChildItem -LiteralPath $installDir -File -Force -ErrorAction SilentlyContinue | Where-Object { $_.Extension -in '.exe','.dll' } | Remove-Item -Force -ErrorAction SilentlyContinue;\n\
             Get-ChildItem -LiteralPath $backupDir -File -Force | Copy-Item -Destination $installDir -Force;\n\
           };\n\
           if ($script:uninstallKeyExisted) { if (Test-Path -LiteralPath $regUninstallBackup) { reg.exe import $regUninstallBackup | Out-Null } }\n\
           else { reg.exe delete $regUninstallKey /f | Out-Null };\n\
           if ($script:runHadRavyn) { if (Test-Path -LiteralPath $regRunBackup) { reg.exe import $regRunBackup | Out-Null } }\n\
           else { Remove-ItemProperty -Path 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Run' -Name 'Ravyn' -Force -ErrorAction SilentlyContinue };\n\
           $i=0;\n\
           foreach ($link in $shortcuts) {\n\
             $saved=Join-Path $shortcutBackupDir (\"$i.lnk\");\n\
             if (Test-Path -LiteralPath $saved) { Copy-Item -LiteralPath $saved -Destination $link -Force -ErrorAction SilentlyContinue }\n\
             elseif (Test-Path -LiteralPath $link) { Remove-Item -LiteralPath $link -Force -ErrorAction SilentlyContinue };\n\
             $i++;\n\
           };\n\
         };\n\
         $ravyn=Get-Process -Id $parentPid -ErrorAction SilentlyContinue;\n\
         if ($null -ne $ravyn) { $ravyn.WaitForExit() };\n\
         $outcome='failed'; $message=''; $launched=$null;\n\
         $script:uninstallKeyExisted=$false; $script:runHadRavyn=$false;\n\
         try {\n\
           Write-Journal 'backup';\n\
           Remove-Item -LiteralPath $ready -Force -ErrorAction SilentlyContinue;\n\
           if (Test-Path -LiteralPath $backupDir) { Remove-Item -LiteralPath $backupDir -Recurse -Force };\n\
           New-Item -ItemType Directory -Force -Path $backupDir | Out-Null;\n\
           Get-ChildItem -LiteralPath $installDir -File -Force | Where-Object { $_.Extension -in '.exe','.dll' } | Copy-Item -Destination $backupDir -Force;\n\
           $script:uninstallKeyExisted=Test-Path 'Registry::HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Ravyn';\n\
           if ($script:uninstallKeyExisted) { reg.exe export $regUninstallKey $regUninstallBackup /y | Out-Null };\n\
           $script:runHadRavyn=$null -ne (Get-ItemProperty -Path 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Run' -Name 'Ravyn' -ErrorAction SilentlyContinue);\n\
           if ($script:runHadRavyn) { reg.exe export $regRunKey $regRunBackup /y | Out-Null };\n\
           New-Item -ItemType Directory -Force -Path $shortcutBackupDir | Out-Null;\n\
           $i=0;\n\
           foreach ($link in $shortcuts) {\n\
             if (Test-Path -LiteralPath $link) { Copy-Item -LiteralPath $link -Destination (Join-Path $shortcutBackupDir (\"$i.lnk\")) -Force };\n\
             $i++;\n\
           };\n\
           Write-Journal 'install';\n\
           Copy-Item -LiteralPath $installer -Destination $installed -Force;\n\
           if (Test-Path 'Registry::HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Ravyn') { Set-ItemProperty -Path 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Ravyn' -Name 'DisplayVersion' -Value $toVersion -ErrorAction SilentlyContinue };\n\
           Write-Journal 'verify';\n\
           $launched=Start-Process -FilePath $installed -PassThru;\n\
           $deadline=(Get-Date).AddSeconds($timeoutSeconds);\n\
           while ((Get-Date) -lt $deadline) {\n\
             if (Test-Path -LiteralPath $ready) { $outcome='succeeded'; $message='The updated version reached backend and UI readiness.'; break };\n\
             if ($launched.HasExited) { break };\n\
             Start-Sleep -Milliseconds 500;\n\
           };\n\
           if ($outcome -ne 'succeeded') {\n\
             $message='The updated version did not reach readiness before the safety deadline.';\n\
             if (($null -ne $launched) -and (!$launched.HasExited)) { Stop-Process -Id $launched.Id -Force -ErrorAction SilentlyContinue; Wait-Process -Id $launched.Id -Timeout 10 -ErrorAction SilentlyContinue };\n\
             Restore-Installation;\n\
             $outcome='rolled_back';\n\
           };\n\
         } catch {\n\
           $message=$_.Exception.Message;\n\
           if (($null -ne $launched) -and (!$launched.HasExited)) { Stop-Process -Id $launched.Id -Force -ErrorAction SilentlyContinue; Wait-Process -Id $launched.Id -Timeout 10 -ErrorAction SilentlyContinue };\n\
           if (Test-Path -LiteralPath $backupDir) {\n\
             Restore-Installation;\n\
             $outcome='rolled_back';\n\
           };\n\
         };\n\
         Write-Journal 'finalize';\n\
         Remove-Item -LiteralPath $backupDir -Recurse -Force -ErrorAction SilentlyContinue;\n\
         Remove-Item -LiteralPath $shortcutBackupDir -Recurse -Force -ErrorAction SilentlyContinue;\n\
         Remove-Item -LiteralPath $regUninstallBackup -Force -ErrorAction SilentlyContinue;\n\
         Remove-Item -LiteralPath $regRunBackup -Force -ErrorAction SilentlyContinue;\n\
         if ($outcome -eq 'succeeded') {\n\
           Remove-Item -LiteralPath $installer -Force -ErrorAction SilentlyContinue;\n\
         };\n\
         try {\n\
           $resultObject=[ordered]@{outcome=$outcome;from_version=$fromVersion;to_version=$toVersion;completed_at_unix_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds();message=$message};\n\
           $json=$resultObject | ConvertTo-Json -Compress;\n\
           $resultTemp=\"$resultPath.tmp\";\n\
           [System.IO.File]::WriteAllText($resultTemp,$json,(New-Object System.Text.UTF8Encoding($false)));\n\
           Remove-Item -LiteralPath $resultPath -Force -ErrorAction SilentlyContinue;\n\
           Move-Item -LiteralPath $resultTemp -Destination $resultPath;\n\
         } catch {\n\
           $message=\"$message Result persistence failed: $($_.Exception.Message)\".Trim();\n\
         } finally {\n\
           Remove-Item -LiteralPath $ready -Force -ErrorAction SilentlyContinue;\n\
           Remove-Item -LiteralPath $pendingStatePath -Force -ErrorAction SilentlyContinue;\n\
           Remove-Item -LiteralPath $transactionPath -Force -ErrorAction SilentlyContinue;\n\
           Remove-Item -LiteralPath $journal -Force -ErrorAction SilentlyContinue;\n\
           if (($outcome -eq 'rolled_back') -or ($outcome -eq 'failed')) { if (Test-Path -LiteralPath $installed) { Start-Process -FilePath $installed | Out-Null } };\n\
         };\n\
         if ($outcome -eq 'failed') { exit 1 } else { exit 0 };\n",
    );
    script
}

pub(super) fn powershell_literal(path: &Path) -> String {
    powershell_string(&path.to_string_lossy())
}

pub(super) fn powershell_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

