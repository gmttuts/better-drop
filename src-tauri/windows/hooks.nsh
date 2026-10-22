; Thêm "BetterDrop" vào menu chuột phải "Send to" (Gửi tới) của Windows.
; Chọn file/thư mục → Send to → BetterDrop: app mở ra với các file đó trong danh sách chờ gửi.
!macro NSIS_HOOK_POSTINSTALL
  CreateShortCut "$SENDTO\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe" "" "$INSTDIR\${MAINBINARYNAME}.exe" 0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$SENDTO\${PRODUCTNAME}.lnk"
!macroend
