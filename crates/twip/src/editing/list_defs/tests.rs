//! The typed list edits against the XML patches they replaced: over every fixture's numbering and styles parts and
//! representative edits, each element a typed edit's holder writes is the one the patch wrote.

use std::rc::Rc;

use crate::api::command::{FollowNumber, LevelAlignment, ListLevel, NumberFormat};
use crate::api::value::{AutoOr, Color, HighlightColor, NoneOr, Points, StyleId, UnderlineStyle};
use crate::document::paragraph::ListId;
use crate::docx::char_props::RunDirect;
use crate::docx::props::ThemeColours;
use crate::docx::resolvers::TypedDefs;
use crate::docx::write::props::Cx;
use crate::format::{CharEdit, CharProps, ListDefId, MultiLevelType, Prop, Toggle};

use super::super::{list_dialogs, list_labels, lists};
use super::{LevelDefinition, Levels, PicChange, complete, definition, level_definition, new_list, overridden, style_numbered, xml};

/// Each fixture's typed definitions, as the editor reads them.
fn documents() -> Vec<(String, Rc<TypedDefs>)> {
    let mut paths = Vec::new();
    crate::layout::tests::output::fixtures(&crate::layout::tests::output::root().join("fixtures"), &mut paths);
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let d = crate::docx::read_editable(&std::fs::read(&path).expect("reads")).ok()?;
            Some((path.display().to_string(), d.resolver.as_ref()?.typed()))
        })
        .collect()
}

/// Where the typed edits write otherwise than the patches did, each a writing Word reads alike: the same elements and
/// attributes, by namespace and name, with the same values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unlike {
    /// A new element is written in the part's names: the patch bound the main namespace on the element itself.
    OwnBinding,
    /// A level copied into a new override of a list, or a list's level into a copy of its definition, is written
    /// from its value: the patch copied it as written, its children's and attributes' order and the spaces in its tags
    /// as they were; the typed edit writes its children in schema order.
    Copied,
    /// A level's tab stops, when its own moves or goes, are written again from their value: the patch kept the others
    /// as written, spaces in their tags too. Its indent keeps its attributes in the order read: the patch wrote the
    /// start indent first.
    Rewritten,
}

/// The elements the two routes wrote apart: what, the patch's, the typed edit's.
#[derive(Default)]
struct Compared {
    checked: usize,
    unlike: Vec<Unlike>,
    apart: Vec<(String, String, String)>,
}

impl Compared {
    /// The two writings of an element: the same bytes.
    fn check(&mut self, what: impl FnOnce() -> String, patched: &str, typed: &str) {
        self.checked += 1;
        if patched != typed {
            self.apart.push((what(), patched.to_string(), typed.to_string()));
        }
    }

    /// The two writings of an element in the part `cx` describes: the same bytes, or, where `unlike` says they may
    /// differ, the same elements and attributes.
    fn check_as(&mut self, unlike: Unlike, cx: &Cx, what: impl FnOnce() -> String, patched: &str, typed: &str) {
        if patched != typed && same_xml(patched, typed, unlike, cx) {
            self.checked += 1;
            self.unlike.push(unlike);
            return;
        }
        self.check(what, patched, typed);
    }

    /// Every element alike, at least `min` compared, and the differences each kind of `unlike` allows found.
    fn alike(self, min: usize, unlike: &[Unlike]) {
        let shown: Vec<String> = self.apart.iter().take(12).map(|(what, a, b)| format!("{what}\n  patch: {a}\n  typed: {b}")).collect();
        assert!(self.apart.is_empty(), "{} of {} apart:\n{}", self.apart.len(), self.checked, shown.join("\n"));
        assert!(self.checked >= min, "only {} compared", self.checked);
        for u in unlike {
            assert!(self.unlike.contains(u), "no {u:?} found: the writings are alike there now");
        }
    }
}

