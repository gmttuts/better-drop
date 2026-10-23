import UIKit
import UniformTypeIdentifiers

/// "Chia sẻ → BetterDrop": chép file/văn bản vào hộp thư chung (App Group) rồi mở app chính.
/// App chính đọc hộp thư trong `takeShared` (plugin betterdrop-mobile) và đưa vào danh sách gửi.
///
/// Nếu bản cài không có App Group (vd ký lại bằng Apple ID mà công cụ ký không cấp nhóm),
/// văn bản/link vẫn được chuyển qua đường dẫn `betterdrop://share?text=…`; file thì không.
final class ShareViewController: UIViewController {
  private let label = UILabel()
  private let spinner = UIActivityIndicatorView(style: .large)

  override func viewDidLoad() {
    super.viewDidLoad()
    buildUI()
    Task { await run() }
  }

  private func buildUI() {
    view.backgroundColor = UIColor.black.withAlphaComponent(0.25)
    let card = UIView()
    card.backgroundColor = .secondarySystemBackground
    card.layer.cornerRadius = 16
    card.translatesAutoresizingMaskIntoConstraints = false
    label.text = "Đang thêm vào BetterDrop…"
    label.textAlignment = .center
    label.numberOfLines = 0
    let stack = UIStackView(arrangedSubviews: [spinner, label])
    stack.axis = .vertical
    stack.spacing = 12
    stack.translatesAutoresizingMaskIntoConstraints = false
    spinner.startAnimating()
    card.addSubview(stack)
    view.addSubview(card)
    NSLayoutConstraint.activate([
      card.centerXAnchor.constraint(equalTo: view.centerXAnchor),
      card.centerYAnchor.constraint(equalTo: view.centerYAnchor),
      card.widthAnchor.constraint(equalToConstant: 280),
      stack.topAnchor.constraint(equalTo: card.topAnchor, constant: 24),
      stack.bottomAnchor.constraint(equalTo: card.bottomAnchor, constant: -24),
      stack.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 20),
      stack.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -20),
    ])
  }

  private func run() async {
    let providers = (extensionContext?.inputItems as? [NSExtensionItem] ?? [])
      .flatMap { $0.attachments ?? [] }
    let inbox = SharedInbox.open()
    let entry = inbox.map { $0.newEntry() }
    var texts: [String] = []
    var files = 0
    var failed = 0

    for p in providers {
      do {
        switch try await load(p, into: entry) {
        case .file: files += 1
        case .text(let t): if !texts.contains(t) { texts.append(t) }
        case .none: break
        }
      } catch {
        NSLog("BetterDrop share: \(error)")
        failed += 1
      }
    }

    let text = texts.isEmpty ? nil : texts.joined(separator: "\n")
    if let entry {
      if let text { try? text.write(to: entry.appendingPathComponent(SharedInbox.textName), atomically: true, encoding: .utf8) }
      if files > 0 || text != nil {
        SharedInbox.markReady(entry)
        openApp(URL(string: "betterdrop://shared")!)
        finish("Đã thêm vào BetterDrop. Mở BetterDrop để chọn máy nhận.")
      } else {
        try? FileManager.default.removeItem(at: entry)
        finish(failed > 0 ? "Không đọc được mục đã chọn." : "Không có gì để gửi.")
      }
      return
    }

    // Không có App Group: chỉ chuyển được văn bản/link, qua URL scheme.
    if let text, var c = URLComponents(string: "betterdrop://share") {
      c.queryItems = [URLQueryItem(name: "text", value: text)]
      if let url = c.url { openApp(url) }
      finish(
        providers.count > texts.count
          ? "Đã chuyển văn bản/link. Bản cài này không có App Group nên không chuyển được file: hãy mở BetterDrop và bấm \"Chọn file\"."
          : "Đã chuyển sang BetterDrop.")
    } else {
      finish("Bản cài này không có App Group nên không chuyển được file qua Chia sẻ. Hãy mở BetterDrop và bấm \"Chọn file\".")
    }
  }

  private enum Loaded { case file, text(String), none }

  private func load(_ p: NSItemProvider, into entry: URL?) async throws -> Loaded {
    let fileURL = UTType.fileURL.identifier
    let webURL = UTType.url.identifier
    let plain = UTType.plainText.identifier

    // File thật (app Tệp, Mail…): chép nguyên file, giữ tên.
    if p.hasItemConformingToTypeIdentifier(fileURL), let entry {
      if let url = try await p.loadItem(forTypeIdentifier: fileURL) as? URL, url.isFileURL {
        try SharedInbox.copy(url, name: url.lastPathComponent, into: entry)
        return .file
      }
    }
    // Link (Safari…) và văn bản.
    if p.hasItemConformingToTypeIdentifier(webURL), !p.hasItemConformingToTypeIdentifier(fileURL) {
      if let url = try await p.loadItem(forTypeIdentifier: webURL) as? URL { return .text(url.absoluteString) }
    }
    if p.hasItemConformingToTypeIdentifier(plain) {
      let item = try await p.loadItem(forTypeIdentifier: plain)
      if let s = item as? String { return .text(s) }
      if let d = item as? Data, let s = String(data: d, encoding: .utf8) { return .text(s) }
    }
    // Ảnh, video và dữ liệu khác: lấy bản file mà app nguồn cung cấp.
    guard let entry,
      let type = p.registeredTypeIdentifiers.first(where: { UTType($0)?.conforms(to: .data) == true })
    else { return .none }
    let name = Self.fileName(p, type: type)
    try await withCheckedThrowingContinuation { (c: CheckedContinuation<Void, Error>) in
      _ = p.loadFileRepresentation(forTypeIdentifier: type) { url, error in
        // File tạm chỉ tồn tại trong lúc gọi lại, nên phải chép ngay tại đây.
        if let url {
          do { try SharedInbox.copy(url, name: name ?? url.lastPathComponent, into: entry); c.resume() }
          catch { c.resume(throwing: error) }
        } else {
          c.resume(throwing: error ?? CocoaError(.fileReadUnknown))
        }
      }
    }
    return .file
  }

  /// Tên gợi ý của app nguồn (vd "IMG_0001") + đuôi theo loại dữ liệu.
  private static func fileName(_ p: NSItemProvider, type: String) -> String? {
    guard let base = p.suggestedName, !base.isEmpty else { return nil }
    guard let ext = UTType(type)?.preferredFilenameExtension, (base as NSString).pathExtension.isEmpty else { return base }
    return "\(base).\(ext)"
  }

  private func finish(_ message: String) {
    spinner.stopAnimating()
    spinner.isHidden = true
    label.text = message
    DispatchQueue.main.asyncAfter(deadline: .now() + 1.6) {
      self.extensionContext?.completeRequest(returningItems: nil)
    }
  }

  /// Extension không được gọi UIApplication.open trực tiếp; tìm UIApplication trong chuỗi responder
  /// rồi gọi `openURL:options:completionHandler:` (cách này vẫn chạy trên iOS 18+).
  private func openApp(_ url: URL) {
    let sel = NSSelectorFromString("openURL:options:completionHandler:")
    var r: UIResponder? = self
    while let cur = r {
      if cur.isKind(of: UIApplication.self), cur.responds(to: sel) {
        typealias Fn = @convention(c) (AnyObject, Selector, URL, [String: Any], ((Bool) -> Void)?) -> Void
        let fn = unsafeBitCast(cur.method(for: sel), to: Fn.self)
        fn(cur, sel, url, [:], nil)
        return
      }
      r = cur.next
    }
  }
}
