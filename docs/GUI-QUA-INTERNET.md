# Gửi qua Internet

Gửi file cho người ở xa, kiểu WeTransfer: app mã hoá file ngay trên máy, tải bản mã hoá lên một dịch vụ lưu file tạm (không cần tài khoản), rồi tạo link để gửi qua Zalo, email… Người nhận mở link bằng trình duyệt, hoặc dán vào app BetterDrop.

## Kiểm chứng dịch vụ (Bước 0)

Ngày kiểm tra: 24/10/2026. Nguồn: tài liệu và điều khoản trên trang của từng dịch vụ, cộng các yêu cầu HTTP chỉ đọc (OPTIONS/GET tới link giả) để xem CORS.

Người dùng đồng ý cho upload dữ liệu ngẫu nhiên tự tạo để thử. Phép thử dùng chính code của app (test `live_services` trong `src-tauri/src/online/e2e_tests.rs`, chỉ chạy tay): tạo file ngẫu nhiên, mã hoá + tải lên, rồi (nếu dịch vụ cho) tải về bằng app, giải mã, so SHA-256 với file gốc. Mạng: Internet gia đình ở Việt Nam, ngày 24/10/2026.

### Kết quả upload thật

| Dịch vụ | Cỡ | Tải lên | Tốc độ lên | Tải về bằng app | Checksum |
|---|---|---|---|---|---|
| storage.to | 1 MB | 4,3 s | 0,2 MB/s (chủ yếu là 3 lần gọi API) | (không cho app tải) | khớp qua trang nhận, xem dưới |
| storage.to | 50 MB | 3,9 s | 12,9 MB/s | — | **khớp** (tải tay ở trang storage.to rồi giải mã trên trang nhận) |
| storage.to | 1 GB | 81,0 s | 12,4 MB/s | — | chưa tải về (giống 50 MB) |
| temp.sh | 1 MB | 3,1 s | 0,3 MB/s | 2,8 s | **khớp** |
| temp.sh | 50 MB | 8,2 s | 6,1 MB/s | 6,7 s | **khớp** |
| temp.sh | 1 GB | 77,6 s | 12,9 MB/s | 79,1 s | **khớp** |
| Litterbox | 1 MB | **bị từ chối** | — | — | — |

Xác nhận thêm:

- **Litterbox chặn mạng thử nghiệm.** Ngay lần gửi đầu tiên (1 MB, trước khi có lần thử lại nào), API trả HTTP 500 qua tường lửa BunkerWeb của họ; các lần sau trả 403, rồi cả trang chủ `litterbox.catbox.moe` cũng 403 với IP này suốt hơn 20 phút ("Your Client Forbidden… try again in a few minutes"). Trang tải `litter.catbox.moe` vẫn trả lời bình thường (404 cho mã không có, kèm CORS `*`). Vì vậy chưa đo được tốc độ, giới hạn 1 GB và 72 giờ của Litterbox. Người dùng khác ở cùng nhà mạng có thể gặp đúng lỗi này, nên app **tự chuyển sang storage.to** khi Litterbox từ chối (hiện ghi chú "Litterbox từ chối tải lên (HTTP 403). Đang chuyển sang storage.to…", tạo khoá mới, đọc lại file từ đầu). Đã thử thật: gửi 1 MB qua Litterbox, nhận về link storage.to.
- **storage.to**: trang file hiện "xoá sau 6 ngày 23 giờ" ngay sau khi tải lên, `expires_at` đúng 7 ngày (app đặt `expiry_days: 7`). Giới hạn thật là **25 GiB** (26 843 545 600 byte): khai báo hơn 1 byte bị từ chối "File too large. Maximum size is 25 GB" ngay ở bước khởi tạo (đã huỷ, không tải dữ liệu). Phần 32 MiB (800 phần cho 25 GiB). App giữ mức 25 GB (thập phân) cho chắc. Nút Download trên trang tải từ `storage.to/<mã>/download` (cần qua trang, có kiểm tra Cloudflare).
- **temp.sh**: trang file ghi "Expire Time" đúng 3 ngày sau khi tải lên. Tải bằng `POST` tới link file trả thẳng file (app dùng cách này). Giới hạn 4 GB chưa thử (cần tải lên hơn 4 GB).
- **CORS**, thử bằng `fetch` thật từ trang nhận: `POST https://temp.sh/<mã>/betterdrop.bdrop` và `GET https://storage.to/<mã>/download` đều bị trình duyệt chặn ("Failed to fetch"). Đúng như bảng dưới: người nhận dùng trình duyệt phải tải tay.

