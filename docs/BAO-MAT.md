# Rà soát bảo mật BetterDrop (v0.3.6)

Phạm vi: máy chủ nhận file `/api/v1` (`server.rs`), chia sẻ qua trình duyệt (`web.rs`,
`web/index.html`), làm sạch đường dẫn (`paths.rs`), sổ máy tin cậy (`devices.rs`).
`src/main.ts` chỉ được đọc để kiểm tra XSS. Không đụng tới `transfer.rs`, `client.rs`,
`discovery.rs`.

Mọi lỗ hổng dưới đây đều có test tái hiện trong `src-tauri/src/tests.rs` (phần
"Bảo mật") hoặc test đơn vị trong file tương ứng. Các test đó **trượt** khi gỡ bản sửa
và **qua** khi có bản sửa. Giao thức không đổi: không thêm/bớt trường, không đổi cổng
hay định dạng API. Bản cũ vẫn gửi/nhận được như trước. Mã lỗi mới (429, 413, 421) chỉ
xuất hiện khi bị tấn công hoặc dùng quá giới hạn.

## Đã sửa (xếp theo mức độ nghiêm trọng)

### 1. Cao: máy lạ chiếm danh tính của một máy đã lưu

Khi có máy gửi tới, máy nhận tự "ghi nhớ" máy gửi (id, tên, địa chỉ, **dấu vân tay
chứng chỉ**) để gửi ngược lại được. Nếu máy thật không hiện qua mDNS (đang tắt, mạng
chặn mDNS, điện thoại thêm bằng QR…), bất kỳ máy nào trong LAN cũng gửi được một **đoạn
văn bản** (không cần ai bấm Chấp nhận) khai `id` của máy đó kèm chứng chỉ của chính nó.
Máy nhận ghi đè địa chỉ và dấu vân tay đã lưu. Lần sau người dùng bấm gửi tới "Máy A",
file đi thẳng tới kẻ giả mạo: client ghim theo dấu vân tay đã lưu, mà dấu vân tay đó đã
bị thay.

Sửa (`server.rs::remember_sender`, `devices.rs::fp_of`): nếu id đã biết (máy tin cậy,
máy đã lưu, hoặc máy thấy qua mDNS) với dấu vân tay **khác**, thì không ghi đè gì.
Máy cài lại app (có chứng chỉ mới) thì người dùng bấm "Quên" rồi thêm lại.
Test: `impostor_cannot_hijack_a_saved_device`.

### 2. Trung bình: spam `/prepare` (hộp thoại, văn bản, bộ nhớ)

- Không có giới hạn số yêu cầu đang chờ người dùng trả lời. Một máy trong LAN mở được
  hàng nghìn hộp thoại Chấp nhận, mỗi cái giữ một kết nối 60 giây.
- Văn bản hiện ngay (không hỏi), không giới hạn số lần, và dài tới 32 MB mỗi lần.

Sửa (`server.rs`): mỗi IP có tối đa `MAX_PENDING_PER_IP = 2` yêu cầu đang chờ, tổng
cộng tối đa `MAX_PENDING = 6`. Quá giới hạn thì trả **429** ngay, không hiện hộp thoại.
Văn bản dài tối đa 1 MB (**413**, giống trang web), mỗi IP gửi tối đa 20 đoạn/phút
(**429**). Trang web (`/api/prepare`, `/api/text`) dùng chung các giới hạn này.
Test: `prepare_flood_is_limited`, `text_flood_and_huge_text_are_limited`.

### 3. Trung bình: dò mã PIN bằng cách đổi IP

PIN 6 số chỉ bị khoá **theo từng IP** (5 lần sai/60 giây). Trong LAN rất dễ đổi IP (nhất
là IPv6), nên kẻ dò mã chỉ cần đổi IP sau mỗi 4 lần thử.

