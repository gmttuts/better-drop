import Foundation

/// Phần đọc hộp thư chung do Share Extension ghi (src-tauri/gen/apple/ShareExtension/SharedInbox.swift,
/// giữ hai bên khớp nhau): `<App Group>/ShareInbox/<mục>/` gồm các file, `.text` và `.ready`.
enum SharedInbox {
  static let textName = ".text"
  static let readyName = ".ready"

  /// Lấy (và xoá khỏi hộp thư) các mục đã chép xong. File được chuyển sang thư mục tạm của app
  /// để vẫn còn tới khi gửi xong; iOS tự dọn thư mục này.
  static func take() -> (uris: [String], text: String?) {
    let fm = FileManager.default
    guard let dir = inboxDir(), let entries = try? fm.contentsOfDirectory(at: dir, includingPropertiesForKeys: [.creationDateKey])
    else { return ([], nil) }
    var uris: [String] = []
    var texts: [String] = []
    for entry in entries.sorted(by: { $0.lastPathComponent < $1.lastPathComponent }) {
      guard fm.fileExists(atPath: entry.appendingPathComponent(readyName).path) else {
        // Mục chép dở bị bỏ (extension bị tắt giữa chừng): dọn sau một ngày.
        let created = (try? entry.resourceValues(forKeys: [.creationDateKey]))?.creationDate ?? Date()
        if Date().timeIntervalSince(created) > 86400 { try? fm.removeItem(at: entry) }
        continue
      }
      if let t = try? String(contentsOf: entry.appendingPathComponent(textName), encoding: .utf8), !t.isEmpty {
        texts.append(t)
      }
      let dest = fm.temporaryDirectory.appendingPathComponent("Shared", isDirectory: true)
        .appendingPathComponent(entry.lastPathComponent, isDirectory: true)
      for f in (try? fm.contentsOfDirectory(at: entry, includingPropertiesForKeys: nil)) ?? [] {
        let name = f.lastPathComponent
        if name == textName || name == readyName { continue }
        try? fm.createDirectory(at: dest, withIntermediateDirectories: true)
        let to = dest.appendingPathComponent(name)
        if (try? fm.moveItem(at: f, to: to)) != nil { uris.append(to.absoluteString) }
      }
      try? fm.removeItem(at: entry)
    }
    return (uris, texts.isEmpty ? nil : texts.joined(separator: "\n"))
  }

  private static func inboxDir() -> URL? {
    for id in groupIDs() {
      if let c = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: id) {
        return c.appendingPathComponent("ShareInbox", isDirectory: true)
      }
    }
    return nil
  }

  /// Mã App Group: công cụ ký lại (Sideloadly, AltStore…) thường đổi mã này, nên đọc mã thật
  /// trong hồ sơ cấp phép đi kèm trước, rồi mới tới mã mặc định trong Info.plist.
  private static func groupIDs() -> [String] {
    var ids: [String] = []
    if let url = Bundle.main.url(forResource: "embedded", withExtension: "mobileprovision"),
      let data = try? Data(contentsOf: url),
      let start = data.range(of: Data("<?xml".utf8)),
      let end = data.range(of: Data("</plist>".utf8), in: start.lowerBound..<data.endIndex),
      let plist = try? PropertyListSerialization.propertyList(from: data[start.lowerBound..<end.upperBound], format: nil)
        as? [String: Any],
      let ent = plist["Entitlements"] as? [String: Any],
      let groups = ent["com.apple.security.application-groups"] as? [String]
    {
      ids += groups
    }
    if let id = Bundle.main.object(forInfoDictionaryKey: "BetterDropAppGroup") as? String, !ids.contains(id) {
      ids.append(id)
    }
    return ids
  }
}
