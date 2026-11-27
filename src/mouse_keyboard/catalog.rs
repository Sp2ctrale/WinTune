//! The pure catalog: setting ids, sections, page ranges, defaults and the
//! rules that turn a wanted choice into a concrete value.

use serde::{Deserialize, Serialize};

/// Wheel lines value meaning "one screen at a time".
pub const WHEEL_PAGESCROLL: u32 = u32::MAX;
/// Pointer precision array Windows uses when it is on: `[t1, t2, accel]`.
pub const PRECISION_DEFAULT: [i32; 3] = [6, 10, 1];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SettingId {
    PointerSpeed,
    PointerPrecision,
    PointerTrails,
    PointerSonar,
    PointerVanish,
    SnapToDefault,
    SwapButtons,
    DoubleClickTime,
    ClickLock,
    ClickLockTime,
    WheelLines,
    WheelChars,
    ScrollInactive,
    KeyRepeatDelay,
    KeyRepeatRate,
    HoverTime,
    MenuShowDelay,
    DragFullWindows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    Pointer,
    Buttons,
    Wheel,
    Keyboard,
    WindowsMenus,
}

impl Section {
    pub const ALL: [Section; 5] = [
        Section::Pointer,
        Section::Buttons,
        Section::Wheel,
        Section::Keyboard,
        Section::WindowsMenus,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Number,
    Flag,
    Mouse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum SettingValue {
    Number(u32),
    Flag(bool),
    Mouse([i32; 3]),
}

impl SettingValue {
    pub fn shape(&self) -> Shape {
        match self {
            SettingValue::Number(_) => Shape::Number,
            SettingValue::Flag(_) => Shape::Flag,
            SettingValue::Mouse(_) => Shape::Mouse,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliderSpec {
    pub min: u32,
    pub max: u32,
    pub step: u32,
}

const fn spec(min: u32, max: u32, step: u32) -> SliderSpec {
    SliderSpec { min, max, step }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Toggle,
    Slider(SliderSpec),
    TrailsLength(SliderSpec),
    LinesOrScreen(SliderSpec),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wanted {
    Value(SettingValue),
    /// Only for `PointerPrecision` and `ScrollInactive`; resolved from the
    /// live read at Apply time.
    Toggle(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MkChange {
    pub id: SettingId,
    pub wanted: Wanted,
}

impl SettingId {
    pub const ALL: [SettingId; 18] = [
        SettingId::PointerSpeed,
        SettingId::PointerPrecision,
        SettingId::PointerTrails,
        SettingId::PointerSonar,
        SettingId::PointerVanish,
        SettingId::SnapToDefault,
        SettingId::SwapButtons,
        SettingId::DoubleClickTime,
        SettingId::ClickLock,
        SettingId::ClickLockTime,
        SettingId::WheelLines,
        SettingId::WheelChars,
        SettingId::ScrollInactive,
        SettingId::KeyRepeatDelay,
        SettingId::KeyRepeatRate,
        SettingId::HoverTime,
        SettingId::MenuShowDelay,
        SettingId::DragFullWindows,
    ];

    pub fn key(self) -> &'static str {
        match self {
            SettingId::PointerSpeed => "pointer_speed",
            SettingId::PointerPrecision => "pointer_precision",
            SettingId::PointerTrails => "pointer_trails",
            SettingId::PointerSonar => "pointer_sonar",
            SettingId::PointerVanish => "pointer_vanish",
            SettingId::SnapToDefault => "snap_to_default",
            SettingId::SwapButtons => "swap_buttons",
            SettingId::DoubleClickTime => "double_click_time",
            SettingId::ClickLock => "click_lock",
            SettingId::ClickLockTime => "click_lock_time",
            SettingId::WheelLines => "wheel_lines",
            SettingId::WheelChars => "wheel_chars",
            SettingId::ScrollInactive => "scroll_inactive",
            SettingId::KeyRepeatDelay => "key_repeat_delay",
            SettingId::KeyRepeatRate => "key_repeat_rate",
            SettingId::HoverTime => "hover_time",
            SettingId::MenuShowDelay => "menu_show_delay",
            SettingId::DragFullWindows => "drag_full_windows",
        }
    }

    pub fn from_key(key: &str) -> Option<SettingId> {
        SettingId::ALL.into_iter().find(|id| id.key() == key)
    }

    pub fn section(self) -> Section {
        use SettingId::*;
        match self {
            PointerSpeed | PointerPrecision | PointerTrails | PointerSonar | PointerVanish
            | SnapToDefault => Section::Pointer,
            SwapButtons | DoubleClickTime | ClickLock | ClickLockTime => Section::Buttons,
            WheelLines | WheelChars | ScrollInactive => Section::Wheel,
            KeyRepeatDelay | KeyRepeatRate => Section::Keyboard,
            HoverTime | MenuShowDelay | DragFullWindows => Section::WindowsMenus,
        }
    }

    pub fn shape(self) -> Shape {
        use SettingId::*;
        match self {
            PointerSonar | PointerVanish | SnapToDefault | SwapButtons | ClickLock
            | DragFullWindows => Shape::Flag,
            PointerPrecision => Shape::Mouse,
            _ => Shape::Number,
        }
    }

    pub fn control(self) -> Control {
        use SettingId::*;
        match self {
            PointerSpeed => Control::Slider(spec(1, 20, 1)),
            PointerTrails => Control::TrailsLength(spec(2, 7, 1)),
            DoubleClickTime => Control::Slider(spec(200, 900, 10)),
            ClickLockTime => Control::Slider(spec(200, 2200, 100)),
            WheelLines => Control::LinesOrScreen(spec(1, 100, 1)),
            WheelChars => Control::Slider(spec(1, 100, 1)),
            KeyRepeatDelay => Control::Slider(spec(0, 3, 1)),
            KeyRepeatRate => Control::Slider(spec(0, 31, 1)),
            HoverTime => Control::Slider(spec(100, 3000, 50)),
            MenuShowDelay => Control::Slider(spec(0, 2000, 50)),
            PointerPrecision | PointerSonar | PointerVanish | SnapToDefault | SwapButtons
            | ClickLock | ScrollInactive | DragFullWindows => Control::Toggle,
        }
    }

    pub fn windows_default(self) -> SettingValue {
        use SettingId::*;
        use SettingValue::*;
        match self {
            PointerSpeed => Number(10),
            PointerPrecision => Mouse(PRECISION_DEFAULT),
            PointerTrails => Number(0),
            PointerSonar => Flag(false),
            PointerVanish => Flag(true),
            SnapToDefault => Flag(false),
            SwapButtons => Flag(false),
            DoubleClickTime => Number(500),
            ClickLock => Flag(false),
            ClickLockTime => Number(1200),
            WheelLines => Number(3),
            WheelChars => Number(3),
            ScrollInactive => Number(2),
            KeyRepeatDelay => Number(1),
            KeyRepeatRate => Number(31),
            HoverTime => Number(400),
            MenuShowDelay => Number(400),
            DragFullWindows => Flag(true),
        }
    }

    /// Whether `v` is a value the page may choose. A shape mismatch is
    /// refused; the slider step is not checked.
    pub fn in_page_range(self, v: &SettingValue) -> bool {
        if v.shape() != self.shape() {
            return false;
        }
        match *v {
            SettingValue::Flag(_) => true,
            SettingValue::Mouse([t1, t2, a]) => t1 >= 0 && t2 >= 0 && (0..=2).contains(&a),
            SettingValue::Number(n) => match self {
                SettingId::PointerTrails => n == 0 || (2..=7).contains(&n),
                SettingId::WheelLines => n == WHEEL_PAGESCROLL || (1..=100).contains(&n),
                SettingId::ScrollInactive => n <= 2,
                _ => match self.control() {
                    Control::Slider(s) | Control::TrailsLength(s) | Control::LinesOrScreen(s) => {
                        (s.min..=s.max).contains(&n)
                    }
                    Control::Toggle => false,
                },
            },
        }
    }
}

/// Whether a toggle-style value reads as "on"; `None` for ids that are not
/// toggles.
pub fn is_on(id: SettingId, v: &SettingValue) -> Option<bool> {
    match (id, v) {
        (_, SettingValue::Flag(b)) => Some(*b),
        (SettingId::PointerPrecision, SettingValue::Mouse([_, _, a])) => Some(*a != 0),
        (SettingId::ScrollInactive, SettingValue::Number(n)) => Some(*n != 0),
        (SettingId::PointerTrails, SettingValue::Number(n)) => Some(*n >= 2),
        _ => None,
    }
}

/// Turn a wanted choice into the value to SET, given the live read and the
/// saved original. `None` when the pair cannot be resolved.
pub fn resolve(
    id: SettingId,
    wanted: &Wanted,
    live: &SettingValue,
    original: &SettingValue,
) -> Option<SettingValue> {
    match *wanted {
        Wanted::Value(v) => Some(v),
        Wanted::Toggle(on) => match id {
            SettingId::PointerPrecision => {
                let SettingValue::Mouse([t1, t2, _]) = *live else {
                    return None;
                };
                if !on {
                    return Some(SettingValue::Mouse([t1, t2, 0]));
                }
                match *original {
                    SettingValue::Mouse([_, _, a]) if a != 0 => Some(*original),
                    _ if t1 != 0 && t2 != 0 => Some(SettingValue::Mouse([t1, t2, 1])),
                    _ => Some(SettingValue::Mouse(PRECISION_DEFAULT)),
                }
            }
            SettingId::ScrollInactive => {
                if !on {
                    return Some(SettingValue::Number(0));
                }
                match *original {
                    SettingValue::Number(n) if n != 0 => Some(*original),
                    _ => Some(SettingValue::Number(2)),
                }
            }
            _ => None,
        },
    }
}

pub fn wanted_matches(id: SettingId, wanted: &Wanted, live: &SettingValue) -> bool {
    match wanted {
        Wanted::Value(v) => live == v,
        Wanted::Toggle(b) => is_on(id, live) == Some(*b),
    }
}

/// Trail length to start from when the user turns trails on.
pub fn trails_start_length(original: &SettingValue) -> u32 {
    match *original {
        SettingValue::Number(n) if n >= 2 => n.min(7),
        _ => 7,
    }
}

pub fn check_change(c: &MkChange) -> Option<MkProblem> {
    let ok = match &c.wanted {
        Wanted::Value(v) => c.id.in_page_range(v),
        Wanted::Toggle(_) => matches!(
            c.id,
            SettingId::PointerPrecision | SettingId::ScrollInactive
        ),
    };
    (!ok).then_some(MkProblem::OutOfRange(c.id))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MkProblem {
    OutOfRange(SettingId),
    UnknownSetting(String),
    Unreadable {
        id: SettingId,
        code: u32,
    },
    WrongAccount {
        process: String,
        session: String,
    },
    AccountUnknown(String),
    /// The store or `.reg` could not be written before any SET.
    NotSaved(String),
    /// A restore id with no snapshot.
    NoOriginal(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MkFailure {
    SetFailed {
        id: SettingId,
        code: u32,
    },
    Mismatch {
        id: SettingId,
        wanted: SettingValue,
        read: SettingValue,
    },
    /// The readback GET failed.
    Unverified {
        id: SettingId,
        code: u32,
    },
    /// The store save after the writes failed.
    StoreNotSaved(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MkIssue {
    Problem(MkProblem),
    Failure(MkFailure),
}

#[cfg(test)]
mod tests {
    use super::SettingValue::*;
    use super::*;

    const KEYS: [&str; 18] = [
        "pointer_speed",
        "pointer_precision",
        "pointer_trails",
        "pointer_sonar",
        "pointer_vanish",
        "snap_to_default",
        "swap_buttons",
        "double_click_time",
        "click_lock",
        "click_lock_time",
        "wheel_lines",
        "wheel_chars",
        "scroll_inactive",
        "key_repeat_delay",
        "key_repeat_rate",
        "hover_time",
        "menu_show_delay",
        "drag_full_windows",
    ];

    #[test]
    fn ids_are_the_spec_table_in_order() {
        assert_eq!(SettingId::ALL.map(SettingId::key), KEYS);
        for id in SettingId::ALL {
            assert_eq!(SettingId::from_key(id.key()), Some(id));
        }
        assert_eq!(SettingId::from_key("caret_blink"), None);
        use Section::*;
        assert_eq!(
            SettingId::ALL.map(SettingId::section),
            [
                Pointer,
                Pointer,
                Pointer,
                Pointer,
                Pointer,
                Pointer,
                Buttons,
                Buttons,
                Buttons,
                Buttons,
                Wheel,
                Wheel,
                Wheel,
                Keyboard,
                Keyboard,
                WindowsMenus,
                WindowsMenus,
                WindowsMenus,
            ]
        );
        assert!(SettingId::ALL.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn defaults_follow_spec_5_1() {
        use SettingId::*;
        assert_eq!(PointerSpeed.windows_default(), Number(10));
        assert_eq!(PointerPrecision.windows_default(), Mouse([6, 10, 1]));
        assert_eq!(PointerVanish.windows_default(), Flag(true));
        assert_eq!(DoubleClickTime.windows_default(), Number(500));
        assert_eq!(ClickLockTime.windows_default(), Number(1200));
        assert_eq!(ScrollInactive.windows_default(), Number(2));
        assert_eq!(KeyRepeatDelay.windows_default(), Number(1));
        assert_eq!(KeyRepeatRate.windows_default(), Number(31));
        assert_eq!(HoverTime.windows_default(), Number(400));
        assert_eq!(MenuShowDelay.windows_default(), Number(400));
        assert_eq!(DragFullWindows.windows_default(), Flag(true));
        assert_eq!(PointerTrails.windows_default(), Number(0));
        for id in SettingId::ALL {
            assert!(id.in_page_range(&id.windows_default()), "{}", id.key());
        }
    }

    #[test]
    fn page_ranges_follow_spec_5_1() {
        use SettingId::*;
        let ok = [
            (PointerSpeed, Number(1)),
            (PointerSpeed, Number(20)),
            (PointerTrails, Number(0)),
            (PointerTrails, Number(2)),
            (PointerTrails, Number(7)),
            (DoubleClickTime, Number(200)),
            (DoubleClickTime, Number(900)),
            (WheelLines, Number(1)),
            (WheelLines, Number(100)),
            (WheelLines, Number(WHEEL_PAGESCROLL)),
            (ScrollInactive, Number(0)),
            (ScrollInactive, Number(1)),
            (ScrollInactive, Number(2)),
            (KeyRepeatDelay, Number(3)),
            (KeyRepeatRate, Number(31)),
            (HoverTime, Number(100)),
            (HoverTime, Number(3000)),
            (MenuShowDelay, Number(0)),
            (MenuShowDelay, Number(2000)),
            (ClickLockTime, Number(2200)),
            (PointerPrecision, Mouse([4, 12, 2])),
        ];
        for (id, v) in ok {
            assert!(id.in_page_range(&v), "{} {:?}", id.key(), v);
        }
        let refused = [
            (PointerSpeed, Number(0)),
            (PointerSpeed, Number(21)),
            (PointerTrails, Number(1)),
            (PointerTrails, Number(8)),
            (DoubleClickTime, Number(199)),
            (DoubleClickTime, Number(901)),
            (WheelLines, Number(0)),
            (WheelLines, Number(101)),
            (WheelChars, Number(0)),
            (ScrollInactive, Number(3)),
            (KeyRepeatDelay, Number(4)),
            (KeyRepeatRate, Number(32)),
            (PointerPrecision, Mouse([6, 10, 3])),
            (PointerPrecision, Mouse([6, 10, -1])),
            (PointerPrecision, Mouse([-1, 10, 1])),
            (PointerSpeed, Flag(true)),
            (SwapButtons, Number(1)),
        ];
        for (id, v) in refused {
            assert!(!id.in_page_range(&v), "{} {:?}", id.key(), v);
        }
    }

    #[test]
    fn toggles_show_on_as_the_spec_says() {
        use SettingId::*;
        assert_eq!(is_on(PointerTrails, &Number(1)), Some(false));
        assert_eq!(is_on(PointerTrails, &Number(2)), Some(true));
        assert_eq!(is_on(ScrollInactive, &Number(1)), Some(true));
        assert_eq!(is_on(PointerPrecision, &Mouse([6, 10, 0])), Some(false));
        assert_eq!(is_on(PointerSpeed, &Number(10)), None);
    }

    #[test]
    fn precision_toggles_resolve_from_live_and_original() {
        use SettingId::PointerPrecision as P;
        let r =
            |w: Wanted, live: [i32; 3], orig: [i32; 3]| resolve(P, &w, &Mouse(live), &Mouse(orig));
        assert_eq!(
            r(Wanted::Toggle(false), [4, 12, 2], [4, 12, 2]),
            Some(Mouse([4, 12, 0]))
        );
        assert_eq!(
            r(Wanted::Toggle(true), [4, 12, 0], [4, 12, 2]),
            Some(Mouse([4, 12, 2]))
        );
        assert_eq!(
            r(Wanted::Toggle(true), [5, 9, 0], [5, 9, 0]),
            Some(Mouse([5, 9, 1]))
        );
        assert_eq!(
            r(Wanted::Toggle(true), [0, 0, 0], [0, 0, 0]),
            Some(Mouse([6, 10, 1]))
        );
        assert_eq!(
            r(Wanted::Value(Mouse([6, 10, 1])), [4, 12, 2], [4, 12, 2]),
            Some(Mouse([6, 10, 1]))
        );
        assert_eq!(
            resolve(P, &Wanted::Toggle(true), &Number(1), &Number(1)),
            None
        );
    }

    #[test]
    fn scroll_inactive_toggles_resolve_from_the_original() {
        use SettingId::ScrollInactive as S;
        let r = |on: bool, orig: u32| resolve(S, &Wanted::Toggle(on), &Number(2), &Number(orig));
        assert_eq!(r(false, 2), Some(Number(0)));
        assert_eq!(r(true, 1), Some(Number(1)));
        assert_eq!(r(true, 0), Some(Number(2)));
        assert_eq!(
            resolve(
                SettingId::SwapButtons,
                &Wanted::Toggle(true),
                &Flag(false),
                &Flag(false)
            ),
            None
        );
    }

    #[test]
    fn wanted_matches_and_trails_length() {
        assert!(wanted_matches(
            SettingId::PointerPrecision,
            &Wanted::Toggle(true),
            &Mouse([6, 10, 1])
        ));
        assert!(!wanted_matches(
            SettingId::PointerSpeed,
            &Wanted::Value(Number(12)),
            &Number(10)
        ));
        let lens = [0, 1, 5, 9].map(|n| trails_start_length(&Number(n)));
        assert_eq!(lens, [7, 7, 5, 7]);
    }

    #[test]
    fn check_change_refuses_out_of_range_and_stray_toggles() {
        let c = |id, wanted| MkChange { id, wanted };
        assert_eq!(
            check_change(&c(SettingId::PointerSpeed, Wanted::Value(Number(21)))),
            Some(MkProblem::OutOfRange(SettingId::PointerSpeed))
        );
        assert_eq!(
            check_change(&c(SettingId::SwapButtons, Wanted::Toggle(true))),
            Some(MkProblem::OutOfRange(SettingId::SwapButtons))
        );
        assert_eq!(
            check_change(&c(SettingId::PointerSpeed, Wanted::Value(Number(12)))),
            None
        );
        assert_eq!(
            check_change(&c(SettingId::ScrollInactive, Wanted::Toggle(true))),
            None
        );
    }

    #[test]
    fn values_serialize_tagged() {
        for (v, json) in [
            (Number(10), r#"{"kind":"Number","data":10}"#),
            (Flag(false), r#"{"kind":"Flag","data":false}"#),
            (Mouse([6, 10, 1]), r#"{"kind":"Mouse","data":[6,10,1]}"#),
        ] {
            assert_eq!(serde_json::to_string(&v).unwrap(), json);
            assert_eq!(serde_json::from_str::<SettingValue>(json).unwrap(), v);
        }
    }
}
