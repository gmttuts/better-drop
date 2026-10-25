import AVFoundation
import ImageIO
import SwiftRs
import Tauri
import UIKit
import WebKit

class UriArgs: Decodable {
  let uri: String
}

class ThumbArgs: Decodable {
  let uri: String
  let video: Bool
}

class BetterDropPlugin: Plugin {
  private let bonjour = Bonjour()

  /// Quảng bá máy này và bắt đầu tìm máy khác (gọi lại khi đổi tên).
  @objc public func startBonjour(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(StartBonjourArgs.self)
    bonjour.start(args)
    invoke.resolve()
  }

  /// Danh sách máy đã tìm thấy, Rust gọi định kỳ.
  @objc public func bonjourPeers(_ invoke: Invoke) throws {
    invoke.resolve(["peers": bonjour.snapshot()])
  }
  /// iOS không cần khoá multicast (Bonjour sẽ được thêm ở giai đoạn iOS).
  @objc public func acquireMulticastLock(_ invoke: Invoke) throws {
    invoke.resolve()
  }

  @objc public func fileInfo(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(UriArgs.self)
    let url = URL(string: args.uri) ?? URL(fileURLWithPath: args.uri)
    let scoped = url.startAccessingSecurityScopedResource()
    defer { if scoped { url.stopAccessingSecurityScopedResource() } }
    let values = try? url.resourceValues(forKeys: [.nameKey, .fileSizeKey])
    invoke.resolve([
      "name": values?.name ?? url.lastPathComponent,
      "size": values?.fileSize ?? -1,
    ])
  }

  /// Ảnh thu nhỏ JPEG (cạnh dài 160px, base64) của một ảnh (kể cả HEIC) hoặc khung hình
  /// gần đầu của một video, để máy nhận xem trước. Chạy ở luồng nền.
  @objc public func mediaThumb(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ThumbArgs.self)
    let url = args.uri.contains("://") ? (URL(string: args.uri) ?? URL(fileURLWithPath: args.uri)) : URL(fileURLWithPath: args.uri)
    DispatchQueue.global(qos: .userInitiated).async {
      let scoped = url.startAccessingSecurityScopedResource()
      defer { if scoped { url.stopAccessingSecurityScopedResource() } }
      var image: CGImage?
      if args.video {
        let generator = AVAssetImageGenerator(asset: AVURLAsset(url: url))
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: 160, height: 160)
        // Giây thứ nhất thường đẹp hơn khung hình đầu (hay bị đen); video ngắn hơn thì lấy khung đầu.
        image = (try? generator.copyCGImage(at: CMTime(seconds: 1, preferredTimescale: 600), actualTime: nil))
          ?? (try? generator.copyCGImage(at: .zero, actualTime: nil))
      } else if let source = CGImageSourceCreateWithURL(url as CFURL, nil) {
        let options: [CFString: Any] = [
          kCGImageSourceCreateThumbnailFromImageAlways: true,
          kCGImageSourceCreateThumbnailWithTransform: true,
          kCGImageSourceThumbnailMaxPixelSize: 160,
        ]
        image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
      }
      guard let image, let data = UIImage(cgImage: image).jpegData(compressionQuality: 0.7) else {
        invoke.reject("Không tạo được ảnh thu nhỏ")
        return
      }
      invoke.resolve(["data": data.base64EncodedString()])
    }
  }

  /// File nhận được lưu trong Documents của app (hiện trong app Tệp).
  @objc public func downloadDir(_ invoke: Invoke) throws {
    let dir = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
    invoke.resolve(["path": dir.path])
  }

  @objc public func deviceName(_ invoke: Invoke) throws {
    invoke.resolve(["path": UIDevice.current.name])
  }

  /// iOS tự hỏi quyền "Mạng cục bộ" khi app dùng mạng lần đầu.
  @objc public func requestLocalNetwork(_ invoke: Invoke) throws {
    invoke.resolve(["granted": true])
  }

  /// Lấy (và xoá) các mục do Share Extension "Chia sẻ → BetterDrop" để lại trong App Group.
  /// "Mở bằng BetterDrop" thì đi đường RunEvent::Opened ở Rust, không qua đây.
  @objc public func takeShared(_ invoke: Invoke) throws {
    let (uris, text) = SharedInbox.take()
    var ret: [String: Any] = ["uris": uris]
    if let text { ret["text"] = text }
    invoke.resolve(ret)
  }

  @objc public func scanFile(_ invoke: Invoke) throws {
    invoke.resolve()
  }
}

