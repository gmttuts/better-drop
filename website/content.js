// Every piece of text on the site, in Vietnamese and English. main.js puts it into the page.
// Keep it true to what the app actually does (see README.md at the repo root).
window.SITE = {
  repo: "gmttuts/better-drop",
  // Shown until the GitHub API answers (or when there is no public release yet).
  fallbackVersion: "0.3.3",

  // Support the author (the app is free). Same accounts as the Better Finder site.
  support: {
    bmc: { qr: "assets/support/bmc-qr.png", url: "https://buymeacoffee.com/gmttuts" },
    momo: { qr: "assets/support/momo-qr.png", url: "https://me.momo.vn/3GIVuoIWseu3uEudf7F7" },
  },

  text: {
    "nav.how": { vi: "Cách dùng", en: "How it works" },
    "nav.features": { vi: "Tính năng", en: "Features" },
    "nav.browser": { vi: "Qua trình duyệt", en: "Browser sharing" },
    "nav.faq": { vi: "Hỏi đáp", en: "FAQ" },
    "nav.download": { vi: "Tải về", en: "Download" },
    "nav.support": { vi: "Ủng hộ", en: "Support" },

    "hero.new": { vi: "Mới", en: "New" },
    "hero.pill": { vi: "Chia sẻ qua trình duyệt", en: "Browser sharing" },
    "hero.title": {
      vi: 'Gửi file giữa mọi thiết bị, <span class="accent">nhanh như AirDrop.</span>',
      en: 'Send files between any devices, <span class="accent">as easy as AirDrop.</span>',
    },
    "hero.sub": {
      vi: "BetterDrop chuyển file, cả thư mục và văn bản giữa Mac, Windows, Linux, Android và iPhone trong cùng mạng Wi‑Fi. File đi thẳng từ máy này sang máy kia, không qua máy chủ nào. Không cần tài khoản, miễn phí.",
      en: "BetterDrop moves files, whole folders and text between Mac, Windows, Linux, Android and iPhone on the same Wi‑Fi. Files go straight from one device to the other, through no server. No account, free.",
    },
    "hero.other": { vi: "Tải cho hệ điều hành khác →", en: "Other platforms →" },
    "hero.fine": { vi: "Miễn phí", en: "Free" },

    "dl.for": { vi: "Tải cho {os}", en: "Download for {os}" },
    "dl.generic": { vi: "Tải BetterDrop", en: "Download BetterDrop" },

    "stats.platforms": { vi: "hệ điều hành gửi nhận qua lại", en: "operating systems, all talking to each other" },
    "stats.cloudN": { vi: "0", en: "0" },
    "stats.cloud": { vi: "máy chủ: file đi thẳng từ máy này sang máy kia", en: "servers: files go straight from one device to the other" },
    "stats.tls": { vi: "mã hoá mọi lần chuyển, mỗi máy một chứng chỉ riêng", en: "encryption on every transfer, one certificate per device" },
    "stats.freeN": { vi: "0 ₫", en: "$0" },
    "stats.free": { vi: "không quảng cáo, không tài khoản", en: "no ads, no account" },

    "how.kicker": { vi: "Cách dùng", en: "How it works" },
    "how.title": { vi: "Ba bước, không cần cài đặt gì thêm.", en: "Three steps, nothing to set up." },
    "how.1.t": { vi: "Mở BetterDrop trên hai máy", en: "Open BetterDrop on both devices" },
    "how.1.d": {
      vi: "Hai máy ở cùng mạng Wi‑Fi hoặc mạng dây là tự thấy nhau. Không thấy thì quét mã QR hoặc nhập địa chỉ IP.",
      en: "On the same Wi‑Fi or wired network they find each other on their own. If not, scan a QR code or type an IP address.",
    },
    "how.2.t": { vi: "Chọn file và máy nhận", en: "Pick files and a device" },
    "how.2.d": {
      vi: "Kéo thả file, thư mục, hoặc dán văn bản, link. Chọn một máy, nhiều máy, hay cả một nhóm.",
      en: "Drag in files or folders, or paste text and links. Pick one device, several, or a whole group.",
    },
    "how.3.t": { vi: "Máy kia bấm Chấp nhận", en: "The other side taps Accept" },
    "how.3.d": {
      vi: "Không ai gửi được file vào máy bạn nếu bạn không đồng ý. File nằm sẵn trong thư mục Downloads/BetterDrop.",
      en: "Nobody can drop files on your device without your OK. Files land in Downloads/BetterDrop.",
    },

    "feat.kicker": { vi: "Tính năng", en: "Features" },
    "feat.title": { vi: "Những gì bạn cần để chuyển file trong văn phòng hay ở nhà.", en: "Everything you need to move files at home or at the office." },

    "br.kicker": { vi: "Chia sẻ qua trình duyệt", en: "Browser sharing" },
    "br.title": { vi: "Máy không cài app vẫn gửi nhận được.", en: "Devices without the app can still join in." },
    "br.sub": {
      vi: "Bật “Trình duyệt” trên máy tính. Điện thoại, máy tính bảng hay máy của khách chỉ cần mở trình duyệt, vào địa chỉ hiện trên màn hình hoặc quét mã QR, rồi nhập mã PIN.",
      en: "Turn on “Browser” on your computer. A phone, tablet or a guest's laptop just opens a browser, goes to the address on screen or scans the QR code, then enters the PIN.",
    },
    "br.1": { vi: "<b>Gửi lên:</b> chọn file hoặc văn bản trên trang, máy tính nhận ngay.", en: "<b>Upload:</b> pick files or text on the page and the computer gets them right away." },
    "br.2": { vi: "<b>Tải về:</b> tải từng file máy tính chia sẻ, hoặc “Tải tất cả” thành một file .zip.", en: "<b>Download:</b> grab each shared file, or “Download all” as one .zip." },
    "br.3": { vi: "<b>Chỉ trong mạng nội bộ</b>, có mã PIN, mặc định tắt. Máy tính được báo mỗi khi có người tải.", en: "<b>Local network only</b>, PIN-protected, off by default. The computer is told whenever someone downloads." },
    "br.mock.title": { vi: "Chia sẻ từ MacBook", en: "Shared from MacBook" },
    "br.mock.files": { vi: "3 file", en: "3 files" },
    "br.mock.all": { vi: "Tải tất cả (.zip)", en: "Download all (.zip)" },
    "br.mock.send": { vi: "Gửi file lên máy tính", en: "Send files to the computer" },

    "dl.kicker": { vi: "Tải về", en: "Download" },
    "dl.title": { vi: "Chọn hệ điều hành của bạn.", en: "Pick your platform." },
    "dl.loading": { vi: "Đang lấy bản mới nhất từ GitHub…", en: "Checking GitHub for the latest version…" },
    "dl.latest": { vi: "Bản mới nhất: <b>{v}</b>, phát hành {date}.", en: "Latest version: <b>{v}</b>, released {date}." },
    "dl.none": {
      vi: "Chưa có bản phát hành công khai. Các nút bên dưới mở trang GitHub Releases, nơi bản cài sẽ xuất hiện.",
      en: "There's no public release yet. The buttons below open the GitHub Releases page, where installers will appear.",
    },
    "dl.notes": { vi: "Xem có gì mới", en: "What's new" },
    "dl.how": { vi: "Cách cài", en: "How to install" },
    "dl.yours": { vi: "Máy của bạn", en: "Your device" },

    "warn.title": { vi: "App chưa được ký số", en: "The app isn't code-signed yet" },
    "warn.body": {
      vi: "Lần đầu mở, hệ điều hành sẽ cảnh báo vì BetterDrop chưa mua chứng chỉ ký số. Đây là cảnh báo bình thường với app miễn phí; bạn có thể xem toàn bộ mã nguồn trên GitHub.",
      en: "The first time you open it, your system will warn you because BetterDrop doesn't have a paid signing certificate. That's normal for free apps; all of the source code is on GitHub.",
    },

    "net.title": { vi: "Nếu hai máy không thấy nhau", en: "If the devices can't see each other" },
    "net.1": {
      vi: "<b>Windows:</b> khi Tường lửa hỏi, cho phép <b>Private networks</b>. Mạng Wi‑Fi phải đặt là <i>Private</i>; để <i>Public</i> thì việc tự tìm máy bị chặn.",
      en: "<b>Windows:</b> when the Firewall asks, allow <b>Private networks</b>. The Wi‑Fi must be set to <i>Private</i>; on <i>Public</i>, discovery is blocked.",
    },
    "net.2": { vi: "<b>macOS:</b> khi hiện hộp thoại “Mạng cục bộ” (Local Network), bấm <b>Cho phép</b>.", en: "<b>macOS:</b> when the “Local Network” prompt appears, click <b>Allow</b>." },
    "net.3": {
      vi: "<b>Wi‑Fi khách, Wi‑Fi công ty</b> có thể bật “cách ly thiết bị”. Khi đó các máy không nói chuyện được với nhau, kể cả khi nhập IP.",
      en: "<b>Guest or office Wi‑Fi</b> may use “client isolation”. Then devices can't reach each other at all, even by IP.",
    },
    "net.4": {
      vi: "Cổng dùng: <code>53318/TCP</code> (app với app), <code>53319/TCP</code> (trình duyệt), <code>5353/UDP</code> (tự tìm máy).",
      en: "Ports: <code>53318/TCP</code> (app to app), <code>53319/TCP</code> (browser), <code>5353/UDP</code> (discovery).",
    },

    "faq.title": { vi: "Câu hỏi thường gặp", en: "Questions" },

    "support.title": { vi: "Một người làm.<br>Miễn phí cho mọi người.", en: "Made by one person.<br>Free for everyone." },
    "support.sub": {
      vi: "BetterDrop không thu phí, không quảng cáo, không cần tài khoản. Nếu app giúp bạn đỡ phải gửi file vòng vèo, mời tác giả một ly cà phê để các bản cập nhật tiếp tục ra đều.",
      en: "BetterDrop has no price tag, ads or account. If it saves you from emailing files to yourself, you can buy the author a coffee. It keeps the updates coming.",
    },
    "support.bmc": { vi: "Buy Me a Coffee", en: "Buy Me a Coffee" },
    "support.momo": { vi: "MoMo", en: "MoMo" },
    "support.title.both": { vi: "Mời tác giả ly cà phê", en: "Buy the author a coffee" },
    "support.scan": { vi: "Quét bằng điện thoại", en: "Scan with your phone" },
    "support.open": { vi: "Mở trang", en: "Open page" },
    "support.noqr": { vi: "Mã QR sắp có", en: "QR code coming soon" },
    "support.thanks": { vi: "Cảm ơn bạn rất nhiều.", en: "Thank you, it means a lot." },
    "support.close": { vi: "Đóng", en: "Close" },

    "cta.title": { vi: "Thử BetterDrop trên hai máy của bạn.", en: "Try BetterDrop on two of your devices." },
    "cta.sub": { vi: "Một phút để cài, và bạn sẽ không phải gửi file qua Zalo hay email cho chính mình nữa.", en: "A minute to install, and you'll stop emailing files to yourself." },

    "footer.made": { vi: "BetterDrop: chuyển file trong mạng nội bộ, giữa mọi hệ điều hành.", en: "BetterDrop: local file sharing across every platform." },
    "footer.issues": { vi: "Báo lỗi", en: "Report a bug" },
    "footer.releases": { vi: "Các phiên bản", en: "Releases" },
    "copy": { vi: "Sao chép", en: "Copy" },
    "copied": { vi: "Đã chép", en: "Copied" },
  },

  features: [
    { icon: "📡", vi: ["Tự tìm máy", "Máy trong cùng mạng tự hiện ra (mDNS/Bonjour). Hoặc ghép bằng mã QR, hoặc nhập IP."], en: ["Finds devices", "Devices on the same network just show up (mDNS/Bonjour). Or pair with a QR code, or by IP."] },
    { icon: "📁", vi: ["Cả thư mục", "Gửi nguyên thư mục, giữ đúng cấu trúc bên trong. Nhiều file chạy song song."], en: ["Whole folders", "Send a folder with everything inside, structure intact. Several files go in parallel."] },
    { icon: "📋", vi: ["Văn bản và link", "Dán đoạn văn, mật khẩu Wi‑Fi hay đường link, máy kia bấm một nút để chép."], en: ["Text and links", "Paste a note, a Wi‑Fi password or a link; the other side copies it with one tap."] },
    { icon: "✋", vi: ["Hỏi trước khi nhận", "Máy nhận thấy tên người gửi, số file, dung lượng, và phải bấm Chấp nhận."], en: ["Asks first", "The receiver sees who's sending, how many files and how big, and must tap Accept."] },
    { icon: "🔒", vi: ["Mã hoá TLS", "Mỗi máy có chứng chỉ riêng. Mã 4 ký tự (vd 520D) để đối chiếu đúng máy."], en: ["TLS encryption", "Each device has its own certificate. A 4‑character code (e.g. 520D) confirms it's the right one."] },
    { icon: "👥", vi: ["Nhiều máy, nhóm máy", "Chọn nhiều máy gửi một lần, lưu thành nhóm như “Phòng kế toán”, hoặc chọn tất cả."], en: ["Many devices, groups", "Send to several devices at once, save them as a group like “Accounting”, or select all."] },
    { icon: "🔤", vi: ["Giữ tên tiếng Việt", "Tên file có dấu giữ nguyên giữa Mac và Windows. Trùng tên thì thêm “ (1)”, không ghi đè."], en: ["Names stay intact", "Accented file names survive between Mac and Windows. Duplicates get “ (1)”, never overwritten."] },
    { icon: "🖥️", vi: ["Chạy nền", "Nằm trên thanh menu Mac hoặc khay hệ thống Windows, báo khi có người gửi, tự mở cùng máy."], en: ["Runs in the background", "Lives in the Mac menu bar or the Windows tray, notifies you when something arrives, starts with your computer."] },
    { icon: "🔁", vi: ["Mọi phiên bản nói chuyện được", "Giao thức giữ nguyên từ v0.1.0, máy cũ máy mới vẫn gửi nhận được với nhau."], en: ["Every version works together", "The protocol hasn't changed since v0.1.0, so old and new versions still talk to each other."] },
  ],

  // Download cards. `match` picks the file out of the GitHub release; `os` is what detectOS() returns.
  platforms: [
    {
      os: "mac", name: "macOS", req: { vi: "Một bản chạy cả Apple silicon và Intel", en: "One app for Apple silicon and Intel" },
      files: [{ label: ".dmg", match: /universal\.dmg$|\.dmg$/ }],
      steps: {
        vi: ["Mở file .dmg, kéo BetterDrop vào Applications.", "Lần đầu mở: <b>chuột phải vào app › Mở</b>, rồi bấm Mở. Trên macOS 15 trở lên: mở app một lần, rồi vào Cài đặt hệ thống › Quyền riêng tư & Bảo mật › <b>Vẫn mở</b>.", "Nếu macOS báo app “bị hỏng”, chạy lệnh dưới đây trong Terminal rồi mở lại:"],
        en: ["Open the .dmg and drag BetterDrop into Applications.", "First launch: <b>Control-click the app › Open</b>, then Open. On macOS 15 or later: open the app once, then System Settings › Privacy & Security › <b>Open Anyway</b>.", "If macOS says the app “is damaged”, run this in Terminal and open it again:"],
      },
      code: 'xattr -dr com.apple.quarantine "/Applications/BetterDrop.app"',
    },
    {
      os: "windows", name: "Windows", req: { vi: "Windows 10 và 11 · 64-bit", en: "Windows 10 and 11 · 64-bit" },
      files: [{ label: ".exe", match: /x64-setup\.exe$|\.exe$/ }, { label: ".msi", match: /\.msi$/ }],
      steps: {
        vi: ["Tải bản <b>.exe</b> (dễ nhất) hoặc <b>.msi</b> (cho quản trị viên cài hàng loạt).", "Nếu SmartScreen hiện “Windows đã bảo vệ máy tính của bạn”: bấm <b>More info › Run anyway</b>.", "Khi Tường lửa hỏi, chọn <b>Private networks</b>."],
        en: ["Get the <b>.exe</b> (easiest) or the <b>.msi</b> (for admins deploying to many PCs).", "If SmartScreen says “Windows protected your PC”: click <b>More info › Run anyway</b>.", "When the Firewall asks, choose <b>Private networks</b>."],
      },
    },
    {
      os: "linux", name: "Linux", req: { vi: "x86_64 · Ubuntu, Debian, Fedora…", en: "x86_64 · Ubuntu, Debian, Fedora…" },
      files: [{ label: ".deb", match: /\.deb$/ }, { label: ".rpm", match: /\.rpm$/ }, { label: ".AppImage", match: /\.AppImage$/ }],
      steps: {
        vi: ["Ubuntu, Debian: cài file <b>.deb</b>. Fedora, openSUSE: cài file <b>.rpm</b>.", "Bản khác: tải <b>.AppImage</b>, cho phép chạy rồi mở:"],
        en: ["Ubuntu, Debian: install the <b>.deb</b>. Fedora, openSUSE: install the <b>.rpm</b>.", "Anything else: get the <b>.AppImage</b>, make it executable and run it:"],
      },
      code: "chmod +x BetterDrop_*.AppImage && ./BetterDrop_*.AppImage",
    },
    {
      os: "android", name: "Android", req: { vi: "Android 7 trở lên", en: "Android 7 or later" },
      files: [{ label: ".apk", match: /\.apk$/ }],
      steps: {
        vi: ["Tải file .apk ngay trên điện thoại rồi mở nó.", "Cho phép “Cài ứng dụng không rõ nguồn gốc” khi được hỏi.", "<b>Android 17:</b> cho phép quyền “Thiết bị lân cận” (Cài đặt › Ứng dụng › BetterDrop › Quyền), nếu không app không thấy máy nào.", "File nhận được nằm ở <b>Download/BetterDrop</b>."],
        en: ["Download the .apk on the phone and open it.", "Allow “Install unknown apps” when asked.", "<b>Android 17:</b> allow the “Nearby devices” permission (Settings › Apps › BetterDrop › Permissions), or the app won't see anything.", "Received files are in <b>Download/BetterDrop</b>."],
      },
    },
    {
      os: "ios", name: "iPhone / iPad", req: { vi: "Bản .ipa chưa ký · cài bằng Sideloadly hoặc AltStore", en: "Unsigned .ipa · install with Sideloadly or AltStore" },
      files: [{ label: ".ipa", match: /\.ipa$/ }],
      steps: {
        vi: ["BetterDrop chưa có trên App Store. Tải file .ipa về máy tính.", "Cài vào iPhone bằng <b>Sideloadly</b> hoặc <b>AltStore</b>, đăng nhập bằng Apple ID của bạn.", "Apple ID miễn phí: app <b>hết hạn sau 7 ngày</b>, phải cài lại.", "iOS dừng app khi bạn chuyển sang app khác, nên <b>hãy mở BetterDrop khi muốn nhận file</b>. File nằm trong Tệp › Trên iPhone › BetterDrop."],
        en: ["BetterDrop isn't on the App Store yet. Download the .ipa to a computer.", "Install it on the iPhone with <b>Sideloadly</b> or <b>AltStore</b>, signing in with your Apple ID.", "With a free Apple ID the app <b>expires after 7 days</b> and must be reinstalled.", "iOS pauses apps in the background, so <b>keep BetterDrop open when you want to receive</b>. Files are in Files › On My iPhone › BetterDrop."],
      },
    },
  ],

  faq: [
    {
      vi: ["Sao không dùng Bluetooth như AirDrop?", "Bluetooth chậm (vài trăm KB/s) và mỗi hãng làm một kiểu, nên Mac, Windows và Android không nói chuyện được với nhau qua đó. Wi‑Fi trong nhà, văn phòng nhanh hơn hàng chục lần và mọi máy đều có. AirDrop thật ra cũng truyền file qua Wi‑Fi, Bluetooth chỉ để tìm máy."],
      en: ["Why not Bluetooth, like AirDrop?", "Bluetooth is slow (a few hundred KB/s) and every vendor does it differently, so Mac, Windows and Android can't share over it. Your home or office Wi‑Fi is tens of times faster and every device has it. AirDrop itself sends the files over Wi‑Fi too; Bluetooth only finds the device."],
    },
    {
      vi: ["Có cần Internet không?", "Không. Chỉ cần hai máy cùng một mạng Wi‑Fi hoặc mạng dây, kể cả mạng không có Internet hay điểm phát Wi‑Fi từ điện thoại. Không có máy chủ trung gian, file không rời khỏi mạng của bạn."],
      en: ["Does it need the Internet?", "No. The two devices just need to be on the same Wi‑Fi or wired network, even one with no Internet, or a phone hotspot. There's no server in between; files never leave your network."],
    },
    {
      vi: ["Có an toàn không?", "Mọi lần chuyển đều mã hoá TLS. Mỗi máy tự tạo chứng chỉ riêng, và bên gửi chỉ nói chuyện với đúng chứng chỉ máy kia đã công bố. Mã 4 ký tự cạnh tên máy giúp bạn đối chiếu đúng máy. Máy nhận luôn phải bấm Chấp nhận, nên không ai “ném” file vào máy bạn được."],
      en: ["Is it safe?", "Every transfer is TLS-encrypted. Each device creates its own certificate, and the sender only talks to the exact certificate the other device announced. The 4‑character code next to each name lets you check it's the right device. The receiver always has to tap Accept, so nobody can push files at you."],
    },
    {
      vi: ["Vì sao iPhone phải mở app mới nhận được?", "iOS tạm dừng mọi app không ở màn hình chính sau vài giây, và chỉ app của Apple mới được chạy nền để nhận file như AirDrop. Vì vậy hãy mở BetterDrop trên iPhone khi muốn nhận. Gửi từ iPhone đi thì không bị ảnh hưởng."],
      en: ["Why does the iPhone need the app open to receive?", "iOS pauses apps shortly after they leave the screen, and only Apple's own apps may stay awake to receive like AirDrop. So keep BetterDrop open on the iPhone when receiving. Sending from the iPhone works normally."],
    },
    {
      vi: ["Android 17 không thấy máy nào?", "Android 17 chặn kết nối trong mạng nội bộ cho tới khi bạn cho phép quyền “Thiết bị lân cận”. Dùng BetterDrop v0.2.2 trở lên trên điện thoại, rồi vào Cài đặt › Ứng dụng › BetterDrop › Quyền để bật."],
      en: ["Android 17 doesn't see any devices?", "Android 17 blocks local-network connections until you allow the “Nearby devices” permission. Use BetterDrop v0.2.2 or later on the phone, then turn it on in Settings › Apps › BetterDrop › Permissions."],
    },
    {
      vi: ["Máy khác đang dùng bản cũ thì sao?", "Vẫn gửi nhận bình thường. Mọi phiên bản BetterDrop dùng chung một giao thức từ v0.1.0. Danh sách máy hiện phiên bản của từng máy để bạn biết máy nào nên cập nhật."],
      en: ["What if the other device has an older version?", "It still works. Every BetterDrop version has spoken the same protocol since v0.1.0. The device list shows each one's version so you know which to update."],
    },
    {
      vi: ["File nhận được nằm ở đâu?", "Máy tính: <code>Downloads/BetterDrop</code> (đổi được trong Cài đặt). Android: <code>Download/BetterDrop</code>. iPhone: Tệp › Trên iPhone › BetterDrop. Nếu trùng tên, file mới được thêm “ (1)”, file cũ không bị ghi đè."],
      en: ["Where do received files go?", "Computers: <code>Downloads/BetterDrop</code> (changeable in Settings). Android: <code>Download/BetterDrop</code>. iPhone: Files › On My iPhone › BetterDrop. If a name is taken, the new file gets “ (1)”; nothing is overwritten."],
    },
    {
      vi: ["Có mất phí không?", "Không. BetterDrop miễn phí, không quảng cáo, không cần tài khoản, và mã nguồn nằm công khai trên GitHub."],
      en: ["Does it cost anything?", "No. BetterDrop is free, with no ads and no account, and its source code is public on GitHub."],
    },
  ],
};