/// Whether two elements in the part `cx` describes hold the same elements and attributes (by namespace and name),
/// values and text: elements in the same order but where `unlike` lets their order differ.
fn same_xml(a: &str, b: &str, unlike: Unlike, cx: &Cx) -> bool {
    fn canon(n: roxmltree::Node, ordered: bool) -> String {
        if n.is_text() {
            return n.text().unwrap_or_default().trim().to_string();
        }
        let mut attrs: Vec<String> =
            n.attributes().map(|a| format!("{{{}}}{}={:?}", a.namespace().unwrap_or_default(), a.name(), a.value())).collect();
        attrs.sort();
        let mut kids: Vec<String> =
            n.children().filter(|c| c.is_element() || c.text().is_some_and(|t| !t.trim().is_empty())).map(|c| canon(c, ordered)).collect();
        if !ordered {
            kids.sort();
        }
        let name = n.tag_name();
        format!("<{{{}}}{} {}>{}</>", name.namespace().unwrap_or_default(), name.name(), attrs.join(" "), kids.concat())
    }
    let ordered = unlike != Unlike::Copied;
    cx.parse(a, |_, x| cx.parse(b, |_, y| canon(x, ordered) == canon(y, ordered))).flatten().unwrap_or(false)
}

/// A level as the definition holds it, in full; a missing one as the dialogs show it.
fn view(d: &crate::format::ListDef, colours: &ThemeColours) -> Vec<LevelDefinition> {
    d.levels
        .iter()
        .map(|l| match l {
            Some(l) => level_definition(l, colours),
            None => LevelDefinition { level: ListLevel { start: 1, ..ListLevel::default() }, ..Default::default() },
        })
        .collect()
}

/// A change to one level.
type Change = (&'static str, fn(&mut LevelDefinition));

/// One of each change a Define dialog makes to a level.
const CHANGES: &[Change] = &[
    ("start", |d| d.level.start += 1),
    ("format", |d| {
        d.format = if d.format == NumberFormat::Bullet { NumberFormat::UpperRoman } else { NumberFormat::Bullet };
        d.level.format = d.format.clone();
    }),
    ("unnamed format", |d| d.format = NumberFormat::Other("decimalEnclosedCircle".into())),
    ("text", |d| d.level.text.push('x')),
    ("alignment", |d| {
        d.level.alignment = match d.level.alignment {
            LevelAlignment::Left => LevelAlignment::Center,
            LevelAlignment::Center => LevelAlignment::Right,
            LevelAlignment::Right => LevelAlignment::Left,
        }
    }),
    ("follow", |d| {
        d.level.follow = match d.level.follow {
            FollowNumber::Tab => FollowNumber::Space,
            FollowNumber::Space => FollowNumber::Nothing,
            FollowNumber::Nothing => FollowNumber::Tab,
        }
    }),
    ("tab moved", |d| d.level.tab = Some(Points(d.level.tab.map_or(d.level.text_indent.0, |t| t.0) + 18.0))),
    ("tab removed", |d| d.level.tab = None),
    ("number position", |d| d.level.number_position.0 += 9.0),
    ("text indent", |d| d.level.text_indent.0 += 18.25),
    ("first line further in", |d| d.level.number_position = Points(d.level.text_indent.0 + 18.0)),
    ("no hanging", |d| d.level.number_position = d.level.text_indent),
    ("restart", |d| d.level.restart_after = if d.level.restart_after.is_some() { None } else { Some(0) }),
    ("legal", |d| d.level.legal = !d.level.legal),
    ("linked style", |d| d.level.linked_style = if d.level.linked_style.is_some() { None } else { Some(StyleId("Heading2".into())) }),
    ("font", |d| d.level.font = if d.level.font.is_some() { None } else { Some("Arial & Co".into()) }),
    ("font changed", |d| d.level.font = Some("Times New Roman".into())),
    ("bold", |d| {
        d.label.bold = match d.label.bold {
            None => Some(true),
            Some(true) => Some(false),
            Some(false) => None,
        }
    }),
    ("italic", |d| d.label.italic = if d.label.italic.is_some() { None } else { Some(true) }),
    ("underline", |d| d.label.underline = if d.label.underline.is_some() { None } else { Some(UnderlineStyle::Double) }),
    ("size", |d| d.label.size = Some(Points(d.label.size.map_or(14.0, |s| s.0 + 1.5)))),
    ("size removed", |d| d.label.size = None),
    ("colour", |d| {
        d.label.color = match d.label.color {
            None => Some(AutoOr::Auto),
            Some(AutoOr::Auto) => Some(AutoOr::Value(Color(0x12_34AB))),
            Some(AutoOr::Value(_)) => None,
        }
    }),
    ("highlight", |d| {
        d.label.highlight = match d.label.highlight {
            None => Some(NoneOr::Value(HighlightColor::Yellow)),
            Some(NoneOr::Value(_)) => Some(NoneOr::None),
            Some(NoneOr::None) => None,
        }
    }),
];

/// Each definition of every fixture, edited both ways: as read, with every level changed at once, named, unnamed,
/// with picture bullets set and removed.
#[test]
fn definition_edits_write_what_the_patches_wrote() {
    definitions(Part::Whole).alike(1_000, &[]);
}

/// Each definition of every fixture with each change at levels 0, 1, 4 and 8, the levels after it left out (as
/// Adjust List Indents leaves them).
#[test]
fn level_edits_write_what_the_patches_wrote() {
    definitions(Part::Levels).alike(5_000, &[]);
}

/// Each definition of every fixture copied, as read and with each change at every level.
#[test]
fn copied_definitions_write_what_the_patches_wrote() {
    definitions(Part::Copies).alike(1_000, &[]);
}

/// Each list's own levels with each change, and a copy of its definition that takes them.
#[test]
fn list_edits_write_what_the_patches_wrote() {
    definitions(Part::Lists).alike(150, &[Unlike::Copied, Unlike::Rewritten]);
}

/// Which of the definition edits are compared.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Whole,
    Levels,
    Copies,
    Lists,
}