Sửa (`web.rs::check_pin`): thêm khoá **toàn trang**. Tổng cộng 20 lần sai (mọi IP) trong
60 giây thì khoá đăng nhập 60 giây với tất cả mọi người. So sánh PIN theo thời gian
hằng. Dọn bảng đếm lỗi cũ để không phình bộ nhớ.
Test: `pin_guessing_from_many_ips_is_locked_out`.

### 4. Trung bình: tổng kích thước bị tràn số, hộp thoại hiện sai

Tổng kích thước trong hộp thoại cộng các `size` do máy gửi khai mà không kiểm tra
tràn. Ví dụ `u64::MAX + 1025` cho ra "1 KB" trong bản release. Người dùng bấm Chấp nhận
một "file nhỏ", rồi máy gửi được phép ghi tới khi đầy ổ đĩa (bản debug thì panic).

Sửa (`server.rs::accept_files`): cộng có kiểm tra. Tràn số thì trả **400** trước khi hỏi.
Test: `overflowing_total_size_is_rejected_before_asking`.

### 5. Trung bình–thấp: DNS rebinding và thiếu header bảo mật ở trang web

- Máy chủ HTTP (cổng 53319) không kiểm tra `Host`. Một trang trên Internet có thể trỏ tên
  miền của nó về IP máy này (DNS rebinding), rồi nhờ trình duyệt của người trong LAN gọi
  API, ví dụ để dò PIN từ xa.
- Trang không có CSP, `X-Frame-Options`, `nosniff`.

Sửa (`web.rs::guard`): chỉ phục vụ khi `Host` là địa chỉ IP, `localhost`, tên một nhãn
(vd `mac-cua-lan`) hoặc `*.local`. Tên miền khác nhận **421**. Mọi phản hồi có thêm:
`Content-Security-Policy` (chỉ chạy đúng đoạn script của trang theo băm SHA-256,
`frame-ancestors 'none'`, `default-src 'none'`), `X-Frame-Options: DENY`,
`X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`,
`Cache-Control: no-store`. Đã thử trang trong trình duyệt thật: đăng nhập bằng PIN trong
mã QR, xem danh sách và tải file đều chạy, không có lỗi CSP.
Test: `web_page_has_security_headers_and_checks_host`, `only_local_hosts_are_served`.

### 6. Thấp: giả đuôi file và giả tên máy bằng ký tự đảo chiều chữ

- `anh\u{202E}gpj.exe` hiện ra là `anhexe.jpg`, trong hộp thoại lẫn trong Finder/Explorer.
  `is_control()` không bắt các ký tự định dạng (Cf) này.
- Tên máy gửi không giới hạn độ dài và giữ nguyên ký tự điều khiển.
- Hộp thoại hiện `rel_path` thô do máy gửi khai, không phải tên thực sự sẽ lưu.

Sửa: `paths.rs` thay các ký tự bidi (U+061C, U+200E/F, U+202A–E, U+2066–9) bằng `_`.
`server.rs::clean_name` bỏ ký tự điều khiển/bidi khỏi tên máy, hệ điều hành, tên nhóm
và tên thành viên nhóm, rồi cắt còn 64 ký tự. Hộp thoại hiện tên đã làm sạch.
Test: `strips_bidi_overrides_that_fake_extensions`, phần cuối `text_flood_and_huge_text_are_limited`.

### 7. Thấp: thiếu một số tên thiết bị Windows

`CONIN$`, `CONOUT$`, `COM0`, `LPT0`, `COM¹²³`, `LPT¹²³` và dạng có khoảng trắng trước
đuôi (`NUL .txt`) chưa bị chặn. Ghi file tên đó trên Windows sẽ ghi vào thiết bị hoặc
báo lỗi. Sửa trong `paths.rs::sanitize_component`.
Test: `blocks_more_windows_device_names`.

### 8. Thấp: symlink "treo" trong thư mục nhận bị đè

`unique_path` dùng `Path::exists`, hàm này coi symlink trỏ tới chỗ không tồn tại là
"chưa có". File nhận được đổi tên đè lên symlink đó (chỉ mất symlink, không ghi xuyên ra
ngoài). Sửa: dùng `symlink_metadata`.
Test: `dangling_symlink_counts_as_taken`, `malicious_names_stay_inside_download_dir`.

