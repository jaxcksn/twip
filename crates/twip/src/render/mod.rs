//! The renderer's look and the screen: what pages and the editor's view overlays are drawn in (the theme, the
//! overlay colours, the document's appearance) and how frames are laid over the pages, drawn by [`graphics`] with
//! twip-graphics. It reads layout's paint traversal and `editing::view` only, and holds no editing state: everything
//! it draws for the editor comes from one [`View`], built from the snapshot it paints.

pub mod appearance;
pub mod graphics;
pub mod preview;
pub mod ruler;
pub mod svg;
pub mod theme;
#[cfg(twip_gpu)]
pub mod window;

pub use theme::Color;
/// Where and how a frame is drawn: the scroll origin (CSS px), the canvas size, device pixels per CSS pixel and CSS
/// pixels per point.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub origin_x: f64,
    pub origin_y: f64,
    pub width: f64,
    pub height: f64,
    pub dpr: f64,
    pub px_per_pt: f64,
    /// Draw the caret (the view's, when it has one): the host's view is focused and the blink is in its visible phase.
    pub caret: bool,
}

/// What frames are drawn in: the desk, the page's colour and the document's appearance, the chrome's theme and the
/// overlay colours, and whether page content is kept between frames.
#[derive(Default)]
pub struct Renderer {
    pub background: Option<Color>,
    /// Table gridlines (cell edges without borders) show, on screen only.
    pub gridlines: bool,
    /// Keep each page's content and draw it again from that ([`graphics::retained`]): on screen, not for PDF export.
    pub retain: bool,
    /// Draw everything from scratch, past any retained page content: what a verified frame is compared with.
    pub direct: bool,
    /// The chrome's colours.
    pub theme: theme::Theme,
    /// Overlay colours the host set; they win over the theme's.
    pub overlay_colours: Option<Palette>,
    pub appearance: appearance::Appearance,
    pub page_colour: Option<u32>,
}

/// The colours of the overlays the host can set. By default the lines and marks are at least 3:1 against the page,
/// and the selection is the light blue of a system text selection, black text over it at least 4.5:1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// Blended under the content (multiply), so text over it keeps its contrast.
    pub selection: Color,
    pub object: Color,
    pub drop: Color,
    pub marks: Color,
    pub leaders: Color,
}

impl Default for Palette {
    fn default() -> Palette {
        theme::Theme::default().palette()
    }
}

/// The chrome's typefaces: subsets of JetBrains Mono (labels, numbers, hints and readouts) that ship with the engine
/// (`twip-tools chrome-fonts`), the same on every target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromeFace {
    Mono500,
    Mono600,
}

/// The chrome faces' font files, by [`ChromeFace`]: both screens draw from these bytes.
pub(crate) const CHROME_FONTS: [&[u8]; 2] =
    [include_bytes!("chrome_fonts/jetbrains-mono-500.ttf"), include_bytes!("chrome_fonts/jetbrains-mono-600.ttf")];

pub fn rgb(c: u32) -> Color {
    Color::rgb(c)
}

impl Renderer {
    /// A renderer for the screen: it keeps each page's content ([`retained`]).
    pub fn retaining() -> Renderer {
        Renderer { retain: true, ..Renderer::default() }
    }

    /// Sets the chrome's theme; the overlay colours follow it unless the host set its own.
    pub fn set_theme(&mut self, theme: theme::Theme) {
        self.theme = theme;
    }

    /// The overlay colours: the host's ([`Renderer::overlay_colours`]), or the page theme's.
    pub fn palette(&self) -> Palette {
        self.overlay_colours.unwrap_or_else(|| self.page_theme().palette())
    }

    /// The caret's colour: black or white, whichever stands out more on the page as drawn. Neither the theme's
    /// colours nor the host's change it.
    pub fn caret(&self) -> Color {
        let page = self.page();
        if theme::contrast(Color::WHITE, page) > theme::contrast(Color::BLACK, page) { Color::WHITE } else { Color::BLACK }
    }

    /// The page's colour as drawn: what handles and buttons on the page are filled with, whatever the chrome's theme.
    pub(super) fn page(&self) -> Color {
        rgb(self.appearance.map(self.page_colour.unwrap_or(0xFF_FFFF)))
    }

    /// The theme of what's drawn over the page (the selection, handles, guides, measurements): the chrome's when its
    /// scheme matches the page, else the matching scheme's with the same accent, so a light page under dark chrome
    /// gets the light scheme's accent and readouts.
    pub fn page_theme(&self) -> theme::Theme {
        let dark = theme::luminance(self.page()) < 0.2;
        match self.theme.scheme {
            theme::Scheme::HighContrast(_) => self.theme,
            _ if dark == self.theme.is_dark() => self.theme,
            _ => theme::Theme::new(if dark { theme::Scheme::Dark } else { theme::Scheme::Light }, self.theme.accent.base),
        }
    }

    /// A document colour as drawn: through the appearance's mapping, unless the item keeps its own colours (`own`).
    pub(crate) fn ink_for(&self, own: bool, c: u32) -> Color {
        rgb(if own { c } else { self.appearance.map(c) })
    }