/// The definition edits of `part` compared.
fn definitions(part: Part) -> Compared {
    let mut n = Compared::default();
    for (path, t) in documents() {
        let (colours, cx) = (t.cascade.colours, t.numbering.cx());
        let mut ids: Vec<&ListDefId> = t.numbering.defs.keys().collect();
        ids.sort();
        let copy_id = ids.last().map_or(0, |i| i.0 + 1);
        for &&id in ids.iter().filter(|_| part != Part::Lists) {
            let holder = &t.numbering.defs[&id];
            let raw = holder.xml(&colours, &cx).into_owned();
            let levels = view(holder.get(), &colours);
            let mut edit = |what: &str, levels: &[LevelDefinition], name: Option<&str>, pics: &[PicChange], to: i64| {
                let patched = xml::abstract_xml(to, Some(&raw), Levels::Full(levels), name, pics, &[], &cx, &colours);
                let typed = definition(to, Some(holder), Levels::Full(levels), name, pics, &[], &colours);
                n.check(|| format!("{path}: definition {}: {what}", id.0), &patched, &typed.xml(&colours, &cx));
            };
            match part {
                Part::Levels => {
                    for (what, change) in CHANGES {
                        for k in [0, 1, 4, 8].into_iter().filter(|k| holder.get().levels[*k].is_some()) {
                            let mut one = levels[..=k].to_vec();
                            change(&mut one[k]);
                            edit(&format!("{what} at level {k}"), &one, None, &[], id.0);
                        }
                    }
                }
                Part::Copies => {
                    edit("copied", &levels[..1], None, &[], copy_id);
                    for (what, change) in CHANGES {
                        let mut all = levels.clone();
                        all.iter_mut().for_each(change);
                        edit(&format!("{what} at every level, copied"), &all, None, &[], copy_id);
                    }
                }
                Part::Whole | Part::Lists => {
                    edit("as read", &levels, None, &[], id.0);
                    edit("named", &levels, Some("Steps & <more>"), &[], id.0);
                    edit("unnamed", &levels, Some(""), &[], id.0);
                    for (what, change) in CHANGES {
                        let mut all = levels.clone();
                        all.iter_mut().for_each(change);
                        edit(&format!("{what} at every level"), &all, None, &[], id.0);
                    }
                    let pics: Vec<PicChange> = holder
                        .get()
                        .levels
                        .iter()
                        .map(|l| Some(if l.as_ref().is_some_and(|l| l.pic_bullet.is_some()) { None } else { Some(42) }))
                        .collect();
                    edit("picture bullets", &levels, None, &pics, id.0);
                }
            }
        }
        if part == Part::Lists {
            list_edits(&mut n, &path, &t, copy_id);
        }
    }
    if part == Part::Lists {
        let blank = crate::docx::read_editable(&crate::docx::blank::blank_docx()).expect("reads");
        let mut r = (**blank.resolver.as_ref().expect("a resolver")).clone();
        r.set_numbering_part(OVERRIDES);
        list_edits(&mut n, "lists overriding their levels", &r.typed(), 9);
    }
    n
}

