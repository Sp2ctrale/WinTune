//! The effects backend: `SystemParametersInfoW` behind the `EffectsApi`
//! trait, and a fake for tests.

use super::{EffectId, SettingValue, Shape};
pub use crate::sysparams::ApiError;
use crate::sysparams::{spi_get, spi_get_animation, spi_set, SetCall, WRONG_SHAPE};

pub trait EffectsApi: Send + Sync {
    fn get(&self, id: EffectId) -> Result<SettingValue, ApiError>;
    fn set(&self, id: EffectId, value: &SettingValue) -> Result<(), ApiError>;
}

/// Where the SET takes the value (spec 5.1: "ui" / "pv" / "ai").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VeFamily {
    /// The value (or the flag as 0/1) in `uiParam`.
    Ui,
    /// The value in `pvParam`, cast to a pointer; `uiParam` 0.
    PvValue,
    /// `pvParam` points to an `ANIMATIONINFO`; `uiParam` 8. The GET too.
    Animation,
}

#[derive(Debug)]
pub(crate) struct VeDescriptor {
    pub(crate) id: EffectId,
    pub(crate) get_action: u32,
    pub(crate) family: VeFamily,
    pub(crate) set_action: u32,
}

const fn ve(id: EffectId, get_action: u32, family: VeFamily, set_action: u32) -> VeDescriptor {
    VeDescriptor {
        id,
        get_action,
        family,
        set_action,
    }
}

/// One row per effect, in the order of spec 5.1 (`EffectId::ALL`).
pub(crate) const VE_DESCRIPTORS: [VeDescriptor; 18] = {
    use EffectId::*;
    use VeFamily::*;
    [
        ve(UiEffects, 0x103E, PvValue, 0x103F),
        ve(WindowAnimation, 0x0048, Animation, 0x0049),
        ve(ClientAreaAnimation, 0x1042, PvValue, 0x1043),
        ve(ComboboxAnimation, 0x1004, PvValue, 0x1005),
        ve(ListboxSmoothScroll, 0x1006, PvValue, 0x1007),
        ve(MenuAnimation, 0x1002, PvValue, 0x1003),
        ve(MenuFade, 0x1012, PvValue, 0x1013),
        ve(SelectionFade, 0x1014, PvValue, 0x1015),
        ve(TooltipAnimation, 0x1016, PvValue, 0x1017),
        ve(TooltipFade, 0x1018, PvValue, 0x1019),
        ve(CursorShadow, 0x101A, PvValue, 0x101B),
        ve(DropShadow, 0x1024, PvValue, 0x1025),
        ve(FontSmoothing, 0x004A, Ui, 0x004B),
        ve(FontSmoothingType, 0x200A, PvValue, 0x200B),
        ve(CleartypeContrast, 0x200C, PvValue, 0x200D),
        ve(CaretWidth, 0x2006, PvValue, 0x2007),
        ve(FocusBorderWidth, 0x200E, PvValue, 0x200F),
        ve(FocusBorderHeight, 0x2010, PvValue, 0x2011),
    ]
};

pub(crate) fn ve_descriptor(id: EffectId) -> &'static VeDescriptor {
    VE_DESCRIPTORS
        .iter()
        .find(|d| d.id == id)
        .expect("every effect id has a descriptor")
}

/// The SET parameters for `value`; a value of another shape than the id's,
/// or one its family cannot carry, gives error 87.
pub(crate) fn ve_set_call(id: EffectId, value: &SettingValue) -> Result<SetCall, ApiError> {
    if value.shape() != id.shape() {
        return Err(WRONG_SHAPE);
    }
    let d = ve_descriptor(id);
    let action = d.set_action;
    match (d.family, *value) {
        (VeFamily::Ui, SettingValue::Number(n)) => Ok(SetCall::Ui { action, ui: n }),
        (VeFamily::Ui, SettingValue::Flag(b)) => Ok(SetCall::Ui {
            action,
            ui: u32::from(b),
        }),
        (VeFamily::PvValue, SettingValue::Number(n)) => Ok(SetCall::PvValue {
            action,
            pv: n as usize,
        }),
        (VeFamily::PvValue, SettingValue::Flag(b)) => Ok(SetCall::PvValue {
            action,
            pv: usize::from(b),
        }),
        (VeFamily::Animation, SettingValue::Flag(on)) => Ok(SetCall::Animation { action, on }),
        _ => Err(WRONG_SHAPE),
    }
}

