//! The pure catalog: effect ids, sections, controls, gates, page ranges,
//! defaults and the write order.

use crate::sysparams::AccountCheck;
pub use crate::sysparams::{SettingValue, Shape};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FE_FONTSMOOTHINGCLEARTYPE, FE_FONTSMOOTHINGSTANDARD,
};

/// Font smoothing type "Standard" (grey scale).
pub const FE_STANDARD: u32 = FE_FONTSMOOTHINGSTANDARD;
/// Font smoothing type "ClearType".
pub const FE_CLEARTYPE: u32 = FE_FONTSMOOTHINGCLEARTYPE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EffectId {
    UiEffects,
    WindowAnimation,
    ClientAreaAnimation,
    ComboboxAnimation,
    ListboxSmoothScroll,
    MenuAnimation,
    MenuFade,
    SelectionFade,
    TooltipAnimation,
    TooltipFade,
    CursorShadow,
    DropShadow,
    FontSmoothing,
    FontSmoothingType,
    CleartypeContrast,
    CaretWidth,
    FocusBorderWidth,
    FocusBorderHeight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VeSection {
    UiEffects,
    Animations,
    MenusTooltips,
    Shadows,
    Text,
    CaretFocus,
}

impl VeSection {
    pub const ALL: [VeSection; 6] = [
        VeSection::UiEffects,
        VeSection::Animations,
        VeSection::MenusTooltips,
        VeSection::Shadows,
        VeSection::Text,
        VeSection::CaretFocus,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VeControl {
    Toggle,
    /// Two choices on a flag: `true` is Fade, `false` is Slide.
    FadeSlide,
    /// Standard / ClearType, a number.
    SmoothingType,
    Slider {
        min: u32,
        max: u32,
        step: u32,
    },
}

/// What a row waits for: it is enabled only while its gate is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// `ui_effects`.
    Master,
    /// That effect's value is on.
    On(EffectId),
    /// Font smoothing on and smoothing type ClearType.
    ClearType,
}

/// Every row gated by the master, directly or through another row (spec 5.3
/// with ruling F10: menu fade and tooltip fade included), in table order.
pub const MASTER_COMPANIONS: [EffectId; 8] = [
    EffectId::ComboboxAnimation,
    EffectId::ListboxSmoothScroll,
    EffectId::MenuAnimation,
    EffectId::MenuFade,
    EffectId::SelectionFade,
    EffectId::TooltipAnimation,
    EffectId::TooltipFade,
    EffectId::CursorShadow,
];

impl EffectId {
    pub const ALL: [EffectId; 18] = [
        EffectId::UiEffects,
        EffectId::WindowAnimation,
        EffectId::ClientAreaAnimation,
        EffectId::ComboboxAnimation,
        EffectId::ListboxSmoothScroll,
        EffectId::MenuAnimation,
        EffectId::MenuFade,
        EffectId::SelectionFade,
        EffectId::TooltipAnimation,
        EffectId::TooltipFade,
        EffectId::CursorShadow,
        EffectId::DropShadow,
        EffectId::FontSmoothing,
        EffectId::FontSmoothingType,
        EffectId::CleartypeContrast,
        EffectId::CaretWidth,
        EffectId::FocusBorderWidth,
        EffectId::FocusBorderHeight,
    ];

    pub fn key(self) -> &'static str {
        match self {
            EffectId::UiEffects => "ui_effects",
            EffectId::WindowAnimation => "window_animation",
            EffectId::ClientAreaAnimation => "client_area_animation",
            EffectId::ComboboxAnimation => "combobox_animation",
            EffectId::ListboxSmoothScroll => "listbox_smooth_scroll",
            EffectId::MenuAnimation => "menu_animation",
            EffectId::MenuFade => "menu_fade",
            EffectId::SelectionFade => "selection_fade",
            EffectId::TooltipAnimation => "tooltip_animation",
            EffectId::TooltipFade => "tooltip_fade",
            EffectId::CursorShadow => "cursor_shadow",
            EffectId::DropShadow => "drop_shadow",
            EffectId::FontSmoothing => "font_smoothing",
            EffectId::FontSmoothingType => "font_smoothing_type",
            EffectId::CleartypeContrast => "cleartype_contrast",
            EffectId::CaretWidth => "caret_width",
            EffectId::FocusBorderWidth => "focus_border_width",
            EffectId::FocusBorderHeight => "focus_border_height",
        }
    }

    pub fn from_key(key: &str) -> Option<EffectId> {
        EffectId::ALL.into_iter().find(|id| id.key() == key)
    }

    pub fn section(self) -> VeSection {
        use EffectId::*;
        match self {
            UiEffects => VeSection::UiEffects,
            WindowAnimation | ClientAreaAnimation | ComboboxAnimation | ListboxSmoothScroll => {
                VeSection::Animations
            }
            MenuAnimation | MenuFade | SelectionFade | TooltipAnimation | TooltipFade => {
                VeSection::MenusTooltips
            }
            CursorShadow | DropShadow => VeSection::Shadows,
            FontSmoothing | FontSmoothingType | CleartypeContrast => VeSection::Text,
            CaretWidth | FocusBorderWidth | FocusBorderHeight => VeSection::CaretFocus,
        }
    }

    pub fn shape(self) -> Shape {
        use EffectId::*;
        match self {
            FontSmoothingType | CleartypeContrast | CaretWidth | FocusBorderWidth
            | FocusBorderHeight => Shape::Number,
            _ => Shape::Flag,
        }
    }

    pub fn control(self) -> VeControl {
        use EffectId::*;
        match self {
            MenuFade | TooltipFade => VeControl::FadeSlide,
            FontSmoothingType => VeControl::SmoothingType,
            CleartypeContrast => VeControl::Slider {
                min: 1000,
                max: 2200,
                step: 100,
            },
            CaretWidth => VeControl::Slider {
                min: 1,
                max: 20,
                step: 1,
            },
            FocusBorderWidth | FocusBorderHeight => VeControl::Slider {
                min: 1,
                max: 10,
                step: 1,
            },
            _ => VeControl::Toggle,
        }
    }

    pub fn windows_default(self) -> SettingValue {
        use EffectId::*;
        match self {
            FontSmoothingType => SettingValue::Number(FE_CLEARTYPE),
            CleartypeContrast => SettingValue::Number(1400),
            CaretWidth | FocusBorderWidth | FocusBorderHeight => SettingValue::Number(1),
            _ => SettingValue::Flag(true),
        }
    }

    /// Whether `v` is a value the page may choose. A shape mismatch is
    /// refused; the slider step is not checked.
    pub fn in_page_range(self, v: &SettingValue) -> bool {
        if v.shape() != self.shape() {
            return false;
        }
        match (*v, self.control()) {
            (SettingValue::Number(n), VeControl::SmoothingType) => {
                n == FE_STANDARD || n == FE_CLEARTYPE
            }
            (SettingValue::Number(n), VeControl::Slider { min, max, .. }) => {
                (min..=max).contains(&n)
            }
            (SettingValue::Flag(_), _) => true,
            _ => false,
        }
    }

    pub fn gate(self) -> Option<Gate> {
        use EffectId::*;
        match self {
            ComboboxAnimation | ListboxSmoothScroll | MenuAnimation | SelectionFade
            | TooltipAnimation | CursorShadow => Some(Gate::Master),
            MenuFade => Some(Gate::On(MenuAnimation)),
            TooltipFade => Some(Gate::On(TooltipAnimation)),
            FontSmoothingType => Some(Gate::On(FontSmoothing)),
            CleartypeContrast => Some(Gate::ClearType),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VeChange {
    pub id: EffectId,
    pub value: SettingValue,
}

pub fn check_change(c: &VeChange) -> Option<VeProblem> {
    (!c.id.in_page_range(&c.value)).then_some(VeProblem::OutOfRange(c.id))
}

/// The ids in table order, deduplicated, except `ui_effects`: first when
/// `master_target` is `Some(true)`, last when `Some(false)` (spec 5.4).
pub fn write_order(ids: &[EffectId], master_target: Option<bool>) -> Vec<EffectId> {
    let mut out: Vec<EffectId> = EffectId::ALL
        .into_iter()
        .filter(|id| ids.contains(id))
        .collect();
    if let Some(on) = master_target {
        if let Some(pos) = out.iter().position(|id| *id == EffectId::UiEffects) {
            let master = out.remove(pos);
            if on {
                out.insert(0, master);
            } else {
                out.push(master);
            }
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VeProblem {
    OutOfRange(EffectId),
    UnknownSetting(String),
    Unreadable {
        id: EffectId,
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
pub enum VeFailure {
    SetFailed {
        id: EffectId,
        code: u32,
    },
    Mismatch {
        id: EffectId,
        wanted: SettingValue,
        read: SettingValue,
    },
    /// The readback GET failed.
    Unverified {
        id: EffectId,
        code: u32,
    },
    /// The value differs while the master is live off (spec 5.3).
    HeldByMaster {
        id: EffectId,
        wanted: SettingValue,
        read: SettingValue,
    },
    /// The store save after the writes failed.
    StoreNotSaved(String),
    /// A gated row not written: its snapshot could not be read again after
    /// the master went on (review T3-C1, T3-C6).
    RereadFailed {
        id: EffectId,
        code: u32,
    },
    /// The master was written, but the rows it gates were not: the store
    /// save after their re-read failed (review T3-C1, T3-C6).
    GatedNotSaved(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VeIssue {
    Problem(VeProblem),
    Failure(VeFailure),
}

pub fn account_problem(check: &AccountCheck) -> Option<VeProblem> {
    match check {
        AccountCheck::Same => None,
        AccountCheck::Different { process, session } => Some(VeProblem::WrongAccount {
            process: process.clone(),
            session: session.clone(),
        }),
        AccountCheck::Unknown(why) => Some(VeProblem::AccountUnknown(why.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::SettingValue::*;
    use super::*;

    const KEYS: [&str; 18] = [
        "ui_effects",
        "window_animation",
        "client_area_animation",
        "combobox_animation",
        "listbox_smooth_scroll",
        "menu_animation",
        "menu_fade",
        "selection_fade",
        "tooltip_animation",
        "tooltip_fade",
        "cursor_shadow",
        "drop_shadow",
        "font_smoothing",
        "font_smoothing_type",
        "cleartype_contrast",
        "caret_width",
        "focus_border_width",
        "focus_border_height",
    ];

    #[test]
    fn ids_are_the_spec_table_in_order() {
        assert_eq!(EffectId::ALL.map(EffectId::key), KEYS);
        for id in EffectId::ALL {
            assert_eq!(EffectId::from_key(id.key()), Some(id));
        }
        assert_eq!(EffectId::from_key("hot_tracking"), None);
        let mut counts = [0usize; 6];
        for id in EffectId::ALL {
            let pos = VeSection::ALL.iter().position(|s| *s == id.section());
            counts[pos.unwrap()] += 1;
        }
        assert_eq!(counts, [1, 4, 5, 2, 3, 3]);
        assert!(EffectId::ALL.windows(2).all(|w| w[0] < w[1]));
        assert!(EffectId::ALL
            .windows(2)
            .all(|w| w[0].section() <= w[1].section()));
    }

    #[test]
    fn defaults_follow_spec_5_1() {
        use EffectId::*;
        assert_eq!(FontSmoothingType.windows_default(), Number(2));
        assert_eq!(CleartypeContrast.windows_default(), Number(1400));
        assert_eq!(CaretWidth.windows_default(), Number(1));
        assert_eq!(FocusBorderWidth.windows_default(), Number(1));
        assert_eq!(FocusBorderHeight.windows_default(), Number(1));
        assert_eq!(MenuFade.windows_default(), Flag(true));
        assert_eq!(UiEffects.windows_default(), Flag(true));
        for id in EffectId::ALL {
            if id.shape() == Shape::Flag {
                assert_eq!(id.windows_default(), Flag(true), "{}", id.key());
            }
            assert!(id.in_page_range(&id.windows_default()), "{}", id.key());
        }
        assert_eq!((FE_STANDARD, FE_CLEARTYPE), (1, 2));
    }

    #[test]
    fn page_ranges_follow_spec_5_1() {
        use EffectId::*;
        let accepted = [
            (FontSmoothingType, 1),
            (FontSmoothingType, 2),
            (CleartypeContrast, 1000),
            (CleartypeContrast, 1250),
            (CleartypeContrast, 2200),
            (CaretWidth, 1),
            (CaretWidth, 20),
            (FocusBorderWidth, 1),
            (FocusBorderWidth, 10),
            (FocusBorderHeight, 1),
            (FocusBorderHeight, 10),
        ];
        for (id, n) in accepted {
            assert!(id.in_page_range(&Number(n)), "{} {n}", id.key());
        }
        let refused = [
            (FontSmoothingType, 0),
            (FontSmoothingType, 3),
            (CleartypeContrast, 999),
            (CleartypeContrast, 2201),
            (CaretWidth, 0),
            (CaretWidth, 21),
            (FocusBorderWidth, 0),
            (FocusBorderWidth, 11),
            (FocusBorderHeight, 0),
            (FocusBorderHeight, 11),
        ];
        for (id, n) in refused {
            assert!(!id.in_page_range(&Number(n)), "{} {n}", id.key());
        }
        assert!(!CaretWidth.in_page_range(&Flag(true)));
        assert!(!CursorShadow.in_page_range(&Number(1)));
        assert!(CursorShadow.in_page_range(&Flag(false)));
    }

    #[test]
    fn controls_follow_spec_5_1() {
        use EffectId::*;
        assert_eq!(MenuFade.control(), VeControl::FadeSlide);
        assert_eq!(TooltipFade.control(), VeControl::FadeSlide);
        assert_eq!(FontSmoothingType.control(), VeControl::SmoothingType);
        assert_eq!(
            CleartypeContrast.control(),
            VeControl::Slider {
                min: 1000,
                max: 2200,
                step: 100
            }
        );
        assert_eq!(UiEffects.control(), VeControl::Toggle);
        assert_eq!(WindowAnimation.control(), VeControl::Toggle);
    }

    #[test]
    fn gates_follow_spec_5_1() {
        use EffectId::*;
        let want = [
            None,
            None,
            None,
            Some(Gate::Master),
            Some(Gate::Master),
            Some(Gate::Master),
            Some(Gate::On(MenuAnimation)),
            Some(Gate::Master),
            Some(Gate::Master),
            Some(Gate::On(TooltipAnimation)),
            Some(Gate::Master),
            None,
            None,
            Some(Gate::On(FontSmoothing)),
            Some(Gate::ClearType),
            None,
            None,
            None,
        ];
        assert_eq!(EffectId::ALL.map(EffectId::gate), want);
        // Every row gated by the master directly or through another row.
        assert_eq!(
            MASTER_COMPANIONS,
            [
                ComboboxAnimation,
                ListboxSmoothScroll,
                MenuAnimation,
                MenuFade,
                SelectionFade,
                TooltipAnimation,
                TooltipFade,
                CursorShadow
            ]
        );
        let derived: Vec<EffectId> = EffectId::ALL
            .into_iter()
            .filter(|id| {
                let mut g = id.gate();
                while let Some(Gate::On(next)) = g {
                    g = next.gate();
                }
                g == Some(Gate::Master)
            })
            .collect();
        assert_eq!(derived, MASTER_COMPANIONS);
    }

    #[test]
    fn the_master_is_written_first_going_on_and_last_going_off() {
        use EffectId::*;
        let ids = [CursorShadow, UiEffects, MenuAnimation];
        assert_eq!(
            write_order(&ids, Some(true)),
            [UiEffects, MenuAnimation, CursorShadow]
        );
        assert_eq!(
            write_order(&ids, Some(false)),
            [MenuAnimation, CursorShadow, UiEffects]
        );
        assert_eq!(
            write_order(&[CaretWidth, FontSmoothingType, FontSmoothing], None),
            [FontSmoothing, FontSmoothingType, CaretWidth]
        );
        assert_eq!(
            write_order(&[CaretWidth, CaretWidth], Some(true)),
            [CaretWidth]
        );
    }

    #[test]
    fn check_change_and_account_problem() {
        use EffectId::*;
        let bad = VeChange {
            id: CleartypeContrast,
            value: Number(2300),
        };
        assert_eq!(
            check_change(&bad),
            Some(VeProblem::OutOfRange(CleartypeContrast))
        );
        let good = VeChange {
            id: CaretWidth,
            value: Number(5),
        };
        assert_eq!(check_change(&good), None);
        assert_eq!(account_problem(&AccountCheck::Same), None);
        assert_eq!(
            account_problem(&AccountCheck::Unknown("x".into())),
            Some(VeProblem::AccountUnknown("x".into()))
        );
        assert_eq!(
            account_problem(&AccountCheck::Different {
                process: "a".into(),
                session: "b".into()
            }),
            Some(VeProblem::WrongAccount {
                process: "a".into(),
                session: "b".into()
            })
        );
    }
}
