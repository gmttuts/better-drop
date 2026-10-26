// Builds the real app UI (index.html + src/) into website/demo/, with scripts/website-demo/mock.js standing in
// for the Rust side, so visitors can click around the actual interface on the website (in English too, via
// src/i18n.ts).
// Run after changing the app's UI:  node scripts/build-website-demo.mjs
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { build } from "vite";

const root = fileURLToPath(new URL("..", import.meta.url));
const read = (f) => readFileSync(new URL(`./website-demo/${f}`, import.meta.url), "utf8");
// mock.js stands in for Rust. English: the app translates itself with src/i18n.ts when the demo is opened
// with ?lang=en (mock.js sets the language setting from that parameter).
// theme.js lets the website switch the demo between light and dark.
const inject = `<script>${read("mock.js")}</script>\n<script>${read("theme.js")}</script>`;

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
