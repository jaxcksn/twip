//! Borders and Shading's values: each edge's line and the shading, of the selection's paragraph and text, or of
//! text in a style.

use super::Editor;
use crate::api::command::{Border, BorderStyle, ShadingPattern, ShadingTarget};
use crate::api::services::{BorderSetting, BordersShadingDialog, BordersTab};
use crate::api::value::{AutoOr, Color, NoneOr, Points};

/// An edge's line, or none.
pub(super) fn line(b: &Option<crate::docx::props::Border>) -> Option<Border> {
    let b = b.as_ref().filter(|b| b.visible())?;
    Some(Border { style: BorderStyle::of_val(&b.style), width: Points(b.width), color: b.color.map_or(AutoOr::Auto, |c| AutoOr::Value(Color(c))) })
}

impl Editor {
    /// Borders and Shading's values for a table style's part (`region`, none for the whole table): its cells' edges,
    /// inside lines and shading.
    pub fn table_part_borders(&self, raw: &str, region: Option<&str>) -> Option<BordersShadingDialog> {
        let res = self.doc.resolver.as_ref()?;
        let mut next = (**res).clone();
        next.styles.set_xml(raw.to_string()).ok()?;
        let id = next.styles.by_id.values().find(|s| s.raw == raw)?.id.clone();
        let r = next.styles.table_region(Some(&id), &region.into_iter().collect::<Vec<_>>());
        // The whole table's edges are the table's; a part's, its cells'.
        let b = if region.is_none() { &r.tbl.borders } else { &r.tc.borders };
        let shd = r.tc.shd.clone();
        let mut v = self.borders_shading_values(None, false)?;
        (v.top, v.bottom, v.left, v.right) = (line(&b.top), line(&b.bottom), line(&b.left), line(&b.right));
        (v.between, v.inside_vertical) = (line(&b.inside_h), line(&b.inside_v));
        v.apply_to = ShadingTarget::Cell;
        v.fill = shd.as_ref().map_or(r.tc.shading.flatten(), |s| s.fill).map_or(NoneOr::None, |c| NoneOr::Value(Color(c)));
        v.pattern = shd.as_ref().map_or(ShadingPattern::Clear, |s| ShadingPattern::of_val(&s.pattern));
        v.pattern_color = shd.as_ref().and_then(|s| s.color).map_or(AutoOr::Auto, |c| AutoOr::Value(Color(c)));
        v.setting = crate::services::dialogs::border_setting(&v, false);
        if let Some(l) = [&v.top, &v.left, &v.bottom, &v.right, &v.between, &v.inside_vertical].into_iter().flatten().next() {
            (v.style, v.color, v.width) = (l.style, l.color, l.width);
        }
        (v.page_top, v.page_bottom, v.page_left, v.page_right, v.page_setting) = (None, None, None, None, BorderSetting::None);
        Some(v)
    }

    /// Borders and Shading's values for the selection's paragraph and text (`style` none), or for text in a style's
    /// XML; `text` applies the shading to the text.
    pub fn borders_shading_values(&self, style: Option<&str>, text: bool) -> Option<BordersShadingDialog> {
        let (p, run) = match style {
            Some(raw) => {
                let (para, run, _) = self.style_formatting(raw)?;
                (para.style.clone(), run)
            }
            None => {
                let focus = self.selection.focus();
                let id = focus.and_then(|p| p.para_offset()).map(|(p, _)| p)?;
                let para = self.index.para(&self.doc, id)?;
                (para.style.clone(), focus.and_then(|c| self.run_at(&c)).unwrap_or_else(|| para.mark.clone()))
            }
        };
        // The shading as written: its pattern and colors; else the resolved fill.
        let shd = if text { run.shd.clone() } else { p.shd.clone() };
        let fill = shd.as_ref().map_or(if text { run.shading } else { p.shading }, |s| s.fill);
        let pattern = shd.as_ref().map_or(ShadingPattern::Clear, |s| ShadingPattern::of_val(&s.pattern));
        let pattern_color = shd.as_ref().and_then(|s| s.color).map_or(AutoOr::Auto, |c| AutoOr::Value(Color(c)));
        // Text has one border, a box: every edge is its line.
        let boxed = text.then(|| line(&run.border));
        let edge = |b: &Option<crate::docx::props::Border>| boxed.clone().unwrap_or_else(|| line(b));
        let mut v = BordersShadingDialog {
            tab: BordersTab::Borders,
            setting: BorderSetting::None,
            style: BorderStyle::Single,
            color: AutoOr::Auto,
            width: Points(0.5),
            top: edge(&p.border_top),
            bottom: edge(&p.border_bottom),
            left: edge(&p.border_left),
            right: edge(&p.border_right),
            between: if text { None } else { line(&p.border_between) },
            apply_to: if text { ShadingTarget::Text } else { ShadingTarget::Paragraph },
            fill: fill.map_or(NoneOr::None, |c| NoneOr::Value(Color(c))),
            pattern,
            pattern_color,
            inside_vertical: None,
            page_setting: BorderSetting::None,
            page_top: None,
            page_bottom: None,
            page_left: None,
            page_right: None,
            page_offset_from: crate::api::command::PageBorderOffset::Page,
            page_display: crate::api::command::PageBorderDisplay::AllPages,
            page_apply_to: crate::api::command::PageBorderScope::ThisSection,
        };
        // The Page Border tab: the selection's section's (none for a style).
        if style.is_none()
            && let Some(b) = self.doc.sections.get(self.selection_section()).and_then(|s| s.page_borders.clone())
        {
            (v.page_top, v.page_bottom, v.page_left, v.page_right) = (line(&b.top), line(&b.bottom), line(&b.left), line(&b.right));
            v.page_offset_from =
                if b.offset_from_page { crate::api::command::PageBorderOffset::Page } else { crate::api::command::PageBorderOffset::Text };
            v.page_display = match b.display.as_str() {
                "firstPage" => crate::api::command::PageBorderDisplay::FirstPage,
                "notFirstPage" => crate::api::command::PageBorderDisplay::NotFirstPage,
                _ => crate::api::command::PageBorderDisplay::AllPages,
            };
            v.page_setting = crate::services::dialogs::page_border_setting(&v, b.bottom.as_ref().is_some_and(|x| x.shadow));
        }
        let shadow = if text { run.border.as_ref().is_some_and(|b| b.shadow) } else { p.border_bottom.as_ref().is_some_and(|b| b.shadow) };
        v.setting = crate::services::dialogs::border_setting(&v, shadow);
        // The line new edges take: the first edge's.
        if let Some(b) = [&v.top, &v.left, &v.bottom, &v.right, &v.between, &v.page_top, &v.page_left].into_iter().flatten().next() {
            (v.style, v.color, v.width) = (b.style, b.color, b.width);
        }
        Some(v)
    }
}
