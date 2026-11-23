//! Header and footer composition

use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::model::*;
use crate::layout::fragment::{BoxFragment, BoxKind, PageChild, PageFragment};
use crate::layout::ids::{FragmentId, ParaId};
use crate::layout::job::{Memos, Yield};
use crate::layout::page::{PassRun, margins, story_ref_in};
use crate::layout::record::{HfRecord, PageRecord};
use crate::layout::snapshot::PageMeta;
use crate::layout::space::HfHeights;
use crate::layout::story::{StoryEnv, hf_limit, stack_limited};
use crate::layout::text::FieldValues;

use super::{PassCx, hook_height, used};

// Header and footer composition
pub(crate) fn hf_fields(run: &PassRun, cx: &PassCx, i: usize) -> FieldValues {
    let pg = &run.pages[i];
    let si = pg.section;
    let info = run.info.as_ref().expect("composition started");
    FieldValues {
        page: Some(pg.number.to_string()),
        num_pages: info.num_pages.clone(),
        section: Some((si + 1).to_string()),
        section_pages: info.section_pages.get(si).cloned(),
        page_refs: Some(info.page_refs.clone()),
        style_refs: style_refs(&cx.wanted, &run.pages, &cx.paras, i),
        page_fmt: cx.doc.sections[si].pg_fmt.clone(),
        chapter: page_chapter(cx, &run.pages, i),
        section_chapter: cx.chapters.sections.get(si).cloned().flatten(),
        chapters: None,
    }
}

/// Chapter numbers (`w:pgNumType w:chapStyle`): each body paragraph's page number prefix, and the headings' levels and
/// numbers, in document order, and each section's chapter number for the `w:pgNum` of its headers and footers.
#[derive(Default)]
pub(super) struct Chapters {
    pub(super) body: Option<Rc<HashMap<ParaId, String>>>,
    heads: Vec<(ParaId, i64, String)>,
    sections: Vec<Option<String>>,
}

/// A heading's level and chapter number: a "heading N" paragraph numbered through its style, its label without the
/// level text's literal text before the first and after the last number.
fn chapter_of(r: &crate::docx::resolve::Resolver, p: &Paragraph) -> Option<(i64, String)> {
    let (label, (num, ilvl), id) = (p.list.as_ref()?, p.style.num?, p.style.style_id.as_deref()?);
    let level = r.styles.by_id.get(id)?.name.to_lowercase().strip_prefix("heading ")?.parse().ok()?;
    if r.styles.chain_ppr(Some(id)).num_id != Some(num) {
        return None;
    }
    let text = &r.numbering.level(num, ilvl, &r.styles)?.text;
    let before = &text[..text.find('%').unwrap_or(0)];
    let after = text.rfind('%').and_then(|k| text.get(k + 2..)).unwrap_or("");
    let t = label.text.strip_prefix(before).unwrap_or(&label.text);
    Some((level, t.strip_suffix(after).unwrap_or(t).to_string()))
}

pub(super) fn chapters(doc: &Document) -> Chapters {
    let Some(r) = doc.resolver.as_deref().filter(|_| doc.sections.iter().any(|s| s.chapter.is_some())) else { return Chapters::default() };
    fn walk<'a>(blocks: &'a [Block], si: usize, out: &mut Vec<(usize, &'a Paragraph)>) {
        for b in blocks {
            match b {
                Block::Para(p) => out.push((si, p)),
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, si, out)),
            }
        }
    }
    let mut paras = Vec::new();
    let mut starts = Vec::new();
    doc.sections.iter().enumerate().for_each(|(si, s)| {
        starts.push(paras.len());
        walk(&s.blocks, si, &mut paras)
    });
    let heads: Vec<(usize, i64, String)> = paras.iter().enumerate().filter_map(|(k, (_, p))| chapter_of(r, p).map(|(l, t)| (k, l, t))).collect();
    // The last heading of the section's level at or before paragraph `k`; before the first, the first.
    let at = |si: usize, k: usize| {
        let (level, sep) = doc.sections[si].chapter.as_ref()?;
        let mut of_level = heads.iter().filter(|h| h.1 == *level);
        of_level.clone().take_while(|h| h.0 <= k).last().or_else(|| of_level.next()).map(|h| format!("{}{sep}", h.2))
    };
    let mut body = HashMap::new();
    for (k, (si, p)) in paras.iter().enumerate() {
        if let Some(c) = at(*si, k) {
            body.insert(ParaId(p.id), c);
        }
    }
    // A header's or footer's `w:pgNum`: the heading in force at the section's first paragraph, on every page of it.
    let sections = starts.iter().enumerate().map(|(si, &k)| at(si, k)).collect();
    let heads = heads.into_iter().map(|(k, l, t)| (ParaId(paras[k].1.id), l, t)).collect();
    Chapters { body: Some(Rc::new(body)), heads, sections }
}