@_cdecl("init_plugin_betterdrop_mobile")
public func initPlugin() -> Plugin {
  return BetterDropPlugin()
}

// MARK: - Bonjour (tìm máy trên iOS)
//
// iOS chặn app tự gửi gói multicast nếu không có quyền đặc biệt của Apple, nên trên
// iOS ta dùng Bonjour của hệ thống (NetService). Giao thức trên mạng vẫn là mDNS chuẩn,
// nên máy Mac/Windows/Android vẫn thấy iPhone và ngược lại.

class StartBonjourArgs: Decodable {
  let id: String
  let name: String
  let os: String
  let fp: String
  let port: Int
  let version: String?
}

final class Bonjour: NSObject, NetServiceBrowserDelegate, NetServiceDelegate {
  static let type = "_betterdrop._tcp."
  private var published: NetService?
  private var browser: NetServiceBrowser?
  private var services: [String: NetService] = [:]
  private var peers: [String: [String: Any]] = [:]
  private let lock = NSLock()

  func start(_ a: StartBonjourArgs) {
    DispatchQueue.main.async {
      self.published?.stop()
      let s = NetService(domain: "local.", type: Bonjour.type, name: a.id, port: Int32(a.port))
      let txt: [String: Data] = [
        "id": Data(a.id.utf8), "name": Data(a.name.utf8), "os": Data(a.os.utf8), "fp": Data(a.fp.utf8),
        "v": Data((a.version ?? "").utf8),
      ]
      s.setTXTRecord(NetService.data(fromTXTRecord: txt))
      s.publish()
      self.published = s

      if self.browser == nil {
        let b = NetServiceBrowser()
        b.delegate = self
        b.searchForServices(ofType: Bonjour.type, inDomain: "local.")
        self.browser = b
      }
    }
  }

  func snapshot() -> [[String: Any]] {
    lock.lock()
    defer { lock.unlock() }
    return Array(peers.values)
  }

  func netServiceBrowser(_ browser: NetServiceBrowser, didFind service: NetService, moreComing: Bool) {
    services[service.name] = service
    service.delegate = self
    service.resolve(withTimeout: 10)
    service.startMonitoring()
  }

  func netServiceBrowser(_ browser: NetServiceBrowser, didRemove service: NetService, moreComing: Bool) {
    services.removeValue(forKey: service.name)
    lock.lock()
    peers.removeValue(forKey: service.name)
    lock.unlock()
  }

  func netServiceDidResolveAddress(_ sender: NetService) { update(sender) }

  func netService(_ sender: NetService, didUpdateTXTRecord data: Data) { update(sender) }

  private func update(_ s: NetService) {
    guard let txtData = s.txtRecordData() else { return }
    let txt = NetService.dictionary(fromTXTRecord: txtData)
    func str(_ k: String) -> String? { txt[k].flatMap { String(data: $0, encoding: .utf8) } }
    guard let id = str("id"), let fp = str("fp") else { return }
    let ips = (s.addresses ?? []).compactMap(Bonjour.ipv4)
    guard !ips.isEmpty, s.port > 0 else { return }
    lock.lock()
    peers[s.name] = [
      "id": id, "name": str("name") ?? "Không tên", "os": str("os") ?? "", "fp": fp,
      "port": s.port, "ips": ips, "version": str("v") ?? "",
    ]
    lock.unlock()
  }

  /// Đổi sockaddr sang chuỗi IPv4 ("192.168.1.5"); bỏ qua IPv6.
  private static func ipv4(_ data: Data) -> String? {
    data.withUnsafeBytes { raw -> String? in
      guard let sa = raw.baseAddress?.assumingMemoryBound(to: sockaddr.self),
        sa.pointee.sa_family == sa_family_t(AF_INET)
      else { return nil }
      var host = [CChar](repeating: 0, count: Int(NI_MAXHOST))
      let ok = getnameinfo(sa, socklen_t(data.count), &host, socklen_t(host.count), nil, 0, NI_NUMERICHOST)
      return ok == 0 ? String(cString: host) : nil
    }
  }
}
