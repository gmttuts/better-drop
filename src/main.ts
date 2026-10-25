import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { cancel as cancelScan, checkPermissions, Format, requestPermissions, scan } from "@tauri-apps/plugin-barcode-scanner";
import { onBackButtonPress } from "@tauri-apps/api/app";
import { resolveLanguage, startEnglish } from "./i18n";

// ---- Kiểu dữ liệu (khớp với src-tauri/src/models.rs) ----

interface DeviceInfo {
  id: string;
  name: string;
  os: string;
  fp: string;
  port: number;
  /** Phiên bản app; trống với bản trước 0.3.2. */
  version?: string;
}
interface Peer extends DeviceInfo {
  addrs: string[];
  manual: boolean;
  online: boolean;
}
interface Settings {
  device_id: string;
  name: string;
  download_dir: string;
  receive_enabled: boolean;
  hide_dock: boolean;
  language?: string;
  background_receive?: boolean;
  hotkeys?: Hotkeys;
}
interface Hotkeys {
  enabled: boolean;
  send_clipboard: string;
  open_app: string;
}
interface SelfInfo {
  info: DeviceInfo;
  settings: Settings;
  addrs: string[];
  autostart: boolean;
  mobile: boolean;
}
interface StagedItem {
  path: string;
  name: string;
  is_dir: boolean;
  size: number | null;
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
  /** Ghi chú tạm thời, vd "Mất kết nối, đang thử lại (lần 1)…". */
  note?: string;
}
interface IncomingRequest {
  request_id: string;
  sender_name: string;
  sender_os: string;
  file_count: number;
  total_bytes: number;
  preview: string[];
  timeout_secs: number;
  /** Máy gửi đã chứng minh danh tính: có thể "Luôn chấp nhận". */
  can_trust?: boolean;
  /** Yêu cầu chia sẻ nhóm: tên nhóm (file_count = số máy, preview = tên máy). */
  group_name?: string;
  /** Vài ảnh/video đầu để xem trước (máy gửi bản cũ không gửi ảnh thu nhỏ). */
  media?: MediaPreview[];
  /** Số ảnh/video còn lại. */
  media_more?: number;
}
interface MediaPreview {
  name: string;
  video: boolean;
  /** data:image/jpeg;base64,… */
  thumb?: string;
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
  if (n < 1024) return `${Math.round(n)} B`;
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

/** So sánh "0.3.1" với "0.3.2": âm nếu a cũ hơn b. */
function compareVersion(a: string, b: string): number {
  const pa = a.split(".").map((n) => parseInt(n, 10) || 0);
  const pb = b.split(".").map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d) return d;
  }
  return 0;
}

/** Nhãn phiên bản của máy khác; báo khi máy đó cũ hơn máy này (vẫn gửi nhận được). */
function versionNote(v: string | undefined): { text: string; old: boolean } {
  const mine = me?.info.version ?? "";
  if (!v) return { text: "bản cũ (trước 0.3.2)", old: true };
  return { text: `v${v}`, old: !!mine && compareVersion(v, mine) < 0 };
}
const OS_LABELS: Record<string, string> = {
  mac: "macOS",
  windows: "Windows",
  linux: "Linux",
  android: "Android",
  ios: "iPhone/iPad",
  web: "trình duyệt",
};
const osLabel = (os: string) => OS_LABELS[os] ?? (os || "Không rõ");
const baseName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

function toast(message: string, kind: "info" | "error" = "info") {
  const t = el("div", { className: `toast ${kind}`, textContent: message });
  $("toasts").append(t);
  // Thông báo dài (hướng dẫn sửa lỗi) cần thời gian đọc.
  const ms = Math.min(15000, Math.max(4000, message.length * 60));
  setTimeout(() => t.classList.add("leaving"), ms);
  setTimeout(() => t.remove(), ms + 400);
}

const ICON_LAPTOP =
  '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 6a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v9H4zM2 18h20"/></svg>';
const ICON_DESKTOP =
  '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 5a1 1 0 0 1 1-1h16a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1zM9 20h6M12 16v4"/></svg>';
const ICON_PHONE =
  '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 3a1 1 0 0 1 1-1h8a1 1 0 0 1 1 1v18a1 1 0 0 1-1 1H8a1 1 0 0 1-1-1zM11 18h2"/></svg>';
const osIcon = (os: string) =>
  os === "android" || os === "ios" ? ICON_PHONE : os === "mac" ? ICON_LAPTOP : ICON_DESKTOP;

// ---- Trạng thái giao diện ----

let me: SelfInfo | null = null;
let peers: Peer[] = [];
let staged: StagedItem[] = [];
let activeTab: "files" | "text" = "files";
const transfers = new Map<string, TransferUpdate>();
const busyPeers = new Set<string>();

// ---- Máy này ----

function renderMe() {
  if (!me) return;
  $("me-name").textContent = me.info.name;
  $("me-code").textContent = shortCode(me.info.fp);
  $("paused-chip").hidden = me.settings.receive_enabled;
  $<HTMLInputElement>("set-receive").checked = me.settings.receive_enabled;
  $<HTMLInputElement>("set-autostart").checked = me.autostart;
  $<HTMLInputElement>("set-dock").checked = me.settings.hide_dock;
  $<HTMLInputElement>("set-background").checked = me.settings.background_receive !== false;
  renderHotkeys();
}

