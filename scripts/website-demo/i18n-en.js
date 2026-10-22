// English for the website demo. The app itself is Vietnamese-only, so when the page opens the demo with
// ?lang=en this swaps the interface text (text nodes, title, placeholder, aria-label) as the app renders it.
// Injected by scripts/build-website-demo.mjs before the app's script; does nothing for other languages.
(() => {
  if (new URLSearchParams(location.search).get("lang") !== "en") return;
  document.documentElement.lang = "en";

  // Whole strings (after trimming).
  const EXACT = {
    "1. Chọn nội dung": "1. Choose what to send",
    "2. Chọn máy nhận": "2. Choose devices",
    "Tệp": "Files",
    "Văn bản": "Text",
    "Kéo thả file hoặc thư mục vào đây": "Drop files or folders here",
    "Chọn file để gửi": "Choose files to send",
    "Chọn file": "Choose files",
    "Chọn thư mục": "Choose folder",
    "Xoá hết": "Clear all",
    "Dán từ clipboard": "Paste from clipboard",
    "Nhập hoặc dán văn bản, đường link…": "Type or paste text, a link…",
    "Dán văn bản, đường link…": "Paste text, a link…",
    "Hoạt động": "Activity",
    "Chưa có lượt chuyển nào.": "No transfers yet.",
    "Kết nối điện thoại": "Connect phone",
    "Kết nối điện thoại bằng mã QR": "Connect a phone with a QR code",
    "Hiện mã QR để điện thoại quét và kết nối": "Show a QR code for a phone to scan",
    "Trình duyệt": "Browser",
    "Cho thiết bị không cài BetterDrop gửi/nhận qua trình duyệt": "Let devices without BetterDrop send and receive in a browser",
    "Cài đặt": "Settings",
    "Thông tin máy này": "This device",
    "Đang tìm máy trong mạng": "Looking for devices on the network",
    "Đang tìm máy trong mạng…": "Looking for devices on the network…",
    "Mở BetterDrop trên máy kia và kết nối cùng một mạng Wi‑Fi/LAN.": "Open BetterDrop on the other device and join the same Wi‑Fi/LAN.",
    "Không thấy máy? Thêm bằng địa chỉ IP": "Don't see it? Add by IP address",
    "vd 192.168.1.20 hoặc liên kết betterdrop://…": "e.g. 192.168.1.20 or a betterdrop://… link",
    "Thêm": "Add",
    "Địa chỉ của máy kia nằm ở": "The other device's address is in",
    "trong BetterDrop trên máy đó.": "in BetterDrop on that device.",
    "Quét mã QR của máy tính": "Scan the computer's QR code",
    "Đưa mã QR trên máy tính vào khung": "Point the camera at the QR code on the computer",
    "Tất cả": "All",
    "+ Nhóm mới": "+ New group",
    "Nhóm mới": "New group",
    "Sửa nhóm": "Edit group",
    "Nhóm máy": "Device groups",
    "Tên nhóm": "Group name",
    "vd Phòng kế toán": "e.g. Accounting",
    "Chọn các máy trong nhóm:": "Devices in this group:",
    "Lưu nhóm": "Save group",
    "Xoá nhóm": "Delete group",
    "Chỉ xoá nhóm, các máy trong nhóm vẫn giữ nguyên.": "Only the group is deleted; its devices stay.",
    "Gửi cho cả nhóm": "Send to the whole group",
    "Lưu thành nhóm": "Save as group",
    "Bỏ chọn": "Deselect",
    "Chọn tất cả": "Select all",
    "Bỏ tất cả": "Deselect all",
    "Chọn để gửi nhiều máy": "Select to send to several devices",
    "Gửi": "Send",
    "Quên máy này": "Forget this device",
    "Quên máy": "Forget",
    "Máy này sẽ không hiện trong danh sách nữa, kể cả khi nó bật lại trong mạng. Bạn có thể hiện lại trong Cài đặt.":
      "This device won't be listed any more, even when it comes back online. You can show it again in Settings.",
    "Máy đã quên": "Forgotten devices",
    "Hiện lại": "Show again",
    "Phiên bản BetterDrop của máy này": "This device's BetterDrop version",
    "bản cũ (trước 0.3.2)": "old version (before 0.3.2)",
    "Máy này dùng phiên bản cũ hơn. Vẫn gửi nhận được, nhưng nên cập nhật để có bản sửa lỗi mới nhất.":
      "This device runs an older version. It still works, but updating brings the latest fixes.",
    "Mã nhận dạng — so với mã hiện ở máy kia": "ID code: compare with the one shown on the other device",
    "Mã nhận dạng:": "ID code:",
    "Đang chờ máy nhận chấp nhận…": "Waiting for the other device to accept…",
    "Đang gửi…": "Sending…",
    "Đang chuyển": "Transferring",
    "Hoàn tất": "Done",
    "Bị từ chối": "Declined",
    "Đã huỷ": "Cancelled",
    "Lỗi": "Error",
    "Lỗi khi chuyển": "Transfer failed",
    "Huỷ": "Cancel",
    "Mở thư mục": "Open folder",
    "thư mục": "folder",
    "muốn gửi cho bạn": "wants to send you",
    "đã gửi văn bản": "sent you text",
    "Từ chối": "Decline",
    "Chấp nhận": "Accept",
    "Sao chép": "Copy",
    "Đóng": "Close",
    "Xong": "Done",
    "Lưu": "Save",
    "Đồng ý": "OK",
    "Đổi": "Change",
    "Chọn…": "Choose…",
    "Tên hiển thị": "Display name",
    "Thư mục lưu file nhận được": "Save received files to",
    "Cho phép nhận file": "Allow receiving files",
    "Mở BetterDrop khi khởi động máy": "Open BetterDrop at login",
    "Ẩn biểu tượng trên Dock": "Hide the Dock icon",
    "Chạy nền": "Runs in the background",
    "Địa chỉ của máy này (để nhập ở máy kia nếu không tự thấy):": "This device's address (type it on the other device if it doesn't show up):",
    "Địa chỉ khác": "Other addresses",
    "Máy nối mạng bằng nhiều cách (dây LAN, Wi‑Fi) nên có nhiều địa chỉ. Dùng địa chỉ nào cũng được.":
      "This device is on the network in more than one way (cable, Wi‑Fi), so it has several addresses. Any of them works.",
    "(không có mạng)": "(no network)",
    "Đang tắt nhận file": "Receiving is off",
    "Bấm để bật lại nhận file": "Click to turn receiving back on",
    "Đã bật lại nhận file": "Receiving is back on",
    "Đã lưu cài đặt": "Settings saved",
    "Khi đóng cửa sổ, BetterDrop vẫn chạy nền và nhận file. Mở lại hoặc thoát hẳn từ biểu tượng trên thanh menu.":
      "When you close the window, BetterDrop keeps running and receiving. Reopen or quit it from the menu bar icon.",
    "Trên điện thoại, hãy mở BetterDrop khi muốn nhận file.": "On a phone, keep BetterDrop open when you want to receive.",
    "Quét để kết nối": "Scan to connect",
    "Trên điện thoại, mở BetterDrop và bấm": "On the phone, open BetterDrop and tap",
    ". Hai máy cần dùng chung mạng Wi‑Fi.": ". Both devices must be on the same Wi‑Fi.",
    "Không quét được? Trên điện thoại, bấm": "Can't scan? On the phone, tap",
    "“Thêm bằng địa chỉ IP”": "“Add by IP address”",
    "và nhập:": "and enter:",
    "Liên kết ghép nối (dán vào ô nhập IP)": "Pairing link (paste it into the IP box)",
    "Sao chép liên kết": "Copy link",
    "Đã sao chép liên kết": "Link copied",
    "Đã sao chép vào clipboard": "Copied to clipboard",
    "Clipboard không có văn bản": "There's no text on the clipboard",
    "Chọn file hoặc nhập văn bản trước": "Choose files or type some text first",
    "Chia sẻ qua trình duyệt": "Share through a browser",
    "Cho điện thoại hoặc máy tính": "Lets a phone or computer",
    "không cài BetterDrop": "without BetterDrop",
    "gửi file, văn bản tới máy này và tải những gì bạn chia sẻ, chỉ bằng trình duyệt. Chỉ dùng trong cùng mạng Wi‑Fi/LAN, cần nhập mã PIN.":
      "send files and text to this device and download what you share, using just a browser. Same Wi‑Fi/LAN only, with a PIN.",
    "Bật chia sẻ": "Turn on sharing",
    "Tắt chia sẻ": "Turn off sharing",
    "Quét QR, hoặc mở trong trình duyệt:": "Scan the QR code, or open in a browser:",
    "Đổi mã PIN: trình duyệt đang kết nối phải nhập lại": "New PIN: connected browsers must enter it again",
    "File cho người khác tải": "Files for others to download",
    "tải được ngay, không cần bấm gửi": "available right away, no need to press send",
    "Chưa có file nào.": "No files yet.",
    "Thêm file…": "Add files…",
    "Thêm thư mục…": "Add folder…",
    "Bỏ chia sẻ file này": "Stop sharing this file",
    "Chia sẻ văn bản": "Share text",
    "tuỳ chọn": "optional",
    "Đóng cửa sổ vẫn tiếp tục chia sẻ.": "Sharing continues when you close the window.",
    "● Đang chia sẻ": "● Sharing",
    "✓ Đang chia sẻ văn bản này": "✓ Sharing this text",
    "Đã chia sẻ văn bản": "Text shared",
    "Đã bỏ chia sẻ văn bản": "Stopped sharing text",
    "Đã tắt chia sẻ qua trình duyệt": "Browser sharing is off",
    "Đã đưa lên trang: người mở trang tải được ngay": "On the page now: anyone with it open can download",
    "trình duyệt": "browser",
    "Không rõ": "Unknown",
    "Không kết nối được": "Couldn't connect",
    "Thiết bị lân cận": "Nearby devices",
    // Demo data and messages from mock.js
    "Phòng kế toán": "Accounting",
    "PC Thiết kế chờ nâng cấp": "Design PC (awaiting upgrade)",
    "Pixel 9 của Tuyến": "Tuyến's Pixel 9",
    "iPad của Dương Vũ": "Dương Vũ's iPad",
    "MacBook của bạn": "Your MacBook",
    "iPhone của bạn": "Your iPhone",
    "Báo cáo quý 3.xlsx": "Q3 report.xlsx",
    "Ảnh sự kiện.zip": "Event photos.zip",
    "Hợp đồng 2026": "Contracts 2026",
    "Bảng lương tháng 10.xlsx": "October payroll.xlsx",
    "Hoá đơn VAT.pdf": "VAT invoice.pdf",
    "Ảnh chứng từ.jpg": "Receipt photo.jpg",
    "Đây là bản dùng thử trên web, không có thư mục thật để mở.": "This is a web demo; there's no real folder to open.",
    "Địa chỉ không hợp lệ. Ví dụ: 192.168.1.20": "That's not a valid address. Example: 192.168.1.20",
    "Hãy đặt tên nhóm": "Give the group a name",
    "Bản dùng thử trên web không mở được camera. Trong app thật, bạn quét mã QR trên máy tính để ghép nối.":
      "The web demo can't use the camera. In the app, you scan the QR code on the computer to pair.",
  };

  const files = (n) => `${n} file${n === "1" ? "" : "s"}`;
  const dev = (n) => `${n} device${n === "1" ? "" : "s"}`;
  // Strings built from parts. Names inside are translated through EXACT too.
  const name = (s) => EXACT[s] || s;
  const RULES = [
    [/^(\d+) file · (.+) · từ máy (.+)$/, (_, n, sz, os) => `${files(n)} · ${sz} · from a ${name(os)} device`],
    [/^(\d+) file · (.+) · gửi qua trình duyệt$/, (_, n, sz) => `${files(n)} · ${sz} · sent from a browser`],
    [/^(·\s*)?(\d+) file · (.+)$/, (_, dot, n, rest) => `${dot || ""}${files(n)} · ${rest}`],
    [/^Tới (.+) · (\d+) file · (.+)$/, (_, p, n, s) => `To ${name(p)} · ${files(n)} · ${s}`],
    [/^Từ (.+) · (\d+) file · (.+)$/, (_, p, n, s) => `From ${name(p)} · ${files(n)} · ${s}`],
    [/^Tới (.+)$/, (_, p) => `To ${name(p)}`],
    [/^Từ (.+)$/, (_, p) => `From ${name(p)}`],
    [/^(\d+) file$/, (_, n) => files(n)],
    [/^(\d+) mục đã chọn$/, (_, n) => `${n} item${n === "1" ? "" : "s"} selected`],
    [/^Thêm (\d+) mục đang chọn$/, (_, n) => `Add the ${n} selected item${n === "1" ? "" : "s"}`],
    [/^Đã chọn (\d+) máy$/, (_, n) => `${dev(n)} selected`],
    [/^Gửi tới (\d+) máy$/, (_, n) => `Send to ${dev(n)}`],
    [/^Gửi tới (.+)$/, (_, p) => `Send to ${name(p)}`],
    [/^Chọn tất cả (\d+) máy đang bật ?(.*)$/, (_, n, w) => `Select all ${n} online device${n === "1" ? "" : "s"}${w ? " " + where(w) : ""}`],
    [/^Bỏ chọn tất cả ?(.*)$/, (_, w) => `Deselect all${w ? " " + where(w) : ""}`],
    [/^Nhóm “(.+)”: (\d+)\/(\d+) máy đang bật$/, (_, g, a, b) => `Group “${name(g)}”: ${a}/${b} devices online`],
    [/^(.+) · (\d+)\/(\d+)$/, (_, g, a, b) => `${name(g)} · ${a}/${b}`],
    [/^(Windows|Linux|Android|macOS|iPhone\/iPad|trình duyệt) · mã$/, (_, os) => `${name(os)} · code`],
    [/^(.+) · đã lưu$/, (_, os) => `${name(os)} · saved`],
    [/^· mã$/, () => "· code"],
    [/^Chọn (.+)$/, (_, p) => `Select ${name(p)}`],
    [/^Quên “(.+)”\?$/, (_, p) => `Forget “${name(p)}”?`],
    [/^Quên (.+)$/, (_, p) => `Forget ${name(p)}`],
    [/^Đã quên (.+)$/, (_, p) => `Forgot ${name(p)}`],
    [/^Xoá nhóm “(.+)”\?$/, (_, g) => `Delete group “${name(g)}”?`],
    [/^Đã tạo nhóm “(.+)”$/, (_, g) => `Created group “${name(g)}”`],
    [/^Đã lưu nhóm “(.+)”$/, (_, g) => `Saved group “${name(g)}”`],
    [/^Đã gửi xong tới (.+)$/, (_, p) => `Sent to ${name(p)}`],
    [/^Đã nhận (\d+) file từ (.+)$/, (_, n, p) => `Received ${files(n)} from ${name(p)}`],
    [/^(.+) đã từ chối$/, (_, p) => `${name(p)} declined`],
    [/^Đang gửi tới (\d+) máy — mỗi máy cần bấm Chấp nhận$/, (_, n) => `Sending to ${dev(n)}; each one has to accept`],
    [/^Đã gửi văn bản tới (\d+) máy$/, (_, n) => `Text sent to ${dev(n)}`],
    [/^Đã gửi văn bản tới (.+)$/, (_, p) => `Text sent to ${name(p)}`],
    [/^Đã thêm (.+) — mã (\w+)$/, (_, p, c) => `Added ${name(p)}, code ${c}`],
    [/^Đã kết nối với (.+) — mã (\w+)$/, (_, p, c) => `Connected to ${name(p)}, code ${c}`],
    [/^Đã lưu vào (.+)$/, (_, d) => `Saved to ${d}`],
    [/^và (\d+) file khác…$/, (_, n) => `and ${n} more file${n === "1" ? "" : "s"}…`],
    [/^● Đang chia sẻ · (\d+) trình duyệt đã vào$/, (_, n) => `● Sharing · ${n} browser${n === "1" ? "" : "s"} connected`],
    [/^Máy (.+)$/, (_, ip) => `Device ${ip}`],
  ];
  function where(w) {
    const m = /^trong nhóm “(.+)”$/.exec(w);
    return m ? `in “${name(m[1])}”` : w;
  }

  function tr(s) {
    const t = s.trim();
    if (!t || !/[À-ỹđĐ]|\d+ file|^· mã$/.test(t)) return null;
    let out = EXACT[t];
    if (out === undefined) for (const [re, fn] of RULES) { const m = re.exec(t); if (m) { out = fn(...m); break; } }
    if (out === undefined || out === t) return null;
    return s.replace(t, out);
  }
  const ATTRS = ["title", "placeholder", "aria-label"];
  function walk(root) {
    if (root.nodeType === 3) {
      const v = tr(root.nodeValue);
      if (v !== null) root.nodeValue = v;
      return;
    }
    if (root.nodeType !== 1 || root.tagName === "SCRIPT" || root.tagName === "STYLE") return;
    for (const a of ATTRS) if (root.hasAttribute(a)) { const v = tr(root.getAttribute(a)); if (v !== null) root.setAttribute(a, v); }
    if (root.tagName === "TEXTAREA") return; // what the visitor typed stays as is
    for (const c of root.childNodes) walk(c);
  }
  new MutationObserver((list) => {
    for (const m of list) {
      if (m.type === "characterData") walk(m.target);
      else if (m.type === "attributes") walk(m.target);
      else m.addedNodes.forEach(walk);
    }
  }).observe(document.documentElement, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ATTRS });
  document.addEventListener("DOMContentLoaded", () => walk(document.body));
})();