    /// The pages a frame shows.
    pub fn visible(stack: &crate::editing::view::PageStack, f: &Frame) -> std::ops::Range<usize> {
        let k = f.px_per_pt;
        let (top, bottom) = (-f.origin_y, -f.origin_y + f.height / f.dpr);
        let shown: Vec<usize> = (0..stack.pages.len())
            .filter(|&i| {
                let [_, y, _, h] = stack.pages[i];
                !((y + h) * k < top || y * k > bottom)
            })
            .collect();
        match (shown.first(), shown.last()) {
            (Some(&a), Some(&b)) => a..b + 1,
            _ => 0..0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use theme::rgb8;

    /// WCAG relative luminance.
    fn luminance(c: Color) -> f64 {
        let lin = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
    }

    #[test]
    fn marks_on_a_dark_page_hold_against_it() {
        let r = Renderer { appearance: appearance::Appearance::Dark { page: appearance::PageMode::Dark }, ..Renderer::default() };
        let page = r.ink_for(false, 0xFF_FFFF);
        for c in [0xE6_0000, 0x00_78D4, 0xD1_3438, 0xC1_9C00] {
            assert!(theme::contrast(r.ink_for(false, c), page) >= 3.0, "{c:06X} on the dark page");
        }
    }

    #[test]
    fn the_caret_is_black_or_white_against_the_page() {
        let mut r = Renderer::default();
        let mut dark = theme::Theme::new(theme::Scheme::Dark, theme::AZURE);
        dark.tokens.ink = rgb8(0xE6, 0xED, 0xF3);
        r.set_theme(dark);
        assert_eq!(r.caret(), Color::BLACK, "on the white page, whatever the chrome's text colour");
        r.appearance = appearance::Appearance::Dark { page: appearance::PageMode::Dark };
        assert_eq!(r.caret(), Color::WHITE, "on the dark page");
        r.appearance = appearance::Appearance::Light;
        r.page_colour = Some(0x10_2030);
        assert_eq!(r.caret(), Color::WHITE, "on a dark page colour");
        r.overlay_colours = Some(Palette { marks: theme::rgb(0xFF_0000), ..Palette::default() });
        assert_eq!(r.caret(), Color::WHITE, "whatever the host's overlay colours");
    }

    #[test]
    fn what_is_drawn_over_the_page_takes_the_pages_scheme() {
        let mut r = Renderer::default();
        r.set_theme(theme::Theme::new(theme::Scheme::Dark, theme::AZURE));
        let light = theme::Tokens::of(theme::Scheme::Light);
        assert_eq!((r.page_theme().tokens, r.page()), (light, Color::WHITE), "a light page under dark chrome");
        assert_eq!(r.palette().object, theme::AZURE, "and the light scheme's accent");
        r.appearance = appearance::Appearance::Dark { page: appearance::PageMode::Dark };
        assert_eq!(r.page_theme(), r.theme, "the dark page: the chrome's own");
    }

    #[test]
    fn overlay_colours_the_host_set_outlast_a_theme_change() {
        let mut r = Renderer::default();
        let mine = Palette { marks: theme::rgb(0xFF_0000), ..Palette::default() };
        r.overlay_colours = Some(mine);
        r.set_theme(theme::Theme::new(theme::Scheme::Dark, theme::AZURE));
        assert_eq!(r.palette(), mine, "the host's colours stay");
        assert_eq!(r.theme.tokens.desk, theme::Tokens::of(theme::Scheme::Dark).desk, "the rest follows the theme");
        r.overlay_colours = None;
        r.set_theme(theme::Theme::new(theme::Scheme::Dark, theme::AZURE));
        r.appearance = appearance::Appearance::Dark { page: appearance::PageMode::Dark };
        assert_eq!(r.palette(), r.theme.palette());
    }

    #[test]
    fn the_chrome_faces_load_and_draw_their_symbols() {
        for (face, data) in [ChromeFace::Mono500, ChromeFace::Mono600].into_iter().zip(CHROME_FONTS) {
            let f = crate::font::shipped(data, "chrome").unwrap_or_else(|| panic!("{face:?} loads"));
            for c in ['0', 'A', 'z', '″', '×', 'Δ', '→', '·', '−', '°', '▾'] {
                assert!(f.glyph(c).is_some_and(|g| g != 0), "{face:?} has {c}");
            }
        }
    }

    #[test]
    fn the_default_overlay_colours_have_3_to_1_contrast_with_the_page_and_the_canvas() {
        // The selection is a light fill text sits on, not a line or mark: its check is black text's contrast over it
        // (`the_default_selection_keeps_black_text_readable`).
        let p = Palette::default();
        for (name, c) in [("object", p.object), ("drop", p.drop), ("marks", p.marks), ("leaders", p.leaders)] {
            for back in [Color::WHITE, rgb8(0xf3, 0xf3, 0xf3)] {
                let (a, b) = (luminance(c), luminance(back));
                let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                assert!(ratio >= 3.0, "{name} against {back:?}: {ratio:.2}");
            }
        }
    }
}
