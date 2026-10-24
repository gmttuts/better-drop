# Website BetterDrop

Trang giới thiệu và tải về, HTML/CSS/JS thuần, không cần build. Được đăng lên GitHub Pages bởi `.github/workflows/website.yml` mỗi khi `website/**` thay đổi trên `main`.

- `index.html`: khung trang. Chữ trên trang nằm trong `content.js` (tiếng Việt và tiếng Anh).
- `main.js`: đổi ngôn ngữ, nhận biết hệ điều hành, lấy file cài từ bản phát hành mới nhất trên GitHub (`/releases/latest`). Bản nháp (draft) không hiện ra, nên khi chưa có bản công khai các nút trỏ tới trang Releases.
- `main.js` chọn ngôn ngữ theo vị trí: hỏi quốc gia của người xem qua `get.geojs.io` (VN → tiếng Việt, nơi khác → tiếng Anh). Người xem tự bấm EN/VI thì lựa chọn đó được nhớ.
- `demo/`: giao diện thật của app (build từ `index.html` + `src/` ở gốc repo) chạy với dữ liệu giả lập trong `scripts/website-demo/mock.js`; khi trang ở tiếng Anh (`?lang=en`), app tự dịch bằng `src/i18n.ts` (chuỗi mới trong app cần thêm bản dịch vào đó). **Đừng sửa tay**; sau khi đổi giao diện app, chạy lại:

  ```bash
  node scripts/build-website-demo.mjs
  ```
- `receive/`: trang nhận link "Gửi qua Internet" (`https://gmttuts.github.io/better-drop/receive/#…`). JS thuần, giải mã bằng WebCrypto ngay trên trình duyệt, lưu thẳng xuống đĩa (`sw.js` cho trình duyệt không có File System Access). Định dạng phải khớp `src-tauri/src/online/`; xem `docs/GUI-QUA-INTERNET.md`.
- `assets/`: icon, ảnh chia sẻ (`og.png`), mã QR ủng hộ.

Xem thử trên máy:

```bash
npx vite website
```
