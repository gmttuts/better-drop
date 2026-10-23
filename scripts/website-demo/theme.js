// Lets the website switch the demo between light and dark. The app styles itself with
// @media (prefers-color-scheme: dark), which an iframe can't be told to change, so this flips those media
// rules on or off. The page calls window.__bdSetTheme("light" | "dark"); ?theme= sets the first one.
(() => {
  let theme = new URLSearchParams(location.search).get("theme");
  const DARK = /prefers-color-scheme:\s*dark/;
  function apply() {
    if (theme !== "light" && theme !== "dark") return;
    document.documentElement.style.colorScheme = theme;
    for (const sheet of document.styleSheets) {
      let rules;
      try { rules = sheet.cssRules; } catch { continue; }
      for (const r of rules) {
        if (!(r instanceof CSSMediaRule)) continue;
        // Remember the original condition so the rule can be switched back later.
        if (r.__bdMedia === undefined) {
          if (!DARK.test(r.media.mediaText)) continue;
          r.__bdMedia = r.media.mediaText;
        }
        r.media.mediaText = theme === "dark" ? "all" : "not all";
      }
    }
  }
  window.__bdSetTheme = (t) => { theme = t; apply(); };
  document.addEventListener("DOMContentLoaded", apply);
  window.addEventListener("load", apply);
})();
