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
fn update_version_compare() {
    use crate::updater::is_newer;
    assert!(is_newer("0.3.7", "0.3.6"));
    assert!(is_newer("v0.4.0", "0.3.10"));
    assert!(is_newer("0.3.10", "0.3.9"));
    assert!(is_newer("1.0", "0.9.9"));
    assert!(!is_newer("0.3.6", "0.3.6"));
    assert!(!is_newer("v0.3.3", "0.3.6"));
    assert!(!is_newer("0.3.6-beta", "0.3.6"));
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

// ---- Bảo mật (xem docs/BAO-MAT.md) ----

/// Gửi JSON thô tới /api/v1/... của `to`, trình chứng chỉ của `me` (None: như bản cũ).
fn raw_client(to: &Node, me: Option<&Node>) -> (reqwest::Client, String) {
    let http = crate::client::build_client(
        crate::client::Pin::Fingerprint(to.st.identity.fingerprint.clone()),
        me.map(|n| &n.st.identity),
    )
    .unwrap();
    (http, format!("127.0.0.1:{}", to.st.port))
}

/// Bấm Chấp nhận/Từ chối cho `n` yêu cầu tiếp theo; trả về số yêu cầu đã thấy.
fn answer_requests(mut n: Node, accept: bool, count: usize) -> tokio::task::JoinHandle<(usize, Node)> {
    tokio::spawn(async move {
        let mut seen = 0;
        while seen < count {
            let Some((ev, v)) = n.events.recv().await else { break };
            if ev == "incoming-request" {
                seen += 1;
                let id = v["request_id"].as_str().unwrap().to_string();
                if let Some(p) = n.st.pending.lock().unwrap().remove(&id) {
                    let _ = p.tx.send(accept);
                }
            }
        }
        (seen, n)
    })
}

/// Máy lạ khai id của một máy đã lưu (kèm chứng chỉ của chính nó) bằng một đoạn văn
/// bản — không cần ai bấm Chấp nhận. Trước đây máy nhận ghi đè địa chỉ + dấu vân tay
/// của máy đã lưu, nên lần sau gửi tới "Máy A" thì file đi tới máy lạ.
#[tokio::test(flavor = "multi_thread")]
async fn impostor_cannot_hijack_a_saved_device() {
    let real = node("Máy A").await;
    let impostor = node("Kẻ giả mạo").await;
    let b = node("Windows").await;
    let saved = Peer { manual: true, ..as_peer(&real) };
    b.st.remember_peer(saved.clone());

    let (http, base) = raw_client(&b, Some(&impostor));
    let mut fake = impostor.st.device_info();
    fake.id = real.st.device_info().id;
    fake.name = "Máy A".into();
    let body = serde_json::json!({ "sender": fake, "kind": "text", "files": [], "text": "hi" });
    let r = http.post(crate::client::api_url(&base, "prepare")).json(&body).send().await.unwrap();
    assert_eq!(r.status(), 200);

    let p = b.st.find_peer(&saved.info.id).unwrap();
    assert_eq!(p.info.fp, real.st.identity.fingerprint, "dấu vân tay của máy đã lưu bị ghi đè");
    assert_eq!(p.addrs, saved.addrs, "địa chỉ của máy đã lưu bị ghi đè");
    let book = b.st.devices.lock().unwrap();
    assert_eq!(book.fp_of(&saved.info.id), Some(real.st.identity.fingerprint.as_str()));
}

/// Kích thước khổng lồ cộng lại bị tràn số thành vài byte trong hộp thoại.
#[tokio::test(flavor = "multi_thread")]
async fn overflowing_total_size_is_rejected_before_asking() {
    let a = node("Mac").await;
    let mut b = node("Windows").await;
    let (http, base) = raw_client(&b, Some(&a));
    let body = serde_json::json!({
        "sender": a.st.device_info(), "kind": "files",
        "files": [{ "id": "f1", "rel_path": "a", "size": u64::MAX }, { "id": "f2", "rel_path": "b", "size": 1025 }],
    });
    let r = http.post(crate::client::api_url(&base, "prepare")).json(&body).send().await.unwrap();
    assert_eq!(r.status(), 400);
    while let Ok((ev, _)) = b.events.try_recv() {
        assert_ne!(ev, "incoming-request");
    }
}

/// Gửi dồn dập /prepare: chỉ MAX_PENDING_PER_IP hộp thoại được mở từ một máy,
/// phần còn lại bị trả 429 ngay (không giữ kết nối, không hiện hộp thoại).
#[tokio::test(flavor = "multi_thread")]
async fn prepare_flood_is_limited() {
    let a = node("Mac").await;
    let b = node("Windows").await;
    let (http, base) = raw_client(&b, Some(&a));
    let body = serde_json::json!({
        "sender": a.st.device_info(), "kind": "files",
        "files": [{ "id": "f1", "rel_path": "spam.txt", "size": 1 }],
    });
    let url = crate::client::api_url(&base, "prepare");
    let mut waiting = vec![];
    for _ in 0..server::MAX_PENDING_PER_IP {
        let (http, url, body) = (http.clone(), url.clone(), body.clone());
        waiting.push(tokio::spawn(async move { http.post(url).json(&body).send().await.unwrap().status() }));
    }
    // Đợi các yêu cầu trên vào hàng chờ.
    for _ in 0..100 {
        if b.st.pending.lock().unwrap().len() == server::MAX_PENDING_PER_IP {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(b.st.pending.lock().unwrap().len(), server::MAX_PENDING_PER_IP);
    for _ in 0..5 {
        assert_eq!(http.post(&url).json(&body).send().await.unwrap().status(), 429);
    }
    let answer = answer_requests(b, false, server::MAX_PENDING_PER_IP);
    for w in waiting {
        assert_eq!(w.await.unwrap(), 403);
    }
    let (seen, b) = answer.await.unwrap();
    assert_eq!(seen, server::MAX_PENDING_PER_IP, "chỉ được hiện đúng số hộp thoại cho phép");
    // Trả lời xong thì được gửi tiếp như thường.
    let answer = answer_requests(b, false, 1);
    assert_eq!(http.post(&url).json(&body).send().await.unwrap().status(), 403);
    answer.await.unwrap();
}

/// Văn bản hiện ngay không cần hỏi: giới hạn độ dài và số lần gửi.
#[tokio::test(flavor = "multi_thread")]
async fn text_flood_and_huge_text_are_limited() {
    let a = node("Mac").await;
    let b = node("Windows").await;
    let (http, base) = raw_client(&b, Some(&a));
    let url = crate::client::api_url(&base, "prepare");
    let text = |t: String| serde_json::json!({ "sender": a.st.device_info(), "kind": "text", "files": [], "text": t });
    let huge = http.post(&url).json(&text("x".repeat(server::MAX_TEXT_BYTES + 1))).send().await.unwrap();
    assert_eq!(huge.status(), 413);
    for i in 0..server::MAX_TEXTS_PER_WINDOW {
        assert_eq!(http.post(&url).json(&text(format!("tin {i}"))).send().await.unwrap().status(), 200);
    }
    assert_eq!(http.post(&url).json(&text("thêm".into())).send().await.unwrap().status(), 429);
    // Tên máy gửi chứa ký tự đảo chiều chữ / quá dài bị làm sạch trước khi hiện lên.
    let c = node("Linux").await;
    let mut c = c;
    let (http, base) = raw_client(&c, Some(&a));
    let mut sender = a.st.device_info();
    sender.name = format!("Máy\u{202E}A{}", "x".repeat(1000));
    let body = serde_json::json!({ "sender": sender, "kind": "text", "files": [], "text": "hi" });
    http.post(crate::client::api_url(&base, "prepare")).json(&body).send().await.unwrap();
    let shown = loop {
        let (ev, v) = c.events.recv().await.unwrap();
        if ev == "incoming-text" {
            break v["sender_name"].as_str().unwrap().to_string();
        }
    };
    assert!(!shown.contains('\u{202E}'));
    assert!(shown.chars().count() <= 64, "{}", shown.len());
}

/// Upload sai token, offset vượt kích thước, ghi quá kích thước đã khai, và dùng lại
/// token sau khi phiên đã lỗi/huỷ: đều bị từ chối, không để lại file.
#[tokio::test(flavor = "multi_thread")]
async fn upload_abuse_is_refused() {
    let a = node("Mac").await;
    let b = node("Windows").await;
    let dl = PathBuf::from(b.st.settings.read().unwrap().download_dir.clone());
    let (http, base) = raw_client(&b, Some(&a));
    let answer = answer_requests(b, true, 2);
    let prepare = || async {
        let body = serde_json::json!({
            "sender": a.st.device_info(), "kind": "files",
            "files": [{ "id": "f1", "rel_path": "a.txt", "size": 5 }, { "id": "f2", "rel_path": "b.txt", "size": 3 }],
        });
        let r = http.post(crate::client::api_url(&base, "prepare")).json(&body).send().await.unwrap();
        assert_eq!(r.status(), 200);
        r.json::<PrepareResponse>().await.unwrap()
    };
    let upload = |p: &PrepareResponse, file: &str, token: &str, offset: Option<u64>, body: &'static [u8]| {
        let mut q = vec![("session", p.session_id.clone()), ("file", file.to_string()), ("token", token.to_string())];
        if let Some(o) = offset {
            q.push(("offset", o.to_string()));
        }
        let req = http.post(crate::client::api_url(&base, "upload")).query(&q).body(body);
        async move { req.send().await.unwrap().status() }
    };

    let p = prepare().await;
    let t1 = p.tokens["f1"].clone();
    let t2 = p.tokens["f2"].clone();
    // Token của file khác / token sai.
    assert_eq!(upload(&p, "f1", &t2, None, b"12345").await, 403);
    assert_eq!(upload(&p, "f1", "", None, b"12345").await, 403);
    // Offset lớn hơn phần đã nhận / lớn hơn kích thước file.
    assert_eq!(upload(&p, "f1", &t1, Some(3), b"45").await, 409);
    assert_eq!(upload(&p, "f1", &t1, Some(u64::MAX), b"").await, 409);
    // Ghi nhiều hơn kích thước đã khai: lỗi, không lưu gì.
    assert_eq!(upload(&p, "f1", &t1, None, b"123456").await, 500);
    // Phiên đã lỗi: token cũ không dùng tiếp được.
    assert_eq!(upload(&p, "f2", &t2, None, b"abc").await, 410);
    assert_eq!(upload(&p, "f1", &t1, None, b"12345").await, 410);
    assert!(!dl.join("a.txt").exists() && !dl.join("b.txt").exists());

    // Phiên bị máy gửi huỷ: token không dùng lại được.
    let p = prepare().await;
    let r = http
        .post(crate::client::api_url(&base, "cancel"))
        .query(&[("session", &p.session_id)])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(upload(&p, "f1", &p.tokens["f1"], None, b"12345").await, 410);
    assert!(!dl.join("a.txt").exists());
    let (_, _b) = answer.await.unwrap(); // giữ thư mục tạm của máy nhận
    // Không còn file tạm nào sót lại.
    let left: Vec<_> = std::fs::read_dir(&dl).unwrap().flatten().map(|e| e.file_name()).collect();
    assert!(left.is_empty(), "{left:?}");
}

/// Tên file độc từ máy gửi (đi ngược thư mục, đường dẫn tuyệt đối, tên thiết bị
/// Windows, symlink sẵn có trong thư mục nhận): luôn lưu bên trong thư mục nhận.
#[tokio::test(flavor = "multi_thread")]
async fn malicious_names_stay_inside_download_dir() {
    let a = node("Mac").await;
    let b = node("Windows").await;
    let dl = PathBuf::from(b.st.settings.read().unwrap().download_dir.clone());
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(&dl).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), dl.join("Photos")).unwrap();
    let (http, base) = raw_client(&b, Some(&a));
    let answer = answer_requests(b, true, 1);
    let names = ["../../thoát.txt", "/etc/passwd", "..\\..\\win.ini", "Photos/a.txt", "CON", "x/../../../y.txt"];
    let files: Vec<_> = names
        .iter()
        .enumerate()
        .map(|(i, n)| serde_json::json!({ "id": format!("f{i}"), "rel_path": n, "size": 1 }))
        .collect();
    let body = serde_json::json!({ "sender": a.st.device_info(), "kind": "files", "files": files });
    let p: PrepareResponse = http
        .post(crate::client::api_url(&base, "prepare"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    for i in 0..names.len() {
        let id = format!("f{i}");
        let r = http
            .post(crate::client::api_url(&base, "upload"))
            .query(&[("session", p.session_id.as_str()), ("file", &id), ("token", &p.tokens[&id])])
            .body("z")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200, "{}", names[i]);
    }
    let (_, _b) = answer.await.unwrap(); // giữ thư mục tạm của máy nhận
    assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none(), "ghi xuyên qua symlink ra ngoài");
    let saved: Vec<PathBuf> = walkdir::WalkDir::new(&dl)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();
    assert_eq!(saved.len(), names.len(), "{saved:?} {:?}", std::fs::read_dir(&dl).unwrap().flatten().map(|e| e.path()).collect::<Vec<_>>());
    for f in &saved {
        assert!(f.starts_with(&dl), "{f:?}");
    }
    assert!(dl.join("_CON").exists());
    assert!(dl.join("thoát.txt").exists());
}

