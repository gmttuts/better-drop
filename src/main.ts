import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";

// ---- Kiểu dữ liệu (khớp với src-tauri/src/models.rs) ----

interface DeviceInfo {
  id: string;
  name: string;
  os: string;
  fp: string;
  port: number;
}
interface Peer extends DeviceInfo {
  addrs: string[];
  manual: boolean;
}
interface Settings {
  device_id: string;
  name: string;
  download_dir: string;
}
interface SelfInfo {
  info: DeviceInfo;
  settings: Settings;
  addrs: string[];
}
type Status = "waiting" | "transferring" | "done" | "rejected" | "cancelled" | "error";
interface TransferUpdate {
  id: string;
  direction: "send" | "receive";
  peer_name: string;
  kind: "files" | "text";
  file_count: number;
  total_bytes: number;
  transferred: number;
  speed_bps: number;
  status: Status;
  error: string | null;
  dir_path: string | null;
}
interface IncomingRequest {
  request_id: string;
  sender_name: string;
  sender_os: string;
  file_count: number;
  total_bytes: number;
  preview: string[];
  timeout_secs: number;
}
interface IncomingText {
  sender_name: string;
  text: string;
}

// ---- Tiện ích ----

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<HTMLElementTagNameMap[K]> & { className?: string } = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children);
  return node;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[i]}`;
}

const shortCode = (fp: string) => fp.slice(0, 4).toUpperCase();
const osLabel = (os: string) => (os === "mac" ? "macOS" : os === "windows" ? "Windows" : os || "Không rõ");
const baseName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

function toast(message: string, kind: "info" | "error" = "info") {
  const t = el("div", { className: `toast ${kind}`, textContent: message });
  $("toasts").append(t);
  setTimeout(() => t.classList.add("leaving"), 4000);
  setTimeout(() => t.remove(), 4400);
}

const ICON_LAPTOP =
  '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 6a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v9H4zM2 18h20"/></svg>';
const ICON_DESKTOP =
  '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 5a1 1 0 0 1 1-1h16a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1zM9 20h6M12 16v4"/></svg>';
const osIcon = (os: string) => (os === "mac" ? ICON_LAPTOP : ICON_DESKTOP);

// ---- Trạng thái giao diện ----

let me: SelfInfo | null = null;
let peers: Peer[] = [];
let staged: string[] = [];
let activeTab: "files" | "text" = "files";
const transfers = new Map<string, TransferUpdate>();
const busyPeers = new Set<string>();

// ---- Máy này ----

function renderMe() {
  if (!me) return;
  $("me-name").textContent = me.info.name;
  $("me-code").textContent = shortCode(me.info.fp);
}

// ---- Tab & nội dung cần gửi ----

function setTab(tab: "files" | "text") {
  activeTab = tab;
  document.querySelectorAll<HTMLButtonElement>(".tab").forEach((b) => {
    b.classList.toggle("active", b.dataset.tab === tab);
    b.setAttribute("aria-selected", String(b.dataset.tab === tab));
  });
  $("panel-files").hidden = tab !== "files";
  $("panel-text").hidden = tab !== "text";
  renderPeers();
}

function addStaged(paths: string[]) {
  for (const p of paths) if (!staged.includes(p)) staged.push(p);
  setTab("files");
  renderStaged();
}

function renderStaged() {
  const list = $("staged");
  list.replaceChildren(
    ...staged.map((p) => {
      const remove = el("button", { className: "icon-btn small", title: "Bỏ", textContent: "×" });
      remove.addEventListener("click", () => {
        staged = staged.filter((s) => s !== p);
        renderStaged();
      });
      return el("li", { title: p }, el("span", { className: "staged-name", textContent: baseName(p) }), remove);
    }),
  );
  $("staged-foot").hidden = staged.length === 0;
  $("staged-count").textContent = `${staged.length} mục đã chọn`;
  $("dropzone").classList.toggle("compact", staged.length > 0);
  renderPeers();
}

function hasContent(): boolean {
  return activeTab === "files" ? staged.length > 0 : $<HTMLTextAreaElement>("text-input").value.trim() !== "";
}

// ---- Danh sách máy ----

function renderPeers() {
  const list = $("peers");
  const ready = hasContent();
  list.replaceChildren(
    ...peers.map((p) => {
      const busy = busyPeers.has(p.id);
      const btn = el("button", {
        className: "btn primary",
        textContent: busy ? "Đang gửi…" : "Gửi",
        disabled: !ready || busy,
        title: ready ? `Gửi tới ${p.name}` : "Chọn file hoặc nhập văn bản trước",
      });
      btn.addEventListener("click", () => sendTo(p));
      const icon = el("span", { className: "peer-icon" });
      icon.innerHTML = osIcon(p.os);
      return el(
        "li",
        { className: "peer" },
        icon,
        el(
          "div",
          { className: "peer-text" },
          el("div", { className: "peer-name", textContent: p.name }),
          el(
            "div",
            { className: "muted small" },
            `${osLabel(p.os)}${p.manual ? " · thêm thủ công" : ""} · mã `,
            el("span", { className: "code", textContent: shortCode(p.fp) }),
          ),
        ),
        btn,
      );
    }),
  );
  $("peers-empty").hidden = peers.length > 0;
}

async function sendTo(peer: Peer) {
  busyPeers.add(peer.id);
  renderPeers();
  try {
    if (activeTab === "files") {
      const paths = [...staged];
      await invoke<string>("send_files", { peerId: peer.id, paths });
      staged = [];
      renderStaged();
    } else {
      const input = $<HTMLTextAreaElement>("text-input");
      await invoke("send_text", { peerId: peer.id, text: input.value });
      input.value = "";
      toast(`Đã gửi văn bản tới ${peer.name}`);
    }
  } catch (e) {
    toast(errorText(e), "error");
  } finally {
    busyPeers.delete(peer.id);
    renderPeers();
  }
}

// ---- Hoạt động ----

const STATUS_TEXT: Record<Status, string> = {
  waiting: "Đang chờ máy nhận chấp nhận…",
  transferring: "Đang chuyển",
  done: "Hoàn tất",
  rejected: "Bị từ chối",
  cancelled: "Đã huỷ",
  error: "Lỗi",
};

const transferNodes = new Map<string, HTMLLIElement>();

function progressOf(t: TransferUpdate): number {
  if (t.total_bytes > 0) return Math.min(100, (t.transferred / t.total_bytes) * 100);
  return t.status === "done" ? 100 : 0;
}

function detailOf(t: TransferUpdate): string {
  if (t.status === "transferring") {
    const pct = progressOf(t);
    let d = `${pct.toFixed(0)}% · ${formatBytes(t.transferred)} / ${formatBytes(t.total_bytes)}`;
    if (t.speed_bps > 0) d += ` · ${formatBytes(t.speed_bps)}/s`;
    return d;
  }
  return t.error && t.status !== "done" ? t.error : STATUS_TEXT[t.status];
}

/// Cập nhật một dòng: nếu chỉ tiến trình thay đổi thì sửa tại chỗ (để nút Huỷ
/// không bị dựng lại liên tục), còn đổi trạng thái thì dựng lại dòng đó.
function upsertTransfer(t: TransferUpdate, prev: TransferUpdate | undefined) {
  const existing = transferNodes.get(t.id);
  if (existing && prev?.status === t.status) {
    const bar = existing.querySelector<HTMLElement>(".bar > i");
    if (bar) bar.style.width = `${progressOf(t)}%`;
    existing.querySelector(".t-detail")!.textContent = detailOf(t);
    return;
  }
  const node = renderTransfer(t);
  if (existing) existing.replaceWith(node);
  else $("transfers").prepend(node);
  transferNodes.set(t.id, node);
  $("transfers-empty").hidden = true;
}

function renderTransfer(t: TransferUpdate): HTMLLIElement {
  const arrow = t.direction === "send" ? "↑" : "↓";
  const who = t.direction === "send" ? `Tới ${t.peer_name}` : `Từ ${t.peer_name}`;
  const what = t.kind === "text" ? "Văn bản" : `${t.file_count} file · ${formatBytes(t.total_bytes)}`;
  const actions = el("div", { className: "t-actions" });
  if (t.status === "waiting" || t.status === "transferring") {
    const b = el("button", { className: "btn small", textContent: "Huỷ" });
    b.addEventListener("click", () => invoke("cancel_transfer", { id: t.id }));
    actions.append(b);
  }
  if (t.status === "done" && t.direction === "receive" && t.dir_path) {
    const b = el("button", { className: "btn small", textContent: "Mở thư mục" });
    b.addEventListener("click", () =>
      invoke("open_folder", { path: t.dir_path }).catch((e) => toast(errorText(e), "error")),
    );
    actions.append(b);
  }

  const showBar = t.kind === "files" && (t.status === "transferring" || t.status === "done");
  return el(
    "li",
    { className: `transfer ${t.status}` },
    el("span", { className: `t-dir ${t.direction}`, textContent: arrow }),
    el(
      "div",
      { className: "t-body" },
      el("div", { className: "t-title" }, el("strong", { textContent: who }), ` · ${what}`),
      showBar ? progressBar(progressOf(t)) : "",
      el("div", { className: "muted small t-detail", textContent: detailOf(t) }),
    ),
    actions,
  );
}

function progressBar(pct: number): HTMLElement {
  const fill = el("i");
  fill.style.width = `${pct}%`;
  return el("div", { className: "bar" }, fill);
}

// ---- Hộp thoại nhận ----

const requestQueue: IncomingRequest[] = [];
let currentRequest: IncomingRequest | null = null;
let requestTimer: number | undefined;

function showNextRequest() {
  const dlg = $<HTMLDialogElement>("dlg-request");
  currentRequest = requestQueue.shift() ?? null;
  clearInterval(requestTimer);
  if (!currentRequest) {
    if (dlg.open) dlg.close();
    return;
  }
  const r = currentRequest;
  $("req-sender").textContent = r.sender_name;
  $("req-summary").textContent = `${r.file_count} file · ${formatBytes(r.total_bytes)} · từ máy ${osLabel(r.sender_os)}`;
  const more = r.file_count - r.preview.length;
  $("req-preview").replaceChildren(
    ...r.preview.map((p) => el("li", { textContent: p })),
    ...(more > 0 ? [el("li", { className: "muted", textContent: `và ${more} file khác…` })] : []),
  );
  const bar = $("req-bar");
  const started = Date.now();
  const tick = () => {
    const left = Math.max(0, 1 - (Date.now() - started) / (r.timeout_secs * 1000));
    bar.style.width = `${left * 100}%`;
  };
  tick();
  requestTimer = window.setInterval(tick, 250);
  if (!dlg.open) dlg.showModal();
}

async function answerRequest(accept: boolean) {
  if (!currentRequest) return;
  const id = currentRequest.request_id;
  currentRequest = null;
  await invoke("respond_request", { requestId: id, accept });
  showNextRequest();
}

// ---- Cài đặt ----

function openSettings() {
  if (!me) return;
  $<HTMLInputElement>("set-name").value = me.settings.name;
  $<HTMLInputElement>("set-dir").value = me.settings.download_dir;
  $("set-code").textContent = shortCode(me.info.fp);
  $("set-addrs").replaceChildren(
    ...(me.addrs.length ? me.addrs : ["(không có mạng)"]).map((a) => el("li", { className: "code", textContent: a })),
  );
  $<HTMLDialogElement>("dlg-settings").showModal();
}

// ---- Khởi động ----

async function init() {
  // Tab
  document.querySelectorAll<HTMLButtonElement>(".tab").forEach((b) =>
    b.addEventListener("click", () => setTab(b.dataset.tab as "files" | "text")),
  );
  $("text-input").addEventListener("input", renderPeers);

  // Chọn file / thư mục
  $("btn-pick-files").addEventListener("click", async () => {
    const r = await open({ multiple: true, directory: false });
    if (r) addStaged(Array.isArray(r) ? r : [r]);
  });
  $("btn-pick-folder").addEventListener("click", async () => {
    const r = await open({ multiple: true, directory: true });
    if (r) addStaged(Array.isArray(r) ? r : [r]);
  });
  $("btn-clear").addEventListener("click", () => {
    staged = [];
    renderStaged();
  });
  $("btn-paste").addEventListener("click", async () => {
    try {
      const t = await readText();
      if (t) {
        $<HTMLTextAreaElement>("text-input").value = t;
        renderPeers();
      }
    } catch {
      toast("Clipboard không có văn bản", "error");
    }
  });

  // Kéo thả trên toàn cửa sổ
  await getCurrentWebview().onDragDropEvent((e) => {
    const dz = $("dropzone");
    if (e.payload.type === "enter" || e.payload.type === "over") dz.classList.add("hover");
    else dz.classList.remove("hover");
    if (e.payload.type === "drop" && e.payload.paths.length) addStaged(e.payload.paths);
  });

  // Thêm máy thủ công
  $("manual-form").addEventListener("submit", async (ev) => {
    ev.preventDefault();
    const input = $<HTMLInputElement>("manual-addr");
    const addr = input.value.trim();
    if (!addr) return;
    const btn = (ev.target as HTMLFormElement).querySelector("button")!;
    btn.disabled = true;
    try {
      const p = await invoke<Peer>("add_manual_peer", { addr });
      input.value = "";
      toast(`Đã thêm ${p.name} — mã ${shortCode(p.fp)}`);
      ($("manual") as HTMLDetailsElement).open = false;
    } catch (e) {
      toast(errorText(e), "error");
    } finally {
      btn.disabled = false;
    }
  });

  // Hộp thoại nhận
  $("btn-accept").addEventListener("click", () => answerRequest(true));
  $("btn-reject").addEventListener("click", () => answerRequest(false));
  $<HTMLDialogElement>("dlg-request").addEventListener("cancel", (e) => {
    e.preventDefault();
    answerRequest(false);
  });
  $("btn-text-close").addEventListener("click", () => $<HTMLDialogElement>("dlg-text").close());
  $("btn-text-copy").addEventListener("click", async () => {
    await writeText($("txt-body").textContent ?? "");
    toast("Đã sao chép vào clipboard");
    $<HTMLDialogElement>("dlg-text").close();
  });

  // Cài đặt
  $("btn-settings").addEventListener("click", openSettings);
  $("me").addEventListener("click", openSettings);
  $("btn-settings-cancel").addEventListener("click", () => $<HTMLDialogElement>("dlg-settings").close());
  $("btn-choose-dir").addEventListener("click", async () => {
    const r = await open({ directory: true, multiple: false });
    if (typeof r === "string") $<HTMLInputElement>("set-dir").value = r;
  });
  $("settings-form").addEventListener("submit", async (ev) => {
    ev.preventDefault();
    try {
      me = await invoke<SelfInfo>("update_settings", {
        name: $<HTMLInputElement>("set-name").value,
        downloadDir: $<HTMLInputElement>("set-dir").value,
      });
      renderMe();
      $<HTMLDialogElement>("dlg-settings").close();
      toast("Đã lưu cài đặt");
    } catch (e) {
      toast(errorText(e), "error");
    }
  });

  // Sự kiện từ Rust
  await listen<Peer[]>("peers-changed", (e) => {
    peers = e.payload;
    renderPeers();
  });
  await listen<TransferUpdate>("transfer-update", (e) => {
    const prev = transfers.get(e.payload.id);
    transfers.set(e.payload.id, e.payload);
    upsertTransfer(e.payload, prev);
    if (prev?.status !== e.payload.status) {
      const t = e.payload;
      if (t.status === "done" && t.kind === "files")
        toast(t.direction === "send" ? `Đã gửi xong tới ${t.peer_name}` : `Đã nhận ${t.file_count} file từ ${t.peer_name}`);
      if (t.status === "rejected") toast(`${t.peer_name} đã từ chối`, "error");
      if (t.status === "error") toast(t.error ?? "Lỗi khi chuyển", "error");
    }
  });
  await listen<IncomingRequest>("incoming-request", (e) => {
    requestQueue.push(e.payload);
    if (!currentRequest) showNextRequest();
  });
  await listen<{ request_id: string }>("request-closed", (e) => {
    const id = e.payload.request_id;
    const i = requestQueue.findIndex((r) => r.request_id === id);
    if (i >= 0) requestQueue.splice(i, 1);
    if (currentRequest?.request_id === id) {
      currentRequest = null;
      showNextRequest();
    }
  });
  await listen<IncomingText>("incoming-text", (e) => {
    $("txt-sender").textContent = e.payload.sender_name;
    $("txt-body").textContent = e.payload.text;
    const dlg = $<HTMLDialogElement>("dlg-text");
    if (!dlg.open) dlg.showModal();
  });
  await listen<string>("fatal-error", (e) => toast(e.payload, "error"));

  me = await invoke<SelfInfo>("get_self_info");
  renderMe();
  peers = await invoke<Peer[]>("get_peers");
  renderPeers();
  renderStaged();
}

init().catch((e) => toast(errorText(e), "error"));
