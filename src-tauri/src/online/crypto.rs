//! Mã hoá đầu cuối cho "Gửi qua Internet": AES-256-GCM theo khối (kiểu STREAM).
//!
//! Định dạng file `.bdrop` (phiên bản 1):
//!
//! ```text
//! "BDRP" | phiên bản (1) | thuật toán (1 = AES-256-GCM) | cỡ khối (u32, big-endian)
//! khối 0 | khối 1 | … | khối cuối
//! ```
//!
//! Mỗi khối là `cỡ khối` byte dữ liệu gốc (khối cuối có thể ngắn hơn) đã mã hoá, cộng
//! 16 byte tag. Nonce 12 byte = 3 byte 0 | số thứ tự khối (u64 BE) | 1 nếu là khối cuối,
//! 0 nếu không. AAD là 10 byte đầu file. Vì khối cuối được đánh dấu ngay trong nonce,
//! cắt bớt file ở bất kỳ đâu (kể cả đúng ranh giới khối) đều bị phát hiện. Khoá ngẫu
//! nhiên mới cho mỗi lượt gửi nên nonce theo số thứ tự không bao giờ lặp lại.
//!
//! Trang nhận trên website giải mã đúng định dạng này bằng WebCrypto.

use base64::Engine as _;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use std::io::{self, Read};

pub const MAGIC: &[u8; 4] = b"BDRP";
pub const VERSION: u8 = 1;
pub const ALG_AES_256_GCM: u8 = 1;
pub const HEADER_LEN: usize = 10;
pub const TAG_LEN: usize = 16;
pub const KEY_LEN: usize = 32;
/// 1 MiB mỗi khối: đủ nhỏ để trình duyệt giải mã từng khối, đủ lớn để ít tốn tag.
pub const CHUNK: usize = 1 << 20;
const MAX_CHUNK: usize = 16 << 20;

pub type Key = [u8; KEY_LEN];

pub fn random_key() -> Key {
    use ring::rand::SecureRandom;
    let mut k = [0u8; KEY_LEN];
    ring::rand::SystemRandom::new().fill(&mut k).expect("hệ điều hành luôn cấp được số ngẫu nhiên");
    k
}

pub fn key_to_b64(k: &Key) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(k)
}

pub fn key_from_b64(s: &str) -> Option<Key> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s.trim_end_matches('='))
        .ok()?
        .try_into()
        .ok()
}

pub fn header(chunk: usize) -> [u8; HEADER_LEN] {
    let mut h = [0u8; HEADER_LEN];
    h[..4].copy_from_slice(MAGIC);
    h[4] = VERSION;
    h[5] = ALG_AES_256_GCM;
    h[6..].copy_from_slice(&(chunk as u32).to_be_bytes());
    h
}

/// Kích thước sau khi mã hoá `plain` byte (để báo trước cho dịch vụ lưu file).
pub fn encrypted_len(plain: u64, chunk: usize) -> u64 {
    let chunks = plain.div_ceil(chunk as u64).max(1);
    HEADER_LEN as u64 + plain + chunks * TAG_LEN as u64
}

fn nonce(index: u64, last: bool) -> Nonce {
    let mut n = [0u8; 12];
    n[3..11].copy_from_slice(&index.to_be_bytes());
    n[11] = last as u8;
    Nonce::assume_unique_for_key(n)
}

fn cipher(key: &Key) -> LessSafeKey {
    LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).expect("khoá đúng 32 byte"))
}

