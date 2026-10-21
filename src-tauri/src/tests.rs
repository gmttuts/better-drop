//! Test đầu-cuối: hai "máy" chạy trên localhost, gửi file và thư mục qua HTTPS thật.

use crate::models::*;
use crate::state::{AppState, EmitFn, Tracker};
use crate::{identity, server, transfer};
use std::path::Path;
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
                    if let Some(tx) = rx.st.pending.lock().unwrap().remove(&id) {
                        tx.send(accept).unwrap();
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
                st.pending.lock().unwrap().remove(&id).unwrap().send(true).unwrap();
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