/// The chapter prefix of page `i`'s headers and footers: from the last heading at or before the page's end.
fn page_chapter(cx: &PassCx, pages: &[PageRecord], i: usize) -> Option<String> {
    let (level, sep) = cx.doc.sections.get(pages[i].section)?.chapter.as_ref()?;
    let heads = &cx.chapters.heads;
    let head = |id: &ParaId| heads.iter().find(|h| h.0 == *id && h.1 == *level);
    let h = (0..=i).rev().find_map(|k| pages[k].effects.paras.iter().rev().find_map(head)).or_else(|| heads.iter().find(|h| h.1 == *level))?;
    Some(format!("{}{sep}", h.2))
}

pub(crate) fn hf_parts<'d>(run: &PassRun, cx: &PassCx<'d>, i: usize) -> [Option<&'d String>; 2] {
    let pg = &run.pages[i];
    if pg.blank {
        return [None, None];
    }
    let pis = run.pages[..i].iter().filter(|p| p.section == pg.section && !p.blank).count();
    let part = |footer| {
        story_ref_in(&cx.doc.sections, cx.ctx.env.text.review.view, cx.ctx.env.even_and_odd, pg.section, pis, pg.number, footer)
            .1
            .filter(|p| cx.doc.stories.contains_key(*p))
    };
    [part(false), part(true)]
}

pub(super) fn compose_page(run: &mut PassRun, cx: &PassCx, i: usize, memos: &mut Memos) -> Result<(), Yield> {
    let pg = &run.pages[i];
    if pg.blank {
        run.pages[i].composed = true;
        return Ok(());
    }
    let (si, num) = (pg.section, pg.number);
    let sec = &cx.doc.sections[si];
    let ctx = cx.ctx;
    let (left, right, top) = margins(sec, &ctx, num);
    let s = sec.section_in(ctx.env.text.review.view);
    let width = s.page_w - left - right;
    let frames = crate::layout::float::Frames {
        page_w: s.page_w,
        page_h: s.page_h,
        margin: [left, right, top, s.bottom],
        column: (left, width),
        compat: ctx.env.text.compat,
        cell: false,
        odd: num % 2 == 1,
    };
    let fields = hf_fields(run, cx, i);
    let parts = hf_parts(run, cx, i);
    let env = StoryEnv { ctx, fields: &fields, notes: &cx.notes.numbers };
    let limit = hf_limit(ctx.env.text.compat, s.page_h);
    let reserved = used(pg);
    let (clip, k) = (run.clip, run.k);
    let mut full: HfHeights = [None, None];
    let mut records: [Option<HfRecord>; 2] = [None, None];
    let mut boxes: [Option<BoxFragment>; 2] = [None, None];
    for (slot, footer) in [false, true].into_iter().enumerate() {
        let Some(part) = parts[slot] else { continue };
        let blocks: Vec<&Block> = cx.doc.stories[part].iter().collect();
        let cap = if clip { reserved[slot].map_or(limit, |r| limit.min(r)) } else { limit };
        let story_fields = crate::layout::page::fields_used(&fields, blocks.iter().copied());
        if footer {
            let foot = s.page_h - s.footer;
            let (l, at, given) = crate::layout::story::footer_layout(&blocks, width, &frames, foot, limit, &env, memos)?;
            let height = given.min(stack_limited(&l, left, 0.0, cap).1);
            full[1] = Some(given);
            let at = at.max(foot - cap);
            let (mut children, _) = stack_limited(&l, left, at, cap);
            children.extend(crate::layout::story::hf_floats(&blocks, &l, &frames, at, &env, memos)?);
            records[1] = Some(HfRecord { part: part.clone(), fields: story_fields, height });
            boxes[1] = Some(BoxFragment {
                kind: BoxKind::Footer { story: part.clone() },
                id: None,
                offset: (0.0, 0.0),
                size: (width, height),
                children: children.into(),
            });
        } else {
            let l = crate::layout::story::hf_layout(&blocks, width, &frames, |_| s.header, &env, memos)?;
            full[0] = Some(stack_limited(&l, left, s.header, limit).1 + hook_height(k));
            let (mut children, height) = if limit.is_infinite() && !clip && s.header + l.height > 0.85 * s.page_h {
                let cut = 0.435 * s.page_h;
                let mut c = Vec::new();
                for u in l.units.iter().take_while(|u| s.header + u.top < cut) {
                    c.push(crate::layout::fragment::Child::Group { offset: (left, s.header + u.top), children: u.content.clone() });
                }
                (c, l.height)
            } else {
                stack_limited(&l, left, s.header, cap)
            };
            children.extend(crate::layout::story::hf_floats(&blocks, &l, &frames, s.header, &env, memos)?);
            records[0] = Some(HfRecord { part: part.clone(), fields: story_fields, height: if clip { height } else { full[0].unwrap_or(height) } });
            boxes[0] = Some(BoxFragment {
                kind: BoxKind::Header { story: part.clone() },
                id: None,
                offset: (0.0, 0.0),
                size: (width, if clip { height } else { l.height }),
                children: children.into(),
            });
        }
    }
    let pg = &mut run.pages[i];
    pg.hf = records;
    pg.hf_boxes = boxes;
    pg.hf_full = full;
    pg.composed = true;
    Ok(())
}

