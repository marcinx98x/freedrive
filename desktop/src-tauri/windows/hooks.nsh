; FreeDrive NSIS installer hooks — My Drive / CfAPI cleanup on uninstall.
; MAINBINARYNAME is defined by Tauri's installer.nsi (cargo binary name by default).

!macro NSIS_HOOK_PREUNINSTALL
  ; Stop a running client so CfAPI disconnect and file deletes can succeed.
  ExecWait 'taskkill /F /IM "${MAINBINARYNAME}.exe" /T'
  ; Exe still exists in $INSTDIR during PREUNINSTALL.
  IfFileExists "$INSTDIR\${MAINBINARYNAME}.exe" 0 +2
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-cleanup'
!macroend
