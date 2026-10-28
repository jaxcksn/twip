//! Comments, comment threads and people: reading the commentsExtended, commentsIds, commentsExtensible and people
//! parts. The comments part itself is read with the document's blocks (its bodies are blocks); this joins what the
//! parts beside it say about each comment, by the `w14:paraId` of the comment's last paragraph.

use std::collections::HashMap;

use roxmltree::Node;

use super::model::Raw;
use super::review::{Comment, CommentId, Comments, Identity, Timestamp};

/// An attribute by local name, whatever its namespace prefix (these parts use `w15:`, `w16cid:` and `w16cex:`).
fn local<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.attributes().find(|a| a.name() == name).map(|a| a.value())
}

/// An `ST_LongHexNumber` (`w14:paraId`, `w16cid:durableId`).
pub(super) fn hex(v: Option<&str>) -> Option<u32> {
    u32::from_str_radix(v?.trim(), 16).ok()
}

/// The root's children named `tag` (any namespace), each through `f`. A part that isn't well-formed gives nothing.
fn entries<T>(xml: Option<&str>, tag: &str, f: impl Fn(Node) -> Option<T>) -> Vec<T> {
    let Some(doc) = xml.and_then(|x| roxmltree::Document::parse(x).ok()) else { return Vec::new() };
    doc.root_element().children().filter(|n| n.is_element() && n.tag_name().name() == tag).filter_map(f).collect()
}

/// The people part's `w15:person` entries: each author's name and identity (`w15:presenceInfo`).
pub(super) fn people(xml: Option<&str>) -> Vec<(String, Option<Identity>)> {
    entries(xml, "person", |p| {
        let identity = p
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == "presenceInfo")
            .and_then(|i| Some(Identity { provider: local(i, "providerId")?.into(), user_id: local(i, "userId")?.into() }));
        Some((local(p, "author")?.to_string(), identity))
    })
}

/// The comments read from the comments part, with what the parts beside it say: replies and the done flag
/// (commentsExtended), durable ids (commentsIds), and UTC dates with the extensible data as written
/// (commentsExtensible).
pub(super) fn link(read: Vec<Comment>, extended: Option<&str>, ids: Option<&str>, extensible: Option<&str>) -> Comments {
    let on = |v: Option<&str>| matches!(v, Some("1" | "true" | "on"));
    let ex: HashMap<u32, (Option<u32>, bool)> =
        entries(extended, "commentEx", |n| Some((hex(local(n, "paraId"))?, (hex(local(n, "paraIdParent")), on(local(n, "done"))))))
            .into_iter()
            .collect();
    let durable: HashMap<u32, u32> =
        entries(ids, "commentId", |n| Some((hex(local(n, "paraId"))?, hex(local(n, "durableId"))?))).into_iter().collect();
    let mut cex: HashMap<u32, (Option<Timestamp>, Raw)> = entries(extensible, "commentExtensible", |n| {
        let raw = Raw(Some(n.document().input_text()[n.range()].into()));
        Some((hex(local(n, "durableId"))?, (local(n, "dateUtc").map(Timestamp::parse), raw)))
    })
    .into_iter()
    .collect();
    let by_para: HashMap<u32, CommentId> = read.iter().map(|c| (c.para_id, c.id)).collect();
    let mut out = Comments::default();
    for mut c in read {
        if let Some(&(parent, done)) = ex.get(&c.para_id) {
            c.parent = parent.and_then(|p| by_para.get(&p).copied()).filter(|p| *p != c.id);
            c.done = done;
        }
        c.durable_id = durable.get(&c.para_id).copied();
        if let Some((utc, raw)) = c.durable_id.and_then(|d| cex.remove(&d)) {
            c.date_utc = utc;
            c.extensible = raw;
        }
        out.by_id.insert(c.id, c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docx::review::AuthorId;

    fn comment(id: i64, para_id: u32) -> Comment {
        Comment {
            id: CommentId(id),
            author: AuthorId(0),
            initials: None,
            date: None,
            date_utc: None,
            body: Vec::new(),
            para_id,
            durable_id: None,
            parent: None,
            done: false,
            extensible: Raw::default(),
        }
    }

    #[test]
    fn parts_join_by_paragraph_and_durable_id() {
        let ex = r#"<w15:commentsEx xmlns:w15="x"><w15:commentEx w15:paraId="0000000A" w15:done="1"/><w15:commentEx w15:paraId="0000000B" w15:paraIdParent="0000000A" w15:done="0"/></w15:commentsEx>"#;
        let ids = r#"<c:commentsIds xmlns:c="y"><c:commentId c:paraId="0000000B" c:durableId="1A000002"/></c:commentsIds>"#;
        let cex = r#"<e:commentsExtensible xmlns:e="z"><e:commentExtensible e:durableId="1A000002" e:dateUtc="2026-02-02T09:00:00Z"/></e:commentsExtensible>"#;
        let cs = link(vec![comment(1, 0xA), comment(2, 0xB)], Some(ex), Some(ids), Some(cex));
        let (a, b) = (&cs.by_id[&CommentId(1)], &cs.by_id[&CommentId(2)]);
        assert!(a.done && a.parent.is_none() && a.durable_id.is_none());
        assert_eq!((b.parent, b.done, b.durable_id), (Some(CommentId(1)), false, Some(0x1A00_0002)));
        assert_eq!(b.date_utc.as_ref().map(|d| d.raw.as_ref()), Some("2026-02-02T09:00:00Z"));
        assert!(b.extensible.0.as_deref().is_some_and(|x| x.starts_with("<e:commentExtensible")));
        // Broken parts give nothing, not an error.
        assert_eq!(link(vec![comment(1, 0xA)], Some("<oops"), None, None).by_id[&CommentId(1)].parent, None);
    }

    #[test]
    fn people_have_identities() {
        let xml = r#"<w15:people xmlns:w15="x"><w15:person w15:author="A"><w15:presenceInfo w15:providerId="AD" w15:userId="a@x"/></w15:person><w15:person w15:author="B"/></w15:people>"#;
        let p = people(Some(xml));
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].1.as_ref().map(|i| (&*i.provider, &*i.user_id)), Some(("AD", "a@x")));
        assert!(p[1].1.is_none());
    }
}
