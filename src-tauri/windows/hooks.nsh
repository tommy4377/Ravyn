!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToLog '"$INSTDIR\Ravyn.exe" --register-firefox-native-host'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog '"$INSTDIR\Ravyn.exe" --unregister-firefox-native-host'
!macroend