/// Lists that define levels of their own, as Word and other writers write them.
const OVERRIDES: &str = concat!(
    r#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:x="urn:x">"#,
    r#"<w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="hybridMultilevel"/>"#,
    r#"<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl>"#,
    r#"<w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="lowerLetter"/><w:lvlText w:val="%2."/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="1440" w:hanging="360"/></w:pPr></w:lvl>"#,
    r#"</w:abstractNum>"#,
    r#"<w:num w:numId="1"><w:abstractNumId w:val="0"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="3"/><w:lvl w:ilvl="0"><w:start w:val="3"/><w:numFmt w:val="upperRoman"/><w:lvlText w:val="%1)"/><w:lvlJc w:val="right"/><w:pPr><w:tabs><w:tab w:val="num" w:pos="900"/></w:tabs><w:ind w:left="900" w:hanging="540"/></w:pPr><w:rPr><w:b/><w:sz w:val="28"/></w:rPr></w:lvl></w:lvlOverride></w:num>"#,
    r#"<w:num w:numId="2"><w:abstractNumId w:val="0" /><w:lvlOverride w:ilvl="1"><w:lvl w:ilvl="1" w:tplc="04090019"><w:start w:val="2" /><w:numFmt w:val="bullet" /><w:lvlText w:val="o" /><w:lvlJc w:val="left" /><w:pPr><w:tabs><w:tab w:val="left" w:leader="dot" w:pos="2000" /><w:tab w:val="clear" w:pos="1440" /></w:tabs><w:ind w:hanging="360" w:left="1440" w:right="72" /></w:pPr><w:rPr><w:rFonts w:hint="default" w:ascii="Courier New" w:hAnsi="Courier New" /><w:color w:val="FF0000" /></w:rPr></w:lvl></w:lvlOverride></w:num>"#,
    r#"<w:num w:numId="3"><w:abstractNumId w:val="0"/><w:lvlOverride w:ilvl="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:pStyle w:val="Heading1"/><w:isLgl/><w:suff w:val="space"/><w:lvlText w:val="%1."/><x:own w:val="1"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="0" w:firstLine="0"/></w:pPr></w:lvl></w:lvlOverride>"#,
    r#"<w:lvlOverride w:ilvl="2"><w:startOverride w:val="5"/></w:lvlOverride></w:num>"#,
    r#"</w:numbering>"#
);

/// Each list of `t` that defines levels of its own, with each change, and a copy of its definition that takes them,
/// under the free id `copy_id`.
fn list_edits(n: &mut Compared, path: &str, t: &TypedDefs, copy_id: i64) {
    let (colours, cx) = (t.cascade.colours, t.numbering.cx());
    let mut nums: Vec<&ListId> = t.numbering.lists.keys().collect();
    nums.sort();
    for &&num in &nums {
        let held = &t.numbering.lists[&num];
        let list = held.get();
        if list.overrides.iter().all(|o| o.def.is_none()) {
            continue;
        }
        let raw = held.xml(&colours, &cx).into_owned();
        let xml_over = xml::override_levels(&raw, &cx);
        let over: Vec<super::Override> =
            (0..9).map(|k| list.override_of(k)?.def.clone().map(|l| (l.clone(), level_definition(&l, &colours)))).collect();
        let old_over: Vec<xml::Override> = over.iter().zip(xml_over).map(|(o, x)| Some((x?, o.as_ref()?.1.clone()))).collect();
        let shown: Vec<LevelDefinition> = (0..9)
            .map(|k| match (&over[k], t.cascade.level(num, k as i64)) {
                (Some((_, v)), _) => v.clone(),
                (None, Some(l)) => level_definition(l, &colours),
                (None, None) => LevelDefinition::default(),
            })
            .collect();
        for (what, change) in CHANGES {
            let mut all = shown.clone();
            all.iter_mut().for_each(change);
            let patched = xml::num_xml(&raw, Levels::Full(&all), &[], &old_over, &cx);
            let typed = overridden(held, Levels::Full(&all), &[], &over).map(|m| m.xml(&colours, &cx).into_owned());
            let what_list = || format!("{path}: list {}: {what}", num.0);
            match (patched, typed) {
                (Some(a), Some(b)) => n.check_as(Unlike::Rewritten, &cx, what_list, &a, &b),
                (a, b) => n.check(what_list, &format!("{a:?}"), &format!("{b:?}")),
            }
            let Some(abstract_id) = t.cascade.abstract_of(num) else { continue };
            let Some(def) = t.numbering.defs.get(&abstract_id) else { continue };
            let def_raw = def.xml(&colours, &cx).into_owned();
            let patched = xml::abstract_xml(copy_id, Some(&def_raw), Levels::Full(&all[..1]), None, &[], &old_over, &cx, &colours);
            let typed = definition(copy_id, Some(def), Levels::Full(&all[..1]), None, &[], &over, &colours);
            let what = || format!("{path}: list {}: {what}, its definition copied", num.0);
            n.check_as(Unlike::Copied, &cx, what, &patched, &typed.xml(&colours, &cx));
        }
    }
}

