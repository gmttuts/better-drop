import Foundation

/// Hộp thư chung giữa Share Extension và app chính, nằm trong App Group:
/// `<nhóm>/ShareInbox/<mục>/` chứa các file được chia sẻ, `.text` (văn bản, nếu có) và
/// `.ready` (ghi cuối cùng, để app không đọc mục đang chép dở).
/// Phần đọc ở app chính: `SharedInbox` trong plugins/betterdrop-mobile/ios (giữ hai bên khớp nhau).
enum SharedInbox {
  static let textName = ".text"
  static let readyName = ".ready"

  static func open() -> SharedInbox.Box? {
    for id in groupIDs() {
      guard let c = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: id) else { continue }
      let dir = c.appendingPathComponent("ShareInbox", isDirectory: true)
      if (try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)) != nil {
        return Box(dir: dir)
      }
    }
    return nil
  }

  struct Box {
    let dir: URL
    func newEntry() -> URL {
      let name = "\(Int(Date().timeIntervalSince1970 * 1000))-\(UUID().uuidString)"
      let url = dir.appendingPathComponent(name, isDirectory: true)
      try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
      return url
    }
  }

  /// Mã App Group: công cụ ký lại (Sideloadly, AltStore…) thường đổi mã này, nên đọc mã thật
  /// trong hồ sơ cấp phép đi kèm trước, rồi mới tới mã mặc định trong Info.plist.
  static func groupIDs() -> [String] {
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

  /// Chép file/thư mục vào mục; trùng tên thì thêm " (1)", " (2)"…
  static func copy(_ src: URL, name: String, into entry: URL) throws {
    let scoped = src.startAccessingSecurityScopedResource()
    defer { if scoped { src.stopAccessingSecurityScopedResource() } }
    let base = (name as NSString).deletingPathExtension
    let ext = (name as NSString).pathExtension
    var dst = entry.appendingPathComponent(name)
    var n = 1
    while FileManager.default.fileExists(atPath: dst.path) {
      dst = entry.appendingPathComponent(ext.isEmpty ? "\(base) (\(n))" : "\(base) (\(n)).\(ext)")
      n += 1
    }
    try FileManager.default.copyItem(at: src, to: dst)
  }

  static func markReady(_ entry: URL) {
    FileManager.default.createFile(atPath: entry.appendingPathComponent(readyName).path, contents: nil)
  }
}
