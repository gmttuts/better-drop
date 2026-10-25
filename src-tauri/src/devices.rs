//! Sổ thiết bị: máy đã thêm thủ công (IP/QR), máy đã ẩn ("quên"), nhóm máy, máy tin cậy,
//! quy tắc nhận và đồng bộ clipboard.
//! Lưu trong `devices.json` ở thư mục dữ liệu, giữ nguyên sau khi tắt app.

use crate::models::{FileMeta, Peer};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Thông tin tối thiểu để hiển thị một máy kể cả khi nó đang tắt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceRef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub os: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub members: Vec<DeviceRef>,
}

/// Máy được "Luôn chấp nhận": file từ máy này được nhận ngay, không hỏi.
/// Gắn với dấu vân tay chứng chỉ, nên máy khác khai cùng id cũng không lọt qua.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedDevice {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub os: String,
    pub fp: String,
    #[serde(default)]
    pub added_at: u64,
}

/// Một máy, nhận diện bằng id + dấu vân tay chứng chỉ.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceKey {
    pub id: String,
    pub fp: String,
}

/// Điều kiện tự nhận file từ máy chưa tin cậy. Không đặt điều kiện nào = tắt (luôn hỏi).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AutoRule {
    /// Tự nhận khi tổng dung lượng ≤ số MB này (None: không xét dung lượng).
    pub max_mb: Option<u64>,
    /// Chỉ tự nhận khi mọi file là ảnh/video.
    pub media_only: bool,
}

impl AutoRule {
    pub fn is_off(&self) -> bool {
        self.max_mb.is_none() && !self.media_only
    }

    /// Các file này có được tự nhận không (phải thoả mọi điều kiện đã đặt).
    pub fn allows(&self, files: &[FileMeta], total_bytes: u64) -> bool {
        if self.is_off() || files.is_empty() {
            return false;
        }
        let size_ok = self.max_mb.is_none_or(|mb| total_bytes <= mb.saturating_mul(1024 * 1024));
        let media_ok = !self.media_only || files.iter().all(|f| crate::thumbs::media_kind(&f.rel_path).is_some());
        size_ok && media_ok
    }
}

/// Quy tắc nhận riêng của một máy, gắn với dấu vân tay như máy tin cậy: máy khác khai
/// cùng id nhưng không có đúng chứng chỉ thì không được hưởng.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceRule {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub os: String,
    pub fp: String,
    /// Thư mục nhận riêng ("" = thư mục nhận mặc định). Máy tính: đường dẫn đầy đủ;
    /// điện thoại: thư mục con trong thư mục nhận mặc định.
    #[serde(default)]
    pub folder: String,
    /// None: theo quy tắc chung. Có giá trị (kể cả "tắt") thì ghi đè quy tắc chung.
    #[serde(default)]
    pub auto: Option<AutoRule>,
}

/// Đồng bộ clipboard với các máy tin cậy đã chọn.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ClipboardSync {
    pub enabled: bool,
    /// id các máy tin cậy được đồng bộ.
    pub devices: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceBook {
    /// Máy thêm bằng IP/QR hoặc tự nhớ khi nhận file (mDNS không tự tìm lại được).
    #[serde(default)]
    pub known: Vec<Peer>,
    /// Máy người dùng đã "quên": không hiện trong danh sách.
    #[serde(default)]
    pub hidden: Vec<DeviceRef>,
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub trusted: Vec<TrustedDevice>,
    /// Quy tắc nhận chung (Cài đặt › Nhận file) cho máy đã lưu nhưng chưa tin cậy.
    #[serde(default)]
    pub default_rule: AutoRule,
    /// Quy tắc riêng từng máy, ghi đè quy tắc chung.
    #[serde(default)]
    pub rules: Vec<DeviceRule>,
    /// Máy người dùng đã tự thêm (IP/QR) hoặc đã bấm Chấp nhận ít nhất một lần.
    /// Chỉ những máy này (và máy có quy tắc riêng) mới được tự nhận theo quy tắc. Máy chỉ
    /// "tự nhớ" vì từng gửi văn bản tới thì không tính: gửi văn bản không cần ai đồng ý.
    #[serde(default)]
    pub approved: Vec<DeviceKey>,
    #[serde(default)]
    pub clipboard: ClipboardSync,
}

