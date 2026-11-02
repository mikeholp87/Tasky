use ::windows::Win32::Foundation::{HLOCAL, LocalFree};
use ::windows::Win32::Security::Authorization::ConvertStringSidToSidW;
use ::windows::Win32::Security::{LookupAccountSidW, PSID, SID_NAME_USE};
use ::windows::core::{HSTRING, PWSTR};

/// Resolves a SID string such as `S-1-5-18` to an account name (`SYSTEM`).
/// sysinfo's user list only covers interactive/local accounts, so service
/// accounts need this fallback. Results are cached by the caller.
pub fn account_name(uid: &sysinfo::Uid) -> Option<String> {
    let sid_string = uid.to_string();
    let mut psid = PSID::default();
    // SAFETY: `psid` receives a LocalAlloc'd SID that we free below.
    unsafe { ConvertStringSidToSidW(&HSTRING::from(sid_string.as_str()), &mut psid) }.ok()?;

    let result = account_name_for_psid(psid);
    // SAFETY: psid was allocated by ConvertStringSidToSidW with LocalAlloc.
    unsafe { LocalFree(Some(HLOCAL(psid.0))) };
    result
}

/// <https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-lookupaccountsidw>
fn account_name_for_psid(psid: PSID) -> Option<String> {
    let mut name = [0u16; 256];
    let mut domain = [0u16; 256];
    let mut name_len = name.len() as u32;
    let mut domain_len = domain.len() as u32;
    let mut use_ = SID_NAME_USE::default();
    // SAFETY: buffers and lengths are valid; lengths are in characters.
    unsafe {
        LookupAccountSidW(
            None,
            psid,
            Some(PWSTR(name.as_mut_ptr())),
            &mut name_len,
            Some(PWSTR(domain.as_mut_ptr())),
            &mut domain_len,
            &mut use_,
        )
    }
    .ok()?;
    let name = String::from_utf16_lossy(&name[..name_len as usize]);
    (!name.is_empty()).then_some(name)
}