/// The real backend: `SystemParametersInfoW`.
pub struct SpiEffects;

impl EffectsApi for SpiEffects {
    fn get(&self, id: EffectId) -> Result<SettingValue, ApiError> {
        let d = ve_descriptor(id);
        if d.family == VeFamily::Animation {
            return spi_get_animation().map(SettingValue::Flag);
        }
        let mut out = 0u32;
        spi_get(d.get_action, &mut out)?;
        Ok(match id.shape() {
            Shape::Flag => SettingValue::Flag(out != 0),
            _ => SettingValue::Number(out),
        })
    }

    fn set(&self, id: EffectId, value: &SettingValue) -> Result<(), ApiError> {
        spi_set(ve_set_call(id, value)?)
    }
}

/// A call made to the fake.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum VeCall {
    Get(EffectId),
    Set(EffectId, SettingValue),
}

/// How the fake treats the master switch (spec 5.3, checklist item 4).
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum MasterMode {
    /// Every value is independent: the master only gates.
    #[default]
    Gates,
    /// Case 1: turning the master off also stores off for its companions.
    Clears,
    /// Case 2: while the master holds off, a companion's GET reads off; its
    /// SET still stores the value.
    ReadsOff,
}

#[cfg(test)]
#[derive(Default)]
struct FakeState {
    values: std::collections::HashMap<EffectId, SettingValue>,
    fail_get: std::collections::HashSet<EffectId>,
    fail_set: std::collections::HashSet<EffectId>,
    keep: std::collections::HashMap<EffectId, SettingValue>,
    mode: MasterMode,
    calls: Vec<VeCall>,
}

/// A map of live values with a call log; can fail a GET or a SET, keep
/// another value than the one set, and model the master switch.
#[cfg(test)]
pub(crate) struct FakeEffects {
    state: std::sync::Mutex<FakeState>,
}