/// Đọc cho tới khi đầy `buf` hoặc hết dữ liệu. Trả về số byte đã đọc.
fn fill(r: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

/// Bọc một nguồn dữ liệu gốc, đọc ra dữ liệu đã mã hoá (kể cả phần đầu file).
pub struct Encryptor<R> {
    inner: R,
    key: LessSafeKey,
    header: [u8; HEADER_LEN],
    chunk: usize,
    index: u64,
    /// Dữ liệu gốc đã đọc trước (tối đa một khối + 1 byte để biết khối nào là cuối).
    plain: Vec<u8>,
    out: Vec<u8>,
    pos: usize,
    done: bool,
}

impl<R: Read> Encryptor<R> {
    pub fn new(inner: R, key: &Key) -> Self {
        Self::with_chunk(inner, key, CHUNK)
    }

    pub fn with_chunk(inner: R, key: &Key, chunk: usize) -> Self {
        let header = header(chunk);
        Encryptor {
            inner,
            key: cipher(key),
            header,
            chunk,
            index: 0,
            plain: Vec::with_capacity(chunk + 1),
            out: header.to_vec(),
            pos: 0,
            done: false,
        }
    }

    fn next_chunk(&mut self) -> io::Result<()> {
        let have = self.plain.len();
        self.plain.resize(self.chunk + 1, 0);
        let got = fill(&mut self.inner, &mut self.plain[have..])?;
        self.plain.truncate(have + got);
        let last = self.plain.len() <= self.chunk;
        let take = self.plain.len().min(self.chunk);
        let mut block: Vec<u8> = self.plain.drain(..take).collect();
        self.key
            .seal_in_place_append_tag(nonce(self.index, last), Aad::from(&self.header), &mut block)
            .map_err(|_| io::Error::other("Không mã hoá được"))?;
        self.index += 1;
        self.out = block;
        self.pos = 0;
        self.done = last;
        Ok(())
    }
}

impl<R: Read> Read for Encryptor<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos == self.out.len() {
            if self.done {
                return Ok(0);
            }
            self.next_chunk()?;
        }
        let n = buf.len().min(self.out.len() - self.pos);
        buf[..n].copy_from_slice(&self.out[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

pub const ERR_WRONG_KEY: &str = "Sai khoá giải mã, hoặc file không phải của link này";
pub const ERR_DAMAGED: &str = "File mã hoá bị hỏng hoặc bị cắt ngắn (tải chưa hết?)";
pub const ERR_NOT_BDROP: &str = "Đây không phải file BetterDrop (.bdrop)";

/// Bọc dữ liệu đã mã hoá, đọc ra dữ liệu gốc. Báo lỗi nếu sai khoá, dữ liệu bị sửa,
/// hoặc bị cắt (thiếu khối cuối).
pub struct Decryptor<R> {
    inner: R,
    key: LessSafeKey,
    header: [u8; HEADER_LEN],
    chunk: usize,
    index: u64,
    started: bool,
    buf: Vec<u8>,
    out: Vec<u8>,
    pos: usize,
    done: bool,
}

impl<R: Read> Decryptor<R> {
    pub fn new(inner: R, key: &Key) -> Self {
        Decryptor {
            inner,
            key: cipher(key),
            header: [0; HEADER_LEN],
            chunk: 0,
            index: 0,
            started: false,
            buf: Vec::new(),
            out: Vec::new(),
            pos: 0,
            done: false,
        }
    }

    fn read_header(&mut self) -> io::Result<()> {
        let n = fill(&mut self.inner, &mut self.header)?;
        if n < HEADER_LEN || &self.header[..4] != MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, ERR_NOT_BDROP));
        }
        if self.header[4] != VERSION || self.header[5] != ALG_AES_256_GCM {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "File được tạo bằng BetterDrop mới hơn. Hãy cập nhật app",
            ));
        }
        let chunk = u32::from_be_bytes(self.header[6..10].try_into().unwrap()) as usize;
        if chunk == 0 || chunk > MAX_CHUNK {
            return Err(io::Error::new(io::ErrorKind::InvalidData, ERR_NOT_BDROP));
        }
        self.chunk = chunk;
        self.buf = Vec::with_capacity(chunk + TAG_LEN + 1);
        self.started = true;
        Ok(())
    }

    fn next_chunk(&mut self) -> io::Result<()> {
        let full = self.chunk + TAG_LEN;
        let have = self.buf.len();
        self.buf.resize(full + 1, 0);
        let got = fill(&mut self.inner, &mut self.buf[have..])?;
        self.buf.truncate(have + got);
        let last = self.buf.len() <= full;
        if self.buf.len() < TAG_LEN {
            return Err(io::Error::new(io::ErrorKind::InvalidData, ERR_DAMAGED));
        }
        let take = self.buf.len().min(full);
        let mut block: Vec<u8> = self.buf.drain(..take).collect();
        let copy = (self.index == 0).then(|| block.clone());
        let plain_len = match self.key.open_in_place(nonce(self.index, last), Aad::from(&self.header), &mut block) {
            Ok(p) => p.len(),
            Err(_) => {
                // Khối đầu hỏng: nếu mở được khi coi là "chưa phải khối cuối" thì khoá đúng
                // nhưng file bị cắt; không thì gần như chắc chắn là sai khoá.
                let truncated = copy.is_some_and(|mut c| {
                    self.key.open_in_place(nonce(0, !last), Aad::from(&self.header), &mut c).is_ok()
                });
                let msg = if self.index == 0 && !truncated { ERR_WRONG_KEY } else { ERR_DAMAGED };
                return Err(io::Error::new(io::ErrorKind::InvalidData, msg));
            }
        };
        block.truncate(plain_len);
        self.index += 1;
        self.out = block;
        self.pos = 0;
        self.done = last;
        Ok(())
    }
}

