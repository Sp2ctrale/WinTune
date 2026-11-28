use super::*;
use std::collections::BTreeSet;

#[test]
fn resx_values_are_decoded_and_comments_ignored() {
    let xml = r#"<root><data name="a" xml:space="preserve"><value>Tom &amp; Jerry &lt;3</value><comment>ignored</comment></data>
<data name="b"><value>line 1
line 2</value></data></root>"#;
    let map = parse_resx(xml).unwrap();
    assert_eq!(map["a"], "Tom & Jerry <3");
    assert_eq!(map["b"], "line 1\nline 2");
    assert_eq!(map.len(), 2);
}

#[test]
fn crlf_in_resx_values_is_normalised_to_lf() {
    let xml = "<root><data name=\"a\"><value>line 1\r\nline 2\rline 3</value></data></root>";
    assert_eq!(parse_resx(xml).unwrap()["a"], "line 1\nline 2\nline 3");
}

#[test]
fn malformed_resx_is_an_error_not_a_panic() {
    assert!(parse_resx("<root><data name=\"a\"><value>x</data>").is_err());
}

#[test]
fn a_key_missing_in_french_falls_back_to_english_and_unknown_keys_show_themselves() {
    let tr = Tr::with_catalogs(
        "fr",
        &[
            (
                "en",
                "<root><data name=\"only.en\"><value>Hello</value></data></root>",
            ),
            ("fr", "<root/>"),
        ],
    );
    assert_eq!(tr.t("only.en"), "Hello");
    assert_eq!(tr.t("no.such.key"), "no.such.key");
}

