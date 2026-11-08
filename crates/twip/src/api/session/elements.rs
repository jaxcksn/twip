//! Merge designer elements (fields and content controls) and custom XML data parts. Their changes are commands.

use super::Session;
use crate::api::command::ElementField;
use crate::api::error::Error;
use crate::api::record::Origin;
use crate::api::services::{
    Block, BlockCell, BlockRow, DataPart, ElementDetail, ElementFilter, ElementInfo, ReferenceResolution, SubTemplateInfo, SubTemplateReference,
    VerticalMerge,
};
use crate::api::state::ElementKind;
use crate::api::value::{
    CheckboxOptions, ControlLock, ControlOptions, DataBinding, DateOptions, ElementId, ListItem, ListOptions, PictureOptions, PlainTextOptions,
};
use crate::docx::{ControlKind, Lock};
use crate::editing::elements::{self, Element};

/// An element's public kind.
fn kind(e: &Element) -> ElementKind {
    let Some(p) = &e.props else { return ElementKind::Field };
    match &p.kind {
        ControlKind::PlainText { .. } => ElementKind::PlainText,
        ControlKind::RichText | ControlKind::Other => ElementKind::RichText,
        ControlKind::Picture => ElementKind::Picture,
        ControlKind::Checkbox { .. } => ElementKind::Checkbox,
        ControlKind::Date { .. } => ElementKind::Date,
        ControlKind::DropDown if p.kind_xml.as_deref().is_some_and(|x| x.contains("comboBox")) => ElementKind::ComboBox,
        ControlKind::DropDown => ElementKind::DropDown,
        ControlKind::Group => ElementKind::Group,
        ControlKind::RepeatingSection => ElementKind::RepeatingSection,
        ControlKind::RepeatingItem => ElementKind::RepeatingItem,
    }
}

fn info(s: &Session, e: &Element) -> ElementInfo {
    let p = e.props.as_ref();
    ElementInfo {
        id: ElementId(elements::id_str(e.id)),
        kind: kind(e),
        code: e.code.clone(),
        tag: p.and_then(|p| p.tag.clone()),
        title: p.and_then(|p| p.alias.clone()),
        display: e.display.clone(),
        story: super::review::story(&s.editor, e.story),
        sub_template: match e.story {
            crate::editing::position::StoryId::BuildingBlock(k) => {
                s.editor.document().glossary.as_ref().and_then(|g| g.parts.get(k as usize)).map(|b| b.name.clone())
            }
            _ => None,
        },
        parent: e.parent.map(|p| ElementId(elements::id_str(p))),
        locked: e.locked,
        showing_placeholder: p.map(|p| p.showing_placeholder),
        reference: match e.id {
            crate::docx::ElementId::Field(f) => s.editor.references().by_field.get(&f).map(|r| reference(s, f, r)),
            crate::docx::ElementId::Control(_) => None,
        },
        size: match e.id {
            crate::docx::ElementId::Control(c) => s.editor.picture_control_size(c),
            crate::docx::ElementId::Field(_) => None,
        },
    }
}

/// A reference field's sub-template as the API reports it.
fn reference(s: &Session, f: crate::docx::FieldId, r: &crate::editing::sub_templates::Reference) -> SubTemplateReference {
    use crate::editing::sub_templates::Resolution as R;
    let (resolution, near_match) = match &r.resolution {
        R::Resolved => (ReferenceResolution::Resolved, None),
        R::Missing => (ReferenceResolution::Missing, None),
        R::NearMatch(n) => (ReferenceResolution::NearMatch, Some(n.clone())),
        R::Cycle => (ReferenceResolution::Cycle, None),
        R::TooDeep => (ReferenceResolution::TooDeep, None),
        R::NotAlone => (ReferenceResolution::NotAlone, None),
        R::NotInBody => (ReferenceResolution::NotInBody, None),
    };
    SubTemplateReference { name: r.name.clone(), resolution, near_match, expanded: s.editor.reference_expanded(f) }
}