Các cột "theo tài liệu" bên dưới lấy từ trang của dịch vụ; phần đã kiểm chứng ghi ở trên.

| | Litterbox | storage.to | Gofile | temp.sh |
|---|---|---|---|---|
| Điều khoản cho app bên ngoài upload tự động | **Được**: API công khai, không cần tài khoản. Cấm lách giới hạn, cấm dùng làm CDN/hotlink cho dịch vụ thương mại, không nhận `.exe .scr .cpl .doc* .jar` (file `.bdrop` không bị ảnh hưởng) | **Được**: "our official API, CLI and desktop apps are permitted". Cấm công cụ tự động trái phép để upload/tải hàng loạt, cấm lách giới hạn, cấm dùng làm CDN/hotlink | Upload được (tự tạo tài khoản khách). Điều khoản: "Most API endpoints require a Premium account" | Không có trang điều khoản. Trang chủ hướng dẫn dùng `curl` và ShareX |
| Cần tài khoản? | Không | Không (không cần API key) | Upload: không. **Tải bằng API: cần Premium** (`GET /contents/{id}` và link tải trực tiếp đều Premium) | Không |
| Dung lượng tối đa (theo tài liệu) | 1 GB | 25 GB mỗi file. Ẩn danh: 50 file và 100 GB upload mỗi 24 giờ cho mỗi mã khách, 500 GB/24 giờ mỗi IP | Không công bố mỗi file. Tài khoản free tối đa 10.000 mục | 4 GB |
| Giữ file (theo tài liệu) | 1, 12, 24 hoặc 72 giờ. App đặt **72 giờ** | 3 ngày mặc định, đặt 1–7 ngày bằng `expiry_days`. App đặt **7 ngày** | **Không cam kết**: "does not guarantee any specific retention duration… may delete inactive content at any time" | 3 ngày |
| Link tải trực tiếp, CORS cho `https://gmttuts.github.io` | **Có**: link `https://litter.catbox.moe/<mã>` trả thẳng file, CORS `*`. Đã thử: trang nhận gọi thật tới một mã không tồn tại và đọc được 404 (báo "link đã hết hạn") | **Không.** Tài liệu cho AI (`/llms.txt`) ghi rõ không có link tải trực tiếp, người nhận phải bấm tải ở trang `storage.to/<mã>`. Trang file nằm sau Cloudflare: `GET /r/<mã>` và `/c/<mã>.json` từ curl trả 403 | Chỉ Premium | **Không** có header `Access-Control-Allow-Origin` (OPTIONS trả 200 nhưng không có CORS) |
| Upload chia phần / tiếp tục khi rớt mạng | Không: một yêu cầu `multipart/form-data` | **Có**: file > 50 MB tự chia phần (32 MiB/phần, URL ký sẵn lên Cloudflare R2), xin lại URL từng phần qua `/upload/parts`. App gửi lại đúng phần bị lỗi, tối đa 4 lần | Không có trong tài liệu | Không: một yêu cầu `multipart/form-data`, rớt mạng thì gửi lại từ đầu |
| Thử upload 1 MB / 50 MB / 1 GB | **Bị tường lửa chặn** từ mạng thử nghiệm (500/403); app tự chuyển sang storage.to | Đạt, ~12,5 MB/s | Không làm (đã loại) | Đạt, ~12,9 MB/s (1 GB) |
| Kết luận | **Dùng, mặc định khi ≤ 1 GB**, tự chuyển sang storage.to nếu bị từ chối. Người nhận mở link là tải và giải mã ngay trong trình duyệt; app cũng tự tải | **Dùng**, mặc định khi > 1 GB hoặc gửi thư mục. Người nhận tải file `.bdrop` từ trang storage.to rồi mở trên trang nhận/app | **Dừng**, xem dưới | **Dùng**. App tự tải về được; trình duyệt phải tải tay rồi mở trên trang nhận |

