//! Host services as typed models: context menu, dialogs, find, spelling, navigation, styles, rulers and accessibility.

pub mod a11y;
pub mod dialogs;
pub mod find;
pub mod menu;
pub mod ruler;
pub mod spelling;
pub mod styles;

/// A session's service state: spell checking, what was last announced, and what the review reported last.
#[derive(Default)]
pub struct Services {
    pub spelling: spelling::Spellers,
    pub announcer: a11y::Announcer,
    /// The revision count at a document revision.
    pub revision_count: Option<(u64, u32)>,
    /// The comments as `commentsChanged` last reported them.
    pub comments_seen: crate::docx::review::Comments,
    /// The visible comments' anchors as `commentAnchors` last reported them.
    pub anchors: Vec<crate::api::review::CommentAnchor>,
}

impl Services {
    /// Another document is open: what was reported about the last one goes.
    pub fn document_replaced(&mut self, doc: &crate::docx::Document) {
        self.revision_count = None;
        self.comments_seen = doc.comments.clone();
        self.anchors.clear();
        self.announcer = a11y::Announcer::default();
    }
}

/// The page (from 1) each laid-out paragraph starts on. Pages not laid out yet aren't looked at.
pub fn para_pages(e: &mut crate::editing::Editor) -> std::collections::HashMap<crate::layout::ParaId, u32> {
    let mut out = std::collections::HashMap::new();
    for (page, geo) in e.page_geometries() {
        for p in geo.lines.iter().filter(|l| !l.repeat).flat_map(|l| l.paras.iter()) {
            out.entry(*p).or_insert(page as u32 + 1);
        }
    }
    out
}

/// The document's counts in the text the markup view shows (All markup counts the accepted text, its deletions left
/// out): words, characters with and without spaces, paragraphs with text, and the body's laid-out lines and pages.
pub fn word_count(e: &mut crate::editing::Editor) -> crate::api::services::WordCount {
    use crate::docx::Block;
    use crate::docx::review::MarkupView;
    let view = match e.document().review.view {
        MarkupView::All => MarkupView::Simple,
        v => v,
    };
    let mut n = crate::api::services::WordCount::default();
    fn walk(blocks: &[Block], view: MarkupView, n: &mut crate::api::services::WordCount) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    let t = crate::editing::text::UnitText::of_view(p, view);
                    // Hidden and masked units are spaces; objects aren't characters.
                    let text: String = t.text.chars().filter(|&c| c != crate::editing::text::OBJECT).collect();
                    let words = text.split_whitespace().count() as u32;
                    n.words += words;
                    n.paragraphs += u32::from(words > 0);
                    n.characters += text.chars().filter(|c| !c.is_whitespace()).count() as u32;
                    n.characters_with_spaces += text.trim_end().chars().filter(|c| !c.is_control() || *c == '\t').count() as u32;
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, view, n)),
            }
        }
    }
    e.document().sections.iter().for_each(|s| walk(&s.blocks, view, &mut n));
    let pages = e.page_geometries();
    n.pages = e.snapshot().pages().len() as u32;
    n.lines = pages
        .iter()
        .flat_map(|(_, g)| {
            g.lines.iter().filter(|l| !l.repeat && g.stories.get(l.story).is_some_and(|s| s.story == crate::editing::position::StoryId::Body))
        })
        .count() as u32;
    n
}

/// The body's headings in document order (tables included): paragraphs with an outline level, from their style or
/// their own `w:outlineLvl`, that show text. A heading's id is its paragraph's, so it holds across edits elsewhere.
pub fn headings(e: &mut crate::editing::Editor) -> Vec<crate::api::services::Heading> {
    let pages = para_pages(e);
    let (doc, index) = (e.document(), e.index());
    spelling::paras_in_order(e)
        .into_iter()
        .filter_map(|id| {
            let p = index.para(doc, id)?;
            let level = p.style.outline_lvl.filter(|l| (0..9).contains(l))? as u32 + 1;
            let t = crate::editing::text::UnitText::of_view(p, doc.review.view);
            let text: String = t.text.chars().map(|c| if c.is_control() || c == crate::editing::text::OBJECT { ' ' } else { c }).collect();
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            (!text.is_empty()).then(|| crate::api::services::Heading {
                id: crate::api::value::HeadingId(id.0.to_string()),
                level,
                text,
                page: pages.get(&id).copied().unwrap_or(1),
            })
        })
        .collect()
}