const FILE: &str = "devices.json";

impl DeviceBook {
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(FILE))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!("{FILE}.tmp"));
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).expect("luôn serialize được"))?;
        std::fs::rename(tmp, dir.join(FILE))
    }

    pub fn is_hidden(&self, id: &str) -> bool {
        self.hidden.iter().any(|d| d.id == id)
    }

    /// Ghi nhớ (hoặc cập nhật địa chỉ/tên) một máy thêm thủ công.
    pub fn remember(&mut self, peer: &Peer) {
        let mut p = peer.clone();
        p.manual = true;
        p.online = true;
        match self.known.iter_mut().find(|k| k.info.id == p.info.id) {
            Some(k) => *k = p,
            None => self.known.push(p),
        }
        self.rename_member(&peer.info.id, &peer.info.name);
    }

    /// Quên một máy: xoá khỏi danh sách đã lưu và ẩn đi (nếu nó tự hiện lại qua mDNS).
    pub fn forget(&mut self, dev: DeviceRef) {
        self.known.retain(|k| k.info.id != dev.id);
        self.untrust(&dev.id);
        self.rules.retain(|r| r.id != dev.id);
        self.approved.retain(|a| a.id != dev.id);
        if !self.is_hidden(&dev.id) {
            self.hidden.push(dev);
        }
    }

    /// Máy `id` với chứng chỉ `fp` có được tin cậy không (phải khớp cả hai).
    pub fn is_trusted(&self, id: &str, fp: &str) -> bool {
        self.trusted.iter().any(|t| t.id == id && t.fp.eq_ignore_ascii_case(fp))
    }

    /// Dấu vân tay chứng chỉ đã lưu của máy `id` (máy tin cậy trước, rồi máy đã lưu).
    pub fn fp_of(&self, id: &str) -> Option<&str> {
        self.trusted
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.fp.as_str())
            .or_else(|| self.known.iter().find(|k| k.info.id == id).map(|k| k.info.fp.as_str()))
    }

    pub fn trust(&mut self, dev: TrustedDevice) {
        // Cùng máy nhưng chứng chỉ mới (cài lại app): bỏ lựa chọn đồng bộ clipboard cũ.
        if self.trusted.iter().any(|t| t.id == dev.id && !t.fp.eq_ignore_ascii_case(&dev.fp)) {
            self.clipboard.devices.retain(|d| *d != dev.id);
        }
        self.approve(&dev.id, &dev.fp);
        self.trusted.retain(|t| t.id != dev.id);
        self.trusted.push(dev);
    }

    pub fn untrust(&mut self, id: &str) {
        self.trusted.retain(|t| t.id != id);
        self.clipboard.devices.retain(|d| d != id);
    }

    /// Quy tắc riêng của máy `id`, chỉ khi `fp` đúng chứng chỉ đã gắn với quy tắc.
    pub fn rule_for(&self, id: &str, fp: &str) -> Option<&DeviceRule> {
        self.rules.iter().find(|r| r.id == id && r.fp.eq_ignore_ascii_case(fp))
    }

    /// Lưu quy tắc riêng; quy tắc trống (thư mục mặc định, theo quy tắc chung) thì xoá.
    pub fn set_rule(&mut self, rule: DeviceRule) {
        self.rules.retain(|r| r.id != rule.id);
        if !rule.folder.trim().is_empty() || rule.auto.is_some() {
            self.rules.push(rule);
        }
    }

    /// Ghi nhận máy `id` với chứng chỉ `fp` là máy người dùng đã đồng ý (thêm tay, bấm Chấp nhận).
    pub fn approve(&mut self, id: &str, fp: &str) {
        if fp.is_empty() || self.is_approved(id, fp) {
            return;
        }
        self.approved.retain(|a| a.id != id);
        self.approved.push(DeviceKey { id: id.into(), fp: fp.into() });
    }

    pub fn is_approved(&self, id: &str, fp: &str) -> bool {
        self.approved.iter().any(|a| a.id == id && a.fp.eq_ignore_ascii_case(fp))
    }

    /// Điều kiện tự nhận cho máy `id` đã chứng minh có chứng chỉ `fp`: quy tắc riêng nếu
    /// có, không thì quy tắc chung. None với máy lạ (chưa từng được đồng ý, chưa có quy tắc
    /// riêng), hoặc máy đã đổi chứng chỉ: luôn hỏi.
    pub fn auto_rule_for(&self, id: &str, fp: &str) -> Option<AutoRule> {
        let rule = self.rule_for(id, fp);
        if rule.is_none() && !self.is_approved(id, fp) && !self.is_trusted(id, fp) {
            return None;
        }
        let auto = rule.and_then(|r| r.auto.clone()).unwrap_or_else(|| self.default_rule.clone());
        (!auto.is_off()).then_some(auto)
    }

    /// Bật/tắt đồng bộ clipboard với máy tin cậy `id` (máy chưa tin cậy thì bỏ qua).
    pub fn set_clipboard_device(&mut self, id: &str, on: bool) {
        self.clipboard.devices.retain(|d| d != id);
        if on && self.trusted.iter().any(|t| t.id == id) {
            self.clipboard.devices.push(id.into());
        }
    }

    /// Máy `id` (đã chứng minh có chứng chỉ `fp`) có được ghi thẳng vào clipboard không.
    pub fn clipboard_allowed(&self, id: &str, fp: &str) -> bool {
        self.clipboard.enabled && self.clipboard.devices.iter().any(|d| d == id) && self.is_trusted(id, fp)
    }

    pub fn unhide(&mut self, id: &str) {
        self.hidden.retain(|d| d.id != id);
    }

    /// Tên trong nhóm theo tên mới nhất của máy.
    fn rename_member(&mut self, id: &str, name: &str) {
        for g in &mut self.groups {
            for m in &mut g.members {
                if m.id == id && m.name != name {
                    m.name = name.to_string();
                }
            }
        }
    }

    /// Tạo mới (id = None) hoặc sửa một nhóm. Trả về id nhóm.
    pub fn save_group(&mut self, id: Option<String>, name: String, members: Vec<DeviceRef>) -> String {
        let mut members = members;
        members.dedup_by(|a, b| a.id == b.id);
        match id.and_then(|id| self.groups.iter_mut().find(|g| g.id == id)) {
            Some(g) => {
                g.name = name;
                g.members = members;
                g.id.clone()
            }
            None => {
                let id = uuid::Uuid::new_v4().simple().to_string();
                self.groups.push(Group { id: id.clone(), name, members });
                id
            }
        }
    }

    pub fn delete_group(&mut self, id: &str) {
        self.groups.retain(|g| g.id != id);
    }
}

