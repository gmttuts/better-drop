//! Sổ thiết bị: máy đã thêm thủ công (IP/QR), máy đã ẩn ("quên") và nhóm máy.
//! Lưu trong `devices.json` ở thư mục dữ liệu, giữ nguyên sau khi tắt app.

use crate::models::Peer;
use serde::{Deserialize, Serialize};
use std::path::Path;

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
        if !self.is_hidden(&dev.id) {
            self.hidden.push(dev);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DeviceInfo;

    fn peer(id: &str, name: &str) -> Peer {
        Peer {
            info: DeviceInfo { id: id.into(), name: name.into(), os: "mac".into(), fp: "aa".into(), port: 1 },
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
}