/// New definitions of each fixture list's levels as the dialogs show them, new lists, the lists the toolbar, the
/// List Bullet style and list styles add, and picture bullets: written alike.
#[test]
fn new_lists_write_what_the_patches_wrote() {
    let mut n = Compared::default();
    let mut shown_all: Vec<(String, Rc<TypedDefs>, Vec<ListLevel>)> = Vec::new();
    for (path, t) in documents() {
        let colours = t.cascade.colours;
        let mut nums: Vec<&ListId> = t.cascade.lists.lists.keys().collect();
        nums.sort();
        for &&num in nums.iter().take(8) {
            let Some(levels) = t.cascade.levels(num) else { continue };
            let shown: Vec<ListLevel> = levels.iter().flatten().map(|l| level_definition(l, &colours).level).collect();
            shown_all.push((format!("{path}: list {}", num.0), t.clone(), shown));
        }
    }
    for (what, t, shown) in &shown_all {
        let (colours, cx) = (t.cascade.colours, t.numbering.cx());
        for k in [1, 3, 9] {
            let levels = complete(&shown[..k.min(shown.len())]);
            let patched = xml::abstract_xml(5, None, Levels::Shown(&levels), None, &[], &[], &cx, &colours);
            let typed = definition(5, None, Levels::Shown(&levels), None, &[], &[], &colours);
            n.check(|| format!("{what}: {k} levels, new"), &patched, &typed.xml(&colours, &cx));
            let full: Vec<LevelDefinition> = levels.iter().map(LevelDefinition::of_level).collect();
            let patched = xml::abstract_xml(5, None, Levels::Full(&full), Some("New"), &[], &[], &cx, &colours);
            let typed = definition(5, None, Levels::Full(&full), Some("New"), &[], &[], &colours);
            n.check(|| format!("{what}: {k} levels, new and named"), &patched, &typed.xml(&colours, &cx));
            let a = xml::abstract_xml(5, None, Levels::Shown(&levels), None, &[], &[], &cx, &colours);
            let patched = xml::style_linked(a, "Outline & List", &cx);
            let typed = list_dialogs::style_definition(5, &levels, "Outline & List", &colours);
            n.check(|| format!("{what}: {k} levels, a list style's"), &patched, &typed.xml(&colours, &cx));
        }
    }
    let Some((_, t, _)) = shown_all.first() else { panic!("no lists") };
    let (colours, cx) = (t.cascade.colours, t.numbering.cx());
    for starts in [&[][..], &[(0, 4)], &[(0, 2), (3, 7)], &[(12, 1)]] {
        let patched = xml::num_new(9, 4, starts, &cx);
        n.check(|| format!("list starting {starts:?}"), &patched, &new_list(9, 4, starts).xml(&colours, &cx));
    }
    for bullet in [false, true] {
        let (patched, num) = xml::added_list(3, 4, "hybridMultilevel", &xml::new_list_levels(bullet));
        let typed = lists::added_definition(3, MultiLevelType::HybridMultilevel, lists::new_list_levels(bullet));
        n.check_as(Unlike::OwnBinding, &cx, || format!("toolbar list, bullets {bullet}"), &patched, &typed.xml(&colours, &cx));
        n.check_as(Unlike::OwnBinding, &cx, || format!("toolbar list, bullets {bullet}: its list"), &num, &new_list(4, 3, &[]).xml(&colours, &cx));
    }
    let (patched, _) = xml::added_list(3, 4, "singleLevel", &xml::style_bullets_level("List & Bullet"));
    let typed = lists::added_definition(3, MultiLevelType::SingleLevel, lists::style_bullets_levels("List & Bullet"));
    n.check_as(Unlike::OwnBinding, &cx, || "List Bullet's list".to_string(), &patched, &typed.xml(&colours, &cx));
    for id in [0, 7] {
        let rel = format!("{}{id}", crate::docx::write::NEW_IMAGE);
        n.check(|| format!("picture bullet {id}"), &xml::picture(id, &rel, &cx), &crate::docx::list_xml::new_picture_xml(id, &rel, &cx));
    }
    n.alike(500, &[Unlike::OwnBinding]);
}

