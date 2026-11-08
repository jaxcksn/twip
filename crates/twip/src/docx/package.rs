//! Package parts: adding a part with its content type and its relationship from the main part.

use std::io::{Cursor, Read};

/// A part's text: as made by this save (`parts`), else as `package` has it; `None` when neither has it.
pub fn read(package: &[u8], parts: &[(String, Vec<u8>)], name: &str) -> Result<Option<String>, String> {
    if let Some((_, b)) = parts.iter().find(|(n, _)| n == name) {
        return std::str::from_utf8(b).map(|s| Some(s.to_string())).map_err(|e| e.to_string());
    }
    let mut zin = zip::ZipArchive::new(Cursor::new(package)).map_err(|e| e.to_string())?;
    let Ok(mut f) = zin.by_name(name) else { return Ok(None) };
    let mut s = String::new();
    f.read_to_string(&mut s).map_err(|e| e.to_string())?;
    Ok(Some(s))
}

/// Sets a part's content in `parts`, replacing what this save made for it.
pub fn set(parts: &mut Vec<(String, Vec<u8>)>, name: &str, bytes: Vec<u8>) {
    match parts.iter_mut().find(|(n, _)| n == name) {
        Some(p) => p.1 = bytes,
        None => parts.push((name.to_string(), bytes)),
    }
}

/// The relationships part of `part` (`word/_rels/document.xml.rels` for `word/document.xml`).
pub fn rels_name(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((d, f)) => format!("{d}/_rels/{f}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// The part `main`'s first relationship of `rel_type` points to, as a part name.
pub fn related(package: &[u8], parts: &[(String, Vec<u8>)], main: &str, rel_type: &str) -> Result<Option<String>, String> {
    let Some(rels) = read(package, parts, &rels_name(main))? else { return Ok(None) };
    let dom = roxmltree::Document::parse(&rels).map_err(|e| e.to_string())?;
    let target = dom
        .root_element()
        .children()
        .filter(|r| r.attribute("Type") == Some(rel_type) && r.attribute("TargetMode") != Some("External"))
        .find_map(|r| r.attribute("Target"));
    Ok(target.map(|t| resolve(main, t)))
}

/// A relationship target of `from` as a part name: absolute targets from the package root, others from `from`'s folder.
fn resolve(from: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_string();
    }
    let mut path: Vec<&str> = from.rsplit_once('/').map_or(Vec::new(), |(d, _)| d.split('/').collect());
    for seg in target.split('/') {
        match seg {
            ".." => {
                path.pop();
            }
            "." | "" => {}
            s => path.push(s),
        }
    }
    path.join("/")
}

/// Adds part `name` to the package's content types (an `Override` of `content_type`) and a relationship of
/// `rel_type` from `main` to it with a free `rId`, both in `parts`. Either is kept when the package already has it.
/// The relationship's id is returned. The part's content is set apart ([`set`]).
pub fn add_part(
    package: &[u8],
    parts: &mut Vec<(String, Vec<u8>)>,
    main: &str,
    name: &str,
    content_type: &str,
    rel_type: &str,
) -> Result<String, String> {
    let mut types = read(package, parts, "[Content_Types].xml")?.ok_or("no content types")?;
    if !types.contains(&format!("PartName=\"/{name}\"")) {
        let close = types.rfind("</Types>").ok_or("content types without their end tag")?;
        types.insert_str(close, &format!(r#"<Override PartName="/{name}" ContentType="{content_type}"/>"#));
        set(parts, "[Content_Types].xml", types.into_bytes());
    }
    let rels_part = rels_name(main);
    let mut rels = read(package, parts, &rels_part)?.unwrap_or_else(|| {
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#
            .to_string()
    });
    // Beside the main part, relative to its folder; elsewhere, from the package root.
    let target = match main.rsplit_once('/') {
        Some((dir, _)) => name.strip_prefix(&format!("{dir}/")).map_or_else(|| format!("/{name}"), str::to_string),
        None => name.to_string(),
    };
    let existing = {
        let dom = roxmltree::Document::parse(&rels).map_err(|e| e.to_string())?;
        dom.root_element()
            .children()
            .find(|r| r.attribute("Type") == Some(rel_type) && r.attribute("Target").is_some_and(|t| resolve(main, t) == name))
            .and_then(|r| r.attribute("Id"))
            .map(str::to_string)
    };
    if let Some(id) = existing {
        return Ok(id);
    }
    let k = (1u32..).find(|k| !rels.contains(&format!("Id=\"rId{k}\""))).unwrap_or(1);
    let close = rels.rfind("</Relationships>").ok_or("relationships without their end tag")?;
    rels.insert_str(close, &format!(r#"<Relationship Id="rId{k}" Type="{rel_type}" Target="{target}"/>"#));
    set(parts, &rels_part, rels.into_bytes());
    Ok(format!("rId{k}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(files: &[(&str, &str)]) -> Vec<u8> {
        use std::io::Write;
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (n, x) in files {
            z.start_file(*n, zip::write::SimpleFileOptions::default()).expect("a zip entry");
            z.write_all(x.as_bytes()).expect("written");
        }
        z.finish().expect("a zip").into_inner()
    }

    #[test]
    fn adds_a_type_and_a_relationship_once() {
        let pkg = package(&[
            ("[Content_Types].xml", r#"<Types xmlns="t"><Default Extension="xml" ContentType="application/xml"/></Types>"#),
            ("word/_rels/document.xml.rels", r#"<Relationships xmlns="r"><Relationship Id="rId1" Type="s" Target="styles.xml"/></Relationships>"#),
        ]);
        let mut parts = Vec::new();
        let id = add_part(&pkg, &mut parts, "word/document.xml", "word/comments.xml", "ct/comments", "rel/comments").expect("added");
        assert_eq!(id, "rId2");
        let types = read(&pkg, &parts, "[Content_Types].xml").expect("read").expect("there");
        assert!(types.contains(r#"<Override PartName="/word/comments.xml" ContentType="ct/comments"/></Types>"#), "{types}");
        let rels = read(&pkg, &parts, "word/_rels/document.xml.rels").expect("read").expect("there");
        assert!(rels.contains(r#"<Relationship Id="rId2" Type="rel/comments" Target="comments.xml"/>"#), "{rels}");
        assert_eq!(related(&pkg, &parts, "word/document.xml", "rel/comments").expect("read"), Some("word/comments.xml".to_string()));
        // Again: nothing more.
        assert_eq!(add_part(&pkg, &mut parts, "word/document.xml", "word/comments.xml", "ct/comments", "rel/comments").expect("kept"), "rId2");
        let again = read(&pkg, &parts, "word/_rels/document.xml.rels").expect("read").expect("there");
        assert_eq!(again.matches("rel/comments").count(), 1);
    }

    #[test]
    fn targets_resolve_from_the_part_folder() {
        assert_eq!(resolve("word/document.xml", "comments.xml"), "word/comments.xml");
        assert_eq!(resolve("word/document.xml", "../customXml/item1.xml"), "customXml/item1.xml");
        assert_eq!(resolve("word/document.xml", "/word/people.xml"), "word/people.xml");
    }
}
