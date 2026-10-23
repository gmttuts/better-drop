//! Test đầu-cuối: hai "máy" chạy trên localhost, gửi file và thư mục qua HTTPS thật.

use crate::models::*;
use crate::state::{AppState, EmitFn, Tracker};
use crate::{identity, server, transfer};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;

struct Node {
    st: Arc<AppState>,
    events: mpsc::UnboundedReceiver<(&'static str, serde_json::Value)>,
    _dir: tempfile::TempDir,
}

async fn node(name: &str) -> Node {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let ident = identity::load_or_create_identity(&dir.path().join("data")).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::unbounded_channel();
    let emit: EmitFn = Arc::new(move |e, v| {
        let _ = tx.send((e, v));
    });
    let settings = Settings {
        device_id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        download_dir: dir.path().join("downloads").to_string_lossy().into_owned(),
        receive_enabled: true,
        hide_dock: false,
        language: "vi".into(),
    };
    let st = Arc::new(AppState::new(emit, dir.path().join("data"), ident, port, settings));
    let s2 = st.clone();
    tokio::spawn(async move { server::serve(s2, listener).await.unwrap() });
    Node { st, events: rx, _dir: dir }
}

fn as_peer(n: &Node) -> Peer {
    Peer {
        info: n.st.device_info(),
        addrs: vec![format!("127.0.0.1:{}", n.st.port)],
        manual: false,
        online: true,
    }
}

/// Trả lời yêu cầu nhận đầu tiên, và thu các trạng thái cuối của bên nhận.
fn auto_respond(mut rx: Node, accept: bool) -> tokio::task::JoinHandle<(Vec<TransferStatus>, Node)> {
    tokio::spawn(async move {
        let mut finals = vec![];
        while let Some((ev, v)) = rx.events.recv().await {
            match ev {
                "incoming-request" => {
                    let id = v["request_id"].as_str().unwrap().to_string();
                    if let Some(p) = rx.st.pending.lock().unwrap().remove(&id) {
                        p.tx.send(accept).unwrap();
                    }
                    if !accept {
                        break;
                    }
                }
                "transfer-update" => {
                    let s: String = serde_json::from_value(v["status"].clone()).unwrap();
                    match s.as_str() {
                        "done" => {
                            finals.push(TransferStatus::Done);
                            break;
                        }
                        "error" | "cancelled" => {
                            finals.push(TransferStatus::Error);
                            break;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        (finals, rx)
    })
}

fn sender_tracker(st: &AppState) -> Arc<Tracker> {
    Tracker::new(
        st.emit.clone(),
        TransferUpdate {
            id: "t".into(),
            direction: Direction::Send,
            peer_name: "x".into(),
            kind: TransferKind::Files,
            file_count: 0,
            total_bytes: 0,
            transferred: 0,
            speed_bps: 0.0,
            status: TransferStatus::Waiting,
            error: None,
            dir_path: None,
        },
    )
}

fn write(p: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_files_and_folders_end_to_end() {
    let a = node("Mac").await;
    let b = node("Windows").await;
    let peer_b = as_peer(&b);
    let b_downloads = b.st.settings.read().unwrap().download_dir.clone();
    let b_downloads = Path::new(&b_downloads).to_path_buf();
    // File trùng tên có sẵn ở máy nhận -> phải được đổi tên, không ghi đè.
    write(&b_downloads.join("báo cáo.txt"), b"old");
    let watcher = auto_respond(b, true);

    let src = tempfile::tempdir().unwrap();
    let big: Vec<u8> = (0..5_000_000u32).map(|i| (i % 251) as u8).collect();
    write(&src.path().join("báo cáo.txt"), b"hello");
    write(&src.path().join("Photos/2024/a.bin"), &big);
    write(&src.path().join("Photos/empty.txt"), b"");
    write(&src.path().join("Photos/.DS_Store"), b"junk");

    let files = transfer::collect_files(&a.st, &[
        src.path().join("báo cáo.txt").to_string_lossy().into_owned(),
        src.path().join("Photos").to_string_lossy().into_owned(),
    ])
    .unwrap();
    assert_eq!(files.len(), 3, ".DS_Store phải bị bỏ qua");

    let tracker = sender_tracker(&a.st);
    transfer::send_files(&a.st, &peer_b, files, &tracker).await.unwrap();
    assert_eq!(tracker.status(), TransferStatus::Done);
    let (finals, _b) = watcher.await.unwrap();
    assert_eq!(finals, vec![TransferStatus::Done]);

    assert_eq!(std::fs::read(b_downloads.join("báo cáo.txt")).unwrap(), b"old");
    assert_eq!(std::fs::read(b_downloads.join("báo cáo (1).txt")).unwrap(), b"hello");
    assert_eq!(std::fs::read(b_downloads.join("Photos/2024/a.bin")).unwrap(), big);
    assert_eq!(std::fs::read(b_downloads.join("Photos/empty.txt")).unwrap(), b"");
    let leftovers: Vec<_> = walkdir::WalkDir::new(&b_downloads)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".part"))
        .collect();
    assert!(leftovers.is_empty(), "không được còn file .part");
}

#[tokio::test(flavor = "multi_thread")]
async fn rejected_request_reports_rejected() {
    let a = node("Mac").await;
    let b = node("Windows").await;
    let peer_b = as_peer(&b);
    let watcher = auto_respond(b, false);

    let src = tempfile::tempdir().unwrap();
    write(&src.path().join("x.txt"), b"x");
    let files = transfer::collect_files(&a.st, &[src.path().join("x.txt").to_string_lossy().into_owned()]).unwrap();
    let tracker = sender_tracker(&a.st);
    transfer::send_files(&a.st, &peer_b, files, &tracker).await.unwrap();
    assert_eq!(tracker.status(), TransferStatus::Rejected);
    watcher.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_fingerprint_is_refused() {
    let a = node("Mac").await;
    let b = node("Windows").await;
    let mut peer_b = as_peer(&b);
    peer_b.info.fp = "ab".repeat(32);

    let src = tempfile::tempdir().unwrap();
    write(&src.path().join("x.txt"), b"x");
    let files = transfer::collect_files(&a.st, &[src.path().join("x.txt").to_string_lossy().into_owned()]).unwrap();
    let tracker = sender_tracker(&a.st);
    let err = transfer::send_files(&a.st, &peer_b, files, &tracker).await.unwrap_err();
    assert!(err.contains("Không kết nối được"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_text() {
    let a = node("Mac").await;
    let mut b = node("Windows").await;
    let peer_b = as_peer(&b);
    transfer::send_text(a.st.clone(), peer_b, "xin chào 👋".into()).await.unwrap();
    loop {
        let (ev, v) = b.events.recv().await.unwrap();
        if ev == "incoming-text" {
            assert_eq!(v["text"], "xin chào 👋");
            assert_eq!(v["sender_name"], "Mac");
            break;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn receive_disabled_is_refused() {
    let a = node("Mac").await;
    let b = node("Windows").await;
    b.st.settings.write().unwrap().receive_enabled = false;
    let peer_b = as_peer(&b);

    let src = tempfile::tempdir().unwrap();
    write(&src.path().join("x.txt"), b"x");
    let files = transfer::collect_files(&a.st, &[src.path().join("x.txt").to_string_lossy().into_owned()]).unwrap();
    let tracker = sender_tracker(&a.st);
    transfer::send_files(&a.st, &peer_b, files, &tracker).await.unwrap();
    assert_eq!(tracker.status(), TransferStatus::Rejected);
    assert!(b.st.pending.lock().unwrap().is_empty(), "không được hỏi người dùng");

    let err = transfer::send_text(a.st.clone(), peer_b, "hi".into()).await.unwrap_err();
    assert!(err.contains("tắt chế độ nhận"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn receiver_remembers_sender_for_replies() {
    let a = node("Phone").await;
    let mut b = node("Mac").await;
    assert!(b.st.peer_list().is_empty());
    transfer::send_text(a.st.clone(), as_peer(&b), "hi".into()).await.unwrap();
    let peers = b.st.peer_list();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].info.name, "Phone");
    assert_eq!(peers[0].addrs, vec![format!("127.0.0.1:{}", a.st.port)]);
    while let Ok((ev, _)) = b.events.try_recv() {
        let _ = ev;
    }

    // Máy nhận gửi ngược lại được cho máy vừa gửi.
    transfer::send_text(b.st.clone(), peers[0].clone(), "chào lại".into()).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn pairs_via_qr_link() {
    let phone = node("Phone").await;
    let mac = node("Mac \"Studio\" & co").await;
    let link = crate::discovery::pair_link(&mac.st.device_info(), &[
        "127.0.0.1:1".into(), // địa chỉ không tới được, phải bỏ qua
        format!("127.0.0.1:{}", mac.st.port),
    ]);
    assert!(crate::discovery::pair_svg(&link).unwrap().starts_with("<?xml"));

    let peer = crate::discovery::add_from_link(&phone.st, &link).await.unwrap();
    assert_eq!(peer.info.name, "Mac \"Studio\" & co");
    assert_eq!(peer.addrs, vec![format!("127.0.0.1:{}", mac.st.port)]);
    assert_eq!(phone.st.peer_list().len(), 1);

    // QR giả mạo: đúng địa chỉ nhưng fingerprint khác thì bị từ chối.
    let mut fake = mac.st.device_info();
    fake.fp = "cd".repeat(32);
    let bad = crate::discovery::pair_link(&fake, &[format!("127.0.0.1:{}", mac.st.port)]);
    let other = node("Other").await;
    assert!(crate::discovery::add_from_link(&other.st, &bad).await.is_err());
    assert!(crate::discovery::add_from_link(&other.st, "https://example.com").await.is_err());
}

#[test]
fn mobile_download_dir_is_refreshed_on_start() {
    let dir = tempfile::tempdir().unwrap();
    let old = identity::load_settings(
        dir.path(),
        identity::Defaults { download_dir: Some("/old/container/Documents".into()), ..Default::default() },
    );
    assert_eq!(old.download_dir, "/old/container/Documents");

    // Không ép: giữ thư mục người dùng đã chọn (máy tính).
    let kept = identity::load_settings(
        dir.path(),
        identity::Defaults { download_dir: Some("/new".into()), ..Default::default() },
    );
    assert_eq!(kept.download_dir, "/old/container/Documents");

    // Ép (điện thoại): luôn dùng đường dẫn mới, và giữ nguyên id máy.
    let fresh = identity::load_settings(
        dir.path(),
        identity::Defaults {
            download_dir: Some("/new/container/Documents".into()),
            force_download_dir: true,
            ..Default::default()
        },
    );
    assert_eq!(fresh.download_dir, "/new/container/Documents");
    assert_eq!(fresh.device_id, old.device_id);
}

#[tokio::test(flavor = "multi_thread")]
async fn remembered_peers_survive_restart_and_forget_hides() {
    let phone = node("Phone").await;
    let mac = node("Mac").await;
    let link = crate::discovery::pair_link(&mac.st.device_info(), &[format!("127.0.0.1:{}", mac.st.port)]);
    crate::discovery::add_from_link(&phone.st, &link).await.unwrap();

    // "Khởi động lại": tạo AppState mới từ cùng thư mục dữ liệu.
    let restart = |st: &AppState| {
        AppState::new(
            Arc::new(|_, _| {}),
            st.data_dir.clone(),
            identity::load_or_create_identity(&st.data_dir).unwrap(),
            1,
            st.settings.read().unwrap().clone(),
        )
    };
    let again = restart(&phone.st);
    let peers = again.peer_list();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].info.name, "Mac");
    assert!(!peers[0].online, "máy đã lưu coi là tắt cho tới khi kiểm tra lại");

    // Quên: biến mất khỏi danh sách, kể cả sau khi khởi động lại.
    {
        let mut book = again.devices.lock().unwrap();
        book.forget(crate::devices::DeviceRef { id: peers[0].info.id.clone(), name: "Mac".into(), os: "mac".into() });
        book.save(&again.data_dir).unwrap();
    }
    assert!(again.peer_list().is_empty());
    assert!(restart(&again).peer_list().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn merges_addresses_so_unreachable_mdns_address_is_not_a_dead_end() {
    let a = node("Mac").await;
    let b = node("Android emulator").await;
    let info = b.st.device_info();
    // mDNS chỉ thấy địa chỉ mạng ảo không tới được...
    a.st.peers.lock().unwrap().insert(
        "mdns:emu".into(),
        Peer { info: info.clone(), addrs: vec!["10.255.255.1:9".into()], manual: false, online: true },
    );
    // ...còn người dùng thêm tay địa chỉ tới được (vd qua adb forward).
    a.st.peers.lock().unwrap().insert(
        format!("manual:{}", info.id),
        Peer { info: info.clone(), addrs: vec![format!("127.0.0.1:{}", b.st.port)], manual: true, online: true },
    );
    let list = a.st.peer_list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].addrs[0], format!("127.0.0.1:{}", b.st.port), "địa chỉ thêm tay thử trước");
    assert_eq!(list[0].addrs.len(), 2);
    transfer::send_text(a.st.clone(), list[0].clone(), "qua địa chỉ thêm tay".into()).await.unwrap();
}

/// Trình duyệt (không cài app) đăng nhập bằng PIN, gửi văn bản + file lên, tải file về.
#[tokio::test(flavor = "multi_thread")]
async fn browser_share_end_to_end() {
    let mut desk = node("Mac").await;
    crate::web::start(desk.st.clone()).await.unwrap();
    let (pin, port) = {
        let w = desk.st.web.lock().unwrap();
        let w = w.as_ref().unwrap();
        (w.pin.clone(), w.port)
    };
    let base = format!("http://127.0.0.1:{port}");
    let http = reqwest::Client::new();

    // Trang web phục vụ được, API khoá khi chưa nhập PIN.
    let page = http.get(&base).send().await.unwrap().text().await.unwrap();
    assert!(page.contains("BetterDrop"));
    assert_eq!(http.get(format!("{base}/api/state")).send().await.unwrap().status(), 401);

    // Sai PIN 5 lần thì bị khoá tạm, kể cả khi sau đó nhập đúng.
    let login = |p: String| {
        let http = http.clone();
        let base = base.clone();
        async move {
            http.post(format!("{base}/api/login"))
                .json(&serde_json::json!({ "pin": p }))
                .send()
                .await
                .unwrap()
        }
    };
    for _ in 0..5 {
        assert_eq!(login("000000x".into()).await.status(), 401);
    }
    assert_eq!(login(pin.clone()).await.status(), 429);
    // Mở khoá (giả lập hết thời gian khoá) rồi đăng nhập đúng.
    crate::web::renew_pin(&desk.st);
    let pin = desk.st.web.lock().unwrap().as_ref().unwrap().pin.clone();
    let ok = login(pin).await;
    assert_eq!(ok.status(), 200);
    let cookie = ok.headers()["set-cookie"].to_str().unwrap().split(';').next().unwrap().to_string();

    // Máy tính chia sẻ file + văn bản, trình duyệt tải về được.
    let src = tempfile::tempdir().unwrap();
    write(&src.path().join("Báo cáo.txt"), b"noi dung bao cao");
    let files = transfer::collect_files(&desk.st, &[src.path().join("Báo cáo.txt").to_string_lossy().into_owned()]).unwrap();
    crate::web::add_files(&desk.st, files);
    // Thêm lại cùng file thì không bị trùng.
    let again = transfer::collect_files(&desk.st, &[src.path().join("Báo cáo.txt").to_string_lossy().into_owned()]).unwrap();
    crate::web::add_files(&desk.st, again);
    assert_eq!(desk.st.web.lock().unwrap().as_ref().unwrap().files.len(), 1);
    crate::web::set_text(&desk.st, Some("link: https://example.com".into()));
    let state: serde_json::Value = http
        .get(format!("{base}/api/state"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(state["text"], "link: https://example.com");
    let id = state["files"][0]["id"].as_str().unwrap().to_string();
    let dl = http.get(format!("{base}/api/file/{id}")).header("cookie", &cookie).send().await.unwrap();
    assert!(dl.headers()["content-disposition"].to_str().unwrap().contains("B%C3%A1o"));
    assert_eq!(dl.bytes().await.unwrap().as_ref(), b"noi dung bao cao");
    // Tải tất cả: một file .zip chứa đủ file, giữ nguyên tên tiếng Việt.
    let more = transfer::collect_files(&desk.st, &[src.path().join("Báo cáo.txt").to_string_lossy().into_owned()]).unwrap();
    write(&src.path().join("Thư mục/ảnh.bin"), &vec![9u8; 300_000]);
    let folder = transfer::collect_files(&desk.st, &[src.path().join("Thư mục").to_string_lossy().into_owned()]).unwrap();
    crate::web::add_files(&desk.st, more);
    crate::web::add_files(&desk.st, folder);
    let zipped = http.get(format!("{base}/api/zip")).header("cookie", &cookie).send().await.unwrap();
    assert_eq!(zipped.headers()["content-type"], "application/zip");
    let bytes = zipped.bytes().await.unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
    let mut names: Vec<String> = (0..archive.len()).map(|i| archive.by_index(i).unwrap().name().unwrap().to_string()).collect();
    names.sort();
    assert_eq!(names, vec!["Báo cáo.txt".to_string(), "Thư mục/ảnh.bin".to_string()]);
    let mut content = Vec::new();
    std::io::Read::read_to_end(&mut archive.by_name("Thư mục/ảnh.bin").unwrap(), &mut content).unwrap();
    assert_eq!(content, vec![9u8; 300_000]);

    // Lượt tải được ghi vào Hoạt động với trạng thái Hoàn tất.
    let mut saw_done = false;
    while let Ok((ev, v)) = desk.events.try_recv() {
        if ev == "transfer-update" && v["peer_name"] == "trình duyệt · Báo cáo.txt" && v["status"] == "done" {
            saw_done = true;
        }
    }
    assert!(saw_done, "lượt tải phải hiện Hoàn tất");

    // Trình duyệt gửi văn bản lên.
    let r = http
        .post(format!("{base}/api/text"))
        .header("cookie", &cookie)
        .json(&serde_json::json!({ "name": "Lan", "text": "xin chào" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);

    // Trình duyệt gửi file lên: máy tính phải bấm Chấp nhận.
    let st = desk.st.clone();
    let accepter = tokio::spawn(async move {
        while let Some((ev, v)) = desk.events.recv().await {
            if ev == "incoming-text" {
                assert_eq!(v["sender_name"], "Lan (trình duyệt)");
            }
            if ev == "incoming-request" {
                let id = v["request_id"].as_str().unwrap().to_string();
                st.pending.lock().unwrap().remove(&id).unwrap().tx.send(true).unwrap();
                return desk;
            }
        }
        panic!("không có yêu cầu nhận");
    });
    let prep: crate::models::PrepareResponse = http
        .post(format!("{base}/api/prepare"))
        .header("cookie", &cookie)
        .json(&serde_json::json!({ "name": "Lan", "files": [{ "id": "f0", "rel_path": "ảnh.jpg", "size": 5 }] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let desk = accepter.await.unwrap();
    let up = http
        .post(format!(
            "{base}/api/upload?session={}&file=f0&token={}",
            prep.session_id, prep.tokens["f0"]
        ))
        .header("cookie", &cookie)
        .body("12345")
        .send()
        .await
        .unwrap();
    assert_eq!(up.status(), 200);
    let dl_dir = desk.st.settings.read().unwrap().download_dir.clone();
    assert_eq!(std::fs::read(Path::new(&dl_dir).join("ảnh.jpg")).unwrap(), b"12345");

    crate::web::stop(&desk.st);
}

/// Chạy tay để xem trang web: `cargo test web_preview -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn web_preview() {
    let desk = node("Mac-Studio").await;
    crate::web::start(desk.st.clone()).await.unwrap();
    let src = tempfile::tempdir().unwrap();
    write(&src.path().join("Báo cáo quý 3.pdf"), &vec![7u8; 1_234_567]);
    write(&src.path().join("ảnh chụp.png"), &vec![1u8; 52_000]);
    let files = transfer::collect_files(&desk.st, &[
        src.path().join("Báo cáo quý 3.pdf").to_string_lossy().into_owned(),
        src.path().join("ảnh chụp.png").to_string_lossy().into_owned(),
    ]).unwrap();
    crate::web::set_files(&desk.st, files);
    crate::web::set_text(&desk.st, Some("Link họp: https://meet.example.com/abc".into()));
    let (pin, port) = { let w = desk.st.web.lock().unwrap(); let w = w.as_ref().unwrap(); (w.pin.clone(), w.port) };
    println!("WEB_PREVIEW http://127.0.0.1:{port}/#pin={pin}");
    tokio::time::sleep(std::time::Duration::from_secs(600)).await;
}

// ---- Tương thích giữa các phiên bản ----
// Dữ liệu dưới đây là ĐÚNG định dạng bản v0.1.0 gửi/nhận. Nếu một thay đổi làm các
// test này hỏng, nghĩa là bản mới sẽ không nói chuyện được với máy đang dùng bản cũ.

/// /api/v1/info và các trường `sender` do bản v0.1.0 tạo ra (không có `version`).
const V010_DEVICE_INFO: &str = r#"{"id":"old-id","name":"Máy cũ","os":"windows","fp":"ab","port":53318}"#;

#[test]
fn new_version_reads_old_wire_format() {
    let info: crate::models::DeviceInfo = serde_json::from_str(V010_DEVICE_INFO).unwrap();
    assert_eq!(info.name, "Máy cũ");
    assert_eq!(info.version, "", "bản cũ không gửi phiên bản");

    let req: crate::models::PrepareRequest = serde_json::from_str(&format!(
        r#"{{"sender":{V010_DEVICE_INFO},"kind":"files","files":[{{"id":"f1","rel_path":"a/b.txt","size":3}}],"text":null}}"#
    ))
    .unwrap();
    assert_eq!(req.files[0].rel_path, "a/b.txt");

    let resp: crate::models::PrepareResponse =
        serde_json::from_str(r#"{"session_id":"s","tokens":{"f1":"t"}}"#).unwrap();
    assert_eq!(resp.tokens["f1"], "t");
}

#[test]
fn old_version_can_read_new_wire_format() {
    // Mọi trường bản v0.1.0 bắt buộc phải có, đúng tên và kiểu; trường mới chỉ được thêm.
    let info = serde_json::to_value(crate::models::DeviceInfo {
        id: "x".into(),
        name: "Mới".into(),
        os: "mac".into(),
        fp: "cd".into(),
        port: 1,
        version: crate::models::APP_VERSION.into(),
    })
    .unwrap();
    for key in ["id", "name", "os", "fp"] {
        assert!(info[key].is_string(), "thiếu/sai kiểu trường `{key}`");
    }
    assert!(info["port"].is_u64());
    let req = serde_json::to_value(crate::models::PrepareRequest {
        sender: serde_json::from_value(info).unwrap(),
        kind: crate::models::TransferKind::Files,
        files: vec![crate::models::FileMeta { id: "f".into(), rel_path: "x".into(), size: 1 }],
        text: None,
        group: None,
    })
    .unwrap();
    assert_eq!(req["kind"], "files");
    assert!(req["files"][0]["rel_path"].is_string() && req["files"][0]["size"].is_u64());
}

/// Một máy chạy bản cũ (gửi đúng JSON của v0.1.0) gửi văn bản và file tới bản mới.
#[tokio::test(flavor = "multi_thread")]
async fn old_client_can_send_to_new_version() {
    let mut new = node("Bản mới").await;
    let base = format!("127.0.0.1:{}", new.st.port);
    let http = crate::client::build_client(crate::client::Pin::Fingerprint(new.st.identity.fingerprint.clone()), None).unwrap(); // bản cũ: không trình chứng chỉ
    let url = |p: &str| crate::client::api_url(&base, p);

    // Bản cũ đọc /info của bản mới (có thêm `version`, bản cũ bỏ qua).
    let info: serde_json::Value = http.get(url("info")).send().await.unwrap().json().await.unwrap();
    assert_eq!(info["name"], "Bản mới");

    let text = format!(r#"{{"sender":{V010_DEVICE_INFO},"kind":"text","files":[],"text":"chào từ bản cũ"}}"#);
    let r = http.post(url("prepare")).header("content-type", "application/json").body(text).send().await.unwrap();
    assert_eq!(r.status(), 200);

    let st = new.st.clone();
    let accepter = tokio::spawn(async move {
        while let Some((ev, v)) = new.events.recv().await {
            if ev == "incoming-request" {
                let id = v["request_id"].as_str().unwrap().to_string();
                st.pending.lock().unwrap().remove(&id).unwrap().tx.send(true).unwrap();
                return new;
            }
        }
        panic!("không có yêu cầu nhận");
    });
    let files = format!(
        r#"{{"sender":{V010_DEVICE_INFO},"kind":"files","files":[{{"id":"f1","rel_path":"cũ.txt","size":3}}],"text":null}}"#
    );
    let prep: serde_json::Value = http
        .post(url("prepare"))
        .header("content-type", "application/json")
        .body(files)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let new = accepter.await.unwrap();
    let up = http
        .post(url("upload"))
        .query(&[
            ("session", prep["session_id"].as_str().unwrap()),
            ("file", "f1"),
            ("token", prep["tokens"]["f1"].as_str().unwrap()),
        ])
        .body("abc")
        .send()
        .await
        .unwrap();
    assert_eq!(up.status(), 200);
    let dir = new.st.settings.read().unwrap().download_dir.clone();
    assert_eq!(std::fs::read(Path::new(&dir).join("cũ.txt")).unwrap(), b"abc");
}

/// File cài đặt/sổ thiết bị do bản cũ ghi (thiếu các trường mới) vẫn đọc được.
#[test]
fn old_saved_files_still_load() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("settings.json"),
        r#"{"device_id":"d","name":"Máy","download_dir":"/tmp/x"}"#,
    )
    .unwrap();
    let s = identity::load_settings(dir.path(), identity::Defaults::default());
    assert_eq!(s.device_id, "d");
    assert!(s.receive_enabled, "mặc định bật nhận file");
    assert_eq!(s.language, "vi", "máy đã cài từ trước giữ tiếng Việt");

    std::fs::write(
        dir.path().join("devices.json"),
        format!(r#"{{"known":[{{"id":"old-id","name":"Máy cũ","os":"windows","fp":"ab","port":53318,"addrs":["1.2.3.4:53318"]}}]}}"#),
    )
    .unwrap();
    let book = crate::devices::DeviceBook::load(dir.path());
    assert_eq!(book.known.len(), 1);
    assert!(book.groups.is_empty());
}

/// Mỗi lượt chuyển kết thúc được ghi vào lịch sử ở cả hai đầu, kèm đường dẫn thật.
#[tokio::test(flavor = "multi_thread")]
async fn transfers_are_recorded_in_history() {
    let a = node("Mac").await;
    let b = node("Windows").await;
    let peer_b = as_peer(&b);
    let b_dir = PathBuf::from(b.st.settings.read().unwrap().download_dir.clone());
    write(&b_dir.join("x.txt"), b"old");
    let watcher = auto_respond(b, true);

    let src = tempfile::tempdir().unwrap();
    write(&src.path().join("x.txt"), b"new");
    let files = transfer::collect_files(&a.st, &[src.path().join("x.txt").to_string_lossy().into_owned()]).unwrap();
    // start_send_files gắn lịch sử như trong app thật.
    transfer::start_send_files(a.st.clone(), peer_b.clone(), files);
    let (_, b) = watcher.await.unwrap();
    for _ in 0..100 {
        if !a.st.history.lock().unwrap().list(10).is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let sent = a.st.history.lock().unwrap().list(10);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].direction, Direction::Send);
    assert_eq!(sent[0].status, TransferStatus::Done);
    assert_eq!(sent[0].peer_id.as_deref(), Some(peer_b.info.id.as_str()));
    assert_eq!(sent[0].files[0].path.as_deref(), Some(src.path().join("x.txt").to_str().unwrap()));

    let got = b.st.history.lock().unwrap().list(10);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].direction, Direction::Receive);
    assert_eq!(got[0].peer_id.as_deref(), Some(a.st.device_info().id.as_str()));
    // Đường dẫn là nơi file thật sự được lưu (đã đổi tên vì trùng).
    assert_eq!(got[0].files[0].path.as_deref(), Some(b_dir.join("x (1).txt").to_str().unwrap()));

    // Văn bản cũng được lưu, và vẫn còn sau khi "khởi động lại".
    transfer::send_text(a.st.clone(), peer_b, "ghi chú".into()).await.unwrap();
    let reloaded = crate::history::History::load(&a.st.data_dir).list(10);
    assert_eq!(reloaded.len(), 2);
    assert_eq!(reloaded[0].text.as_deref(), Some("ghi chú"));
}

// ---- Máy tin cậy ----

fn trusted_entry(st: &AppState) -> crate::devices::TrustedDevice {
    let me = st.device_info();
    crate::devices::TrustedDevice { id: me.id, name: me.name, os: me.os, fp: me.fp, added_at: 0 }
}

/// Máy đã được tin cậy gửi tới: nhận luôn, không hiện hộp thoại Chấp nhận.
#[tokio::test(flavor = "multi_thread")]
async fn trusted_device_is_auto_accepted() {
    let a = node("Mac").await;
    let mut b = node("Windows").await;
    b.st.devices.lock().unwrap().trust(trusted_entry(&a.st));

    let src = tempfile::tempdir().unwrap();
    write(&src.path().join("tin cậy.txt"), b"ok");
    let files = transfer::collect_files(&a.st, &[src.path().join("tin cậy.txt").to_string_lossy().into_owned()]).unwrap();
    let tracker = sender_tracker(&a.st);
    transfer::send_files(&a.st, &as_peer(&b), files, &tracker).await.unwrap();
    assert_eq!(tracker.status(), TransferStatus::Done);

    let mut saw_auto = false;
    while let Ok((ev, _)) = b.events.try_recv() {
        assert_ne!(ev, "incoming-request", "máy tin cậy không được bị hỏi");
        saw_auto |= ev == "auto-accepted";
    }
    assert!(saw_auto);
    let dir = b.st.settings.read().unwrap().download_dir.clone();
    assert_eq!(std::fs::read(Path::new(&dir).join("tin cậy.txt")).unwrap(), b"ok");
}

/// Máy lạ khai đúng id + dấu vân tay của máy tin cậy nhưng không có khoá bí mật của
/// nó (trình chứng chỉ khác): vẫn phải hỏi, và không được đề nghị "Luôn chấp nhận".
#[tokio::test(flavor = "multi_thread")]
async fn impostor_claiming_trusted_identity_is_still_asked() {
    let trusted = node("Mac thật").await;
    let impostor = node("Kẻ giả mạo").await;
    let mut b = node("Windows").await;
    b.st.devices.lock().unwrap().trust(trusted_entry(&trusted.st));

    let client = crate::client::build_client(
        crate::client::Pin::Fingerprint(b.st.identity.fingerprint.clone()),
        Some(&impostor.st.identity),
    )
    .unwrap();
    let body = serde_json::json!({
        "sender": trusted.st.device_info(),      // khai là máy tin cậy
        "kind": "files",
        "files": [{ "id": "f1", "rel_path": "x.txt", "size": 1 }],
    });
    let url = crate::client::api_url(&format!("127.0.0.1:{}", b.st.port), "prepare");
    let req = tokio::spawn(async move { client.post(url).json(&body).send().await.unwrap().status() });

    let request = loop {
        let (ev, v) = b.events.recv().await.unwrap();
        assert_ne!(ev, "auto-accepted", "kẻ giả mạo không được tự chấp nhận");
        if ev == "incoming-request" {
            break v;
        }
    };
    assert_eq!(request["can_trust"], false, "danh tính chưa được chứng minh");
    let id = request["request_id"].as_str().unwrap().to_string();
    let p = b.st.pending.lock().unwrap().remove(&id).unwrap();
    assert!(p.trust_candidate.is_none());
    p.tx.send(false).unwrap();
    assert_eq!(req.await.unwrap(), 403);
}

/// Máy chạy bản mới (có chứng chỉ) gửi lần đầu: được hỏi, kèm lựa chọn "Luôn chấp nhận".
#[tokio::test(flavor = "multi_thread")]
async fn proven_sender_can_be_trusted() {
    let a = node("Mac").await;
    let mut b = node("Windows").await;
    let src = tempfile::tempdir().unwrap();
    write(&src.path().join("x.txt"), b"x");
    let files = transfer::collect_files(&a.st, &[src.path().join("x.txt").to_string_lossy().into_owned()]).unwrap();
    let st_a = a.st.clone();
    let peer_b = as_peer(&b);
    let send = tokio::spawn(async move {
        let t = sender_tracker(&st_a);
        transfer::send_files(&st_a, &peer_b, files, &t).await.unwrap();
        t.status()
    });
    let request = loop {
        let (ev, v) = b.events.recv().await.unwrap();
        if ev == "incoming-request" {
            break v;
        }
    };
    assert_eq!(request["can_trust"], true);
    let id = request["request_id"].as_str().unwrap().to_string();
    let p = b.st.pending.lock().unwrap().remove(&id).unwrap();
    let candidate = p.trust_candidate.expect("có thể tin cậy");
    assert_eq!(candidate.id, a.st.device_info().id);
    assert_eq!(candidate.fp, a.st.identity.fingerprint);
    b.st.devices.lock().unwrap().trust(candidate);
    p.tx.send(true).unwrap();
    assert_eq!(send.await.unwrap(), TransferStatus::Done);
    assert!(b.st.devices.lock().unwrap().is_trusted(&a.st.device_info().id, &a.st.identity.fingerprint));
}

#[tokio::test(flavor = "multi_thread")]
async fn unreachable_iphone_gets_open_the_app_hint() {
    let a = node("Mac").await;
    let mut iphone = as_peer(&node("iPhone").await);
    iphone.info.os = "ios".into();
    iphone.addrs = vec!["127.0.0.1:1".into()]; // không ai nghe: như iPhone đã đưa app ra nền
    let err = transfer::send_text(a.st.clone(), iphone, "x".into()).await.unwrap_err();
    assert!(err.starts_with(transfer::IOS_HINT), "{err}");
}

// ---- Gửi tiếp khi bị ngắt ----

/// Proxy TCP giả lập Wi‑Fi rớt: kết nối đầu tiên bị cắt sau `cut_after` byte gửi đi;
/// các kết nối sau đi qua bình thường.
async fn flaky_proxy(target: u16, cut_after: usize) -> u16 {
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let cut_done = Arc::new(AtomicBool::new(false));
    tokio::spawn(async move {
        loop {
            let Ok((client, _)) = listener.accept().await else { return };
            let cut_done = cut_done.clone();
            tokio::spawn(async move {
                let server = tokio::net::TcpStream::connect(("127.0.0.1", target)).await.unwrap();
                let (mut cr, mut cw) = client.into_split();
                let (mut sr, mut sw) = server.into_split();
                let down = tokio::spawn(async move { let _ = tokio::io::copy(&mut sr, &mut cw).await; });
                let mut sent = 0usize;
                let mut buf = vec![0u8; 16 * 1024];
                loop {
                    let n = match cr.read(&mut buf).await { Ok(0) | Err(_) => break, Ok(n) => n };
                    sent += n;
                    if sent > cut_after && !cut_done.swap(true, Ordering::SeqCst) {
                        break; // rớt mạng: đóng cả hai chiều
                    }
                    if sw.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                down.abort();
            });
        }
    });
    port
}

#[tokio::test(flavor = "multi_thread")]
async fn resumes_after_connection_drop() {
    let a = node("Mac").await;
    let b = node("Windows").await;
    let mut peer_b = as_peer(&b);
    let proxy = flaky_proxy(b.st.port, 1_000_000).await;
    peer_b.addrs = vec![format!("127.0.0.1:{proxy}")];
    let b_dir = PathBuf::from(b.st.settings.read().unwrap().download_dir.clone());
    let watcher = auto_respond(b, true);

    let src = tempfile::tempdir().unwrap();
    let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 253) as u8).collect();
    write(&src.path().join("lớn.bin"), &data);
    let files = transfer::collect_files(&a.st, &[src.path().join("lớn.bin").to_string_lossy().into_owned()]).unwrap();
    let tracker = sender_tracker(&a.st);
    let started = std::time::Instant::now();
    transfer::send_files(&a.st, &peer_b, files, &tracker).await.unwrap();
    assert_eq!(tracker.status(), TransferStatus::Done);
    assert_eq!(tracker.snapshot().transferred, data.len() as u64, "tiến trình không bị cộng hai lần");
    // Đã thật sự bị cắt và thử lại (lần thử lại đầu chờ 2 giây), nhưng không phải gửi lại từ đầu.
    assert!(started.elapsed() >= std::time::Duration::from_secs(2), "proxy phải cắt kết nối");
    assert!(started.elapsed() < std::time::Duration::from_secs(30));

    let (finals, b) = watcher.await.unwrap();
    assert_eq!(finals, vec![TransferStatus::Done]);
    assert_eq!(std::fs::read(b_dir.join("lớn.bin")).unwrap(), data, "nội dung đúng từng byte sau khi gửi tiếp");
    let session = b.st.incoming.lock().unwrap().values().next().cloned().unwrap();
    assert_eq!(session.tracker.snapshot().transferred, data.len() as u64);
}

// ---- Chia sẻ nhóm ----

#[tokio::test(flavor = "multi_thread")]
async fn shares_a_group_with_another_device() {
    let a = node("Mac").await;
    let b = node("Windows").await;
    let c = node("Máy in phòng 2").await;
    let peer_b = as_peer(&b);
    let c_info = c.st.device_info();
    let group = crate::models::SharedGroup {
        name: "Phòng kế toán".into(),
        members: vec![
            crate::models::SharedMember {
                id: c_info.id.clone(),
                name: c_info.name.clone(),
                os: c_info.os.clone(),
                fp: c_info.fp.clone(),
                addrs: vec![format!("127.0.0.1:{}", c.st.port)],
            },
            // Chính máy nhận cũng nằm trong nhóm: phải được bỏ ra.
            crate::models::SharedMember { id: b.st.device_info().id, name: "Windows".into(), os: "windows".into(), fp: String::new(), addrs: vec![] },
        ],
    };
    let st_b = b.st.clone();
    let watcher = tokio::spawn(async move {
        let mut b = b;
        loop {
            let (ev, v) = b.events.recv().await.unwrap();
            if ev == "incoming-request" {
                assert_eq!(v["group_name"], "Phòng kế toán");
                assert_eq!(v["file_count"], 1, "số máy, không tính chính máy nhận");
                let id = v["request_id"].as_str().unwrap().to_string();
                st_b.pending.lock().unwrap().remove(&id).unwrap().tx.send(true).unwrap();
                return b;
            }
        }
    });
    transfer::share_group(a.st.clone(), peer_b, group).await.unwrap();
    let b = watcher.await.unwrap();

    let book = b.st.devices.lock().unwrap().clone();
    assert_eq!(book.groups.len(), 1);
    assert_eq!(book.groups[0].name, "Phòng kế toán");
    assert_eq!(book.groups[0].members.len(), 1);
    // Máy C được thêm vào để gửi được ngay, và gửi thử được thật.
    let c_peer = b.st.find_peer(&c_info.id).expect("máy trong nhóm được lưu lại");
    assert_eq!(c_peer.info.fp, c_info.fp);
    let mut c_events = c;
    transfer::send_text(b.st.clone(), c_peer, "chào phòng kế toán".into()).await.unwrap();
    loop {
        let (ev, v) = c_events.events.recv().await.unwrap();
        if ev == "incoming-text" {
            assert_eq!(v["text"], "chào phòng kế toán");
            break;
        }
    }
}

#[test]
fn group_request_is_rejected_by_old_format_parsers() {
    // Bản cũ chỉ biết "files" | "text": loại "group" làm hỏng việc đọc yêu cầu (422),
    // nên máy gửi báo cần cập nhật thay vì gửi nhầm thành file/văn bản.
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "lowercase")]
    #[allow(dead_code)]
    enum OldKind { Files, Text }
    assert!(serde_json::from_str::<OldKind>("\"group\"").is_err());
}

#[test]
fn shared_paths_come_from_command_line_args() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("Báo cáo.pdf");
    std::fs::write(&f, b"x").unwrap();
    let args = vec![
        "BetterDrop.exe".to_string(),
        "--hidden".to_string(),
        f.to_string_lossy().into_owned(),
        dir.path().to_string_lossy().into_owned(),
        "/không/tồn/tại".to_string(),
    ];
    assert_eq!(
        crate::paths_from_args(args),
        vec![f.to_string_lossy().into_owned(), dir.path().to_string_lossy().into_owned()]
    );
}

#[test]
fn shared_text_comes_from_share_url() {
    let parse = |s: &str| crate::shared_text_from_url(&tauri::Url::parse(s).unwrap());
    assert_eq!(
        parse("betterdrop://share?text=Xin%20ch%C3%A0o%0Ahttps%3A%2F%2Fvi.wikipedia.org%2Fwiki%2FH%C3%A0_N%E1%BB%99i%3Fa%3D1%26b%3D2"),
        Some("Xin chào\nhttps://vi.wikipedia.org/wiki/Hà_Nội?a=1&b=2".to_string())
    );
    assert_eq!(parse("betterdrop://share?text="), None);
    assert_eq!(parse("betterdrop://shared"), None);
    assert_eq!(parse("https://share?text=x"), None);
    assert_eq!(parse("file:///tmp/a.txt"), None);
}