/// The value of attribute `name` (`w:xpath`) in an element's XML as written.
fn attribute(xml: &str, name: &str) -> Option<String> {
    let at = xml.find(&format!("{name}=\""))? + name.len() + 2;
    let end = xml[at..].find('"')?;
    Some(xml[at..at + end].replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&"))
}

/// A control kind's settings.
fn options(s: &Session, e: &Element) -> Option<ControlOptions> {
    let p = e.props.as_ref()?;
    Some(match &p.kind {
        ControlKind::Checkbox { checked, .. } => ControlOptions::Checkbox(CheckboxOptions { checked: Some(e.display == checked.to_string()) }),
        ControlKind::Date { format, lid } => {
            ControlOptions::Date(DateOptions { format: Some(format.clone()).filter(|f| !f.is_empty()), locale: lid.clone() })
        }
        ControlKind::DropDown => {
            let xml = p.kind_xml.as_deref().unwrap_or("");
            let items = xml
                .split("<w:listItem")
                .skip(1)
                .map(|item| {
                    let text = attribute(item, "w:displayText").unwrap_or_default();
                    ListItem { value: attribute(item, "w:value").unwrap_or_else(|| text.clone()), text }
                })
                .collect();
            ControlOptions::List(ListOptions { items })
        }
        ControlKind::PlainText { multi_line } => ControlOptions::PlainText(PlainTextOptions { multiline: Some(*multi_line) }),
        ControlKind::Picture => {
            let crate::docx::ElementId::Control(c) = e.id else { return None };
            let size = s.editor.picture_control_size(c)?;
            ControlOptions::Picture(PictureOptions { width: Some(size.width), height: Some(size.height) })
        }
        _ => return None,
    })
}

/// The blocks as the structure query gives them, `depth` levels of tables deep.
fn blocks(list: &[crate::docx::Block], depth: u32) -> Vec<Block> {
    use crate::docx::{Block as B, Inline, VMerge};
    list.iter()
        .map(|b| match b {
            B::Para(p) => Block::Paragraph {
                text_length: crate::editing::text::UnitText::of(p).len().0,
                elements: p
                    .inlines
                    .iter()
                    .filter_map(|i| match i {
                        Inline::FieldBegin { id, .. } => Some(crate::docx::ElementId::Field(*id)),
                        Inline::ControlStart(c) | Inline::Sym { control: Some(c), .. } => Some(crate::docx::ElementId::Control(*c)),
                        _ => None,
                    })
                    .map(|id| ElementId(elements::id_str(id)))
                    .collect(),
            },
            B::Table(t) => Block::Table {
                rows: t
                    .rows
                    .iter()
                    .map(|r| BlockRow {
                        cells: r
                            .cells
                            .iter()
                            .map(|c| BlockCell {
                                span: c.span.max(1) as u32,
                                vertical_merge: match c.v_merge {
                                    VMerge::None => VerticalMerge::None,
                                    VMerge::Restart => VerticalMerge::Start,
                                    VMerge::Continue => VerticalMerge::Continue,
                                },
                                empty: !c.blocks.iter().any(|b| match b {
                                    B::Para(p) => {
                                        !crate::docx::fields::text(p).trim().is_empty()
                                            || p.inlines.iter().any(|i| i.units() > 0 && !matches!(i, Inline::Text(..)))
                                    }
                                    B::Table(_) => true,
                                }),
                                blocks: if depth > 0 { blocks(&c.blocks, depth - 1) } else { Vec::new() },
                            })
                            .collect(),
                    })
                    .collect(),
            },
        })
        .collect()
}

/// The blocks that carry control `c` (its content, for a block control), wherever they are in the body.
fn carrying(list: &[crate::docx::Block], c: crate::docx::ControlId, out: &mut Vec<crate::docx::Block>) {
    use crate::docx::Block as B;
    for b in list {
        match b {
            B::Para(p) if p.controls.contains(&c) || p.inlines.iter().any(|i| matches!(i, crate::docx::Inline::ControlStart(x) if *x == c)) => {
                out.push(b.clone())
            }
            B::Table(t) if t.controls.contains(&c) => out.push(b.clone()),
            B::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|cell| {
                if cell.controls.contains(&c) {
                    out.extend(cell.blocks.iter().cloned());
                } else {
                    carrying(&cell.blocks, c, out);
                }
            }),
            _ => {}
        }
    }
}

impl Session {
    /// Every element, unfiltered, in document order.
    fn all_elements(&self) -> Vec<Element> {
        elements::enumerate(self.editor.document(), self.editor.index(), &elements::Filter::default())
    }

    /// The elements the filter matches, in document order.
    pub fn elements(&self, filter: &ElementFilter) -> Vec<ElementInfo> {
        self.all_elements()
            .iter()
            .map(|e| info(self, e))
            .filter(|e| {
                filter.kind.is_none_or(|k| k == e.kind)
                    && filter.story.is_none_or(|s| s == e.story)
                    && filter.sub_template.as_ref().is_none_or(|n| e.sub_template.as_ref() == Some(n))
                    && filter.matches.as_ref().is_none_or(|m| {
                        let v = match m.field {
                            ElementField::Code => e.code.as_deref(),
                            ElementField::Tag => e.tag.as_deref(),
                            ElementField::Title => e.title.as_deref(),
                        };
                        v.is_some_and(|v| v.contains(m.text.as_str()))
                    })
            })
            .collect()
    }

