//! Content control kind elements

use super::super::binding::{tag_attrs, tag_end, unescape};
use super::super::model::{ControlAppearance, ControlBinding, ControlKind, ControlProps, DateStorage, KindXml, Lock};
use super::props::esc;

/// The `w:lock` value of `l`, `None` for no lock.
pub(crate) fn lock_val(l: Lock) -> Option<&'static str> {
    match l {
        Lock::None => None,
        Lock::ControlLocked => Some("sdtLocked"),
        Lock::ContentLocked => Some("contentLocked"),
        Lock::Both => Some("sdtContentLocked"),
    }
}

/// The `w15:appearance` value of `a`.
pub(super) fn appearance_val(a: ControlAppearance) -> &'static str {
    match a {
        ControlAppearance::BoundingBox => "boundingBox",
        ControlAppearance::Tags => "tags",
        ControlAppearance::Hidden => "hidden",
    }
}

/// The `w:storeMappedDataAs` value of `s`.
fn storage_val(s: DateStorage) -> &'static str {
    match s {
        DateStorage::Text => "text",
        DateStorage::Date => "date",
        DateStorage::DateTime => "dateTime",
    }
}

/// A date control's `w:date`: as read while its settings are; else the element as read with the settings that changed
/// set in it, or (for one not read) one from its settings.
pub(super) fn date_xml(w: &str, p: &ControlProps) -> String {
    let ControlKind::Date { format, lid, calendar, storage, full_date } = &p.kind else { return String::new() };
    let x = match &p.kind_xml {
        Some(k) if k.kind == p.kind => return k.xml.clone(),
        Some(KindXml { xml, kind: ControlKind::Date { format: f0, lid: l0, calendar: c0, storage: s0, full_date: d0 } }) => {
            let mut x = xml.clone();
            if format != f0 {
                x = with_child_val(&x, "dateFormat", format, w, None);
            }
            // A locale is set, never cleared: no edit takes one away.
            if let Some(l) = lid.as_deref().filter(|_| lid != l0) {
                x = with_child_val(&x, "lid", l, w, Some("dateFormat"));
            }
            let calendar = calendar.as_deref().filter(|_| calendar != c0);
            let x = date_storage_xml(Some(&x), format, lid.as_deref(), calendar, storage.filter(|_| storage != s0), w);
            if full_date == d0 {
                return x;
            }
            x
        }
        _ if calendar.is_none() && storage.is_none() => format!(
            "<{w}date><{w}dateFormat {w}val=\"{}\"/>{}<{w}storeMappedDataAs {w}val=\"dateTime\"/><{w}calendar {w}val=\"gregorian\"/></{w}date>",
            esc(format),
            lid.as_ref().map_or(String::new(), |l| format!("<{w}lid {w}val=\"{}\"/>", esc(l)))
        ),
        _ => date_storage_xml(None, format, lid.as_deref(), calendar.as_deref(), *storage, w),
    };
    with_full_date(&x, full_date.as_deref()).unwrap_or(x)
}

/// The `val` attribute (any prefix) of element `x`'s start tag, unescaped.
pub(super) fn val(x: &str) -> Option<String> {
    attribute(x, "val")
}

/// Attribute `local` (any prefix) of element `x`'s start tag, unescaped.
fn attribute(x: &str, local: &str) -> Option<String> {
    let head = &x[..tag_end(x)?];
    tag_attrs(head).find(|(name, ..)| name.rsplit(':').next() == Some(local)).map(|(_, v, _)| unescape(v))
}

/// A `w:placeholder`'s building block name (its `w:docPart`'s `val`, any prefix).
pub(super) fn placeholder_of(x: &str) -> Option<String> {
    let mut from = 1;
    while let Some(k) = x[from..].find('<').map(|k| from + k) {
        let name = x[k + 1..].split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or_default();
        if name.rsplit(':').next() == Some("docPart") {
            return val(&x[k..]);
        }
        from = k + 1;
    }
    None
}