#[test]
fn placeholders_are_filled_and_never_panic() {
    let tr = Tr::with_catalogs(
        "en",
        &[(
            "en",
            r#"<root>
<data name="two"><value>{0} of {1}</value></data>
<data name="braces"><value>{{literal}} {0}</value></data>
<data name="extra"><value>{0} and {2}</value></data>
<data name="stray"><value>oops { here</value></data></root>"#,
        )],
    );
    assert_eq!(tr.t_args("two", &[&3, &13]), "3 of 13");
    assert_eq!(tr.t_args("braces", &[&"x"]), "{literal} x");
    assert_eq!(tr.t_args("extra", &[&"a"]), "a and "); // missing argument: empty
    assert_eq!(tr.t_args("stray", &[]), "oops { here"); // unmatched brace kept verbatim
}

#[test]
fn unknown_language_codes_give_english() {
    assert_eq!(Tr::new("de").code(), "en");
    assert_eq!(Tr::new("fr").code(), "fr");
}

#[test]
fn shipped_languages_are_listed_with_their_own_names() {
    let langs = Tr::languages();
    assert_eq!(langs[0], ("en", "English".to_string()));
    assert!(langs.contains(&("fr", "Français".to_string())));
    assert!(langs.iter().any(|(code, _)| *code == "en"), "no English");
}

fn placeholder_set(s: &str) -> BTreeSet<usize> {
    let re = regex::Regex::new(r"\{(\d+)\}").unwrap();
    let cleaned = s.replace("{{", "").replace("}}", "");
    re.captures_iter(&cleaned)
        .map(|c| c[1].parse().unwrap())
        .collect()
}

#[test]
fn shipped_translations_have_no_orphan_keys() {
    let en = keys_of("en").expect("English catalog");
    for (code, _) in Tr::languages() {
        for key in keys_of(code).unwrap().keys() {
            assert!(
                en.contains_key(key),
                "{code}: key {key:?} is not in English"
            );
        }
    }
}

#[test]
fn shipped_translations_are_complete() {
    let en = keys_of("en").expect("English catalog");
    for (code, _) in Tr::languages() {
        if code == "en" {
            continue;
        }
        let own = keys_of(code).unwrap_or_else(|| panic!("{code} catalog"));
        let missing: Vec<_> = en.keys().filter(|k| !own.contains_key(*k)).collect();
        assert!(missing.is_empty(), "keys missing in {code}: {missing:?}");
    }
}

#[test]
fn shipped_translations_use_the_same_placeholders_as_english() {
    let en = keys_of("en").expect("English catalog");
    for (code, _) in Tr::languages() {
        for (key, value) in keys_of(code).unwrap() {
            if let Some(reference) = en.get(key) {
                assert_eq!(
                    placeholder_set(value),
                    placeholder_set(reference),
                    "{code}: placeholders of {key:?} differ from English"
                );
            }
        }
    }
}

#[test]
fn every_engine_message_translates_in_every_shipped_language() {
    use super::messages::{custom_reason, drift_reason, rule_problem};
    use crate::associations::{DriftField, DriftReason, RuleProblem};
    use crate::tweaks::CustomReason;

    let s = |x: &str| x.to_string();
    // (message builder, sample arguments it must contain)
    type Case = (Box<dyn Fn(Tr) -> String>, Vec<&'static str>, String);
    let mut cases: Vec<Case> = Vec::new();
    let mut problem = |p: RuleProblem, args: Vec<&'static str>| {
        let shown = p.to_string();
        cases.push((Box::new(move |tr| rule_problem(tr, &p)), args, shown));
    };
    problem(
        RuleProblem::InvalidExtension { ext: s("EXT1") },
        vec!["EXT1"],
    );
    problem(
        RuleProblem::ReservedExtension { ext: s("EXT2") },
        vec!["EXT2"],
    );
    problem(
        RuleProblem::InvalidProgId { prog_id: s("PID1") },
        vec!["PID1"],
    );
    problem(
        RuleProblem::ReservedProgId {
            prog_id: s("PID2"),
            suggestion: s("SUG2"),
        },
        vec!["PID2", "SUG2"],
    );
    problem(
        RuleProblem::DuplicateExtension { ext: s("EXT3") },
        vec!["EXT3"],
    );
    problem(
        RuleProblem::SharedProgId { prog_id: s("PID3") },
        vec!["PID3"],
    );
    let mut drift = |r: DriftReason, args: Vec<&'static str>| {
        let shown = r.to_string();
        cases.push((Box::new(move |tr| drift_reason(tr, &r)), args, shown));
    };
    drift(
        DriftReason::DefaultChanged {
            ext: s("EXT4"),
            found: s("FOUND4"),
            expected: s("EXP4"),
        },
        vec!["EXT4", "FOUND4", "EXP4"],
    );
    drift(
        DriftReason::DefaultMissing {
            ext: s("EXT5"),
            expected: s("EXP5"),
        },
        vec!["EXT5", "EXP5"],
    );
    drift(
        DriftReason::ProgIdMissing { prog_id: s("PID5") },
        vec!["PID5"],
    );
    for field in [
        DriftField::Description,
        DriftField::Icon,
        DriftField::OpenCommand,
    ] {
        drift(
            DriftReason::ValueDiffers {
                field,
                expected: s("EXP6"),
                found: s("FOUND6"),
            },
            vec!["EXP6", "FOUND6"],
        );
    }
    drift(
        DriftReason::UserChoice {
            choice: s("CHOICE7"),
            user: s("USER7"),
            ext: s("EXT7"),
            prog_id: s("PID7"),
        },
        vec!["CHOICE7", "USER7", "EXT7", "PID7"],
    );
    drift(
        DriftReason::UserOverride {
            user: s("USER8"),
            ext: s("EXT8"),
            prog_id: s("PID8"),
        },
        vec!["USER8", "EXT8", "PID8"],
    );
    let mut custom = |c: CustomReason, args: Vec<&'static str>| {
        // The one exception: the engine's "unreadable: <os error>" is shown
        // as a translated summary followed by the raw detail (UI redesign PR
        // 4 ruling; the engine stays untouched).
        let shown = match &c {
            CustomReason::Unreadable(e) => format!("Can't read this setting: {e}"),
            _ => c.to_string(),
        };
        cases.push((Box::new(move |tr| custom_reason(tr, &c)), args, shown));
    };
    custom(
        CustomReason::UnexpectedValue {
            name: s("NAME9"),
            value: 4242,
        },
        vec!["NAME9", "4242"],
    );
    custom(
        CustomReason::UnexpectedType { name: s("NAME10") },
        vec!["NAME10"],
    );
    custom(CustomReason::Unreadable(s("WHY11")), vec!["WHY11"]);
    // Load errors: `Display` keeps the historical English sentences (logs,
    // reports), the UI has its own wording (UI redesign PR 6 ruling). Both
    // carry the path and the raw detail.
    let mut load = |e: crate::persist::LoadError, args: Vec<&'static str>, shown: &str| {
        for a in &args {
            assert!(e.to_string().contains(a), "Display lacks {a:?}");
        }
        let shown = shown.to_string();
        cases.push((
            Box::new(move |tr| super::messages::load_error(tr, &e)),
            args,
            shown,
        ));
    };
    load(
        crate::persist::LoadError::Corrupt {
            path: "PATH12".into(),
            detail: s("WHY12"),
        },
        vec!["PATH12", "WHY12"],
        "PATH12 can't be read (WHY12). It was left untouched: fix or move it before applying changes.",
    );
    load(
        crate::persist::LoadError::Unreadable {
            path: "PATH13".into(),
            detail: s("WHY13"),
        },
        vec!["PATH13", "WHY13"],
        "Can't read PATH13: WHY13",
    );

    for (code, _) in Tr::languages() {
        let tr = Tr::new(code);
        for (build, args, shown) in &cases {
            let text = build(tr);
            assert!(!text.is_empty(), "{code}: empty message");
            if code == "en" {
                assert_eq!(
                    &text, shown,
                    "English .resx must equal the expected English"
                );
            }
            assert!(
                !text.contains('{'),
                "{code}: unfilled placeholder in {text:?}"
            );
            assert!(
                !text.contains('.') || text.contains(' '),
                "{code}: looks like a bare key: {text:?}"
            );
            for a in args {
                assert!(text.contains(a), "{code}: {text:?} lacks {a:?}");
            }
        }
    }
}

