// Stands in for the Rust side of BetterDrop so the real app UI (src/) can run on the website as a demo.
// Injected before the app's own script by scripts/build-website-demo.mjs. Nothing leaves the browser.
(() => {
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  let nextCb = 1;
  const handlers = {}; // event name -> [callback id]
  const emit = (event, payload) => (handlers[event] || []).forEach((id) => window["_" + id] && window["_" + id]({ event, id, payload }));

  // ---- Fake state ----
  const me = {
    info: { id: "me", name: "MacBook của bạn", os: "mac", fp: "520d9a1b", port: 53318, version: "0.3.3" },
    settings: { device_id: "me", name: "MacBook của bạn", download_dir: "~/Downloads/BetterDrop", receive_enabled: true, hide_dock: false,
      language: new URLSearchParams(location.search).get("lang") === "en" ? "en" : "vi" },
    addrs: ["192.168.1.12:53318"],
    autostart: true,
    // Narrow frame (the website on a phone): show the app's phone layout.
    mobile: window.innerWidth < 520,
  };
  if (me.mobile) Object.assign(me.info, { name: "iPhone của bạn", os: "ios" }), (me.settings.name = me.info.name);
  const peer = (id, name, os, fp, version, ip) => ({ id, name, os, fp, port: 53318, version, addrs: [ip + ":53318"], manual: false, online: true });
  let peers = [
    peer("p1", "Laptop Phạm Bá Cháy", "windows", "7f3ca1e0", "0.3.3", "192.168.1.21"),
    peer("p2", "Pixel 9 của Tuyến", "android", "c41e02b7", "0.3.3", "192.168.1.34"),
    peer("p3", "iPad của Dương Vũ", "ios", "9a0b77d2", "0.3.2", "192.168.1.40"),
    peer("p4", "Chande Sever", "linux", "e2d913aa", "0.3.1", "192.168.1.5"),
    peer("p5", "PC Thiết kế chờ nâng cấp", "windows", "1b7a4c90", "0.3.3", "192.168.1.27"),
  ];
  let groups = [{ id: "g1", name: "Phòng kế toán", members: [{ id: "p1", name: "Laptop Phạm Bá Cháy", os: "windows" }, { id: "p5", name: "PC Thiết kế chờ nâng cấp", os: "windows" }] }];
  let hidden = [];
  let trusted = [{ id: "p2", name: "Pixel 9 của Tuyến", os: "android", fp: "c41e02b7" }];
  const devices = () => ({ groups, hidden, trusted });

  // ---- History (finished transfers), newest first ----
  const now = () => Math.floor(Date.now() / 1000);
  let history = [
    { id: "h1", finished_at: now() - 1500, direction: "receive", peer_name: "Pixel 9 của Tuyến", peer_id: "p2", kind: "files", file_count: 2, total_bytes: 6291456, status: "done", error: null,
      dir_path: "~/Downloads/BetterDrop", files: [{ name: "Ảnh chứng từ.jpg", path: "~/Downloads/BetterDrop/Ảnh chứng từ.jpg" }, { name: "Hoá đơn VAT.pdf", path: "~/Downloads/BetterDrop/Hoá đơn VAT.pdf" }], text: null },
    { id: "h2", finished_at: now() - 3 * 3600, direction: "send", peer_name: "Laptop Phạm Bá Cháy", peer_id: "p1", kind: "text", file_count: 0, total_bytes: 40, status: "done", error: null,
      dir_path: null, files: [], text: "https://gmttuts.github.io/better-drop/" },
    { id: "h3", finished_at: now() - 26 * 3600, direction: "send", peer_name: "iPad của Dương Vũ", peer_id: "p3", kind: "files", file_count: 1, total_bytes: 482133, status: "rejected", error: "Máy nhận đã từ chối",
      dir_path: null, files: [{ name: "Báo cáo quý 3.xlsx", path: "/Users/ban/Documents/Báo cáo quý 3.xlsx" }], text: null },
  ];
  function finishToHistory(t, files) {
    const entry = { id: t.id, finished_at: now(), direction: t.direction, peer_name: t.peer_name, peer_id: t.peer_id || null, kind: t.kind, file_count: t.file_count,
      total_bytes: t.total_bytes, status: t.status, error: t.error, dir_path: t.dir_path, files: files || [], text: t.text || null };
    history = [entry, ...history];
    emit("history-added", entry);
  }

  const SIZES = { "Báo cáo quý 3.xlsx": 482133, "Ảnh sự kiện.zip": 73400320, "Hợp đồng 2026.pdf": 1843200, "Ghi chú họp.txt": 3120 };
  const sampleFiles = ["/Users/ban/Documents/Báo cáo quý 3.xlsx", "/Users/ban/Documents/Ảnh sự kiện.zip"];
  const sampleFolder = ["/Users/ban/Documents/Hợp đồng 2026"];
  const baseName = (p) => p.split("/").pop();
  const sizeOf = (p) => (baseName(p).includes(".") ? SIZES[baseName(p)] ?? 1200000 : 24117248);

  // A QR-looking pattern for the pairing and browser dialogs (decorative; it's a demo).
  function fakeQr() {
    const n = 25, c = 8;
    let seed = 7;
    const rnd = () => ((seed = (seed * 9301 + 49297) % 233280) / 233280);
    let cells = "";
    const finder = (x, y) => (x < 7 && y < 7) || (x >= n - 7 && y < 7) || (x < 7 && y >= n - 7);
    for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) {
      if (finder(x, y)) continue;
      if (rnd() < 0.48) cells += `<rect x="${x * c}" y="${y * c}" width="${c}" height="${c}"/>`;
    }
    const eye = (x, y) => `<rect x="${x * c}" y="${y * c}" width="${7 * c}" height="${7 * c}"/><rect x="${(x + 1) * c}" y="${(y + 1) * c}" width="${5 * c}" height="${5 * c}" fill="#fff"/><rect x="${(x + 2) * c}" y="${(y + 2) * c}" width="${3 * c}" height="${3 * c}"/>`;
    return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="-16 -16 ${n * c + 32} ${n * c + 32}" style="width:100%;height:auto;stroke:none"><rect x="-16" y="-16" width="${n * c + 32}" height="${n * c + 32}" fill="#fff"/><g fill="#111">${cells}${eye(0, 0)}${eye(n - 7, 0)}${eye(0, n - 7)}</g></svg>`;
  }
  const QR = fakeQr();

  let web = null; // WebInfo while browser sharing is on
  const webInfo = () => web && { ...web, files: web.files.map((f) => ({ ...f })) };

  // ---- Transfers: wait for "Accept", then stream progress ----
  let tid = 0;
  const live = new Map();
  async function runTransfer(t, acceptAfter) {
    live.set(t.id, t);
    emit("transfer-update", { ...t });
    await wait(acceptAfter);
    if (t.status === "cancelled") return;
    t.status = "transferring";
    const speed = 40e6 + Math.random() * 30e6;
    const step = 120;
    while (t.transferred < t.total_bytes) {
      await wait(step);
      if (t.status === "cancelled") return;
      t.transferred = Math.min(t.total_bytes, t.transferred + speed * (step / 1000) * (t.total_bytes > 2e7 ? 1 : 0.2) + 1);
      t.speed_bps = speed;
      emit("transfer-update", { ...t });
    }
    t.status = "done";
    t.speed_bps = 0;
    emit("transfer-update", { ...t });
    live.delete(t.id);
    finishToHistory(t, t.files);
  }

  // After the visitor's first send finishes, someone sends something back so they can try "Accept".
  let demoIncomingDone = false;
  function scheduleIncoming() {
    if (demoIncomingDone) return;
    demoIncomingDone = true;
    setTimeout(() => {
      emit("incoming-request", {
        request_id: "r1", sender_name: "Laptop Phạm Bá Cháy", sender_os: "windows", file_count: 3, total_bytes: 5300000,
        preview: ["Bảng lương tháng 10.xlsx", "Hoá đơn VAT.pdf", "Ảnh chứng từ.jpg"], timeout_secs: 60, can_trust: true,
      });
    }, 2500);
  }

  // ---- Commands ----
  const commands = {
    get_self_info: () => me,
    get_peers: () => peers,
    get_devices: () => devices(),
    ensure_local_network: () => true,
    describe_paths: ({ paths }) => paths.map((p) => ({ path: p, name: baseName(p), is_dir: !baseName(p).includes("."), size: baseName(p).includes(".") ? sizeOf(p) : null })),
    async send_files({ peerId, paths }) {
      const p = peers.find((x) => x.id === peerId);
      const t = { id: "t" + ++tid, direction: "send", peer_name: p.name, peer_id: p.id, kind: "files", file_count: paths.length + (paths.some((x) => !baseName(x).includes(".")) ? 11 : 0),
        total_bytes: paths.reduce((s, x) => s + sizeOf(x), 0), transferred: 0, speed_bps: 0, status: "waiting", error: null, dir_path: null,
        files: paths.map((x) => ({ name: baseName(x), path: x })) };
      // Máy tin cậy nhận luôn, không cần chờ bấm Chấp nhận.
      const isTrusted = trusted.some((d) => d.id === p.id);
      runTransfer(t, isTrusted ? 200 : 1200 + Math.random() * 900).then(() => { if (t.status === "done") scheduleIncoming(); });
      return t.id;
    },
    async send_text({ peerId, text }) {
      await wait(350);
      const p = peers.find((x) => x.id === peerId);
      finishToHistory({ id: "t" + ++tid, direction: "send", peer_name: p.name, peer_id: p.id, kind: "text", file_count: 0, total_bytes: text.length, status: "done", error: null, dir_path: null, text });
      setTimeout(scheduleIncoming, 1500);
      return null;
    },
    cancel_transfer({ id }) {
      const t = live.get(id);
      if (t) { t.status = "cancelled"; emit("transfer-update", { ...t }); live.delete(id); }
    },
    respond_request({ requestId, accept, trust }) {
      emit("request-closed", { request_id: requestId });
      if (accept && trust && !trusted.some((d) => d.id === "p1")) {
        trusted = [...trusted, { id: "p1", name: "Laptop Phạm Bá Cháy", os: "windows", fp: "7f3ca1e0" }];
        setTimeout(() => emit("devices-changed", devices()), 0);
      }
      const names = ["Bảng lương tháng 10.xlsx", "Hoá đơn VAT.pdf", "Ảnh chứng từ.jpg"];
      if (accept) runTransfer({ id: "t" + ++tid, direction: "receive", peer_name: "Laptop Phạm Bá Cháy", peer_id: "p1", kind: "files", file_count: 3, total_bytes: 5300000,
        transferred: 0, speed_bps: 0, status: "waiting", error: null, dir_path: "~/Downloads/BetterDrop",
        files: names.map((n) => ({ name: n, path: "~/Downloads/BetterDrop/" + n })) }, 300);
    },
    async share_group() { await wait(900); return []; },
    take_shared_items: () => ({ paths: [], text: null }),
    check_update: () => null,
    install_update: () => null,
    untrust_device({ id }) { trusted = trusted.filter((d) => d.id !== id); return devices(); },
    get_history: () => history,
    delete_history_item({ id }) { history = history.filter((h) => h.id !== id); return null; },
    clear_history() { history = []; return null; },
    reveal_path: () => { throw "Đây là bản dùng thử trên web, không có file thật để mở."; },
    async resend_history({ id, peerId }) {
      const h = history.find((x) => x.id === id);
      if (!h) return null;
      if (h.kind === "text") return commands.send_text({ peerId, text: h.text });
      return commands.send_files({ peerId, paths: h.files.map((f) => f.path) });
    },
    open_folder: () => { throw "Đây là bản dùng thử trên web, không có thư mục thật để mở."; },
    pair_info: () => ({ link: "betterdrop://pair?h=192.168.1.12&p=53318&fp=520d9a1b", svg: QR }),
    async add_manual_peer({ addr }) {
      await wait(500);
      const ip = addr.replace(/:\d+$/, "");
      if (!/^\d+\.\d+\.\d+\.\d+$/.test(ip)) throw "Địa chỉ không hợp lệ. Ví dụ: 192.168.1.20";
      const p = { ...peer("m" + Date.now(), "Máy " + ip, "linux", "ab12cd34", "0.3.3", ip), manual: true };
      peers = [...peers, p];
      setTimeout(() => emit("peers-changed", peers), 0);
      return p;
    },
    save_group({ id, name, members }) {
      name = String(name || "").trim();
      if (!name) throw "Hãy đặt tên nhóm";
      if (id) groups = groups.map((g) => (g.id === id ? { ...g, name, members } : g));
      else groups = [...groups, { id: "g" + Date.now(), name, members }];
      return devices();
    },
    delete_group({ id }) { groups = groups.filter((g) => g.id !== id); return devices(); },
    forget_peer({ id }) {
      const p = peers.find((x) => x.id === id);
      if (p) hidden = [...hidden, { id: p.id, name: p.name, os: p.os }];
      return devices();
    },
    unhide_peer({ id }) { hidden = hidden.filter((h) => h.id !== id); return devices(); },
    update_settings({ name, downloadDir }) { me.settings.name = me.info.name = name || me.info.name; me.settings.download_dir = downloadDir; return me; },
    set_receive_enabled({ enabled }) { me.settings.receive_enabled = enabled; return me; },
    set_autostart({ enabled }) { me.autostart = enabled; return me; },
    set_hide_dock({ hide }) { me.settings.hide_dock = hide; return me; },
    set_language({ language }) {
      // Bản demo đổi ngôn ngữ bằng tham số ?lang= trên URL.
      const u = new URL(location.href);
      u.searchParams.set("lang", language === "en" ? "en" : "vi");
      me.settings.language = language;
      setTimeout(() => location.replace(u), 0);
      return me;
    },

    // Gửi qua Internet: giả lập tải lên (không có gì rời khỏi trình duyệt).
    online_providers: () => [
      { id: "litterbox", name: "Litterbox", max_bytes: 1e9, days: 3, app_download: true, browser_download: true },
      { id: "storage-to", name: "storage.to", max_bytes: 25e9, days: 7, app_download: false, browser_download: false },
      { id: "temp-sh", name: "temp.sh", max_bytes: 4e9, days: 3, app_download: true, browser_download: false },
    ],
    online_send({ paths, provider }) {
      const name = { "temp-sh": "temp.sh", litterbox: "Litterbox" }[provider] || "storage.to";
      const days = provider === "storage-to" ? 7 : 3;
      const t = { id: "t" + ++tid, direction: "send", peer_name: name, kind: "files", file_count: paths.length,
        total_bytes: paths.reduce((s, x) => s + sizeOf(x), 0), transferred: 0, speed_bps: 0, status: "waiting", error: null, dir_path: null,
        files: paths.map((x) => ({ name: baseName(x), path: x })) };
      const link = `https://gmttuts.github.io/better-drop/nhan/#1.${{ "temp-sh": "ts.dEmO1/betterdrop.bdrop", litterbox: "lb.dEmO12.bdrop" }[provider] || "st.DEMOdemo1"}.Wb3lYv0Yx2Qd5c8m1nH4pK7rT9uE6sA0zB3gJ5kL8oM`;
      const expires_at = now() + days * 86400;
      live.set(t.id, t);
      (async () => {
        emit("transfer-update", { ...t });
        await wait(300);
        t.status = "transferring";
        while (t.transferred < t.total_bytes) {
          await wait(120);
          if (t.status === "cancelled") return;
          t.speed_bps = 12e6;
          t.transferred = Math.min(t.total_bytes, t.transferred + t.speed_bps * 0.12 * (t.total_bytes > 2e7 ? 3 : 0.3) + 1);
          emit("transfer-update", { ...t });
        }
        t.status = "done";
        emit("transfer-update", { ...t });
        live.delete(t.id);
        emit("online-done", { transfer_id: t.id, link, provider, provider_name: name, page_url: "", expires_at, qr: QR, file_count: paths.length, name: baseName(paths[0]) });
        const entry = { id: t.id, finished_at: now(), direction: "send", peer_name: name, peer_id: null, kind: "files", file_count: t.file_count, total_bytes: t.total_bytes,
          status: "done", error: null, dir_path: null, files: t.files, text: null, online: { url: link, provider: name, expires_at } };
        history = [entry, ...history];
        emit("history-added", entry);
      })();
      return t.id;
    },
    online_receive_link: ({ link }) => {
      if (!/\/nhan\/#1\.(st|ts|lb)\./.test(link)) throw "Link BetterDrop không hợp lệ (thiếu hoặc sai phần sau dấu #)";
      if (!link.includes("#1.st.")) throw "Bản dùng thử trên web không tải được file thật.";
      return { transfer_id: null, provider_name: "storage.to", page_url: "" };
    },
    online_receive_file: () => { throw "Bản dùng thử trên web không đọc được file trên máy bạn."; },
    online_open_page: () => null,
    online_qr: () => QR,

    web_share_status: () => webInfo(),
    web_share_start() { web = { pin: "4821", urls: ["http://192.168.1.12:53319"], qr_svg: QR, files: [], text: null, clients: 0 }; setTimeout(() => { if (web) { web.clients = 1; emit("web-changed", null); } }, 4000); return webInfo(); },
    web_share_stop() { web = null; return null; },
    web_share_new_pin() { web.pin = String(1000 + Math.floor(Math.random() * 9000)); return webInfo(); },
    web_share_add_files({ paths }) { for (const p of paths) web.files.push({ id: "f" + web.files.length, rel_path: baseName(p), size: sizeOf(p) }); return webInfo(); },
    web_share_set_files({ paths }) { web.files = paths.map((p, i) => ({ id: "f" + i, rel_path: baseName(p), size: sizeOf(p) })); return webInfo(); },
    web_share_set_text({ text }) { web.text = text || null; return webInfo(); },

    // Plugins
    "plugin:event|listen": ({ event, handler }) => { (handlers[event] = handlers[event] || []).push(handler); return handler; },
    "plugin:event|unlisten": () => null,
    "plugin:dialog|open": ({ options }) => (options && options.directory ? sampleFolder : sampleFiles),
    "plugin:clipboard-manager|read_text": () => "https://gmttuts.github.io/better-drop/",
    "plugin:clipboard-manager|write_text": () => null,
    "plugin:barcode-scanner|checkPermissions": () => ({ camera: "granted" }),
    "plugin:barcode-scanner|requestPermissions": () => ({ camera: "granted" }),
    "plugin:barcode-scanner|scan": () => { throw "Bản dùng thử trên web không mở được camera. Trong app thật, bạn quét mã QR trên máy tính để ghép nối."; },
    "plugin:barcode-scanner|cancel": () => null,
  };

  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
    transformCallback: (fn, once) => {
      const id = nextCb++;
      window["_" + id] = (...a) => { if (once) delete window["_" + id]; return fn && fn(...a); };
      return id;
    },
    unregisterCallback: (id) => { delete window["_" + id]; },
    convertFileSrc: (s) => s,
    invoke: async (cmd, args = {}) => {
      const fn = commands[cmd];
      if (!fn) return null;
      return fn(args);
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
})();
