//! Interface kinds from IP Helper. Match the OS description/alias, never
//! infer physical devices from user-editable interface names.

use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};

use crate::models::InterfaceKind;

fn wide(value: &[u16]) -> String {
    String::from_utf16_lossy(&value[..value.iter().position(|&c| c == 0).unwrap_or(value.len())])
}

pub fn interface_kind(name: &str) -> InterfaceKind {
    let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
    // SAFETY: IP Helper allocates the returned table, freed below exactly once.
    if unsafe { GetIfTable2(&mut table) }.is_err() || table.is_null() {
        return InterfaceKind::Unknown;
    }
    // SAFETY: a successful GetIfTable2 returns NumEntries contiguous rows.
    let rows = unsafe {
        std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize)
    };
    let kind = rows
        .iter()
        .find(|r| wide(&r.Description) == name || wide(&r.Alias) == name)
        .map(|r| {
            // HardwareInterface is bit 0 of the documented status bitfield.
            classify(r.Type, r.InterfaceAndOperStatusFlags._bitfield & 1 != 0)
        })
        .unwrap_or(InterfaceKind::Unknown);
    // SAFETY: table came from GetIfTable2 and no references escape this function.
    unsafe { FreeMibTable(table.cast()) };
    kind
}

fn classify(kind: u32, hardware: bool) -> InterfaceKind {
    match kind {
        24 => InterfaceKind::Loopback,
        23 | 131 => InterfaceKind::Vpn, // PPP, tunnel
        71 => InterfaceKind::Wifi,
        6 if hardware => InterfaceKind::Ethernet,
        6 | 53 | 135 => InterfaceKind::Virtual,
        _ => InterfaceKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_and_virtual_interfaces_do_not_double_count_traffic() {
        assert_eq!(classify(6, true), InterfaceKind::Ethernet);
        assert_eq!(classify(6, false), InterfaceKind::Virtual);
        assert_eq!(classify(71, true), InterfaceKind::Wifi);
        assert_eq!(classify(131, false), InterfaceKind::Vpn);
        assert!(!classify(6, false).counts_in_totals());
        assert_eq!(
            interface_kind("not-an-existing-interface"),
            InterfaceKind::Unknown
        );
    }
}
