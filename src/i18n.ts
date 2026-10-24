// Giao diện tiếng Anh. App viết chữ tiếng Việt trong HTML/TS; khi chọn tiếng Anh, module này dịch
// các chữ hiển thị (text, title, placeholder, aria-label) ngay khi chúng xuất hiện trên trang.
// Dùng chung cho app và bản demo trên website (scripts/build-website-demo.mjs).
// Thêm chữ mới vào giao diện thì thêm bản dịch vào EXACT (cả câu) hoặc RULES (câu có số/tên).

/* eslint-disable */
  // Whole strings (after trimming).
const EXACT: Record<string, string> = {
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
  "· Văn bản": "· Text",
  // Gửi qua Internet
  "Gửi qua Internet": "Send over the Internet",
  "Mã hoá rồi tải lên dịch vụ lưu file, tạo link gửi cho bất kỳ ai": "Encrypt, upload to a file host and get a link you can send to anyone",
  "Nhận bằng link": "Receive from a link",
  "Lưu ở dịch vụ": "Store with",
  "File được": "Files are",
  "mã hoá ngay trên máy này": "encrypted on this device",
  "trước khi tải lên. Dịch vụ lưu file không xem được nội dung hay tên file. Ai có link thì tải được: chỉ gửi link cho người cần nhận.": "before uploading. The file host can't see the contents or the file names. Anyone with the link can download: only send it to the people who need it.",
  "Tải lên và tạo link": "Upload and create link",
  "Đang mã hoá và tải lên…": "Encrypting and uploading…",
  "Đang chuẩn bị…": "Preparing…",
  "Đóng cửa sổ này vẫn tiếp tục tải lên (xem ở mục Hoạt động).": "Closing this window keeps the upload going (see Activity).",
  "Huỷ tải lên": "Cancel upload",
  "Ẩn": "Hide",
  "Đã tải lên. Gửi link này cho người nhận:": "Uploaded. Send this link to the recipient:",
  "Gửi link tới máy gần": "Send the link to a nearby device",
  "Không có máy nào đang bật": "No devices online",
  "Dán link BetterDrop (https://gmttuts.github.io/better-drop/receive/#…)": "Paste a BetterDrop link (https://gmttuts.github.io/better-drop/receive/#…)",
  "Tải về": "Download",
  "Tải về bằng link này": "Download from this link",
  "Mở trang tải": "Open the download page",
  "và bấm tải file": "and download the file",
  "Chọn file đã tải…": "Choose the downloaded file…",
  "để giải mã và lưu": "to decrypt and save it",
  "File được giải mã ngay trên máy này và lưu vào thư mục nhận file.": "Files are decrypted on this device and saved to your download folder.",
  "Sao chép link": "Copy link",
  "Đã sao chép link": "Link copied",
  "Hiện mã QR": "Show QR code",
  "Link đã hết hạn": "Link expired",
  "Người nhận tải thẳng trong trình duyệt": "Recipients download straight in the browser",
  "Người nhận mở link bằng app BetterDrop, hoặc tải file rồi mở trên trang nhận": "Recipients open the link in BetterDrop, or download the file and open it on the receive page",
  "Người nhận tải file từ trang của dịch vụ rồi mở trên trang nhận": "Recipients download the file from the host's page, then open it on the receive page",
  "Quá dung lượng tối đa của dịch vụ này": "Too large for this service",
  "Người nhận mở link bằng trình duyệt là tải được, không cần cài app.": "Recipients just open the link in a browser; no app needed.",
  "Đang tải về và giải mã…": "Downloading and decrypting…",
  "Đang giải mã…": "Decrypting…",
  "Sai khoá giải mã, hoặc file không phải của link này": "Wrong decryption key, or the file doesn't belong to this link",
  "File mã hoá bị hỏng hoặc bị cắt ngắn (tải chưa hết?)": "The encrypted file is damaged or incomplete (did the download finish?)",
  "Đây không phải file BetterDrop (.bdrop)": "This isn't a BetterDrop (.bdrop) file",
  "File được tạo bằng BetterDrop mới hơn. Hãy cập nhật app": "This file was made by a newer BetterDrop. Please update the app",
  "Link được tạo bằng BetterDrop mới hơn. Hãy cập nhật app": "This link was made by a newer BetterDrop. Please update the app",
  "Link BetterDrop không hợp lệ (thiếu hoặc sai phần sau dấu #)": "That BetterDrop link isn't valid (the part after # is missing or wrong)",
  "Link dùng dịch vụ lưu file mà bản này chưa hỗ trợ": "This link uses a file host this version doesn't support yet",
  "temp.sh: file quá lớn": "temp.sh: file too large",
  "Quá nhiều file trong một lượt gửi": "Too many files in one transfer",
  "Dữ liệu không hợp lệ": "Invalid data",
  "Gói file bị hỏng": "The package is damaged",
  "Dữ liệu đọc ra không đủ": "Couldn't read all the data",
  // Thông báo lỗi/trạng thái từ phía Rust
  "Không tìm thấy máy nhận": "Couldn't find that device",
  "Máy nhận đang tắt chế độ nhận file": "That device has receiving turned off",
  "Hết thời gian chờ phản hồi": "The other device didn't respond in time",
  "Máy nhận đã huỷ": "The receiver cancelled",
  "Máy gửi đã huỷ": "The sender cancelled",
  "Mất kết nối với máy gửi": "Lost connection to the sender",
  "Trình duyệt dừng tải": "The browser stopped downloading",
  "Không có file nào để gửi": "Nothing to send",
  "Đây là chính máy này": "That's this device",
  "Đây là mã QR của chính máy này": "That's this device's own QR code",
  "Đây không phải mã QR của BetterDrop": "That's not a BetterDrop QR code",
  "Liên kết không hợp lệ": "That link isn't valid",
  "Địa chỉ này không phải BetterDrop": "That address isn't a BetterDrop device",
  "Tên máy không được để trống": "The device name can't be empty",
  "Chưa chọn thư mục lưu": "Choose a folder to save to",
  "Tên nhóm không được để trống": "The group needs a name",
  "Nhóm cần ít nhất một máy": "A group needs at least one device",
  "Các file gốc không còn ở vị trí cũ": "The original files are no longer there",
  "Không tìm thấy mục này trong lịch sử": "That item is no longer in the history",
  "File không còn ở vị trí cũ (đã bị xoá hoặc di chuyển)": "The file is no longer there (deleted or moved)",
  "Ngôn ngữ / Language": "Language",
  "Tự động (theo máy) / Automatic": "Automatic",
  "Mất kết nối, đang chờ máy gửi nối lại…": "Connection lost, waiting for the sender to reconnect…",
  "Không gửi được dữ liệu trong 30 giây": "No data could be sent for 30 seconds",
  "Phiên nhận đã kết thúc": "The receiving session has ended",
  "Chia sẻ nhóm": "Share group",
  "Chia sẻ": "Share",
  "Chọn ít nhất một máy": "Choose at least one device",
  "Không có máy nào khác đang bật.": "No other devices are online.",
  "Máy nhận sẽ có nhóm này cùng các máy trong nhóm (sau khi bấm Chấp nhận).": "The other device gets this group and its devices (after it accepts).",
  "Máy nhận cần cập nhật BetterDrop 0.3.5 trở lên để nhận nhóm": "The other device needs BetterDrop 0.3.5 or newer to receive groups",
  "Không tìm thấy nhóm": "Couldn't find that group",
  "Đã nhận văn bản từ app khác — chọn máy nhận để gửi": "Got text from another app; choose devices to send to",
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
  // Lịch sử & máy tin cậy (v0.3.4)
  "Đã gửi": "Sent",
  "Đã nhận": "Received",
  "Xoá lịch sử": "Clear history",
  "Xem thêm": "Show more",
  "Gửi lại": "Send again",
  "Hiện trong Finder": "Show in Finder",
  "Hiện trong Explorer": "Show in Explorer",
  "Bấm để xem chi tiết": "Click for details",
  "Xoá khỏi lịch sử (không xoá file)": "Remove from history (files are kept)",
  "Máy nhận hiện không kết nối được": "That device isn't reachable right now",
  "Xoá toàn bộ lịch sử?": "Clear all history?",
  "Chỉ xoá danh sách trong mục Hoạt động. File đã gửi/nhận vẫn còn nguyên.": "Only the Activity list is cleared. Sent and received files stay where they are.",
  "Đã sao chép": "Copied",
  "Máy nhận đã từ chối": "The receiver declined",
  "Luôn chấp nhận từ máy này": "Always accept from this device",
  "(lần sau nhận luôn, không hỏi)": "(next time, receive without asking)",
  "✓ tin cậy": "✓ trusted",
  "File từ máy này được nhận luôn, không hỏi": "Files from this device are received without asking",
  "Máy tin cậy": "Trusted devices",
  "(file được nhận luôn, không hỏi)": "(files are received without asking)",
  "Bỏ tin cậy": "Stop trusting",
  "Đã thêm vào máy tin cậy: lần sau nhận luôn, không hỏi": "Added to trusted devices: next time it's received without asking",
  "Đây là bản dùng thử trên web, không có file thật để mở.": "This is a web demo; there's no real file to show.",
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
  "Cập nhật và mở lại": "Update and restart",
  "Để sau": "Later",
  "Tải bản mới": "Download update",
  "Kiểm tra bản mới": "Check for updates",
  "Đang kiểm tra…": "Checking…",
  "Đang dùng bản mới nhất": "You're on the latest version",
  "Đang tải bản mới…": "Downloading the update…",
  "Không cài được bản mới": "Couldn't install the update",
  "Không kiểm tra được bản mới": "Couldn't check for updates",
};

