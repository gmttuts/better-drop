//! Lịch sử chuyển file: mỗi lượt gửi/nhận khi kết thúc được ghi một dòng JSON vào
//! `history.jsonl` trong thư mục dữ liệu, giữ tối đa `MAX_ENTRIES` lượt gần nhất.

use crate::models::{Direction, TransferKind, TransferStatus};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const MAX_ENTRIES: usize = 1000;
/// Văn bản dài hơn thì chỉ lưu phần đầu.
pub const MAX_TEXT: usize = 10_000;
/// Lượt gửi cả nghìn file thì chỉ lưu danh sách phần đầu.
pub const MAX_FILES: usize = 500;
const FILE: &str = "history.jsonl";
/// Ảnh thu nhỏ của lượt `id` nằm ở `thumbs/<id>/<n>.jpg` (n: vị trí trong `HistoryEntry::thumbs`).
const THUMBS_DIR: &str = "thumbs";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoryFile {
    /// Tên hiển thị (đường dẫn tương đối, vd "Ảnh/a.jpg").
    pub name: String,
    /// Nhận: nơi file đã được lưu. Gửi: file gốc (để gửi lại).
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    /// Thời điểm kết thúc, giây kể từ 1970 (UTC).
    pub finished_at: u64,
    pub direction: Direction,
    pub peer_name: String,
    #[serde(default)]
    pub peer_id: Option<String>,
    pub kind: TransferKind,
    pub file_count: usize,
    pub total_bytes: u64,
    pub status: TransferStatus,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub dir_path: Option<String>,
    #[serde(default)]
    pub files: Vec<HistoryFile>,
    #[serde(default)]
    pub text: Option<String>,
    /// Lượt gửi/nhận qua Internet: link để chép lại.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub online: Option<OnlineLink>,
    /// File ảnh/video có ảnh thu nhỏ (lấy ảnh bằng `History::thumbs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thumbs: Vec<HistoryThumb>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HistoryThumb {
    /// Trùng `HistoryFile::name` của file đó.
    pub name: String,
    /// JPEG, chỉ có khi vừa ghi; `append` lưu ra file riêng chứ không ghi vào history.jsonl.
    #[serde(skip)]
    pub jpeg: Vec<u8>,
}

/// Link "Gửi qua Internet" lưu trong lịch sử.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OnlineLink {
    pub url: String,
    /// Tên dịch vụ lưu file (vd "storage.to").
    pub provider: String,
    /// Hết hạn lúc (giây kể từ 1970); 0 nếu không biết.
    #[serde(default)]
    pub expires_at: u64,
}

/// Ghi một lượt vào lịch sử (dùng trong Tracker).
pub type Recorder = Arc<dyn Fn(HistoryEntry) + Send + Sync>;

pub struct History {
    path: PathBuf,
    /// Cũ trước, mới sau.
    entries: Vec<HistoryEntry>,
}

