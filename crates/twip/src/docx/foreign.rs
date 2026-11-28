//! Foreign content in DOCX: elements kept as read

use std::collections::HashSet;
use std::rc::Rc;

use roxmltree::Node;

use super::model::{Foreign, FormatId};
use super::xml::{MC, R};

/// The format of a payload read from, and written to, a DOCX part.
pub const DOCX: FormatId = FormatId("docx");

/// VML's office namespace, whose `o:relid` names a relationship.
const O: &str = "urn:schemas-microsoft-com:office:office";

/// `payload`, the XML of the elements `nodes` (each element whose name and attributes it holds), as foreign content:
/// the bindings its names use that its start tag doesn't declare, and whether it names a relationship.
pub(super) fn read<'a, 'i: 'a>(payload: Rc<str>, nodes: impl Iterator<Item = Node<'a, 'i>>, equation: bool) -> Foreign {
    let head = &payload[..payload.find('>').unwrap_or(payload.len())];
    let declared = |p: &str| {
        let decl = if p.is_empty() { "xmlns".to_string() } else { format!("xmlns:{p}") };
        head.match_indices(&decl).any(|(i, m)| head[i + m.len()..].trim_start().starts_with('='))
    };
    let mut seen: HashSet<&str> = HashSet::new();
    let mut bindings: Vec<(Rc<str>, Rc<str>)> = Vec::new();
    let mut package_bound = false;
    let mut add = |n: Node, p: &'a str| {
        if p != "xml"
            && seen.insert(p)
            && !declared(p)
            && let Some(uri) = n.lookup_namespace_uri((!p.is_empty()).then_some(p))
        {
            bindings.push((p.into(), uri.into()));
        }
    };
    for n in nodes.filter(|n| n.is_element()) {
        let text = n.document().input_text();
        let tag = &text[n.range()][1..];
        let qname = &tag[..tag.find(|c: char| c.is_whitespace() || c == '/' || c == '>').unwrap_or(tag.len())];
        if let Some((p, _)) = qname.split_once(':') {
            add(n, p);
        } else if n.tag_name().namespace().is_some() {
            add(n, "");
        }
        for a in n.attributes() {
            if let Some((p, _)) = text[a.range_qname()].split_once(':') {
                add(n, p);
            }
            package_bound |= a.namespace() == Some(R) || (a.namespace() == Some(O) && a.name() == "relid");
            // Markup compatibility's prefixes, alone or in a qualified name
            let words = match (a.namespace(), a.name()) {
                (Some(MC), "Ignorable" | "MustUnderstand" | "ProcessContent" | "PreserveElements" | "PreserveAttributes") => true,
                (None, "Requires") => n.tag_name().namespace() == Some(MC),
                _ => false,
            };
            if words {
                for w in a.value().split_ascii_whitespace() {
                    add(n, w.split_once(':').map_or(w, |(p, _)| p));
                }
            }
        }
    }
    Foreign { format: DOCX, payload, bindings: bindings.into(), package_bound, equation }
}

/// `f` declaring on its start tag each binding of `decls` (prefix, empty for the default namespace, and URI) whose
/// prefix it uses undeclared, in their order: what it needs written under a root that lacks them; `None` when it needs
/// none of them.
pub fn declaring(f: &Foreign, decls: &[(String, String)]) -> Option<Foreign> {
    let mut out = String::new();
    let mut bindings: Vec<(Rc<str>, Rc<str>)> = f.bindings.to_vec();
    for (p, uri) in decls {
        let Some(k) = bindings.iter().position(|(b, _)| **b == **p) else { continue };
        bindings.remove(k);
        let decl = if p.is_empty() { "xmlns".to_string() } else { format!("xmlns:{p}") };
        out.push_str(&format!(" {decl}=\"{}\"", super::write::props::esc(uri)));
    }
    if out.is_empty() {
        return None;
    }
    let at = f.payload.find(|c: char| c.is_whitespace() || c == '/' || c == '>')?;
    let payload = format!("{}{out}{}", &f.payload[..at], &f.payload[at..]);
    Some(Foreign { payload: payload.into(), bindings: bindings.into(), ..f.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

    fn read_first(xml: &str, name: &str) -> Foreign {
        let doc = roxmltree::Document::parse(xml).expect("parses");
        let e = doc.descendants().find(|n| n.tag_name().name() == name).expect("element");
        read(xml[e.range()].into(), e.descendants(), false)
    }

    #[test]
    fn a_payload_keeps_the_bindings_it_uses_undeclared_and_whether_it_names_a_relationship() {
        let root = format!(r#"<w:document xmlns:w="{W}" xmlns:m="{M}" xmlns:r="{R}" xmlns:mc="{MC}" xmlns:x="urn:x"><w:p>"#);
        let math = read_first(&format!("{root}<m:oMath><m:r><w:rPr/><m:t>x</m:t></m:r></m:oMath></w:p></w:document>"), "oMath");
        let names: Vec<(&str, &str)> = math.bindings.iter().map(|(p, u)| (&**p, &**u)).collect();
        assert_eq!(names, [("m", M), ("w", W)], "in first-use order");
        assert!(!math.package_bound);

        let sub = read_first(&format!(r#"{root}<w:subDoc r:id="rId4" mc:Ignorable="x"/></w:p></w:document>"#), "subDoc");
        let names: Vec<&str> = sub.bindings.iter().map(|(p, _)| &**p).collect();
        assert_eq!(names, ["w", "r", "mc", "x"], "markup compatibility's prefixes too");
        assert!(sub.package_bound, "an r:id");

        let own = read_first(&format!(r#"{root}<m:oMath xmlns:m="{M}"><m:r/></m:oMath></w:p></w:document>"#), "oMath");
        assert!(own.bindings.is_empty(), "declared on its start tag");
    }

    #[test]
    fn a_payload_declares_only_the_missing_bindings_it_uses() {
        let math = read_first(&format!(r#"<w:document xmlns:w="{W}" xmlns:m="{M}"><m:oMath><m:r/></m:oMath></w:document>"#), "oMath");
        let decls = [("w".to_string(), W.to_string()), ("a".to_string(), "urn:a".to_string()), ("m".to_string(), M.to_string())];
        let f = declaring(&math, &decls).expect("needs m");
        assert_eq!(&*f.payload, format!(r#"<m:oMath xmlns:m="{M}"><m:r/></m:oMath>"#), "only those it uses");
        assert!(f.bindings.is_empty(), "now declared");
        assert!(declaring(&f, &decls).is_none());
    }
}