impl<R: Read> Read for Decryptor<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.started {
            self.read_header()?;
        }
        while self.pos == self.out.len() {
            if self.done {
                return Ok(0);
            }
            self.next_chunk()?;
        }
        let n = buf.len().min(self.out.len() - self.pos);
        buf[..n].copy_from_slice(&self.out[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i * 31 % 251) as u8).collect()
    }

    fn enc(plain: &[u8], key: &Key, chunk: usize) -> Vec<u8> {
        let mut out = Vec::new();
        Encryptor::with_chunk(plain, key, chunk).read_to_end(&mut out).unwrap();
        out
    }

    fn dec(ct: &[u8], key: &Key) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        Decryptor::new(ct, key).read_to_end(&mut out)?;
        Ok(out)
    }

    #[test]
    fn round_trip_various_sizes() {
        let key = random_key();
        for (n, chunk) in [(0, 16), (1, 16), (15, 16), (16, 16), (17, 16), (64, 16), (1000, 64), (3 * CHUNK + 5, CHUNK)] {
            let plain = data(n);
            let ct = enc(&plain, &key, chunk);
            assert_eq!(ct.len() as u64, encrypted_len(n as u64, chunk), "độ dài khi n={n}");
            assert_eq!(dec(&ct, &key).unwrap(), plain, "vòng tròn khi n={n}");
        }
    }

    #[test]
    fn truncation_is_detected() {
        let key = random_key();
        let plain = data(16 * 4);
        let ct = enc(&plain, &key, 16);
        // Cắt đúng ranh giới khối (bỏ hẳn khối cuối) và cắt giữa khối.
        let block = 16 + TAG_LEN;
        for cut in [ct.len() - block, ct.len() - 1, HEADER_LEN + block, HEADER_LEN + 3] {
            let err = dec(&ct[..cut], &key).unwrap_err();
            assert_eq!(err.to_string(), ERR_DAMAGED, "cắt còn {cut} byte");
        }
        // Thêm rác vào cuối cũng bị phát hiện.
        let mut longer = ct.clone();
        longer.extend_from_slice(&[0; 5]);
        assert!(dec(&longer, &key).is_err());
    }

    #[test]
    fn wrong_key_and_tampering_fail() {
        let key = random_key();
        let ct = enc(&data(100), &key, 32);
        assert_eq!(dec(&ct, &random_key()).unwrap_err().to_string(), ERR_WRONG_KEY);
        let mut bad = ct.clone();
        bad[HEADER_LEN + 40] ^= 1;
        assert!(dec(&bad, &key).is_err());
        // Sửa cỡ khối trong phần đầu (thuộc AAD) cũng không qua được.
        let mut hdr = ct.clone();
        hdr[9] ^= 1;
        assert!(dec(&hdr, &key).is_err());
        assert_eq!(dec(b"hello world", &key).unwrap_err().to_string(), ERR_NOT_BDROP);
    }

    #[test]
    fn key_b64_round_trip() {
        let k = random_key();
        let s = key_to_b64(&k);
        assert_eq!(s.len(), 43);
        assert!(!s.contains(['.', '+', '/', '=']));
        assert_eq!(key_from_b64(&s), Some(k));
        assert_eq!(key_from_b64("abc"), None);
    }

    /// Vector cố định để kiểm tra trang nhận trên website giải mã ra cùng kết quả.
    #[test]
    fn known_vector() {
        let key = [7u8; KEY_LEN];
        let ct = enc(b"BetterDrop", &key, 4);
        assert_eq!(&ct[..HEADER_LEN], &[b'B', b'D', b'R', b'P', 1, 1, 0, 0, 0, 4]);
        assert_eq!(ct.len(), HEADER_LEN + 10 + 3 * TAG_LEN);
        assert_eq!(dec(&ct, &key).unwrap(), b"BetterDrop");
    }
}
