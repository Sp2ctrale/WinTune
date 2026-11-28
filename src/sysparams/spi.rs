//! `SystemParametersInfoW`: the GET and SET calls and their error.

/// A failed system call: the `GetLastError` code (87 for a value of the
/// wrong shape).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiError {
    pub code: u32,
}

/// `ERROR_INVALID_PARAMETER`, given for a value of the wrong shape.
pub(crate) const WRONG_SHAPE: ApiError = ApiError { code: 87 };

/// The parameters of one `SystemParametersInfoW` SET.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetCall {
    Ui { action: u32, ui: u32 },
    PvValue { action: u32, pv: usize },
    PvArray { action: u32, array: [i32; 3] },
}

pub(crate) fn last_error() -> ApiError {
    // SAFETY: GetLastError only reads the calling thread's last-error value.
    let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
    ApiError { code }
}

/// `SystemParametersInfoW(action, 0, out, 0)`: a GET into `*out`.
pub(crate) fn spi_get<T>(action: u32, out: &mut T) -> Result<(), ApiError> {
    use windows_sys::Win32::UI::WindowsAndMessaging::SystemParametersInfoW;
    // SAFETY: `out` is a live, writable `T` that the caller sized to the
    // action's documented output (u32, BOOL or int[3]); fWinIni 0 writes
    // nothing to the profile.
    let ok = unsafe { SystemParametersInfoW(action, 0, (out as *mut T).cast(), 0) };
    if ok == 0 {
        Err(last_error())
    } else {
        Ok(())
    }
}

/// One SET, always `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE`.
pub(crate) fn spi_set(call: SetCall) -> Result<(), ApiError> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE,
    };
    let flags = SPIF_UPDATEINIFILE | SPIF_SENDCHANGE;
    let ok = match call {
        // SAFETY: this action takes its value in uiParam and ignores
        // pvParam, so a null pointer is never dereferenced.
        SetCall::Ui { action, ui } => unsafe {
            SystemParametersInfoW(action, ui, std::ptr::null_mut(), flags)
        },
        // SAFETY: this action takes its value in pvParam itself (cast to
        // a pointer, spec 5.1 "pv"); the system never dereferences it.
        SetCall::PvValue { action, pv } => unsafe {
            SystemParametersInfoW(action, 0, pv as *mut core::ffi::c_void, flags)
        },
        SetCall::PvArray { action, mut array } => {
            // SAFETY: pvParam points to a live int[3] on this stack frame,
            // the layout SPI_SETMOUSE reads; it outlives the call.
            unsafe { SystemParametersInfoW(action, 0, array.as_mut_ptr().cast(), flags) }
        }
    };
    if ok == 0 {
        Err(last_error())
    } else {
        Ok(())
    }
}