/// A `w:dataBinding`'s attributes.
pub(super) fn binding_of(x: &str) -> ControlBinding {
    ControlBinding { xpath: attribute(x, "xpath"), prefix_mappings: attribute(x, "prefixMappings"), store_item_id: attribute(x, "storeItemID") }
}

/// A `w:placeholder` naming building block `name`, in the main namespace's prefix `w`.
pub(super) fn placeholder_xml(w: &str, name: &str) -> String {
    format!("<{w}placeholder><{w}docPart {w}val=\"{}\"/></{w}placeholder>", esc(name))
}

/// A `w:dataBinding` of `b`, in the main namespace's prefix `w`.
pub(super) fn binding_xml(w: &str, b: &ControlBinding) -> String {
    let mut x = format!("<{w}dataBinding");
    if let Some(p) = &b.prefix_mappings {
        x.push_str(&format!(" {w}prefixMappings=\"{}\"", esc(p)));
    }
    if let Some(p) = &b.xpath {
        x.push_str(&format!(" {w}xpath=\"{}\"", esc(p)));
    }
    if let Some(i) = &b.store_item_id {
        x.push_str(&format!(" {w}storeItemID=\"{}\"", esc(i)));
    }
    x + "/>"
}

/// A checkbox's `w14:checkbox` once its symbols are set (from its settings, `checked` as given; Word's font for a
/// symbol without one): `None` while it keeps its element as read, or for a new one whose symbols weren't set.
pub(super) fn checkbox_edited(p: &ControlProps, checked: bool) -> Option<String> {
    let ControlKind::Checkbox { checked: on, unchecked: off, checked_font, unchecked_font, .. } = &p.kind else { return None };
    match &p.kind_xml {
        Some(k) if k.kind == p.kind => return None,
        None if checked_font.is_none() && unchecked_font.is_none() => return None,
        _ => {}
    }
    let font = |f: &Option<String>| esc(f.as_deref().unwrap_or("MS Gothic"));
    Some(format!(
        r#"<w14:checkbox xmlns:w14="{}"><w14:checked w14:val="{}"/><w14:checkedState w14:val="{:04X}" w14:font="{}"/><w14:uncheckedState w14:val="{:04X}" w14:font="{}"/></w14:checkbox>"#,
        super::super::xml::W14,
        u8::from(checked),
        *on as u32,
        font(checked_font),
        *off as u32,
        font(unchecked_font)
    ))
}

/// A `w:date` (`xml` as kept, or one of `format`) with its calendar and storage set where given.
fn date_storage_xml(xml: Option<&str>, format: &str, lid: Option<&str>, calendar: Option<&str>, storage: Option<DateStorage>, w: &str) -> String {
    let mut x = xml.map_or_else(
        || {
            let lid = lid.map(|l| format!("<{w}lid {w}val=\"{}\"/>", esc(l))).unwrap_or_default();
            format!("<{w}date><{w}dateFormat {w}val=\"{}\"/>{lid}</{w}date>", esc(format))
        },
        str::to_string,
    );
    let after_lid = if x.contains("lid") { "lid" } else { "dateFormat" };
    if let Some(s) = storage {
        x = with_child_val(&x, "storeMappedDataAs", storage_val(s), w, Some(after_lid));
    }
    if let Some(cal) = calendar {
        let after = if x.contains("storeMappedDataAs") { "storeMappedDataAs" } else { after_lid };
        x = with_child_val(&x, "calendar", cal, w, Some(after));
    }
    x
}

