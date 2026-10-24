//! Test đầu-cuối "Gửi qua Internet" với máy chủ giả lập storage.to và temp.sh (làm theo tài liệu
//! API của hai dịch vụ): gửi file/thư mục, chia phần, gửi lại phần bị lỗi, tải về và giải mã.

use super::providers::{test_base, Provider};
use super::*;
use axum::body::Bytes;
use axum::extract::{Path as AxPath, Query, State};
use axum::http::StatusCode;
use axum::routing::{post, put};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use tokio::sync::mpsc;

/// Phần nhỏ để thử chia phần mà không cần file lớn.
const PART: usize = 64 * 1024;
const SINGLE_MAX: u64 = 100 * 1024;

#[derive(Default)]
struct Mock {
    /// r2_key -> phần (số thứ tự -> dữ liệu)
    parts: HashMap<String, HashMap<u64, Vec<u8>>>,
    /// mã file -> dữ liệu đã lưu
    files: HashMap<String, Vec<u8>>,
    /// Lần PUT phần 2 đầu tiên trả lỗi 500 (thử gửi lại).
    fail_once: bool,
    visitor: Option<String>,
    expiry_days: Option<u64>,
}

type Shared = Arc<Mutex<Mock>>;

async fn init(State(m): State<Shared>, headers: axum::http::HeaderMap, Json(b): Json<Value>) -> Json<Value> {
    let size = b["size"].as_u64().unwrap();
    assert_eq!(b["filename"], "betterdrop.bdrop", "không lộ tên thật");
    m.lock().unwrap().visitor = headers.get("x-visitor-token").map(|v| v.to_str().unwrap().to_string());
    let key = uuid::Uuid::new_v4().simple().to_string();
    if size <= SINGLE_MAX {
        return Json(json!({ "success": true, "type": "single", "upload_url": format!("{}/r2/{key}?part=1", base()), "r2_key": key }));
    }
    let total = (size as usize).div_ceil(PART);
    // Chỉ cấp sẵn URL cho 2 phần đầu: phần sau phải xin thêm qua /upload/parts.
    let urls: HashMap<String, String> =
        (1..=total.min(2)).map(|n| (n.to_string(), format!("{}/r2/{key}?part={n}", base()))).collect();
    Json(json!({
        "success": true, "type": "multipart", "upload_id": key, "r2_key": key,
        "part_size": PART, "total_parts": total, "initial_urls": urls, "owner_token": "owner_v1_test"
    }))
}

fn base() -> String {
    test_base::get().unwrap()
}

#[derive(serde::Deserialize)]
struct PartQ {
    part: u64,
}

async fn put_part(
    State(m): State<Shared>,
    AxPath(key): AxPath<String>,
    Query(q): Query<PartQ>,
    body: Bytes,
) -> (StatusCode, [(String, String); 1]) {
    let mut m = m.lock().unwrap();
    if q.part == 2 && m.fail_once {
        m.fail_once = false;
        return (StatusCode::INTERNAL_SERVER_ERROR, [("x".into(), "y".into())]);
    }
    m.parts.entry(key).or_default().insert(q.part, body.to_vec());
    (StatusCode::OK, [("etag".into(), format!("\"etag-{}\"", q.part))])
}

