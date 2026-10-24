//! Zip kiểu Stored (không nén) tối giản để gói nhiều file/thư mục thành một luồng.
//!
//! Biết trước kích thước và CRC32 của từng file nên tính được chính xác độ dài cả gói
//! (dịch vụ lưu file cần biết trước), và không cần "data descriptor" — dạng mà một số
//! trình giải nén xử lý kém. Tự dùng Zip64 khi file hoặc gói vượt 4 GB.

const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const EOCD_SIG: u32 = 0x0605_4b50;
const EOCD64_SIG: u32 = 0x0606_4b50;
const LOCATOR64_SIG: u32 = 0x0706_4b50;
/// Bit 11: tên file mã hoá UTF-8.
const FLAG_UTF8: u16 = 0x0800;
const MAX32: u64 = 0xFFFF_FFFF;

#[derive(Debug, Clone)]
pub struct Entry {
    /// Đường dẫn tương đối, phân tách bằng "/".
    pub path: String,
    pub size: u64,
    pub crc: u32,
    /// Thời điểm sửa (giây kể từ 1970, UTC).
    pub mtime: u64,
}

struct Buf(Vec<u8>);

impl Buf {
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
}

/// Giờ và ngày kiểu MS-DOS (năm từ 1980).
fn dos_time(secs: u64) -> (u16, u16) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Đổi số ngày ra ngày tháng năm (thuật toán "civil from days").
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    if !(1980..=2107).contains(&y) {
        return (0, (1 << 5) | 1);
    }
    let time = ((rem / 3600) << 11 | (rem % 3600 / 60) << 5 | (rem % 60) / 2) as u16;
    let date = (((y - 1980) as u16) << 9) | ((m as u16) << 5) | d as u16;
    (time, date)
}

fn local_header(e: &Entry) -> Vec<u8> {
    let big = e.size >= MAX32;
    let mut b = Buf(Vec::with_capacity(30 + e.path.len() + 20));
    let (time, date) = dos_time(e.mtime);
    b.u32(LOCAL_SIG);
    b.u16(if big { 45 } else { 20 });
    b.u16(FLAG_UTF8);
    b.u16(0); // Stored
    b.u16(time);
    b.u16(date);
    b.u32(e.crc);
    let s = if big { MAX32 as u32 } else { e.size as u32 };
    b.u32(s);
    b.u32(s);
    b.u16(e.path.len() as u16);
    b.u16(if big { 20 } else { 0 });
    b.0.extend_from_slice(e.path.as_bytes());
    if big {
        b.u16(0x0001);
        b.u16(16);
        b.u64(e.size);
        b.u64(e.size);
    }
    b.0
}

fn central(entries: &[(Entry, u64)], cd_offset: u64) -> Vec<u8> {
    let mut b = Buf(Vec::new());
    for (e, offset) in entries {
        let big_size = e.size >= MAX32;
        let big_off = *offset >= MAX32;
        let (time, date) = dos_time(e.mtime);
        let mut extra = Buf(Vec::new());
        if big_size || big_off {
            let n = (big_size as u16) * 16 + (big_off as u16) * 8;
            extra.u16(0x0001);
            extra.u16(n);
            if big_size {
                extra.u64(e.size);
                extra.u64(e.size);
            }
            if big_off {
                extra.u64(*offset);
            }
        }
        b.u32(CENTRAL_SIG);
        b.u16((3 << 8) | 45); // tạo trên Unix, bản 4.5
        b.u16(if big_size || big_off { 45 } else { 20 });
        b.u16(FLAG_UTF8);
        b.u16(0);
        b.u16(time);
        b.u16(date);
        b.u32(e.crc);
        let s = if big_size { MAX32 as u32 } else { e.size as u32 };
        b.u32(s);
        b.u32(s);
        b.u16(e.path.len() as u16);
        b.u16(extra.0.len() as u16);
        b.u16(0); // chú thích
        b.u16(0); // đĩa
        b.u16(0); // thuộc tính trong
        b.u32(0o100644 << 16); // quyền rw-r--r--
        b.u32(if big_off { MAX32 as u32 } else { *offset as u32 });
        b.0.extend_from_slice(e.path.as_bytes());
        b.0.extend_from_slice(&extra.0);
    }
    let cd_size = b.0.len() as u64;
    let count = entries.len() as u64;
    let need64 = count >= 0xFFFF || cd_size >= MAX32 || cd_offset >= MAX32;
    if need64 {
        let eocd64_offset = cd_offset + cd_size;
        b.u32(EOCD64_SIG);
        b.u64(44);
        b.u16((3 << 8) | 45);
        b.u16(45);
        b.u32(0);
        b.u32(0);
        b.u64(count);
        b.u64(count);
        b.u64(cd_size);
        b.u64(cd_offset);
        b.u32(LOCATOR64_SIG);
        b.u32(0);
        b.u64(eocd64_offset);
        b.u32(1);
    }
    b.u32(EOCD_SIG);
    b.u16(0);
    b.u16(0);
    let c16 = if need64 { 0xFFFF } else { count as u16 };
    b.u16(c16);
    b.u16(c16);
    b.u32(if need64 { MAX32 as u32 } else { cd_size as u32 });
    b.u32(if need64 { MAX32 as u32 } else { cd_offset as u32 });
    b.u16(0);
    b.0
}

