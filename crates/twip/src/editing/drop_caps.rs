//! Drop caps: a paragraph's first letter in a frame of its own, dropped into the text or in the margin

use std::rc::Rc;

use super::Editor;
use super::commands::Outcome;
use super::position::StoryId;
use super::step::{Edges, Step};
use crate::api::command::{DropCap, DropCapPosition};
use crate::api::value::Points;
use crate::docx::run_xml;
use crate::docx::write::props::esc;
use crate::docx::{Block, Inline, Paragraph};
use crate::font::Fonts;
use crate::layout::{FloatId, ParaId, Unit};

/// Lines a drop cap drops at most (Word's).
const MAX_LINES: u32 = 10;

fn units(inlines: &[Inline], k: usize) -> Unit {
    Unit(inlines[..k].iter().map(|i| i.units() as u32).sum())
}

impl Editor {
    /// The paragraph the Drop Cap dialog works on: the selection's, or the one the selected drop cap anchors to.
    fn drop_cap_para(&self) -> Option<ParaId> {
        if let Some(f) = self.frame_here() {
            return self.index.float_anchor(f);
        }
        self.selection.focus()?.para_offset().map(|(p, _)| p)
    }

    /// The paragraph's drop cap: where its float is in the inlines, the float, and its settings.
    fn drop_cap_in(&self, para: ParaId) -> Option<(usize, FloatId, DropCap)> {
        let p = self.index.para(&self.doc, para)?;
        let (k, f, fr, position) = p.inlines.iter().enumerate().find_map(|(k, i)| {
            let f = match i {
                Inline::Float(f, _) if f.is_frame() => FloatId(f.id),
                _ => return None,
            };
            let fr = self.frame_pr(f)?;
            let position = match fr.drop_cap.as_deref() {
                Some("drop") => DropCapPosition::Dropped,
                Some("margin") => DropCapPosition::InMargin,
                _ => return None,
            };
            Some((k, f, fr, position))
        })?;
        let font = self.float_of(f)?.text_box.as_ref()?.blocks.iter().find_map(|b| match b {
            Block::Para(q) => q.inlines.iter().find_map(|i| if let Inline::Text(_, r) = i { Some(r.ascii.clone()) } else { None }),
            Block::Table(_) => None,
        });
        let lines = fr.lines.map_or(3, |l| l.clamp(1, i64::from(MAX_LINES)) as u32);
        Some((k, f, DropCap { position, font, lines: Some(lines), distance: Some(Points(fr.h_space)) }))
    }

    /// The Drop Cap dialog's values for the selection's paragraph: none there when it has no drop cap.
    pub fn drop_cap(&self) -> Option<DropCap> {
        let para = self.drop_cap_para()?;
        Some(self.drop_cap_in(para).map_or(DropCap { position: DropCapPosition::None, font: None, lines: None, distance: None }, |(.., d)| d))
    }

    /// Drops the selection's paragraph's first letter (or changes its drop cap), or puts the letter back (`none`).
    pub(in crate::editing) fn set_drop_cap(&mut self, d: DropCap, fonts: &Fonts) -> Outcome {
        if self.style_target.is_some() {
            return Outcome::unavailable("not in a style");
        }
        let Some(para) = self.drop_cap_para() else { return Outcome::unavailable("no paragraph") };
        let Some(p) = self.index.para(&self.doc, para).cloned() else { return Outcome::unavailable("no paragraph") };
        if p.style.in_frame {
            return Outcome::unavailable("a drop cap goes in a paragraph, not a frame");
        }
        if self.index.para_slot(para).is_some_and(|s| matches!(self.index.story(s.container), StoryId::TextBox(_))) {
            return Outcome::unavailable("text boxes don't nest");
        }
        let lines = d.lines.unwrap_or(3);
        if !(1..=MAX_LINES).contains(&lines) || d.distance.is_some_and(|x| !(0.0..=1584.0).contains(&x.0)) {
            return Outcome::Refused(crate::api::error::Error::invalid("a drop cap drops 1 to 10 lines, 0 to 1584 points from the text"));
        }
        let existing = self.drop_cap_in(para);
        let mut tx = self.begin("Drop Cap");
        let ok = match &existing {
            // The float swapped for the new one (it takes no units), or taken out with its letters back in its place.
            Some((k, f, _)) => {
                let letters: String = self.float_of(*f).and_then(|fl| fl.text_box.as_ref()).map_or_else(String::new, |tb| {
                    tb.blocks.iter().filter_map(|b| if let Block::Para(q) = b { Some(crate::docx::fields::text(q)) } else { None }).collect()
                });
                let run = p.inlines[k + 1..].iter().find_map(|i| if let Inline::Text(_, r) = i { Some(r.clone()) } else { None });
                let run = run.unwrap_or_else(|| p.mark.clone());
                let mut inlines = p.inlines.clone();
                match d.position {
                    DropCapPosition::None => {
                        inlines.remove(*k);
                        let at = units(&p.inlines, *k);
                        self.step(&mut tx, Step::Restyle { para, inlines }).is_ok()
                            && self
                                .step(&mut tx, Step::Text { para, range: at..at, with: vec![Inline::Text(letters, run)], edges: Edges::default() })
                                .is_ok()
                    }
                    _ => match self.drop_cap_float(&p, &d, lines, &letters, &run, fonts) {
                        Some(f) => {
                            inlines[*k] = Inline::Float(Box::new(f), run);
                            self.step(&mut tx, Step::Restyle { para, inlines }).is_ok()
                        }
                        None => false,
                    },
                }
            }
            None if d.position == DropCapPosition::None => {
                self.abort(tx);
                return Outcome::Unchanged("no drop cap");
            }
            // The first letter out of the text, its frame in its place.
            None => {
                let Some(k) = p.inlines.iter().position(|i| i.units() > 0) else {
                    self.abort(tx);
                    return Outcome::unavailable("the paragraph has no text");
                };
                let Inline::Text(t, run) = &p.inlines[k] else {
                    self.abort(tx);
                    return Outcome::unavailable("the paragraph doesn't start with text");
                };
                let Some(first) = t.chars().next().filter(|c| !c.is_whitespace()) else {
                    self.abort(tx);
                    return Outcome::unavailable("the paragraph doesn't start with a letter");
                };
                let at = units(&p.inlines, k);
                match self.drop_cap_float(&p, &d, lines, &first.to_string(), run, fonts) {
                    Some(f) => {
                        let with = vec![Inline::Float(Box::new(f), run.clone())];
                        self.step(&mut tx, Step::Text { para, range: at..Unit(at.0 + 1), with, edges: Edges::default() }).is_ok()
                    }
                    None => false,
                }
            }
        };
        if !ok {
            self.abort(tx);
            return Outcome::Unchanged("stale paragraph");
        }
        self.follow(&tx);
        self.commit(tx, None, fonts)
    }

