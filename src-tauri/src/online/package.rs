//! Gói nội dung gửi qua Internet thành một luồng dữ liệu gốc (trước khi mã hoá):
//!
//! ```text
//! độ dài manifest (u32, big-endian) | manifest (JSON) | nội dung
//! ```
//!
//! Nội dung là chính file đó (gửi một file), hoặc một gói zip Stored (nhiều file, thư
//! mục). Manifest chứa tên và danh sách file, nằm trong phần được mã hoá nên dịch vụ
//! lưu file không biết tên file.

use super::zip::{self, Entry, Piece};
use crate::paths::{sanitize_component, sanitize_rel_path, unique_path};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_MANIFEST: usize = 16 << 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    File,
    Zip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub v: u32,
    pub kind: Kind,
    /// Tên để lưu nội dung (file gốc, hoặc "<thư mục>.zip").
    pub name: String,
    /// Độ dài phần nội dung.
    pub size: u64,
    pub files: Vec<ManifestFile>,
}

/// Một file cần gửi: nguồn (đường dẫn hoặc URI) và đường dẫn tương đối ("Ảnh/a.jpg").
#[derive(Debug, Clone)]
pub struct Item {
    pub source: String,
    pub path: String,
    pub size: u64,
}

/// Mở nguồn file (trên điện thoại có thể là `content://`, `file://`).
pub type Opener = Arc<dyn Fn(&str) -> io::Result<std::fs::File> + Send + Sync>;

pub struct Package {
    pub manifest: Manifest,
    prefix: Vec<u8>,
    pieces: Vec<Piece>,
    items: Vec<Item>,
    crcs: Vec<Option<u32>>,
    /// Tổng độ dài dữ liệu gốc (prefix + nội dung).
    pub plain_len: u64,
}

fn mtime_of(source: &str) -> u64 {
    std::fs::metadata(source)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or_else(crate::history::now_secs)
}

/// CRC32 của cả file (cần cho zip). Đọc trước một lượt; ổ đĩa nhanh hơn mạng nhiều.
fn crc_of(open: &Opener, item: &Item, cancelled: &dyn Fn() -> bool) -> Result<u32, String> {
    let mut f = open(&item.source).map_err(|e| format!("Không đọc được {}: {e}", item.path))?;
    let mut h = crc32fast::Hasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        if cancelled() {
            return Err("Đã huỷ".into());
        }
        let n = f.read(&mut buf).map_err(|e| format!("Không đọc được {}: {e}", item.path))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize())
}

impl Package {
    /// Một file đơn lẻ (không nằm trong thư mục) thì gửi nguyên file; còn lại gói zip.
    pub fn build(items: Vec<Item>, open: &Opener, cancelled: &dyn Fn() -> bool) -> Result<Package, String> {
        if items.is_empty() {
            return Err("Không có file nào để gửi".into());
        }
        let files: Vec<ManifestFile> = items.iter().map(|i| ManifestFile { path: i.path.clone(), size: i.size }).collect();
        let (manifest, pieces, crcs) = if items.len() == 1 && !items[0].path.contains('/') {
            let m = Manifest { v: 1, kind: Kind::File, name: items[0].path.clone(), size: items[0].size, files };
            (m, vec![Piece::File(0)], vec![None])
        } else {
            let mut entries = Vec::with_capacity(items.len());
            let mut crcs = Vec::with_capacity(items.len());
            for it in &items {
                let crc = crc_of(open, it, cancelled)?;
                crcs.push(Some(crc));
                entries.push(Entry { path: it.path.clone(), size: it.size, crc, mtime: mtime_of(&it.source) });
            }
            let (pieces, size) = zip::layout(&entries);
            let root = items[0].path.split('/').next().unwrap_or_default().to_string();
            let one_root = items.iter().all(|i| i.path.starts_with(&format!("{root}/")));
            let name = if one_root { format!("{root}.zip") } else { "BetterDrop.zip".to_string() };
            (Manifest { v: 1, kind: Kind::Zip, name, size, files }, pieces, crcs)
        };
        let json = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
        if json.len() > MAX_MANIFEST {
            return Err("Quá nhiều file trong một lượt gửi".into());
        }
        let mut prefix = (json.len() as u32).to_be_bytes().to_vec();
        prefix.extend(json);
        let plain_len = prefix.len() as u64 + manifest.size;
        Ok(Package { manifest, prefix, pieces, items, crcs, plain_len })
    }

