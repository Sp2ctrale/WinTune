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
                assert_eq!(&text, shown, "English .resx must equal Display");
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