fn collect_rs(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "i18n") {
                continue;
            }
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn every_context_menu_message_translates_in_every_shipped_language() {
    use super::messages::{
        entry_problem, entry_status, existing_state, history_module, locked, place_name,
        rename_problem,
    };
    use crate::context_menu::{
        EntryProblem, EntryStatus, ExistingState, Locked, Place, RenameProblem,
    };

    let problems = [
        EntryProblem::EmptyLabel,
        EntryProblem::LabelTooLong,
        EntryProblem::NoPlace,
        EntryProblem::ProgramNotFound,
        EntryProblem::IconNotFound,
        EntryProblem::DuplicateLabel(Place::Background),
        EntryProblem::ShellOnFiles,
    ];
    let statuses = [
        EntryStatus::Applied,
        EntryStatus::NotApplied,
        EntryStatus::Changed(vec![Place::Drive]),
    ];
    for (code, _) in Tr::languages() {
        let tr = Tr::new(code);
        let mut texts: Vec<String> = problems.iter().map(|p| entry_problem(tr, p)).collect();
        texts.extend(statuses.iter().map(|s| entry_status(tr, s)));
        texts.extend(Place::ALL.iter().map(|p| place_name(tr, *p)));
        texts.push(history_module(tr, "Context menu"));
        texts.extend(
            [
                ExistingState::Shown,
                ExistingState::HiddenByWinTune,
                ExistingState::HiddenElsewhere,
            ]
            .map(|s| existing_state(tr, s)),
        );
        texts.push(locked(tr, &Locked::MachineWide));
        let unreadable = locked(tr, &Locked::Unreadable("WHY14".into()));
        assert!(unreadable.contains("WHY14"), "{code}: {unreadable:?}");
        texts.push(unreadable);
        // A rename problem reads like the same problem of an added entry.
        for (rename, entry) in [
            (RenameProblem::Empty, EntryProblem::EmptyLabel),
            (RenameProblem::TooLong, EntryProblem::LabelTooLong),
            (RenameProblem::LeadingAt, EntryProblem::LeadingAt),
        ] {
            assert_eq!(rename_problem(tr, &rename), entry_problem(tr, &entry));
        }
        let unique: BTreeSet<&String> = texts.iter().collect();
        for text in &texts {
            assert!(!text.is_empty() && !text.contains('{'), "{code}: {text:?}");
            assert!(
                !text.contains('.') || text.contains(' '),
                "{code}: looks like a bare key: {text:?}"
            );
        }
        assert!(unique.len() >= texts.len() - 1, "{code}: {texts:?}");
        let duplicate = entry_problem(tr, &EntryProblem::DuplicateLabel(Place::Background));
        assert!(duplicate.contains(&place_name(tr, Place::Background)));
    }
}

