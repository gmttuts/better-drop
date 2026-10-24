// Trang nhận "Gửi qua Internet" của BetterDrop.
// Link: https://gmttuts.github.io/better-drop/nhan/#1.<dịch vụ>.<mã file>.<khoá base64url>
// Phần sau # không bao giờ được trình duyệt gửi lên máy chủ. File được giải mã ngay tại đây bằng
// WebCrypto (AES-256-GCM theo khối, định dạng .bdrop v1, xem src-tauri/src/online/crypto.rs) và
// ghi thẳng xuống đĩa từng khối, không giữ cả file trong bộ nhớ (trừ trình duyệt không hỗ trợ).
(() => {
  "use strict";

  // ---------- Chữ (tiếng Việt / tiếng Anh) ----------
  const TEXT = {
    title: { vi: "Có người gửi file cho bạn", en: "Someone sent you files" },
    sub: {
      vi: "Gửi bằng BetterDrop. File đã được mã hoá; khoá giải mã nằm trong link và chỉ trình duyệt của bạn đọc được.",
      en: "Sent with BetterDrop. The files are encrypted; the key is in the link and only your browser can read it.",
    },
    pastePh: { vi: "Dán link BetterDrop vào đây", en: "Paste a BetterDrop link here" },
    open: { vi: "Mở", en: "Open" },
    noLink: { vi: "Không thấy link nhận file. Dán link bạn được gửi:", en: "No transfer link found. Paste the link you were sent:" },
    badLink: { vi: "Link không hợp lệ (thiếu hoặc sai phần sau dấu #). Hãy chép lại đầy đủ link.", en: "This link isn't valid (the part after # is missing or wrong). Copy the whole link again." },
    newer: { vi: "Link được tạo bằng BetterDrop mới hơn trang này. Hãy tải lại trang.", en: "This link was made by a newer BetterDrop. Reload the page." },
    unknownProvider: { vi: "Link dùng dịch vụ lưu file mà trang này chưa hỗ trợ.", en: "This link uses a file host this page doesn't support yet." },
    meta: { vi: "Lưu ở {p}, mã {id}", en: "Stored on {p}, ID {id}" },
    step1: { vi: "Tải file đã mã hoá", en: "Download the encrypted file" },
    step1Note: {
      vi: "Mở trang {p} rồi bấm Download. Bạn sẽ được file {f} (chưa mở được, cần giải mã ở bước 2).",
      en: "Open the {p} page and click Download. You'll get {f} (it can't be opened yet; decrypt it in step 2).",
    },
    openProvider: { vi: "Mở trang tải {p} ↗", en: "Open {p} ↗" },
    step2: { vi: "Giải mã và lưu", en: "Decrypt and save" },
    drop: { vi: "Kéo thả file betterdrop.bdrop vào đây", en: "Drop betterdrop.bdrop here" },
    dropOr: { vi: "hoặc bấm để chọn file đã tải", en: "or click to choose the downloaded file" },
    checking: { vi: "Đang kiểm tra khoá…", en: "Checking the key…" },
    files: { vi: "{n} file · {s}", en: "{n} file{pl} · {s}" },
    zipNote: { vi: "Nhiều file được gói trong một file .zip; mở file đó để giải nén.", en: "Multiple files come as one .zip; open it to extract." },
    save: { vi: "Lưu file", en: "Save file" },
    memWarn: {
      vi: "Trình duyệt này phải giữ cả file trong bộ nhớ trước khi lưu. Với file lớn, nên dùng Chrome hoặc Edge trên máy tính, hoặc app BetterDrop.",
      en: "This browser has to hold the whole file in memory before saving. For large files, use Chrome or Edge on a computer, or the BetterDrop app.",
    },
    progress: { vi: "Đang giải mã… {p}% · {a} / {b}", en: "Decrypting… {p}% · {a} / {b}" },
    done: { vi: "Đã lưu {name}.", en: "Saved {name}." },
    wrongKey: { vi: "Sai khoá giải mã, hoặc đây không phải file của link này.", en: "Wrong key, or this file doesn't belong to this link." },
    damaged: { vi: "File bị hỏng hoặc chưa tải hết. Hãy tải lại file ở bước 1.", en: "The file is damaged or incomplete. Download it again in step 1." },
    notBdrop: { vi: "Đây không phải file BetterDrop (.bdrop).", en: "This isn't a BetterDrop (.bdrop) file." },
    fileNewer: { vi: "File được tạo bằng BetterDrop mới hơn trang này. Hãy tải lại trang.", en: "This file was made by a newer BetterDrop. Reload the page." },
    cancelled: { vi: "Đã huỷ lưu.", en: "Saving was cancelled." },
    expired: { vi: "File không còn trên {p}: link đã hết hạn hoặc file đã bị xoá.", en: "The file is no longer on {p}: the link expired or the file was deleted." },
    fetchFailed: {
      vi: "Không tự tải được từ {p} ({e}). Bấm nút ở bước 1 để tải file, rồi thả vào bước 2.",
      en: "Couldn't download from {p} automatically ({e}). Use the button in step 1 to download the file, then drop it in step 2.",
    },
    failed: { vi: "Lỗi khi lưu: {e}", en: "Couldn't save: {e}" },
    appTitle: { vi: "Có app BetterDrop?", en: "Have the BetterDrop app?" },
    appText: {
      vi: "Mở BetterDrop, bấm “Nhận bằng link” và dán link này.",
      en: "Open BetterDrop, click “Receive from a link” and paste this link.",
    },
    appTextDirect: {
      vi: "Mở BetterDrop, bấm “Nhận bằng link” và dán link này: app tự tải về và giải mã, không cần bước 1.",
      en: "Open BetterDrop, click “Receive from a link” and paste this link: the app downloads and decrypts it for you, no step 1.",
    },
    copyLink: { vi: "Sao chép link", en: "Copy link" },
    copied: { vi: "Đã sao chép", en: "Copied" },
    privacy: {
      vi: "Trang này không gửi khoá hay nội dung file đi đâu cả: mọi thứ được giải mã trong trình duyệt của bạn.",
      en: "This page never sends the key or the file contents anywhere: everything is decrypted in your browser.",
    },
  };

  const store = {
    get(k) { try { return localStorage.getItem(k); } catch { return null; } },
    set(k, v) { try { localStorage.setItem(k, v); } catch {} },
  };
  const urlLang = new URLSearchParams(location.search).get("lang");
  const browserVi = /^vi\b/i.test((navigator.languages && navigator.languages[0]) || navigator.language || "");
  let lang = urlLang || store.get("bd.lang") || (browserVi ? "vi" : "en");
  if (!["vi", "en"].includes(lang)) lang = "en";
  const t = (key, vars = {}) =>
    (TEXT[key] ? TEXT[key][lang] : key).replace(/\{(\w+)\}/g, (_, k) => (vars[k] ?? ""));
  const $ = (id) => document.getElementById(id);

  // ---------- Dịch vụ lưu file ----------
  // direct: URL để trang tự tải (dịch vụ phải cho phép CORS). null: người nhận tự tải file .bdrop.
  // Khớp với src-tauri/src/online/providers.rs.
  const PROVIDERS = {
    st: {
      name: "storage.to",
      valid: (id) => /^[A-Za-z0-9_-]{6,32}$/.test(id),
      page: (id) => `https://storage.to/${id}`,
      direct: null,
      appDirect: false,
    },
    ts: {
      name: "temp.sh",
      valid: (id) => /^[A-Za-z0-9_-]+\/[A-Za-z0-9_-][A-Za-z0-9_.-]*$/.test(id),
      page: (id) => `https://temp.sh/${id}`,
      direct: null,
      appDirect: true,
    },
  };
  const UPLOAD_NAME = "betterdrop.bdrop";

  /** Đọc link: { provider, id, key } hoặc { error }. */
  function parseLink(s) {
    const i = s.lastIndexOf("#");
    const frag = (i >= 0 ? s.slice(i + 1) : s).trim();
    if (!frag) return { error: "noLink" };
    const a = frag.indexOf(".");
    if (a < 0) return { error: "badLink" };
    if (frag.slice(0, a) !== "1") return { error: /^\d+$/.test(frag.slice(0, a)) ? "newer" : "badLink" };
    const b = frag.indexOf(".", a + 1);
    const c = frag.lastIndexOf(".");
    if (b < 0 || c <= b) return { error: "badLink" };
    const code = frag.slice(a + 1, b);
    const id = frag.slice(b + 1, c);
    const provider = PROVIDERS[code];
    if (!provider) return { error: "unknownProvider" };
    if (!provider.valid(id)) return { error: "badLink" };
    const key = b64urlDecode(frag.slice(c + 1));
    if (!key || key.length !== 32) return { error: "badLink" };
    return { provider, id, key };
  }

  function b64urlDecode(s) {
    if (!/^[A-Za-z0-9_-]+=*$/.test(s)) return null;
    const b = s.replace(/=+$/, "").replace(/-/g, "+").replace(/_/g, "/");
    try {
      const bin = atob(b + "===".slice((b.length + 3) % 4));
      return Uint8Array.from(bin, (ch) => ch.charCodeAt(0));
    } catch {
      return null;
    }
  }

  function formatBytes(n) {
    if (n < 1024) return `${n} B`;
    const u = ["KB", "MB", "GB", "TB"];
    let v = n / 1024;
    let i = 0;
    while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
    return `${v.toFixed(v < 10 ? 1 : 0)} ${u[i]}`;
  }

  // ---------- Giải mã (định dạng .bdrop v1) ----------
  const HEADER_LEN = 10;
  const TAG_LEN = 16;
  const MAX_CHUNK = 16 << 20;

  class BdropError extends Error {
    constructor(code) { super(code); this.code = code; }
  }

  function nonce(index, last) {
    const n = new Uint8Array(12);
    // 3 byte 0 | số thứ tự khối (u64 big-endian) | cờ khối cuối
    let x = BigInt(index);
    for (let i = 10; i >= 3; i--) { n[i] = Number(x & 0xffn); x >>= 8n; }
    n[11] = last ? 1 : 0;
    return n;
  }

  /** Nguồn byte từ một File (đọc từng đoạn, không đọc cả file vào RAM). */
  function fileSource(file) {
    let pos = 0;
    return {
      size: file.size,
      async read(n) {
        const end = Math.min(file.size, pos + n);
        const buf = new Uint8Array(await file.slice(pos, end).arrayBuffer());
        pos = end;
        return buf;
      },
    };
  }

  /** Nguồn byte từ fetch (dịch vụ cho phép CORS). */
  async function fetchSource(url) {
    const r = await fetch(url, { referrerPolicy: "no-referrer" });
    if (r.status === 404 || r.status === 410) throw new BdropError("expired");
    if (!r.ok || !r.body) throw new Error(`HTTP ${r.status}`);
    const reader = r.body.getReader();
    let buf = new Uint8Array(0);
    let ended = false;
    const size = Number(r.headers.get("content-length")) || 0;
    return {
      size,
      async read(n) {
        while (buf.length < n && !ended) {
          const { value, done } = await reader.read();
          if (done) { ended = true; break; }
          const next = new Uint8Array(buf.length + value.length);
          next.set(buf);
          next.set(value, buf.length);
          buf = next;
        }
        const out = buf.slice(0, n);
        buf = buf.slice(out.length);
        return out;
      },
    };
  }

  /** Giải mã từng khối. Trả về hàm lấy khối dữ liệu gốc tiếp theo (null khi hết). */
  async function decryptor(src, rawKey) {
    const header = await src.read(HEADER_LEN);
    const magic = String.fromCharCode(...header.slice(0, 4));
    if (header.length < HEADER_LEN || magic !== "BDRP") throw new BdropError("notBdrop");
    if (header[4] !== 1 || header[5] !== 1) throw new BdropError("fileNewer");
    const chunk = new DataView(header.buffer, header.byteOffset).getUint32(6);
    if (!chunk || chunk > MAX_CHUNK) throw new BdropError("notBdrop");
    const key = await crypto.subtle.importKey("raw", rawKey, "AES-GCM", false, ["decrypt"]);
    const full = chunk + TAG_LEN;
    const open = (data, index, last) =>
      crypto.subtle.decrypt({ name: "AES-GCM", iv: nonce(index, last), additionalData: header, tagLength: 128 }, key, data);
    let index = 0;
    let done = false;
    // Đọc trước 1 byte để biết khối hiện tại có phải khối cuối không.
    let pending = await src.read(full + 1);
    return async function next() {
      if (done) return null;
      if (pending.length < TAG_LEN) throw new BdropError("damaged");
      const last = pending.length <= full;
      const block = pending.slice(0, Math.min(full, pending.length));
      const rest = pending.slice(block.length);
      let plain;
      try {
        plain = new Uint8Array(await open(block, index, last));
      } catch {
        if (index > 0) throw new BdropError("damaged");
        // Khối đầu: mở được khi coi là "chưa phải khối cuối" nghĩa là khoá đúng nhưng file bị cắt.
        const truncated = await open(block, 0, !last).then(() => true, () => false);
        throw new BdropError(truncated ? "damaged" : "wrongKey");
      }
      index++;
      if (last) {
        done = true;
      } else {
        const more = await src.read(full + 1 - rest.length);
        pending = new Uint8Array(rest.length + more.length);
        pending.set(rest);
        pending.set(more, rest.length);
      }
      return plain;
    };
  }

  /** Đọc manifest ở đầu dữ liệu gốc. Trả về { manifest, first, next } (first: phần nội dung đã đọc). */
  async function openPackage(src, key) {
    const next = await decryptor(src, key);
    let buf = new Uint8Array(0);
    const need = async (n) => {
      while (buf.length < n) {
        const p = await next();
        if (!p) throw new BdropError("damaged");
        const b = new Uint8Array(buf.length + p.length);
        b.set(buf);
        b.set(p, buf.length);
        buf = b;
      }
    };
    await need(4);
    const len = new DataView(buf.buffer).getUint32(0);
    if (len > 16 << 20) throw new BdropError("notBdrop");
    await need(4 + len);
    let manifest;
    try {
      manifest = JSON.parse(new TextDecoder().decode(buf.slice(4, 4 + len)));
    } catch {
      throw new BdropError("notBdrop");
    }
    if (manifest.v !== 1) throw new BdropError("fileNewer");
    return { manifest, first: buf.slice(4 + len), next };
  }

  // ---------- Lưu xuống đĩa ----------
  // ?sink=sw / ?sink=blob: ép dùng cách lưu dự phòng (để kiểm thử).
  const forcedSink = new URLSearchParams(location.search).get("sink");
  const canPickSave = typeof window.showSaveFilePicker === "function" && !forcedSink;
  const isSafari = /^((?!chrome|android|crios|fxios|edg).)*safari/i.test(navigator.userAgent);
  let swReady = null;
  if (!canPickSave && (!isSafari || forcedSink === "sw") && forcedSink !== "blob" && "serviceWorker" in navigator && window.isSecureContext) {
    swReady = navigator.serviceWorker.register("sw.js").then(() => navigator.serviceWorker.ready).catch(() => null);
  }

  /** Nơi ghi: write(chunk) theo thứ tự, close() khi xong, abort() khi lỗi. */
  async function openSink(name, size) {
    if (canPickSave) {
      const handle = await window.showSaveFilePicker({ suggestedName: name });
      const w = await handle.createWritable();
      return { write: (c) => w.write(c), close: () => w.close(), abort: () => w.abort() };
    }
    const reg = swReady && (await swReady);
    if (reg && reg.active && navigator.serviceWorker.controller) return swSink(reg.active, name, size);
    // Dự phòng: gom vào Blob rồi tải về (trình duyệt giữ trong bộ nhớ).
    const parts = [];
    return {
      write: async (c) => { parts.push(c); },
      close: async () => {
        const url = URL.createObjectURL(new Blob(parts, { type: "application/octet-stream" }));
        const a = Object.assign(document.createElement("a"), { href: url, download: name });
        document.body.append(a);
        a.click();
        a.remove();
        setTimeout(() => URL.revokeObjectURL(url), 60_000);
      },
      abort: async () => { parts.length = 0; },
    };
  }

  /** Service Worker trả một luồng tải xuống; trang đẩy từng khối khi SW xin thêm. */
  function swSink(worker, name, size) {
    const id = Math.random().toString(36).slice(2) + Date.now().toString(36);
    const ch = new MessageChannel();
    let wantMore = null;
    let cancelled = false;
    let pulls = 0;
    ch.port1.onmessage = ({ data }) => {
      if (data.type === "pull") {
        pulls++;
        if (wantMore) { wantMore(); wantMore = null; }
      } else if (data.type === "cancel") {
        cancelled = true;
        if (wantMore) { wantMore(); wantMore = null; }
      }
    };
    worker.postMessage({ type: "bd-download", id, name, size }, [ch.port2]);
    const ping = setInterval(() => worker.postMessage({ type: "bd-ping" }), 10_000);
    const iframe = Object.assign(document.createElement("iframe"), { hidden: true, src: `dl/${id}/${encodeURIComponent(name)}` });
    document.body.append(iframe);
    const stop = () => { clearInterval(ping); setTimeout(() => iframe.remove(), 60_000); };
    return {
      async write(c) {
        if (pulls === 0) await new Promise((r) => (wantMore = r));
        if (cancelled) throw new BdropError("cancelled");
        pulls--;
        ch.port1.postMessage({ type: "chunk", chunk: c }, [c.buffer]);
      },
      async close() { ch.port1.postMessage({ type: "end" }); stop(); },
      async abort() { ch.port1.postMessage({ type: "abort" }); stop(); },
    };
  }

  // ---------- Giao diện ----------
  let link = null;
  let opened = null; // { manifest, first, next, total }

  function applyLang() {
    document.documentElement.lang = lang;
    document.title = `${t("title")} · BetterDrop`;
    document.querySelectorAll("[data-t]").forEach((n) => (n.textContent = t(n.dataset.t)));
    document.querySelectorAll("[data-t-placeholder]").forEach((n) => (n.placeholder = t(n.dataset.tPlaceholder)));
    document.querySelectorAll("[data-lang]").forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.lang === lang)));
    render();
  }

  function show(id, on) { $(id).hidden = !on; }

  function setError(code, extra) {
    const msg = TEXT[code] ? t(code, extra) : String(code);
    $("error").textContent = msg;
    show("error", true);
  }

  function render() {
    const parsed = link;
    if (!parsed || parsed.error) {
      show("bad", true);
      show("steps", false);
      show("app-box", false);
      $("bad-msg").textContent = t(parsed ? parsed.error : "noLink");
      return;
    }
    show("bad", false);
    show("steps", true);
    show("app-box", true);
    const p = parsed.provider;
    $("meta").textContent = t("meta", { p: p.name, id: parsed.id });
    $("step1-note").textContent = t("step1Note", { p: p.name, f: UPLOAD_NAME });
    $("provider-link").textContent = t("openProvider", { p: p.name });
    $("provider-link").href = p.page(parsed.id);
    show("manual-steps", !p.direct || !opened);
    $("app-text").textContent = t(p.appDirect ? "appTextDirect" : "appText");
    if (opened) renderReady();
  }

  function renderReady() {
    const m = opened.manifest;
    const n = m.files.length;
    $("out-name").textContent = m.name;
    $("out-info").textContent = t("files", { n, pl: n === 1 ? "" : "s", s: formatBytes(m.size) }) + (m.kind === "zip" ? ` · ${t("zipNote")}` : "");
    show("mem-warn", !canPickSave && !(navigator.serviceWorker && navigator.serviceWorker.controller) && m.size > 300e6);
    $("mem-warn").textContent = t("memWarn");
    show("ready", true);
  }

  async function loadFile(file) {
    show("error", false);
    show("done", false);
    show("ready", false);
    opened = null;
    try {
      $("progress-text").textContent = t("checking");
      show("progress", true);
      const src = fileSource(file);
      opened = { ...(await openPackage(src, link.key)), total: file.size };
      show("progress", false);
      renderReady();
    } catch (e) {
      show("progress", false);
      setError(e instanceof BdropError ? e.code : "failed", { e: e.message });
    }
  }

  async function save() {
    if (!opened) return;
    const { manifest, first, next } = opened;
    let sink;
    try {
      sink = await openSink(manifest.name, manifest.size);
    } catch (e) {
      if (e && e.name === "AbortError") return; // bấm Huỷ ở hộp thoại lưu
      return setError("failed", { e: e.message });
    }
    show("ready", false);
    show("progress", true);
    let written = 0;
    const report = () => {
      const pct = manifest.size ? Math.floor((written / manifest.size) * 100) : 100;
      $("bar").style.width = `${pct}%`;
      $("progress-text").textContent = t("progress", { p: pct, a: formatBytes(written), b: formatBytes(manifest.size) });
    };
    try {
      // Phần nội dung là mọi thứ sau manifest, đúng manifest.size byte.
      let chunk = first;
      while (chunk) {
        const len = chunk.length; // đọc trước: gửi sang Service Worker thì `chunk` bị chuyển đi (rỗng)
        if (len) {
          if (written + len > manifest.size) throw new BdropError("damaged");
          await sink.write(chunk);
          written += len;
          report();
        }
        chunk = await next();
      }
      if (written !== manifest.size) throw new BdropError("damaged");
      await sink.close();
      show("progress", false);
      $("done").textContent = t("done", { name: manifest.name });
      show("done", true);
    } catch (e) {
      try { await sink.abort(); } catch {}
      show("progress", false);
      setError(e instanceof BdropError ? e.code : "failed", { e: e.message });
    }
    opened = null; // luồng đã dùng hết; muốn lưu lại thì chọn lại file
  }

  function setLink(s) {
    link = parseLink(s);
    opened = null;
    show("ready", false);
    show("done", false);
    show("error", false);
    render();
    if (link && !link.error && link.provider.direct) loadDirect();
  }

  async function loadDirect() {
    try {
      $("progress-text").textContent = t("checking");
      show("progress", true);
      const src = await fetchSource(link.provider.direct(link.id));
      opened = { ...(await openPackage(src, link.key)), total: src.size };
      show("progress", false);
      render();
    } catch (e) {
      show("progress", false);
      // Không tải thẳng được thì vẫn còn cách tải tay ở bước 1.
      const p = link.provider.name;
      setError(e instanceof BdropError ? e.code : "fetchFailed", { p, e: e.message });
    }
  }

  // Kéo thả / chọn file
  const drop = $("drop");
  ["dragenter", "dragover"].forEach((ev) => drop.addEventListener(ev, (e) => { e.preventDefault(); drop.classList.add("hover"); }));
  ["dragleave", "drop"].forEach((ev) => drop.addEventListener(ev, () => drop.classList.remove("hover")));
  drop.addEventListener("drop", (e) => {
    e.preventDefault();
    const f = e.dataTransfer && e.dataTransfer.files[0];
    if (f) loadFile(f);
  });
  // Thả nhầm ra ngoài ô thì không để trình duyệt mở file.
  window.addEventListener("dragover", (e) => e.preventDefault());
  window.addEventListener("drop", (e) => {
    e.preventDefault();
    const f = e.dataTransfer && e.dataTransfer.files[0];
    if (f && link && !link.error) loadFile(f);
  });
  $("file-input").addEventListener("change", (e) => {
    const f = e.target.files[0];
    if (f) loadFile(f);
    e.target.value = "";
  });
  $("btn-save").addEventListener("click", save);
  $("btn-copy").addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(location.href);
      $("btn-copy").textContent = t("copied");
      setTimeout(() => ($("btn-copy").textContent = t("copyLink")), 1500);
    } catch {}
  });
  $("paste-form").addEventListener("submit", (e) => {
    e.preventDefault();
    const v = $("paste-link").value.trim();
    if (!v) return;
    const i = v.lastIndexOf("#");
    if (i >= 0) location.hash = v.slice(i + 1);
    else setLink(v);
  });
  document.querySelectorAll("[data-lang]").forEach((b) =>
    b.addEventListener("click", () => {
      lang = b.dataset.lang;
      store.set("bd.lang", lang);
      applyLang();
    }),
  );
  window.addEventListener("hashchange", () => setLink(location.hash));

  // Cho phép kiểm thử giải mã (xem docs/GUI-QUA-INTERNET.md).
  window.BetterDropReceive = { parseLink, openPackage, fileSource, fetchSource };

  link = parseLink(location.hash);
  applyLang();
  if (link && !link.error && link.provider.direct) loadDirect();
})();