async fn part_urls(Json(b): Json<Value>) -> Json<Value> {
    let key = b["upload_id"].as_str().unwrap().to_string();
    let urls: Vec<Value> = b["part_numbers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| json!({ "partNumber": n, "url": format!("{}/r2/{key}?part={n}", base()) }))
        .collect();
    Json(json!({ "success": true, "part_urls": urls }))
}

async fn complete(State(m): State<Shared>, headers: axum::http::HeaderMap, Json(b): Json<Value>) -> Json<Value> {
    assert_eq!(headers.get("x-owner-token").unwrap(), "owner_v1_test");
    let key = b["upload_id"].as_str().unwrap();
    let parts = b["parts"].as_array().unwrap();
    for (i, p) in parts.iter().enumerate() {
        assert_eq!(p["partNumber"], i as u64 + 1);
        assert_eq!(p["etag"], format!("\"etag-{}\"", i + 1));
    }
    let m = m.lock().unwrap();
    let stored = m.parts.get(key).unwrap();
    assert_eq!(stored.len(), parts.len());
    Json(json!({ "success": true }))
}

async fn confirm(State(m): State<Shared>, Json(b): Json<Value>) -> Json<Value> {
    let key = b["r2_key"].as_str().unwrap();
    let mut m = m.lock().unwrap();
    m.expiry_days = b["expiry_days"].as_u64();
    let parts = m.parts.remove(key).unwrap();
    let mut data = Vec::new();
    for n in 1..=parts.len() as u64 {
        data.extend_from_slice(&parts[&n]);
    }
    assert_eq!(data.len() as u64, b["size"].as_u64().unwrap());
    let id = format!("F{}", &key[..8]);
    m.files.insert(id.clone(), data);
    Json(json!({ "success": true, "file": { "id": id, "url": format!("https://storage.to/{id}"), "expires_at": "2030-01-08T00:00:00Z" } }))
}

async fn temp_upload(State(m): State<Shared>, headers: axum::http::HeaderMap, body: Bytes) -> String {
    // Tách phần "file" trong multipart/form-data (đủ cho một phần duy nhất).
    let ct = headers["content-type"].to_str().unwrap();
    let boundary = ct.split("boundary=").nth(1).unwrap();
    let head_end = body.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let head = String::from_utf8_lossy(&body[..head_end]);
    assert!(head.contains("name=\"file\""));
    assert!(head.contains("filename=\"betterdrop.bdrop\""));
    let tail = format!("\r\n--{boundary}--\r\n");
    assert!(body.ends_with(tail.as_bytes()));
    let data = body[head_end..body.len() - tail.len()].to_vec();
    m.lock().unwrap().files.insert("TsAbC".into(), data);
    "https://temp.sh/TsAbC/betterdrop.bdrop\n".into()
}

async fn temp_download(State(m): State<Shared>, AxPath((code, _name)): AxPath<(String, String)>) -> (StatusCode, Vec<u8>) {
    match m.lock().unwrap().files.get(&code) {
        Some(d) => (StatusCode::OK, d.clone()),
        None => (StatusCode::NOT_FOUND, vec![]),
    }
}

async fn mock_server(m: Shared) -> String {
    let app = Router::new()
        .route("/api/upload/init", post(init))
        .route("/api/upload/parts", post(part_urls))
        .route("/api/upload/complete-multipart", post(complete))
        .route("/api/upload/confirm", post(confirm))
        .route("/api/upload/abort", post(|| async { Json(json!({ "success": true })) }))
        .route("/r2/{key}", put(put_part))
        .route("/upload", post(temp_upload))
        .route("/{code}/{name}", post(temp_download))
        .layer(axum::extract::DefaultBodyLimit::disable())
        .with_state(m);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

struct Node {
    st: Arc<AppState>,
    events: mpsc::UnboundedReceiver<(&'static str, Value)>,
    _dir: tempfile::TempDir,
}

fn node() -> Node {
    let dir = tempfile::tempdir().unwrap();
    let ident = crate::identity::load_or_create_identity(&dir.path().join("data")).unwrap();
    let (tx, rx) = mpsc::unbounded_channel();
    let emit: crate::state::EmitFn = Arc::new(move |e, v| {
        let _ = tx.send((e, v));
    });
    let settings = Settings {
        device_id: "me".into(),
        name: "Mac".into(),
        download_dir: dir.path().join("downloads").to_string_lossy().into_owned(),
        receive_enabled: true,
        hide_dock: false,
        language: "vi".into(),
        background_receive: true,
    };
    let st = Arc::new(AppState::new(emit, dir.path().join("data"), ident, 0, settings));
    Node { st, events: rx, _dir: dir }
}

/// Chờ một event cụ thể (bỏ qua các event khác).
async fn wait_for(n: &mut Node, name: &str) -> Value {
    wait_for_secs(n, name, 60).await
}

async fn wait_for_secs(n: &mut Node, name: &str, secs: u64) -> Value {
    let fut = async {
        while let Some((e, v)) = n.events.recv().await {
            if e == name {
                return v;
            }
            if e == "transfer-update" && v["status"] == "error" {
                panic!("lượt chuyển lỗi: {}", v["error"]);
            }
        }
        panic!("hết event");
    };
    tokio::time::timeout(std::time::Duration::from_secs(secs), fut).await.expect("quá lâu")
}

fn write(p: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

fn data(n: usize, seed: u32) -> Vec<u8> {
    (0..n as u32).map(|i| (i.wrapping_mul(2654435761).wrapping_add(seed) >> 13) as u8).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn send_and_receive_through_mock_services() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mock: Shared = Arc::new(Mutex::new(Mock { fail_once: true, ..Default::default() }));
    test_base::set(Some(mock_server(mock.clone()).await));

    let src = tempfile::tempdir().unwrap();
    let big = data(300 * 1024 + 7, 1); // > SINGLE_MAX: chia 5 phần
    write(&src.path().join("phim.mp4"), &big);
    write(&src.path().join("Ảnh/a.jpg"), &data(5000, 2));
    write(&src.path().join("Ảnh/thư mục con/b.txt"), b"xin chao");

    // --- storage.to, một file lớn: chia phần, phần 2 lỗi một lần rồi gửi lại ---
    let mut a = node();
    let path = src.path().join("phim.mp4").to_string_lossy().into_owned();
    send(a.st.clone(), vec![path.clone()], Provider::StorageTo).await.unwrap();
    let done = wait_for(&mut a, "online-done").await;
    let link = done["link"].as_str().unwrap().to_string();
    assert!(link.starts_with("https://gmttuts.github.io/better-drop/nhan/#1.st.F"));
    assert_eq!(mock.lock().unwrap().expiry_days, Some(7));
    let visitor = mock.lock().unwrap().visitor.clone().unwrap();
    assert!(visitor.starts_with("betterdrop-"));
    let hist = wait_for(&mut a, "history-added").await;
    assert_eq!(hist["online"]["url"], link.as_str());
    assert_eq!(hist["status"], "done");
    assert_eq!(hist["files"][0]["path"], path.as_str());

    // Dịch vụ không thấy tên file hay nội dung gốc.
    let parsed = Link::parse(&link).unwrap();
    let stored = mock.lock().unwrap().files[&parsed.id].clone();
    assert!(!stored.windows(8).any(|w| w == b"phim.mp4"));
    assert!(!stored.windows(64).any(|w| w == &big[1000..1064]));

    // storage.to không cho app tải thẳng: app trả về trang để người dùng tự tải.
    let mut b = node();
    let r = receive_link(b.st.clone(), link.clone()).await.unwrap();
    assert!(r.transfer_id.is_none());
    assert_eq!(r.page_url, format!("https://storage.to/{}", parsed.id));
    // Người nhận tải file .bdrop về rồi chọn file đó.
    let bdrop = b._dir.path().join("betterdrop.bdrop");
    std::fs::write(&bdrop, &stored).unwrap();
    receive_file(b.st.clone(), link.clone(), bdrop.to_string_lossy().into_owned()).await.unwrap();
    let hist = wait_for(&mut b, "history-added").await;
    assert_eq!(hist["status"], "done", "{hist}");
    let dl = PathBuf::from(b.st.settings.read().unwrap().download_dir.clone());
    assert_eq!(std::fs::read(dl.join("phim.mp4")).unwrap(), big);

    // Sai khoá: báo lỗi ngay khi bấm, không tạo file.
    let mut wrong = parsed.clone();
    wrong.key = crypto::random_key();
    let err = receive_file(b.st.clone(), wrong.to_url(), bdrop.to_string_lossy().into_owned()).await.unwrap_err();
    assert_eq!(err, crypto::ERR_WRONG_KEY);

    // --- temp.sh, cả thư mục: app tự tải về và giải nén ---
    let folder = src.path().join("Ảnh").to_string_lossy().into_owned();
    send(a.st.clone(), vec![folder], Provider::TempSh).await.unwrap();
    let done = wait_for(&mut a, "online-done").await;
    let link = done["link"].as_str().unwrap().to_string();
    assert!(link.contains("#1.ts.TsAbC/betterdrop.bdrop."));
    assert_eq!(done["name"], "Ảnh.zip");

    let r = receive_link(b.st.clone(), link).await.unwrap();
    assert!(r.transfer_id.is_some());
    let hist = loop {
        let h = wait_for(&mut b, "history-added").await;
        if h["online"]["provider"] == "temp.sh" {
            break h;
        }
    };
    assert_eq!(hist["status"], "done", "{hist}");
    assert_eq!(hist["file_count"], 2);
    assert_eq!(std::fs::read(dl.join("Ảnh/a.jpg")).unwrap(), data(5000, 2));
    assert_eq!(std::fs::read(dl.join("Ảnh/thư mục con/b.txt")).unwrap(), b"xin chao");
    assert_eq!(hist["dir_path"], dl.to_string_lossy().as_ref());

    // Quá giới hạn dịch vụ: báo lỗi ngay, không tải lên.
    let huge = src.path().join("huge.bin");
    std::fs::File::create(&huge).unwrap().set_len(5_000_000_000).unwrap();
    let err = send(a.st.clone(), vec![huge.to_string_lossy().into_owned()], Provider::TempSh).await.unwrap_err();
    assert!(err.contains("Quá giới hạn của temp.sh"), "{err}");

    test_base::set(None);
}

/// Thử thật với dịch vụ trên Internet, dữ liệu ngẫu nhiên tự tạo. Chỉ chạy tay:
/// `BDROP_LIVE=st,ts BDROP_SIZES=1000000,50000000 cargo test live_services -- --ignored --nocapture`
/// In ra mỗi dòng `RESULT dịch-vụ cỡ giây-tải-lên MB/s giây-tải-về checksum link`.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn live_services() {
    use ring::rand::SecureRandom;
    use sha2::{Digest, Sha256};
    use std::io::Write;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let list = std::env::var("BDROP_LIVE").expect("thiếu BDROP_LIVE");
    let sizes: Vec<u64> = std::env::var("BDROP_SIZES").expect("thiếu BDROP_SIZES").split(',').map(|x| x.parse().unwrap()).collect();
    let src = tempfile::tempdir().unwrap();
    for code in list.split(',') {
        let p = Provider::from_code(code).expect("mã dịch vụ");
        for &size in &sizes {
            // File ngẫu nhiên tự tạo (không phải file của người dùng).
            let path = src.path().join(format!("ngau-nhien-{size}.bin"));
            let mut f = std::fs::File::create(&path).unwrap();
            let mut h = Sha256::new();
            let mut buf = vec![0u8; 1 << 20];
            let mut left = size;
            while left > 0 {
                let n = left.min(buf.len() as u64) as usize;
                ring::rand::SystemRandom::new().fill(&mut buf[..n]).unwrap();
                f.write_all(&buf[..n]).unwrap();
                h.update(&buf[..n]);
                left -= n as u64;
            }
            drop(f);
            let want = hex::encode(h.finalize());
            let mut a = node();
            let t0 = std::time::Instant::now();
            send(a.st.clone(), vec![path.to_string_lossy().into_owned()], p).await.unwrap();
            let done = wait_for_secs(&mut a, "online-done", 3 * 3600).await;
            let up = t0.elapsed().as_secs_f64();
            let link = done["link"].as_str().unwrap().to_string();
            let mut down = String::from("-");
            let mut check = String::from("tải tay");
            if p.info().app_download {
                let mut b = node();
                let t1 = std::time::Instant::now();
                receive_link(b.st.clone(), link.clone()).await.unwrap();
                let hist = wait_for_secs(&mut b, "history-added", 3 * 3600).await;
                assert_eq!(hist["status"], "done", "{hist}");
                down = format!("{:.1}", t1.elapsed().as_secs_f64());
                let got = hist["files"][0]["path"].as_str().unwrap().to_string();
                let got = hex::encode(Sha256::digest(std::fs::read(&got).unwrap()));
                check = if got == want { "khớp".into() } else { format!("SAI {got}") };
            }
            println!(
                "RESULT {} {} {:.1} {:.1} {} {} sha={} hết-hạn={} {}",
                p.info().name,
                size,
                up,
                size as f64 / up / 1e6,
                down,
                check,
                &want[..16],
                done["expires_at"],
                link
            );
            let _ = std::fs::remove_file(&path);
        }
    }
}