#[test]
fn every_startup_message_translates_in_every_shipped_language() {
    use super::messages::{history_module, startup_lock, startup_source, startup_state};
    use crate::startup::{Source, StartupLock, StartupState};

    let en = Tr::english();
    for (code, _) in Tr::languages() {
        let tr = Tr::new(code);
        let mut texts: Vec<String> = [
            StartupState::On,
            StartupState::Off,
            StartupState::OffElsewhere,
        ]
        .map(|s| startup_state(tr, s))
        .into();
        for lock in [
            StartupLock::UnknownShape,
            StartupLock::OffForEveryone,
            StartupLock::AllUsersOnly,
        ] {
            texts.push(startup_lock(tr, &lock));
        }
        let unreadable = startup_lock(tr, &StartupLock::Unreadable("X".into()));
        assert!(unreadable.contains('X'), "{code}: {unreadable:?}");
        texts.push(unreadable);
        texts.extend(Source::ALL.map(|s| startup_source(tr, s)));
        // An unmapped label passes through unchanged, so compare with the key.
        let module = history_module(tr, "Startup apps");
        assert_eq!(module, tr.t("history.module.startup"), "{code}");
        assert_ne!(module, "history.module.startup", "{code}");
        texts.push(module);

        let unique: BTreeSet<&String> = texts.iter().collect();
        assert_eq!(unique.len(), texts.len(), "{code}: duplicates in {texts:?}");
        for text in &texts {
            assert!(!text.is_empty() && !text.contains('{'), "{code}: {text:?}");
            assert!(
                !text.starts_with("startup.") && !text.starts_with("history."),
                "{code}: looks like a bare key: {text:?}"
            );
        }
    }
    assert_eq!(
        startup_state(en, StartupState::Off),
        "Turned off by WinTune"
    );
    assert_eq!(
        history_module(Tr::new("fr"), "Startup apps"),
        "Applications au d\u{e9}marrage"
    );
}

#[test]
fn every_environment_message_translates_in_every_shipped_language() {
    use super::messages::{env_problem, env_warning, history_module};
    use crate::environment::{EnvProblem, EnvWarning};

    let named = |make: fn(String) -> EnvProblem| make("X".to_string());
    for (code, _) in Tr::languages() {
        let tr = Tr::new(code);
        let mut texts: Vec<String> = [
            EnvProblem::EmptyName,
            EnvProblem::NameHasEquals,
            EnvProblem::NameHasNul,
            EnvProblem::NameTooLong,
            EnvProblem::PathName,
            EnvProblem::EmptyValue,
            EnvProblem::ValueTooLong,
            EnvProblem::ValueHasNul,
            EnvProblem::EmptyEntry,
            EnvProblem::EntryHasSemicolon,
        ]
        .iter()
        .map(|p| env_problem(tr, p))
        .collect();
        for problem in [
            named(EnvProblem::NameTaken),
            named(EnvProblem::Gone),
            named(EnvProblem::NotText),
            named(EnvProblem::Twice),
            named(EnvProblem::ChangedSinceScan),
        ] {
            let text = env_problem(tr, &problem);
            assert!(text.contains('X'), "{code}: {text:?}");
            texts.push(text);
        }
        let removes = env_warning(tr, &EnvWarning::Removes("X".into()));
        assert!(removes.contains('X'), "{code}: {removes:?}");
        let long = env_warning(tr, &EnvWarning::PathTooLong(9000));
        assert!(long.contains("9000"), "{code}: {long:?}");
        texts.extend([
            removes,
            long,
            env_warning(tr, &EnvWarning::PathChangedElsewhere),
        ]);
        // An unmapped label passes through unchanged, so compare with the key.
        let module = history_module(tr, "Environment variables");
        assert_eq!(module, tr.t("history.module.environment"), "{code}");
        assert_ne!(module, "history.module.environment", "{code}");
        texts.push(module);

        let unique: BTreeSet<&String> = texts.iter().collect();
        assert_eq!(unique.len(), texts.len(), "{code}: duplicates in {texts:?}");
        for text in &texts {
            assert!(!text.is_empty() && !text.contains('{'), "{code}: {text:?}");
            assert!(
                !text.starts_with("env.") && !text.starts_with("history."),
                "{code}: looks like a bare key: {text:?}"
            );
        }
    }
    assert_eq!(
        history_module(Tr::new("fr"), "Environment variables"),
        "Variables d'environnement"
    );
}

