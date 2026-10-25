# BetterDrop

Chuyển file, thư mục và văn bản giữa Mac ⇄ Windows ⇄ Linux ⇄ Android ⇄ iPhone/iPad qua mạng Wi‑Fi/LAN, giống AirDrop.

- Tự tìm máy trong cùng mạng (mDNS/Bonjour). Nếu không thấy, điện thoại **quét mã QR** trên máy tính (Cài đặt › "Kết nối điện thoại bằng mã QR"), hoặc nhập IP.
- Máy nhận tự nhớ máy vừa gửi tới, nên gửi ngược lại được ngay.
- Máy nhận phải bấm **Chấp nhận** mới bắt đầu nhận. Tích **"Luôn chấp nhận từ máy này"** để lần sau nhận luôn (máy tin cậy). Máy gửi phải chứng minh danh tính bằng chứng chỉ của nó (TLS hai chiều), nên máy khác không giả mạo được. Bỏ tin cậy trong Cài đặt.
- **Lịch sử**: mục Hoạt động lưu 1000 lượt gần nhất (kể cả sau khi tắt app), có "Hiện trong Finder/Explorer", "Gửi lại", "Sao chép".
- **Gửi qua Internet** (kiểu WeTransfer, nút ☁ trong "Chọn nội dung"): gửi cho người ở xa, không cần cùng mạng. App **mã hoá ngay trên máy** (AES-256-GCM), tải lên dịch vụ lưu file tạm không cần tài khoản (storage.to: 25 GB, giữ 7 ngày, mặc định; temp.sh: 4 GB, 3 ngày), rồi tạo link `https://gmttuts.github.io/better-drop/receive/#…` kèm mã QR và nút "Gửi link tới máy gần". Khoá nằm sau dấu `#` nên không bao giờ tới máy chủ nào. Người nhận mở link bằng trình duyệt (trang nhận giải mã ngay trên trình duyệt) hoặc dán vào "Nhận bằng link" trong app. Lịch sử giữ link để chép lại. Chi tiết, kết quả kiểm chứng từng dịch vụ: [docs/GUI-QUA-INTERNET.md](docs/GUI-QUA-INTERNET.md).
- **Chia sẻ qua trình duyệt** (nút 🌐 Trình duyệt): thiết bị không cài BetterDrop mở `http://<ip-máy-tính>:53319` (hoặc quét QR), nhập mã PIN, rồi gửi file/văn bản lên hoặc tải những gì máy tính chia sẻ. Chỉ trong mạng LAN, mặc định tắt.
- Dữ liệu đi qua HTTPS (TLS). Mỗi máy có chứng chỉ riêng, và bên gửi chỉ chấp nhận đúng chứng chỉ đã quảng bá. Mã 4 ký tự (vd `520D`) dùng để so với máy kia.
- **Gửi tiếp khi rớt mạng**: mất Wi‑Fi giữa chừng thì tự gửi tiếp từ chỗ đang dở (chờ tối đa 2 phút), không gửi lại từ đầu.
- **Nhóm máy**: chia sẻ một nhóm cho máy khác bằng nút "Gửi nhóm".
- **Chia sẻ từ app khác**: Android "Chia sẻ → BetterDrop", Mac "Mở bằng"/kéo vào Dock, Windows chuột phải › "Send to › BetterDrop", iPhone/iPad "Chia sẻ → BetterDrop" (ảnh, video, file trong app Tệp, link và văn bản từ Safari…) hoặc "Mở bằng". File/văn bản được đưa sẵn vào danh sách gửi.
- **Gửi nhanh từ thanh menu/khay hệ thống** (Mac/Windows/Linux): bấm biểu tượng BetterDrop › "Gửi clipboard tới" hoặc "Gửi file tới" › chọn máy, không cần mở cửa sổ. Kết quả báo bằng thông báo hệ thống và vẫn ghi vào lịch sử.
- **Phím tắt toàn cục** (Mac/Windows/Linux): ⌘⇧D / Ctrl+Shift+D gửi văn bản trong clipboard tới máy gần nhất (máy đang bật mà bạn gửi tới gần đây nhất; nếu có nhiều máy mà chưa gửi lần nào thì mở cửa sổ ở tab Văn bản với nội dung đã điền sẵn). ⌘⇧B / Ctrl+Shift+B mở BetterDrop. Bật/tắt và đổi tổ hợp trong Cài đặt › Phím tắt.
- **Tiếng Anh**: Cài đặt › Ngôn ngữ (Tự động/Tiếng Việt/English).
- **Tự cập nhật** (Mac/Windows/Linux AppImage): mở app sẽ tự xem có bản phát hành mới trên GitHub, bấm "Cập nhật và mở lại". Bản cập nhật được ký, app kiểm tra chữ ký trước khi cài.
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

