import SwiftRs
import Tauri
import UIKit
import WebKit

class UriArgs: Decodable {
  let uri: String
}

class BetterDropPlugin: Plugin {
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

  /// File nhận được lưu trong Documents của app (hiện trong app Tệp).
  @objc public func downloadDir(_ invoke: Invoke) throws {
    let dir = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
    invoke.resolve(["path": dir.path])
  }

  @objc public func deviceName(_ invoke: Invoke) throws {
    invoke.resolve(["path": UIDevice.current.name])
  }

  @objc public func scanFile(_ invoke: Invoke) throws {
    invoke.resolve()
  }
}

@_cdecl("init_plugin_betterdrop_mobile")
public func initPlugin() -> Plugin {
  return BetterDropPlugin()
}