### 9. Thấp: trình duyệt huỷ được lượt gửi đi của máy tính

`/api/cancel` của trang web gọi `transfer::cancel`, hàm này huỷ cả lượt **gửi đi**. Giờ
trang web chỉ huỷ được lượt nhận (thứ trình duyệt gửi lên). Cần biết id phiên (UUID ngẫu
nhiên) mới lợi dụng được, nên mức thấp.

### 10. Rất thấp: so sánh token upload không theo thời gian hằng

Token 128 bit, nên dò qua mạng không khả thi. Vẫn đổi sang `server::ct_eq` cho chắc.

## Đã kiểm tra, không có lỗi

- **Path traversal**: `..`, `/etc/passwd`, `..\..\win.ini`, `C:\…`, `x/../../../y` đều
  nằm trong thư mục nhận (test đầu-cuối `malicious_names_stay_inside_download_dir`).
  Unicode được chuẩn hoá NFC.
- **Symlink có sẵn trong thư mục nhận** trỏ ra ngoài (vd `Photos -> /tmp/x`): thư mục
  gốc của lượt nhận luôn được tạo mới với tên khác (`Photos (1)`), không ghi xuyên qua.
  File `.part` có 8 ký tự token trong tên, nên máy lạ không đặt sẵn symlink được.
- **Upload**: ghi quá kích thước đã khai thì lỗi và xoá file tạm. `offset` là `u64`, nên
  số âm bị từ chối khi đọc query. `offset` lớn hơn phần đã nhận hoặc lớn hơn kích thước
  file trả 409. Token sai trả 403. Token dùng lại sau khi phiên đã huỷ/lỗi/xong trả
  410/409 (test `upload_abuse_is_refused`).
- **Máy tin cậy**: chỉ tự chấp nhận khi chứng chỉ trình trong TLS hai chiều (đã kiểm chữ
  ký bắt tay) có đúng dấu vân tay đã lưu cho id đó. `AnyClientCert` nhận chứng chỉ tự ký
  bất kỳ là **đúng thiết kế**: rustls vẫn kiểm chữ ký `CertificateVerify`, nên máy gửi
  phải có khoá bí mật. Dấu vân tay chỉ dùng để nhận diện, không thay cho chuỗi CA (test
  có sẵn `impostor_claiming_trusted_identity_is_still_asked`).
- **Cookie phiên**: `HttpOnly; SameSite=Strict`, token ~244 bit ngẫu nhiên (hai UUID v4).
- **CSRF**: API ghi đều cần cookie (SameSite=Strict nên trang khác không gửi kèm được).
  `login`/`prepare`/`text` đọc JSON (cần `content-type: application/json`, nên form của
  trang khác phải qua preflight CORS mà máy chủ không cho).
- **XSS**:
  - `web/index.html` chỉ dùng `textContent`, link tải dùng `encodeURIComponent`.
  - `src/main.ts` hiện tên file, tên máy, văn bản bằng `textContent`/`el(...)`. `innerHTML`
    chỉ gán SVG hằng (icon) hoặc mã QR do chính máy này tạo.
  - `Content-Disposition` mã hoá tên theo RFC 5987, không chèn được header.

  Không cần sửa `main.ts`.

## Rủi ro còn lại / đề xuất

1. **Trang web dùng HTTP thường.** Ai nghe lén được Wi-Fi (mạng mở, Wi-Fi công cộng) sẽ
   thấy PIN, cookie và nội dung file. Đây là đánh đổi có chủ ý (chứng chỉ tự ký làm
   trình duyệt cảnh báo). Nên tắt chia sẻ qua trình duyệt khi không dùng, và không bật
   trên mạng công cộng.
2. **PIN 6 số.** Với khoá toàn trang, kẻ dò mã thử được tối đa khoảng 20 lần/phút
   (~28.800 lần/ngày): mất trung bình khoảng 17 ngày dò liên tục mới có 50% khả năng
   trúng. Đổi PIN (nút "Đổi PIN") sẽ xoá mọi phiên. Có thể cân nhắc tự đổi PIN sau một số
   lần sai, hoặc dùng PIN dài hơn.