/// Trang web: chặn DNS rebinding (Host lạ) và có các header chống XSS/nhúng khung.
#[tokio::test(flavor = "multi_thread")]
async fn web_page_has_security_headers_and_checks_host() {
    let desk = node("Mac").await;
    crate::web::start(desk.st.clone()).await.unwrap();
    let port = desk.st.web.lock().unwrap().as_ref().unwrap().port;
    let http = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");

    let r = http.get(&base).header("host", format!("rebind.attacker.example:{port}")).send().await.unwrap();
    assert_eq!(r.status(), 421);
    let r = http
        .post(format!("{base}/api/login"))
        .header("host", "rebind.attacker.example")
        .json(&serde_json::json!({ "pin": "000000" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 421);

    let r = http.get(&base).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let h = r.headers().clone();
    assert_eq!(h["x-content-type-options"], "nosniff");
    assert_eq!(h["x-frame-options"], "DENY");
    let csp = h["content-security-policy"].to_str().unwrap().to_string();
    assert!(csp.contains("frame-ancestors 'none'") && !csp.contains("unsafe-eval"), "{csp}");
    // Băm của đoạn script trong trang phải khớp, không thì trang không chạy.
    use sha2::Digest;
    let page = r.text().await.unwrap();
    let script = page.split_once("<script>").unwrap().1.split_once("</script>").unwrap().0;
    let hash = crate::web::base64(&sha2::Sha256::digest(script.as_bytes()));
    assert!(csp.contains(&format!("script-src 'sha256-{hash}'")), "{csp}");
    assert_eq!(page.matches("<script").count(), 1, "chỉ được có một thẻ script (CSP băm đúng một đoạn)");
    crate::web::stop(&desk.st);
}

/// Nhiều file trùng tên tới cùng lúc: mỗi file một tên riêng, không file nào bị ghi đè.
/// (Chọn tên + đổi tên chạy trên luồng blocking, vẫn phải nguyên tử.)
#[tokio::test(flavor = "multi_thread")]
async fn same_name_files_received_concurrently_get_unique_names() {
    use futures_util::StreamExt;
    let node = node("B").await;
    let st = node.st.clone();
    let metas: Vec<FileMeta> =
        (0..40).map(|i| FileMeta { id: i.to_string(), rel_path: "trùng.txt".into(), size: 4 }).collect();
    let trust = server::Trust { auto: true, candidate: None };
    let prep = server::accept_files(&st, "A", None, "macos", &metas, trust).await.unwrap();
    futures_util::stream::iter(metas)
        .for_each_concurrent(16, |m| {
            let st = st.clone();
            let q = UploadQuery {
                session: prep.session_id.clone(),
                token: prep.tokens[&m.id].clone(),
                file: m.id.clone(),
                offset: None,
            };
            async move {
                let body = axum::body::Body::from(format!("{:>4}", m.id));
                server::receive_upload(&st, q, body).await.unwrap();
            }
        })
        .await;
    let dir = PathBuf::from(st.settings.read().unwrap().download_dir.clone());
    let mut got: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap().trim().to_string())
        .collect();
    got.sort_by_key(|s| s.parse::<u32>().unwrap());
    assert_eq!(got, (0..40).map(|i| i.to_string()).collect::<Vec<_>>());
    assert!(dir.join("trùng.txt").exists() && dir.join("trùng (39).txt").exists());
}

// ---- Đo tốc độ (chạy tay: `cargo test --release -- --ignored bench_ --nocapture --test-threads=1`) ----

/// Proxy TCP thêm độ trễ `rtt` (chia đều hai chiều) như Wi‑Fi thật, không giới hạn băng thông.
async fn latency_proxy(target: u16, rtt: std::time::Duration) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let delay = rtt / 2;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    fn pipe(mut r: tokio::net::tcp::OwnedReadHalf, mut w: tokio::net::tcp::OwnedWriteHalf, delay: std::time::Duration) {
        let (tx, mut rx) = mpsc::unbounded_channel::<(tokio::time::Instant, Vec<u8>)>();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                let n = match r.read(&mut buf).await { Ok(0) | Err(_) => break, Ok(n) => n };
                if tx.send((tokio::time::Instant::now() + delay, buf[..n].to_vec())).is_err() {
                    break;
                }
            }
        });
        tokio::spawn(async move {
            while let Some((at, data)) = rx.recv().await {
                tokio::time::sleep_until(at).await;
                if w.write_all(&data).await.is_err() {
                    break;
                }
            }
        });
    }
    tokio::spawn(async move {
        loop {
            let Ok((client, _)) = listener.accept().await else { return };
            client.set_nodelay(true).unwrap();
            let server = tokio::net::TcpStream::connect(("127.0.0.1", target)).await.unwrap();
            server.set_nodelay(true).unwrap();
            let (cr, cw) = client.into_split();
            let (sr, sw) = server.into_split();
            pipe(cr, sw, delay);
            pipe(sr, cw, delay);
        }
    });
    port
}