    /// Luồng đọc dữ liệu gốc. Đọc lại được nhiều lần (mỗi lần một luồng mới).
    pub fn reader(&self, open: Opener) -> PlainReader {
        let mut pieces = vec![Piece::Bytes(self.prefix.clone())];
        pieces.extend(self.pieces.iter().cloned());
        PlainReader {
            pieces,
            items: self.items.clone(),
            crcs: self.crcs.clone(),
            open,
            idx: 0,
            pos: 0,
            file: None,
        }
    }
}

struct OpenFile {
    f: std::fs::File,
    left: u64,
    crc: crc32fast::Hasher,
}

pub struct PlainReader {
    pieces: Vec<Piece>,
    items: Vec<Item>,
    crcs: Vec<Option<u32>>,
    open: Opener,
    idx: usize,
    pos: usize,
    file: Option<OpenFile>,
}

fn changed(item: &Item) -> io::Error {
    io::Error::other(format!("File {} đã bị sửa trong lúc gửi. Hãy gửi lại", item.path))
}

impl Read for PlainReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let Some(piece) = self.pieces.get(self.idx) else { return Ok(0) };
            match piece {
                Piece::Bytes(b) => {
                    if self.pos < b.len() {
                        let n = buf.len().min(b.len() - self.pos);
                        buf[..n].copy_from_slice(&b[self.pos..self.pos + n]);
                        self.pos += n;
                        return Ok(n);
                    }
                }
                Piece::File(i) => {
                    let item = &self.items[*i];
                    if self.file.is_none() {
                        let f = (self.open)(&item.source)
                            .map_err(|e| io::Error::other(format!("Không đọc được {}: {e}", item.path)))?;
                        self.file = Some(OpenFile { f, left: item.size, crc: crc32fast::Hasher::new() });
                    }
                    let of = self.file.as_mut().unwrap();
                    if of.left > 0 {
                        let want = buf.len().min(of.left.min(usize::MAX as u64) as usize);
                        let n = of.f.read(&mut buf[..want])?;
                        if n == 0 {
                            return Err(changed(item));
                        }
                        of.left -= n as u64;
                        of.crc.update(&buf[..n]);
                        return Ok(n);
                    }
                    let of = self.file.take().unwrap();
                    if let Some(expected) = self.crcs[*i] {
                        if of.crc.finalize() != expected {
                            return Err(changed(item));
                        }
                    }
                }
            }
            self.idx += 1;
            self.pos = 0;
        }
    }
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// Đọc manifest ở đầu luồng dữ liệu gốc.
pub fn read_manifest(r: &mut impl Read) -> io::Result<Manifest> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_MANIFEST {
        return Err(bad("Dữ liệu không hợp lệ"));
    }
    let mut json = vec![0u8; len];
    r.read_exact(&mut json)?;
    let m: Manifest = serde_json::from_slice(&json).map_err(|_| bad("Dữ liệu không hợp lệ"))?;
    if m.v != 1 {
        return Err(bad("File được tạo bằng BetterDrop mới hơn. Hãy cập nhật app"));
    }
    Ok(m)
}

fn copy_exact(r: &mut impl Read, w: &mut impl Write, mut n: u64) -> io::Result<()> {
    let mut buf = vec![0u8; 256 * 1024];
    while n > 0 {
        let want = buf.len().min(n.min(usize::MAX as u64) as usize);
        let got = r.read(&mut buf[..want])?;
        if got == 0 {
            return Err(bad(super::crypto::ERR_DAMAGED));
        }
        w.write_all(&buf[..got])?;
        n -= got as u64;
    }
    Ok(())
}

/// Đọc tới hết luồng (để khối cuối được xác thực) và chắc chắn không còn dữ liệu thừa.
fn finish(r: &mut impl Read) -> io::Result<()> {
    let mut rest = [0u8; 64 * 1024];
    let mut extra = 0u64;
    loop {
        let n = r.read(&mut rest)?;
        if n == 0 {
            break;
        }
        extra += n as u64;
        if extra > MAX_MANIFEST as u64 {
            return Err(bad("Dữ liệu không hợp lệ"));
        }
    }
    Ok(())
}