    /// The frame holding the letters: its paragraph framed to drop `lines` lines, its line exactly that tall, and the
    /// letters sized to fill it.
    fn drop_cap_float(
        &self,
        p: &Paragraph,
        d: &DropCap,
        lines: u32,
        letters: &str,
        run: &crate::docx::Run,
        fonts: &Fonts,
    ) -> Option<crate::docx::Float> {
        let res = self.doc.resolver.clone()?;
        let cx = self.cx();
        let w = &cx.w;
        // One line of the paragraph's text, as layout sets it.
        let face = fonts.face(&run.ascii, None, run.bold, run.italic);
        let line = face.map_or(run.size * 1.2, |f| {
            let (a, de, gap) = crate::layout::text::metrics(&f, run.size);
            a + de + gap
        });
        let drop = f64::from(lines) * line;
        // shortcut: Word sizes the letter from its font's cap height; four thirds of the drop is what it gives for
        // the usual fonts, lowered by a twelfth of its size.
        // At most Word's largest font size.
        let size = ((drop * 4.0 / 3.0 * 2.0).round() / 2.0).min(1638.0);
        let lower = (size / 12.0 * 2.0).round();
        let twips = |pt: f64| (pt * 20.0).round() as i64;
        let (kind, h_anchor) = if d.position == DropCapPosition::InMargin { ("margin", "page") } else { ("drop", "text") };
        let style = p.style.style_id.as_deref().map_or(String::new(), |s| format!("<{w}pStyle {w}val=\"{}\"/>", esc(s)));
        let space = d.distance.map_or(String::new(), |x| format!(" {w}hSpace=\"{}\"", twips(x.0)));
        let ppr = format!(
            "<{w}pPr>{style}<{w}keepNext/><{w}framePr {w}dropCap=\"{kind}\" {w}lines=\"{lines}\" {w}wrap=\"around\" {w}vAnchor=\"text\" {w}hAnchor=\"{h_anchor}\"{space}/><{w}spacing {w}after=\"0\" {w}line=\"{}\" {w}lineRule=\"exact\"/><{w}textAlignment {w}val=\"baseline\"/></{w}pPr>",
            twips(drop)
        );
        let frame = res.para_from_xml(Some(&ppr), None, &cx).style;
        use crate::format::{FontRef, Prop, ScriptClass, ScriptFonts};
        let mut e = run_xml::edit_of(run);
        if let Some(font) = d.font.as_deref().filter(|f| !f.is_empty()) {
            let f = Some(FontRef::Family(font.into()));
            (e.fonts, e.hint) = (ScriptFonts { latin: f.clone(), latin_extended: f.clone(), east_asian: None, complex: f }, None);
            for c in [ScriptClass::Latin, ScriptClass::LatinExtended, ScriptClass::Complex] {
                e.whole(Prop::Font(c));
            }
        }
        e.position = Some(-lower / 2.0);
        e.whole(Prop::Position);
        let half = ((size * 2.0) as i64) as f64 / 2.0;
        (e.size, e.complex_size) = (Some(half), Some(half));
        e.whole(Prop::Size);
        e.whole(Prop::ComplexSize);
        let mut base = res.styles.doc_rpr.clone();
        base.merge_level(&res.styles.chain_rpr(res.styles.paragraph_style(p.style.style_id.as_deref())));
        let big = run_xml::run_of(&res, &base, run.direct.edit(e));
        let para = Paragraph {
            style: Rc::new(frame),
            mark: big.clone(),
            list: None,
            inlines: vec![Inline::Text(letters.to_string(), big)],
            src: None,
            id: crate::docx::next_para_id(),
            div: None,
            controls: Vec::new(),
        };
        Some(crate::docx::frames::float(vec![para], self.doc.settings.compat() == crate::docx::Compat::Legacy))
    }
}
