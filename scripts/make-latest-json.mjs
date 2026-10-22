// Tạo latest.json cho tính năng tự cập nhật từ các file cài đã ký trong thư mục release.
// Dùng trong CI: node scripts/make-latest-json.mjs <thư mục file> <tag> <repo>
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

const [dir, tag, repo] = process.argv.slice(2);
const files = [];
(function walk(d) {
  for (const n of readdirSync(d)) {
    const p = join(d, n);
    statSync(p).isDirectory() ? walk(p) : files.push(p);
  }
})(dir);

const url = (f) => `https://github.com/${repo}/releases/download/${tag}/${basename(f)}`;
const signed = (test) => {
  const f = files.find((p) => test(basename(p)) && files.includes(p + ".sig"));
  return f && { signature: readFileSync(f + ".sig", "utf8").trim(), url: url(f) };
};

const mac = signed((n) => n.endsWith(".app.tar.gz"));
const nsis = signed((n) => n.endsWith("-setup.exe"));
const msi = signed((n) => n.endsWith(".msi"));
const appimage = signed((n) => n.endsWith(".AppImage"));
const deb = signed((n) => n.endsWith(".deb"));
const rpm = signed((n) => n.endsWith(".rpm"));

const platforms = {};
const put = (keys, v) => v && keys.forEach((k) => (platforms[k] = v));
put(["darwin-aarch64", "darwin-x86_64", "darwin-aarch64-app", "darwin-x86_64-app"], mac);
put(["windows-x86_64", "windows-x86_64-nsis"], nsis);
put(["windows-x86_64-msi"], msi);
put(["linux-x86_64", "linux-x86_64-appimage"], appimage);
put(["linux-x86_64-deb"], deb);
put(["linux-x86_64-rpm"], rpm);

if (!Object.keys(platforms).length) {
  console.error("Không có file cài nào được ký (.sig) — bỏ qua latest.json");
  process.exit(0);
}
const out = {
  version: tag.replace(/^v/, ""),
  notes: `BetterDrop ${tag}`,
  pub_date: new Date().toISOString(),
  platforms,
};
writeFileSync(join(dir, "latest.json"), JSON.stringify(out, null, 2));
console.log(Object.keys(platforms).join(", "));
