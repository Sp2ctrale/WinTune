//! The check that WinTune runs as the user signed in to this session.

use super::last_error;

/// Whether WinTune runs as the user signed in to this session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountCheck {
    Same,
    Different { process: String, session: String },
    Unknown(String),
}

/// `Same` when the two `DOMAIN\user` names match, ignoring case.
pub fn compare_accounts(process: &str, session: &str) -> AccountCheck {
    if process.to_lowercase() == session.to_lowercase() {
        AccountCheck::Same
    } else {
        AccountCheck::Different {
            process: process.to_string(),
            session: session.to_string(),
        }
    }
}

/// Compares the process account (from its token) with the user signed in
/// to this session.
pub fn session_account_check() -> AccountCheck {
    use windows_sys::Win32::System::RemoteDesktop::{WTSDomainName, WTSUserName};
    let process = match process_account() {
        Ok(name) => name,
        Err(code) => return AccountCheck::Unknown(format!("GetUserNameExW failed: error {code}")),
    };
    let session = wts_string(WTSDomainName).and_then(|domain| {
        let user = wts_string(WTSUserName)?;
        Ok((domain, user))
    });
    match session {
        Err(code) => {
            AccountCheck::Unknown(format!("WTSQuerySessionInformationW failed: error {code}"))
        }
        Ok((_, user)) if user.is_empty() => {
            AccountCheck::Unknown("no user is signed in to this session".to_string())
        }
        Ok((domain, user)) => compare_accounts(&process, &format!("{domain}\\{user}")),
    }
}

/// `DOMAIN\user` of the process token, or the `GetLastError` code.
fn process_account() -> Result<String, u32> {
    use windows_sys::Win32::Foundation::ERROR_MORE_DATA;
    use windows_sys::Win32::Security::Authentication::Identity::{
        GetUserNameExW, NameSamCompatible,
    };
    let mut buf = vec![0u16; 256];
    for _ in 0..2 {
        let mut len = buf.len() as u32;
        // SAFETY: `buf` is writable for `len` UTF-16 units; on success `len`
        // is the name's length without the NUL, on ERROR_MORE_DATA the size
        // needed.
        let ok = unsafe { GetUserNameExW(NameSamCompatible, buf.as_mut_ptr(), &mut len) };
        if ok != 0 {
            let end = (len as usize).min(buf.len());
            return Ok(String::from_utf16_lossy(&buf[..end]));
        }
        let code = last_error().code;
        if code != ERROR_MORE_DATA {
            return Err(code);
        }
        buf.resize(len as usize + 1, 0);
    }
    Err(ERROR_MORE_DATA)
}

/// One string of this session's information, or the `GetLastError` code.
fn wts_string(
    class: windows_sys::Win32::System::RemoteDesktop::WTS_INFO_CLASS,
) -> Result<String, u32> {
    use windows_sys::Win32::System::RemoteDesktop::{
        WTSFreeMemory, WTSQuerySessionInformationW, WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION,
    };
    let mut ptr: *mut u16 = std::ptr::null_mut();
    let mut bytes = 0u32;
    // SAFETY: both out-pointers are live locals; on success the system
    // allocates `bytes` bytes at `ptr`, freed below with WTSFreeMemory.
    let ok = unsafe {
        WTSQuerySessionInformationW(
            WTS_CURRENT_SERVER_HANDLE,
            WTS_CURRENT_SESSION,
            class,
            &mut ptr,
            &mut bytes,
        )
    };
    if ok == 0 {
        return Err(last_error().code);
    }
    if ptr.is_null() {
        return Ok(String::new());
    }
    // SAFETY: `ptr` holds `bytes` bytes of UTF-16 owned by us until the
    // WTSFreeMemory call; the slice is copied into a String before it.
    let text = unsafe {
        let units = std::slice::from_raw_parts(ptr, bytes as usize / 2);
        let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        let text = String::from_utf16_lossy(&units[..end]);
        WTSFreeMemory(ptr.cast());
        text
    };
    Ok(text)
}