#[test]
fn every_mouse_keyboard_message_translates_in_every_shipped_language() {
    use super::messages::{
        history_module, mk_description, mk_failure, mk_issue, mk_problem, mk_section, mk_title,
        mk_value,
    };
    use crate::mouse_keyboard::SettingId::*;
    use crate::mouse_keyboard::SettingValue::*;
    use crate::mouse_keyboard::{
        MkFailure, MkIssue, MkProblem, Section, SettingId, WHEEL_PAGESCROLL,
    };

    for (code, _) in Tr::languages() {
        let tr = Tr::new(code);
        let titles: Vec<String> = SettingId::ALL.map(|id| mk_title(tr, id)).into();
        let unique: BTreeSet<&String> = titles.iter().collect();
        assert_eq!(unique.len(), 18, "{code}: {titles:?}");
        let speed = mk_title(tr, PointerSpeed);
        let mut texts = titles.clone();
        texts.extend(Section::ALL.map(|s| mk_section(tr, s)));
        let descriptions: Vec<String> = SettingId::ALL
            .iter()
            .filter_map(|&id| mk_description(tr, id))
            .collect();
        assert_eq!(descriptions.len(), 7, "{code}: {descriptions:?}");
        texts.extend(descriptions);

        // Each issue text, with the arguments it must show.
        let problems: [(MkProblem, Vec<&str>); 8] = [
            (MkProblem::OutOfRange(PointerSpeed), vec![&speed]),
            (MkProblem::UnknownSetting("X".into()), vec!["X"]),
            (
                MkProblem::Unreadable {
                    id: PointerSpeed,
                    code: 5,
                },
                vec![&speed, "5"],
            ),
            (
                MkProblem::WrongAccount {
                    process: "X".into(),
                    session: "Y".into(),
                },
                vec!["X", "Y"],
            ),
            (MkProblem::AccountUnknown("X".into()), vec!["X"]),
            (MkProblem::NotSaved("X".into()), vec!["X"]),
            (MkProblem::NoOriginal("X".into()), vec!["X"]),
            (MkProblem::NoOriginal("pointer_speed".into()), vec![&speed]),
        ];
        for (problem, args) in problems {
            let text = mk_problem(tr, &problem);
            for arg in args {
                assert!(text.contains(arg), "{code}: {arg:?} not in {text:?}");
            }
            assert_eq!(mk_issue(tr, &MkIssue::Problem(problem)), text);
            texts.push(text);
        }
        let failures: [(MkFailure, Vec<&str>); 4] = [
            (
                MkFailure::SetFailed {
                    id: PointerSpeed,
                    code: 5,
                },
                vec![&speed, "5"],
            ),
            (
                MkFailure::Mismatch {
                    id: PointerSpeed,
                    wanted: Number(14),
                    read: Number(10),
                },
                vec![&speed, "14", "10"],
            ),
            (
                MkFailure::Unverified {
                    id: PointerSpeed,
                    code: 5,
                },
                vec![&speed, "5"],
            ),
            (MkFailure::StoreNotSaved("X".into()), vec!["X"]),
        ];
        for (failure, args) in failures {
            let text = mk_failure(tr, &failure);
            for arg in args {
                assert!(text.contains(arg), "{code}: {arg:?} not in {text:?}");
            }
            assert_eq!(mk_issue(tr, &MkIssue::Failure(failure)), text);
            texts.push(text);
        }

        // Values, as on the page.
        assert_eq!(
            mk_value(tr, Some(WheelLines), &Number(WHEEL_PAGESCROLL)),
            tr.t("mk.value.one_screen")
        );
        assert_eq!(
            mk_value(tr, Some(WheelLines), &Number(0)),
            tr.t("mk.value.no_scroll")
        );
        let ms = mk_value(tr, Some(DoubleClickTime), &Number(500));
        assert!(ms.contains("500"), "{code}: {ms:?}");
        assert_ne!(ms, "500", "{code}");
        assert_eq!(
            mk_value(tr, Some(PointerPrecision), &Mouse([6, 10, 1])),
            "{6, 10, 1}"
        );
        let (on, off) = (tr.t("mk.value.on"), tr.t("mk.value.off"));
        assert_eq!(mk_value(tr, None, &Flag(true)), on);
        assert_eq!(mk_value(tr, Some(SwapButtons), &Flag(false)), off);
        assert_eq!(mk_value(tr, Some(PointerTrails), &Number(1)), off);
        assert_eq!(mk_value(tr, Some(PointerTrails), &Number(5)), "5");
        assert_eq!(mk_value(tr, Some(ScrollInactive), &Number(0)), off);
        assert_eq!(mk_value(tr, Some(ScrollInactive), &Number(2)), on);
        assert_eq!(mk_value(tr, Some(PointerSpeed), &Number(10)), "10");
        assert_eq!(mk_value(tr, None, &Number(500)), "500");
        texts.extend([
            on,
            off,
            ms,
            mk_value(tr, Some(WheelLines), &Number(WHEEL_PAGESCROLL)),
            mk_value(tr, Some(WheelLines), &Number(0)),
        ]);

        // An unmapped label passes through unchanged, so compare with the key.
        let module = history_module(tr, "Mouse & keyboard");
        assert_eq!(module, tr.t("history.module.mouse_keyboard"), "{code}");
        assert_ne!(module, "history.module.mouse_keyboard", "{code}");
        texts.push(module);

        for text in &texts {
            assert!(!text.is_empty() && !text.contains('{'), "{code}: {text:?}");
            assert!(
                !text.starts_with("mk.") && !text.starts_with("history."),
                "{code}: looks like a bare key: {text:?}"
            );
        }
    }
    let en = Tr::english();
    assert_eq!(
        mk_failure(
            en,
            &MkFailure::Mismatch {
                id: DoubleClickTime,
                wanted: Number(700),
                read: Number(500),
            }
        ),
        "Double-click speed: Windows kept 500 ms instead of 700 ms."
    );
    assert_eq!(
        history_module(Tr::new("fr"), "Mouse & keyboard"),
        "Souris et clavier"
    );
}