/// Đếm số lần báo tiến trình ("transfer-update") còn trong hàng đợi sự kiện.
fn count_updates(rx: &mut mpsc::UnboundedReceiver<(&'static str, serde_json::Value)>) -> usize {
    std::iter::from_fn(|| rx.try_recv().ok()).filter(|(e, _)| *e == "transfer-update").count()
}

/// Gửi `paths` từ máy A sang máy B qua HTTPS trên localhost (thêm độ trễ `rtt` nếu có),
/// in MB/s, file/giây và số lần mỗi bên báo tiến trình lên giao diện.
async fn bench_send(label: &str, paths: Vec<String>, rtt: Option<std::time::Duration>) {
    let mut a = node("A").await;
    let mut b = node("B").await;
    let mut peer_b = as_peer(&b);
    if let Some(rtt) = rtt {
        peer_b.addrs = vec![format!("127.0.0.1:{}", latency_proxy(b.st.port, rtt).await)];
    }
    let b_dir = PathBuf::from(b.st.settings.read().unwrap().download_dir.clone());
    // Bên nhận tự chấp nhận và đếm số lần báo tiến trình cho tới khi xong.
    let watcher = tokio::spawn(async move {
        let mut updates = 0usize;
        while let Some((ev, v)) = b.events.recv().await {
            if ev == "incoming-request" {
                let id = v["request_id"].as_str().unwrap().to_string();
                if let Some(p) = b.st.pending.lock().unwrap().remove(&id) {
                    p.tx.send(true).unwrap();
                }
            } else if ev == "transfer-update" {
                updates += 1;
                if v["status"] == "done" {
                    return updates;
                }
                assert!(v["status"] != "error" && v["status"] != "cancelled", "{v}");
            }
        }
        panic!("bên nhận dừng giữa chừng");
    });

    let files = transfer::collect_files(&a.st, &paths).unwrap();
    let count = files.len();
    let bytes: u64 = files.iter().map(|f| f.meta.size).sum();
    let tracker = sender_tracker(&a.st);
    let started = std::time::Instant::now();
    transfer::send_files(&a.st, &peer_b, files, &tracker).await.unwrap();
    let recv_updates = watcher.await.unwrap();
    let secs = started.elapsed().as_secs_f64();
    assert_eq!(tracker.status(), TransferStatus::Done);
    assert_eq!(tracker.snapshot().transferred, bytes);
    let sent_updates = count_updates(&mut a.events);

    let (mut got_files, mut got_bytes) = (0usize, 0u64);
    for e in walkdir::WalkDir::new(&b_dir).into_iter().filter_map(Result::ok) {
        if e.file_type().is_file() {
            got_files += 1;
            got_bytes += e.metadata().unwrap().len();
        }
    }
    assert_eq!((got_files, got_bytes), (count, bytes));
    let label = match rtt {
        Some(r) => format!("{label}, RTT {} ms", r.as_millis()),
        None => label.to_string(),
    };
    println!(
        "BENCH {label}: {count} file, {:.1} MB trong {secs:.2} s -> {:.1} MB/s, {:.0} file/s, \
         báo tiến trình: gửi {sent_updates}, nhận {recv_updates}",
        bytes as f64 / 1e6,
        bytes as f64 / 1e6 / secs,
        count as f64 / secs
    );
}

/// Độ trễ khứ hồi điển hình của Wi‑Fi trong nhà.
const BENCH_RTT: std::time::Duration = std::time::Duration::from_millis(5);

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn bench_one_2gb_file() {
    let src = tempfile::tempdir().unwrap();
    let p = src.path().join("lớn.bin");
    // File thưa: không tốn đĩa bên gửi (bên nhận vẫn ghi đủ 2 GB, xoá khi test xong).
    std::fs::File::create(&p).unwrap().set_len(2 << 30).unwrap();
    bench_send("1 file 2 GB", vec![p.to_string_lossy().into_owned()], None).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn bench_5000_small_files() {
    let src = tempfile::tempdir().unwrap();
    let root = src.path().join("nhỏ");
    let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
    for i in 0..5000 {
        write(&root.join(format!("{}/{}/f{i}.txt", i % 10, i % 7)), &data);
    }
    let paths = vec![root.to_string_lossy().into_owned()];
    bench_send("5000 file 10 KB", paths.clone(), None).await;
    bench_send("5000 file 10 KB", paths, Some(BENCH_RTT)).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn bench_200_files_5mb() {
    let src = tempfile::tempdir().unwrap();
    let root = src.path().join("vừa");
    let data: Vec<u8> = (0..5_000_000u32).map(|i| (i % 251) as u8).collect();
    for i in 0..200 {
        write(&root.join(format!("{}/f{i}.bin", i % 4)), &data);
    }
    let paths = vec![root.to_string_lossy().into_owned()];
    bench_send("200 file 5 MB", paths.clone(), None).await;
    bench_send("200 file 5 MB", paths, Some(BENCH_RTT)).await;
}

/// Chỉ đo phía nhận: gọi thẳng `receive_upload` (không TLS, không bên gửi), `parallel`
/// file cùng lúc, body chia chunk 16 KB như hyper giao từ TLS.
async fn bench_receive(label: &str, count: usize, size: usize, parallel: usize) {
    use futures_util::StreamExt;
    let node = node("B").await;
    let st = node.st.clone();
    let metas: Vec<FileMeta> = (0..count)
        .map(|i| FileMeta { id: i.to_string(), rel_path: format!("thư mục/{i}.bin"), size: size as u64 })
        .collect();
    let trust = server::Trust { auto: true, candidate: None };
    let prep = server::accept_files(&st, "A", None, "macos", &metas, trust).await.unwrap();
    let data = axum::body::Bytes::from((0..size).map(|i| (i % 251) as u8).collect::<Vec<u8>>());
    let started = std::time::Instant::now();
    futures_util::stream::iter(metas)
        .for_each_concurrent(parallel, |m| {
            let (st, data) = (st.clone(), data.clone());
            let q = UploadQuery {
                session: prep.session_id.clone(),
                token: prep.tokens[&m.id].clone(),
                file: m.id,
                offset: None,
            };
            async move {
                let chunks: Vec<Result<axum::body::Bytes, std::io::Error>> =
                    (0..size).step_by(16 << 10).map(|o| Ok(data.slice(o..(o + (16 << 10)).min(size)))).collect();
                let body = axum::body::Body::from_stream(futures_util::stream::iter(chunks));
                server::receive_upload(&st, q, body).await.unwrap();
            }
        })
        .await;
    let secs = started.elapsed().as_secs_f64();
    let session = st.incoming.lock().unwrap().values().next().cloned().unwrap();
    assert_eq!(session.tracker.status(), TransferStatus::Done);
    let mb = (count * size) as f64 / 1e6;
    println!(
        "BENCH nhận {label}: {count} file trong {secs:.2} s -> {:.0} file/s, {:.0} MB/s",
        count as f64 / secs,
        mb / secs
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn bench_receive_small() {
    for _ in 0..3 {
        bench_receive("5000 file 10 KB", 5000, 10_000, 16).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn bench_receive_large() {
    for _ in 0..2 {
        bench_receive("200 file 5 MB", 200, 5_000_000, 3).await;
    }
}
