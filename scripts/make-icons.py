"""Tách hình đám mây trắng từ app-icon.png để làm:
- lớp trước icon thích ứng (adaptive) Android, nằm trong vùng an toàn;
- icon đơn sắc (template) trên thanh menu macOS;
- logo nhỏ dùng trong giao diện."""
import sys
from PIL import Image, ImageChops

src, out_dir, res_dir = sys.argv[1], sys.argv[2], sys.argv[3]
img = Image.open(src).convert("RGBA")
r, g, b, a = img.split()
# Nền xanh có kênh đỏ ~0-20, hình trắng = 255: dùng kênh đỏ làm độ đậm của hình.
glyph_alpha = r.point(lambda v: 0 if v < 40 else min(255, int((v - 40) * 255 / 200)))
glyph_alpha = ImageChops.multiply(glyph_alpha, a)
bbox = glyph_alpha.getbbox()
print("glyph bbox", bbox)

def glyph(color, size, fill_ratio):
    """Hình đám mây màu `color`, căn giữa trong ô vuông `size`, chiếm `fill_ratio`."""
    crop = glyph_alpha.crop(bbox)
    w, h = crop.size
    scale = size * fill_ratio / max(w, h)
    crop = crop.resize((max(1, round(w * scale)), max(1, round(h * scale))), Image.LANCZOS)
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    solid = Image.new("RGBA", crop.size, color)
    solid.putalpha(crop)
    canvas.paste(solid, ((size - crop.size[0]) // 2, (size - crop.size[1]) // 2), solid)
    return canvas

# Android adaptive: lớp trước 108dp, vùng an toàn là vòng tròn đường kính 66dp ở giữa.
for folder, px in {"mdpi": 108, "hdpi": 162, "xhdpi": 216, "xxhdpi": 324, "xxxhdpi": 432}.items():
    glyph((255, 255, 255, 255), px, 0.50).save(f"{res_dir}/mipmap-{folder}/ic_launcher_foreground.png")

# Thanh menu macOS: icon template đen + trong suốt, hệ thống tự đổi màu sáng/tối.
glyph((0, 0, 0, 255), 64, 0.92).save(f"{out_dir}/tray-template.png")

# Logo nhỏ cho giao diện (giữ nguyên nền xanh bo góc).
img.resize((96, 96), Image.LANCZOS).save(f"{out_dir}/logo-96.png")
print("ok")