/// Các ô tích ở mục "Chạy nền" có hiệu lực ngay, không cần bấm Lưu.
function bindToggle(id: string, command: string, arg: string) {
  const box = $<HTMLInputElement>(id);
  box.addEventListener("change", async () => {
    box.disabled = true;
    try {
      me = await invoke<SelfInfo>(command, { [arg]: box.checked });
      renderMe();
    } catch (e) {
      box.checked = !box.checked;
      toast(errorText(e), "error");
    } finally {
      box.disabled = false;
    }
  });
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

async function addStaged(paths: string[]) {
  const fresh = paths.filter((p) => !staged.some((s) => s.path === p));
  if (!fresh.length) return;
  setTab("files");
  try {
    staged.push(...(await invoke<StagedItem[]>("describe_paths", { paths: fresh })));
  } catch {
    staged.push(...fresh.map((p) => ({ path: p, name: baseName(p), is_dir: false, size: null })));
  }
  renderStaged();
}

function renderStaged() {
  const list = $("staged");
  list.replaceChildren(
    ...staged.map((item) => {
      const remove = el("button", { className: "icon-btn small", title: "Bỏ", textContent: "×" });
      remove.addEventListener("click", () => {
        staged = staged.filter((s) => s !== item);
        renderStaged();
      });
      const size = item.is_dir ? "thư mục" : item.size != null ? formatBytes(item.size) : "";
      return el(
        "li",
        { title: item.path },
        el("span", { className: "staged-name", textContent: item.name }),
        el("span", { className: "staged-size", textContent: size }),
        remove,
      );
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

// ---- Danh sách máy, chọn nhiều, nhóm ----

interface DeviceRef {
  id: string;
  name: string;
  os: string;
}
interface Group {
  id: string;
  name: string;
  members: DeviceRef[];
}

interface TrustedDevice {
  id: string;
  name: string;
  os: string;
  fp: string;
}

let groups: Group[] = [];
let trusted: TrustedDevice[] = [];
let hiddenDevices: DeviceRef[] = [];
/** Nhóm đang lọc; null = tất cả máy. */
let activeGroup: string | null = null;
const selected = new Set<string>();

const ICON_FORGET =
  '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13"/></svg>';

const currentGroup = () => groups.find((g) => g.id === activeGroup) ?? null;
const peerById = (id: string) => peers.find((p) => p.id === id);
const isOnline = (id: string) => peerById(id)?.online ?? false;

/** Các máy hiển thị: tất cả máy, hoặc thành viên nhóm đang chọn (kể cả máy đang tắt). */
function visibleDevices(): (DeviceRef & { peer?: Peer })[] {
  const g = currentGroup();
  if (!g) return peers.map((p) => ({ id: p.id, name: p.name, os: p.os, peer: p }));
  return g.members.map((m) => {
    const p = peerById(m.id);
    return p ? { id: p.id, name: p.name, os: p.os, peer: p } : m;
  });
}

function confirmDialog(title: string, body: string, yes = "Đồng ý"): Promise<boolean> {
  const dlg = $<HTMLDialogElement>("dlg-confirm");
  $("confirm-title").textContent = title;
  $("confirm-body").textContent = body;
  $("btn-confirm-yes").textContent = yes;
  dlg.showModal();
  return new Promise((resolve) => {
    const done = (v: boolean) => {
      dlg.close();
      $("btn-confirm-yes").removeEventListener("click", ok);
      $("btn-confirm-no").removeEventListener("click", no);
      resolve(v);
    };
    const ok = () => done(true);
    const no = () => done(false);
    $("btn-confirm-yes").addEventListener("click", ok);
    $("btn-confirm-no").addEventListener("click", no);
  });
}

function renderChips() {
  const chips: HTMLElement[] = [];
  const chip = (label: string, active: boolean, onClick: () => void, extra = "") => {
    const b = el("button", { className: `chip${active ? " active" : ""}${extra}`, textContent: label });
    b.addEventListener("click", onClick);
    chips.push(b);
  };
  chip("Tất cả", activeGroup === null, () => setGroup(null));
  for (const g of groups) {
    const on = g.members.filter((m) => isOnline(m.id)).length;
    chip(`${g.name} · ${on}/${g.members.length}`, activeGroup === g.id, () => setGroup(g.id));
  }
  chip("+ Nhóm mới", false, () => openGroupDialog(null), " add");
  $("group-chips").replaceChildren(...chips);
  $("group-chips").hidden = groups.length === 0 && peers.length < 2;
}

function setGroup(id: string | null) {
  activeGroup = id;
  selected.clear();
  renderPeers();
}

function renderBulkBar() {
  const bar = $("bulk-bar");
  const g = currentGroup();
  const ready = hasContent();
  const actions: HTMLElement[] = [];
  const action = (label: string, cls: string, onClick: () => void, disabled = false) => {
    const b = el("button", { className: `btn small ${cls}`, textContent: label, disabled });
    b.addEventListener("click", onClick);
    actions.push(b);
  };

  if (selected.size > 0) {
    const targets = [...selected].map(peerById).filter((p): p is Peer => !!p && p.online);
    $("bulk-label").textContent = `Đã chọn ${selected.size} máy`;
    action(`Gửi tới ${targets.length} máy`, "primary", () => sendMany(targets), !ready || targets.length === 0);
    action("Lưu thành nhóm", "", () => openGroupDialog(null));
    action("Bỏ chọn", "", () => {
      selected.clear();
      renderPeers();
    });
  } else if (g) {
    const targets = g.members.map((m) => peerById(m.id)).filter((p): p is Peer => !!p && p.online);
    $("bulk-label").textContent = `Nhóm “${g.name}”: ${targets.length}/${g.members.length} máy đang bật`;
    action("Gửi cho cả nhóm", "primary", () => sendMany(targets), !ready || targets.length === 0);
    action("Sửa nhóm", "", () => openGroupDialog(g));
    action("Chia sẻ nhóm", "", () => openShareGroup(g));
    action("Xoá nhóm", "", async () => {
      if (!(await confirmDialog(`Xoá nhóm “${g.name}”?`, "Chỉ xoá nhóm, các máy trong nhóm vẫn giữ nguyên.", "Xoá nhóm"))) return;
      try {
        applyDevices(await invoke("delete_group", { id: g.id }));
        setGroup(null);
      } catch (e) {
        toast(errorText(e), "error");
      }
    });
  }
  bar.hidden = actions.length === 0;
  $("bulk-actions").replaceChildren(...actions);
}

/** Ô "Chọn tất cả": chọn mọi máy đang bật trong danh sách đang xem (cả nhóm hoặc tất cả). */
function renderSelectAll(visible: (DeviceRef & { peer?: Peer })[]) {
  const ids = visible.filter((d) => d.peer?.online).map((d) => d.id);
  const chosen = ids.filter((id) => selected.has(id)).length;
  const box = $<HTMLInputElement>("select-all");
  box.checked = ids.length > 0 && chosen === ids.length;
  box.indeterminate = chosen > 0 && chosen < ids.length;
  const where = currentGroup() ? `trong nhóm “${currentGroup()!.name}”` : "";
  $("select-all-label").textContent = box.checked
    ? `Bỏ chọn tất cả ${where}`.trim()
    : `Chọn tất cả ${ids.length} máy đang bật ${where}`.trim();
  $("select-all-row").hidden = ids.length < 2;
}

function toggleSelectAll() {
  const ids = visibleDevices()
    .filter((d) => d.peer?.online)
    .map((d) => d.id);
  const all = ids.every((id) => selected.has(id));
  if (all) ids.forEach((id) => selected.delete(id));
  else ids.forEach((id) => selected.add(id));
  renderPeers();
}

function renderPeers() {
  // Bỏ chọn máy không còn trong danh sách đang xem.
  const visible = visibleDevices();
  for (const id of [...selected]) if (!visible.some((d) => d.id === id)) selected.delete(id);

  const ready = hasContent();
  $("peers").replaceChildren(
    ...visible.map((d) => {
      const p = d.peer;
      const online = !!p?.online;
      const busy = busyPeers.has(d.id);

      const check = el("input", { type: "checkbox", className: "peer-check", checked: selected.has(d.id), title: "Chọn để gửi nhiều máy" });
      check.setAttribute("aria-label", `Chọn ${d.name}`);
      check.addEventListener("change", () => {
        if (check.checked) selected.add(d.id);
        else selected.delete(d.id);
        renderPeers();
      });

      const send = el("button", {
        className: "btn primary",
        textContent: busy ? "Đang gửi…" : "Gửi",
        disabled: !ready || busy || !online,
        title: !online
          ? "Không kết nối được tới máy này. Hai máy có cùng mạng Wi‑Fi không? Có thể thêm bằng IP hoặc quét mã QR."
          : ready
            ? `Gửi tới ${d.name}`
            : "Chọn file hoặc nhập văn bản trước",
      });
      send.addEventListener("click", () => p && sendMany([p]));

      const forget = el("button", { className: "icon-btn", title: "Quên máy này" });
      forget.innerHTML = ICON_FORGET;
      forget.setAttribute("aria-label", `Quên ${d.name}`);
      forget.addEventListener("click", () => forgetDevice(d));

      const icon = el("span", { className: "peer-icon" });
      icon.innerHTML = osIcon(d.os);
      const meta = el("div", { className: "muted small" });
      if (!online)
        meta.append(el("span", { className: "offline-tag", textContent: p ? "Không kết nối được" : "Đang tắt" }), " · ");
      meta.append(`${osLabel(d.os)}${p?.manual ? " · đã lưu" : ""}`);
      if (p) {
        meta.append(" · mã ", el("span", { className: "code", textContent: shortCode(p.fp) }));
        const vn = versionNote(p.version);
        const tag = el("span", {
          className: vn.old ? "version-tag old" : "version-tag",
          textContent: vn.text,
          title: vn.old
            ? "Máy này dùng phiên bản cũ hơn. Vẫn gửi nhận được, nhưng nên cập nhật để có bản sửa lỗi mới nhất."
            : "Phiên bản BetterDrop của máy này",
        });
        meta.append(" · ", tag);
        if (trusted.some((t) => t.id === p.id && t.fp === p.fp))
          meta.append(" · ", el("span", { className: "trust-tag", textContent: "✓ tin cậy", title: "File từ máy này được nhận luôn, không hỏi" }));
      }

      return el(
        "li",
        { className: `peer${selected.has(d.id) ? " selected" : ""}${online ? "" : " offline"}` },
        check,
        icon,
        el("div", { className: "peer-text" }, el("div", { className: "peer-name", textContent: d.name }), meta),
        send,
        forget,
      );
    }),
  );
  const empty = visible.length === 0;
  $("peers-empty").hidden = !empty || activeGroup !== null;
  renderSelectAll(visible);
  renderChips();
  renderBulkBar();
}

async function forgetDevice(d: DeviceRef) {
  const ok = await confirmDialog(
    `Quên “${d.name}”?`,
    "Máy này sẽ không hiện trong danh sách nữa, kể cả khi nó bật lại trong mạng. Bạn có thể hiện lại trong Cài đặt.",
    "Quên máy",
  );
  if (!ok) return;
  try {
    applyDevices(await invoke("forget_peer", { id: d.id }));
    selected.delete(d.id);
    toast(`Đã quên ${d.name}`);
  } catch (e) {
    toast(errorText(e), "error");
  }
}

function applyDevices(info: { groups: Group[]; hidden: DeviceRef[]; trusted?: TrustedDevice[] }) {
  groups = info.groups;
  hiddenDevices = info.hidden;
  trusted = info.trusted ?? [];
  renderTrusted();
  if (activeGroup && !groups.some((g) => g.id === activeGroup)) activeGroup = null;
  renderPeers();
  renderHidden();
}

/** Gửi nội dung đang chọn tới một hoặc nhiều máy. Mỗi máy nhận tự bấm Chấp nhận. */
async function sendMany(targets: Peer[]) {
  if (!targets.length) return;
  targets.forEach((p) => busyPeers.add(p.id));
  renderPeers();
  const isFiles = activeTab === "files";
  const paths = staged.map((s) => s.path);
  const text = $<HTMLTextAreaElement>("text-input").value;
  const results = await Promise.allSettled(
    targets.map((p) =>
      isFiles ? invoke<string>("send_files", { peerId: p.id, paths }) : invoke("send_text", { peerId: p.id, text }),
    ),
  );
  targets.forEach((p) => busyPeers.delete(p.id));
  const failed = results
    .map((r, i) => (r.status === "rejected" ? `${targets[i].name}: ${errorText(r.reason)}` : null))
    .filter(Boolean);
  if (failed.length < targets.length) {
    if (isFiles) {
      staged = [];
      renderStaged();
    } else {
      $<HTMLTextAreaElement>("text-input").value = "";
      const ok = targets.length - failed.length;
      toast(ok === 1 && targets.length === 1 ? `Đã gửi văn bản tới ${targets[0].name}` : `Đã gửi văn bản tới ${ok} máy`);
    }
    if (targets.length > 1 && isFiles) toast(`Đang gửi tới ${targets.length - failed.length} máy — mỗi máy cần bấm Chấp nhận`);
  }
  for (const f of failed) toast(f as string, "error");
  selected.clear();
  renderPeers();
}

// ---- Chia sẻ nhóm cho máy khác ----

let sharingGroup: Group | null = null;

function openShareGroup(g: Group) {
  sharingGroup = g;
  $("share-group-title").textContent = `Chia sẻ nhóm “${g.name}”`;
  const online = peers.filter((p) => p.online);
  $("share-group-empty").hidden = online.length > 0;
  $("share-group-pick").replaceChildren(
    ...online.map((p) => {
      const box = el("input", { type: "checkbox", value: p.id });
      return el("li", {}, el("label", { className: "check" }, box, `${p.name} `, el("span", { className: "muted small", textContent: osLabel(p.os) })));
    }),
  );
  $<HTMLDialogElement>("dlg-share-group").showModal();
}

async function sendSharedGroup() {
  const g = sharingGroup;
  const ids = [...document.querySelectorAll<HTMLInputElement>("#share-group-pick input:checked")].map((b) => b.value);
  if (!g || ids.length === 0) {
    toast("Chọn ít nhất một máy", "error");
    return;
  }
  $<HTMLDialogElement>("dlg-share-group").close();
  toast(`Đã gửi nhóm “${g.name}” — máy nhận cần bấm Chấp nhận`);
  try {
    const failed = await invoke<string[]>("share_group", { groupId: g.id, peerIds: ids });
    const ok = ids.length - failed.length;
    if (ok > 0) toast(`${ok} máy đã nhận nhóm “${g.name}”`);
    for (const f of failed) toast(f, "error");
  } catch (e) {
    toast(errorText(e), "error");
  }
}

// ---- Hộp thoại nhóm ----

let editingGroup: Group | null = null;

function openGroupDialog(g: Group | null) {
  editingGroup = g;
  $("group-title").textContent = g ? "Sửa nhóm" : "Nhóm mới";
  $<HTMLInputElement>("group-name").value = g?.name ?? "";
  // Ứng viên: mọi máy đang thấy + thành viên cũ (có thể đang tắt).
  const candidates = new Map<string, DeviceRef>();
  for (const p of peers) candidates.set(p.id, { id: p.id, name: p.name, os: p.os });
  for (const m of g?.members ?? []) if (!candidates.has(m.id)) candidates.set(m.id, m);
  const chosen = new Set(g ? g.members.map((m) => m.id) : [...selected]);
  $("group-pick").replaceChildren(
    ...[...candidates.values()].map((d) => {
      const box = el("input", { type: "checkbox", checked: chosen.has(d.id), value: d.id });
      box.dataset.name = d.name;
      box.dataset.os = d.os;
      const label = el("label", { className: "check" }, box, `${d.name} `, el("span", { className: "muted small", textContent: osLabel(d.os) }));
      return el("li", {}, label);
    }),
  );
  $<HTMLDialogElement>("dlg-group").showModal();
  $<HTMLInputElement>("group-name").focus();
}

async function saveGroupFromDialog(ev: Event) {
  ev.preventDefault();
  const members = [...document.querySelectorAll<HTMLInputElement>("#group-pick input:checked")].map((b) => ({
    id: b.value,
    name: b.dataset.name ?? "",
    os: b.dataset.os ?? "",
  }));
  try {
    const info = await invoke<{ groups: Group[]; hidden: DeviceRef[] }>("save_group", {
      id: editingGroup?.id ?? null,
      name: $<HTMLInputElement>("group-name").value,
      members,
    });
    $<HTMLDialogElement>("dlg-group").close();
    const name = $<HTMLInputElement>("group-name").value.trim();
    selected.clear();
    applyDevices(info);
    const g = editingGroup ? groups.find((x) => x.id === editingGroup!.id) : groups[groups.length - 1];
    if (g) setGroup(g.id);
    toast(editingGroup ? `Đã lưu nhóm “${name}”` : `Đã tạo nhóm “${name}”`);
  } catch (e) {
    toast(errorText(e), "error");
  }
}

function renderTrusted() {
  $("trusted-block").hidden = trusted.length === 0;
  $("trusted-list").replaceChildren(
    ...trusted.map((d) => {
      const b = el("button", { className: "btn small", textContent: "Bỏ tin cậy", type: "button" });
      b.addEventListener("click", async () => {
        try {
          applyDevices(await invoke("untrust_device", { id: d.id }));
          toast(`${d.name}: lần sau sẽ hỏi lại trước khi nhận`);
        } catch (e) {
          toast(errorText(e), "error");
        }
      });
      return el(
        "li",
        {},
        el("span", {}, `${d.name} `, el("span", { className: "muted small code", textContent: shortCode(d.fp) })),
        b,
      );
    }),
  );
}

function renderHidden() {
  $("hidden-block").hidden = hiddenDevices.length === 0;
  $("hidden-list").replaceChildren(
    ...hiddenDevices.map((d) => {
      const b = el("button", { className: "btn small", textContent: "Hiện lại", type: "button" });
      b.addEventListener("click", async () => {
        try {
          applyDevices(await invoke("unhide_peer", { id: d.id }));
        } catch (e) {
          toast(errorText(e), "error");
        }
      });
      return el("li", {}, el("span", {}, `${d.name} `, el("span", { className: "muted small", textContent: osLabel(d.os) })), b);
    }),
  );
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
  if (t.status === "transferring" && t.note) return t.note;
  if (t.status === "transferring") {
    const pct = progressOf(t);
    let d = `${pct.toFixed(0)}% · ${formatBytes(t.transferred)} / ${formatBytes(t.total_bytes)}`;
    if (t.speed_bps > 0) d += ` · ${formatBytes(t.speed_bps)}/s`;
    return d;
  }
  if (t.status === "done" && t.direction === "receive" && t.dir_path && me?.mobile) return `Đã lưu vào ${t.dir_path}`;
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
  updateActivityEmpty();
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
  if (t.status === "done" && t.direction === "receive" && t.dir_path && !me?.mobile) {
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

// ---- Lịch sử ----

interface HistoryFile {
  name: string;
  path: string | null;
}
interface HistoryEntry {
  id: string;
  finished_at: number;
  direction: "send" | "receive";
  peer_name: string;
  peer_id: string | null;
  kind: "files" | "text";
  file_count: number;
  total_bytes: number;
  status: Status;
  error: string | null;
  dir_path: string | null;
  files: HistoryFile[];
  text: string | null;
  online?: OnlineLink | null;
  /** File có ảnh thu nhỏ (ảnh lấy bằng lệnh history_thumbs). */
  thumbs?: { name: string }[];
}
interface OnlineLink {
  url: string;
  provider: string;
  expires_at: number;
}

let history: HistoryEntry[] = [];
let historyFilter: "all" | "send" | "receive" = "all";
let historyShown = 30;
const expanded = new Set<string>();

function whenText(secs: number): string {
  const d = new Date(secs * 1000);
  const now = new Date();
  const hm = d.toLocaleTimeString("vi-VN", { hour: "2-digit", minute: "2-digit" });
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((day(now) - day(d)) / 86400000);
  if (diff === 0) return `${hm} hôm nay`;
  if (diff === 1) return `${hm} hôm qua`;
  const pad = (n: number) => String(n).padStart(2, "0");
  const date = `${pad(d.getDate())}/${pad(d.getMonth() + 1)}${d.getFullYear() === now.getFullYear() ? "" : `/${d.getFullYear()}`}`;
  return `${hm} ${date}`;
}

function updateActivityEmpty() {
  const live = $("transfers").children.length;
  $("transfers-empty").hidden = live > 0 || history.length > 0;
  $("hist-tools").hidden = history.length === 0;
}

function renderHistory() {
  const list = history.filter((h) => historyFilter === "all" || h.direction === historyFilter);
  $("history").replaceChildren(...list.slice(0, historyShown).map(renderHistoryEntry));
  $("btn-more-history").hidden = list.length <= historyShown;
  updateActivityEmpty();
}

function renderHistoryEntry(h: HistoryEntry): HTMLLIElement {
  const arrow = h.direction === "send" ? "↑" : "↓";
  const who = h.direction === "send" ? `Tới ${h.peer_name}` : `Từ ${h.peer_name}`;
  const what = h.kind === "text" ? "Văn bản" : `${h.file_count} file · ${formatBytes(h.total_bytes)}`;
  const status = h.status === "done" ? STATUS_TEXT.done : h.error || STATUS_TEXT[h.status];
  const actions = el("div", { className: "t-actions" });
  const stop = (b: HTMLElement) => b.addEventListener("click", (e) => e.stopPropagation());

  if (h.kind === "text" && h.text) {
    const b = el("button", { className: "btn small", textContent: "Sao chép" });
    b.addEventListener("click", async () => {
      await writeText(h.text ?? "");
      toast("Đã sao chép");
    });
    stop(b);
    actions.append(b);
  }
  if (h.online && h.direction === "send" && h.status === "done") {
    const b = el("button", { className: "btn small", textContent: "Sao chép link", title: expiryText(h.online.expires_at) });
    b.addEventListener("click", () => copyText(h.online!.url, "Đã sao chép link"));
    stop(b);
    actions.append(b);
  }
  if (h.direction === "receive" && h.kind === "files" && h.status === "done" && h.dir_path && !me?.mobile) {
    const b = el("button", { className: "btn small", textContent: "Mở thư mục" });
    b.addEventListener("click", () => invoke("open_folder", { path: h.dir_path }).catch((e) => toast(errorText(e), "error")));
    stop(b);
    actions.append(b);
  }
  if (h.direction === "send" && h.peer_id) {
    const target = peers.find((p) => p.id === h.peer_id && p.online);
    const b = el("button", {
      className: "btn small",
      textContent: "Gửi lại",
      disabled: !target,
      title: target ? `Gửi lại tới ${target.name}` : "Máy nhận hiện không kết nối được",
    });
    b.addEventListener("click", async () => {
      try {
        await invoke("resend_history", { id: h.id, peerId: h.peer_id });
        if (h.kind === "text") toast(`Đã gửi lại văn bản tới ${target?.name}`);
      } catch (e) {
        toast(errorText(e), "error");
      }
    });
    stop(b);
    actions.append(b);
  }
  const del = el("button", { className: "icon-btn small", title: "Xoá khỏi lịch sử (không xoá file)", textContent: "×" });
  del.addEventListener("click", async () => {
    await invoke("delete_history_item", { id: h.id });
    history = history.filter((x) => x.id !== h.id);
    thumbCache.delete(h.id);
    renderHistory();
  });
  stop(del);
  actions.append(del);

  const body = el(
    "div",
    { className: "t-body" },
    el("div", { className: "t-title" }, el("strong", { textContent: who }), ` · ${what}`),
    el("div", { className: "muted small t-detail", textContent: `${status} · ${whenText(h.finished_at)}` }),
  );
  if (h.thumbs?.length) body.append(renderHistoryThumbs(h));
  if (expanded.has(h.id)) body.append(renderHistoryDetails(h));

  const li = el("li", { className: `transfer ${h.status}`, title: "Bấm để xem chi tiết" },
    el("span", { className: `t-dir ${h.direction}`, textContent: arrow }), body, actions);
  li.addEventListener("click", () => {
    if (expanded.has(h.id)) expanded.delete(h.id);
    else expanded.add(h.id);
    renderHistory();
  });
  return li;
}

// ---- Ảnh thu nhỏ ----

const MEDIA_EXT = /\.(jpe?g|png|webp|gif|heic|heif|avif|bmp|tiff?|mp4|mov|m4v|3gp|mkv|webm|avi)$/i;
const VIDEO_EXT = /\.(mp4|mov|m4v|3gp|mkv|webm|avi)$/i;
const HISTORY_THUMBS = 4;
/** Ảnh thu nhỏ đã tải của từng lượt (giữ lại để vẽ lại danh sách không phải đọc đĩa). */
const thumbCache = new Map<string, Promise<string[]>>();

function historyThumbs(id: string): Promise<string[]> {
  let p = thumbCache.get(id);
  if (!p) {
    p = invoke<string[]>("history_thumbs", { id }).catch(() => []);
    thumbCache.set(id, p);
  }
  return p;
}

/** Một ô ảnh/video: ảnh thu nhỏ nếu có, không thì biểu tượng. */
function mediaTile(name: string, thumb: string | undefined, video: boolean, className: string): HTMLElement {
  const tile = el("div", { className: `${className}${video ? " video" : ""}`, title: name });
  if (thumb) tile.append(el("img", { src: thumb, alt: "", draggable: false }));
  else tile.append(el("span", { className: "media-icon", textContent: video ? "🎬" : "🖼" }));
  if (video) tile.append(el("span", { className: "media-play", textContent: "▶" }));
  return tile;
}

function renderHistoryThumbs(h: HistoryEntry): HTMLElement {
  const strip = el("div", { className: "t-thumbs" });
  const shown = (h.thumbs ?? []).slice(0, HISTORY_THUMBS);
  shown.forEach((t, i) => {
    const tile = mediaTile(t.name, undefined, VIDEO_EXT.test(t.name), "t-thumb");
    const file = h.files.find((f) => f.name === t.name);
    const canOpen = !me?.mobile && !!file?.path && !file.path.includes("://");
    tile.title = canOpen ? `Bấm để mở: ${t.name}` : t.name;
    tile.addEventListener("click", (e) => {
      e.stopPropagation();
      if (canOpen) invoke("open_file", { path: file!.path }).catch((err) => toast(errorText(err), "error"));
      else historyThumbs(h.id).then((urls) => urls[i] && showThumb(urls[i], t.name));
    });
    historyThumbs(h.id).then((urls) => {
      if (!urls[i]) return;
      tile.querySelector(".media-icon")?.remove();
      tile.prepend(el("img", { src: urls[i], alt: "", draggable: false }));
    });
    strip.append(tile);
  });
  const more = h.files.filter((f) => MEDIA_EXT.test(f.name)).length - shown.length;
  if (more > 0) strip.append(el("span", { className: "muted small t-thumbs-more", textContent: `+${more}` }));
  return strip;
}

/** Xem lớn ảnh thu nhỏ (điện thoại, hoặc file gốc không mở được). */
function showThumb(src: string, name: string) {
  $<HTMLImageElement>("thumb-img").src = src;
  $("thumb-name").textContent = name;
  $<HTMLDialogElement>("dlg-thumb").showModal();
}

function renderHistoryDetails(h: HistoryEntry): HTMLElement {
  const box = el("div", { className: "t-more" });
  box.addEventListener("click", (e) => e.stopPropagation());
  if (h.text) box.append(el("pre", { textContent: h.text }));
  if (h.online && h.direction === "send" && h.status === "done") {
    const qr = el("button", { className: "link small", textContent: "Hiện mã QR" });
    qr.addEventListener("click", () => showOnlineResult(h.online!));
    box.append(
      el("div", { className: "online-hist" }, el("span", { className: "code online-hist-link", textContent: h.online.url }), qr),
      el("div", { className: "muted small", textContent: `${h.online.provider} · ${expiryText(h.online.expires_at)}` }),
    );
  }
  for (const f of h.files) {
    const row = el("div", { className: "file-row" }, el("span", { className: "name", textContent: f.name, title: f.path ?? f.name }));
    if (f.path && !me?.mobile && !f.path.startsWith("content://")) {
      const b = el("button", { className: "link small", textContent: me?.info.os === "windows" ? "Hiện trong Explorer" : "Hiện trong Finder" });
      b.addEventListener("click", () => invoke("reveal_path", { path: f.path }).catch((e) => toast(errorText(e), "error")));
      row.append(b);
    }
    box.append(row);
  }
  if (h.file_count > h.files.length) box.append(el("div", { className: "muted small", textContent: `… và ${h.file_count - h.files.length} file khác` }));
  return box;
}

function progressBar(pct: number): HTMLElement {
  const fill = el("i");
  fill.style.width = `${pct}%`;
  return el("div", { className: "bar" }, fill);
}

// ---- Gửi qua Internet ----

type ProviderId = "storage-to" | "temp-sh";
interface ProviderInfo {
  id: ProviderId;
  name: string;
  max_bytes: number;
  days: number;
  app_download: boolean;
  browser_download: boolean;
}
interface OnlineSent {
  transfer_id: string;
  link: string;
  provider: ProviderId;
  provider_name: string;
  page_url: string;
  expires_at: number;
  qr: string;
  file_count: number;
  name: string;
}

let providers: ProviderInfo[] = [];
let onlineProvider: ProviderId = "storage-to";
/** Lượt tải lên đang hiện trong hộp thoại. */
let onlineTransfer: string | null = null;
let onlineRecvLink = "";

async function copyText(text: string, message = "Đã sao chép") {
  try {
    await writeText(text);
    toast(message);
  } catch (e) {
    toast(errorText(e), "error");
  }
}

function dateText(secs: number): string {
  const d = new Date(secs * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())} ${pad(d.getDate())}/${pad(d.getMonth() + 1)}/${d.getFullYear()}`;
}

function expiryText(secs: number): string {
  if (!secs) return "";
  return secs * 1000 < Date.now() ? "Link đã hết hạn" : `Hết hạn lúc ${dateText(secs)}`;
}

const gbText = (n: number) => `${Math.round(n / 1e9)} GB`;

function stagedBytes(): number {
  return staged.reduce((sum, s) => sum + (s.size ?? 0), 0);
}

function onlineStep(step: "pick" | "progress" | "done") {
  $("online-pick").hidden = step !== "pick";
  $("online-progress").hidden = step !== "progress";
  $("online-done").hidden = step !== "done";
}

function renderProviders() {
  const size = stagedBytes();
  const hasDir = staged.some((s) => s.is_dir);
  $("online-providers").replaceChildren(
    ...providers.map((p) => {
      const tooBig = !hasDir && size > p.max_bytes;
      const radio = el("input", { type: "radio", name: "online-provider", value: p.id, checked: p.id === onlineProvider, disabled: tooBig });
      radio.addEventListener("change", () => (onlineProvider = p.id));
      const how = p.browser_download
        ? "Người nhận tải thẳng trong trình duyệt"
        : p.app_download
          ? "Người nhận mở link bằng app BetterDrop, hoặc tải file rồi mở trên trang nhận"
          : "Người nhận tải file từ trang của dịch vụ rồi mở trên trang nhận";
      return el(
        "label",
        { className: `online-provider${tooBig ? " disabled" : ""}` },
        radio,
        el(
          "div",
          {},
          el("div", {}, el("strong", { textContent: p.name }), ` · tối đa ${gbText(p.max_bytes)} · giữ ${p.days} ngày`),
          el("div", { className: "muted small", textContent: tooBig ? "Quá dung lượng tối đa của dịch vụ này" : how }),
        ),
      );
    }),
  );
  if (providers.find((p) => p.id === onlineProvider && !hasDir && size > p.max_bytes)) {
    const ok = providers.find((p) => hasDir || size <= p.max_bytes);
    if (ok) onlineProvider = ok.id;
    renderProviders();
  }
}

async function openOnline() {
  if (!staged.length) {
    const r = await open({ multiple: true, directory: false });
    if (!r) return;
    await addStaged(Array.isArray(r) ? r : [r]);
    if (!staged.length) return;
  }
  if (!providers.length) providers = await invoke<ProviderInfo[]>("online_providers");
  const size = stagedBytes();
  $("online-summary").textContent =
    staged.length === 1
      ? `${staged[0].name}${size ? ` · ${formatBytes(size)}` : ""}`
      : `${staged.length} mục đã chọn${size ? ` · ${formatBytes(size)}` : ""}`;
  renderProviders();
  onlineStep("pick");
  const dlg = $<HTMLDialogElement>("dlg-online");
  if (!dlg.open) dlg.showModal();
}

async function startOnline() {
  const btn = $<HTMLButtonElement>("btn-online-start");
  btn.disabled = true;
  try {
    const id = await invoke<string>("online_send", { paths: staged.map((s) => s.path), provider: onlineProvider });
    onlineTransfer = id;
    staged = [];
    renderStaged();
    $("online-status").textContent = "Đang mã hoá và tải lên…";
    $("online-detail").textContent = "";
    $("online-bar").style.width = "0%";
    onlineStep("progress");
    const t = transfers.get(id);
    if (t) updateOnlineProgress(t);
  } catch (e) {
    toast(errorText(e), "error");
  } finally {
    btn.disabled = false;
  }
}

function updateOnlineProgress(t: TransferUpdate) {
  if (t.id !== onlineTransfer || $("online-progress").hidden) return;
  $("online-bar").style.width = `${progressOf(t)}%`;
  $("online-detail").textContent = t.status === "waiting" ? t.note ?? "Đang chuẩn bị…" : detailOf(t);
  if (t.status === "error" || t.status === "cancelled") {
    onlineTransfer = null;
    $<HTMLDialogElement>("dlg-online").close();
  }
}

function renderOnlinePeers() {
  const sel = $<HTMLSelectElement>("online-peer");
  const online = peers.filter((p) => p.online);
  sel.replaceChildren(
    ...(online.length
      ? online.map((p) => el("option", { value: p.id, textContent: p.name }))
      : [el("option", { value: "", textContent: "Không có máy nào đang bật" })]),
  );
  sel.disabled = !online.length;
  $<HTMLButtonElement>("btn-online-send-link").disabled = !online.length;
}

/** Hiện link đã tạo (vừa tải lên xong, hoặc mở lại từ Lịch sử). */
async function showOnlineResult(r: { url: string; expires_at: number; provider: string; qr?: string }) {
  $<HTMLInputElement>("online-link").value = r.url;
  $("online-expiry").textContent = `Lưu ở ${r.provider} · ${expiryText(r.expires_at)}`;
  $("online-qr").innerHTML = r.qr ?? (await invoke<string>("online_qr", { link: r.url }).catch(() => ""));
  const p = providers.find((x) => x.name === r.provider);
  $("online-receive-hint").textContent = p?.browser_download
    ? "Người nhận mở link bằng trình duyệt là tải được, không cần cài app."
    : `Người nhận mở link bằng trình duyệt, làm theo hướng dẫn để tải file từ ${r.provider} rồi giải mã ngay trên trang. Có BetterDrop thì dán link vào “Nhận bằng link”.`;
  renderOnlinePeers();
  onlineStep("done");
  const dlg = $<HTMLDialogElement>("dlg-online");
  if (!dlg.open) dlg.showModal();
}

function openOnlineRecv(link = "") {
  $<HTMLInputElement>("online-recv-link").value = link;
  $("online-recv-manual").hidden = true;
  $<HTMLDialogElement>("dlg-online-recv").showModal();
  if (link) receiveOnline();
}

async function receiveOnline(ev?: Event) {
  ev?.preventDefault();
  const link = $<HTMLInputElement>("online-recv-link").value.trim();
  if (!link) return;
  const btn = $<HTMLButtonElement>("btn-online-recv-go");
  btn.disabled = true;
  try {
    const r = await invoke<{ transfer_id: string | null; provider_name: string; page_url: string }>("online_receive_link", { link });
    if (r.transfer_id) {
      $<HTMLDialogElement>("dlg-online-recv").close();
      toast("Đang tải về và giải mã…");
      return;
    }
    onlineRecvLink = link;
    $("online-recv-why").textContent = `${r.provider_name} không cho app tải thẳng. Làm hai bước:`;
    $("online-recv-manual").hidden = false;
  } catch (e) {
    toast(errorText(e), "error");
  } finally {
    btn.disabled = false;
  }
}

async function receiveOnlineFile() {
  const r = await open({ multiple: false, directory: false });
  const path = Array.isArray(r) ? r[0] : r;
  if (!path) return;
  try {
    await invoke<string>("online_receive_file", { link: onlineRecvLink, path });
    $<HTMLDialogElement>("dlg-online-recv").close();
    toast("Đang giải mã…");
  } catch (e) {
    toast(errorText(e), "error");
  }
}

/** Link BetterDrop gửi qua Internet? */
const isOnlineLink = (s: string) => /\/(receive|nhan)\/#\d+\.[a-z]+\./.test(s.trim());

function bindOnline() {
  $("btn-online").addEventListener("click", openOnline);
  $("btn-online-recv").addEventListener("click", () => openOnlineRecv());
  $("btn-online-cancel").addEventListener("click", () => $<HTMLDialogElement>("dlg-online").close());
  $("btn-online-start").addEventListener("click", startOnline);
  $("btn-online-abort").addEventListener("click", () => onlineTransfer && invoke("cancel_transfer", { id: onlineTransfer }));
  $("btn-online-hide").addEventListener("click", () => $<HTMLDialogElement>("dlg-online").close());
  $("btn-online-close").addEventListener("click", () => $<HTMLDialogElement>("dlg-online").close());
  $("btn-online-copy").addEventListener("click", () => copyText($<HTMLInputElement>("online-link").value, "Đã sao chép link"));
  $("btn-online-send-link").addEventListener("click", async () => {
    const peerId = $<HTMLSelectElement>("online-peer").value;
    const p = peers.find((x) => x.id === peerId);
    if (!p) return;
    try {
      await invoke("send_text", { peerId, text: $<HTMLInputElement>("online-link").value });
      toast(`Đã gửi link tới ${p.name}`);
    } catch (e) {
      toast(errorText(e), "error");
    }
  });
  $("online-recv-form").addEventListener("submit", receiveOnline);
  $("btn-online-recv-close").addEventListener("click", () => $<HTMLDialogElement>("dlg-online-recv").close());
  $("btn-online-recv-page").addEventListener("click", () =>
    invoke("online_open_page", { link: onlineRecvLink }).catch((e) => toast(errorText(e), "error")),
  );
  $("btn-online-recv-file").addEventListener("click", receiveOnlineFile);
}

// ---- Hộp thoại nhận ----

const requestQueue: IncomingRequest[] = [];
let currentRequest: IncomingRequest | null = null;
let requestTimer: number | undefined;

function queueRequest(r: IncomingRequest) {
  if (currentRequest?.request_id === r.request_id || requestQueue.some((q) => q.request_id === r.request_id)) return;
  requestQueue.push(r);
  if (!currentRequest) showNextRequest();
}

function showNextRequest() {
  const dlg = $<HTMLDialogElement>("dlg-request");
  currentRequest = requestQueue.shift() ?? null;
  clearInterval(requestTimer);
  if (!currentRequest) {
    if (dlg.open) dlg.close();
    return;
  }
  const r = currentRequest;
  $("req-trust-row").hidden = !r.can_trust || !!r.group_name;
  $<HTMLInputElement>("req-trust").checked = false;
  const more = r.file_count - r.preview.length;
  if (r.group_name) {
    $("req-title").textContent = `${r.sender_name} muốn chia sẻ nhóm “${r.group_name}” với bạn`;
    $("req-summary").textContent = `${r.file_count} máy · bấm Chấp nhận để thêm nhóm này`;
  } else {
    $("req-title").replaceChildren(el("span", { id: "req-sender", textContent: r.sender_name }), " muốn gửi cho bạn");
    $("req-summary").textContent = `${r.file_count} file · ${formatBytes(r.total_bytes)} · ${r.sender_os === "web" ? "gửi qua trình duyệt" : `từ máy ${osLabel(r.sender_os)}`}`;
  }
  const media = r.media ?? [];
  const tiles = media.map((m) => mediaTile(m.name, m.thumb, m.video, "req-tile"));
  if (r.media_more && tiles.length) {
    tiles[tiles.length - 1].append(el("span", { className: "req-more", textContent: `+${r.media_more}` }));
  }
  $("req-media").replaceChildren(...tiles);
  $("req-media").hidden = !media.length;
  $("req-preview").replaceChildren(
    ...r.preview.map((p) => el("li", { textContent: p })),
    ...(more > 0 ? [el("li", { className: "muted", textContent: r.group_name ? `và ${more} máy khác…` : `và ${more} file khác…` })] : []),
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
  const trust = accept && $<HTMLInputElement>("req-trust").checked;
  await invoke("respond_request", { requestId: id, accept, trust });
  if (trust) toast("Đã thêm vào máy tin cậy: lần sau nhận luôn, không hỏi");
  showNextRequest();
}

// ---- Tự cập nhật (máy tính) / báo có bản mới (điện thoại) ----

// Điện thoại có thêm `url`: link tải APK (Android) hoặc trang phát hành (iOS).
type UpdateInfo = { version: string; current: string; notes?: string | null; url?: string };

let updateDismissed = false;

function showUpdate(u: UpdateInfo) {
  $("update-text").textContent = `Đã có BetterDrop v${u.version} (máy này đang dùng v${u.current}).`;
  $("update-banner").hidden = updateDismissed;
  $("update-status").textContent = `Có bản mới v${u.version}`;
  $("btn-update-install").textContent = u.url ? "Tải bản mới" : "Cập nhật và mở lại";
}

async function checkUpdate(manual: boolean) {
  const status = $("update-status");
  const btn = $<HTMLButtonElement>("btn-check-update");
  if (manual) {
    btn.disabled = true;
    status.textContent = "Đang kiểm tra…";
  }
  try {
    const u = await invoke<UpdateInfo | null>("check_update");
    if (u) {
      if (manual) updateDismissed = false;
      showUpdate(u);
    } else if (manual) {
      status.textContent = "Đang dùng bản mới nhất";
    }
  } catch (e) {
    // Tự kiểm tra lúc mở app mà lỗi (vd không có mạng Internet) thì im lặng.
    if (manual) status.textContent = errorText(e);
  } finally {
    btn.disabled = false;
  }
}

async function installUpdate() {
  const btn = $<HTMLButtonElement>("btn-update-install");
  if (me?.mobile) {
    // Điện thoại không tự cài được: mở link tải (Android tải APK về rồi bấm cài).
    btn.disabled = true;
    try {
      await invoke("install_update");
    } catch (e) {
      toast(errorText(e), "error");
    } finally {
      btn.disabled = false;
    }
    return;
  }
  btn.disabled = true;
  $("btn-update-later").hidden = true;
  $("update-text").textContent = "Đang tải bản mới…";
  try {
    // Cài xong app tự mở lại nên lệnh này thường không trả về.
    await invoke("install_update");
  } catch (e) {
    $("update-text").textContent = errorText(e);
    btn.disabled = false;
    $("btn-update-later").hidden = false;
  }
}

// ---- Cài đặt ----

function openSettings() {
  if (!me) return;
  $<HTMLInputElement>("set-name").value = me.settings.name;
  $<HTMLInputElement>("set-dir").value = me.settings.download_dir;
  $("set-code").textContent = `${shortCode(me.info.fp)} · BetterDrop v${me.info.version ?? "?"}`;
  const isMac = me.info.os === "mac";
  $("row-dock").hidden = !isMac;
  const android = me.info.os === "android";
  $("row-autostart").hidden = me.mobile;
  $("row-background").hidden = !android;
  $("btn-choose-dir").hidden = me.mobile;
  $("row-hotkeys").hidden = me.mobile || !me.settings.hotkeys;
  renderHotkeys();
  refreshHotkeyStatus();
  $("bg-hint").textContent = android
    ? me.settings.background_receive !== false
      ? "BetterDrop giữ một thông báo thường trực để Android không tắt app. Vuốt app khỏi danh sách ứng dụng gần đây sẽ thoát hẳn."
      : "Hãy mở BetterDrop khi muốn nhận file."
    : me.mobile
    ? "Trên điện thoại, hãy mở BetterDrop khi muốn nhận file."
    : `Khi đóng cửa sổ, BetterDrop vẫn chạy nền và nhận file. Mở lại hoặc thoát hẳn từ biểu tượng trên ${
        isMac ? "thanh menu" : "khay hệ thống (góc dưới bên phải)"
      }.`;
  $("set-addrs").replaceChildren(
    ...(me.addrs.length ? me.addrs : ["(không có mạng)"]).map((a) => el("li", { className: "code", textContent: a })),
  );
  $<HTMLDialogElement>("dlg-settings").showModal();
}

// ---- Phím tắt toàn cục (máy tính) ----

const HOTKEY_FIELDS = { "hk-send": "send_clipboard", "hk-open": "open_app" } as const;

/** "CmdOrCtrl+Shift+D" → "⌘⇧D" trên Mac, "Ctrl+Shift+D" trên Windows/Linux (giống hotkey::label). */
function hotkeyLabel(s: string): string {
  const mac = me?.info.os === "mac";
  const names: Record<string, [string, string]> = {
    cmdorctrl: ["⌘", "Ctrl"],
    commandorcontrol: ["⌘", "Ctrl"],
    cmd: ["⌘", "Win"],
    command: ["⌘", "Win"],
    super: ["⌘", "Win"],
    meta: ["⌘", "Win"],
    ctrl: ["⌃", "Ctrl"],
    control: ["⌃", "Ctrl"],
    alt: ["⌥", "Alt"],
    option: ["⌥", "Alt"],
    shift: ["⇧", "Shift"],
  };
  const parts = s.split("+").map((p) => names[p.toLowerCase()]?.[mac ? 0 : 1] ?? p.replace(/^(Key|Digit)/, ""));
  return parts.join(mac ? "" : "+");
}

/** Tổ hợp vừa nhấn, dạng Rust đọc được ("Ctrl+Alt+Shift+Cmd+D"); null khi mới nhấn phím bổ trợ. */
function hotkeyFromEvent(e: KeyboardEvent): string | null {
  if (["Control", "Shift", "Alt", "Meta", "CapsLock", "Fn"].includes(e.key) || !e.code) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push(me?.info.os === "mac" ? "Cmd" : "Super");
  parts.push(e.code.replace(/^(Key|Digit)/, ""));
  return parts.join("+");
}

function showHotkeyError(err: string | null) {
  $("hk-error").hidden = !err;
  $("hk-error").textContent = err ?? "";
}

function renderHotkeys() {
  const hk = me?.settings.hotkeys;
  if (!hk) return;
  $<HTMLInputElement>("set-hotkeys").checked = hk.enabled;
  for (const [id, key] of Object.entries(HOTKEY_FIELDS)) {
    const input = $<HTMLInputElement>(id);
    input.disabled = !hk.enabled;
    if (document.activeElement !== input) input.value = hotkeyLabel(hk[key]);
  }
}

async function saveHotkeys(change: Partial<Hotkeys>): Promise<boolean> {
  if (!me?.settings.hotkeys) return false;
  try {
    me = await invoke<SelfInfo>("set_hotkeys", { hotkeys: { ...me.settings.hotkeys, ...change } });
    showHotkeyError(null);
    renderMe();
    return true;
  } catch (e) {
    showHotkeyError(errorText(e));
    renderMe();
    return false;
  }
}

function bindHotkeys() {
  $("set-hotkeys").addEventListener("change", () => saveHotkeys({ enabled: $<HTMLInputElement>("set-hotkeys").checked }));
  for (const [id, key] of Object.entries(HOTKEY_FIELDS)) {
    const input = $<HTMLInputElement>(id);
    // Bỏ phím tắt cũ trong lúc bắt phím, để nhấn lại chính tổ hợp đó không kích hoạt nó.
    input.addEventListener("focus", () => {
      input.value = "";
      invoke("pause_hotkeys", { paused: true }).catch(() => {});
    });
    input.addEventListener("blur", async () => {
      renderHotkeys();
      const err = await invoke<string | null>("pause_hotkeys", { paused: false }).catch(() => null);
      if (err) showHotkeyError(err);
    });
    input.addEventListener("keydown", async (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape" || e.key === "Tab") return input.blur();
      const combo = hotkeyFromEvent(e);
      if (!combo) return;
      if (await saveHotkeys({ [key]: combo })) toast("Đã đổi phím tắt");
      input.blur();
    });
  }
}

/** Mở Cài đặt: thử đăng ký lại phím tắt (app chiếm tổ hợp có thể đã thoát) và hiện lỗi nếu còn. */
async function refreshHotkeyStatus() {
  if (me?.mobile) return;
  showHotkeyError(await invoke<string | null>("pause_hotkeys", { paused: false }).catch(() => null));
}

// ---- Ghép nối bằng mã QR ----

async function showPairQr() {
  try {
    const p = await invoke<{ link: string; svg: string }>("pair_info");
    $("qr-svg").innerHTML = p.svg;
    $("qr-link").textContent = p.link;
    const addrs = me?.addrs ?? [];
    $("qr-addrs-block").hidden = addrs.length === 0;
    // Cổng mặc định (53318) được tự điền nên chỉ cần nhập IP.
    $("qr-addrs").replaceChildren(
      ...addrs.map((a) => el("li", { className: "code", textContent: a.endsWith(":53318") ? a.slice(0, -6) : a })),
    );
    $<HTMLDialogElement>("dlg-qr").showModal();
  } catch (e) {
    toast(errorText(e), "error");
  }
}

let scanning = false;

/** Thoát màn hình quét: huỷ camera và hiện lại giao diện. */
async function stopScanning() {
  if (!scanning) return;
  scanning = false;
  try {
    await cancelScan();
  } catch {
    /* camera đã đóng */
  }
}

/** Kết nối bằng link ghép nối (quét QR trong app, hoặc camera thường mở app bằng link). */
async function connectPairLink(link: string) {
  try {
    const p = await invoke<Peer>("add_manual_peer", { addr: link });
    toast(`Đã kết nối với ${p.name} — mã ${shortCode(p.fp)}`);
  } catch (e) {
    const msg = errorText(e);
    // Android 16+: thiếu quyền "Thiết bị lân cận" thì mọi kết nối trong LAN đều treo tới hết giờ.
    // Xin quyền (hoặc xác nhận đã có) để báo đúng nguyên nhân.
    if (me?.info.os === "android" && msg.startsWith("Không kết nối được")) {
      const granted = await invoke<boolean>("ensure_local_network").catch(() => true);
      toast(
        granted
          ? "Không kết nối được tới máy tính. Kiểm tra: điện thoại và máy tính cùng một mạng Wi‑Fi (không phải mạng khách), router không bật “cách ly thiết bị” (AP isolation), điện thoại không bật VPN. Đã cấp quyền thì quét lại."
          : "BetterDrop chưa được phép kết nối trong mạng Wi‑Fi. Vào Cài đặt › Ứng dụng › BetterDrop › Quyền › Thiết bị lân cận › Cho phép, rồi quét lại.",
        "error",
      );
      return;
    }
    throw e;
  }
}

async function scanPairQr() {
  if (scanning) return;
  try {
    if ((await checkPermissions()) !== "granted" && (await requestPermissions()) !== "granted") {
      toast("Cần cho phép dùng camera để quét mã QR. Hoặc dùng “Thêm bằng địa chỉ IP”.", "error");
      return;
    }
  } catch (e) {
    toast(errorText(e), "error");
    return;
  }

  // Camera hiện phía sau trang (windowed), trang trong suốt với khung ngắm và nút Huỷ.
  scanning = true;
  document.documentElement.classList.add("scanning");
  $("scan-overlay").hidden = false;
  // Nút Quay lại của Android: chỉ huỷ quét, không đóng app.
  const back = await onBackButtonPress(() => stopScanning()).catch(() => null);
  try {
    const res = await scan({ windowed: true, formats: [Format.QRCode] });
    if (scanning) await connectPairLink(res.content);
  } catch (e) {
    if (scanning) toast(errorText(e), "error"); // Lỗi thật, không phải do người dùng bấm Huỷ.
  } finally {
    scanning = false;
    document.documentElement.classList.remove("scanning");
    $("scan-overlay").hidden = true;
    await back?.unregister();
  }
}

// ---- Chia sẻ qua trình duyệt ----

interface WebInfo {
  pin: string;
  urls: string[];
  qr_svg: string;
  files: { id: string; rel_path: string; size: number }[];
  text: string | null;
  clients: number;
}

let webInfo: WebInfo | null = null;

function renderWeb() {
  const on = !!webInfo;
  $("web-off").hidden = on;
  $("web-on").hidden = !on;
  $("web-dot").hidden = !on;
  $("web-live").hidden = !on;
  $("web-foot").hidden = !on;
  if (!webInfo) return;
  $("web-live").textContent =
    webInfo.clients > 0 ? `● Đang chia sẻ · ${webInfo.clients} trình duyệt đã vào` : "● Đang chia sẻ";
  $("web-text-state").textContent = webInfo.text ? "✓ Đang chia sẻ văn bản này" : "";
  $("web-qr").innerHTML = webInfo.qr_svg;
  // Một địa chỉ chính (cũng là địa chỉ trong mã QR); các địa chỉ khác gấp gọn bên dưới.
  // Bỏ "http://" cho gọn: gõ "ip:cổng" vào trình duyệt vẫn mở được; QR vẫn chứa link đầy đủ.
  const short = (u: string) => u.replace(/^http:\/\//, "");
  $("web-url").textContent = webInfo.urls[0] ? short(webInfo.urls[0]) : "(không có mạng)";
  $("web-more").hidden = webInfo.urls.length < 2;
  $("web-urls").replaceChildren(...webInfo.urls.slice(1).map((u) => el("li", { className: "code", textContent: short(u) })));
  $("web-pin").textContent = webInfo.pin;
  $("web-files").replaceChildren(
    ...webInfo.files.map((f) => {
      const rm = el("button", { className: "icon-btn small", title: "Bỏ chia sẻ file này", textContent: "×" });
      rm.addEventListener("click", () => webCall("web_share_remove_file", { id: f.id }));
      return el(
        "li",
        {},
        el("span", { className: "staged-name", textContent: f.rel_path }),
        el("span", { className: "staged-size", textContent: formatBytes(f.size) }),
        rm,
      );
    }),
  );
  $("web-files-empty").hidden = webInfo.files.length > 0;
  $("btn-web-unshare").hidden = webInfo.files.length === 0;
  // Lối tắt: chia sẻ luôn những file đã chọn ở màn hình chính (nếu có).
  const shareBtn = $("btn-web-share-files");
  shareBtn.hidden = staged.length === 0;
  shareBtn.textContent = `Thêm ${staged.length} mục đang chọn`;
  $("btn-web-add-folder").hidden = !!me?.mobile;
  if (document.activeElement !== $("web-text")) $<HTMLTextAreaElement>("web-text").value = webInfo.text ?? "";
}

async function webCall(cmd: string, args: Record<string, unknown> = {}) {
  try {
    webInfo = await invoke<WebInfo | null>(cmd, args);
    renderWeb();
  } catch (e) {
    toast(errorText(e), "error");
  }
}

async function openWebDialog() {
  webInfo = await invoke<WebInfo | null>("web_share_status");
  renderWeb();
  $<HTMLDialogElement>("dlg-web").showModal();
}

// ---- File/văn bản chia sẻ vào từ app khác ----

/** Lấy các mục được chia sẻ vào (Mở bằng, Send to, menu Chia sẻ) và đưa vào danh sách chờ gửi. */
async function pullShared() {
  const s = await invoke<{ paths: string[]; text: string | null; link?: string | null }>("take_shared_items").catch(
    () => null,
  );
  if (!s) return;
  if (s.link) await connectPairLink(s.link).catch((e) => toast(errorText(e), "error"));
  if (s.paths.length) {
    await addStaged(s.paths);
    toast(`Đã thêm ${s.paths.length} mục từ app khác — chọn máy nhận để gửi`);
  }
  if (s.text) {
    setTab("text");
    $<HTMLTextAreaElement>("text-input").value = s.text;
    renderPeers();
    toast("Đã nhận văn bản từ app khác — chọn máy nhận để gửi");
  }
}

// ---- Khởi động ----

async function init() {
  me = await invoke<SelfInfo>("get_self_info");
  if (resolveLanguage(me.settings.language) === "en") startEnglish();
  $<HTMLSelectElement>("set-language").value = me.settings.language ?? "auto";
  $("set-language").addEventListener("change", async () => {
    try {
      await invoke("set_language", { language: $<HTMLSelectElement>("set-language").value });
      location.reload(); // đổi toàn bộ chữ trên giao diện
    } catch (e) {
      toast(errorText(e), "error");
    }
  });
  document.body.classList.toggle("mobile", me.mobile);
  if (me.mobile) {
    $("dz-title").textContent = "Chọn file để gửi";
    $("btn-pick-folder").hidden = true;
    $("btn-scan").hidden = false;
  } else {
    $("btn-show-qr").hidden = false;
    $("btn-topbar-qr").hidden = false;
  }
  $("btn-scan").addEventListener("click", scanPairQr);
  $("btn-web").addEventListener("click", openWebDialog);
  $("btn-web-start").addEventListener("click", () => webCall("web_share_start"));
  $("btn-web-stop").addEventListener("click", async () => {
    await invoke("web_share_stop");
    webInfo = null;
    renderWeb();
    toast("Đã tắt chia sẻ qua trình duyệt");
  });
  $("btn-web-pin").addEventListener("click", () => webCall("web_share_new_pin"));
  const addShared = async (paths: string[]) => {
    if (!paths.length) return;
    await webCall("web_share_add_files", { paths });
    toast("Đã đưa lên trang: người mở trang tải được ngay");
  };
  $("btn-web-add-files").addEventListener("click", async () => {
    const r = await open({ multiple: true, directory: false });
    if (r) await addShared(Array.isArray(r) ? r : [r]);
  });
  $("btn-web-add-folder").addEventListener("click", async () => {
    const r = await open({ multiple: true, directory: true });
    if (r) await addShared(Array.isArray(r) ? r : [r]);
  });
  $("btn-web-share-files").addEventListener("click", () => addShared(staged.map((s) => s.path)));
  $("btn-web-unshare").addEventListener("click", () => webCall("web_share_set_files", { paths: [] }));
  await listen("web-changed", async () => {
    if (!$<HTMLDialogElement>("dlg-web").open) return;
    webInfo = await invoke<WebInfo | null>("web_share_status");
    renderWeb();
  });
  $("btn-web-text").addEventListener("click", async () => {
    await webCall("web_share_set_text", { text: $<HTMLTextAreaElement>("web-text").value });
    toast(webInfo?.text ? "Đã chia sẻ văn bản" : "Đã bỏ chia sẻ văn bản");
  });
  for (const id of ["btn-web-close1", "btn-web-close2"])
    $(id).addEventListener("click", () => $<HTMLDialogElement>("dlg-web").close());
  $("btn-scan-cancel").addEventListener("click", stopScanning);
  $("btn-show-qr").addEventListener("click", showPairQr);
  $("btn-topbar-qr").addEventListener("click", showPairQr);
  $("btn-qr-close").addEventListener("click", () => $<HTMLDialogElement>("dlg-qr").close());
  $("btn-copy-link").addEventListener("click", async () => {
    await writeText($("qr-link").textContent ?? "");
    toast("Đã sao chép liên kết");
  });

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

  $("btn-check-update").addEventListener("click", () => checkUpdate(true));
  $("btn-update-install").addEventListener("click", installUpdate);
  $("btn-update-later").addEventListener("click", () => {
    updateDismissed = true;
    $("update-banner").hidden = true;
  });
  await listen<{ downloaded: number; total?: number | null }>("update-progress", (e) => {
    const { downloaded, total } = e.payload;
    $("update-text").textContent = total
      ? `Đang tải bản mới… ${Math.floor((downloaded / total) * 100)}%`
      : "Đang tải bản mới…";
  });

  // Kéo thả trên toàn cửa sổ (chỉ có trên máy tính)
  if (!me.mobile) {
    await getCurrentWebview().onDragDropEvent((e) => {
      const dz = $("dropzone");
      if (e.payload.type === "enter" || e.payload.type === "over") dz.classList.add("hover");
      else dz.classList.remove("hover");
      if (e.payload.type === "drop" && e.payload.paths.length) addStaged(e.payload.paths);
    });
  }

  // Thêm máy thủ công
  $("manual-form").addEventListener("submit", async (ev) => {
    ev.preventDefault();
    const input = $<HTMLInputElement>("manual-addr");
    const addr = input.value.trim();
    if (!addr) return;
    if (isOnlineLink(addr)) {
      input.value = "";
      openOnlineRecv(addr);
      return;
    }
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
  const thumbDlg = $<HTMLDialogElement>("dlg-thumb");
  $("btn-thumb-close").addEventListener("click", () => thumbDlg.close());
  thumbDlg.addEventListener("click", (e) => e.target === thumbDlg && thumbDlg.close());
  $("btn-text-receive").addEventListener("click", () => {
    $<HTMLDialogElement>("dlg-text").close();
    openOnlineRecv($("txt-body").textContent ?? "");
  });
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

  bindToggle("set-receive", "set_receive_enabled", "enabled");
  bindToggle("set-autostart", "set_autostart", "enabled");
  bindToggle("set-dock", "set_hide_dock", "hide");
  bindToggle("set-background", "set_background_receive", "enabled");
  bindHotkeys();
  $("paused-chip").addEventListener("click", async () => {
    try {
      me = await invoke<SelfInfo>("set_receive_enabled", { enabled: true });
      renderMe();
      toast("Đã bật lại nhận file");
    } catch (e) {
      toast(errorText(e), "error");
    }
  });

  $("select-all").addEventListener("change", toggleSelectAll);
  $("group-form").addEventListener("submit", saveGroupFromDialog);
  $("btn-share-group-send").addEventListener("click", sendSharedGroup);
  $("btn-share-group-cancel").addEventListener("click", () => $<HTMLDialogElement>("dlg-share-group").close());
  $("btn-group-cancel").addEventListener("click", () => $<HTMLDialogElement>("dlg-group").close());

  // Sự kiện từ Rust
  await listen<{ groups: Group[]; hidden: DeviceRef[] }>("devices-changed", (e) => applyDevices(e.payload));
  await listen<SelfInfo>("settings-changed", (e) => {
    me = e.payload;
    renderMe();
  });
  await listen<Peer[]>("peers-changed", (e) => {
    peers = e.payload;
    renderPeers();
    renderHistory();
  });
  await listen<TransferUpdate>("transfer-update", (e) => {
    const prev = transfers.get(e.payload.id);
    transfers.set(e.payload.id, e.payload);
    upsertTransfer(e.payload, prev);
    updateOnlineProgress(e.payload);
    if (prev?.status !== e.payload.status) {
      const t = e.payload;
      if (t.status === "done" && t.kind === "files" && t.id === onlineTransfer) {
        // Hộp thoại "Gửi qua Internet" tự hiện link (event online-done).
      } else if (t.status === "done" && t.kind === "files")
        toast(t.direction === "send" ? `Đã gửi xong tới ${t.peer_name}` : `Đã nhận ${t.file_count} file từ ${t.peer_name}`);
      if (t.status === "rejected") toast(`${t.peer_name} đã từ chối`, "error");
      if (t.status === "error") toast(t.error ?? "Lỗi khi chuyển", "error");
    }
  });
  await listen<HistoryEntry>("history-added", (e) => {
    // Lượt chuyển đã kết thúc: chuyển từ danh sách "đang chạy" sang lịch sử.
    transferNodes.get(e.payload.id)?.remove();
    transferNodes.delete(e.payload.id);
    history = [e.payload, ...history.filter((h) => h.id !== e.payload.id)];
    renderHistory();
  });
  await listen<{ sender_name: string; file_count: number; total_bytes: number }>("auto-accepted", (e) =>
    toast(`Đang nhận ${e.payload.file_count} file từ ${e.payload.sender_name} (máy tin cậy)`),
  );
  await listen<{ name: string; sender_name: string }>("group-received", (e) =>
    toast(`Đã thêm nhóm “${e.payload.name}” từ ${e.payload.sender_name}`),
  );
  await listen<IncomingRequest>("incoming-request", (e) => queueRequest(e.payload));
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
    $("btn-text-receive").hidden = !isOnlineLink(e.payload.text);
    const dlg = $<HTMLDialogElement>("dlg-text");
    if (!dlg.open) dlg.showModal();
  });
  await listen<string>("fatal-error", (e) => toast(e.payload, "error"));
  await listen<OnlineSent>("online-done", (e) => {
    const r = e.payload;
    const show = () => showOnlineResult({ url: r.link, expires_at: r.expires_at, provider: r.provider_name, qr: r.qr });
    if (r.transfer_id === onlineTransfer && $<HTMLDialogElement>("dlg-online").open) show();
    else toast(`Đã tải lên ${r.provider_name}: mở Lịch sử để sao chép link`);
    onlineTransfer = null;
  });
  bindOnline();

  renderMe();
  if (me.mobile) {
    // Android 17+ chặn kết nối trong mạng LAN tới khi người dùng cho phép "Thiết bị lân cận".
    invoke<boolean>("ensure_local_network")
      .then((ok) => {
        if (!ok)
          toast(
            "BetterDrop cần quyền “Thiết bị lân cận” để tìm và kết nối máy trong mạng Wi‑Fi. Vào Cài đặt › Ứng dụng › BetterDrop › Quyền để bật.",
            "error",
          );
      })
      // Android 13+: nhận file khi chạy nền cần quyền hiện thông báo (hỏi sau quyền trên, không chồng hộp thoại).
      .then(() => (me?.info.os === "android" && me.settings.background_receive !== false ? invoke<boolean>("ensure_notifications") : true))
      .then((ok) => {
        if (!ok) toast("Chưa được phép hiện thông báo: khi BetterDrop ở nền bạn sẽ không thấy máy khác gửi tới.", "error");
      });
    // Giao diện vừa nạp lại trong lúc app chạy nền: hiện lại yêu cầu đang chờ.
    for (const r of await invoke<IncomingRequest[]>("get_pending_requests")) queueRequest(r);
  }
  peers = await invoke<Peer[]>("get_peers");
  applyDevices(await invoke("get_devices"));
  await pullShared();
  await listen("shared-items", pullShared);
  // Mở app thì tự xem có bản mới không (mỗi 6 tiếng xem lại, vì app hay chạy nền lâu).
  setTimeout(() => checkUpdate(false), 3000);
  setInterval(() => checkUpdate(false), 6 * 3600 * 1000);
  // Android: quay lại app sau khi chọn "Chia sẻ → BetterDrop" từ app khác.
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) pullShared();
  });
  history = await invoke<HistoryEntry[]>("get_history", { limit: 500 }).catch(() => []);
  renderHistory();
  document.querySelectorAll<HTMLButtonElement>("#hist-filter .chip").forEach((b) =>
    b.addEventListener("click", () => {
      historyFilter = b.dataset.filter as typeof historyFilter;
      document.querySelectorAll("#hist-filter .chip").forEach((x) => x.classList.toggle("active", x === b));
      historyShown = 30;
      renderHistory();
    }),
  );
  $("btn-more-history").addEventListener("click", () => {
    historyShown += 30;
    renderHistory();
  });
  $("btn-clear-history").addEventListener("click", async () => {
    if (!(await confirmDialog("Xoá toàn bộ lịch sử?", "Chỉ xoá danh sách trong mục Hoạt động. File đã gửi/nhận vẫn còn nguyên.", "Xoá lịch sử"))) return;
    await invoke("clear_history");
    history = [];
    thumbCache.clear();
    renderHistory();
  });
  renderPeers();
  renderStaged();
}

init().catch((e) => toast(errorText(e), "error"));
