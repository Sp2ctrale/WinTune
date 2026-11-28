//! The input backend: `SystemParametersInfoW` behind the `InputApi` trait,
//! a fake for tests, and the check that the process runs as the session's
//! user.

use super::{MkProblem, SettingId, SettingValue, Shape};
pub use crate::sysparams::ApiError;
pub(crate) use crate::sysparams::SetCall;
pub use crate::sysparams::{compare_accounts, session_account_check, AccountCheck};
use crate::sysparams::{spi_get, spi_set, WRONG_SHAPE};

pub trait InputApi: Send + Sync {
    fn get(&self, id: SettingId) -> Result<SettingValue, ApiError>;
    fn set(&self, id: SettingId, value: &SettingValue) -> Result<(), ApiError>;
    fn filter_keys_on(&self) -> Result<bool, ApiError>;
}

/// How a setting is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GetCall {
    /// `SystemParametersInfoW(action, 0, &mut out, 0)`; the output type
    /// follows the id's shape.
    Spi(u32),
    /// `GetSystemMetrics(SM_SWAPBUTTON)`.
    SwapMetric,
    /// `GetDoubleClickTime()`.
    DoubleClickTime,
}

/// Where the SET takes the value (spec 5.1: "ui" / "pv").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetFamily {
    /// The value (or the flag as 0/1) in `uiParam`.
    Ui,
    /// The value in `pvParam`, cast to a pointer; `uiParam` 0.
    PvValue,
    /// `pvParam` points to the `[i32; 3]` array; `uiParam` 0.
    PvArray,
}

#[derive(Debug)]
pub(crate) struct Descriptor {
    pub(crate) id: SettingId,
    pub(crate) get: GetCall,
    pub(crate) set: SetFamily,
    pub(crate) set_action: u32,
}

const fn spi(id: SettingId, get: u32, set: SetFamily, set_action: u32) -> Descriptor {
    Descriptor {
        id,
        get: GetCall::Spi(get),
        set,
        set_action,
    }
}

/// One row per setting, in the order of spec 5.1 (`SettingId::ALL`).
pub(crate) const DESCRIPTORS: [Descriptor; 18] = {
    use SetFamily::*;
    use SettingId::*;
    [
        spi(PointerSpeed, 0x0070, PvValue, 0x0071),
        spi(PointerPrecision, 0x0003, PvArray, 0x0004),
        spi(PointerTrails, 0x005E, Ui, 0x005D),
        spi(PointerSonar, 0x101C, PvValue, 0x101D),
        spi(PointerVanish, 0x1020, PvValue, 0x1021),
        spi(SnapToDefault, 0x005F, Ui, 0x0060),
        Descriptor {
            id: SwapButtons,
            get: GetCall::SwapMetric,
            set: Ui,
            set_action: 0x0021,
        },
        Descriptor {
            id: DoubleClickTime,
            get: GetCall::DoubleClickTime,
            set: Ui,
            set_action: 0x0020,
        },
        spi(ClickLock, 0x101E, PvValue, 0x101F),
        spi(ClickLockTime, 0x2008, PvValue, 0x2009),
        spi(WheelLines, 0x0068, Ui, 0x0069),
        spi(WheelChars, 0x006C, Ui, 0x006D),
        spi(ScrollInactive, 0x201C, PvValue, 0x201D),
        spi(KeyRepeatDelay, 0x0016, Ui, 0x0017),
        spi(KeyRepeatRate, 0x000A, Ui, 0x000B),
        spi(HoverTime, 0x0066, Ui, 0x0067),
        spi(MenuShowDelay, 0x006A, Ui, 0x006B),
        spi(DragFullWindows, 0x0026, Ui, 0x0025),
    ]
};

pub(crate) fn descriptor(id: SettingId) -> &'static Descriptor {
    DESCRIPTORS
        .iter()
        .find(|d| d.id == id)
        .expect("every setting id has a descriptor")
}

