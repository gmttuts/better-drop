# Kế hoạch BetterDrop v0.2

> **Trạng thái (v0.3.4):** cả 3 tính năng đã xong. 1. Chạy nền: v0.2.0. 2. Máy tin cậy (TLS hai chiều, gắn với dấu vân tay chứng chỉ): v0.3.4. 3. Lịch sử: v0.3.4.

Ba tính năng: **chạy nền trên thanh menu/khay hệ thống**, **tự chấp nhận từ máy tin cậy** và **lịch sử chuyển file**.

Thứ tự làm: 1 → 2 → 3. Chạy nền đi trước vì "tự chấp nhận" chỉ thật sự có ích khi app chạy nền. Lịch sử độc lập nên làm lúc nào cũng được.

---

## 1. Chạy nền trên thanh menu (Mac) / khay hệ thống (Windows)

**Người dùng thấy gì**
- Bấm nút đóng cửa sổ thì app **ẩn đi** thay vì thoát, và vẫn nhận file bình thường.
- Biểu tượng trên thanh menu (Mac) hoặc khay hệ thống cạnh đồng hồ (Windows) có các mục:
  - Mở BetterDrop
  - Mở thư mục nhận
  - ☑ Cho phép nhận file (tắt khi không muốn bị làm phiền)
  - Thoát
- Khi có máy gửi tới mà cửa sổ đang ẩn, hệ điều hành **hiện thông báo**. Bấm vào thông báo thì cửa sổ bật lên kèm hộp thoại Chấp nhận/Từ chối.
- Trong Cài đặt có thêm:
  - ☐ Mở BetterDrop khi khởi động máy
  - ☐ Ẩn biểu tượng trên Dock (chỉ trên Mac)

**Cách làm**
- Bật tính năng `tray-icon` của Tauri và dùng `TrayIconBuilder` cùng menu trong `lib.rs` (đặt ở module mới `tray.rs`).
- Bắt `WindowEvent::CloseRequested`, gọi `api.prevent_close()` rồi `window.hide()`. Chỉ thoát thật khi chọn "Thoát" trong menu.
- Dùng các plugin:
  - `tauri-plugin-notification` để hiện thông báo.
  - `tauri-plugin-autostart` để mở cùng máy tính.
  - `tauri-plugin-single-instance` để khi mở app lần nữa thì hiện cửa sổ cũ, không chạy bản thứ hai. Plugin này cần bỏ qua khi có `BETTERDROP_PROFILE`, để vẫn chạy được 2 bản khi thử nghiệm.
- Ẩn Dock trên Mac: `app.set_activation_policy(ActivationPolicy::Accessory)`.
- "Cho phép nhận file" là một cờ trong `Settings`. Khi tắt, `/prepare` trả `403` ngay và máy gửi thấy "Máy nhận đang tắt chế độ nhận".
- Icon khay hệ thống: dùng icon đơn sắc (template icon) cho Mac để tự đổi màu theo chế độ sáng/tối.

**File chính:** `src-tauri/src/tray.rs` (mới), `lib.rs`, `models.rs` (Settings), `server.rs` (cờ nhận), `capabilities/default.json`, `src/main.ts` (Cài đặt).

**Kiểm thử**
- Đóng cửa sổ, rồi gửi từ máy kia: thông báo hiện ra, bấm vào thì thấy hộp thoại.
- Tắt "Cho phép nhận", gửi từ máy kia: bên gửi thấy thông báo bị từ chối.
- Bật "Mở cùng máy tính", khởi động lại máy, app tự chạy nền.

---

## 2. Tự chấp nhận từ máy tin cậy

**Người dùng thấy gì**
- Hộp thoại nhận có thêm ô ☐ **"Luôn chấp nhận từ máy này"**.
- Từ lần sau, file từ máy đó được nhận ngay. Vẫn có thông báo "Đã nhận 3 file từ …" để bạn biết.
- Trong Cài đặt có danh sách **Máy tin cậy** (tên, mã 4 ký tự, ngày thêm) kèm nút **Bỏ tin cậy**.
- Văn bản từ máy tin cậy: vẫn hiện hộp thoại như cũ, kèm tuỳ chọn "Tự sao chép vào clipboard".

