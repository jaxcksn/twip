//! Host services as typed models: context menu, dialogs, find, spelling, navigation, styles, rulers and accessibility.

pub mod a11y;
mod background_dialogs;
pub mod dialogs;
pub mod fields;
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
    /// The revision count as last counted.
    pub revision_count: Option<RevisionCount>,
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
    pub fn document_replaced(&mut self) {
        self.revision_count = None;
        self.announcer = a11y::Announcer::default();
        self.tree_cache = a11y::TreeCache::default();
    }
}

/// The revision count, and when it was counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RevisionCount {
    /// The marks' generations ([`crate::editing::Editor::revision_marks`]).
    pub marks: (u64, u64),
    /// The sub-template being edited.
    pub sub_template: Option<u32>,
    /// The count.
    pub n: u32,
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
    word_count_in(e, WordCountScope::default())
}

/// What a word count counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WordCountScope {
    /// The selection's text, when there is a selection (else the document's): its lines are the laid-out body lines
    /// it covers part of, and the pages are the document's.
    pub selection: bool,
    /// Text boxes, footnotes and endnotes count as well as the body (Word's Include textboxes, footnotes and
    /// endnotes, on by default).
    pub boxes_and_notes: bool,
}

impl Default for WordCountScope {
    fn default() -> WordCountScope {
        WordCountScope { selection: false, boxes_and_notes: true }
    }
}

/// The counts [`word_count`] gives, over `scope`.
pub fn word_count_in(e: &mut crate::editing::Editor, scope: WordCountScope) -> crate::api::services::WordCount {
    use crate::docx::review::MarkupView;
    use crate::editing::position::StoryId;
    let view = match e.document().review.view {
        MarkupView::All => MarkupView::Simple,
        v => v,
    };
    let ranges = if scope.selection && !e.selection().is_collapsed() { e.selected_ranges() } else { Vec::new() };
    let whole = ranges.is_empty();
    let stories = crate::editing::find::Stories { boxes: scope.boxes_and_notes, notes: scope.boxes_and_notes, headers: false };
    let paras: Vec<(crate::layout::ParaId, u32, u32)> = if whole {
        e.story_paras(stories).into_iter().map(|p| (p, 0, u32::MAX)).collect()
    } else {
        ranges.iter().map(|&(p, a, b)| (p, a.0, b.0)).collect()
    };
    let mut n = crate::api::services::WordCount::default();
    let (doc, index) = (e.document(), e.index());
    let counted = |story: StoryId| match story {
        StoryId::Body => true,
        StoryId::TextBox(_) | StoryId::Footnote(_) | StoryId::Endnote(_) => scope.boxes_and_notes,
        _ => false,
    };
    let mut text = String::new();
    for &(para, a, b) in &paras {
        let Some(p) = index.para(doc, para) else { continue };
        if !whole && !index.para_slot(para).is_some_and(|s| counted(index.story(s.container))) {
            continue;
        }
        let t = crate::editing::text::Seen::of(p, |r| !view.hides(&r.rev), true);
        // Objects aren't characters.
        text.clear();
        text.extend(t.chars.iter().zip(&t.units).filter(|&(&c, &u)| c != crate::editing::text::OBJECT && (a..b).contains(&u)).map(|(c, _)| *c));
        let words = text.split_whitespace().count() as u32;
        n.words += words;
        n.paragraphs += u32::from(words > 0);
        n.characters += text.chars().filter(|c| !c.is_whitespace()).count() as u32;
        n.characters_with_spaces += text.trim_end().chars().filter(|c| !c.is_control() || *c == '\t').count() as u32;
    }
    let pages = e.page_geometries();
    n.pages = e.snapshot().pages().len() as u32;
    // Ranges by paragraph, so each line looks up only its own paragraphs' ranges.
    let mut by_para = ranges;
    by_para.sort_unstable_by_key(|&(p, a, _)| (p, a));
    let covers = |l: &crate::editing::geometry::GLine| {
        whole
            || l.paras.iter().any(|&p| {
                let from = by_para.partition_point(|r| r.0 < p);
                by_para[from..]
                    .iter()
                    .take_while(|r| r.0 == p)
                    .any(|&(_, a, b)| l.first_of(p).is_none_or(|start| start.0 < b.0.max(a.0 + 1)) && l.last_of(p).is_none_or(|end| end.0 >= a.0))
            })
    };
    n.lines = pages
        .iter()
        .flat_map(|(_, g)| g.lines.iter().filter(|l| !l.repeat && g.stories.get(l.story).is_some_and(|s| s.story == StoryId::Body)))
        .filter(|l| covers(l))
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
