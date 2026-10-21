#!/bin/sh
# Đổi icon app: thay file app-icon.png (vuông, >= 1024px khuyến nghị) rồi chạy:
#   sh scripts/update-icon.sh
# Cần Pillow cho bước tách hình (Android adaptive + icon thanh menu Mac):
#   python3 -m venv .venv-icons && .venv-icons/bin/pip install pillow
set -e
cd "$(dirname "$0")/.."
npx tauri icon app-icon.png --ios-color "#0067D6"
rm -rf src-tauri/icons/android src-tauri/icons/ios
PY=${PYTHON:-.venv-icons/bin/python}
"$PY" scripts/make-icons.py app-icon.png src-tauri/icons src-tauri/gen/android/app/src/main/res
mv src-tauri/icons/logo-96.png public/logo.png
echo "Xong. Nhớ cập nhật ảnh logo nhúng trong src-tauri/web/index.html nếu cần."
