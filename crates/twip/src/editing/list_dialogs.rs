//! The list dialogs' values for the selection's list, and the commands they apply.

use crate::api::command::{Command, ListApplyTo, ListDefinitionChange, ListIndents, ListLevel, NumberFormat, NumberingMode, NumberingValue};
use crate::api::services::{
    AdjustListIndentsDialog, BulletsNumberingDialog, DefineBulletDialog, DefineMultilevelDialog, DefineNumberFormatDialog, DialogKind, DialogValues,
    ListEntryId, ListGalleryKind, SetNumberingValueDialog,
};
use crate::services::lists;

use super::{Editor, list_defs};
use crate::document::paragraph::{ListId, ListRef};
use crate::docx::list_xml::ListDefDirect;

impl Editor {
    /// The selection's list: its `numId`, the level, the nine levels, its name, and the paragraph's number.
    fn list_here(&mut self) -> Option<(i64, usize, Vec<ListLevel>, String, u32)> {
        let (para, num, ilvl) = self.selection_list()?;
        let levels = self.list_levels(num)?;
        let c = self.doc.definitions.as_ref()?.cascade();
        let name =
            c.abstract_of(ListId(num)).and_then(|a| c.lists.defs.get(&a)).and_then(|a| a.name.as_deref().map(str::to_string)).unwrap_or_default();
        let value = self.list_value(para).unwrap_or(1);
        Some((num, ilvl.clamp(0, 8) as usize, levels, name, value))
    }

    /// The number of a list paragraph, counted as the labels are.
    fn list_value(&self, para: crate::layout::ParaId) -> Option<u32> {
        let res = self.doc.definitions.as_ref()?;
        let cx = self.cx();
        let c = res.cascade();
        let mut counters = crate::format::counters::Counters::default();
        for (p, uncounted) in super::lists::numbered_paras(&self.doc, &self.index) {
            let Some((num, ilvl)) = super::lists::counted(res, p, uncounted, &cx) else { continue };
            let list = crate::document::paragraph::ListId(num);
            counters.next(&c, list, ilvl);
            if p.id == para.0 {
                return counters.value(&c, list, ilvl).map(|v| v.max(0) as u32);
            }
        }
        None
    }

    /// A list dialog's values for the selection (or, for Bullets and Numbering, a style's XML).
    pub fn list_dialog_values(&mut self, kind: DialogKind, style: Option<&str>) -> Option<DialogValues> {
        if kind == DialogKind::BulletsNumbering {
            let (para, _, _) = self.style_formatting(style?)?;
            let num = para.style.num.filter(|ListRef { list: ListId(n), level: _ }| *n != 0).map(|ListRef { list: ListId(n), level: _ }| n);
            let gallery = num.and_then(|n| self.list_levels(n)).map_or(ListGalleryKind::Bullet, |l| super::list_gallery::gallery_of(&l, false));
            return Some(DialogValues::BulletsNumbering(BulletsNumberingDialog {
                gallery,
                entry: num.map(|n| ListEntryId(format!("document:{n}"))),
            }));
        }
        let here = self.list_here();
        let level = here.as_ref().map(|(_, k, l, _, _)| l[*k].clone());
        Some(match kind {
            DialogKind::DefineBullet => {
                let l = level.filter(|l| l.format == NumberFormat::Bullet).unwrap_or_else(|| lists::levels(lists::BULLETS[0]).remove(0));
                DialogValues::DefineBullet(DefineBulletDialog { text: l.text, font: l.font, picture: l.picture, alignment: l.alignment })
            }
            DialogKind::DefineNumberFormat => {
                let l = level.filter(|l| l.format != NumberFormat::Bullet).unwrap_or_else(|| lists::levels(lists::NUMBERING[0]).remove(0));
                DialogValues::DefineNumberFormat(DefineNumberFormatDialog { format: l.format, text: l.text, font: l.font, alignment: l.alignment })
            }
            DialogKind::DefineMultilevelList => {
                let (levels, level, name) = match here {
                    Some((_, k, l, name, _)) => (l, k as u8, name),
                    None => (lists::levels(lists::MULTILEVEL[1]), 0, String::new()),
                };
                DialogValues::DefineMultilevelList(DefineMultilevelDialog {
                    levels,
                    level,
                    apply_to: ListApplyTo::WholeList,
                    gallery_level: 1,
                    list_name: name,
                })
            }
            DialogKind::SetNumberingValue => {
                let (_, _, _, _, value) = here?;
                DialogValues::SetNumberingValue(SetNumberingValueDialog { mode: NumberingMode::StartNew, value, advance: false })
            }
            DialogKind::AdjustListIndents => {
                let l = level?;
                DialogValues::AdjustListIndents(AdjustListIndentsDialog {
                    number_position: l.number_position,
                    text_indent: l.text_indent,
                    follow: l.follow,
                    tab: l.tab,
                })
            }
            _ => return None,
        })
    }

