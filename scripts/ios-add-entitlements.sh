#!/bin/bash
# Gắn entitlements (App Group cho "Chia sẻ → BetterDrop") vào IPA chưa ký bằng chữ ký ad-hoc.
# `tauri ios build --no-sign` bỏ hết entitlements; công cụ ký lại (Sideloadly, AltStore…) đọc
# entitlements có sẵn trong file để biết cần xin App Group cho app và Share Extension.
# Dùng: scripts/ios-add-entitlements.sh <file.ipa>
set -euo pipefail
ipa="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
root="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/gen/apple"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cd "$work"
unzip -q "$ipa"
app=$(echo Payload/*.app)
appex="$app/PlugIns/BetterDropShare.appex"
[ -d "$appex" ] || { echo "Không thấy $appex trong IPA" >&2; exit 1; }

# Ký từ trong ra ngoài: extension trước, app sau.
codesign --force --sign - --entitlements "$root/ShareExtension/ShareExtension.entitlements" "$appex"
codesign --force --sign - --entitlements "$root/better-drop_iOS/better-drop_iOS.entitlements" "$app"

rm "$ipa"
zip -qr -y "$ipa" Payload
codesign -d --entitlements - "$appex" 2>/dev/null | grep -q application-groups
echo "Đã gắn entitlements vào $ipa"
