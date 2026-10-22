// Builds the real app UI (index.html + src/) into website/demo/, with scripts/website-demo/mock.js standing in
// for the Rust side, so visitors can click around the actual interface on the website (in English too, via
// scripts/website-demo/i18n-en.js).
// Run after changing the app's UI:  node scripts/build-website-demo.mjs
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { build } from "vite";

const root = fileURLToPath(new URL("..", import.meta.url));
const read = (f) => readFileSync(new URL(`./website-demo/${f}`, import.meta.url), "utf8");
// mock.js stands in for Rust; i18n-en.js translates the UI when the page opens the demo with ?lang=en.
const inject = `<script>${read("mock.js")}</script>\n<script>${read("i18n-en.js")}</script>`;

await build({
  root,
  configFile: false,
  base: "./",
  logLevel: "warn",
  build: { outDir: "website/demo", emptyOutDir: true, assetsInlineLimit: 0 },
  plugins: [
    {
      name: "betterdrop-demo-mock",
      transformIndexHtml: (html) =>
        html
          .replace("<head>", () => `<head>\n<meta name="robots" content="noindex">\n${inject}`)
          .replace(/<title>[^<]*<\/title>/, "<title>BetterDrop (bản dùng thử)</title>"),
    },
  ],
});
console.log("website/demo built");