/// Every style of every fixture linked to a list's level, at its first and another, and unlinked; every definition
/// and list whose levels link to a style, unlinked from it but at one level.
#[test]
fn style_links_write_what_the_patches_wrote() {
    let mut n = Compared::default();
    for (path, t) in documents() {
        let colours = t.cascade.colours;
        let scx = t.styles.cx();
        let mut ids: Vec<&Rc<str>> = t.styles.styles.keys().collect();
        ids.sort();
        for id in ids {
            let held = &t.styles.styles[id];
            let raw = held.xml(&colours, &scx).into_owned();
            for to in [Some((7, 0)), Some((7, 3)), None] {
                let patched = xml::style_numbered(&raw, to, &scx);
                n.check(|| format!("{path}: style {id}: numbered {to:?}"), &patched, &style_numbered(held, to).xml(&colours, &scx));
            }
        }
        let cx = t.numbering.cx();
        for (id, held) in &t.numbering.defs {
            for (k, style) in held.get().levels.iter().enumerate().filter_map(|(k, l)| Some((k, l.as_ref()?.style.clone()?))) {
                let raw = held.xml(&colours, &cx).into_owned();
                for keep in [None, Some(k), Some((k + 1) % 9)] {
                    let patched = xml::unlinked(&raw, &style, keep, &cx);
                    let typed = super::unlinked(held, &style, keep);
                    n.check(|| format!("{path}: definition {}: {style} unlinked but {keep:?}", id.0), &patched, &typed.xml(&colours, &cx));
                }
            }
        }
        for (id, held) in &t.numbering.lists {
            for o in &held.get().overrides {
                let Some(style) = o.def.as_ref().and_then(|l| l.style.clone()) else { continue };
                let raw = held.xml(&colours, &cx).into_owned();
                for keep in [None, Some(o.level)] {
                    let patched = xml::unlinked_num(&raw, &style, keep.map(|k| k as usize), &cx);
                    let typed = super::unlinked_list(held, &style, keep);
                    n.check(|| format!("{path}: list {}: {style} unlinked but {keep:?}", id.0), &patched, &typed.xml(&colours, &cx));
                }
            }
        }
    }
    n.alike(1_000, &[]);
}