    /// The command a list dialog applies to the selection, or none when nothing changed.
    pub fn list_dialog_command(&mut self, values: &DialogValues) -> Option<Command> {
        let kind = match values {
            DialogValues::DefineBullet(_) => DialogKind::DefineBullet,
            DialogValues::DefineNumberFormat(_) => DialogKind::DefineNumberFormat,
            DialogValues::DefineMultilevelList(_) => DialogKind::DefineMultilevelList,
            DialogValues::SetNumberingValue(_) => DialogKind::SetNumberingValue,
            DialogValues::AdjustListIndents(_) => DialogKind::AdjustListIndents,
            _ => return None,
        };
        if self.list_dialog_values(kind, None).as_ref() == Some(values) {
            return None;
        }
        let here = self.list_here();
        // One level changed: the selection's, or a new list's first, after a library entry's levels.
        let one = |library: &[lists::L], set: &dyn Fn(&mut ListLevel)| -> Command {
            let (mut levels, k) = match &here {
                Some((_, k, l, _, _)) => (l.clone(), *k),
                None => (lists::levels(library), 0),
            };
            set(&mut levels[k]);
            if here.is_some() {
                levels.truncate(k + 1);
            }
            Command::SetListDefinition(ListDefinitionChange { levels, apply_to: ListApplyTo::WholeList, name: None })
        };
        Some(match values {
            DialogValues::DefineBullet(v) => one(lists::BULLETS[0], &|l| {
                (l.format, l.text, l.font, l.picture, l.alignment) =
                    (NumberFormat::Bullet, v.text.clone(), v.font.clone(), v.picture.clone(), v.alignment);
            }),
            DialogValues::DefineNumberFormat(v) => one(lists::NUMBERING[0], &|l| {
                (l.format, l.text, l.font, l.picture, l.alignment) = (v.format.clone(), v.text.clone(), v.font.clone(), None, v.alignment);
            }),
            DialogValues::DefineMultilevelList(v) => {
                Command::SetListDefinition(ListDefinitionChange { levels: v.levels.clone(), apply_to: v.apply_to, name: Some(v.list_name.clone()) })
            }
            DialogValues::SetNumberingValue(v) => Command::SetNumberingValue(NumberingValue { mode: v.mode, value: v.value, advance: v.advance }),
            DialogValues::AdjustListIndents(v) => Command::AdjustListIndents(ListIndents {
                number_position: v.number_position,
                text_indent: v.text_indent,
                follow: v.follow,
                tab: v.tab,
            }),
            _ => return None,
        })
    }

    /// The levels a Define dialog's preview shows, and whether it is a multilevel list.
    pub fn list_dialog_preview(&mut self, values: &DialogValues) -> Option<Vec<crate::docx::Block>> {
        let (levels, multi) = match values {
            DialogValues::DefineMultilevelList(v) => (v.levels.clone(), true),
            DialogValues::DefineBullet(_) | DialogValues::DefineNumberFormat(_) => match self.list_dialog_command(values) {
                Some(Command::SetListDefinition(d)) => {
                    let k = d.levels.len() - 1;
                    (vec![d.levels[k].clone()], false)
                }
                _ => {
                    let here = self.list_here()?;
                    (vec![here.2[here.1].clone()], false)
                }
            },
            _ => return None,
        };
        self.list_preview(&levels, multi)
    }

