//! Làm sạch đường dẫn nhận từ máy khác để không thể ghi ra ngoài thư mục tải về,
//! và để tên file hợp lệ trên cả macOS lẫn Windows.

use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Làm sạch một thành phần (tên file hoặc thư mục). Trả về None nếu nên bỏ qua.
pub fn sanitize_component(raw: &str) -> Option<String> {
    let mut s: String = raw
        .nfc()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    // Windows không cho phép tên kết thúc bằng dấu chấm hoặc khoảng trắng.
    while s.ends_with('.') || s.ends_with(' ') {
        s.pop();
    }
    let s = s.trim_start().to_string();
    if s.is_empty() || s == "." || s == ".." {
        return None;
    }
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    if WINDOWS_RESERVED.contains(&stem.as_str()) {
        return Some(format!("_{s}"));
    }
    Some(s)
}

/// Tách và làm sạch một đường dẫn tương đối. Chấp nhận cả "/" lẫn "\".
/// Bỏ các thành phần rỗng, ".", ".." và tiền tố ổ đĩa, nên kết quả luôn nằm
/// bên trong thư mục gốc.
pub fn sanitize_rel_path(raw: &str) -> Vec<String> {
    let parts: Vec<String> = raw
        .split(['/', '\\'])
        .filter_map(sanitize_component)
        .collect();
    if parts.is_empty() {
        vec!["file".to_string()]
    } else {
        parts
    }
}

/// Trả về đường dẫn chưa tồn tại, thêm hậu tố " (1)", " (2)"... nếu cần.
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    // Thư mục thì không tách phần mở rộng.
    let (stem, ext) = if path.is_dir() {
        (
            path.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            String::new(),
        )
    } else {
        (stem, ext)
    };
    (1..)
        .map(|i| parent.join(format!("{stem} ({i}){ext}")))
        .find(|p| !p.exists())
        .expect("vòng lặp vô hạn luôn tìm được tên")
}

/// Tên đường dẫn tương đối gửi đi: luôn dùng "/" và chuẩn hoá Unicode NFC
/// (macOS hay lưu tên tiếng Việt ở dạng NFD).
pub fn to_wire_path(components: &[String]) -> String {
    components
        .iter()
        .map(|c| c.nfc().collect::<String>())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_traversal() {
        assert_eq!(sanitize_rel_path("../../etc/passwd"), vec!["etc", "passwd"]);
        assert_eq!(sanitize_rel_path("a/../../b"), vec!["a", "b"]);
        assert_eq!(sanitize_rel_path("..\\..\\win.ini"), vec!["win.ini"]);
    }

    #[test]
    fn strips_absolute_and_drive() {
        assert_eq!(sanitize_rel_path("/etc/hosts"), vec!["etc", "hosts"]);
        assert_eq!(sanitize_rel_path("C:\\Windows\\x.dll"), vec!["C_", "Windows", "x.dll"]);
    }

    #[test]
    fn replaces_forbidden_chars() {
        assert_eq!(sanitize_component("a<b>c:d\"e|f?g*h"), Some("a_b_c_d_e_f_g_h".into()));
        assert_eq!(sanitize_component("name. . "), Some("name".into()));
        assert_eq!(sanitize_component("CON.txt"), Some("_CON.txt".into()));
        assert_eq!(sanitize_component(".."), None);
        assert_eq!(sanitize_rel_path(""), vec!["file"]);
    }

    #[test]
    fn normalizes_vietnamese_to_nfc() {
        let nfd = "Ba\u{0301}o ca\u{0301}o.pdf"; // "Báo cáo.pdf" dạng NFD
        assert_eq!(sanitize_component(nfd), Some("Báo cáo.pdf".into()));
        assert_eq!(to_wire_path(&[nfd.to_string()]), "Báo cáo.pdf");
    }

    #[test]
    fn unique_adds_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        assert_eq!(unique_path(&p), p);
        std::fs::write(&p, "x").unwrap();
        assert_eq!(unique_path(&p), dir.path().join("a (1).txt"));
        std::fs::write(dir.path().join("a (1).txt"), "x").unwrap();
        assert_eq!(unique_path(&p), dir.path().join("a (2).txt"));

        let d = dir.path().join("Photos");
        std::fs::create_dir(&d).unwrap();
        assert_eq!(unique_path(&d), dir.path().join("Photos (1)"));
    }
}
