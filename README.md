# BetterDrop

Chuyển file, thư mục và văn bản giữa Mac ⇄ Windows ⇄ Linux qua mạng Wi‑Fi/LAN, giống AirDrop.

- Tự tìm máy trong cùng mạng (mDNS/Bonjour), hoặc thêm thủ công bằng IP.
- Máy nhận phải bấm **Chấp nhận** mới bắt đầu nhận.
- Dữ liệu đi qua HTTPS (TLS). Mỗi máy có chứng chỉ riêng, và bên gửi chỉ chấp nhận đúng chứng chỉ đã quảng bá. Mã 4 ký tự (vd `520D`) dùng để so với máy kia.
- File nhận được lưu vào `~/Downloads/BetterDrop` (đổi được trong Cài đặt). Nếu trùng tên, file mới được thêm hậu tố ` (1)`, không ghi đè file cũ.

## Chạy khi phát triển

```bash
npm install
npm run tauri dev
```

Chạy thêm bản thứ hai trên cùng máy để thử (dùng thư mục dữ liệu và cổng riêng):

```bash
BETTERDROP_PROFILE=b ./src-tauri/target/debug/better-drop
```

Chạy test (gồm test gửi file/thư mục thật giữa hai "máy" trên localhost):

```bash
cd src-tauri && cargo test
```

## Đóng gói

**macOS** — tạo `.dmg` trong `src-tauri/target/release/bundle/dmg/`:

```bash
npm run tauri build
```

**Windows**: phải build trên máy Windows.
1. Cài [Rust](https://rustup.rs), [Node.js LTS](https://nodejs.org) và "Desktop development with C++" (Visual Studio Build Tools). WebView2 có sẵn trên Windows 10/11.
2. Chép thư mục dự án sang (không cần `node_modules`, `src-tauri/target`), rồi chạy `npm install` và `npm run tauri build`.
3. Bộ cài nằm ở `src-tauri\target\release\bundle\msi\` và `...\nsis\`.

## Lưu ý mạng

- **Windows**: lần đầu mở app, Windows Firewall sẽ hỏi. Hãy cho phép **Private networks**. Mạng Wi‑Fi phải được đặt là *Private*, nếu để *Public* thì mDNS bị chặn.
- **macOS**: khi hiện hộp thoại "Local Network", hãy bấm **Allow**.
- Cổng mặc định là `53318/TCP`, còn mDNS dùng `5353/UDP`.
- Wi‑Fi khách hoặc Wi‑Fi công ty có thể bật "client isolation". Khi đó hai máy không thể nói chuyện trực tiếp với nhau, kể cả khi nhập IP.

## Cấu trúc

| File | Vai trò |
|---|---|
| `src-tauri/src/discovery.rs` | Quảng bá và tìm máy qua mDNS, thêm máy bằng IP |
| `src-tauri/src/server.rs` | Máy chủ HTTPS: `/info`, `/prepare` (xin phép), `/upload` (stream), `/cancel` |
| `src-tauri/src/transfer.rs` | Phía gửi: gom file và thư mục, gửi 3 file song song, huỷ |
| `src-tauri/src/client.rs` | HTTP client kiểm tra chứng chỉ bằng fingerprint |
| `src-tauri/src/paths.rs` | Làm sạch đường dẫn (chặn `../`, ký tự cấm trên Windows, chuẩn hoá tên tiếng Việt) |
| `src-tauri/src/identity.rs` | Cài đặt và chứng chỉ tự ký |
| `src/main.ts`, `index.html`, `src/styles.css` | Giao diện |
