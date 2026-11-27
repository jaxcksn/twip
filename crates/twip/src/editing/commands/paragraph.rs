//! Paragraph formatting

use crate::api::command::{BorderPreset, Indents, ListKind, Outline, ParagraphFlow, ParagraphSpacing, TextFlow};
use crate::api::value::{Alignment, AutoOr, LineSpacing, StyleId};
use crate::font::Fonts;
use crate::layout::ParaId;

use super::Outcome;
use crate::document::format::{Jc, LineRule};
use crate::document::paragraph::{ListId, ListRef};
use crate::editing::Editor;
use crate::editing::format::{Format, twip};
use crate::format::{ParaFlag, ParaProp, TightWrap};

impl Editor {
    pub(in crate::editing) fn set_alignment(&mut self, a: Alignment, fonts: &Fonts) -> Outcome {
        let jc = match a {
            Alignment::Left => Jc::Left,
            Alignment::Center => Jc::Center,
            Alignment::Right => Jc::Right,
            Alignment::Justify => Jc::Both,
        };
        self.format_paras_typed(
            "Paragraph Formatting",
            &|e, _| {
                e.align = Some(jc);
                e.whole(ParaProp::Align);
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_line_spacing(&mut self, s: LineSpacing, fonts: &Fonts) -> Outcome {
        let (line, rule) = match s {
            LineSpacing::Multiple { lines } => ((lines * 240.0).round(), LineRule::Auto),
            LineSpacing::Exact { size } => ((size.0 * 20.0).round(), LineRule::Exact),
            LineSpacing::AtLeast { size } => ((size.0 * 20.0).round(), LineRule::AtLeast),
        };
        self.format_paras_typed(
            "Line Spacing",
            &|e, _| {
                (e.line, e.line_rule) = (Some(line), Some(rule));
                e.part(ParaProp::Line);
                e.part(ParaProp::LineRule);
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_paragraph_spacing(&mut self, s: ParagraphSpacing, fonts: &Fonts) -> Outcome {
        self.format_paras_typed(
            "Paragraph Spacing",
            &|e, _| {
                for (v, before) in [(s.before, true), (s.after, false)] {
                    let (space, auto) =
                        if before { (ParaProp::SpaceBefore, ParaProp::SpaceBeforeAuto) } else { (ParaProp::SpaceAfter, ParaProp::SpaceAfterAuto) };
                    match v {
                        Some(AutoOr::Auto) => {
                            *if before { &mut e.space_before_auto } else { &mut e.space_after_auto } = Some(true);
                            e.part(auto);
                        }
                        Some(AutoOr::Value(pt)) => {
                            *if before { &mut e.space_before } else { &mut e.space_after } = Some(twip(pt.0));
                            *if before { &mut e.space_before_auto } else { &mut e.space_after_auto } = None;
                            e.part(space);
                            e.part(auto);
                        }
                        None => {}
                    }
                }
                if let Some(on) = s.contextual {
                    e.flag(ParaFlag::CONTEXTUAL, on);
                }
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_indents(&mut self, i: Indents, fonts: &Fonts) -> Outcome {
        self.format_paras_typed(
            "Indent",
            &|e, _| {
                if let Some(v) = i.left {
                    e.indent_left = Some(twip(v.0));
                    e.part(ParaProp::IndentLeft);
                }
                if let Some(v) = i.right {
                    e.indent_right = Some(twip(v.0));
                    e.part(ParaProp::IndentRight);
                }
                if let Some(v) = i.first_line {
                    e.indent_first = Some(twip(v.0));
                    e.part(ParaProp::IndentFirst);
                }
                if let Some(v) = i.hanging {
                    e.indent_first = Some(-twip(v.0));
                    e.part(ParaProp::IndentFirst);
                }
                if let Some(on) = i.mirror {
                    e.flag(ParaFlag::MIRROR_INDENTS, on);
                }
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_outline(&mut self, o: Outline, fonts: &Fonts) -> Outcome {
        if o.level.is_some_and(|l| l > 9) {
            return Outcome::invalid("the outline level is 0…9");
        }
        self.format_paras_typed(
            "Outline Level",
            &|e, _| {
                // Body text over a heading style's level is 9.
                if let Some(l) = o.level {
                    e.outline_level = Some(if l == 0 { 9 } else { i64::from(l) - 1 });
                    e.whole(ParaProp::OutlineLevel);
                }
                if let Some(on) = o.collapsed {
                    e.collapsed = Some(on);
                    e.whole(ParaProp::Collapsed);
                }
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_paragraph_text_direction(&mut self, d: TextFlow, fonts: &Fonts) -> Outcome {
        let code = crate::docx::model::text_flow(text_direction(d));
        self.format_paras_typed(
            "Text Direction",
            &|e, _| {
                e.text_flow = Some(code);
                e.whole(ParaProp::TextFlow);
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_paragraph_style(&mut self, id: StyleId, fonts: &Fonts) -> Outcome {
        let kind = self.doc.resolver.as_ref().and_then(|r| r.cascade().styles.get(&id.0).map(|s| s.kind));
        match kind {
            None => return Outcome::invalid("no such style"),
            Some(crate::format::StyleKind::Character) => return Outcome::invalid("a character style; applyCharacterStyle applies it"),
            Some(k) if k != crate::format::StyleKind::Paragraph => return Outcome::invalid("not a paragraph style"),
            Some(_) => {}
        }
        self.format(&Format::ParaStyle(id.0), fonts)
    }

    pub(crate) fn para_list(&self, para: ParaId) -> Option<bool> {
        let p = self.index.para(&self.doc, para)?;
        let ListRef { list: ListId(id), level: ilvl } = p.style.num.filter(|ListRef { list: ListId(id), level: _ }| *id != 0)?;
        let c = self.doc.resolver.as_ref()?.cascade();
        Some(crate::docx::list_xml::format_name(c.level(ListId(id), ilvl)?.format.as_ref()) == "bullet")
    }

    pub(in crate::editing) fn set_list(&mut self, kind: Option<ListKind>, fonts: &Fonts) -> Outcome {
        let now: Vec<Option<bool>> = self.selected_paras().into_iter().map(|p| self.para_list(p)).collect();
        match kind {
            Some(k) => {
                let bullet = k == ListKind::Bullet;
                if now.iter().all(|l| *l == Some(bullet)) {
                    return Outcome::Unchanged("already so");
                }
                self.toggle_list(bullet, fonts)
            }
            None if now.iter().all(Option::is_none) => Outcome::Unchanged("not in a list"),
            None => match now.iter().flatten().next() {
                Some(&b) if now.iter().all(|l| *l == Some(b)) => self.toggle_list(b, fonts),
                _ => self.joined("Numbering", fonts, |e| {
                    e.toggle_list(true, fonts);
                    e.toggle_list(true, fonts)
                }),
            },
        }
    }

    pub(in crate::editing) fn set_paragraph_flow(&mut self, f: ParagraphFlow, fonts: &Fonts) -> Outcome {
        let flags = [
            (ParaFlag::KEEP_NEXT, f.keep_with_next),
            (ParaFlag::KEEP_LINES, f.keep_lines_together),
            (ParaFlag::PAGE_BREAK_BEFORE, f.page_break_before),
            (ParaFlag::WIDOW_CONTROL, f.widow_control),
            (ParaFlag::SUPPRESS_AUTO_HYPHENS, f.dont_hyphenate),
            (ParaFlag::SUPPRESS_LINE_NUMBERS, f.suppress_line_numbers),
        ];
        self.format_paras_typed(
            "Paragraph Formatting",
            &|e, _| {
                for (flag, on) in flags {
                    if let Some(on) = on {
                        e.flag(flag, on);
                    }
                }
                if let Some(t) = f.textbox_tight_wrap {
                    // None is no tight wrap, written as none set.
                    e.tight_wrap = match t.ooxml().0 {
                        "allLines" => Some(TightWrap::AllLines),
                        "firstAndLastLine" => Some(TightWrap::FirstAndLastLine),
                        "firstLineOnly" => Some(TightWrap::FirstLineOnly),
                        "lastLineOnly" => Some(TightWrap::LastLineOnly),
                        _ => None,
                    };
                    e.whole(ParaProp::TightWrap);
                }
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_paragraph_borders(&mut self, b: BorderPreset, fonts: &Fonts) -> Outcome {
        let which = match b {
            BorderPreset::Box => "box",
            BorderPreset::Bottom => "bottom",
            BorderPreset::None => "none",
        };
        self.format(&Format::ParaBorders(which.into()), fonts)
    }
}

/// A text direction's `w:textDirection` value.
pub(in crate::editing) fn text_direction(d: TextFlow) -> &'static str {
    match d {
        TextFlow::LrTb => "lrTb",
        TextFlow::TbRl => "tbRl",
        TextFlow::BtLr => "btLr",
        TextFlow::LrTbV => "lrTbV",
        TextFlow::TbRlV => "tbRlV",
        TextFlow::TbLrV => "tbLrV",
    }
}