### Vì sao dừng Gofile

- Không tải được bằng chương trình nếu không có tài khoản Premium: đọc nội dung (`GET /contents/{id}`) và link tải trực tiếp đều là tính năng Premium. Cách duy nhất còn lại là giả lập trang web của Gofile (lấy token nội bộ của trang), tức là lách giới hạn, trái điều khoản ("Platform Abuse").
- Không cam kết thời gian giữ file: file free có thể bị xoá bất cứ lúc nào nếu ít được tải. "Khoảng 10 ngày" trên các trang tổng hợp không có trong tài liệu chính thức.

### Thay thế Gofile

Người dùng chọn thêm **Litterbox** (đã làm, xem cột Litterbox ở bảng trên). Các phương án đã cân nhắc:

| Dịch vụ | Ưu | Nhược | Đã kiểm tra |
|---|---|---|---|
| **Litterbox** (`litter.catbox.moe`), đã thêm | Link tải trực tiếp, **CORS `*`**: trang nhận tự tải và giải mã, người nhận chỉ cần bấm một nút (giống WeTransfer nhất) | 1 GB, tối đa 72 giờ | Điều khoản: đã đọc. CORS: có. Upload thật: chưa |
| **filebin.net** | 6 ngày, CORS `*` trên `filebin.net` | Tải file đi qua chuyển hướng sang kho S3: cần kiểm tra CORS ở bước đó; giới hạn dung lượng chưa rõ | CORS trang chính: có. Còn lại: chưa |
| Pixeldrain | 10 GB | Cần API key (cần tài khoản) | Loại |

Thêm một dịch vụ khác chỉ cần một biến thể mới trong `src-tauri/src/online/providers.rs` và một dòng trong `PROVIDERS` của `website/nhan/nhan.js` (đặt `direct` để trang tự tải).

## Người nhận làm gì

| Dịch vụ | Mở link bằng trình duyệt | Dán link vào app BetterDrop |
|---|---|---|
| Litterbox | Trang tự tải và kiểm tra khoá, người nhận chỉ bấm "Lưu file" | App tự tải về, giải mã, lưu vào thư mục nhận |
| storage.to | Bước 1: bấm "Mở trang tải storage.to", bấm Download, được file `betterdrop.bdrop`. Bước 2: kéo file đó vào trang, bấm "Lưu file" | App mở trang tải; tải xong bấm "Chọn file đã tải…" |
| temp.sh | Như storage.to | App tự tải về, giải mã, lưu vào thư mục nhận |

Gửi nhiều file hoặc thư mục: trình duyệt lưu một file `.zip` (không nén), app tự giải nén và giữ nguyên cấu trúc thư mục.

## Thiết kế

### Link

```
https://gmttuts.github.io/better-drop/nhan/#1.<dịch vụ>.<mã file>.<khoá>
```

- `1`: phiên bản định dạng link và file. Đổi cách mã hoá/đóng gói thì tăng số này; bản cũ báo "Link được tạo bằng BetterDrop mới hơn".
- `<dịch vụ>`: `lb` (Litterbox), `st` (storage.to), `ts` (temp.sh).
- `<mã file>`: mã của dịch vụ (`x7k2p9.bdrop`, `FQxyz1234`, `AbCdE/betterdrop.bdrop`). Được kiểm tra chặt để link giả không chèn được đường dẫn lạ.
- `<khoá>`: 32 byte ngẫu nhiên, base64url (43 ký tự). Mỗi lượt gửi một khoá mới.

Phần sau `#` không bao giờ được trình duyệt gửi lên máy chủ (GitHub Pages hay dịch vụ lưu file). Trang nhận đặt `referrer` là `no-referrer`.

### File `.bdrop` (phiên bản 1)

```
"BDRP" | 0x01 (phiên bản) | 0x01 (AES-256-GCM) | cỡ khối (u32 big-endian, mặc định 1 MiB)
khối 0 | khối 1 | … | khối cuối
```