const files = (n: string) => `${n} file${n === "1" ? "" : "s"}`;
const dev = (n: string) => `${n} device${n === "1" ? "" : "s"}`;
// Strings built from parts. Names inside are translated through EXACT too.
const name = (s: string) => EXACT[s] || s;
const RULES: [RegExp, (...m: string[]) => string][] = [
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
  [/^(.+) · (\d\d:\d\d) hôm nay$/, (_, st, t) => `${inner(st)} · today ${t}`],
  [/^(.+) · (\d\d:\d\d) hôm qua$/, (_, st, t) => `${inner(st)} · yesterday ${t}`],
  [/^(.+) · (\d\d:\d\d) (\d\d\/\d\d(?:\/\d{4})?)$/, (_, st, t, d) => `${inner(st)} · ${d} ${t}`],
  [/^Đang nhận (\d+) file từ (.+) \(máy tin cậy\)$/, (_, n, p) => `Receiving ${files(n)} from ${name(p)} (trusted device)`],
  [/^(.+): lần sau sẽ hỏi lại trước khi nhận$/, (_, p) => `${name(p)}: you'll be asked again next time`],
  [/^… và (\d+) file khác$/, (_, n) => `… and ${n} more file${n === "1" ? "" : "s"}`],
  [/^Gửi lại tới (.+)$/, (_, p) => `Send again to ${name(p)}`],
  [/^Đã gửi lại văn bản tới (.+)$/, (_, p) => `Sent the text again to ${name(p)}`],
  [/^Không kết nối được tới máy nhận \((.+)\)$/, (_, d) => `Couldn't reach the other device (${d})`],
  [/^Không kết nối được tới máy trong mã QR\. Hai máy có cùng mạng Wi‑Fi không\? \((.+)\)$/, (_, d) => `Couldn't reach the device in the QR code. Are both on the same Wi‑Fi? (${d})`],
  [/^iPhone\/iPad chỉ nhận được khi BetterDrop đang mở trên màn hình\. Hãy mở app trên máy đó rồi gửi lại\. \((.+)\)$/, (_, d) =>
    `An iPhone/iPad can only receive while BetterDrop is open on screen. Open the app on that device and send again. (${inner(d)})`],
  [/^Mất kết nối, đang thử lại \(lần (\d+)\)…$/, (_, n) => `Connection lost, retrying (attempt ${n})…`],
  [/^(.+) muốn chia sẻ nhóm “(.+)” với bạn$/, (_, p, g) => `${name(p)} wants to share the group “${name(g)}” with you`],
  [/^(\d+) máy · bấm Chấp nhận để thêm nhóm này$/, (_, n) => `${dev(n)} · accept to add this group`],
  [/^và (\d+) máy khác…$/, (_, n) => `and ${dev(n)} more…`],
  [/^Chia sẻ nhóm “(.+)”$/, (_, g) => `Share the group “${name(g)}”`],
  [/^Đã gửi nhóm “(.+)” — máy nhận cần bấm Chấp nhận$/, (_, g) => `Sent the group “${name(g)}”; the other device needs to accept`],
  [/^(\d+) máy đã nhận nhóm “(.+)”$/, (_, n, g) => `${dev(n)} received the group “${name(g)}”`],
  [/^Đã thêm nhóm “(.+)” từ (.+)$/, (_, g, p) => `Added the group “${name(g)}” from ${name(p)}`],
  [/^(.+): (.+) đã từ chối$/, (_, p) => `${name(p)} declined`],
  [/^Đã thêm (\d+) mục từ app khác — chọn máy nhận để gửi$/, (_, n) => `Added ${n} item${n === "1" ? "" : "s"} from another app; choose devices to send to`],
  [/^Máy nhận trả lỗi (.+)$/, (_, c) => `The other device returned an error (${c})`],
  [/^Không đọc được (.+): (.+)$/, (_, f, e) => `Couldn't read ${f}: ${e}`],
  [/^Không mở được (.+): (.+)$/, (_, f, e) => `Couldn't open ${f}: ${e}`],
  [/^(\d+) mục đã chọn · (.+)$/, (_, n, b) => `${n} item${n === "1" ? "" : "s"} selected · ${b}`],
  [/^· tối đa (\d+) GB · giữ (\d+) ngày$/, (_, g, d) => `· up to ${g} GB · kept ${d} day${d === "1" ? "" : "s"}`],
  [/^Hết hạn lúc (.+)$/, (_, d) => `Expires ${d}`],
  [/^Lưu ở (.+) · (.+)$/, (_, p, e) => `Stored on ${p} · ${inner(e)}`],
  [/^([a-z0-9.]+) · (Hết hạn lúc .+|Link đã hết hạn)$/, (_, p, e) => `${p} · ${inner(e)}`],
  [/^Người nhận mở link bằng trình duyệt, làm theo hướng dẫn để tải file từ (.+) rồi giải mã ngay trên trang\. Có BetterDrop thì dán link vào “Nhận bằng link”\.$/, (_, p) => `Recipients open the link in a browser and follow the steps to download the file from ${p}, then decrypt it right on the page. With BetterDrop, paste the link into “Receive from a link”.`],
  [/^(.+) không cho app tải thẳng\. Làm hai bước:$/, (_, p) => `${p} doesn't allow apps to download directly. Two steps:`],
  [/^Đã gửi link tới (.+)$/, (_, p) => `Sent the link to ${name(p)}`],
  [/^Đã tải lên (.+): mở Lịch sử để sao chép link$/, (_, p) => `Uploaded to ${p}: copy the link from Activity`],
  [/^Quá giới hạn của (.+): tối đa (\d+) GB mỗi lượt gửi$/, (_, p, g) => `Too large for ${p}: up to ${g} GB per transfer`],
  [/^Quá giới hạn của (.+): tối đa (\d+) GB$/, (_, p, g) => `Too large for ${p}: up to ${g} GB`],
  [/^Mất kết nối, đang thử lại \((\d+)\/(\d+)\)…$/, (_, a, b) => `Connection lost, retrying (${a}/${b})…`],
  [/^Mất kết nối Internet \((.+)\)$/, (_, e) => `Lost the Internet connection (${e})`],
  [/^storage\.to: đã dùng hết hạn mức hôm nay(.*)$/, (_, e) => `storage.to: today's upload quota is used up${e}`],
  [/^File không còn trên (.+) \(đã hết hạn hoặc bị xoá\)$/, (_, p) => `The file is no longer on ${p} (expired or deleted)`],
  [/^(.+) trả về trang web thay vì file$/, (_, p) => `${p} returned a web page instead of the file`],
  [/^(.+) trả lỗi HTTP (\d+)$/, (_, p, c) => `${p} returned HTTP ${c}`],
  [/^Dịch vụ từ chối \(HTTP (\d+)\)$/, (_, c) => `The file host refused (HTTP ${c})`],
  [/^(.+) trả về kết quả lạ: (.*)$/, (_, p, r) => `${p} returned something unexpected: ${r}`],
  [/^File (.+) đã bị sửa trong lúc gửi\. Hãy gửi lại$/, (_, f) => `${f} changed while sending. Please send again`],
  // Chung chung nhất: để cuối cùng.
  [/^Đã có BetterDrop v(.+) \(máy này đang dùng v(.+)\)\.$/, (_, a, b) => `BetterDrop v${a} is available (this device has v${b}).`],
  [/^Có bản mới v(.+)$/, (_, v) => `Update available: v${v}`],
  [/^Đang tải bản mới… (\d+)%$/, (_, n) => `Downloading the update… ${n}%`],
  [/^Không kiểm tra được bản mới: không kết nối được Internet$/, () => "Couldn't check for updates: no Internet connection"],
  [/^Không kiểm tra được bản mới: (.+)$/, (_, e) => `Couldn't check for updates: ${e}`],
  [/^Không cài được bản mới: (.+)$/, (_, e) => `Couldn't install the update: ${e}`],
  [/^Máy (.+)$/, (_, ip) => `Device ${ip}`],
];
/** Dịch phần trạng thái/lỗi đứng trước thời gian trong dòng lịch sử. */
function inner(s: string): string {
  return tr(s) ?? s;
}
function where(w: string) {
  const m = /^trong nhóm “(.+)”$/.exec(w);
  return m ? `in “${name(m[1])}”` : w;
}

