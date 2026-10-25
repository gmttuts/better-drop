; Thêm "BetterDrop" vào menu chuột phải "Send to" (Gửi tới) của Windows.
; Chọn file/thư mục → Send to → BetterDrop: app mở ra với các file đó trong danh sách chờ gửi.
;
; Thêm mục "Gửi bằng BetterDrop" vào menu chuột phải của file và thư mục (chỉ cho người dùng
; hiện tại, HKCU, không cần quyền admin). Khi chạy, app ghi lại mục này thành menu con có danh
; sách máy (src-tauri/src/context_menu.rs); gỡ cài đặt thì xoá cả khoá.
!define BETTERDROP_MENU_FILES "Software\Classes\*\shell\BetterDrop"
!define BETTERDROP_MENU_DIRS "Software\Classes\Directory\shell\BetterDrop"

!macro BETTERDROP_MENU_KEY ROOT LABEL
  ; Xoá bản cũ (có thể là menu con app đã ghi) rồi ghi mục cơ bản.
  DeleteRegKey HKCU "${ROOT}"
  WriteRegStr HKCU "${ROOT}" "MUIVerb" "${LABEL}"
  WriteRegStr HKCU "${ROOT}" "Icon" '"$INSTDIR\${MAINBINARYNAME}.exe",0'
  ; Chọn nhiều file vẫn hiện mục này; mỗi file chạy app một lần, single-instance gom lại.
  WriteRegStr HKCU "${ROOT}" "MultiSelectModel" "Player"
  WriteRegStr HKCU "${ROOT}\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
!macroend

!macro NSIS_HOOK_POSTINSTALL
  CreateShortCut "$SENDTO\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe" "" "$INSTDIR\${MAINBINARYNAME}.exe" 0

  ; Chữ trên menu theo ngôn ngữ Windows (1066 = tiếng Việt); app sẽ ghi lại theo cài đặt của nó.
  Push $0
  Push $1
  System::Call 'kernel32::GetUserDefaultUILanguage() i .r0'
  StrCpy $1 "Send with BetterDrop"
  IntCmp $0 1066 0 +2 +2
    StrCpy $1 "G${U+1EED}i b${U+1EB1}ng BetterDrop"
  !insertmacro BETTERDROP_MENU_KEY "${BETTERDROP_MENU_FILES}" "$1"
  !insertmacro BETTERDROP_MENU_KEY "${BETTERDROP_MENU_DIRS}" "$1"
  Pop $1
  Pop $0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$SENDTO\${PRODUCTNAME}.lnk"
  DeleteRegKey HKCU "${BETTERDROP_MENU_FILES}"
  DeleteRegKey HKCU "${BETTERDROP_MENU_DIRS}"
!macroend