- Mỗi khối: tối đa `cỡ khối` byte dữ liệu gốc đã mã hoá AES-256-GCM, cộng 16 byte tag.
- Nonce 12 byte: `00 00 00 | số thứ tự khối (u64 big-endian) | 01 nếu là khối cuối, 00 nếu không`. Khoá mới cho mỗi lượt gửi nên nonce không bao giờ lặp.
- AAD: 10 byte đầu file (sửa cỡ khối hay phiên bản là hỏng).
- Cờ "khối cuối" nằm trong nonce, nên cắt bớt file ở bất kỳ đâu, kể cả đúng ranh giới khối, đều bị phát hiện ("File bị hỏng hoặc chưa tải hết"). Thêm dữ liệu vào cuối cũng vậy.
- Tương thích WebCrypto: `crypto.subtle.decrypt({ name: "AES-GCM", iv: nonce, additionalData: header, tagLength: 128 }, key, khối)`.

Dữ liệu gốc (trước khi mã hoá):

```
độ dài manifest (u32 big-endian) | manifest (JSON) | nội dung
```

```json
{ "v": 1, "kind": "file" | "zip", "name": "báo cáo.pdf", "size": 12345,
  "files": [{ "path": "Ảnh/a.jpg", "size": 100 }] }
```

- Một file đơn lẻ: nội dung là chính file đó (`kind: "file"`).
- Nhiều file hoặc thư mục: nội dung là một gói **zip Stored** (không nén, `kind: "zip"`, tên `<thư mục>.zip`). App tính CRC32 trước (đọc file một lượt) để biết chính xác kích thước gói, không dùng "data descriptor", tự dùng Zip64 khi > 4 GB. Đã thử giải nén bằng `unzip` và `ditto` (Archive Utility) trên macOS.
- Tên file và danh sách file nằm trong manifest đã mã hoá. Dịch vụ lưu file chỉ thấy file `betterdrop.bdrop` kiểu `application/octet-stream` và kích thước.

### App (`src-tauri/src/online/`)

| File | Vai trò |
|---|---|
| `crypto.rs` | `Encryptor`/`Decryptor` bọc `Read`: mã hoá/giải mã từng khối, không đọc cả file vào RAM |
| `package.rs` | Manifest, gói file/zip thành một luồng đọc; giải gói vào thư mục tạm rồi mới chuyển vào thư mục nhận khi đã xác thực hết (file dở không bao giờ xuất hiện). Đường dẫn được làm sạch như khi nhận qua LAN |
| `zip.rs` | Viết zip Stored tối giản, tính trước độ dài |
| `providers.rs` | Từng dịch vụ: thông số (dung lượng, số ngày, tải thẳng được không), upload, download |
| `mod.rs` | Link, lệnh `online_send`, `online_receive_link`, `online_receive_file`, lịch sử |

- Dịch vụ mặc định trong ô chọn: Litterbox khi chắc chắn ≤ 1 GB (chỉ chọn file), còn lại (lớn hơn, hoặc có thư mục chưa biết dung lượng) là storage.to. Người dùng đổi được; dịch vụ không đủ dung lượng bị làm mờ.
- Upload dạng stream: storage.to đọc và mã hoá từng phần 32 MiB (giữ trong RAM để gửi lại khi lỗi), Litterbox và temp.sh đẩy từng 1 MiB.
- Tiến trình và nút Huỷ dùng chung với lượt chuyển trong mạng LAN (mục Hoạt động).
- Kiểm tra dung lượng trước khi đọc file, theo giới hạn của dịch vụ đã chọn.
- storage.to: mỗi máy dùng một mã khách cố định (`online.json` trong thư mục dữ liệu), vì hạn mức tính theo mã này (đổi mã liên tục là lách giới hạn).
- Lịch sử lưu link (trường `online` trong `history.jsonl`) để "Sao chép link" và "Hiện mã QR" lại.
- File bị sửa trong lúc gửi (kích thước hoặc CRC đổi): báo lỗi, không tạo link hỏng.

### Trang nhận (`website/nhan/`)

- JS thuần, dùng chung `website/styles.css`, tiếng Việt/Anh (nút EN/VI, nhớ lựa chọn như trang chủ).
- Đọc phần `#`, kiểm tra khoá ngay khi thả file (giải mã khối đầu, đọc manifest) để báo sai khoá trước khi lưu.
- Lưu thẳng xuống đĩa, từng khối:
  1. Chrome/Edge trên máy tính: File System Access API (`showSaveFilePicker`). Lỗi giữa chừng thì huỷ file đang ghi.
  2. Firefox và trình duyệt khác có Service Worker: `sw.js` biến dữ liệu giải mã thành một lượt tải xuống bình thường (có báo ngược để không đẩy quá nhanh).
  3. Safari/iOS: gom vào Blob rồi tải (cảnh báo khi file > 300 MB).
