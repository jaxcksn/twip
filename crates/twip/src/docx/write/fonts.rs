//! Font renames

use std::io::{Cursor, Read};

use crate::docx::Document;
use crate::docx::font_table::FontRename;

/// The style, numbering, theme and font table parts with the document's font renames applied, added to `parts`.
pub(super) fn font_renames(doc: &Document, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    if doc.font_renames.is_empty() {
        return Ok(());
    }
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let mut zin = zip::ZipArchive::new(Cursor::new(&src.package[..])).map_err(|e| e.to_string())?;
    let mut read = |parts: &[(String, Vec<u8>)], n: &str| -> Result<Option<String>, String> {
        if let Some((_, b)) = parts.iter().find(|(m, _)| m == n) {
            return String::from_utf8(b.clone()).map(Some).map_err(|e| e.to_string());
        }
        let Ok(mut f) = zin.by_name(n) else { return Ok(None) };
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(|e| e.to_string())?;
        Ok(Some(s))
    };
    let main = src.main.clone();
    let (dir, file) = main.rsplit_once('/').unwrap_or(("", &main));
    let rels_name = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
    let rels = read(parts, &rels_name)?.ok_or("no main part relationships")?;
    for kind in ["styles", "numbering", "theme", "fontTable"] {
        let Some(name) = target(&rels, kind, dir) else { continue };
        let Some(xml) = read(parts, &name)? else { continue };
        let out = match kind {
            "theme" => rewrite(&xml, &["latin", "ea", "cs", "font"], &["typeface"], &doc.font_renames, |_| false),
            "fontTable" => {
                let names: Vec<String> = roxmltree::Document::parse(&xml)
                    .map(|d| {
                        d.descendants()
                            .filter(|n| n.has_tag_name("font"))
                            .filter_map(|n| n.attributes().find(|a| a.name() == "name").map(|a| a.value().to_lowercase()))
                            .collect()
                    })
                    .unwrap_or_default();
                // An entry for the new name already: the old entry stays as it is.
                rewrite(&xml, &["font"], &["name"], &doc.font_renames, |to| names.contains(&to.to_lowercase()))
            }
            _ => rewrite(&xml, &["rFonts"], &["ascii", "hAnsi", "eastAsia", "cs"], &doc.font_renames, |_| false),
        };
        if out != xml {
            match parts.iter_mut().find(|(m, _)| *m == name) {
                Some(p) => p.1 = out.into_bytes(),
                None => parts.push((name, out.into_bytes())),
            }
        }
    }
    Ok(())
}

/// The part a main-part relationship of `kind` targets, as a package path.
fn target(rels: &str, kind: &str, dir: &str) -> Option<String> {
    let t = rels
        .split("<Relationship ")
        .find(|r| r.contains(&format!("relationships/{kind}\"")))
        .and_then(|r| r.split("Target=\"").nth(1))
        .and_then(|t| t.split('"').next())?;
    Some(match t.strip_prefix('/') {
        Some(abs) => abs.to_string(),
        None if dir.is_empty() => t.to_string(),
        None => format!("{dir}/{t}"),
    })
}

/// `xml` with the attributes `attrs` of the elements `tags` (local names, any prefix) renamed by `renames`, in order.
/// Everything else is kept byte for byte. `keep(to)` leaves a rename to `to` undone.
// shortcut: tags are found by scanning for `<` and `>`; a `>` inside an attribute value or a comment ends the tag
// early. Parse with positions if a part needs it.
fn rewrite(xml: &str, tags: &[&str], attrs: &[&str], renames: &[FontRename], keep: impl Fn(&str) -> bool) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        rest = &rest[lt..];
        let end = rest.find('>').map_or(rest.len(), |e| e + 1);
        out.push_str(&rewrite_tag(&rest[..end], tags, attrs, renames, &keep));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn rewrite_tag(tag: &str, tags: &[&str], attrs: &[&str], renames: &[FontRename], keep: &impl Fn(&str) -> bool) -> String {
    if tag.starts_with("</") || tag.starts_with("<?") || tag.starts_with("<!") {
        return tag.to_string();
    }
    let name: String = tag[1..].chars().take_while(|c| !c.is_whitespace() && *c != '/' && *c != '>').collect();
    if !tags.contains(&local(&name)) {
        return tag.to_string();
    }
    let mut out = String::with_capacity(tag.len());
    let mut rest = tag;
    while let Some(eq) = rest.find('=') {
        let quote = rest[eq + 1..].chars().next();
        let (Some(q @ ('"' | '\'')), Some(attr)) = (quote, rest[..eq].rsplit(|c: char| c.is_whitespace()).next()) else { break };
        let open = eq + 2;
        let Some(len) = rest[open..].find(q) else { break };
        out.push_str(&rest[..open]);
        let value = &rest[open..open + len];
        let renamed = attrs.contains(&local(attr)).then(|| renamed(&unescape(value), renames)).flatten().filter(|to| !keep(to));
        match renamed {
            Some(to) => out.push_str(&crate::docx::write::props::esc(&to)),
            None => out.push_str(value),
        }
        rest = &rest[open + len..];
    }
    out.push_str(rest);
    out
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn unescape(v: &str) -> String {
    v.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// `family` after the renames, when one applies.
fn renamed(family: &str, renames: &[FontRename]) -> Option<String> {
    let mut out: Option<String> = None;
    for r in renames {
        if out.as_deref().unwrap_or(family).eq_ignore_ascii_case(&r.from) {
            out = Some(r.to.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_renamed_attribute_values_change() {
        let renames = [FontRename { from: "Calibri".into(), to: "Aptos & Co".into() }];
        let xml = r#"<w:style w:styleId="A"><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="calibri" w:eastAsia="MS Gothic"  w:hint='eastAsia'/><w:sz w:val="22"/></w:rPr><w:name w:val="Calibri"/></w:style>"#;
        let out = rewrite(xml, &["rFonts"], &["ascii", "hAnsi", "eastAsia", "cs"], &renames, |_| false);
        assert_eq!(
            out,
            r#"<w:style w:styleId="A"><w:rPr><w:rFonts w:ascii="Aptos &amp; Co" w:hAnsi="Aptos &amp; Co" w:eastAsia="MS Gothic"  w:hint='eastAsia'/><w:sz w:val="22"/></w:rPr><w:name w:val="Calibri"/></w:style>"#
        );
        let theme = r#"<a:minorFont><a:latin typeface="Calibri" panose="020F0502020204030204"/><a:ea typeface=""/></a:minorFont>"#;
        assert_eq!(
            rewrite(theme, &["latin", "ea", "cs", "font"], &["typeface"], &renames, |_| false),
            r#"<a:minorFont><a:latin typeface="Aptos &amp; Co" panose="020F0502020204030204"/><a:ea typeface=""/></a:minorFont>"#
        );
        let chained = [FontRename { from: "A".into(), to: "B".into() }, FontRename { from: "B".into(), to: "C".into() }];
        assert_eq!(renamed("a", &chained).as_deref(), Some("C"));
    }
}