3. **Các giới hạn chống spam có thể bị lợi dụng để phá đám (DoS).** Kẻ xấu trong LAN làm
   đầy hàng chờ (6 yêu cầu) hoặc gây khoá đăng nhập trang web, khiến người dùng thật phải
   chờ tối đa 60 giây. Giới hạn theo IP dễ lách bằng cách đổi IP, nhưng tổng toàn máy thì
   không lách được.
4. **mDNS giả mạo** (`discovery.rs`, ngoài phạm vi). Máy lạ quảng bá được id của máy
   khác với chứng chỉ của nó. Nếu máy thật chưa từng được lưu, danh sách có thể hiện bản
   giả. Lần kết nối đầu tiên theo kiểu "tin ngay lần đầu" (TOFU). Đề xuất: hiện "mã" (dấu
   vân tay rút gọn) trong hộp thoại Chấp nhận và khi gửi lần đầu, để người dùng đối chiếu.
   Phần này cần sửa `main.ts`/`discovery.rs`, chưa làm.
5. **Phiên nhận không bao giờ bị xoá khỏi bộ nhớ** (`AppState::incoming`). Chỉ phình
   sau khi người dùng đã chấp nhận, nên ít nguy hiểm. Có thể dọn phiên đã kết thúc quá
   `RESUME_GRACE`. Cần sửa `state.rs`, ngoài phạm vi lần này.
6. **`/prepare` vẫn nhận body JSON tới 32 MB** (thư mục rất nhiều file). Giới hạn số yêu
   cầu đồng thời ở trên chặn được việc nhân bản lên nhiều lần.
7. **Tên file dài hơn 255 byte** làm lượt nhận báo lỗi (không nguy hiểm). Có thể cắt
   ngắn trong `sanitize_component`.
8. **Kiểm tra `Host`** chặn người mở trang bằng tên miền đầy đủ (vd `mac.nha.example`).
   Mở bằng IP hoặc mã QR như hướng dẫn thì không bị ảnh hưởng.
9. **CSP của webview Tauri** đang là `null` (`tauri.conf.json`). Giao diện app hiện không
   có lỗi XSS, nhưng bật CSP sẽ thêm một lớp phòng thủ. Cần thử kỹ với các plugin, nên để
   làm riêng.

## Ảnh thu nhỏ trong `/prepare` (từ 0.3.7)

Máy gửi kèm ảnh thu nhỏ của vài ảnh/video đầu (trường `thumbs`, có `serde(default)` nên
bản cũ vẫn gửi/nhận như trước). Ảnh hiện trong hộp thoại Chấp nhận **trước khi** người
dùng đồng ý, nên máy nhận coi đây là dữ liệu không tin cậy (`thumbs.rs::accept`):

- Chỉ giữ ảnh của một trong 6 file ảnh/video đầu của chính yêu cầu đó, mỗi file một ảnh.
- Chuỗi base64 quá dài thì bỏ trước khi giải mã; mỗi ảnh tối đa 20 KB, tổng tối đa
  150 KB. Không làm tăng giới hạn body 32 MB của `/prepare`.
- Chỉ nhận JPEG trọn vẹn (bắt đầu `FF D8 FF`, kết thúc `FF D9`); giao diện hiện bằng
  `data:image/jpeg;base64,…` trong thẻ `<img>`, không bao giờ là HTML/SVG.
- Lịch sử lưu ảnh ở `thumbs/<id lượt>/<n>.jpg` trong thư mục dữ liệu; id chỉ gồm chữ,
  số và `-` (id lạ thì không lưu ảnh). Đọc lại cũng kiểm tra JPEG và kích thước.

Test: `bad_thumbnails_are_dropped`, `thumbnail_total_is_capped`,
`unsafe_id_gets_no_thumb_dir`.
