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

    let files = transfer::collect_files(&[
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
    let files = transfer::collect_files(&[src.path().join("x.txt").to_string_lossy().into_owned()]).unwrap();
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
    let files = transfer::collect_files(&[src.path().join("x.txt").to_string_lossy().into_owned()]).unwrap();
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