#[test]
fn every_visual_effects_message_translates_in_every_shipped_language() {
    use super::messages::{
        history_module, ve_companions, ve_description, ve_failure, ve_issue, ve_not_written,
        ve_problem, ve_section, ve_title, ve_value,
    };
    use crate::visual_effects::EffectId::*;
    use crate::visual_effects::SettingValue::*;
    use crate::visual_effects::{EffectId, VeFailure, VeIssue, VeProblem, VeSection};

    for (code, _) in Tr::languages() {
        let tr = Tr::new(code);
        let titles: Vec<String> = EffectId::ALL.map(|id| ve_title(tr, id)).into();
        let unique: BTreeSet<&String> = titles.iter().collect();
        assert_eq!(unique.len(), 18, "{code}: {titles:?}");
        let sections: Vec<String> = VeSection::ALL.map(|s| ve_section(tr, s)).into();
        let unique: BTreeSet<&String> = sections.iter().collect();
        assert_eq!(unique.len(), 6, "{code}: {sections:?}");
        let descriptions: Vec<String> = EffectId::ALL
            .iter()
            .filter_map(|&id| ve_description(tr, id))
            .collect();
        assert_eq!(descriptions.len(), 1, "{code}: {descriptions:?}");
        let caret = ve_title(tr, CaretWidth);
        let mut texts = titles.clone();
        texts.extend(sections);
        texts.extend(descriptions);

        // Each issue text, with the arguments it must show.
        let problems: [(VeProblem, Vec<&str>); 8] = [
            (VeProblem::OutOfRange(CaretWidth), vec![&caret]),
            (VeProblem::UnknownSetting("X".into()), vec!["X"]),
            (
                VeProblem::Unreadable {
                    id: CaretWidth,
                    code: 5,
                },
                vec![&caret, "5"],
            ),
            (
                VeProblem::WrongAccount {
                    process: "X".into(),
                    session: "Y".into(),
                },
                vec!["X", "Y"],
            ),
            (VeProblem::AccountUnknown("X".into()), vec!["X"]),
            (VeProblem::NotSaved("X".into()), vec!["X"]),
            (VeProblem::NoOriginal("X".into()), vec!["X"]),
            (VeProblem::NoOriginal("caret_width".into()), vec![&caret]),
        ];
        for (problem, args) in problems {
            let text = ve_problem(tr, &problem);
            for arg in args {
                assert!(text.contains(arg), "{code}: {arg:?} not in {text:?}");
            }
            assert_eq!(ve_issue(tr, &VeIssue::Problem(problem)), text);
            texts.push(text);
        }
        let shadow = ve_title(tr, CursorShadow);
        let px = |n: u32| ve_value(tr, Some(CaretWidth), &Number(n));
        let (two_px, one_px) = (px(2), px(1));
        let failures: [(VeFailure, Vec<&str>); 7] = [
            (
                VeFailure::SetFailed {
                    id: CaretWidth,
                    code: 5,
                },
                vec![&caret, "5"],
            ),
            (
                VeFailure::Mismatch {
                    id: CaretWidth,
                    wanted: Number(2),
                    read: Number(1),
                },
                vec![&caret, &two_px, &one_px],
            ),
            (
                VeFailure::Unverified {
                    id: CaretWidth,
                    code: 5,
                },
                vec![&caret, "5"],
            ),
            (
                VeFailure::HeldByMaster {
                    id: CursorShadow,
                    wanted: Flag(true),
                    read: Flag(false),
                },
                vec![&shadow],
            ),
            (VeFailure::StoreNotSaved("X".into()), vec!["X"]),
            (
                VeFailure::RereadFailed {
                    id: CursorShadow,
                    code: 5,
                },
                vec![&shadow, "5"],
            ),
            (VeFailure::GatedNotSaved("X".into()), vec!["X"]),
        ];
        for (failure, args) in failures {
            let text = ve_failure(tr, &failure);
            for arg in args {
                assert!(text.contains(arg), "{code}: {arg:?} not in {text:?}");
            }
            assert_eq!(ve_issue(tr, &VeIssue::Failure(failure.clone())), text);
            // The two "not written" failures read as `ve_not_written` says.
            if let Some(not_written) = ve_not_written(tr, &failure, &shadow) {
                assert_eq!(text, not_written, "{code}");
            }
            texts.push(text);
        }

        let companions = ve_companions(tr, 2, &["A".into(), "B".into()]);
        assert!(companions.contains("A, B") && companions.contains('2'));
        let one = ve_companions(tr, 1, &["A".into()]);
        assert!(one.contains('A') && one.contains('1'));
        assert_ne!(one, ve_companions(tr, 2, &["A".into()]), "{code}: singular");
        texts.extend([companions, one]);

        // An unmapped label passes through unchanged, so compare with the key.
        let module = history_module(tr, "Visual effects");
        assert_eq!(module, tr.t("history.module.visual_effects"), "{code}");
        assert_ne!(module, "history.module.visual_effects", "{code}");
        texts.push(module);

        for text in &texts {
            assert!(!text.is_empty() && !text.contains('{'), "{code}: {text:?}");
            assert!(
                !text.starts_with("ve.") && !text.starts_with("history."),
                "{code}: looks like a bare key: {text:?}"
            );
        }
    }

    let en = Tr::english();
    let value = |id: EffectId, v| ve_value(en, Some(id), &v);
    assert_eq!(value(MenuFade, Flag(false)), "Slide");
    assert_eq!(value(TooltipFade, Flag(true)), "Fade");
    assert_eq!(value(CursorShadow, Flag(true)), "On");
    assert_eq!(value(CursorShadow, Flag(false)), "Off");
    assert_eq!(value(FontSmoothingType, Number(1)), "Standard");
    assert_eq!(value(FontSmoothingType, Number(2)), "ClearType");
    assert_eq!(value(FontSmoothingType, Number(3)), "Unknown (3)");
    assert_eq!(value(CleartypeContrast, Number(1200)), "1,200");
    assert_eq!(value(CleartypeContrast, Number(999)), "999");
    assert_eq!(value(CleartypeContrast, Number(1_234_567)), "1,234,567");
    assert_eq!(value(CaretWidth, Number(2)), "2 px");
    assert_eq!(value(FocusBorderHeight, Number(4)), "4 px");
    assert_eq!(ve_value(en, None, &Number(1200)), "1200");
    assert_eq!(ve_value(en, None, &Flag(true)), "On");
    let fr = Tr::new("fr");
    assert_eq!(
        ve_value(fr, Some(CleartypeContrast), &Number(1200)),
        "1\u{202F}200"
    );
    assert_eq!(ve_value(fr, Some(MenuFade), &Flag(true)), "Fondu");
    assert_eq!(history_module(fr, "Visual effects"), "Effets visuels");
    assert_eq!(
        ve_failure(
            en,
            &VeFailure::Mismatch {
                id: CleartypeContrast,
                wanted: Number(1600),
                read: Number(1200),
            }
        ),
        "ClearType contrast: Windows kept 1,200 instead of 1,600."
    );
}

