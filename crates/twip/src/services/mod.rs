//! Host services as typed models: context menu, dialogs, find, spelling, navigation, styles, rulers and accessibility.

pub mod a11y;
mod background_dialogs;
pub mod dialogs;
pub mod find;
pub mod lists;
pub mod menu;
pub mod ruler;
pub mod spelling;
pub mod styles;

/// A session's service state: spell checking, what was last announced, and what the review reported last.
#[derive(Default)]
pub struct Services {
    pub spelling: spelling::Spellers,
    /// The hyphenation patterns the host supplied with its dictionaries.
    pub hyphenation: Option<crate::layout::hyphenate::Hyphenation>,
    pub announcer: a11y::Announcer,
    /// The paragraphs the accessibility tree read last, for its next build.
    pub tree_cache: a11y::TreeCache,
    /// What the command being run says (undo's label, a format turned on), said with the change it makes.
    pub said: Vec<(crate::api::services::AnnouncementKind, String)>,
    /// The kinds of announcements the application turned off.
    pub silenced: Vec<crate::api::services::AnnouncementKind>,
    /// The revision count at a document revision, with the sub-template then edited.
    pub revision_count: Option<((u64, Option<u32>), u32)>,
    /// The comments as `commentsChanged` last reported them.
    pub comments_seen: crate::docx::review::Comments,
    /// The visible comments' anchors as `commentAnchors` last reported them (kept over an open, so the host hears
    /// that the last document's are gone).
    pub anchors: Vec<crate::api::review::CommentAnchor>,
}

impl Services {
    /// Adds a dictionary's spelling and hyphenation patterns; when its spelling doesn't load, neither is added.
    pub fn add_dictionary(&mut self, d: &crate::api::engine::Dictionary) -> Result<(), String> {
        if !(d.hyphenation.is_some() && d.aff.0.is_empty() && d.dic.0.is_empty()) {
            self.spelling.add_dictionary(&d.language, &d.aff.0, &d.dic.0)?;
        }
        if let Some(h) = &d.hyphenation {
            let patterns = crate::layout::hyphenate::Patterns::parse(&String::from_utf8_lossy(&h.0));
            self.hyphenation = Some(self.hyphenation.clone().unwrap_or_default().with(&d.language, patterns));
        }
        Ok(())
    }

    /// Another document is open: what was reported about the last one goes, but for the anchors the host still
    /// shows.
    pub fn document_replaced(&mut self, doc: &crate::docx::Document) {
        self.revision_count = None;
        self.comments_seen = doc.comments.clone();
        self.announcer = a11y::Announcer::default();
        self.tree_cache = a11y::TreeCache::default();
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
/// out), fields as their results, in the body, text boxes and notes (as Word counts by default): words, characters
/// with and without spaces, paragraphs with text, and the body's laid-out lines and pages.
pub fn word_count(e: &mut crate::editing::Editor) -> crate::api::services::WordCount {
    use crate::docx::review::MarkupView;
    let view = match e.document().review.view {
        MarkupView::All => MarkupView::Simple,
        v => v,
    };
    let mut n = crate::api::services::WordCount::default();
    let (doc, index) = (e.document(), e.index());
    let stories = crate::editing::find::Stories { boxes: true, notes: true, headers: false };
    for para in e.story_paras(stories) {
        let Some(p) = index.para(doc, para) else { continue };
        let t = crate::editing::text::Seen::of(p, |r| !view.hides(&r.rev), true);
        // Objects aren't characters.
        let text: String = t.chars.into_iter().filter(|&c| c != crate::editing::text::OBJECT).collect();
        let words = text.split_whitespace().count() as u32;
        n.words += words;
        n.paragraphs += u32::from(words > 0);
        n.characters += text.chars().filter(|c| !c.is_whitespace()).count() as u32;
        n.characters_with_spaces += text.trim_end().chars().filter(|c| !c.is_control() || *c == '\t').count() as u32;
    }
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

/// The body's headings in document order (tables included): paragraphs with a heading level (as the reading tree
/// gives it) that show text. A heading's id is its paragraph's, so it holds across edits elsewhere.
pub fn headings(e: &mut crate::editing::Editor) -> Vec<crate::api::services::Heading> {
    let pages = para_pages(e);
    let (doc, index) = (e.document(), e.index());
    spelling::paras_in_order(e)
        .into_iter()
        .filter_map(|id| {
            let p = index.para(doc, id)?;
            let level = a11y::roles::heading_level(doc, p)?;
            let view = doc.review.view;
            let t = crate::editing::text::Seen::of(p, |r| !view.hides(&r.rev), true);
            let text: String = t.chars.into_iter().map(|c| if c.is_control() || c == crate::editing::text::OBJECT { ' ' } else { c }).collect();
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