/// A page's border lines: each edge inside the rectangle its outer edges are on. Art borders aren't drawn.
fn page_borders(pg: &PageRecord) -> Box<[crate::layout::fragment::Item]> {
    let Some((b, [x0, y0, x1, y1])) = &pg.page_borders else { return Box::new([]) };
    let mut out = Vec::new();
    let line = |x: &Option<crate::docx::props::Border>| x.as_ref().filter(|x| x.visible() && crate::layout::table::drawn(x)).cloned();
    let w = |x: &Option<crate::docx::props::Border>| line(x).map_or(0.0, |x| crate::layout::table::thick(&x));
    let (lw, rw) = (w(&b.left), w(&b.right));
    if let Some(t) = line(&b.top) {
        crate::layout::table::edge(&mut out, [*x0, *y0, *x1, y0 + crate::layout::table::thick(&t)], true, false, &t);
    }
    if let Some(t) = line(&b.bottom) {
        crate::layout::table::edge(&mut out, [*x0, y1 - crate::layout::table::thick(&t), *x1, *y1], true, true, &t);
        crate::layout::table::shadow(
            &mut out,
            [*x0, y1 - crate::layout::table::thick(&t), *x1, *y1],
            true,
            true,
            &t,
            line(&b.left).is_some(),
            line(&b.right).is_some(),
        );
    }
    if let Some(l) = line(&b.left) {
        crate::layout::table::edge(&mut out, [*x0, *y0, x0 + lw, *y1], false, false, &l);
    }
    if let Some(r) = line(&b.right) {
        crate::layout::table::edge(&mut out, [x1 - rw, *y0, *x1, *y1], false, true, &r);
        crate::layout::table::shadow(&mut out, [x1 - rw, *y0, *x1, *y1], false, true, &r, line(&b.top).is_some(), false);
    }
    out.into_boxed_slice()
}

pub(crate) fn finish_page(pg: &PageRecord, section_page: usize) -> (Rc<PageFragment>, PageMeta) {
    let mut children: Vec<PageChild> = pg.bodies.iter().map(|b| PageChild::Body(b.body.clone())).collect();
    if let Some(h) = &pg.hf_boxes[0] {
        children.push(PageChild::Header(h.clone()));
    }
    if let Some(f) = &pg.hf_boxes[1] {
        children.push(PageChild::Footer(f.clone()));
    }
    let page =
        PageFragment { id: FragmentId::next(), w: pg.w, h: pg.h, body_area: pg.body_area, children: children.into(), borders: page_borders(pg) };
    (Rc::new(page), PageMeta { section: pg.section, number: pg.number, section_page, blank: pg.blank })
}

/// Each header and footer `STYLEREF`'s text on page `i`, by its code: the first in its style on the page (`\l`: the
/// last), else the last on an earlier page, else the first on a later one.
// ponytail: a page's paragraphs whole; a character style's runs on the part of a split paragraph another page holds
// count on this one too.
fn style_refs(
    wanted: &[(String, crate::docx::fields::StyleQuery)],
    pages: &[PageRecord],
    paras: &HashMap<ParaId, Vec<(String, bool, String)>>,
    i: usize,
) -> Vec<(String, String)> {
    let on = |k: usize| pages.get(k).map(|p| &p.effects.paras[..]).unwrap_or(&[]);
    wanted
        .iter()
        .filter_map(|(code, q)| {
            let hit = |list: &[ParaId], rev: bool| {
                let find = |r: &ParaId| q.find(paras.get(r)?, rev);
                if rev { list.iter().rev().find_map(find) } else { list.iter().find_map(find) }
            };
            hit(on(i), q.last)
                .or_else(|| (0..i).rev().find_map(|k| hit(on(k), true)))
                .or_else(|| (i + 1..pages.len()).find_map(|k| hit(on(k), false)))
                .map(|t| (code.clone(), t))
        })
        .collect()
}
