import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { cancel as cancelScan, checkPermissions, Format, requestPermissions, scan } from "@tauri-apps/plugin-barcode-scanner";
import { onBackButtonPress } from "@tauri-apps/api/app";

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
  online: boolean;
}
interface Settings {
  device_id: string;
  name: string;
  download_dir: string;
  receive_enabled: boolean;
  hide_dock: boolean;
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
  setTimeout(() => t.classList.add("leaving"), 4000);
  setTimeout(() => t.remove(), 4400);
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

let groups: Group[] = [];
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
      if (p) meta.append(" · mã ", el("span", { className: "code", textContent: shortCode(p.fp) }));

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

function applyDevices(info: { groups: Group[]; hidden: DeviceRef[] }) {
  groups = info.groups;
  hiddenDevices = info.hidden;
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
  $("req-summary").textContent = `${r.file_count} file · ${formatBytes(r.total_bytes)} · ${r.sender_os === "web" ? "gửi qua trình duyệt" : `từ máy ${osLabel(r.sender_os)}`}`;
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
  const isMac = me.info.os === "mac";
  $("row-dock").hidden = !isMac;
  $("row-autostart").hidden = me.mobile;
  $("btn-choose-dir").hidden = me.mobile;
  $("bg-hint").textContent = me.mobile
    ? "Trên điện thoại, hãy mở BetterDrop khi muốn nhận file."
    : `Khi đóng cửa sổ, BetterDrop vẫn chạy nền và nhận file. Mở lại hoặc thoát hẳn từ biểu tượng trên ${
        isMac ? "thanh menu" : "khay hệ thống (góc dưới bên phải)"
      }.`;
  $("set-addrs").replaceChildren(
    ...(me.addrs.length ? me.addrs : ["(không có mạng)"]).map((a) => el("li", { className: "code", textContent: a })),
  );
  $<HTMLDialogElement>("dlg-settings").showModal();
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
    if (scanning) {
      const p = await invoke<Peer>("add_manual_peer", { addr: res.content });
      toast(`Đã kết nối với ${p.name} — mã ${shortCode(p.fp)}`);
    }
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
  if (!webInfo) return;
  $("web-clients").textContent =
    webInfo.clients > 0
      ? `${webInfo.clients} trình duyệt đã kết nối bằng mã PIN này.`
      : "Chưa có trình duyệt nào kết nối.";
  $("web-text-state").textContent = webInfo.text ? "✓ Đang chia sẻ văn bản này" : "";
  $("web-qr").innerHTML = webInfo.qr_svg;
  // Một địa chỉ chính (cũng là địa chỉ trong mã QR); các địa chỉ khác gấp gọn bên dưới.
  $("web-url").textContent = webInfo.urls[0] ?? "(không có mạng)";
  $("web-more").hidden = webInfo.urls.length < 2;
  $("web-urls").replaceChildren(...webInfo.urls.slice(1).map((u) => el("li", { className: "code", textContent: u })));
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

// ---- Khởi động ----

async function init() {
  me = await invoke<SelfInfo>("get_self_info");
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

  bindToggle("set-receive", "set_receive_enabled", "enabled");
  bindToggle("set-autostart", "set_autostart", "enabled");
  bindToggle("set-dock", "set_hide_dock", "hide");
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

  renderMe();
  if (me.mobile) {
    // Android 17+ chặn kết nối trong mạng LAN tới khi người dùng cho phép "Thiết bị lân cận".
    invoke<boolean>("ensure_local_network").then((ok) => {
      if (!ok)
        toast(
          "BetterDrop cần quyền “Thiết bị lân cận” để tìm và kết nối máy trong mạng Wi‑Fi. Vào Cài đặt › Ứng dụng › BetterDrop › Quyền để bật.",
          "error",
        );
    });
  }
  peers = await invoke<Peer[]>("get_peers");
  applyDevices(await invoke("get_devices"));
  renderPeers();
  renderStaged();
}

init().catch((e) => toast(errorText(e), "error"));
