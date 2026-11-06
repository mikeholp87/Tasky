//! Bounded registry reads and narrowly scoped writes for platform features.

use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, WIN32_ERROR,
};
use windows::Win32::System::Registry::*;
use windows::core::{PCWSTR, PWSTR};

use crate::errors::{AppError, AppResult};

pub struct Key(pub HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: this key was opened by us and is closed exactly once.
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

pub fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

pub fn error(code: WIN32_ERROR) -> AppError {
    if code == ERROR_ACCESS_DENIED {
        AppError::PermissionDenied("Windows denied access to this startup entry. System entries may require administrator rights.".into())
    } else {
        AppError::SystemQueryFailed(format!("Registry operation failed (error {}).", code.0))
    }
}

pub fn open(root: HKEY, path: &str, view: REG_SAM_FLAGS) -> AppResult<Option<Key>> {
    let path = wide(path);
    let mut key = HKEY::default();
    // SAFETY: NUL-terminated path and a valid output handle.
    let rc = unsafe { RegOpenKeyExW(root, PCWSTR(path.as_ptr()), None, KEY_READ | view, &mut key) };
    if rc == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    rc.ok().map_err(|_| error(rc))?;
    Ok(Some(Key(key)))
}

/// Read raw data without expanding environment variables. Preserve its type.
pub fn value(key: HKEY, name: &str) -> AppResult<Option<(REG_VALUE_TYPE, Vec<u8>)>> {
    let name = wide(name);
    let mut kind = REG_VALUE_TYPE::default();
    let mut bytes = 0;
    // SAFETY: the first call queries size only; all pointers refer to local values.
    let rc = unsafe {
        RegGetValueW(
            key,
            None,
            PCWSTR(name.as_ptr()),
            RRF_RT_ANY | RRF_NOEXPAND,
            Some(&mut kind),
            None,
            Some(&mut bytes),
        )
    };
    if rc == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if rc.is_err() && rc != ERROR_MORE_DATA {
        return Err(error(rc));
    }
    for _ in 0..3 {
        if bytes > 64 * 1024 {
            return Err(AppError::ActionUnavailable(
                "The registry value is too large to inspect.".into(),
            ));
        }
        let mut data = vec![0u8; bytes as usize];
        // SAFETY: data is writable for the number of bytes passed in `bytes`.
        let rc = unsafe {
            RegGetValueW(
                key,
                None,
                PCWSTR(name.as_ptr()),
                RRF_RT_ANY | RRF_NOEXPAND,
                Some(&mut kind),
                Some(data.as_mut_ptr().cast()),
                Some(&mut bytes),
            )
        };
        if rc == ERROR_MORE_DATA {
            continue;
        }
        rc.ok().map_err(|_| error(rc))?;
        data.truncate(bytes as usize);
        return Ok(Some((kind, data)));
    }
    Err(AppError::OperationBusy)
}

pub fn names(key: HKEY) -> AppResult<Vec<String>> {
    let mut out = Vec::new();
    for index in 0..10_000 {
        let mut name = vec![0u16; 16_384];
        let mut len = name.len() as u32;
        // SAFETY: buffer size is expressed in UTF-16 code units, as required.
        let rc = unsafe {
            RegEnumValueW(
                key,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut len,
                None,
                None,
                None,
                None,
            )
        };
        if rc == ERROR_NO_MORE_ITEMS {
            return Ok(out);
        }
        rc.ok().map_err(|_| error(rc))?;
        out.push(String::from_utf16_lossy(&name[..len as usize]));
    }
    Err(AppError::ActionUnavailable(
        "Too many startup entries to inspect.".into(),
    ))
}

pub fn decode_string(kind: REG_VALUE_TYPE, data: &[u8]) -> Option<String> {
    if !matches!(kind, REG_SZ | REG_EXPAND_SZ) || !data.len().is_multiple_of(2) {
        return None;
    }
    let words: Vec<_> = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&c| c != 0)
        .collect();
    Some(String::from_utf16_lossy(&words))
}

pub fn string(root: HKEY, path: &str, name: &str) -> Option<String> {
    let key = open(root, path, KEY_WOW64_64KEY).ok()??;
    let (kind, data) = value(key.0, name).ok()??;
    decode_string(kind, &data)
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

pub fn set_binary(root: HKEY, path: &str, name: &str, data: &[u8]) -> AppResult<()> {
    let path = wide(path);
    let name = wide(name);
    let mut key = HKEY::default();
    // SAFETY: valid NUL-terminated path and a writable output handle. Only this
    // fixed StartupApproved subkey is created; callers do not accept paths from IPC.
    let rc = unsafe {
        RegCreateKeyExW(
            root,
            PCWSTR(path.as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE | KEY_QUERY_VALUE | KEY_WOW64_64KEY,
            None,
            &mut key,
            None,
        )
    };
    rc.ok().map_err(|_| error(rc))?;
    let key = Key(key);
    // SAFETY: `data` and the NUL-terminated value name live through this call.
    let rc = unsafe { RegSetValueExW(key.0, PCWSTR(name.as_ptr()), None, REG_BINARY, Some(data)) };
    rc.ok().map_err(|_| error(rc))
}
