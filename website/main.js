// Fills the page from content.js: language, download buttons (from the latest GitHub release), FAQ.
(() => {
  const S = window.SITE;
  const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];
  const each = (sel, fn) => $$(sel).forEach(fn);
  const store = {
    get(k) { try { return localStorage.getItem(k); } catch { return null; } },
    set(k, v) { try { localStorage.setItem(k, v); } catch {} },
  };
  const RELEASES = `https://github.com/${S.repo}/releases`;

  // ---------- Language: by where the visitor is (Vietnam → Vietnamese, elsewhere → English) ----------
  // Order: ?lang= in the URL, then a language the visitor picked, then their country, then a guess from the
  // browser while the country lookup is still running (or if it fails).
  const browserVi = () => /^vi\b/i.test((navigator.languages && navigator.languages[0]) || navigator.language || "");
  function guessLang() {
    let tz = "";
    try { tz = Intl.DateTimeFormat().resolvedOptions().timeZone || ""; } catch {}
    return browserVi() || /^Asia\/(Ho_Chi_Minh|Saigon)$/.test(tz) ? "vi" : "en";
  }
  const langFor = (country) => (country === "VN" || browserVi() ? "vi" : "en");
  const urlLang = new URLSearchParams(location.search).get("lang");
  const picked = store.get("bd.lang");
  const knownCountry = store.get("bd.country");
  let lang = urlLang || picked || (knownCountry ? langFor(knownCountry) : guessLang());
  if (!["vi", "en"].includes(lang)) lang = "en";
  async function lookupCountry() {
    if (urlLang || picked || knownCountry) return;
    try {
      const ctl = new AbortController();
      setTimeout(() => ctl.abort(), 2500);
      const r = await fetch("https://get.geojs.io/v1/ip/country.json", { signal: ctl.signal });
      const country = ((await r.json()).country || "").toUpperCase();
      if (!/^[A-Z]{2}$/.test(country)) return;
      store.set("bd.country", country);
      if (!store.get("bd.lang") && langFor(country) !== lang) { lang = langFor(country); applyLang(); }
    } catch {}
  }
  const t = (key, vars = {}) => {
    const s = (S.text[key] || {})[lang] ?? (S.text[key] || {}).vi ?? key;
    return s.replace(/\{(\w+)\}/g, (_, k) => vars[k] ?? "");
  };
  const tv = (v) => (v && typeof v === "object" && !Array.isArray(v) ? v[lang] ?? v.vi : v);

  // ---------- Which OS is the visitor on? ----------
  function detectOS() {
    const ua = navigator.userAgent;
    const plat = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
    if (/android/i.test(ua)) return "android";
    if (/iPhone|iPad|iPod/.test(ua) || (/Mac/.test(plat) && navigator.maxTouchPoints > 1)) return "ios";
    if (/Win/i.test(plat) || /Windows/.test(ua)) return "windows";
    if (/Mac/i.test(plat) || /Mac OS X/.test(ua)) return "mac";
    if (/Linux|X11|CrOS/i.test(plat + ua)) return "linux";
    return null;
  }
  const os = detectOS();

  const ICONS = {
    mac: '<svg viewBox="0 0 24 24"><path d="M16.4 12.6c0-2.5 2-3.6 2.1-3.7-1.2-1.7-3-1.9-3.6-2-1.5-.2-3 .9-3.7.9-.8 0-2-.9-3.2-.9-1.7 0-3.2 1-4.1 2.4-1.7 3-.4 7.4 1.2 9.8.8 1.2 1.8 2.5 3.1 2.4 1.2 0 1.7-.8 3.2-.8s1.9.8 3.2.8c1.3 0 2.2-1.2 3-2.4.9-1.4 1.3-2.7 1.3-2.8-.1 0-2.5-1-2.5-3.7zM14 5.2c.7-.8 1.1-1.9 1-3-1 0-2.1.7-2.8 1.5-.6.7-1.2 1.8-1 2.9 1.1.1 2.1-.6 2.8-1.4z"/></svg>',
    windows: '<svg viewBox="0 0 16 16"><path d="M1 2.3l5.7-.8v5.5H1zM7.5 1.4L15 .3V7H7.5zM1 8h5.7v5.5L1 12.7zM7.5 8H15v6.7l-7.5-1z"/></svg>',
    linux: '<svg viewBox="0 0 16 16"><path d="M8 1c-1.7 0-2.6 1.4-2.6 3.3 0 1-.5 1.6-1.2 2.5C3.3 8 2.6 9.3 2.6 10.6c0 .5.1 1 .3 1.3-.6.3-1 .8-1 1.3 0 .9 1.2 1.3 2.5 1.6.8.2 1.4.6 2 .9.5.2 1 .3 1.6.3s1.1-.1 1.6-.3c.6-.3 1.2-.7 2-.9 1.3-.3 2.5-.7 2.5-1.6 0-.5-.4-1-1-1.3.2-.3.3-.8.3-1.3 0-1.3-.7-2.6-1.6-3.8-.7-.9-1.2-1.5-1.2-2.5C10.6 2.4 9.7 1 8 1z"/></svg>',
    android: '<svg viewBox="0 0 24 24"><path d="M17.6 9.5l1.8-3.1a.4.4 0 00-.7-.4l-1.8 3.1A11 11 0 0012 8a11 11 0 00-4.9 1.1L5.3 6a.4.4 0 00-.7.4l1.8 3.1A10.4 10.4 0 001 17.8h22a10.4 10.4 0 00-5.4-8.3zM7 15a1 1 0 110-2 1 1 0 010 2zm10 0a1 1 0 110-2 1 1 0 010 2z"/></svg>',
    ios: '<svg viewBox="0 0 24 24"><path d="M16 1H8a3 3 0 00-3 3v16a3 3 0 003 3h8a3 3 0 003-3V4a3 3 0 00-3-3zm-4 20.2a1.2 1.2 0 110-2.4 1.2 1.2 0 010 2.4zM17 17H7V4h10z"/></svg>',
  };
  each("[data-os-row]", (row) => { row.innerHTML = S.platforms.map((p) => `<span title="${p.name}">${ICONS[p.os]}</span>`).join(""); });

  // ---------- Latest release from GitHub (cached for an hour; release drafts are invisible to the API) ----------
  let release = null; // { version, date, url, assets: [{name, url}] } once known
  let releaseState = "loading"; // loading | ok | none (no public release) | unknown (API unreachable)

  function assetFor(file) {
    if (release) return release.assets.find((a) => file.match.test(a.name)) || null;
    // GitHub API unreachable or rate-limited (not "no release"): link to the known version's file directly.
    if (releaseState === "unknown") {
      const v = S.fallbackVersion;
      return { url: `${RELEASES}/download/v${v}/${file.file.replace("{v}", v)}` };
    }
    return null;
  }
  function primaryHref() {
    const p = S.platforms.find((x) => x.os === os);
    const a = p && assetFor(p.files[0]);
    return a ? a.url : `${RELEASES}/latest`;
  }

  async function loadRelease() {
    const KEY = "bd.release";
    try {
      const cached = JSON.parse(sessionStorage.getItem(KEY) || "null");
      if (cached && Date.now() - cached.at < 3600e3) return cached.data;
    } catch {}
    let data = null;
    try {
      const r = await fetch(`https://api.github.com/repos/${S.repo}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } });
      if (!r.ok && r.status !== 404) return "unknown"; // rate limit etc.: don't cache, don't claim there's no release
      if (r.ok) {
        const j = await r.json();
        data = {
          version: (j.tag_name || "").replace(/^v/, ""),
          date: j.published_at,
          url: j.html_url,
          assets: (j.assets || []).map((a) => ({ name: a.name, url: a.browser_download_url, size: a.size })),
        };
      }
    } catch { return "unknown"; }
    try { sessionStorage.setItem(KEY, JSON.stringify({ at: Date.now(), data })); } catch {}
    return data;
  }

  // ---------- Rendering ----------
  const fmtSize = (n) => (n >= 1048576 ? `${(n / 1048576).toFixed(n < 10485760 ? 1 : 0)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`);
  const fmtDate = (iso) => new Date(iso).toLocaleDateString(lang === "vi" ? "vi-VN" : "en-US", { day: "numeric", month: "long", year: "numeric" });

  function renderDownloads() {
    const osName = (S.platforms.find((p) => p.os === os) || {}).name;
    each("[data-primary-label]", (el) => { el.textContent = osName ? t("dl.for", { os: osName }) : t("dl.generic"); });
    each("[data-os-icon]", (el) => { el.innerHTML = ICONS[os] || ""; el.hidden = !os; });
    each("[data-primary-download]", (a) => { a.href = primaryHref(); });
    each("[data-version-label]", (el) => { el.textContent = "v" + (release ? release.version : S.fallbackVersion); });
    each("[data-year]", (el) => { el.textContent = new Date().getFullYear(); });

    each("[data-release-line]", (el) => {
      if (releaseState === "loading") el.innerHTML = t("dl.loading");
      else if (releaseState === "unknown") el.innerHTML = `${t("dl.unknown", { v: "v" + S.fallbackVersion })} <a href="${RELEASES}">${t("dl.notes")} →</a>`;
      else if (release) el.innerHTML = `${t("dl.latest", { v: "v" + release.version, date: fmtDate(release.date) })} <a href="${release.url}">${t("dl.notes")} →</a>`;
      else el.innerHTML = `${t("dl.none")} <a href="${RELEASES}">github.com/${S.repo}/releases →</a>`;
    });

    each("[data-platforms]", (box) => {
      // The visitor's own platform goes first.
      const list = [...S.platforms].sort((a, b) => (b.os === os) - (a.os === os));
      box.innerHTML = list.map((p) => {
        const buttons = p.files.map((f, i) => {
          const a = assetFor(f);
          const href = a ? a.url : `${RELEASES}/latest`;
          const size = a && a.size ? ` <small>${fmtSize(a.size)}</small>` : "";
          return `<a class="btn ${i ? "btn-soft" : ""}" href="${href}">${f.label}${size}</a>`;
        }).join("");
        const steps = tv(p.steps).map((s) => `<li>${s}</li>`).join("");
        const code = p.code ? `<div class="code"><code>${p.code}</code><button type="button" data-copy>${t("copy")}</button></div>` : "";
        return `<article class="platform${p.os === os ? " mine" : ""}" id="dl-${p.os}">
          <header><span class="picon">${ICONS[p.os]}</span><div><h3>${p.name}${p.os === os ? ` <span class="badge">${t("dl.yours")}</span>` : ""}</h3><p class="fine">${tv(p.req)}</p></div></header>
          <div class="cta-row">${buttons}</div>
          <details><summary>${t("dl.how")}</summary><ol>${steps}</ol>${code}</details>
        </article>`;
      }).join("");
    });
  }

  function renderFeatures() {
    each("[data-features]", (box) => {
      box.innerHTML = S.features.map((f) => {
        const [title, body] = f[lang] || f.vi;
        return `<article class="feature"><span class="ficon">${f.icon}</span><h3>${title}</h3><p>${body}</p></article>`;
      }).join("");
    });
  }

  function renderShare() {
    each("[data-share]", (box) => {
      const list = [...S.share].sort((a, b) => (b.os === os) - (a.os === os));
      box.innerHTML = list.map((p) => `<article class="share-card${p.os === os ? " mine" : ""}">
          <header><span class="picon">${ICONS[p.os]}</span><h3>${p.name}</h3></header>
          <ul>${tv(p.steps).map((s) => `<li>${s}</li>`).join("")}</ul>
        </article>`).join("");
    });
  }

  function renderFaq() {
    each("[data-faq]", (box) => {
      box.innerHTML = S.faq.map((f) => {
        const [q, a] = f[lang] || f.vi;
        return `<details><summary>${q}</summary><p>${a}</p></details>`;
      }).join("");
    });
  }

  function applyLang() {
    document.documentElement.lang = lang;
    document.title = lang === "vi" ? "BetterDrop — gửi file giữa mọi thiết bị, như AirDrop" : "BetterDrop — send files between any devices, like AirDrop";
    each("[data-i18n]", (el) => { el.innerHTML = t(el.dataset.i18n); });
    each("[data-lang]", (b) => b.setAttribute("aria-pressed", String(b.dataset.lang === lang)));
    each("[data-i18n-label]", (el) => { el.setAttribute("aria-label", t(el.dataset.i18nLabel)); });
    renderFeatures();
    renderShare();
    renderFaq();
    renderDownloads();
    // The demo follows the page language (it restarts, which is fine for a demo).
    each("[data-demo]", (f) => {
      const src = `demo/index.html?lang=${lang}`;
      if (f.getAttribute("src") !== src) f.setAttribute("src", src);
    });
  }
  each("[data-lang]", (b) => b.addEventListener("click", () => {
    lang = b.dataset.lang;
    store.set("bd.lang", lang);
    applyLang();
  }));

  // ---------- Support the author (QR modal) ----------
  const modal = document.querySelector("[data-support-modal]");
  // which = "bmc", "momo" or "both" (both QR codes side by side).
  function openSupport(which) {
    if (!modal) return;
    const keys = which === "both" ? Object.keys(S.support) : [which];
    modal.querySelector("[data-support-title]").textContent = which === "both" ? t("support.title.both") : t("support." + which);
    modal.classList.toggle("both", keys.length > 1);
    modal.querySelector("[data-support-qr]").innerHTML = keys.map((k) => {
      const cfg = S.support[k];
      return `<div class="qr-card">${keys.length > 1 ? `<b>${t("support." + k)}</b>` : ""}
        <div class="qr"><img src="${cfg.qr}" alt="${t("support." + k)} QR" onerror="this.outerHTML='<div class=qr-missing>${t("support.noqr")}</div>'"></div>
        <a class="btn btn-small" href="${cfg.url}" target="_blank" rel="noopener">${t("support.open")}</a></div>`;
    }).join("");
    modal.showModal ? modal.showModal() : modal.setAttribute("open", "");
  }
  each("[data-support]", (b) => b.addEventListener("click", () => openSupport(b.dataset.support)));
  modal?.addEventListener("click", (e) => { if (e.target === modal || e.target.closest("[data-close]")) modal.close(); });

  // Copy buttons inside code boxes (rendered later, so delegate).
  document.addEventListener("click", async (e) => {
    const btn = e.target.closest("[data-copy]");
    if (!btn) return;
    const code = btn.parentElement.querySelector("code").textContent;
    try { await navigator.clipboard.writeText(code); } catch { return; }
    btn.textContent = t("copied");
    setTimeout(() => { btn.textContent = t("copy"); }, 1500);
  });

  // ---------- Live demo: the real app UI in an iframe, sized to its content so it never scrolls inside ----------
  // Below 400px it is laid out at phone width (400px) and scaled down to fit.
  const DEMO_MIN = 400;
  each("[data-demo]", (frame) => {
    const box = frame.parentElement;
    const fit = () => {
      const w = box.clientWidth;
      const logical = Math.max(w, DEMO_MIN);
      const scale = w / logical;
      frame.style.width = logical + "px";
      frame.style.transform = scale < 1 ? `scale(${scale})` : "";
      try {
        const doc = frame.contentDocument;
        if (!doc || !doc.body) return;
        const h = Math.max(520, doc.body.scrollHeight);
        frame.style.height = h + "px";
        box.style.height = Math.ceil(h * scale) + "px";
      } catch {}
    };
    fit();
    frame.addEventListener("load", () => {
      fit();
      try { new ResizeObserver(fit).observe(frame.contentDocument.body); } catch {}
    });
    window.addEventListener("resize", fit);
  });

  applyLang();
  lookupCountry();
  loadRelease().then((data) => {
    release = data === "unknown" ? null : data;
    releaseState = data === "unknown" ? "unknown" : data ? "ok" : "none";
    renderDownloads();
  });
})();