- Dịch vụ cho phép CORS (đặt `direct` trong `PROVIDERS`, hiện là Litterbox) thì trang tự tải theo dạng stream, không cần bước 1. Tải thẳng lỗi thì vẫn còn cách tải tay ở bước 1.
- Kiểm thử: `?sink=sw` hoặc `?sink=blob` ép dùng cách lưu dự phòng.

## Kiểm tra đã làm

- `cargo test` (68 test, thêm kiểm tra mã file Litterbox): mã hoá/giải mã vòng tròn nhiều kích thước, cắt bớt (giữa khối, đúng ranh giới khối, bỏ khối cuối) phải báo lỗi, thêm rác vào cuối, sai khoá, sửa phần đầu file, link hỏng, đường dẫn độc hại trong manifest, file bị sửa trong lúc gửi, nhận trùng tên thư mục.
- Test đầu-cuối với **máy chủ giả lập** storage.to, temp.sh và Litterbox làm đúng theo tài liệu API (`online/e2e_tests.rs`): gửi file 300 KB qua storage.to (chia 5 phần, URL phần 3–5 phải xin thêm, phần 2 lỗi 500 một lần rồi gửi lại), kiểm tra dịch vụ không thấy tên file/nội dung, nhận bằng file `.bdrop`, sai khoá báo lỗi ngay; gửi thư mục qua temp.sh, app tự tải về và giải nén đúng; gửi file qua Litterbox (kiểm tra `reqtype`, `time=72h`), app tự tải bằng link trực tiếp; file quá giới hạn bị chặn trước khi tải lên.
- `cargo check` cho iOS (`aarch64-apple-ios`) và Android (`aarch64-linux-android`): đạt.
- `npx tsc --noEmit`: đạt.
- Trang nhận (Chrome trong app Claude, `npx vite website`): file 300 MB tạo bằng code Rust giải mã ra đúng SHA-256 của file gốc, qua cả đường `showSaveFilePicker`; thư mục 21 file lưu qua Service Worker, giải nén bằng `unzip` và `ditto` khớp checksum từng file; sai khoá, file bị cắt, file không phải `.bdrop`, link hỏng đều báo đúng lỗi. Đường tải thẳng (fetch dạng stream rồi giải mã) với file 300 MB khớp SHA-256.

## Thử thật

Đã làm (xem [Kết quả upload thật](#kết-quả-upload-thật)). Chạy lại:

```bash
cd src-tauri
BDROP_LIVE=lb,st,ts BDROP_SIZES=1000000,50000000,1000000000 cargo test live_services -- --ignored --nocapture
```

Mỗi lượt in một dòng `RESULT dịch-vụ cỡ giây-tải-lên MB/s giây-tải-về checksum … link`. Chỉ tải lên dữ liệu ngẫu nhiên tự tạo. Với storage.to (app không tự tải được), mở link in ra, bấm tải ở trang storage.to rồi kéo file vào trang nhận.

Còn chưa làm:

- Đo Litterbox từ một mạng khác (mạng thử nghiệm bị tường lửa của Litterbox chặn).
- Thử giới hạn 4 GB của temp.sh (phải tải lên hơn 4 GB).
- Rút mạng giữa chừng khi gửi 1 GB qua storage.to để xem app gửi lại phần lỗi trên mạng thật (đã thử bằng máy chủ giả lập).
- Bấm thử trong cửa sổ app thật (`npm run tauri dev`): công cụ hiện có không điều khiển được cửa sổ app, nên luồng giao diện đã thử trên bản demo website (cùng giao diện) còn phần gửi/nhận đã thử bằng chính code Rust của app.

Tạo file mẫu không cần mạng: `BDROP_SRC=<file hoặc thư mục> BDROP_OUT=<thư mục> cargo test make_sample -- --ignored --nocapture` in ra link (mã file giả) và file `betterdrop.bdrop` để thử trang nhận.