/// Một đoạn của gói: byte có sẵn (phần đầu/cuối zip) hoặc nội dung file thứ `i`.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Bytes(Vec<u8>),
    File(usize),
}

/// Bố cục của cả gói zip: các đoạn theo thứ tự và tổng độ dài.
pub fn layout(entries: &[Entry]) -> (Vec<Piece>, u64) {
    let mut pieces = Vec::new();
    let mut offset = 0u64;
    let mut placed = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        let h = local_header(e);
        placed.push((e.clone(), offset));
        offset += h.len() as u64 + e.size;
        pieces.push(Piece::Bytes(h));
        pieces.push(Piece::File(i));
    }
    let tail = central(&placed, offset);
    offset += tail.len() as u64;
    pieces.push(Piece::Bytes(tail));
    (pieces, offset)
}

/// Đọc phần đầu (local header) của một mục trong luồng zip do `layout` tạo ra, trả
/// về số byte cần bỏ qua sau 30 byte đầu (tên + extra).
pub fn local_header_skip(fixed: &[u8; 30]) -> Result<usize, String> {
    if u32::from_le_bytes(fixed[..4].try_into().unwrap()) != LOCAL_SIG {
        return Err("Gói file bị hỏng".into());
    }
    let name = u16::from_le_bytes([fixed[26], fixed[27]]) as usize;
    let extra = u16::from_le_bytes([fixed[28], fixed[29]]) as usize;
    Ok(name + extra)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(files: &[(&str, &[u8])]) -> Vec<u8> {
        let entries: Vec<Entry> = files
            .iter()
            .map(|(p, d)| Entry { path: p.to_string(), size: d.len() as u64, crc: crc32fast::hash(d), mtime: 1_700_000_000 })
            .collect();
        let (pieces, total) = layout(&entries);
        let mut out = Vec::new();
        for p in pieces {
            match p {
                Piece::Bytes(b) => out.extend(b),
                Piece::File(i) => out.extend_from_slice(files[i].1),
            }
        }
        assert_eq!(out.len() as u64, total);
        out
    }

    #[test]
    fn readable_by_zip_crate() {
        use std::io::Read;
        let data = build(&[("Ảnh/a.txt", b"xin chao"), ("Ảnh/sub/b.bin", &[1, 2, 3]), ("Ảnh/rỗng.txt", b"")]);
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(data)).unwrap();
        assert_eq!(z.len(), 3);
        let mut s = String::new();
        z.by_name("Ảnh/a.txt").unwrap().read_to_string(&mut s).unwrap();
        assert_eq!(s, "xin chao");
        let mut b = Vec::new();
        z.by_name("Ảnh/sub/b.bin").unwrap().read_to_end(&mut b).unwrap();
        assert_eq!(b, [1, 2, 3]);
    }

    #[test]
    fn dos_time_is_correct() {
        // 2023-11-14 22:13:20 UTC
        let (t, d) = dos_time(1_700_000_000);
        assert_eq!(d >> 9, 2023 - 1980);
        assert_eq!((d >> 5) & 0xF, 11);
        assert_eq!(d & 0x1F, 14);
        assert_eq!(t >> 11, 22);
        assert_eq!((t >> 5) & 0x3F, 13);
        assert_eq!((t & 0x1F) * 2, 20);
    }
}
