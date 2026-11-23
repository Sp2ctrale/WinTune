//! Minimal `.resx` reader: `<data name="k"><value>v</value></data>` pairs.

use std::collections::BTreeMap;

use quick_xml::events::Event;
use quick_xml::Reader;

/// Parses a ResX document into a key -> value map. `<comment>`, `<resheader>`
/// and `<schema>` are ignored. Any XML error (including unclosed tags) is an
/// `Err`.
pub fn parse_resx(xml: &str) -> Result<BTreeMap<String, String>, String> {
    let mut reader = Reader::from_str(xml);
    let mut map = BTreeMap::new();
    let mut name: Option<String> = None;
    let mut in_value = false;
    let mut value = String::new();
    // Element nesting inside <data>/<value> that we do not care about.
    let mut depth_ignored = 0usize;

    loop {
        let event = reader
            .read_event()
            .map_err(|e| format!("resx parse error at {}: {e}", reader.buffer_position()))?;
        match event {
            Event::Start(e) => {
                let tag = e.name();
                if depth_ignored > 0 {
                    depth_ignored += 1;
                } else if in_value {
                    // Unexpected markup inside <value>: skip it.
                    depth_ignored += 1;
                } else if tag.as_ref() == b"data" {
                    name = None;
                    for attr in e.attributes() {
                        let attr = attr.map_err(|e| format!("bad attribute: {e}"))?;
                        if attr.key.as_ref() == b"name" {
                            let v = attr
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .map_err(|e| format!("bad attribute value: {e}"))?;
                            name = Some(v.into_owned());
                        }
                    }
                } else if tag.as_ref() == b"value" && name.is_some() {
                    in_value = true;
                    value.clear();
                } else if tag.as_ref() == b"comment"
                    || tag.as_ref() == b"resheader"
                    || tag.as_ref() == b"schema"
                {
                    depth_ignored = 1;
                }
            }
            Event::End(e) => {
                if depth_ignored > 0 {
                    depth_ignored -= 1;
                } else if e.name().as_ref() == b"value" && in_value {
                    in_value = false;
                    if let Some(n) = name.as_ref() {
                        map.insert(n.clone(), std::mem::take(&mut value));
                    }
                } else if e.name().as_ref() == b"data" {
                    name = None;
                }
            }
            Event::Text(t) if in_value && depth_ignored == 0 => {
                let s = t.decode().map_err(|e| format!("bad text: {e}"))?;
                value.push_str(&s);
            }
            Event::GeneralRef(r) if in_value && depth_ignored == 0 => {
                if let Some(c) = r
                    .resolve_char_ref()
                    .map_err(|e| format!("bad character reference: {e}"))?
                {
                    value.push(c);
                } else {
                    let n = r.decode().map_err(|e| format!("bad reference: {e}"))?;
                    match n.as_ref() {
                        "amp" => value.push('&'),
                        "lt" => value.push('<'),
                        "gt" => value.push('>'),
                        "quot" => value.push('"'),
                        "apos" => value.push('\''),
                        other => return Err(format!("unknown entity &{other};")),
                    }
                }
            }
            Event::CData(c) if in_value && depth_ignored == 0 => {
                let s = c.decode().map_err(|e| format!("bad CDATA: {e}"))?;
                value.push_str(&s);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if in_value || name.is_some() || depth_ignored > 0 {
        return Err("resx parse error: unclosed element".to_string());
    }
    Ok(map)
}