function tr(s: string): string | null {
  const t = s.trim();
  if (!t || !/[À-ỹđĐ]|\d+ file|^· mã$/.test(t)) return null;
  let out = EXACT[t];
  if (out === undefined) for (const [re, fn] of RULES) { const m = re.exec(t); if (m) { out = fn(...Array.from(m)); break; } }
  if (out === undefined || out === t) return null;
  return s.replace(t, out);
}
const ATTRS = ["title", "placeholder", "aria-label"];
function walk(root: Node) {
  if (root.nodeType === Node.TEXT_NODE) {
    const v = tr(root.nodeValue ?? "");
    if (v !== null) root.nodeValue = v;
    return;
  }
  if (root.nodeType !== Node.ELEMENT_NODE) return;
  const el = root as Element;
  if (el.tagName === "SCRIPT" || el.tagName === "STYLE") return;
  for (const a of ATTRS) if (el.hasAttribute(a)) { const v = tr(el.getAttribute(a) ?? ""); if (v !== null) el.setAttribute(a, v); }
  if (el.tagName === "TEXTAREA") return; // chữ người dùng gõ giữ nguyên
  for (const c of Array.from(el.childNodes)) walk(c);
}

let started = false;

/** Bật tiếng Anh cho toàn bộ trang (gọi một lần; không có cách tắt ngoài tải lại trang). */
export function startEnglish() {
  if (started) return;
  started = true;
  document.documentElement.lang = "en";
  walk(document.body);
  new MutationObserver((list) => {
    for (const m of list) {
      if (m.type === "characterData") walk(m.target);
      else if (m.type === "attributes") walk(m.target);
      else m.addedNodes.forEach(walk);
    }
  }).observe(document.documentElement, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ATTRS });
}

/** Dịch một chuỗi (dùng cho chữ không nằm trong trang, vd thông báo hệ thống). */
export function translate(s: string): string {
  return started ? tr(s) ?? s : s;
}

/** "auto" theo ngôn ngữ của máy; ?lang=vi|en trên URL (bản demo) được ưu tiên. */
export function resolveLanguage(setting: string | undefined): "vi" | "en" {
  const url = new URLSearchParams(location.search).get("lang");
  if (url === "vi" || url === "en") return url;
  if (setting === "vi" || setting === "en") return setting;
  const nav = (navigator.languages && navigator.languages[0]) || navigator.language || "vi";
  return /^vi\b/i.test(nav) ? "vi" : "en";
}