#[test]
fn the_visual_effects_not_written_messages_translate_in_every_shipped_language() {
    use super::messages::ve_not_written;
    use crate::visual_effects::{EffectId, VeFailure};

    let reread = VeFailure::RereadFailed {
        id: EffectId::MenuFade,
        code: 5,
    };
    let not_saved = VeFailure::GatedNotSaved("disk full".into());
    for (code, _) in Tr::languages() {
        let tr = Tr::new(code);
        let texts = [
            ve_not_written(tr, &reread, "Menu fade").unwrap(),
            ve_not_written(tr, &not_saved, "").unwrap(),
        ];
        assert!(
            texts[0].contains("Menu fade") && texts[0].contains('5'),
            "{code}"
        );
        assert!(texts[1].contains("disk full"), "{code}");
        for text in &texts {
            assert!(!text.contains('{'), "{code}: {text:?}");
            assert!(!text.starts_with("ve."), "{code}: bare key {text:?}");
        }
    }
    assert_eq!(
        ve_not_written(Tr::new("fr"), &not_saved, ""),
        Some(
            "Les effets de l'interface ont été modifiés, mais les effets qui en dépendent \
             n'ont pas été écrits : les valeurs d'origine n'ont pas pu être mises à jour \
             (disk full)."
                .to_string()
        )
    );
    let other = VeFailure::StoreNotSaved("x".into());
    assert_eq!(ve_not_written(Tr::english(), &other, ""), None);
}

