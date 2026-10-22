# Website BetterDrop

Trang giới thiệu và tải về, HTML/CSS/JS thuần, không cần build. Được đăng lên GitHub Pages bởi `.github/workflows/website.yml` mỗi khi `website/**` thay đổi trên `main`.

- `index.html`: khung trang. Chữ trên trang nằm trong `content.js` (tiếng Việt và tiếng Anh).
- `main.js`: đổi ngôn ngữ, nhận biết hệ điều hành, lấy file cài từ bản phát hành mới nhất trên GitHub (`/releases/latest`). Bản nháp (draft) không hiện ra, nên khi chưa có bản công khai các nút trỏ tới trang Releases.
- `assets/`: icon, ảnh chụp app (`app.jpg`), ảnh chia sẻ (`og.png`).

Xem thử trên máy:

```bash
npx vite website
```
