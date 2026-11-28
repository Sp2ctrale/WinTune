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
    Ui {
        action: u32,
        ui: u32,
    },
    PvValue {
        action: u32,
        pv: usize,
    },
    PvArray {
        action: u32,
        array: [i32; 3],
    },
    /// `SPI_SETANIMATION`: a pointer to `ANIMATIONINFO { 8, on }`.
    Animation {
        action: u32,
        on: bool,
    },
}

/// `SPI_GETANIMATION`: whether minimize/maximize animation is on (any
/// non-zero `iMinAnimate`).
pub(crate) fn spi_get_animation() -> Result<bool, ApiError> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, ANIMATIONINFO, SPI_GETANIMATION,
    };
    let size = std::mem::size_of::<ANIMATIONINFO>() as u32;
    let mut info = ANIMATIONINFO {
        cbSize: size,
        iMinAnimate: 0,
    };
    // SAFETY: `info` is a live ANIMATIONINFO with `cbSize` set, and uiParam
    // is its size, as SPI_GETANIMATION requires.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETANIMATION,
            size,
            (&mut info as *mut ANIMATIONINFO).cast(),
            0,
        )
    };
    if ok == 0 {
        Err(last_error())
    } else {
        Ok(info.iMinAnimate != 0)
    }
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
        SetCall::Animation { action, on } => {
            use windows_sys::Win32::UI::WindowsAndMessaging::ANIMATIONINFO;
            let size = std::mem::size_of::<ANIMATIONINFO>() as u32;
            let mut info = ANIMATIONINFO {
                cbSize: size,
                iMinAnimate: i32::from(on),
            };
            // SAFETY: pvParam points to a live ANIMATIONINFO with `cbSize`
            // set and uiParam is its size, as SPI_SETANIMATION requires; it
            // outlives the call.
            unsafe {
                SystemParametersInfoW(
                    action,
                    size,
                    (&mut info as *mut ANIMATIONINFO).cast(),
                    flags,
                )
            }
        }
    };
    if ok == 0 {
        Err(last_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn animation_info_is_eight_bytes() {
        assert_eq!(
            std::mem::size_of::<windows_sys::Win32::UI::WindowsAndMessaging::ANIMATIONINFO>(),
            8
        );
    }
}