/// A label formatting edit's commands, as the formatting commands edit the label's properties.
type Commands = (&'static str, fn(&mut CharEdit));

const LABEL_EDITS: &[Commands] = &[
    ("bold", |e| e.toggle(Toggle::BOLD, true)),
    ("not italic", |e| e.toggle(Toggle::ITALIC, false)),
    ("size", |e| {
        (e.size, e.complex_size) = (Some(15.5), Some(15.5));
        e.whole(Prop::Size);
        e.whole(Prop::ComplexSize);
    }),
    ("colour", |e| {
        e.color = Some(crate::format::Color::Rgb(0xC0_FFEE));
        e.whole(Prop::Color);
    }),
    ("font", |e| {
        let f = Some(crate::format::FontRef::Family("Wingdings".into()));
        (e.fonts.latin, e.fonts.latin_extended) = (f.clone(), f);
        e.whole(Prop::Font(crate::format::ScriptClass::Latin));
        e.whole(Prop::Font(crate::format::ScriptClass::LatinExtended));
    }),
    ("cleared", |e| {
        e.props = CharProps::default();
        e.whole(Prop::Size);
        e.whole(Prop::Toggle(Toggle::BOLD));
    }),
];

/// The `w:rPr` `xml` with `edit` written over it, as the label's formatting chained the commands' edits.
fn rpr_edited(xml: Option<&str>, colours: &ThemeColours, cx: &Cx, edit: fn(&mut CharEdit)) -> Option<String> {
    let direct = RunDirect::from_xml(xml, cx);
    let mut e = CharEdit::new(direct.props().map_or_else(CharProps::default, |q| (**q).clone()));
    edit(&mut e);
    direct.edit(e).xml(colours, cx).map(std::borrow::Cow::into_owned)
}

/// Every level of every fixture's definitions and lists formatted by each of the label edits, and by all of them in
/// turn: in the definition, in the list's override of it, and in an override a list gets for it.
#[test]
fn label_formatting_writes_what_the_patches_wrote() {
    let mut n = Compared::default();
    for (path, t) in documents() {
        let (colours, cx) = (t.cascade.colours, t.numbering.cx());
        let mut nums: Vec<&ListId> = t.numbering.lists.keys().collect();
        nums.sort();
        for &&num in &nums {
            let list = &t.numbering.lists[&num];
            let Some(def) = t.cascade.abstract_of(num).and_then(|a| t.numbering.defs.get(&a)) else { continue };
            let (num_raw, def_raw) = (list.xml(&colours, &cx).into_owned(), def.xml(&colours, &cx).into_owned());
            for level in 0..9i64 {
                let own = list.get().override_of(level).and_then(|o| o.def.clone());
                let Some(lvl) = own.clone().or_else(|| def.get().levels[level as usize].clone()) else { continue };
                let Some(lvl_raw) = xml::level_in(if own.is_some() { &num_raw } else { &def_raw }, level, own.is_some(), &cx) else { continue };
                let mut edits: Vec<(&str, Vec<fn(&mut CharEdit)>)> = LABEL_EDITS.iter().map(|(w, e)| (*w, vec![*e])).collect();
                edits.push(("each in turn", LABEL_EDITS.iter().map(|(_, e)| *e).collect()));
                for (what, edits) in edits {
                    let mut rpr = xml::rpr_of(&lvl_raw, &cx);
                    let mut direct = RunDirect::typed((lvl.char != CharProps::default()).then(|| Rc::new(lvl.char.clone())));
                    let mut set = Vec::new();
                    for edit in edits {
                        rpr = rpr_edited(rpr.as_deref(), &colours, &cx, edit);
                        let mut e = CharEdit::new(direct.props().map_or_else(CharProps::default, |p| (**p).clone()));
                        edit(&mut e);
                        set.extend_from_slice(&e.set);
                        direct = direct.edit(e);
                    }
                    let Some(patched_lvl) = xml::with_rpr(&lvl_raw, rpr, &cx) else { continue };
                    let label = direct.props().map_or_else(CharProps::default, |p| (**p).clone());
                    for write_own in [false, true].into_iter().filter(|w| *w || own.is_none()) {
                        let patched = xml::with_level(if write_own { &num_raw } else { &def_raw }, level, write_own, &patched_lvl, &cx);
                        let typed = match list_labels::labelled(&lvl, label.clone(), &set, level, write_own.then_some(list).ok_or(def)) {
                            list_labels::Edited::Def(d) => d.xml(&colours, &cx).into_owned(),
                            list_labels::Edited::List(m) => m.xml(&colours, &cx).into_owned(),
                        };
                        let what = || format!("{path}: list {} level {level}: {what}, in its own {write_own}", num.0);
                        match write_own && own.is_none() {
                            true => n.check_as(Unlike::Copied, &cx, what, &patched, &typed),
                            false => n.check(what, &patched, &typed),
                        }
                    }
                }
            }
        }
    }
    n.alike(1_000, &[Unlike::Copied]);
}
