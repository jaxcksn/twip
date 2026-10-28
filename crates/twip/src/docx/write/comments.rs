//! Saving comments and people: the comments, commentsExtended, commentsIds, commentsExtensible and people parts,
//! written from the model when what they hold changed since reading, else kept as read.

use super::props::esc;
use super::{People, new_blocks_xml, set_attr};
use crate::docx::model::Document;
use crate::docx::package;
use crate::docx::review::Comments;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
const W15: &str = "http://schemas.microsoft.com/office/word/2012/wordml";
const W16CID: &str = "http://schemas.microsoft.com/office/word/2016/wordml/cid";
const W16CEX: &str = "http://schemas.microsoft.com/office/word/2018/wordml/cex";
const DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

/// The parts comments and people are saved in.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Comments,
    Extended,
    Ids,
    Extensible,
    People,
}

impl Part {
    const ALL: [Part; 5] = [Part::Comments, Part::Extended, Part::Ids, Part::Extensible, Part::People];

    fn file(self) -> &'static str {
        match self {
            Part::Comments => "comments.xml",
            Part::Extended => "commentsExtended.xml",
            Part::Ids => "commentsIds.xml",
            Part::Extensible => "commentsExtensible.xml",
            Part::People => "people.xml",
        }
    }

    fn rel_type(self) -> &'static str {
        match self {
            Part::Comments => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments",
            Part::Extended => "http://schemas.microsoft.com/office/2011/relationships/commentsExtended",
            Part::Ids => "http://schemas.microsoft.com/office/2016/09/relationships/commentsIds",
            Part::Extensible => "http://schemas.microsoft.com/office/2018/08/relationships/commentsExtensible",
            Part::People => "http://schemas.microsoft.com/office/2011/relationships/people",
        }
    }

    fn content_type(self) -> String {
        let name = self.file().trim_end_matches(".xml");
        format!("application/vnd.openxmlformats-officedocument.wordprocessingml.{name}+xml")
    }

    /// The root element's name and the namespaces its children are written with (prefix, URI).
    fn root(self) -> (&'static str, &'static [(&'static str, &'static str)]) {
        match self {
            Part::Comments => ("w:comments", &[("w", W), ("w14", W14)]),
            Part::Extended => ("w15:commentsEx", &[("w15", W15)]),
            Part::Ids => ("w16cid:commentsIds", &[("w16cid", W16CID)]),
            Part::Extensible => ("w16cex:commentsExtensible", &[("w16cex", W16CEX)]),
            Part::People => ("w15:people", &[("w15", W15)]),
        }
    }

    /// The part's content (its root's children) for these comments and people.
    fn children(self, comments: &Comments, people: &People, shell: &str) -> Result<String, String> {
        let mut out = String::new();
        let hex = |v: u32| format!("{v:08X}");
        match self {
            Part::Comments => {
                for c in comments.by_id.values() {
                    let person = people.get(c.author);
                    let mut a = format!(" w:id=\"{}\" w:author=\"{}\"", c.id.0, esc(person.map_or("", |p| &p.name)));
                    if let Some(d) = &c.date {
                        a.push_str(&format!(" w:date=\"{}\"", esc(&d.raw)));
                    }
                    if let Some(i) = c.initials.as_ref().or(person.and_then(|p| p.initials.as_ref())) {
                        a.push_str(&format!(" w:initials=\"{}\"", esc(i)));
                    }
                    let body = comment_body(&new_blocks_xml(shell, &c.body, people)?, c.para_id);
                    out.push_str(&format!("<w:comment{a}>{body}</w:comment>"));
                }
            }
            Part::Extended => {
                for c in comments.by_id.values() {
                    let parent =
                        c.parent.and_then(|p| comments.by_id.get(&p)).map_or(String::new(), |p| format!(" w15:paraIdParent=\"{}\"", hex(p.para_id)));
                    out.push_str(&format!("<w15:commentEx w15:paraId=\"{}\"{parent} w15:done=\"{}\"/>", hex(c.para_id), u8::from(c.done)));
                }
            }
            Part::Ids => {
                for c in comments.by_id.values() {
                    if let Some(d) = c.durable_id {
                        out.push_str(&format!("<w16cid:commentId w16cid:paraId=\"{}\" w16cid:durableId=\"{}\"/>", hex(c.para_id), hex(d)));
                    }
                }
            }
            Part::Extensible => {
                for c in comments.by_id.values() {
                    let Some(d) = c.durable_id else { continue };
                    match c.extensible.0.as_deref() {
                        // As read, with the model's ids and date.
                        Some(raw) => {
                            let end = raw.find('>').map_or(raw.len(), |e| e + 1);
                            let name = raw.get(1..).unwrap_or("").split(|ch: char| ch.is_whitespace() || ch == '>' || ch == '/').next().unwrap_or("");
                            let p = name.rsplit_once(':').map_or(String::new(), |(p, _)| format!("{p}:"));
                            let mut tag = set_attr(&raw[..end], &format!("{p}durableId"), &hex(d));
                            if let Some(t) = &c.date_utc {
                                tag = set_attr(&tag, &format!("{p}dateUtc"), &esc(&t.raw));
                            }
                            out.push_str(&tag);
                            out.push_str(&raw[end..]);
                        }
                        None => {
                            let date = c.date_utc.as_ref().map_or(String::new(), |t| format!(" w16cex:dateUtc=\"{}\"", esc(&t.raw)));
                            out.push_str(&format!("<w16cex:commentExtensible w16cex:durableId=\"{}\"{date}/>", hex(d)));
                        }
                    }
                }
            }
            Part::People => {
                for p in &people.persons {
                    let presence = p.identity.as_ref().map_or(String::new(), |i| {
                        format!("<w15:presenceInfo w15:providerId=\"{}\" w15:userId=\"{}\"/>", esc(&i.provider), esc(&i.user_id))
                    });
                    out.push_str(&format!("<w15:person w15:author=\"{}\">{presence}</w15:person>", esc(&p.name)));
                }
            }
        }
        Ok(out)
    }
}