#[test]
fn every_key_used_in_the_code_exists_in_english() {
    let en = keys_of("en").expect("English catalog");
    let re = regex::Regex::new(r#"\.t(?:_args)?\(\s*"([A-Za-z0-9_.]+)""#).unwrap();
    let mut files = Vec::new();
    collect_rs(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let mut missing = BTreeSet::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for cap in re.captures_iter(&text) {
            if !en.contains_key(&cap[1]) {
                missing.insert(format!("{} ({})", &cap[1], file.display()));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "keys used but missing in English: {missing:#?}"
    );
}

#[test]
fn every_tweak_and_group_has_texts_in_every_shipped_language() {
    for (code, _) in Tr::languages() {
        let tr = Tr::new(code);
        for t in crate::tweaks::catalog() {
            assert_ne!(
                messages::tweak_title(tr, t.id),
                format!("tweak.{}.title", t.id),
                "{code}"
            );
            assert_ne!(
                messages::tweak_description(tr, t.id),
                format!("tweak.{}.description", t.id),
                "{code}"
            );
        }
        for g in crate::tweaks::TweakGroup::ALL {
            assert_ne!(
                messages::group_title(tr, g),
                format!("group.{}", g.id()),
                "{code}"
            );
        }
    }
}

#[test]
fn english_tweak_texts_are_unchanged() {
    assert_eq!(
        messages::tweak_title(Tr::english(), "show_extensions"),
        "Show file extensions"
    );
}

#[test]
fn known_history_labels_are_translated_and_unknown_ones_kept() {
    use super::messages::{history_action, history_module};
    let fr = Tr::new("fr");
    assert_eq!(
        history_module(fr, "File associations"),
        fr.t("history.module.associations")
    );
    assert_eq!(
        history_action(fr, "Rollback"),
        fr.t("history.action.rollback")
    );
    assert_eq!(history_module(fr, "Legacy thing"), "Legacy thing");
}

#[test]
fn menu_pending_resolves_to_its_texts_in_both_languages() {
    assert_eq!(Tr::new("en").t_args("menu.pending", &[&3]), "3 not applied");
    let fr = Tr::new("fr").t_args("menu.pending", &[&3]);
    assert_ne!(fr, "menu.pending");
    assert_ne!(fr, "3 not applied");
    assert!(fr.contains('3'), "{fr}");
}

#[test]
fn every_data_element_in_the_shipped_resx_is_well_formed() {
    for file in ["strings.resx", "strings.fr.resx"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("i18n")
            .join(file);
        let text = std::fs::read_to_string(path).unwrap();
        for line in text
            .lines()
            .filter(|l| l.trim_start().starts_with("<data "))
        {
            assert!(
                line.contains("\" xml:space=\"preserve\">"),
                "{file}: ill-formed element {line:?}"
            );
        }
    }
}
