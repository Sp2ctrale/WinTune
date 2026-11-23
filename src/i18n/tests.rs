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
    let fr = keys_of("fr").expect("French catalog");
    let missing: Vec<_> = en.keys().filter(|k| !fr.contains_key(*k)).collect();
    assert!(missing.is_empty(), "keys missing in fr: {missing:?}");
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