    /// Makes a list of `levels` (one undo step), for a style to number with; its `numId`.
    pub fn add_list(&mut self, levels: &[ListLevel], fonts: &crate::font::Fonts) -> (super::commands::Outcome, i64) {
        let Some(res) = self.doc.definitions.clone() else { return (super::commands::Outcome::Unchanged("no styles"), 0) };
        let (abstract_id, num_id) = res.cascade().lists.free_ids();
        let (abstract_id, num_id) = (abstract_id.0, num_id.0);
        let levels = list_defs::complete(levels);
        let out = self.edit_numbering(
            "Define List",
            |n, _, colours| {
                n.set_list_def(list_defs::definition(abstract_id, None, list_defs::Levels::Shown(&levels), None, &[], &[], colours));
                n.set_list(list_defs::new_list(num_id, abstract_id, &[]));
                Some(())
            },
            &[],
            false,
            fonts,
        );
        (out, num_id)
    }

    /// A list style's own list: its definition (linked to the style) set to `levels`, made with a list the style
    /// numbers with when it has none.
    pub(super) fn define_style_list(&mut self, id: &str, levels: &[ListLevel], fonts: &crate::font::Fonts) -> super::commands::Outcome {
        use super::commands::Outcome;
        let Some(res) = self.doc.definitions.clone() else { return Outcome::Unchanged("no styles") };
        let typed = res.typed();
        let c = &*typed.cascade;
        let num = crate::docx::para_props::num_id(c.chain_para(Some(id)).list).filter(|n| *n != 0 && c.lists.lists.contains_key(&ListId(*n)));
        let levels = list_defs::complete(levels);
        match num.and_then(|n| c.abstract_of(ListId(n))) {
            Some(abstract_id) => {
                let old = list_defs::def_holder(&typed, abstract_id);
                self.edit_numbering(
                    "Modify Style",
                    |n, _, colours| {
                        n.set_list_def(list_defs::definition(
                            abstract_id.0,
                            old.as_ref(),
                            list_defs::Levels::Shown(&levels),
                            None,
                            &[],
                            &[],
                            colours,
                        ));
                        Some(())
                    },
                    &[],
                    false,
                    fonts,
                )
            }
            None => {
                let (abstract_id, num_id) = c.lists.free_ids();
                let (abstract_id, num_id) = (abstract_id.0, num_id.0);
                let out = self.edit_numbering(
                    "New Style",
                    |n, _, colours| {
                        n.set_list_def(style_definition(abstract_id, &levels, id, colours));
                        n.set_list(list_defs::new_list(num_id, abstract_id, &[]));
                        Some(())
                    },
                    &[],
                    false,
                    fonts,
                );
                if matches!(out, Outcome::Refused(_) | Outcome::Unchanged(_)) {
                    return out;
                }
                let typed = self.doc.definitions.as_ref().map(|r| r.typed());
                let Some(style) = typed.as_ref().and_then(|t| list_defs::style_holder(t, id)) else { return out };
                self.edit_styles("New Style", |s| s.set(list_defs::style_numbered(&style, Some((num_id, 0)))), fonts)
            }
        }
    }

    /// The levels and continued list of a gallery entry, for Bullets and Numbering.
    pub fn list_entry_definition(&self, id: &ListEntryId) -> Option<(Vec<ListLevel>, Option<i64>)> {
        self.list_entry(id).map(|(_, l, n)| (l, n))
    }
}

/// A list style's new definition `abstract_id` of `levels`: the style `style`'s own.
pub(super) fn style_definition(abstract_id: i64, levels: &[ListLevel], style: &str, colours: &crate::docx::props::ThemeColours) -> ListDefDirect {
    let d = list_defs::definition(abstract_id, None, list_defs::Levels::Shown(levels), None, &[], &[], colours);
    let mut v = (**d.get()).clone();
    v.style_link = Some(style.into());
    ListDefDirect::typed(std::rc::Rc::new(v))
}