/// Thư mục nhận theo quy tắc riêng: đường dẫn đầy đủ (chỉ máy tính), hoặc thư mục con của
/// thư mục nhận mặc định (đã làm sạch, không thoát ra ngoài được).
pub fn receive_dir(download_dir: &Path, folder: &str, mobile: bool) -> PathBuf {
    let folder = folder.trim();
    if folder.is_empty() {
        return download_dir.to_path_buf();
    }
    if Path::new(folder).is_absolute() && !mobile {
        return PathBuf::from(folder);
    }
    let mut dir = download_dir.to_path_buf();
    for c in crate::paths::sanitize_rel_path(folder) {
        dir.push(c);
    }
    dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DeviceInfo;

    fn peer(id: &str, name: &str) -> Peer {
        Peer {
            info: DeviceInfo { id: id.into(), name: name.into(), os: "mac".into(), fp: "aa".into(), port: 1, version: String::new() },
            addrs: vec!["10.0.0.1:1".into()],
            manual: true,
            online: true,
        }
    }

    fn dref(id: &str, name: &str) -> DeviceRef {
        DeviceRef { id: id.into(), name: name.into(), os: "mac".into() }
    }

    #[test]
    fn remember_forget_unhide_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let mut book = DeviceBook::default();
        book.remember(&peer("a", "Máy A"));
        book.remember(&peer("a", "Máy A mới"));
        assert_eq!(book.known.len(), 1);
        assert_eq!(book.known[0].info.name, "Máy A mới");

        let gid = book.save_group(None, "Phòng kế toán".into(), vec![dref("a", "Máy A"), dref("b", "Máy B")]);
        book.remember(&peer("a", "Máy A đổi tên"));
        assert_eq!(book.groups[0].members[0].name, "Máy A đổi tên");

        book.forget(dref("a", "Máy A"));
        assert!(book.known.is_empty());
        assert!(book.is_hidden("a"));
        // Quên không làm mất thành viên nhóm (người dùng tự sửa nhóm nếu muốn).
        assert_eq!(book.groups[0].members.len(), 2);

        book.save(dir.path()).unwrap();
        let loaded = DeviceBook::load(dir.path());
        assert!(loaded.is_hidden("a"));
        assert_eq!(loaded.groups[0].name, "Phòng kế toán");

        let mut loaded = loaded;
        loaded.unhide("a");
        assert!(!loaded.is_hidden("a"));
        assert_eq!(loaded.save_group(Some(gid.clone()), "Kế toán".into(), vec![dref("b", "Máy B")]), gid);
        assert_eq!(loaded.groups.len(), 1);
        assert_eq!(loaded.groups[0].members.len(), 1);
        loaded.delete_group(&gid);
        assert!(loaded.groups.is_empty());
    }

    fn file(name: &str, size: u64) -> FileMeta {
        FileMeta { id: name.into(), rel_path: name.into(), size }
    }

    fn rule(id: &str, fp: &str, folder: &str, auto: Option<AutoRule>) -> DeviceRule {
        DeviceRule { id: id.into(), name: "Máy".into(), os: "ios".into(), fp: fp.into(), folder: folder.into(), auto }
    }

    const MB: u64 = 1024 * 1024;

    #[test]
    fn auto_rule_checks_size_and_media() {
        let off = AutoRule::default();
        assert!(!off.allows(&[file("a.jpg", 1)], 1), "mặc định tắt");
        let small = AutoRule { max_mb: Some(10), media_only: false };
        assert!(small.allows(&[file("a.pdf", 10 * MB)], 10 * MB));
        assert!(!small.allows(&[file("a.pdf", 10 * MB + 1)], 10 * MB + 1));
        let media = AutoRule { max_mb: None, media_only: true };
        assert!(media.allows(&[file("a.JPG", 5), file("b.mov", 5)], 10));
        assert!(!media.allows(&[file("a.jpg", 5), file("b.exe", 5)], 10));
        let both = AutoRule { max_mb: Some(1), media_only: true };
        assert!(both.allows(&[file("a.png", MB)], MB));
        assert!(!both.allows(&[file("a.png", 2 * MB)], 2 * MB));
        assert!(!both.allows(&[file("a.txt", 1)], 1));
        // Số MB khổng lồ không làm tràn số.
        assert!(AutoRule { max_mb: Some(u64::MAX), media_only: false }.allows(&[file("x", u64::MAX)], u64::MAX));
    }

    #[test]
    fn device_rule_overrides_default_rule() {
        let mut book = DeviceBook { default_rule: AutoRule { max_mb: Some(5), media_only: false }, ..Default::default() };
        // Máy lạ: không áp dụng quy tắc nào, kể cả quy tắc chung.
        assert_eq!(book.auto_rule_for("a", "fa"), None);
        book.approve("a", "fa");
        assert_eq!(book.auto_rule_for("a", "fa"), Some(book.default_rule.clone()));
        // Quy tắc riêng ghi đè quy tắc chung…
        let media = AutoRule { max_mb: None, media_only: true };
        book.set_rule(rule("a", "fa", "", Some(media.clone())));
        assert_eq!(book.auto_rule_for("a", "fa"), Some(media));
        // …kể cả khi tắt hẳn cho máy này.
        book.set_rule(rule("a", "fa", "", Some(AutoRule::default())));
        assert_eq!(book.auto_rule_for("a", "fa"), None);
        // Chỉ có thư mục riêng: điều kiện tự nhận theo quy tắc chung.
        book.set_rule(rule("a", "fa", "/x", None));
        assert_eq!(book.auto_rule_for("a", "fa"), Some(book.default_rule.clone()));
        // Có quy tắc riêng là đủ (người dùng tự đặt), không cần đã từng bấm Chấp nhận.
        book.set_rule(rule("b", "fb", "", Some(AutoRule { max_mb: Some(1), media_only: false })));
        assert!(book.auto_rule_for("b", "fb").is_some());
        // Quy tắc trống thì xoá.
        book.set_rule(rule("b", "fb", " ", None));
        assert!(book.rule_for("b", "fb").is_none());
    }

    #[test]
    fn rules_are_bound_to_the_certificate() {
        let mut book = DeviceBook { default_rule: AutoRule { max_mb: Some(5), media_only: false }, ..Default::default() };
        book.approve("a", "fa");
        book.set_rule(rule("a", "fa", "/Lan", Some(AutoRule { max_mb: Some(100), media_only: false })));
        // Cùng id, chứng chỉ khác: không có quy tắc riêng, không có quy tắc chung.
        assert!(book.rule_for("a", "ff").is_none());
        assert_eq!(book.auto_rule_for("a", "ff"), None);
        assert!(book.rule_for("a", "FA").is_some(), "dấu vân tay không phân biệt hoa thường");
        // Quên máy thì xoá hết quy tắc.
        book.forget(DeviceRef { id: "a".into(), name: "A".into(), os: String::new() });
        assert!(book.rules.is_empty() && book.approved.is_empty());
    }

    #[test]
    fn clipboard_only_for_selected_trusted_devices() {
        let mut book = DeviceBook::default();
        book.set_clipboard_device("a", true);
        assert!(book.clipboard.devices.is_empty(), "chỉ máy tin cậy");
        book.trust(TrustedDevice { id: "a".into(), name: "A".into(), os: String::new(), fp: "fa".into(), added_at: 0 });
        book.set_clipboard_device("a", true);
        assert!(!book.clipboard_allowed("a", "fa"), "chưa bật đồng bộ");
        book.clipboard.enabled = true;
        assert!(book.clipboard_allowed("a", "fa"));
        assert!(!book.clipboard_allowed("a", "ff"), "sai chứng chỉ");
        book.untrust("a");
        assert!(!book.clipboard_allowed("a", "fa"));
        assert!(book.clipboard.devices.is_empty());
    }

    #[test]
    fn receive_dir_stays_inside_on_mobile() {
        let base = Path::new("/dl");
        assert_eq!(receive_dir(base, "", false), base);
        assert_eq!(receive_dir(base, "iPhone của Lan", true), base.join("iPhone của Lan"));
        assert_eq!(receive_dir(base, "../../etc", true), base.join("etc"));
        if cfg!(unix) {
            assert_eq!(receive_dir(base, "/Users/x/Ảnh", false), Path::new("/Users/x/Ảnh"));
            assert_eq!(receive_dir(base, "/sdcard/x", true), base.join("sdcard").join("x"));
        }
    }
}
