//! Translations. English (`i18n/strings.resx`) is the reference and the
//! fallback; a key missing even there renders as the key itself.
//!
//! Catalogs are embedded at build time (see `build.rs`) and parsed once.

use std::collections::BTreeMap;
use std::fmt::Display;
use std::sync::OnceLock;

pub mod resx;

pub use resx::parse_resx;

include!(concat!(env!("OUT_DIR"), "/i18n_langs.rs"));

type Catalog = BTreeMap<String, String>;

/// All shipped catalogs, parsed once: English first, then by code.
fn catalogs() -> &'static [(&'static str, Catalog)] {
    static CATALOGS: OnceLock<Vec<(&'static str, Catalog)>> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        LANG_FILES
            .iter()
            .filter_map(|(code, xml)| match parse_resx(xml) {
                Ok(map) => Some((*code, map)),
                Err(e) => {
                    debug_assert!(false, "i18n/{code}: invalid resx: {e}");
                    None
                }
            })
            .collect()
    })
}

/// The keys of a shipped language, if it exists.
pub(crate) fn keys_of(code: &str) -> Option<&'static Catalog> {
    catalogs().iter().find(|(c, _)| *c == code).map(|(_, m)| m)
}

fn empty_catalog() -> &'static Catalog {
    static EMPTY: OnceLock<Catalog> = OnceLock::new();
    EMPTY.get_or_init(Catalog::new)
}

/// A cheap, copyable handle on one language (with the English fallback).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tr {
    code: &'static str,
    lang: &'static Catalog,
    en: &'static Catalog,
}

impl Tr {
    /// Translator for `code`; an unknown code gives English.
    pub fn new(code: &str) -> Tr {
        let en = keys_of("en").unwrap_or_else(empty_catalog);
        match catalogs().iter().find(|(c, _)| *c == code) {
            Some((c, map)) => Tr {
                code: c,
                lang: map,
                en,
            },
            None => Tr::english(),
        }
    }

    pub fn english() -> Tr {
        let en = keys_of("en").unwrap_or_else(empty_catalog);
        Tr {
            code: "en",
            lang: en,
            en,
        }
    }

    /// Test-only translator built from in-memory catalogs (leaked so that
    /// `Tr` stays `Copy`). `en` is the fallback, if present.
    #[cfg(test)]
    pub(crate) fn with_catalogs(code: &str, catalogs: &[(&str, &str)]) -> Tr {
        let parse = |c: &str| -> &'static Catalog {
            let xml = catalogs.iter().find(|(k, _)| *k == c).map(|(_, x)| *x);
            Box::leak(Box::new(
                xml.map(|x| parse_resx(x).expect("test catalog must parse"))
                    .unwrap_or_default(),
            ))
        };
        Tr {
            code: Box::leak(code.to_string().into_boxed_str()),
            lang: parse(code),
            en: parse("en"),
        }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    /// The text for `key`: this language, then English, then the key itself.
    pub fn t(&self, key: &str) -> String {
        self.lang
            .get(key)
            .or_else(|| self.en.get(key))
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    /// Like [`Tr::t`], filling .NET-style `{0}`, `{1}` placeholders. `{{` and
    /// `}}` are literal braces; a missing argument renders empty; any other
    /// brace is kept verbatim.
    pub fn t_args(&self, key: &str, args: &[&dyn Display]) -> String {
        let text = self.t(key);
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '{' if chars.get(i + 1) == Some(&'{') => {
                    out.push('{');
                    i += 2;
                }
                '}' if chars.get(i + 1) == Some(&'}') => {
                    out.push('}');
                    i += 2;
                }
                '{' => {
                    let mut j = i + 1;
                    while j < chars.len() && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                    if j > i + 1 && chars.get(j) == Some(&'}') {
                        let digits: String = chars[i + 1..j].iter().collect();
                        if let Some(arg) = digits.parse::<usize>().ok().and_then(|n| args.get(n)) {
                            out.push_str(&arg.to_string());
                        }
                        i = j + 1;
                    } else {
                        out.push('{');
                        i += 1;
                    }
                }
                c => {
                    out.push(c);
                    i += 1;
                }
            }
        }
        out
    }

    /// Shipped languages as (code, the language's own name): English first,
    /// then by code.
    pub fn languages() -> Vec<(&'static str, String)> {
        let mut v: Vec<(&'static str, String)> = catalogs()
            .iter()
            .map(|(code, map)| {
                let name = map
                    .get("language.name")
                    .cloned()
                    .unwrap_or_else(|| code.to_string());
                (*code, name)
            })
            .collect();
        v.sort_by(|a, b| (a.0 != "en", a.0).cmp(&(b.0 != "en", b.0)));
        v
    }
}

#[cfg(test)]
mod tests;