/// Kết quả giải gói: các mục đã lưu (file, hoặc thư mục gốc) trong thư mục nhận.
#[derive(Debug, Clone, Serialize)]
pub struct Saved {
    pub manifest: Manifest,
    /// Đường dẫn tới từng file đã lưu (theo thứ tự trong manifest).
    pub files: Vec<PathBuf>,
    /// Mục chính để "Hiện trong thư mục" (file, hoặc thư mục gốc).
    pub main: PathBuf,
}

/// Giải gói từ luồng dữ liệu gốc (đã giải mã) vào `dest`. Ghi vào chỗ tạm trước, chỉ
/// chuyển vào thư mục nhận khi đã đọc và xác thực hết dữ liệu.
#[cfg(test)]
pub fn unpack(r: &mut impl Read, dest: &Path) -> io::Result<Saved> {
    let manifest = read_manifest(r)?;
    unpack_with(r, manifest, dest)
}

/// Như `unpack`, khi manifest đã được đọc trước.
pub fn unpack_with(r: &mut impl Read, manifest: Manifest, dest: &Path) -> io::Result<Saved> {
    std::fs::create_dir_all(dest)?;
    let tmp = dest.join(format!(".betterdrop-{}.part", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&tmp)?;
    let result = unpack_into(r, &manifest, &tmp, dest);
    let _ = std::fs::remove_dir_all(&tmp);
    let (files, main) = result?;
    Ok(Saved { manifest, files, main })
}

fn unpack_into(r: &mut impl Read, m: &Manifest, tmp: &Path, dest: &Path) -> io::Result<(Vec<PathBuf>, PathBuf)> {
    let mut rels: Vec<Vec<String>> = Vec::with_capacity(m.files.len());
    match m.kind {
        Kind::File => {
            let name = sanitize_component(&m.name).unwrap_or_else(|| "file".into());
            let mut f = std::fs::File::create(tmp.join(&name))?;
            copy_exact(r, &mut f, m.size)?;
            f.sync_all()?;
            rels.push(vec![name]);
        }
        Kind::Zip => {
            let mut used = 0u64;
            for file in &m.files {
                let mut fixed = [0u8; 30];
                r.read_exact(&mut fixed)?;
                let skip = zip::local_header_skip(&fixed).map_err(|e| bad(&e))?;
                io::copy(&mut r.by_ref().take(skip as u64), &mut io::sink())?;
                let rel = sanitize_rel_path(&file.path);
                let mut p = tmp.to_path_buf();
                p.extend(&rel);
                if let Some(parent) = p.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut f = std::fs::File::create(&p)?;
                copy_exact(r, &mut f, file.size)?;
                used += 30 + skip as u64 + file.size;
                rels.push(rel);
            }
            if used > m.size {
                return Err(bad("Dữ liệu không hợp lệ"));
            }
        }
    }
    finish(r)?;
    // Đã xác thực xong: chuyển từng mục gốc vào thư mục nhận (đổi tên nếu trùng).
    let mut moved: std::collections::HashMap<String, PathBuf> = Default::default();
    let mut files = Vec::with_capacity(rels.len());
    for rel in &rels {
        let root = &rel[0];
        let target = match moved.get(root) {
            Some(t) => t.clone(),
            None => {
                let t = unique_path(&dest.join(root));
                std::fs::rename(tmp.join(root), &t)?;
                moved.insert(root.clone(), t.clone());
                t
            }
        };
        let mut p = target;
        p.extend(&rel[1..]);
        files.push(p);
    }
    let main = moved.get(&rels[0][0]).cloned().unwrap_or_else(|| dest.to_path_buf());
    Ok((files, main))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::online::crypto;

    fn opener() -> Opener {
        Arc::new(|p: &str| std::fs::File::open(p))
    }

    fn write(dir: &Path, rel: &str, data: &[u8]) -> Item {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, data).unwrap();
        Item { source: p.to_string_lossy().into_owned(), path: rel.into(), size: data.len() as u64 }
    }

    fn round_trip(items: Vec<Item>) -> (Saved, tempfile::TempDir) {
        let pkg = Package::build(items, &opener(), &|| false).unwrap();
        let key = crypto::random_key();
        let mut ct = Vec::new();
        crypto::Encryptor::with_chunk(pkg.reader(opener()), &key, 64).read_to_end(&mut ct).unwrap();
        assert_eq!(ct.len() as u64, crypto::encrypted_len(pkg.plain_len, 64));
        let out = tempfile::tempdir().unwrap();
        let saved = unpack(&mut crypto::Decryptor::new(&ct[..], &key), out.path()).unwrap();
        (saved, out)
    }

    #[test]
    fn single_file_round_trip() {
        let src = tempfile::tempdir().unwrap();
        let data: Vec<u8> = (0..5000u32).map(|i| i as u8).collect();
        let (saved, out) = round_trip(vec![write(src.path(), "báo cáo.pdf", &data)]);
        assert_eq!(saved.manifest.kind, Kind::File);
        assert_eq!(std::fs::read(out.path().join("báo cáo.pdf")).unwrap(), data);
        // Không để lại thư mục tạm.
        assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 1);
    }

    #[test]
    fn folder_round_trip_and_name_clash() {
        let src = tempfile::tempdir().unwrap();
        let items = vec![
            write(src.path(), "Ảnh/a.txt", b"aaa"),
            write(src.path(), "Ảnh/sub/b.txt", b"bbbb"),
            write(src.path(), "Ảnh/rỗng", b""),
        ];
        let (saved, out) = round_trip(items.clone());
        assert_eq!(saved.manifest.kind, Kind::Zip);
        assert_eq!(saved.manifest.name, "Ảnh.zip");
        assert_eq!(std::fs::read(out.path().join("Ảnh/sub/b.txt")).unwrap(), b"bbbb");
        assert_eq!(saved.main, out.path().join("Ảnh"));
        // Nhận lần nữa vào cùng thư mục: không ghi đè, đặt tên "Ảnh (1)".
        let pkg = Package::build(items, &opener(), &|| false).unwrap();
        let key = crypto::random_key();
        let mut ct = Vec::new();
        crypto::Encryptor::new(pkg.reader(opener()), &key).read_to_end(&mut ct).unwrap();
        let again = unpack(&mut crypto::Decryptor::new(&ct[..], &key), out.path()).unwrap();
        assert_eq!(again.main, out.path().join("Ảnh (1)"));
        assert_eq!(std::fs::read(out.path().join("Ảnh (1)/a.txt")).unwrap(), b"aaa");
    }

    #[test]
    fn hostile_paths_stay_inside() {
        let src = tempfile::tempdir().unwrap();
        let mut a = write(src.path(), "x/a.txt", b"1");
        a.path = "../../evil/a.txt".into();
        let b = write(src.path(), "x/b.txt", b"2");
        let (saved, out) = round_trip(vec![a, b]);
        for f in &saved.files {
            assert!(f.starts_with(out.path()), "{f:?} nằm ngoài thư mục nhận");
        }
    }

    #[test]
    fn truncated_stream_leaves_nothing() {
        let src = tempfile::tempdir().unwrap();
        let items = vec![write(src.path(), "d/a.bin", &[9u8; 3000]), write(src.path(), "d/b.bin", &[8u8; 3000])];
        let pkg = Package::build(items, &opener(), &|| false).unwrap();
        let key = crypto::random_key();
        let mut ct = Vec::new();
        crypto::Encryptor::with_chunk(pkg.reader(opener()), &key, 256).read_to_end(&mut ct).unwrap();
        ct.truncate(ct.len() - 100);
        let out = tempfile::tempdir().unwrap();
        assert!(unpack(&mut crypto::Decryptor::new(&ct[..], &key), out.path()).is_err());
        assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0, "không để lại file dở");
    }

    #[test]
    fn file_changed_while_sending_is_an_error() {
        let src = tempfile::tempdir().unwrap();
        let items = vec![write(src.path(), "d/a.txt", b"hello"), write(src.path(), "d/b.txt", b"x")];
        let pkg = Package::build(items, &opener(), &|| false).unwrap();
        std::fs::write(src.path().join("d/a.txt"), b"HELLO").unwrap();
        let mut sink = Vec::new();
        let err = pkg.reader(opener()).read_to_end(&mut sink).unwrap_err();
        assert!(err.to_string().contains("đã bị sửa"));
    }
}