/// A `w:date` with the full date `want` (`w:fullDate`, any prefix, either quote, replaced), or without one, in the
/// element's own prefix. `None` when it already is so.
fn with_full_date(xml: &str, want: Option<&str>) -> Option<String> {
    let end = tag_end(xml)?;
    let mut head = xml[..end].to_string();
    let have = tag_attrs(&head).find(|(name, ..)| name.rsplit(':').next() == Some("fullDate")).map(|(_, v, r)| (unescape(v), r));
    if have.as_ref().map(|(v, _)| v.as_str()) == want {
        return None;
    }
    if let Some((_, r)) = have {
        head.replace_range(head[..r.start].trim_end().len()..r.end, "");
    }
    let name_end = head.find(|c: char| c.is_whitespace() || c == '/').unwrap_or(head.len());
    let prefix = head.get(1..name_end).and_then(|n| n.split_once(':')).map_or(String::new(), |(p, _)| format!("{p}:"));
    if let Some(f) = want {
        head.insert_str(name_end, &format!(" {prefix}fullDate=\"{}\"", esc(f)));
    }
    Some(head + &xml[end..])
}

/// Element `xml` with its child `local`'s `val` set to `val`: a child it doesn't have is added (after child `after`,
/// else first), written with prefix `w`.
fn with_child_val(xml: &str, local: &str, val: &str, w: &str, after: Option<&str>) -> String {
    let find = |name: &str| {
        let mut from = 0;
        while let Some(k) = xml[from..].find('<').map(|k| from + k) {
            let n = xml[k + 1..].split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or_default();
            if n.rsplit(':').next() == Some(name) && !n.starts_with('/') {
                return Some((k, k + tag_end(&xml[k..])? + 1));
            }
            from = k + 1;
        }
        None
    };
    let name_end = |tag: &str| tag.find(|c: char| c.is_whitespace() || c == '/' || c == '>').unwrap_or(tag.len());
    if let Some((start, end)) = find(local) {
        let tag = &xml[start..end];
        let new = match attr_value(tag, "val") {
            Some(v) => format!("{}{}{}", &tag[..v.start], esc(val), &tag[v.end..]),
            None => {
                let at = name_end(tag);
                format!("{} {w}val=\"{}\"{}", &tag[..at], esc(val), &tag[at..])
            }
        };
        return format!("{}{new}{}", &xml[..start], &xml[end..]);
    }
    let child = format!("<{w}{local} {w}val=\"{}\"/>", esc(val));
    if let Some((k, end)) = after.and_then(find) {
        // After the whole sibling, its end tag too
        let at = if xml[..end].ends_with("/>") {
            end
        } else {
            let close = format!("</{}>", &xml[k + 1..k + name_end(&xml[k..])]);
            xml[end..].find(&close).map_or(end, |c| end + c + close.len())
        };
        return format!("{}{child}{}", &xml[..at], &xml[at..]);
    }
    let Some(at) = tag_end(xml).map(|e| e + 1) else { return format!("{xml}{child}") };
    // A root without children opens for the child
    match xml[..at].strip_suffix("/>") {
        Some(head) => {
            let name = &xml[1..name_end(xml)];
            format!("{}>{child}</{name}>{}", head.trim_end(), &xml[at..])
        }
        None => format!("{}{child}{}", &xml[..at], &xml[at..]),
    }
}

