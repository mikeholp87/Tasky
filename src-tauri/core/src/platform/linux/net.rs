//! Interface classification from `/sys/class/net/<name>`.

use std::path::Path;

use super::super::linux_facts::{self as facts, NetFacts};
use crate::models::InterfaceKind;

pub fn interface_kind(name: &str) -> InterfaceKind {
    // Interface names cannot contain '/', but never build a path from one
    // that could escape the directory.
    if name.is_empty() || name.contains('/') || name == "." || name == ".." {
        return InterfaceKind::Unknown;
    }
    let dir = Path::new("/sys/class/net").join(name);
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok();
    facts::classify_interface(&NetFacts {
        arp_type: read("type").and_then(|t| facts::parse_sysfs_u32(&t)),
        wireless: dir.join("wireless").exists() || dir.join("phy80211").exists(),
        tun_flags: read("tun_flags").and_then(|t| facts::parse_sysfs_u32(&t)),
        bridge: dir.join("bridge").exists(),
        has_device: dir.join("device").exists(),
        devtype: read("uevent").and_then(|t| facts::uevent_devtype(&t)),
    })
}
