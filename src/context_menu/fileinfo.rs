//! What Windows says about a resource string or a program file: indirect
//! strings (`@file,-id`) and a program's product name.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows_sys::Win32::UI::Shell::SHLoadIndirectString;

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

/// UTF-16LE text up to its first NUL.
pub(crate) fn decode_utf16le(bytes: &[u8]) -> String {
    let words: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&w| w != 0)
        .collect();
    String::from_utf16_lossy(&words)
}

/// The text an indirect string (`@shell32.dll,-8506`, `@{package?ms-resource://...}`)
/// points to. `None` when Windows cannot resolve it.
pub fn resolve_indirect(text: &str) -> Option<String> {
    let source = wide(OsStr::new(text));
    let mut buf = [0u16; 512];
    // SAFETY: `source` is NUL-terminated and outlives the call; `buf` holds
    // `buf.len()` u16s and the API writes at most that many, NUL included.
    let hr = unsafe {
        SHLoadIndirectString(
            source.as_ptr(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            std::ptr::null(),
        )
    };
    if hr < 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let resolved = String::from_utf16_lossy(&buf[..len]);
    (!resolved.trim().is_empty()).then_some(resolved)
}

/// `bytes` bytes of `block` at `ptr`, when that range lies inside `block`.
fn within(block: &[u8], ptr: *const core::ffi::c_void, bytes: usize) -> Option<&[u8]> {
    let offset = (ptr as usize).checked_sub(block.as_ptr() as usize)?;
    block.get(offset..offset.checked_add(bytes)?)
}

/// `VerQueryValueW` on `sub`: the returned pointer and length, as given.
fn query(block: &[u8], sub: &str) -> Option<(*const core::ffi::c_void, usize)> {
    let sub = wide(OsStr::new(sub));
    let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
    let mut len = 0u32;
    // SAFETY: `block` is the buffer `GetFileVersionInfoW` filled and outlives
    // the call; `sub` is NUL-terminated; both out pointers are live locals.
    let ok = unsafe { VerQueryValueW(block.as_ptr().cast(), sub.as_ptr(), &mut ptr, &mut len) };
    (ok != 0 && !ptr.is_null() && len > 0).then_some((ptr.cast_const(), len as usize))
}

/// The `ProductName` of a program's version resource, in the first language
/// it lists. `None` on any failure (no file, no version info, no name).
pub fn product_name(path: &Path) -> Option<String> {
    let file = wide(path.as_os_str());
    let mut handle = 0u32;
    // SAFETY: `file` is NUL-terminated and outlives the call; `handle` is a
    // live local the API may write to.
    let size = unsafe { GetFileVersionInfoSizeW(file.as_ptr(), &mut handle) };
    if size == 0 {
        return None;
    }
    let mut block = vec![0u8; size as usize];
    // SAFETY: `block` holds exactly `size` bytes, the length passed in.
    let ok = unsafe { GetFileVersionInfoW(file.as_ptr(), 0, size, block.as_mut_ptr().cast()) };
    if ok == 0 {
        return None;
    }

    // Pairs of (language, code page), each a little-endian u16.
    let mut languages: Vec<String> = query(&block, r"\VarFileInfo\Translation")
        .and_then(|(ptr, bytes)| within(&block, ptr, bytes))
        .map(|pairs| {
            pairs
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| {
                    let lang = u16::from_le_bytes([p[0], p[1]]);
                    let cp = u16::from_le_bytes([p[2], p[3]]);
                    format!("{lang:04x}{cp:04x}")
                })
                .collect()
        })
        .unwrap_or_default();
    // US English, Unicode then Windows-1252: what most files carry.
    languages.extend(["040904b0".to_string(), "040904e4".to_string()]);

    languages.iter().find_map(|lang| {
        let (ptr, chars) = query(&block, &format!(r"\StringFileInfo\{lang}\ProductName"))?;
        // The length of a string value is in characters, NUL included.
        let name = decode_utf16le(within(&block, ptr, chars * 2)?);
        let name = name.trim();
        (!name.is_empty()).then(|| name.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_name_of_a_file_without_version_info_is_none() {
        let path = std::env::temp_dir().join(format!(
            "wintune-fileinfo-{}-{}.exe",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, b"not a program").unwrap();
        assert_eq!(product_name(&path), None);
        assert_eq!(product_name(&path.with_extension("missing")), None);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(windows)]
    #[test]
    fn product_name_of_cmd_is_known() {
        let root = std::env::var("SystemRoot").expect("SystemRoot is always set");
        let cmd = Path::new(&root).join(r"System32\cmd.exe");
        assert!(product_name(&cmd).is_some());
    }
}