/// The SET parameters for `value`; a value of another shape than the id's,
/// or one its family cannot carry, gives error 87.
pub(crate) fn set_call(id: SettingId, value: &SettingValue) -> Result<SetCall, ApiError> {
    if value.shape() != id.shape() {
        return Err(WRONG_SHAPE);
    }
    let d = descriptor(id);
    let action = d.set_action;
    match (d.set, *value) {
        (SetFamily::Ui, SettingValue::Number(n)) => Ok(SetCall::Ui { action, ui: n }),
        (SetFamily::Ui, SettingValue::Flag(b)) => Ok(SetCall::Ui {
            action,
            ui: u32::from(b),
        }),
        (SetFamily::PvValue, SettingValue::Number(n)) => Ok(SetCall::PvValue {
            action,
            pv: n as usize,
        }),
        (SetFamily::PvValue, SettingValue::Flag(b)) => Ok(SetCall::PvValue {
            action,
            pv: usize::from(b),
        }),
        (SetFamily::PvArray, SettingValue::Mouse(array)) => Ok(SetCall::PvArray { action, array }),
        _ => Err(WRONG_SHAPE),
    }
}

/// The real backend: `SystemParametersInfoW` and friends.
pub struct SpiInput;

impl InputApi for SpiInput {
    fn get(&self, id: SettingId) -> Result<SettingValue, ApiError> {
        match descriptor(id).get {
            GetCall::SwapMetric => {
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    GetSystemMetrics, SM_SWAPBUTTON,
                };
                // SAFETY: GetSystemMetrics takes an index by value and only
                // reads system state.
                let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) };
                Ok(SettingValue::Flag(swapped != 0))
            }
            GetCall::DoubleClickTime => {
                use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
                // SAFETY: GetDoubleClickTime takes no argument and only reads
                // system state.
                Ok(SettingValue::Number(unsafe { GetDoubleClickTime() }))
            }
            GetCall::Spi(action) => match id.shape() {
                Shape::Number => {
                    let mut out = 0u32;
                    spi_get(action, &mut out)?;
                    Ok(SettingValue::Number(out))
                }
                Shape::Flag => {
                    let mut out = 0i32;
                    spi_get(action, &mut out)?;
                    Ok(SettingValue::Flag(out != 0))
                }
                Shape::Mouse => {
                    let mut out = [0i32; 3];
                    spi_get(action, &mut out)?;
                    Ok(SettingValue::Mouse(out))
                }
            },
        }
    }

    fn set(&self, id: SettingId, value: &SettingValue) -> Result<(), ApiError> {
        spi_set(set_call(id, value)?)
    }

    fn filter_keys_on(&self) -> Result<bool, ApiError> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SystemParametersInfoW, FKF_FILTERKEYSON, SPI_GETFILTERKEYS,
        };
        let size = std::mem::size_of::<FilterKeys>() as u32;
        let mut fk = FilterKeys {
            cb_size: size,
            dw_flags: 0,
            i_wait_msec: 0,
            i_delay_msec: 0,
            i_repeat_msec: 0,
            i_bounce_msec: 0,
        };
        // SAFETY: `fk` is a live FILTERKEYS (six u32, `cb_size` set) and
        // uiParam is its size, as SPI_GETFILTERKEYS requires.
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_GETFILTERKEYS,
                size,
                (&mut fk as *mut FilterKeys).cast(),
                0,
            )
        };
        if ok == 0 {
            return Err(crate::sysparams::last_error());
        }
        Ok(fk.dw_flags & FKF_FILTERKEYSON != 0)
    }
}

/// The `FILTERKEYS` structure, declared here so no accessibility feature of
/// windows-sys is needed.
#[repr(C)]
struct FilterKeys {
    cb_size: u32,
    dw_flags: u32,
    i_wait_msec: u32,
    i_delay_msec: u32,
    i_repeat_msec: u32,
    i_bounce_msec: u32,
}

impl AccountCheck {
    pub fn problem(&self) -> Option<MkProblem> {
        match self {
            AccountCheck::Same => None,
            AccountCheck::Different { process, session } => Some(MkProblem::WrongAccount {
                process: process.clone(),
                session: session.clone(),
            }),
            AccountCheck::Unknown(why) => Some(MkProblem::AccountUnknown(why.clone())),
        }
    }
}

/// A call made to the fake.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FakeCall {
    Get(SettingId),
    Set(SettingId, SettingValue),
    FilterKeys,
}

