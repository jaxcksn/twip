//! What the built-in rulers are drawn from: the frame's interaction state ([`RulerFrame`]) beside the ruler view.
//! `render::graphics::ruler` paints them from these alone, so a later painter can replace it without touching the
//! model, the commands or the interaction.

use super::theme::Theme;
use crate::api::services::{Popover, TabSelector};
use crate::editing::view::RulerMeasure;
use crate::services::ruler::{Bands, Marker};

/// A marker a drag moves: where the drag puts it (page points along its ruler), and whether the release removes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dragged {
    pub marker: Marker,
    pub at: f64,
    pub removing: bool,
}

/// Where the rulers go and what else they show: which bands, the viewport (CSS px), device pixels per CSS pixel, the
/// tab type box's type, the guide line of a drag (horizontal ruler: an x, else a y; client px), the dragged marker,
/// the ghost of the stop a click adds (page points), the hovered marker, the popover open, and the measurements.
pub struct RulerFrame<'a> {
    pub bands: Bands,
    pub width: f64,
    pub height: f64,
    pub dpr: f64,
    pub selector: TabSelector,
    pub guide: Option<(bool, f64)>,
    pub drag: Option<Dragged>,
    pub ghost: Option<f64>,
    pub hover: Option<Marker>,
    pub popover: Option<&'a Popover>,
    pub measures: &'a [RulerMeasure],
    /// The theme of what's drawn over the page, the guide and the measurements ([`super::Renderer::page_theme`]).
    pub page: Theme,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::services::{RulerAxis, RulerModel, RulerView, Span, TabStopInfo};
    use crate::api::value::Points;
    use crate::api::value::TabAlignment;
    use crate::editing::view::Tone;
    use crate::render::theme::{AZURE, Scheme};

    fn view() -> RulerView {
        let mut m = RulerModel::default();
        m.horizontal.page_width = Points(612.0);
        m.horizontal.text = Span { start: Points(72.0), end: Points(540.0) };
        m.horizontal.tabs = vec![TabStopInfo { position: Points(144.0), alignment: TabAlignment::Center, ..TabStopInfo::default() }];
        m.vertical.page_height = Points(792.0);
        m.vertical.text = Span { start: Points(72.0), end: Points(720.0) };
        RulerView {
            model: m,
            page: 1,
            horizontal: RulerAxis { origin: 24.0, scale: 1.0 },
            vertical: RulerAxis { origin: 24.0, scale: 1.0 },
            ..RulerView::default()
        }
    }

    fn frame<'a>(measures: &'a [RulerMeasure], drag: Option<Dragged>) -> RulerFrame<'a> {
        RulerFrame {
            bands: Bands { horizontal: true, vertical: true },
            width: 400.0,
            height: 300.0,
            dpr: 1.0,
            selector: TabSelector::Left,
            guide: Some((true, 200.0)),
            drag,
            ghost: None,
            hover: None,
            popover: None,
            measures,
            page: Theme::default(),
        }
    }

    /// The rulers drawn on a grey viewport 400 by 300, as RGBA: by Skia, or by twip-graphics while
    /// The rulers drawn by twip-graphics over grey, 400 × 300 device pixels.
    fn draw(theme: &Theme, f: &RulerFrame) -> Vec<u8> {
        let mut pixels = vec![0u8; 400 * 300 * 4];
        let mut g = crate::render::graphics::Graphics::default();
        g.clear_with(crate::render::theme::rgb(0x80_8080));
        g.ruler(&view(), theme, f);
        let mut t = twip_graphics::cpu::Target::new(&mut pixels, 400, 300, 400 * 4, twip_graphics::cpu::Format::Rgba8).expect("the buffer fits");
        g.present(&mut t);
        pixels
    }

    fn at(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
        pixels[(y * 400 + x) * 4..][..4].try_into().unwrap_or_default()
    }

    #[test]
    fn the_rulers_draw_on_the_paper_with_hatched_margins() {
        the_rulers_draw_on_the_paper_with_hatched_margins_on();
    }

    fn the_rulers_draw_on_the_paper_with_hatched_margins_on() {
        let theme = Theme::default();
        let p = draw(&theme, &frame(&[], None));
        // The text area is the paper strip, 2 to 16 px down the band, floating on the desk; the document below is
        // untouched.
        let desk = [0xe8, 0xe7, 0xe3];
        assert_eq!(at(&p, 24 + 300, 6), [255, 255, 255, 255]);
        assert_eq!(at(&p, 24 + 300, 0)[..3], desk, "the desk above the strip");
        assert_eq!(at(&p, 24 + 300, 17)[..3], desk, "and below it");
        assert_eq!(at(&p, 21, 0)[..3], desk);
        assert_eq!(at(&p, 300, 150), [0x80, 0x80, 0x80, 255]);
        // The margins are hatched: some pixels of the left margin's strip are darker than the paper.
        assert!((30..90).any(|x| at(&p, x, 8)[0] < 0xf8), "hatched margin");
        // The vertical ruler's text area is the paper, its strip 2 to 16 px across the desk.
        assert_eq!(at(&p, 10, 200)[..3], [255, 255, 255]);
        assert_eq!(at(&p, 0, 200)[..3], desk);
        assert_eq!(at(&p, 17, 200)[..3], desk);
        // The guide is accent on the strip.
        assert_eq!(at(&p, 200, 8)[..3], [0x0a, 0x74, 0xe4]);
        // A unit tick, 6 px up from the strip's bottom, one inch from the text start.
        assert_eq!(at(&p, 24 + 72 + 72, 15)[..3], [0x62, 0x65, 0x6c]);
        // The dark theme draws dark rulers.
        let dark = draw(&Theme::new(Scheme::Dark, AZURE), &frame(&[], None));
        assert_eq!(at(&dark, 24 + 300, 6)[..3], [0x22, 0x23, 0x27]);
    }

    #[test]
    fn a_dragged_marker_is_drawn_where_the_drag_puts_it_in_accent() {
        a_dragged_marker_is_drawn_where_the_drag_puts_it_in_accent_on();
    }

    fn a_dragged_marker_is_drawn_where_the_drag_puts_it_in_accent_on() {
        let theme = Theme::default();
        let drag = Dragged { marker: Marker::Tab(144.0), at: 72.0 + 198.0, removing: false };
        let p = draw(&theme, &frame(&[], Some(drag)));
        // The centre tab's stem, 2 px wide at the stop, from 7 to 16 px.
        let stem = |p: &[u8], x: usize| at(p, x, 12);
        assert_eq!(stem(&p, 24 + 270)[..3], [0x0a, 0x74, 0xe4], "at the drag, accent");
        assert_ne!(stem(&p, 24 + 216)[..3], [0x1c, 0x1d, 0x1f], "no longer where it was");
        let removing = draw(&theme, &frame(&[], Some(Dragged { removing: true, ..drag })));
        assert!(stem(&removing, 24 + 270)[0] > 0x80, "faded while a release removes it");
        // The ghost of the stop a click adds: the box's type, half-faded accent.
        let ghost = draw(&theme, &RulerFrame { ghost: Some(72.0 + 198.0), ..frame(&[], None) });
        let g = stem(&ghost, 24 + 270);
        assert!(g[2] > g[0] && g[0] > 0x60 && g[0] < 0xd0, "{g:?}");
    }

    #[test]
    fn measurements_draw_over_the_document() {
        measurements_draw_over_the_document_on();
    }

    fn measurements_draw_over_the_document_on() {
        let theme = Theme::default();
        let dimension = RulerMeasure::Dimension { from: [10.0, 60.0], to: [350.0, 60.0], label: "First line 1.00″".into(), tone: Tone::Accent };
        let chip = RulerMeasure::Chip { at: [100.0, 34.0], label: "First line 0.50″".into() };
        let p = draw(&theme, &frame(&[dimension, chip], None));
        // The dimension's end ticks, at the page's x 10 and 350 and y 60, and its line.
        let accent = [0x0a, 0x74, 0xe4];
        assert_eq!(at(&p, 24 + 10, 24 + 60 + 2)[..3], accent);
        assert_eq!(at(&p, 24 + 349, 24 + 60 - 2)[..3], accent);
        assert_eq!(at(&p, 24 + 40, 24 + 60)[..3], accent);
        // The chips' face has the dash and the values' prime and delta.
        let mono = crate::font::shipped(crate::render::CHROME_FONTS[0], "chrome").expect("the chips' face");
        assert!(['—', '″', 'Δ', '·', '−'].iter().all(|c| mono.glyph(*c).is_some_and(|g| g != 0)));
        // The chip's dark backdrop under the band.
        assert_eq!(at(&p, 102, 18 + 16 + 1)[..3], [0x1c, 0x1d, 0x1f]);
    }
}