    /// The sub-templates: live building blocks in the rules' categories, in glossary order, each with its references
    /// and its body's elements. None without rules.
    pub fn sub_templates(&self) -> Vec<SubTemplateInfo> {
        let rules = self.editor.sub_template_rules();
        let Some(g) = &self.editor.document().glossary else { return Vec::new() };
        let all: Vec<ElementInfo> = self.all_elements().iter().map(|e| info(self, e)).collect();
        g.parts
            .iter()
            .filter(|b| b.state != crate::docx::PartState::Removed && rules.iter().any(|r| r.category == b.category))
            .map(|b| {
                let naming: Vec<&ElementInfo> = all.iter().filter(|e| e.reference.as_ref().is_some_and(|r| r.name == b.name)).collect();
                let nested = naming.iter().filter(|e| e.story == crate::api::value::Story::SubTemplate).count() as u32;
                SubTemplateInfo {
                    name: b.name.clone(),
                    category: b.category.clone(),
                    references: naming.len() as u32 - nested,
                    nested_references: nested,
                    elements: all.iter().filter(|e| e.sub_template.as_deref() == Some(b.name.as_str())).cloned().collect(),
                }
            })
            .collect()
    }

    /// A standalone DOCX of a sub-template's body, with the document's styles, numbering, theme and fonts, that
    /// `createSubTemplate`'s `from` reads back unchanged (`category`: when several rules name categories).
    pub fn export_sub_template(&self, name: &str, category: Option<&str>) -> Result<crate::api::value::Bytes, Error> {
        self.editor.export_sub_template(name, category).map(crate::api::value::Bytes)
    }

    /// An element's boxes on screen, in client px, one per line it spans: none while it is out of view.
    pub fn element_boxes(&mut self, id: &ElementId) -> Result<Vec<crate::api::services::ClientRect>, Error> {
        let e = elements::parse_id(&id.0).ok_or_else(|| Error::invalid("not an element id"))?;
        if self.editor.index().element(e).is_none() {
            return Err(Error::unavailable("no such element"));
        }
        Ok(self.element_boxes_on_screen(e))
    }

    /// One element with its settings and children.
    pub fn element(&self, id: ElementId) -> Result<ElementDetail, Error> {
        let all = self.all_elements();
        let want = elements::parse_id(&id.0).ok_or_else(|| Error::invalid("not an element id"))?;
        let e = all.iter().find(|e| e.id == want).ok_or_else(|| Error::unavailable("no such element"))?;
        let i = info(self, e);
        let p = e.props.as_ref();
        Ok(ElementDetail {
            id: i.id,
            kind: i.kind,
            code: i.code,
            tag: i.tag,
            title: i.title,
            display: i.display,
            story: i.story,
            sub_template: i.sub_template,
            parent: i.parent,
            locked: i.locked,
            lock: match p.map_or(Lock::None, |p| p.lock) {
                Lock::None => ControlLock::None,
                Lock::ControlLocked => ControlLock::Control,
                Lock::ContentLocked => ControlLock::Content,
                Lock::Both => ControlLock::Both,
            },
            placeholder: p.and_then(|p| p.placeholder.as_deref()).and_then(|x| attribute(x, "w:val").or_else(|| Some(x.to_string()))),
            binding: p.and_then(|p| p.binding.as_deref()).and_then(|x| {
                Some(DataBinding {
                    xpath: attribute(x, "w:xpath")?,
                    store_item_id: attribute(x, "w:storeItemID"),
                    prefix_mappings: attribute(x, "w:prefixMappings"),
                })
            }),
            options: options(self, e),
            children: all.iter().filter(|c| c.parent == Some(want)).map(|c| ElementId(elements::id_str(c.id))).collect(),
        })
    }