/// Where the value of `tag`'s attribute of local name `local` is (`w:val = "x"` as well as `w:val="x"`).
fn attr_value(tag: &str, local: &str) -> Option<std::ops::Range<usize>> {
    let b = tag.as_bytes();
    let space = |i: usize| b.get(i).is_some_and(u8::is_ascii_whitespace);
    let mut i = tag.find(|c: char| c.is_whitespace() || c == '/' || c == '>')?;
    loop {
        while space(i) {
            i += 1;
        }
        let start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b'=' | b'/' | b'>') {
            i += 1;
        }
        let name = &tag[start..i];
        while space(i) {
            i += 1;
        }
        if name.is_empty() || b.get(i) != Some(&b'=') {
            return None;
        }
        i += 1;
        while space(i) {
            i += 1;
        }
        let q = *b.get(i).filter(|q| matches!(q, b'"' | b'\''))?;
        let from = i + 1;
        let to = from + tag[from..].find(char::from(q))?;
        if name.rsplit(':').next() == Some(local) {
            return Some(from..to);
        }
        i = to + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{date_xml, with_child_val};
    use crate::docx::model::{ControlKind, ControlProps, KindXml};

    #[test]
    fn a_child_value_is_set_where_its_attribute_is() {
        let x = r#"<w:date w:fullDate="2026-01-01T00:00:00Z"><w:dateFormat w:val = "M/d/yyyy"/></w:date>"#;
        assert_eq!(
            with_child_val(x, "dateFormat", "d MMM", "w:", None),
            r#"<w:date w:fullDate="2026-01-01T00:00:00Z"><w:dateFormat w:val = "d MMM"/></w:date>"#
        );
        let x = r#"<w:date><w:dateFormat w:x="1"/></w:date>"#;
        assert_eq!(with_child_val(x, "dateFormat", "d", "w:", None), r#"<w:date><w:dateFormat w:val="d" w:x="1"/></w:date>"#);
    }

    #[test]
    fn a_child_goes_inside_a_root_without_children() {
        let x = r#"<w:date w:fullDate="2026-01-01T00:00:00Z"/>"#;
        let with = with_child_val(x, "dateFormat", "d", "w:", None);
        assert_eq!(with, r#"<w:date w:fullDate="2026-01-01T00:00:00Z"><w:dateFormat w:val="d"/></w:date>"#);
        assert_eq!(
            with_child_val(&with, "lid", "en-GB", "w:", Some("dateFormat")),
            r#"<w:date w:fullDate="2026-01-01T00:00:00Z"><w:dateFormat w:val="d"/><w:lid w:val="en-GB"/></w:date>"#
        );
        assert_eq!(with_child_val("<w:date/>", "lid", "en-GB", "w:", Some("dateFormat")), r#"<w:date><w:lid w:val="en-GB"/></w:date>"#);
    }

    #[test]
    fn a_child_goes_after_a_siblings_end_tag() {
        let x = r#"<w:date><w:lid w:val="en-US"></w:lid></w:date>"#;
        assert_eq!(
            with_child_val(x, "storeMappedDataAs", "date", "w:", Some("lid")),
            r#"<w:date><w:lid w:val="en-US"></w:lid><w:storeMappedDataAs w:val="date"/></w:date>"#
        );
    }

    #[test]
    fn a_dates_full_date_is_replaced_whatever_its_quotes() {
        let date = |full: Option<&str>| ControlKind::Date {
            format: "M/d/yyyy".into(),
            lid: None,
            calendar: None,
            storage: None,
            full_date: full.map(str::to_string),
        };
        let props = |x: &str, now: Option<&str>| ControlProps {
            kind: date(now),
            kind_xml: Some(KindXml { xml: x.into(), kind: date(Some("2026-01-01T00:00:00Z")) }),
            ..ControlProps::default()
        };
        for x in [
            r#"<w:date w:fullDate='2026-01-01T00:00:00Z'><w:dateFormat w:val="M/d/yyyy"/></w:date>"#,
            r#"<w:date w:x="a>b" w:fullDate="2026-01-01T00:00:00Z"><w:dateFormat w:val="M/d/yyyy"/></w:date>"#,
        ] {
            let out = date_xml("w:", &props(x, Some("2027-02-03T00:00:00Z")));
            let xml = out.replacen("<w:date", "<w:date xmlns:w=\"urn:w\"", 1);
            let doc = roxmltree::Document::parse(&xml).unwrap_or_else(|e| panic!("well formed: {out}: {e}"));
            let dates: Vec<_> = doc.root_element().attributes().filter(|a| a.name() == "fullDate").map(|a| a.value()).collect();
            assert_eq!(dates, ["2027-02-03T00:00:00Z"], "{out}");
        }
        // Spaces around `=`: without a date it goes; kept, the element is as read.
        let spaced = r#"<w:date w:fullDate = "2026-01-01T00:00:00Z"><w:dateFormat w:val="M/d/yyyy"/></w:date>"#;
        let out = date_xml("w:", &props(spaced, None));
        assert!(!out.contains("fullDate"), "{out}");
        assert_eq!(date_xml("w:", &props(spaced, Some("2026-01-01T00:00:00Z"))), spaced);
    }
}
