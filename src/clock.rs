//! Local wall-clock timestamps (no extra dependency: uses `GetLocalTime`).

use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

fn local_time() -> SYSTEMTIME {
    // SAFETY: GetLocalTime only writes into the struct we hand it.
    unsafe {
        let mut st: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut st);
        st
    }
}

/// e.g. `2026-11-19 14:03:07`
pub fn now_local() -> String {
    let t = local_time();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    )
}

/// e.g. `20261119-140307`
pub fn file_stamp() -> String {
    let t = local_time();
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(s: &str) -> String {
        s.chars()
            .map(|c| if c.is_ascii_digit() { '9' } else { c })
            .collect()
    }

    #[test]
    fn now_local_is_human_readable() {
        assert_eq!(shape(&now_local()), "9999-99-99 99:99:99");
    }

    #[test]
    fn file_stamp_is_safe_for_file_names() {
        assert_eq!(shape(&file_stamp()), "99999999-999999");
    }
}
