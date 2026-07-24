; FreeDrive NSIS installer hooks — My Drive / CfAPI + AppData cleanup on uninstall.
; MAINBINARYNAME is defined by Tauri's installer.nsi (cargo binary name by default).
; App data lives in %APPDATA%\FreeDrive (not com.freedrive.desktop / BUNDLEID).

!macro NSIS_HOOK_PREUNINSTALL
  ; Stop a running client so CfAPI disconnect and file deletes can succeed.
  ExecWait 'taskkill /F /IM "${MAINBINARYNAME}.exe" /T'
  ; Exe still exists in $INSTDIR during PREUNINSTALL.
  IfFileExists "$INSTDIR\${MAINBINARYNAME}.exe" 0 +2
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-cleanup'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; When "Delete application data" is checked, also remove FreeDrive AppData.
  ; (Tauri only deletes %APPDATA%\<BUNDLEID>; our data_dir is FreeDrive.)
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    RmDir /r "$APPDATA\FreeDrive"
    RmDir /r "$LOCALAPPDATA\FreeDrive"
  ${EndIf}
!macroend