    /// The body (or an element's content) as blocks, `depth` levels of tables deep (all of them by default).
    pub fn structure(&self, root: Option<ElementId>, depth: Option<u32>) -> Vec<Block> {
        let depth = depth.unwrap_or(u32::MAX);
        let doc = self.editor.document();
        let body: Vec<&[crate::docx::Block]> = doc.sections.iter().map(|s| s.blocks.as_slice()).collect();
        match root.and_then(|r| elements::parse_id(&r.0)) {
            None => body.iter().flat_map(|b| blocks(b, depth)).collect(),
            Some(crate::docx::ElementId::Control(c)) => {
                let mut out = Vec::new();
                body.iter().for_each(|b| carrying(b, c, &mut out));
                blocks(&out, depth)
            }
            // A field is inline: its paragraph.
            Some(id) => self
                .all_elements()
                .iter()
                .find(|e| e.id == id)
                .and_then(|e| e.at)
                .and_then(|p| self.editor.index().para(doc, p))
                .map(|p| blocks(&[crate::docx::Block::Para(p.clone())], depth))
                .unwrap_or_default(),
        }
    }

    /// Selects an element, entering a header or footer when needed, and reveals it if asked.
    pub fn select_element(&mut self, id: ElementId, reveal: bool) -> Result<(), Error> {
        let internal = elements::parse_id(&id.0).ok_or_else(|| Error::invalid("not an element id"))?;
        if !self.all_elements().iter().any(|e| e.id == internal) {
            return Err(Error::unavailable("no such element"));
        }
        self.apply_with(Origin::Api, |e, fonts| e.select_element(&id, reveal, fonts)).map(|_| ())
    }

    /// The custom XML data parts with this namespace and item id (either may be left out).
    pub fn data_parts(&self, namespace: Option<&str>, item_id: Option<&str>) -> Vec<DataPart> {
        self.editor
            .document()
            .data_parts
            .iter()
            .filter(|d| namespace.is_none_or(|n| d.namespace.as_deref() == Some(n)) && item_id.is_none_or(|i| d.item_id.eq_ignore_ascii_case(i)))
            .map(|d| DataPart { namespace: d.namespace.clone(), item_id: d.item_id.clone(), xml: d.xml.clone() })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::api::engine::{Engine, SessionOptions};
    use crate::api::fonts::FontLibrary;
    use crate::api::services::{ElementFilter, ElementMatch};
    use crate::api::state::ElementKind;

    #[test]
    fn elements_list_filter_and_select() {
        let mut engine = Engine::new(FontLibrary::system_recommended());
        let id = engine.create_session(SessionOptions::default());
        let s = engine.session(id).expect("a session");
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-MERGE-FIELDS.docx");
        s.open(&std::fs::read(path).expect("fixture")).expect("opens");
        let all = s.elements(&ElementFilter::default());
        let fields = s.elements(&ElementFilter { kind: Some(ElementKind::Field), ..ElementFilter::default() });
        assert!(!fields.is_empty() && fields.len() <= all.len());
        let code = fields[0].code.clone().expect("a field has a code");
        let word: String = code.split_whitespace().last().unwrap_or("").to_string();
        let found = s.elements(&ElementFilter {
            matches: Some(ElementMatch { field: crate::api::command::ElementField::Code, text: word }),
            ..ElementFilter::default()
        });
        assert!(found.iter().any(|e| e.id == fields[0].id));
        let detail = s.element(fields[0].id.clone()).expect("one");
        assert_eq!(detail.code, fields[0].code);
        assert!(s.element(crate::api::value::ElementId("x1".into())).is_err());
        assert!(!s.structure(None, Some(0)).is_empty());
        s.select_element(fields[0].id.clone(), true).expect("selects");
        assert!(s.data_parts(Some("urn:none"), None).is_empty());
    }

    #[test]
    fn unfilled_picture_controls_list_until_filled() {
        let mut engine = Engine::new(FontLibrary::system_recommended());
        let id = engine.create_session(SessionOptions::default());
        let s = engine.session(id).expect("a session");
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-PLACEHOLDER.docx");
        s.open(&std::fs::read(path).expect("fixture")).expect("opens");
        let unfilled = |s: &crate::api::session::Session| -> Vec<crate::api::services::ElementInfo> {
            s.elements(&ElementFilter::default()).into_iter().filter(|e| e.size.is_some() && e.showing_placeholder == Some(true)).collect()
        };
        let all = unfilled(s);
        assert!(!all.is_empty(), "{all:?}");
        let obj = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-OBJ.docx")).expect("fixture");
        let png = crate::docx::read(&obj).expect("reads").images.last().cloned().expect("an image");
        let fill = crate::api::command::FillControl { id: all[0].id.clone(), text: None, image: Some(crate::api::value::Bytes(png)), docx: None };
        s.run(crate::api::command::Command::FillControl(fill)).expect("fills");
        assert!(unfilled(s).iter().all(|e| e.id != all[0].id), "filled ones go");
    }
}
