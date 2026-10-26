# BetterDrop

Chuyển file, thư mục và văn bản giữa Mac ⇄ Windows ⇄ Linux ⇄ Android ⇄ iPhone/iPad qua mạng Wi‑Fi/LAN, giống AirDrop.

- Tự tìm máy trong cùng mạng (mDNS/Bonjour). Nếu không thấy, điện thoại **quét mã QR** trên máy tính (Cài đặt › "Kết nối điện thoại bằng mã QR"), hoặc nhập IP.
- Máy nhận tự nhớ máy vừa gửi tới, nên gửi ngược lại được ngay.
- Máy nhận phải bấm **Chấp nhận** mới bắt đầu nhận. Tích **"Luôn chấp nhận từ máy này"** để lần sau nhận luôn (máy tin cậy). Máy gửi phải chứng minh danh tính bằng chứng chỉ của nó (TLS hai chiều), nên máy khác không giả mạo được. Bỏ tin cậy trong Cài đặt.
- **Quy tắc nhận**: mỗi máy có thể có **thư mục nhận riêng** (vd file từ "iPhone của Lan" vào `~/Downloads/BetterDrop/iPhone của Lan`; điện thoại chỉ cho thư mục con trong thư mục nhận) và điều kiện **tự nhận không hỏi** cho máy chưa tin cậy: tổng dung lượng ≤ N MB và/hoặc chỉ ảnh/video. Đặt quy tắc chung trong Cài đặt › Nhận file (mặc định tắt), quy tắc riêng bằng nút hình thư mục cạnh tên máy (hoặc "Quy tắc…" trong danh sách máy tin cậy); quy tắc riêng ghi đè quy tắc chung. Chỉ áp dụng cho máy bạn đã thêm (IP/QR) hoặc đã từng bấm Chấp nhận, hoặc máy có quy tắc riêng; máy lạ luôn được hỏi, kể cả khi nó từng gửi văn bản tới (gửi văn bản không cần ai đồng ý). Quy tắc gắn với dấu vân tay chứng chỉ như máy tin cậy: máy khác khai cùng id không được hưởng.
- **Đồng bộ clipboard** (Cài đặt › Đồng bộ clipboard, mặc định tắt): chọn các máy tin cậy để đồng bộ. Máy tính đọc clipboard mỗi giây; chép văn bản mới (≤ 100 KB) là gửi tới các máy đã chọn đang bật, máy đó ghi thẳng vào clipboard và chỉ hiện một thông báo nhỏ. Văn bản vừa nhận không bị gửi ngược lại, chuỗi giống hệt lần trước thì bỏ qua. Máy không tin cậy hoặc chưa bật đồng bộ thì văn bản hiện như văn bản thường. Các lượt đồng bộ **không ghi vào Lịch sử** (mỗi lần chép một dòng thì Lịch sử thành rác, và nội dung đã nằm sẵn trong clipboard). **Điện thoại chỉ nhận**: Android/iOS không cho app đọc clipboard ở nền; để gửi, chọn máy rồi bấm **Gửi clipboard** trong tab Văn bản. Máy chạy bản cũ hơn (chưa hiểu đồng bộ clipboard) không được tự gửi tới, để không bật hộp thoại văn bản mỗi lần chép.
- **Lịch sử**: mục Hoạt động lưu 1000 lượt gần nhất (kể cả sau khi tắt app), có "Hiện trong Finder/Explorer", "Gửi lại", "Sao chép".
- **Gửi qua Internet** (kiểu WeTransfer, nút ☁ trong "Chọn nội dung"): gửi cho người ở xa, không cần cùng mạng. App **mã hoá ngay trên máy** (AES-256-GCM), tải lên dịch vụ lưu file tạm không cần tài khoản (storage.to: 25 GB, giữ 7 ngày, mặc định; temp.sh: 4 GB, 3 ngày), rồi tạo link `https://gmttuts.github.io/better-drop/receive/#…` kèm mã QR và nút "Gửi link tới máy gần". Khoá nằm sau dấu `#` nên không bao giờ tới máy chủ nào. Người nhận mở link bằng trình duyệt (trang nhận giải mã ngay trên trình duyệt) hoặc dán vào "Nhận bằng link" trong app. Lịch sử giữ link để chép lại. Chi tiết, kết quả kiểm chứng từng dịch vụ: [docs/GUI-QUA-INTERNET.md](docs/GUI-QUA-INTERNET.md).
- **Chia sẻ qua trình duyệt** (nút 🌐 Trình duyệt): thiết bị không cài BetterDrop mở `http://<ip-máy-tính>:53319` (hoặc quét QR), nhập mã PIN, rồi gửi file/văn bản lên hoặc tải những gì máy tính chia sẻ. Chỉ trong mạng LAN, mặc định tắt.
- Dữ liệu đi qua HTTPS (TLS). Mỗi máy có chứng chỉ riêng, và bên gửi chỉ chấp nhận đúng chứng chỉ đã quảng bá. Mã 4 ký tự (vd `520D`) dùng để so với máy kia.
- **Gửi tiếp khi rớt mạng**: mất Wi‑Fi giữa chừng thì tự gửi tiếp từ chỗ đang dở (chờ tối đa 2 phút), không gửi lại từ đầu.
- **Tạm dừng / Tiếp tục**: nút ⏸/▶ cạnh nút Huỷ của lượt đang gửi. Tạm dừng thì ngắt kết nối nhưng máy nhận giữ phần đã nhận (hiện "Đã tạm dừng"); tiếp tục thì gửi tiếp từ chỗ đang dở, bao lâu cũng được (máy nhận giữ tối đa 24 giờ). Máy nhận dùng bản cũ (trước 0.3.7) chỉ chờ 2 phút: tạm dừng lâu hơn thì khi tiếp tục, app xin phiên mới và gửi lại các file chưa xong (file đang dở gửi lại từ đầu, file đã xong không gửi lại, máy nhận phải bấm Chấp nhận lại nếu chưa tin cậy). App báo điều này ngay khi bấm tạm dừng.
- **Giới hạn tốc độ gửi**: Cài đặt › "Giới hạn tốc độ gửi" (Không giới hạn / 1 / 5 / 10 / 50 MB/s hoặc tự nhập). Một giới hạn chung cho mọi lượt đang gửi, đổi là có hiệu lực ngay. Có cả trên điện thoại, tiện khi dùng mạng di động làm điểm phát Wi‑Fi.
- **Nhóm máy**: chia sẻ một nhóm cho máy khác bằng nút "Gửi nhóm".
- **Chia sẻ từ app khác**: Android "Chia sẻ → BetterDrop", Mac "Mở bằng"/kéo vào Dock, Windows chuột phải › "Gửi bằng BetterDrop" (hoặc "Send to › BetterDrop"), iPhone/iPad "Chia sẻ → BetterDrop" (ảnh, video, file trong app Tệp, link và văn bản từ Safari…) hoặc "Mở bằng". File/văn bản được đưa sẵn vào danh sách gửi.
- **Menu chuột phải trên Windows**: chọn file/thư mục (một hay nhiều) › chuột phải › **Gửi bằng BetterDrop** › tên máy để gửi luôn, hoặc "Chọn máy khác…" để mở app với các mục đó trong danh sách gửi. Chi tiết ở mục [Menu chuột phải trên Windows](#menu-chuột-phải-trên-windows).
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

**Khoá ký APK Android**: mọi bản APK phải ký cùng một khoá thì điện thoại mới cài đè được (không phải gỡ app, không mất dữ liệu). Khoá nằm ở `~/.tauri/betterdrop-android.jks` (mật khẩu trong `~/.tauri/betterdrop-android.password`) trên máy chủ dự án, và trong GitHub Secret `ANDROID_KEYSTORE_BASE64` + `ANDROID_KEYSTORE_PASSWORD`. Build trên máy: tạo `src-tauri/gen/android/keystore.properties` (đã nằm trong `.gitignore`) gồm `storeFile=…`, `password=…`, `keyAlias=betterdrop`. Không có file này thì APK được ký bằng khoá debug. **Mất khoá thì người dùng phải gỡ app để cài bản mới**, hãy sao lưu.

## Menu chuột phải trên Windows

Bộ cài thêm mục **Gửi bằng BetterDrop** (*Send with BetterDrop*) vào menu chuột phải của file và thư mục, cho riêng người dùng đang cài (`HKCU\Software\Classes\*\shell\BetterDrop` và `...\Directory\shell\BetterDrop`, không cần quyền admin). Gỡ cài đặt thì xoá các khoá này.

- Lúc mới cài, mục này mở BetterDrop với các mục đã chọn nằm sẵn trong danh sách gửi.
- Khi BetterDrop chạy, app ghi lại mục đó thành menu con: **Gửi bằng BetterDrop › Máy A / Máy B / Chọn máy khác…** (tối đa 8 máy: máy đang thấy, máy đã lưu, máy tin cậy, máy trong nhóm; bỏ máy đã "quên"). Danh sách tự cập nhật khi có máy mới, chữ đổi theo ngôn ngữ trong Cài đặt.
- Chọn tên máy: app chạy `BetterDrop.exe --send-to <id> "<file>"` và gửi luôn (máy nhận vẫn bấm Chấp nhận như thường, trừ máy tin cậy). Máy đó không bật (chờ tối đa 8 giây để tìm thấy khi app vừa mở) thì app mở cửa sổ với các mục nằm sẵn trong danh sách gửi và hiện thông báo.
- Chọn nhiều file: Explorer chạy app một lần cho mỗi file, bản đang chạy gom lại (chờ 0,8 giây không có file mới) rồi gửi một lượt.

Giới hạn:
- **Windows 11**: menu mới chỉ hiện mục của app có gói MSIX, nên mục này nằm trong **Show more options** (hoặc Shift + chuột phải). Windows 10 hiện ngay.
- **Bộ cài MSI** (`src-tauri/windows/context-menu.wxs`): cũng tạo mục này, nhưng chỉ cho người đã chạy bộ cài (MSI cài cho cả máy, còn menu nằm trong HKCU) và chữ ban đầu là tiếng Anh (bảng mã của MSI không có ký tự tiếng Việt); mở app một lần là chữ đổi theo ngôn ngữ. Người dùng khác trên cùng máy thì dùng "Send to".
- App chỉ ghi menu con khi bộ cài đã tạo mục cơ bản, và không ghi khi chạy bản dev (`npm run tauri dev`) hay khi đặt `BETTERDROP_PROFILE`.

**Cần thử trên máy Windows thật** (chưa kiểm chứng được vì không có máy Windows):
1. Cài bằng bộ cài NSIS (`.exe`): chuột phải một file → có "Gửi bằng BetterDrop" (Windows tiếng Việt) hoặc "Send with BetterDrop", có icon BetterDrop; bấm vào → app mở với file trong danh sách gửi.
2. Mở app, đợi thấy máy khác → đóng cửa sổ (app chạy nền) → chuột phải lại: có menu con với tên máy và "Chọn máy khác…" (có kẻ ngang phía trên).
3. Chọn 1 file › tên máy đang bật → gửi luôn, máy kia hiện yêu cầu nhận.
4. Chọn 20 file (và một thư mục) › tên máy → **chỉ một** yêu cầu nhận chứa đủ file; thử cả lúc app đang tắt hẳn (thoát từ khay) lẫn lúc đang chạy.
5. Tắt BetterDrop trên máy nhận › chuột phải › tên máy đó → sau vài giây app mở với file trong danh sách gửi và có thông báo "… đang không bật".
6. Chuột phải một thư mục, và chuột phải trong ổ mạng/USB: menu vẫn hiện, gửi được.
7. Tên máy có dấu `&` hiện đúng; đổi ngôn ngữ trong Cài đặt → chữ trên menu đổi theo.
8. Windows 11: mục nằm trong "Show more options".
9. Gỡ cài đặt → mục biến mất hẳn (kiểm tra `regedit` không còn `HKCU\Software\Classes\*\shell\BetterDrop`). Cập nhật lên bản mới → menu vẫn còn.
10. Cài bằng `.msi`: có "Send with BetterDrop"; mở app → chữ đổi theo ngôn ngữ và có menu con; gỡ bằng MSI → mục biến mất.

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
- Đồng bộ clipboard thêm trường `clipboard` (mặc định `false`, chỉ gửi khi `true`) vào `prepare`. Bản cũ bỏ qua trường này và hiện như văn bản thường.
- Tạm dừng (từ 0.3.7) dùng đường dẫn mới `POST /api/v1/pause?session=…&paused=true|false`. Bản cũ trả 404: máy gửi biết máy nhận chỉ chờ nối lại 2 phút, nên khi phiên cũ đã mất thì `prepare` lại và gửi lại file chưa xong.
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
| `src-tauri/src/context_menu.rs`, `src-tauri/windows/` | Windows: menu chuột phải "Gửi bằng BetterDrop" (registry, `--send-to`), hook bộ cài NSIS, fragment WiX cho MSI |
| `src-tauri/src/devices.rs` | Sổ thiết bị (`devices.json`): máy đã lưu/ẩn, nhóm, máy tin cậy, quy tắc nhận, chọn máy đồng bộ clipboard |
| `src-tauri/src/clipboard_sync.rs` | Đồng bộ clipboard: đọc định kỳ, gửi tới máy tin cậy, ghi vào clipboard khi nhận, chống vòng lặp |
| `src-tauri/src/tray.rs` | Thanh menu/khay hệ thống: chạy nền, thông báo, gửi nhanh clipboard/file |
| `src-tauri/src/online/` | Gửi qua Internet: mã hoá theo khối (`crypto.rs`), đóng gói zip (`package.rs`, `zip.rs`), dịch vụ lưu file (`providers.rs`) |
| `website/receive/` | Trang nhận link "Gửi qua Internet" (giải mã bằng WebCrypto, lưu thẳng xuống đĩa) |
| `src/main.ts`, `index.html`, `src/styles.css` | Giao diện |