impl History {
    /// Đọc lịch sử; bỏ dòng hỏng; nếu quá dài thì cắt bớt và ghi lại file.
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(FILE);
        let mut entries: Vec<HistoryEntry> = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        let mut h = History { path, entries: Vec::new() };
        if entries.len() > MAX_ENTRIES {
            for old in entries.drain(..entries.len() - MAX_ENTRIES) {
                h.remove_thumbs(&old.id);
            }
            h.entries = entries;
            let _ = h.rewrite();
        } else {
            h.entries = entries;
        }
        h
    }

    /// Mới nhất trước.
    pub fn list(&self, limit: usize) -> Vec<HistoryEntry> {
        self.entries.iter().rev().take(limit).cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<&HistoryEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn append(&mut self, mut entry: HistoryEntry) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        self.save_thumbs(&mut entry);
        let line = serde_json::to_string(&entry).expect("luôn serialize được");
        self.entries.push(entry);
        if self.entries.len() > MAX_ENTRIES + MAX_ENTRIES / 5 {
            // Thỉnh thoảng mới cắt bớt để không phải ghi lại cả file mỗi lần.
            let old: Vec<String> = self.entries.drain(..self.entries.len() - MAX_ENTRIES).map(|e| e.id).collect();
            for id in old {
                self.remove_thumbs(&id);
            }
            return self.rewrite();
        }
        let mut f = OpenOptions::new().create(true).append(true).open(&self.path)?;
        writeln!(f, "{line}")
    }

    pub fn delete(&mut self, id: &str) -> std::io::Result<()> {
        self.entries.retain(|e| e.id != id);
        self.remove_thumbs(id);
        self.rewrite()
    }

    /// Xoá lịch sử (chỉ xoá bản ghi, không xoá file đã nhận).
    pub fn clear(&mut self) -> std::io::Result<()> {
        self.entries.clear();
        if let Some(dir) = self.thumbs_root() {
            let _ = std::fs::remove_dir_all(dir);
        }
        self.rewrite()
    }

    fn thumbs_root(&self) -> Option<PathBuf> {
        self.path.parent().map(|d| d.join(THUMBS_DIR))
    }

    /// Thư mục ảnh thu nhỏ của một lượt; None nếu id không an toàn để làm tên thư mục.
    fn thumbs_dir(&self, id: &str) -> Option<PathBuf> {
        let safe = !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        if !safe {
            return None;
        }
        self.thumbs_root().map(|d| d.join(id))
    }

    fn remove_thumbs(&self, id: &str) {
        if let Some(dir) = self.thumbs_dir(id) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// Ghi JPEG của các ảnh thu nhỏ ra file; ảnh nào không ghi được thì bỏ khỏi mục.
    fn save_thumbs(&self, entry: &mut HistoryEntry) {
        let thumbs = std::mem::take(&mut entry.thumbs);
        let valid: Vec<HistoryThumb> = thumbs.into_iter().filter(|t| crate::thumbs::is_jpeg(&t.jpeg)).collect();
        if valid.is_empty() {
            return;
        }
        let Some(dir) = self.thumbs_dir(&entry.id) else { return };
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        for t in valid {
            let n = entry.thumbs.len();
            if std::fs::write(dir.join(format!("{n}.jpg")), &t.jpeg).is_ok() {
                entry.thumbs.push(HistoryThumb { name: t.name, jpeg: Vec::new() });
            }
        }
    }

    /// Ảnh thu nhỏ của một lượt dạng `data:` URL, cùng thứ tự với `HistoryEntry::thumbs`
    /// (chuỗi rỗng nếu file ảnh đã mất).
    pub fn thumbs(&self, id: &str) -> Vec<String> {
        let (Some(entry), Some(dir)) = (self.get(id), self.thumbs_dir(id)) else { return Vec::new() };
        (0..entry.thumbs.len())
            .map(|n| match std::fs::read(dir.join(format!("{n}.jpg"))) {
                Ok(jpeg) if crate::thumbs::is_jpeg(&jpeg) && jpeg.len() <= crate::thumbs::MAX_BYTES => crate::thumbs::data_url(&jpeg),
                _ => String::new(),
            })
            .collect()
    }

    fn rewrite(&self) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("jsonl.tmp");
        let mut body = String::new();
        for e in &self.entries {
            body.push_str(&serde_json::to_string(e).expect("luôn serialize được"));
            body.push('\n');
        }
        std::fs::write(&tmp, body)?;
        std::fs::rename(tmp, &self.path)
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Rút gọn văn bản dài trước khi lưu.
pub fn clip_text(text: &str) -> String {
    if text.chars().count() <= MAX_TEXT {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(MAX_TEXT).collect();
        s.push('…');
        s
    }
}

/// Recorder ghi vào `history` và báo giao diện bằng event `history-added`.
pub fn recorder(history: Arc<Mutex<History>>, emit: crate::state::EmitFn) -> Recorder {
    Arc::new(move |entry: HistoryEntry| {
        let value = serde_json::to_value(&entry).ok();
        let _ = history.lock().unwrap().append(entry);
        if let Some(v) = value {
            emit("history-added", v);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> HistoryEntry {
        HistoryEntry {
            id: id.into(),
            finished_at: 1,
            direction: Direction::Receive,
            peer_name: "Mac".into(),
            peer_id: None,
            kind: TransferKind::Files,
            file_count: 1,
            total_bytes: 3,
            status: TransferStatus::Done,
            error: None,
            dir_path: None,
            files: vec![HistoryFile { name: "a.txt".into(), path: Some("/x/a.txt".into()) }],
            text: None,
            online: None,
            thumbs: vec![],
        }
    }

    #[test]
    fn append_list_delete_clear_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::load(dir.path());
        h.append(entry("1")).unwrap();
        h.append(entry("2")).unwrap();
        assert_eq!(h.list(10).iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["2", "1"]);

        let mut again = History::load(dir.path());
        assert_eq!(again.list(10).len(), 2);
        again.delete("1").unwrap();
        assert_eq!(History::load(dir.path()).list(10).len(), 1);
        again.clear().unwrap();
        assert!(History::load(dir.path()).list(10).is_empty());
    }

    #[test]
    fn keeps_only_latest_entries_and_skips_broken_lines() {
        let dir = tempfile::tempdir().unwrap();
        let mut body = String::from("dòng hỏng\n");
        for i in 0..(MAX_ENTRIES + 50) {
            body.push_str(&serde_json::to_string(&entry(&i.to_string())).unwrap());
            body.push('\n');
        }
        std::fs::write(dir.path().join(FILE), body).unwrap();
        let h = History::load(dir.path());
        assert_eq!(h.list(usize::MAX).len(), MAX_ENTRIES);
        assert_eq!(h.list(1)[0].id, (MAX_ENTRIES + 49).to_string(), "giữ lượt mới nhất");
        let lines = std::fs::read_to_string(dir.path().join(FILE)).unwrap().lines().count();
        assert_eq!(lines, MAX_ENTRIES, "file được cắt bớt");
    }

    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 0xFF, 0xD9];

    #[test]
    fn thumbs_saved_beside_history_and_removed_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::load(dir.path());
        let mut e = entry("abc-1");
        e.thumbs = vec![
            HistoryThumb { name: "a.jpg".into(), jpeg: JPEG.to_vec() },
            HistoryThumb { name: "b.jpg".into(), jpeg: b"not a jpeg".to_vec() },
        ];
        h.append(e).unwrap();
        h.append(entry("abc-2")).unwrap();
        let jsonl = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
        assert!(jsonl.contains("\"thumbs\":[{\"name\":\"a.jpg\"}]"), "chỉ lưu tên, không lưu ảnh: {jsonl}");
        assert!(dir.path().join("thumbs/abc-1/0.jpg").exists());

        let again = History::load(dir.path());
        let urls = again.thumbs("abc-1");
        assert_eq!(urls.len(), 1);
        assert!(urls[0].starts_with("data:image/jpeg;base64,/9j/"));
        assert!(again.thumbs("abc-2").is_empty());

        h.delete("abc-1").unwrap();
        assert!(!dir.path().join("thumbs/abc-1").exists());
        let mut e = entry("abc-3");
        e.thumbs = vec![HistoryThumb { name: "a.jpg".into(), jpeg: JPEG.to_vec() }];
        h.append(e).unwrap();
        h.clear().unwrap();
        assert!(!dir.path().join("thumbs").exists());
    }

    #[test]
    fn unsafe_id_gets_no_thumb_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::load(dir.path());
        let mut e = entry("../x");
        e.thumbs = vec![HistoryThumb { name: "a.jpg".into(), jpeg: JPEG.to_vec() }];
        h.append(e).unwrap();
        assert!(h.list(1)[0].thumbs.is_empty());
        assert!(!dir.path().join("x").exists() && !dir.path().join("thumbs").exists());
    }

    #[test]
    fn long_text_is_clipped() {
        let t = "a".repeat(MAX_TEXT + 10);
        assert_eq!(clip_text(&t).chars().count(), MAX_TEXT + 1);
        assert_eq!(clip_text("ngắn"), "ngắn");
    }
}