**GitHub Actions**:
- Đẩy code lên `main`: chỉ chạy test (`test.yml`, một máy Linux).
- Gắn tag `v*` (vd `git tag v0.3.7 && git push origin v0.3.7`): build đủ mọi nền tảng và tạo bản phát hành nháp (`build.yml`).
- Muốn lấy file cài thử mà không phát hành: vào tab Actions › Build › **Run workflow**, tích nền tảng cần build (macOS/iOS tốn nhất nên mặc định tắt). File nằm trong mục Artifacts, giữ 5 ngày.

**macOS** — tạo `.dmg` trong `src-tauri/target/release/bundle/dmg/`:

```bash
npm run tauri build
```

**Windows**: phải build trên máy Windows.
1. Cài [Rust](https://rustup.rs), [Node.js LTS](https://nodejs.org) và "Desktop development with C++" (Visual Studio Build Tools). WebView2 có sẵn trên Windows 10/11.
2. Chép thư mục dự án sang (không cần `node_modules`, `src-tauri/target`), rồi chạy `npm install` và `npm run tauri build`.
3. Bộ cài nằm ở `src-tauri\target\release\bundle\msi\` và `...\nsis\`.

**Khoá ký bản cập nhật**: bản phát hành trên GitHub được ký bằng khoá trong GitHub Secret `TAURI_SIGNING_PRIVATE_KEY` (+ `_PASSWORD`); bản sao nằm ở `~/.tauri/betterdrop-updater.key` trên máy của chủ dự án. **Mất khoá thì các bản đã cài không tự cập nhật được nữa**, hãy cất giữ cẩn thận. Build bản có ký trên máy: đặt hai biến môi trường đó rồi chạy `npm run tauri build -- --config src-tauri/tauri.updater.conf.json`. App chỉ thấy bản mới khi bản phát hành đã **Publish** (không phải nháp) và có file `latest.json`.

## Điện thoại

**Android**: tải `BetterDrop-android.apk` từ trang Releases (hoặc Artifacts của lần chạy Build), mở trên điện thoại và cho phép "Cài ứng dụng không rõ nguồn gốc". File nhận được nằm ở `Download/BetterDrop` (xem bằng app Files).
- **Nhận file khi chạy nền** (Cài đặt › Chạy nền, mặc định bật): bấm Home hay tắt màn hình thì máy khác vẫn thấy điện thoại và gửi được. BetterDrop hiện thông báo thường trực "BetterDrop đang sẵn sàng nhận file" (nút **Tắt** để tắt chế độ này) và giữ Wi‑Fi/mDNS hoạt động (Foreground Service loại `connectedDevice`).
- Có máy gửi tới khi app ở nền: hiện thông báo, chạm vào để mở app và bấm Chấp nhận/Từ chối (trong 60 giây). Máy tin cậy thì nhận luôn, xong sẽ có thông báo. Android 13+ hỏi quyền hiện thông báo khi mở app lần đầu; từ chối thì vẫn nhận được nhưng không thấy báo.
- Vuốt BetterDrop khỏi danh sách ứng dụng gần đây là thoát hẳn (như "Thoát" trên máy tính). Một số hãng (Xiaomi, Oppo…) còn tự tắt app chạy nền: cho BetterDrop "Không hạn chế pin" nếu bị tắt.

**iPhone/iPad**: cần build bằng Xcode trên máy Mac.
1. Một lần duy nhất: `sudo xcode-select -s /Applications/Xcode.app/Contents/Developer`.
2. `source scripts/mobile-env.sh && npm run tauri ios dev` (máy ảo), hoặc `npm run tauri ios build -- --open`. Sau đó trong Xcode, chọn *Signing & Capabilities › Team* là Apple ID của bạn, cắm iPhone và bấm Run.
3. Nếu dùng Apple ID miễn phí, app hết hạn sau 7 ngày (phải cài lại). Tài khoản Apple Developer (99 USD/năm) thì không hết hạn và đưa lên TestFlight được.
4. File nhận được nằm trong app **Tệp › Trên iPhone › BetterDrop**.
5. iOS dừng app khi chuyển ra màn hình khác, nên **phải mở BetterDrop khi muốn nhận file**.
6. **Chia sẻ → BetterDrop**: nếu chưa thấy BetterDrop trong hàng app của bảng Chia sẻ, kéo hàng đó sang phải, bấm *Thêm* và bật BetterDrop. Sau khi chia sẻ, BetterDrop tự mở (nếu iOS không cho thì tự mở app); mục vừa chia sẻ nằm sẵn trong danh sách gửi.
   - Phần này là một Share Extension (`src-tauri/gen/apple/ShareExtension`), chép mục được chia sẻ vào **App Group** `group.com.nguyentuyen.betterdrop` để app chính đọc. Khi ký bằng Xcode, bật App Groups cho cả hai target `better-drop_iOS` và `BetterDropShare` với cùng một mã nhóm.
   - Cài bằng Sideloadly/AltStore: IPA trên GitHub mang sẵn entitlements App Group (ký ad-hoc, `scripts/ios-add-entitlements.sh`) để công cụ ký lại xin nhóm cho bạn; app tự đọc mã nhóm thật trong hồ sơ cấp phép nên mã có bị đổi vẫn chạy. Extension tính thêm một App ID (Apple ID miễn phí chỉ có vài App ID mỗi tuần). Nếu công cụ không cấp được App Group (hoặc bỏ extension), **link và văn bản vẫn gửi được** (qua `betterdrop://share?text=…`), còn **file thì không**: mở BetterDrop và bấm *Chọn file* (bộ chọn file của iOS đọc được cả Ảnh lẫn app Tệp).

Build Android trên máy: `source scripts/mobile-env.sh && npm run tauri android build -- --apk` (cần Java 17, Android SDK/NDK).

## Lưu ý mạng

- **Windows**: lần đầu mở app, Windows Firewall sẽ hỏi. Hãy cho phép **Private networks**. Mạng Wi‑Fi phải được đặt là *Private*, nếu để *Public* thì mDNS bị chặn.
- **macOS**: khi hiện hộp thoại "Local Network", hãy bấm **Allow**.
- Cổng mặc định là `53318/TCP` (app với app) và `53319/TCP` (chia sẻ qua trình duyệt), còn mDNS dùng `5353/UDP`.
- Wi‑Fi khách hoặc Wi‑Fi công ty có thể bật "client isolation". Khi đó hai máy không thể nói chuyện trực tiếp với nhau, kể cả khi nhập IP.

## Tương thích giữa các phiên bản

Mọi phiên bản BetterDrop gửi nhận được với nhau. Giao thức `/api/v1` (info, prepare, upload, cancel) và các khoá mDNS (`id`, `name`, `os`, `fp`) giữ nguyên từ v0.1.0. Khi thêm tính năng:

- Chỉ **thêm** trường mới, kèm `#[serde(default)]`. Không đổi tên, không xoá, không đổi kiểu trường cũ. Bản cũ tự bỏ qua trường lạ.
- Không đổi đường dẫn API cũ. Nếu buộc phải thay đổi không tương thích, làm API mới (`/api/v2/...`) và giữ `/api/v1` song song.
- Test `new_version_reads_old_wire_format`, `old_version_can_read_new_wire_format`, `old_client_can_send_to_new_version` và `old_saved_files_still_load` (trong `src-tauri/src/tests.rs`) dùng đúng dữ liệu của v0.1.0. CI chạy chúng ở mọi lần đẩy code.
- Từ v0.3.2, mỗi máy báo phiên bản (`version` trong `/api/v1/info`, khoá `v` trong mDNS). Danh sách máy hiện phiên bản và đánh dấu máy dùng bản cũ hơn.

Lưu ý: tương thích giao thức không giúp được khi hệ điều hành chặn kết nối. Ví dụ Android 17 cần BetterDrop **v0.2.2 trở lên** trên chính điện thoại đó, vì từ bản này app mới xin quyền "Thiết bị lân cận".

## Cấu trúc

| File | Vai trò |
|---|---|
| `src-tauri/src/discovery.rs` | Quảng bá và tìm máy qua mDNS, thêm máy bằng IP |
| `src-tauri/src/server.rs` | Máy chủ HTTPS: `/info`, `/prepare` (xin phép), `/upload` (stream), `/cancel` |
| `src-tauri/src/transfer.rs` | Phía gửi: gom file và thư mục, gửi 3 file song song, huỷ |
| `src-tauri/src/client.rs` | HTTP client kiểm tra chứng chỉ bằng fingerprint |
| `src-tauri/src/paths.rs` | Làm sạch đường dẫn (chặn `../`, ký tự cấm trên Windows, chuẩn hoá tên tiếng Việt) |
| `src-tauri/src/identity.rs` | Cài đặt và chứng chỉ tự ký |
| `src-tauri/src/tray.rs` | Thanh menu/khay hệ thống: chạy nền, thông báo, gửi nhanh clipboard/file |
| `src-tauri/src/online/` | Gửi qua Internet: mã hoá theo khối (`crypto.rs`), đóng gói zip (`package.rs`, `zip.rs`), dịch vụ lưu file (`providers.rs`) |
| `website/receive/` | Trang nhận link "Gửi qua Internet" (giải mã bằng WebCrypto, lưu thẳng xuống đĩa) |
| `src/main.ts`, `index.html`, `src/styles.css` | Giao diện |