/// A comment's body as written: its last paragraph with the comment's `w14:paraId`, and its first starting with the
/// annotation mark (a body with no paragraph gets one).
fn comment_body(blocks: &str, para_id: u32) -> String {
    let mut s = if blocks.contains("<w:p>") || blocks.contains("<w:p ") { blocks.to_string() } else { format!("{blocks}<w:p></w:p>") };
    let start = |s: &str, from_end: bool| {
        let (a, b) = if from_end { (s.rfind("<w:p>"), s.rfind("<w:p ")) } else { (s.find("<w:p>"), s.find("<w:p ")) };
        match (a, b) {
            (Some(a), Some(b)) => Some(if from_end { a.max(b) } else { a.min(b) }),
            (x, y) => x.or(y),
        }
    };
    if !s.contains("annotationRef")
        && let Some(k) = start(&s, false)
    {
        let open_end = s[k..].find('>').map_or(s.len(), |e| k + e + 1);
        // After the paragraph's properties, when it starts with them.
        let at = if s[open_end..].starts_with("<w:pPr") {
            s[open_end..].find("</w:pPr>").map(|e| open_end + e + "</w:pPr>".len()).or_else(|| s[open_end..].find("/>").map(|e| open_end + e + 2))
        } else {
            None
        };
        s.insert_str(at.unwrap_or(open_end), "<w:r><w:annotationRef/></w:r>");
    }
    if let Some(k) = start(&s, true)
        && let Some(end) = s[k..].find('>').map(|e| k + e + 1)
    {
        let tag = set_attr(&super::strip_attr(&s[k..end], "paraId"), "w14:paraId", &format!("{para_id:08X}"));
        s.replace_range(k..end, &tag);
    }
    s
}

/// The root start tag of a part as read (`xml`) or a new one, with every namespace its children are written with.
fn root_open(part: Part, xml: Option<&str>) -> (String, String) {
    let (name, ns) = part.root();
    let read = xml.and_then(|x| {
        let dom = roxmltree::Document::parse(x).ok()?;
        let r = dom.root_element().range();
        let decl = x[..r.start].to_string();
        let el = &x[r.clone()];
        let end = el.find('>')?;
        let open = el[..end].trim_end_matches('/').to_string() + ">";
        let close = format!("</{}>", el[1..].split(|c: char| c.is_whitespace() || c == '>' || c == '/').next().unwrap_or(name));
        Some((decl, open, close))
    });
    let (decl, mut open, close) = read.unwrap_or_else(|| (DECL.to_string(), format!("<{name}>"), format!("</{name}>")));
    for (p, uri) in ns {
        if !open.contains(&format!("xmlns:{p}=")) {
            let at = open.len() - 1;
            open.insert_str(at, &format!(" xmlns:{p}=\"{uri}\""));
        }
    }
    (decl + &open, close)
}

