// Builds the real app UI (index.html + src/) into website/demo/, with scripts/website-demo/mock.js standing in
// for the Rust side, so visitors can click around the actual interface on the website.
// Run after changing the app's UI:  node scripts/build-website-demo.mjs
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { build } from "vite";

const root = fileURLToPath(new URL("..", import.meta.url));
const mock = readFileSync(new URL("./website-demo/mock.js", import.meta.url), "utf8");

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
          .replace("<head>", `<head>\n<meta name="robots" content="noindex">\n<script>${mock}</script>`)
          .replace(/<title>[^<]*<\/title>/, "<title>BetterDrop (bản dùng thử)</title>"),
    },
  ],
});
console.log("website/demo built");