**Vấn đề bảo mật cần giải quyết trước**
Hiện tại máy nhận **không kiểm tra danh tính máy gửi**. Tên và id trong `/prepare` là do máy gửi tự khai, nên ai trong mạng cũng có thể giả làm "máy tin cậy". Vì vậy cần **xác thực hai chiều (mutual TLS)**:
- Máy gửi trình chứng chỉ của chính nó khi kết nối. Chứng chỉ này đã có sẵn từ v0.1, chỉ cần gắn vào client bằng `with_client_auth_cert`.
- Máy chủ chấp nhận mọi chứng chỉ client (vì là tự ký) nhưng **ghi lại fingerprint** của chứng chỉ đó. Cách làm: viết một `Accept` tuỳ biến cho `axum-server`, lấy `peer_certificates()` sau khi bắt tay TLS rồi gắn fingerprint vào request (dạng `Extension`).
- "Tin cậy" = cặp **(id máy, fingerprint chứng chỉ)**. Chỉ tự chấp nhận khi fingerprint thật trong kết nối TLS khớp với fingerprint đã lưu.
- Đây là phần kỹ thuật khó nhất của v0.2. Làm và viết test cho phần này trước, gồm test "máy giả id nhưng sai chứng chỉ thì không được tự chấp nhận".

**Cách làm**
- `Settings.trusted: Vec<TrustedDevice { id, name, fp, added_at }>` được lưu trong `settings.json`.
- `respond_request(request_id, accept, trust: bool)`. Nếu `trust` là true thì thêm máy vào danh sách tin cậy.
- `prepare`: nếu fingerprint client nằm trong danh sách tin cậy thì bỏ qua bước hỏi, tạo phiên luôn và gửi thông báo.
- Lệnh mới: `list_trusted`, `remove_trusted`.

**Tương thích:** máy chạy v0.1 không gửi chứng chỉ client nên luôn bị hỏi như cũ, vẫn chuyển file được. Nên cập nhật cả hai máy lên v0.2.

**File chính:** `client.rs` (gắn chứng chỉ client), `server.rs` (acceptor + kiểm tra), `models.rs`, `lib.rs`, `src/main.ts`.

**Kiểm thử (tự động)**
- Gửi từ máy đã tin cậy: không có `incoming-request`, file được nhận luôn.
- Máy khác khai cùng id nhưng chứng chỉ khác: vẫn phải hỏi.
- Bỏ tin cậy: lần sau phải hỏi lại.

---

## 3. Lịch sử chuyển file

**Người dùng thấy gì**
- Mục "Hoạt động" đổi thành **"Lịch sử"** và vẫn còn sau khi tắt rồi mở lại app.
- Mỗi dòng gồm: chiều gửi/nhận, tên máy, số file và dung lượng, thời gian ("Hôm nay 14:05"), trạng thái.
- Bấm vào một dòng để xem danh sách file. Có các nút:
  - Nhận file: **Hiện trong Finder/Explorer** cho từng file và **Mở thư mục**.
  - Văn bản: **Sao chép lại**.
  - Gửi file: **Gửi lại** (nếu file gốc còn tồn tại).
- Có ô lọc (Tất cả / Đã gửi / Đã nhận) và nút **Xoá lịch sử**. Xoá lịch sử chỉ xoá bản ghi, không xoá file.

**Cách làm**
- Lưu vào `history.jsonl` trong thư mục dữ liệu, mỗi dòng một bản ghi. Ghi thêm vào cuối file khi một lượt chuyển kết thúc (done/rejected/cancelled/error). Giữ tối đa 1.000 bản ghi và tự cắt bớt khi mở app.
- Mỗi bản ghi gồm: `TransferUpdate` + `finished_at` + `files: [{name, path}]`. Với chiều nhận, `path` là **đường dẫn thật sau khi đổi tên** (ví dụ "báo cáo (1).txt"). Muốn vậy, `InSession` cần ghi lại `final_path` của từng file. Với văn bản, lưu tối đa 10.000 ký tự đầu.
- Lệnh mới:
  - `get_history(limit, offset)`
  - `clear_history`, `delete_history_item`
  - `reveal_path(path)` (dùng `opener().reveal_item_in_dir`)
  - `resend(history_id, peer_id)`
- Ghi lịch sử nằm ở một chỗ duy nhất: khi `Tracker::set_status` chuyển sang trạng thái kết thúc. Như vậy không bỏ sót trường hợp nào.

**File chính:** `src-tauri/src/history.rs` (mới), `state.rs` (Tracker, InSession), `server.rs`, `lib.rs`, `src/main.ts`, `src/styles.css`.

**Kiểm thử**
- Test Rust: gửi xong thì `history.jsonl` có bản ghi với đường dẫn file thật. Ghi quá 1.000 bản ghi thì tự cắt bớt.
- Thử tay: tắt rồi mở lại app, lịch sử còn nguyên. Nút "Hiện trong Finder" chỉ đúng file.

---

## Phát hành
Mỗi tính năng là một commit hoặc PR riêng. GitHub Actions build bản Windows và macOS cho mỗi lần đẩy code. Xong cả 3 thì gắn tag `v0.2.0` để tạo bản phát hành.