#[cfg(test)]
#[derive(Default)]
struct FakeState {
    values: std::collections::HashMap<SettingId, SettingValue>,
    fail_get: std::collections::HashSet<SettingId>,
    fail_set: std::collections::HashSet<SettingId>,
    keep: std::collections::HashMap<SettingId, SettingValue>,
    filter_keys: bool,
    calls: Vec<FakeCall>,
}

/// A map of live values with a call log; can fail a GET or a SET, or keep
/// another value than the one set.
#[cfg(test)]
pub(crate) struct FakeInput {
    state: std::sync::Mutex<FakeState>,
}

#[cfg(test)]
impl FakeInput {
    /// Seeded with the values seen on the owner's machine (spec 3).
    pub(crate) fn new() -> Self {
        use SettingId::*;
        use SettingValue::*;
        let seen = [
            (PointerSpeed, Number(10)),
            (PointerPrecision, Mouse([6, 10, 1])),
            (PointerTrails, Number(0)),
            (PointerSonar, Flag(false)),
            (PointerVanish, Flag(true)),
            (SnapToDefault, Flag(false)),
            (SwapButtons, Flag(false)),
            (DoubleClickTime, Number(500)),
            (ClickLock, Flag(false)),
            (ClickLockTime, Number(1200)),
            (WheelLines, Number(3)),
            (WheelChars, Number(3)),
            (ScrollInactive, Number(2)),
            (KeyRepeatDelay, Number(1)),
            (KeyRepeatRate, Number(31)),
            (HoverTime, Number(400)),
            (MenuShowDelay, Number(400)),
            (DragFullWindows, Flag(true)),
        ];
        FakeInput {
            state: std::sync::Mutex::new(FakeState {
                values: seen.into_iter().collect(),
                ..FakeState::default()
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, FakeState> {
        self.state.lock().expect("fake input lock")
    }

    /// The live value, without logging a call.
    pub(crate) fn value(&self, id: SettingId) -> SettingValue {
        self.state().values[&id]
    }

    /// A change made elsewhere (not logged).
    pub(crate) fn put(&self, id: SettingId, v: SettingValue) {
        self.state().values.insert(id, v);
    }

    pub(crate) fn fail_get(&self, id: SettingId) {
        self.state().fail_get.insert(id);
    }

    pub(crate) fn fail_set(&self, id: SettingId) {
        self.state().fail_set.insert(id);
    }

    /// Later SETs of `id` succeed but leave `v` as the live value.
    pub(crate) fn keep(&self, id: SettingId, v: SettingValue) {
        self.state().keep.insert(id, v);
    }

    pub(crate) fn set_filter_keys(&self, on: bool) {
        self.state().filter_keys = on;
    }

    pub(crate) fn calls(&self) -> Vec<FakeCall> {
        self.state().calls.clone()
    }

    pub(crate) fn sets(&self) -> Vec<(SettingId, SettingValue)> {
        self.state()
            .calls
            .iter()
            .filter_map(|c| match c {
                FakeCall::Set(id, v) => Some((*id, *v)),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
impl InputApi for FakeInput {
    fn get(&self, id: SettingId) -> Result<SettingValue, ApiError> {
        let mut s = self.state();
        s.calls.push(FakeCall::Get(id));
        if s.fail_get.contains(&id) {
            return Err(ApiError { code: 5 });
        }
        Ok(s.values[&id])
    }

    fn set(&self, id: SettingId, value: &SettingValue) -> Result<(), ApiError> {
        let mut s = self.state();
        s.calls.push(FakeCall::Set(id, *value));
        if s.fail_set.contains(&id) {
            return Err(ApiError { code: 5 });
        }
        set_call(id, value)?;
        let live = s.keep.get(&id).copied().unwrap_or(*value);
        s.values.insert(id, live);
        Ok(())
    }

    fn filter_keys_on(&self) -> Result<bool, ApiError> {
        let mut s = self.state();
        s.calls.push(FakeCall::FilterKeys);
        Ok(s.filter_keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mouse_keyboard::SettingValue::*;
    use crate::mouse_keyboard::*;

    #[test]
    fn descriptors_follow_the_spec_table() {
        use GetCall::*;
        use SetFamily::*;
        assert_eq!(DESCRIPTORS.map(|d| d.id), SettingId::ALL);
        let table = [
            (Spi(0x0070), PvValue, 0x0071),
            (Spi(0x0003), PvArray, 0x0004),
            (Spi(0x005E), Ui, 0x005D),
            (Spi(0x101C), PvValue, 0x101D),
            (Spi(0x1020), PvValue, 0x1021),
            (Spi(0x005F), Ui, 0x0060),
            (SwapMetric, Ui, 0x0021),
            (DoubleClickTime, Ui, 0x0020),
            (Spi(0x101E), PvValue, 0x101F),
            (Spi(0x2008), PvValue, 0x2009),
            (Spi(0x0068), Ui, 0x0069),
            (Spi(0x006C), Ui, 0x006D),
            (Spi(0x201C), PvValue, 0x201D),
            (Spi(0x0016), Ui, 0x0017),
            (Spi(0x000A), Ui, 0x000B),
            (Spi(0x0066), Ui, 0x0067),
            (Spi(0x006A), Ui, 0x006B),
            (Spi(0x0026), Ui, 0x0025),
        ];
        for (d, want) in DESCRIPTORS.iter().zip(table) {
            assert_eq!((d.get, d.set, d.set_action), want, "{}", d.id.key());
            let found = descriptor(d.id);
            assert_eq!(found.id, d.id);
            assert_eq!((found.get, found.set, found.set_action), want);
        }
        assert!(DESCRIPTORS.iter().all(|d| d.set_action != 0x104F));
    }

    #[test]
    fn set_calls_put_the_value_where_each_family_takes_it() {
        use SettingId::*;
        let ok = [
            (
                PointerSpeed,
                Number(14),
                SetCall::PvValue {
                    action: 0x71,
                    pv: 14,
                },
            ),
            (
                ClickLockTime,
                Number(900),
                SetCall::PvValue {
                    action: 0x2009,
                    pv: 900,
                },
            ),
            (
                ScrollInactive,
                Number(2),
                SetCall::PvValue {
                    action: 0x201D,
                    pv: 2,
                },
            ),
            (
                SwapButtons,
                Flag(true),
                SetCall::Ui {
                    action: 0x21,
                    ui: 1,
                },
            ),
            (
                WheelLines,
                Number(WHEEL_PAGESCROLL),
                SetCall::Ui {
                    action: 0x69,
                    ui: u32::MAX,
                },
            ),
            (
                PointerPrecision,
                Mouse([6, 10, 1]),
                SetCall::PvArray {
                    action: 0x4,
                    array: [6, 10, 1],
                },
            ),
            (
                PointerSonar,
                Flag(false),
                SetCall::PvValue {
                    action: 0x101D,
                    pv: 0,
                },
            ),
        ];
        for (id, v, call) in ok {
            assert_eq!(set_call(id, &v), Ok(call), "{}", id.key());
        }
        let refused = [
            (PointerSpeed, Flag(true)),
            (SwapButtons, Number(1)),
            (PointerPrecision, Number(1)),
            (WheelLines, Mouse([6, 10, 1])),
        ];
        for (id, v) in refused {
            assert_eq!(set_call(id, &v), Err(ApiError { code: 87 }), "{}", id.key());
        }
    }

    /// Reads every live value once through the real API; never calls `set`.
    #[test]
    fn every_real_get_reads_its_shape_in_the_documented_range() {
        use SettingId::*;
        for id in SettingId::ALL {
            let v = SpiInput
                .get(id)
                .unwrap_or_else(|e| panic!("{}: {e:?}", id.key()));
            assert_eq!(v.shape(), id.shape(), "{}", id.key());
            let fits = match (id, v) {
                (PointerSpeed, Number(n)) => (1..=20).contains(&n),
                (KeyRepeatDelay, Number(n)) => n <= 3,
                (KeyRepeatRate, Number(n)) => n <= 31,
                (ScrollInactive, Number(n)) => n <= 2,
                (DoubleClickTime, Number(n)) => n <= 5000,
                (PointerPrecision, Mouse([_, _, a])) => (0..=2).contains(&a),
                _ => true,
            };
            assert!(fits, "{} read {v:?}", id.key());
        }
        assert!(SpiInput.filter_keys_on().is_ok());
    }

    #[test]
    fn accounts_compare_ignoring_case() {
        assert_eq!(compare_accounts("PC\\Ana", "pc\\ana"), AccountCheck::Same);
        assert_eq!(
            compare_accounts("PC\\Admin", "PC\\Ana"),
            AccountCheck::Different {
                process: "PC\\Admin".into(),
                session: "PC\\Ana".into(),
            }
        );
        assert_eq!(AccountCheck::Same.problem(), None);
        assert_eq!(
            compare_accounts("PC\\Admin", "PC\\Ana").problem(),
            Some(MkProblem::WrongAccount {
                process: "PC\\Admin".into(),
                session: "PC\\Ana".into(),
            })
        );
        assert_eq!(
            AccountCheck::Unknown("x".into()).problem(),
            Some(MkProblem::AccountUnknown("x".into()))
        );
    }

    #[test]
    fn the_fake_records_calls_fails_and_keeps_other_values() {
        use SettingId::*;
        let fake = FakeInput::new();
        assert_eq!(fake.value(PointerVanish), Flag(true));
        let seen = [
            (PointerSpeed, Number(10)),
            (PointerPrecision, Mouse([6, 10, 1])),
            (PointerTrails, Number(0)),
            (PointerSonar, Flag(false)),
            (PointerVanish, Flag(true)),
            (SnapToDefault, Flag(false)),
            (SwapButtons, Flag(false)),
            (DoubleClickTime, Number(500)),
            (ClickLock, Flag(false)),
            (ClickLockTime, Number(1200)),
            (WheelLines, Number(3)),
            (WheelChars, Number(3)),
            (ScrollInactive, Number(2)),
            (KeyRepeatDelay, Number(1)),
            (KeyRepeatRate, Number(31)),
            (HoverTime, Number(400)),
            (MenuShowDelay, Number(400)),
            (DragFullWindows, Flag(true)),
        ];
        for (id, v) in seen {
            assert_eq!(fake.value(id), v, "{}", id.key());
        }
        assert_eq!(fake.filter_keys_on(), Ok(false));

        assert_eq!(fake.set(PointerSpeed, &Number(12)), Ok(()));
        assert_eq!(fake.get(PointerSpeed), Ok(Number(12)));

        fake.keep(DoubleClickTime, Number(500));
        assert_eq!(fake.set(DoubleClickTime, &Number(700)), Ok(()));
        assert_eq!(fake.get(DoubleClickTime), Ok(Number(500)));

        fake.fail_get(HoverTime);
        assert_eq!(fake.get(HoverTime), Err(ApiError { code: 5 }));

        fake.fail_set(MenuShowDelay);
        assert_eq!(
            fake.set(MenuShowDelay, &Number(200)),
            Err(ApiError { code: 5 })
        );
        assert_eq!(fake.value(MenuShowDelay), Number(400));

        assert_eq!(
            fake.set(SwapButtons, &Number(1)),
            Err(ApiError { code: 87 })
        );

        fake.put(WheelLines, Number(7));
        assert_eq!(fake.value(WheelLines), Number(7));

        fake.set_filter_keys(true);
        assert_eq!(fake.filter_keys_on(), Ok(true));

        assert_eq!(
            fake.calls(),
            vec![
                FakeCall::FilterKeys,
                FakeCall::Set(PointerSpeed, Number(12)),
                FakeCall::Get(PointerSpeed),
                FakeCall::Set(DoubleClickTime, Number(700)),
                FakeCall::Get(DoubleClickTime),
                FakeCall::Get(HoverTime),
                FakeCall::Set(MenuShowDelay, Number(200)),
                FakeCall::Set(SwapButtons, Number(1)),
                FakeCall::FilterKeys,
            ]
        );
        assert_eq!(
            fake.sets(),
            vec![
                (PointerSpeed, Number(12)),
                (DoubleClickTime, Number(700)),
                (MenuShowDelay, Number(200)),
                (SwapButtons, Number(1)),
            ]
        );
    }
}