/// The comment and people parts whose content changed since reading (`read`: the document as read), written from the
/// model into `parts`; new parts are added to the package.
pub(super) fn save(doc: &Document, read: &Document, package: &[u8], main: &str, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    for part in Part::ALL {
        let existing = package::related(package, parts, main, part.rel_type())?;
        let xml = match &existing {
            Some(n) => package::read(package, parts, n)?,
            None => None,
        };
        let (open, close) = root_open(part, xml.as_deref());
        let shell = format!("{open}{close}");
        let now = part.children(&doc.comments, &doc.people, &shell)?;
        if now == part.children(&read.comments, &read.people, &shell)? || (existing.is_none() && now.is_empty()) {
            continue;
        }
        let name = match existing {
            Some(n) => n,
            None => {
                let name = match main.rsplit_once('/') {
                    Some((dir, _)) => format!("{dir}/{}", part.file()),
                    None => part.file().to_string(),
                };
                package::add_part(package, parts, main, &name, &part.content_type(), part.rel_type())?;
                name
            }
        };
        package::set(parts, &name, format!("{open}{now}{close}").into_bytes());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::docx::model::Raw;
    use crate::docx::review::{AuthorId, Comment, CommentId, Identity, Timestamp};

    fn comment(id: i64, para: u32, parent: Option<i64>, done: bool) -> Comment {
        Comment {
            id: CommentId(id),
            author: AuthorId(0),
            initials: Some("AA".into()),
            date: Some(Timestamp::parse("2026-02-01T09:00:00Z")),
            date_utc: Some(Timestamp::parse("2026-02-01T09:00:00Z")),
            body: Vec::new(),
            para_id: para,
            durable_id: Some(0x1A00_0000 + para),
            parent: parent.map(CommentId),
            done,
            extensible: Raw::default(),
        }
    }

    fn model() -> (Comments, People) {
        let mut people = People::default();
        let id = Identity { provider: "AD".into(), user_id: "a@example.com".into() };
        people.author("Author A", Some(&id));
        people.author("Author <B>", None);
        let mut cs = Comments::default();
        for c in [comment(0, 0x10, None, true), comment(1, 0x11, Some(0), false)] {
            cs.by_id.insert(c.id, c);
        }
        (cs, people)
    }

    #[test]
    fn threads_and_people_parts() {
        let (cs, people) = model();
        let ex = Part::Extended.children(&cs, &people, "").expect("written");
        assert_eq!(
            ex,
            r#"<w15:commentEx w15:paraId="00000010" w15:done="1"/><w15:commentEx w15:paraId="00000011" w15:paraIdParent="00000010" w15:done="0"/>"#
        );
        let ids = Part::Ids.children(&cs, &people, "").expect("written");
        assert!(ids.starts_with(r#"<w16cid:commentId w16cid:paraId="00000010" w16cid:durableId="1A000010"/>"#), "{ids}");
        let cex = Part::Extensible.children(&cs, &people, "").expect("written");
        assert!(cex.starts_with(r#"<w16cex:commentExtensible w16cex:durableId="1A000010" w16cex:dateUtc="2026-02-01T09:00:00Z"/>"#), "{cex}");
        let ps = Part::People.children(&cs, &people, "").expect("written");
        assert_eq!(
            ps,
            r#"<w15:person w15:author="Author A"><w15:presenceInfo w15:providerId="AD" w15:userId="a@example.com"/></w15:person><w15:person w15:author="Author &lt;B&gt;"></w15:person>"#
        );
    }

    #[test]
    fn extensible_data_is_kept_with_the_model_date() {
        let (mut cs, people) = model();
        if let Some(c) = cs.by_id.get_mut(&CommentId(0)) {
            c.extensible = Raw(Some(Rc::from(
                r#"<w16cex:commentExtensible w16cex:durableId="1" w16cex:dateUtc="old"><w16cex:extLst/></w16cex:commentExtensible>"#,
            )));
        }
        let cex = Part::Extensible.children(&cs, &people, "").expect("written");
        assert!(
            cex.starts_with(r#"<w16cex:commentExtensible w16cex:durableId="1A000010" w16cex:dateUtc="2026-02-01T09:00:00Z"><w16cex:extLst/></w16cex:commentExtensible>"#),
            "{cex}"
        );
    }

    #[test]
    fn bodies_carry_the_para_id_and_the_annotation_mark() {
        let (cs, people) = model();
        let (open, close) = root_open(Part::Comments, None);
        let xml = Part::Comments.children(&cs, &people, &format!("{open}{close}")).expect("written");
        assert!(
            xml.starts_with(r#"<w:comment w:id="0" w:author="Author A" w:date="2026-02-01T09:00:00Z" w:initials="AA"><w:p w14:paraId="00000010"><w:r><w:annotationRef/></w:r></w:p></w:comment>"#),
            "{xml}"
        );
        let two = comment_body(
            r#"<w:p><w:pPr><w:pStyle w:val="CommentText"/></w:pPr><w:r><w:t>a</w:t></w:r></w:p><w:p w14:paraId="0000AAAA"><w:r><w:t>b</w:t></w:r></w:p>"#,
            0x42,
        );
        assert_eq!(
            two,
            r#"<w:p><w:pPr><w:pStyle w:val="CommentText"/></w:pPr><w:r><w:annotationRef/></w:r><w:r><w:t>a</w:t></w:r></w:p><w:p w14:paraId="00000042"><w:r><w:t>b</w:t></w:r></w:p>"#
        );
        assert!(open.contains(&format!("xmlns:w14=\"{W14}\"")), "{open}");
        // A part as read keeps its root, with the namespaces its content needs.
        let (open, close) = root_open(Part::People, Some(r#"<?xml version="1.0"?><w15:people xmlns:w15="x" xmlns:mc="m"/>"#));
        assert_eq!((open.as_str(), close.as_str()), (r#"<?xml version="1.0"?><w15:people xmlns:w15="x" xmlns:mc="m">"#, "</w15:people>"));
    }
}