#[cfg(test)]
impl FakeEffects {
    /// Seeded with the values seen on the owner's machine (spec 3).
    pub(crate) fn new() -> Self {
        use SettingValue::*;
        let values = EffectId::ALL
            .into_iter()
            .map(|id| {
                let v = match id {
                    EffectId::CursorShadow => Flag(false),
                    EffectId::FontSmoothingType => Number(2),
                    EffectId::CleartypeContrast => Number(1200),
                    EffectId::CaretWidth
                    | EffectId::FocusBorderWidth
                    | EffectId::FocusBorderHeight => Number(1),
                    _ => Flag(true),
                };
                (id, v)
            })
            .collect();
        FakeEffects {
            state: std::sync::Mutex::new(FakeState {
                values,
                ..FakeState::default()
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, FakeState> {
        self.state.lock().expect("fake effects lock")
    }

    /// The stored value, without logging a call.
    pub(crate) fn value(&self, id: EffectId) -> SettingValue {
        self.state().values[&id]
    }

    /// A change made elsewhere (not logged).
    pub(crate) fn put(&self, id: EffectId, v: SettingValue) {
        self.state().values.insert(id, v);
    }

    pub(crate) fn fail_get(&self, id: EffectId) {
        self.state().fail_get.insert(id);
    }

    pub(crate) fn fail_set(&self, id: EffectId) {
        self.state().fail_set.insert(id);
    }

    /// Later SETs of `id` succeed but leave `v` as the live value.
    pub(crate) fn keep(&self, id: EffectId, v: SettingValue) {
        self.state().keep.insert(id, v);
    }

    pub(crate) fn master_mode(&self, mode: MasterMode) {
        self.state().mode = mode;
    }

    pub(crate) fn calls(&self) -> Vec<VeCall> {
        self.state().calls.clone()
    }

    pub(crate) fn sets(&self) -> Vec<(EffectId, SettingValue)> {
        self.state()
            .calls
            .iter()
            .filter_map(|c| match c {
                VeCall::Set(id, v) => Some((*id, *v)),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
impl EffectsApi for FakeEffects {
    fn get(&self, id: EffectId) -> Result<SettingValue, ApiError> {
        let mut s = self.state();
        s.calls.push(VeCall::Get(id));
        if s.fail_get.contains(&id) {
            return Err(ApiError { code: 5 });
        }
        let master_off = s.values[&EffectId::UiEffects] == SettingValue::Flag(false);
        if s.mode == MasterMode::ReadsOff && master_off && super::MASTER_COMPANIONS.contains(&id) {
            return Ok(SettingValue::Flag(false));
        }
        Ok(s.values[&id])
    }

    fn set(&self, id: EffectId, value: &SettingValue) -> Result<(), ApiError> {
        let mut s = self.state();
        s.calls.push(VeCall::Set(id, *value));
        if s.fail_set.contains(&id) {
            return Err(ApiError { code: 5 });
        }
        ve_set_call(id, value)?;
        let live = s.keep.get(&id).copied().unwrap_or(*value);
        s.values.insert(id, live);
        if s.mode == MasterMode::Clears
            && id == EffectId::UiEffects
            && *value == SettingValue::Flag(false)
        {
            for c in super::MASTER_COMPANIONS {
                s.values.insert(c, SettingValue::Flag(false));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visual_effects::SettingValue::*;

    #[test]
    fn descriptors_follow_the_spec_table() {
        use VeFamily::*;
        assert_eq!(VE_DESCRIPTORS.map(|d| d.id), EffectId::ALL);
        let table = [
            (0x103E, PvValue, 0x103F),
            (0x0048, Animation, 0x0049),
            (0x1042, PvValue, 0x1043),
            (0x1004, PvValue, 0x1005),
            (0x1006, PvValue, 0x1007),
            (0x1002, PvValue, 0x1003),
            (0x1012, PvValue, 0x1013),
            (0x1014, PvValue, 0x1015),
            (0x1016, PvValue, 0x1017),
            (0x1018, PvValue, 0x1019),
            (0x101A, PvValue, 0x101B),
            (0x1024, PvValue, 0x1025),
            (0x004A, Ui, 0x004B),
            (0x200A, PvValue, 0x200B),
            (0x200C, PvValue, 0x200D),
            (0x2006, PvValue, 0x2007),
            (0x200E, PvValue, 0x200F),
            (0x2010, PvValue, 0x2011),
        ];
        for (d, want) in VE_DESCRIPTORS.iter().zip(table) {
            assert_eq!(
                (d.get_action, d.family, d.set_action),
                want,
                "{}",
                d.id.key()
            );
            let found = ve_descriptor(d.id);
            assert_eq!(found.id, d.id);
            assert_eq!((found.get_action, found.family, found.set_action), want);
        }
        assert!(VE_DESCRIPTORS.iter().all(|d| d.set_action != 0x0025));
    }

    #[test]
    fn set_calls_put_the_value_where_each_family_takes_it() {
        use EffectId::*;
        let ok = [
            (
                FontSmoothing,
                Flag(true),
                SetCall::Ui {
                    action: 0x4B,
                    ui: 1,
                },
            ),
            (
                CursorShadow,
                Flag(true),
                SetCall::PvValue {
                    action: 0x101B,
                    pv: 1,
                },
            ),
            (
                CleartypeContrast,
                Number(1200),
                SetCall::PvValue {
                    action: 0x200D,
                    pv: 1200,
                },
            ),
            (
                WindowAnimation,
                Flag(false),
                SetCall::Animation {
                    action: 0x49,
                    on: false,
                },
            ),
        ];
        for (id, v, call) in ok {
            assert_eq!(ve_set_call(id, &v), Ok(call), "{}", id.key());
        }
        assert_eq!(
            ve_set_call(CleartypeContrast, &Flag(true)),
            Err(ApiError { code: 87 })
        );
        assert_eq!(
            ve_set_call(CursorShadow, &Number(1)),
            Err(ApiError { code: 87 })
        );
    }

    /// Reads every live value once through the real API; never calls `set`.
    #[test]
    fn every_real_effect_get_reads_its_shape_in_the_documented_range() {
        use EffectId::*;
        for id in EffectId::ALL {
            let v = SpiEffects
                .get(id)
                .unwrap_or_else(|e| panic!("{}: {e:?}", id.key()));
            assert_eq!(v.shape(), id.shape(), "{}", id.key());
            let fits = match (id, v) {
                (FontSmoothingType, Number(n)) => n == 1 || n == 2,
                (CleartypeContrast, Number(n)) => (1000..=2200).contains(&n),
                (CaretWidth | FocusBorderWidth | FocusBorderHeight, Number(n)) => n >= 1,
                _ => true,
            };
            assert!(fits, "{} read {v:?}", id.key());
        }
    }

    #[test]
    fn the_fake_models_both_master_cases() {
        use EffectId::*;
        let off = Flag(false);

        let fake = FakeEffects::new();
        assert_eq!(fake.value(CursorShadow), off);
        assert_eq!(fake.value(CleartypeContrast), Number(1200));
        assert_eq!(fake.set(UiEffects, &off), Ok(()));
        assert_eq!(fake.value(MenuAnimation), Flag(true));
        assert_eq!(fake.get(MenuAnimation), Ok(Flag(true)));

        let fake = FakeEffects::new();
        fake.master_mode(MasterMode::Clears);
        assert_eq!(fake.set(UiEffects, &off), Ok(()));
        for id in crate::visual_effects::MASTER_COMPANIONS {
            assert_eq!(fake.value(id), off, "{}", id.key());
        }
        assert_eq!(fake.value(DropShadow), Flag(true));
        assert_eq!(fake.value(WindowAnimation), Flag(true));

        let fake = FakeEffects::new();
        fake.master_mode(MasterMode::ReadsOff);
        assert_eq!(fake.set(UiEffects, &off), Ok(()));
        assert_eq!(fake.get(MenuAnimation), Ok(off));
        assert_eq!(fake.value(MenuAnimation), Flag(true));
        assert_eq!(fake.get(DropShadow), Ok(Flag(true)));
        assert_eq!(fake.set(MenuAnimation, &Flag(true)), Ok(()));
        assert_eq!(fake.value(MenuAnimation), Flag(true));
        assert_eq!(fake.set(MenuAnimation, &off), Ok(()));
        assert_eq!(fake.value(MenuAnimation), off);
    }

    #[test]
    fn the_fake_records_calls_fails_and_keeps_other_values() {
        use EffectId::*;
        let fake = FakeEffects::new();
        assert_eq!(fake.set(CaretWidth, &Number(5)), Ok(()));
        assert_eq!(fake.get(CaretWidth), Ok(Number(5)));

        fake.keep(DropShadow, Flag(true));
        assert_eq!(fake.set(DropShadow, &Flag(false)), Ok(()));
        assert_eq!(fake.get(DropShadow), Ok(Flag(true)));

        fake.fail_get(MenuFade);
        assert_eq!(fake.get(MenuFade), Err(ApiError { code: 5 }));

        fake.fail_set(TooltipFade);
        assert_eq!(
            fake.set(TooltipFade, &Flag(false)),
            Err(ApiError { code: 5 })
        );
        assert_eq!(fake.value(TooltipFade), Flag(true));

        assert_eq!(
            fake.set(CaretWidth, &Flag(true)),
            Err(ApiError { code: 87 })
        );

        fake.put(FocusBorderWidth, Number(4));
        assert_eq!(fake.value(FocusBorderWidth), Number(4));

        assert_eq!(
            fake.calls(),
            vec![
                VeCall::Set(CaretWidth, Number(5)),
                VeCall::Get(CaretWidth),
                VeCall::Set(DropShadow, Flag(false)),
                VeCall::Get(DropShadow),
                VeCall::Get(MenuFade),
                VeCall::Set(TooltipFade, Flag(false)),
                VeCall::Set(CaretWidth, Flag(true)),
            ]
        );
        assert_eq!(
            fake.sets(),
            vec![
                (CaretWidth, Number(5)),
                (DropShadow, Flag(false)),
                (TooltipFade, Flag(false)),
                (CaretWidth, Flag(true)),
            ]
        );
    }
}
