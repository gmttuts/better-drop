# Tốc độ nhận file (phía máy nhận)

Đo ngày 24/10/2026 trên Apple M4 Max (16 nhân), ổ APFS còn trống ~9 %, máy đang rất tải
(load average 15–95 do emulator Android, Chrome và các phiên build khác). Số liệu vì vậy
rất nhiễu: cùng một bản có lượt nhanh gấp 3 lượt chậm. Mỗi ô là **trung vị** của nhiều lượt
chạy **xen kẽ** bản trước/sau (cùng lúc, cùng tải máy).

## Cách đo

```bash
cd src-tauri
# Đầu-cuối: bên gửi + HTTPS thật + bên nhận (bài đo của phiên tối ưu bên gửi)
cargo test --release --lib -- --ignored bench_ --nocapture --test-threads=1
# Chỉ phía nhận: gọi thẳng receive_upload, không TLS, không bên gửi
cargo test --release --lib -- --ignored bench_receive_ --nocapture --test-threads=1
```

Profile bằng `sample` (có sẵn trên macOS) với bản build giữ symbol:
`CARGO_TARGET_DIR=target-prof CARGO_PROFILE_RELEASE_STRIP=false CARGO_PROFILE_RELEASE_DEBUG=line-tables-only cargo test --release --lib --no-run`.

## Chỗ nghẽn tìm được (trước khi sửa)

1. **Luồng worker tokio đứng chờ `rename_lock`.** Đổi tên `.part` → tên thật (khoá + `lstat` +
   `rename`) chạy ngay trên luồng async. Với 16 file nhỏ tới cùng lúc, đây là chỗ tốn nhiều
   thời gian nhất ngoài lúc rảnh: các worker bị chặn nên không xử lý được kết nối khác.
2. **Syscall thừa mỗi file:** `dest_path` bị gọi 2 lần (mỗi lần `create_dir_all` = `mkdir` +
   `stat`), thêm một `stat` file `.part` kể cả khi gửi từ đầu.
3. **Mỗi file một sự kiện `transfer-update`** do `tracker.set_note(None)` chạy cho mọi file:
   5.000 file nhỏ = 5.003 sự kiện lên webview (~11.000 sự kiện/giây trên mạng nhanh).
4. Nhỏ: khoá map `counted` + cấp phát chuỗi mỗi chunk 16 KB; khoá `last_emit` mỗi chunk;
   cấp phát đệm 1 MB cho cả file 10 KB.

Không phải nghẽn: `fsync` (không có), TLS (ring + AES phần cứng), `set_len` (chỉ dùng khi gửi
tiếp). File lớn bị giới hạn bởi `write` xuống đĩa và giải mã TLS.

## Đã sửa (`server.rs`, `state.rs`)

- Chọn tên + `rename` + hook `file_saved` + ghi sổ (lịch sử, `done_files`, trạng thái Xong)
  chạy trong `spawn_blocking`, nên luôn chạy trọn kể cả khi máy gửi ngắt lúc đang chờ.
- `InSession::made_dirs`: nhớ thư mục đã tạo, nên không phải `mkdir`/`stat` lại cho mỗi file. Nếu
  thư mục bị xoá giữa chừng thì `File::create` báo NotFound, lúc đó tạo lại thư mục rồi thử lại
  (giống hành vi cũ).
- `dest_path` gọi một lần, không `stat` file `.part` khi `offset == 0`.
- `set_note(None)` chỉ khi file đó vừa được nối lại sau khi mất kết nối.
- `counted` cộng một lần khi `write_body` kết thúc (lượt gửi tiếp của cùng file bị `active`
  chặn nên không ai đọc giữa chừng). `Tracker::add` dùng atomic thay cho Mutex `last_emit`.
- Đệm ghi = min(kích thước còn lại, 1 MB), tối thiểu 8 KB.

Giữ nguyên: gửi tiếp theo offset, `upload-status`, chờ 120 giây, huỷ thì xoá `.part`, không
ghi quá kích thước đã khai, các giới hạn trong `docs/BAO-MAT.md`, lịch sử, giao thức.

## Đã thử, không giữ

- **Bỏ khoá khi `rename`** (chỉ khoá lúc chọn tên, giữ chỗ tên trong bộ nhớ): **chậm hơn**.
  CPU hệ thống tăng từ 4,1 s lên 13,6 s vì APFS tranh chấp khoá thư mục trong kernel khi nhiều
  `rename` cùng lúc vào một thư mục. Khoá trong tiến trình hoá ra rẻ hơn.
- Đổi tên ngay trên luồng async (giữ các sửa khác): nằm giữa bản gốc và bản cuối, CPU hệ
  thống cao hơn bản cuối (5,65 s so với 4,09 s).

## Kết quả

### Chỉ phía nhận (`bench_receive_*`, 16 file cùng lúc, chunk 16 KB)

| Bài đo | Trước | Sau |
|---|---|---|
| 5.000 file 10 KB (18 lượt) | 8.424 file/s (5.066–9.044) | **10.535 file/s (7.098–12.023), +25 %** |
| 200 file 5 MB, 3 file cùng lúc (20 lượt) | 2.017 MB/s (421–3.880) | 1.673 MB/s (683–4.251): nhiễu đĩa, cùng đường ghi |
| CPU hệ thống, 15.000 file nhỏ (7 lượt) | 5,33 s | **4,09 s (−23 %)** |
| Thời gian chạy, 15.000 file nhỏ | 2,79 s | 2,39 s (−14 %) |

### Đầu-cuối (`bench_*` có sẵn, bên gửi và bên nhận chung một tiến trình)

| Bài đo | Trước | Sau |
|---|---|---|
| 5.000 file 10 KB, localhost (10 lượt) | 6.987 file/s | 6.782 file/s (nhiễu, 3.100–9.900 cả hai bên) |
| 5.000 file 10 KB, RTT 5 ms (10 lượt) | 1.610 file/s | 1.598 file/s |
| 200 file 5 MB, localhost / RTT 5 ms | ~1.500 / ~1.020 MB/s | ~1.350 / ~1.180 MB/s (nhiễu) |
| 1 file 2 GB (5 lượt) | 679 MB/s (310–1.788) | 1.744 MB/s (1.230–2.280): nhiễu đĩa, ít lượt, chưa dám coi là cải thiện |
| **Sự kiện tiến trình bên nhận, 5.000 file** | **5.003** | **6–21** |
| Sự kiện tiến trình bên nhận, 200 file 5 MB | 203 | 7–8 |

**Nói thẳng:** tốc độ đầu-cuối không đổi đo được. Ở bài đo này phía nhận không phải nút cổ
chai: bên gửi (đọc file + TLS, cùng tiến trình), RTT (mỗi file nhỏ ≈ 1 RTT) và đĩa mới là giới
hạn. Lợi ích thật nằm ở:
- máy nhận tốn ít CPU hơn cho mỗi file (−23 % CPU hệ thống), có ích khi máy nhận yếu (điện
  thoại, laptop cũ) hoặc khi bên gửi nhanh hơn;
- giao diện bên nhận không còn bị dội 5.000 sự kiện khi nhận thư mục nhiều file nhỏ.

Muốn file nhỏ nhanh hơn nữa qua Wi‑Fi phải đổi giao thức (gom nhiều file nhỏ vào một request),
cần sửa cả hai phía.
