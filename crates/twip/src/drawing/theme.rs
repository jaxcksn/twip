//! Theme and colour map

use roxmltree::Node;

use super::colour::{Rgba, SchemeSlot};
use super::paint::{EffectList, Fill, Line};
use crate::docx::xml::{A, W, is};

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub colours: [Rgba; 12],
    pub fills: Vec<Fill>,
    pub lines: Vec<Line>,
    pub effects: Vec<EffectList>,
    pub bg_fills: Vec<Fill>,
}

const DEFAULT_COLOURS: [u32; 12] =
    [0x000000, 0xFFFFFF, 0x0E2841, 0xE8E8E8, 0x156082, 0xE97132, 0x196B24, 0x0F9ED5, 0xA02B93, 0x4EA72E, 0x467886, 0x96607D];

const DEFAULT_FMT: &str = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:fmtScheme name="Office"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:lumMod val="110000"/><a:satMod val="105000"/><a:tint val="67000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:lumMod val="105000"/><a:satMod val="103000"/><a:tint val="73000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:lumMod val="105000"/><a:satMod val="109000"/><a:tint val="81000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:satMod val="103000"/><a:lumMod val="102000"/><a:tint val="94000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:satMod val="110000"/><a:lumMod val="100000"/><a:shade val="100000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:lumMod val="99000"/><a:satMod val="120000"/><a:shade val="78000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln><a:ln w="12700" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln><a:ln w="19050" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst><a:outerShdw blurRad="57150" dist="19050" dir="5400000" algn="ctr" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="63000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"><a:tint val="95000"/><a:satMod val="170000"/></a:schemeClr></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:tint val="93000"/><a:satMod val="150000"/><a:shade val="98000"/><a:lumMod val="102000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:tint val="98000"/><a:satMod val="130000"/><a:shade val="90000"/><a:lumMod val="103000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:shade val="63000"/><a:satMod val="120000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#;

impl Default for Theme {
    fn default() -> Theme {
        thread_local! {
            static DEFAULT: Theme = {
                let mut t = Theme { colours: DEFAULT_COLOURS.map(Rgba::rgb), fills: Vec::new(), lines: Vec::new(), effects: Vec::new(), bg_fills: Vec::new() };
                if let Ok(doc) = roxmltree::Document::parse(DEFAULT_FMT) {
                    t.format_scheme(doc.root_element());
                }
                t
            };
        }
        DEFAULT.with(Theme::clone)
    }
}

impl Theme {
    pub fn parse(xml: Option<&str>) -> Theme {
        let mut t = Theme::default();
        let Some(doc) = xml.and_then(|x| roxmltree::Document::parse(x).ok()) else { return t };
        if let Some(cs) = doc.descendants().find(|n| is(*n, A, "clrScheme")) {
            for (i, name) in ["dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hlink", "folHlink"]
                .into_iter()
                .enumerate()
            {
                let c = cs.children().find(|c| is(*c, A, name)).and_then(super::colour::Colour::child_of);
                if let Some(c) = c {
                    t.colours[i] = c.resolve(&Theme::default(), &ColourMap::default(), None);
                }
            }
        }
        t.format_scheme(doc.root_element());
        t
    }

    fn format_scheme(&mut self, root: Node) {
        if let Some(fs) = root.descendants().find(|n| is(*n, A, "fmtScheme")) {
            let list = |name: &str| fs.children().find(|c| is(*c, A, name)).into_iter().flat_map(|l| l.children().filter(Node::is_element));
            self.fills = list("fillStyleLst").filter_map(Fill::parse).collect();
            self.lines = list("lnStyleLst").map(Line::parse).collect();
            self.effects =
                list("effectStyleLst").map(|e| e.children().find(|c| is(*c, A, "effectLst")).map(EffectList::parse).unwrap_or_default()).collect();
            self.bg_fills = list("bgFillStyleLst").filter_map(Fill::parse).collect();
        }
    }

    pub fn colour(&self, slot: SchemeSlot) -> Rgba {
        slot.theme_index().map_or(Rgba::BLACK, |i| self.colours[i])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColourMap {
    pub bg1: SchemeSlot,
    pub tx1: SchemeSlot,
    pub bg2: SchemeSlot,
    pub tx2: SchemeSlot,
    pub accents: [SchemeSlot; 6],
    pub hlink: SchemeSlot,
    pub fol_hlink: SchemeSlot,
}

impl Default for ColourMap {
    fn default() -> ColourMap {
        use SchemeSlot::*;
        ColourMap {
            bg1: Lt1,
            tx1: Dk1,
            bg2: Lt2,
            tx2: Dk2,
            accents: [Accent(1), Accent(2), Accent(3), Accent(4), Accent(5), Accent(6)],
            hlink: Hlink,
            fol_hlink: FolHlink,
        }
    }
}

impl ColourMap {
    pub fn parse(settings_root: Option<Node>) -> ColourMap {
        let mut m = ColourMap::default();
        let Some(n) = settings_root.and_then(|r| r.children().find(|c| is(*c, W, "clrSchemeMapping"))) else { return m };
        let slot = |v: &str| {
            use SchemeSlot::*;
            Some(match v {
                "dark1" => Dk1,
                "light1" => Lt1,
                "dark2" => Dk2,
                "light2" => Lt2,
                "hyperlink" => Hlink,
                "followedHyperlink" => FolHlink,
                _ => Accent(v.strip_prefix("accent")?.parse().ok().filter(|n| (1..=6).contains(n))?),
            })
        };
        let get = |k: &str| n.attribute((W, k)).and_then(slot);
        m.bg1 = get("bg1").unwrap_or(m.bg1);
        m.tx1 = get("t1").unwrap_or(m.tx1);
        m.bg2 = get("bg2").unwrap_or(m.bg2);
        m.tx2 = get("t2").unwrap_or(m.tx2);
        for i in 0..6 {
            m.accents[i] = get(&format!("accent{}", i + 1)).unwrap_or(m.accents[i]);
        }
        m.hlink = get("hyperlink").unwrap_or(m.hlink);
        m.fol_hlink = get("followedHyperlink").unwrap_or(m.fol_hlink);
        m
    }

    pub fn map(&self, s: SchemeSlot) -> SchemeSlot {
        use SchemeSlot::*;
        match s {
            Bg1 => self.bg1,
            Tx1 => self.tx1,
            Bg2 => self.bg2,
            Tx2 => self.tx2,
            Accent(n) => self.accents[(n as usize).clamp(1, 6) - 1],
            Hlink => self.hlink,
            FolHlink => self.fol_hlink,
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THEME: &str = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements>
        <a:clrScheme name="T"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
        <a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1>
        <a:accent2><a:srgbClr val="ED7D31"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4>
        <a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink>
        <a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme>
        <a:fmtScheme name="T"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:noFill/></a:fillStyleLst>
        <a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst>
        <a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst>
        </a:fmtScheme></a:themeElements></a:theme>"#;

    #[test]
    fn a_theme_gives_its_colours_and_style_lists() {
        let t = Theme::parse(Some(THEME));
        assert_eq!(t.colour(SchemeSlot::Accent(1)), Rgba::rgb(0x4472C4));
        assert_eq!(t.colour(SchemeSlot::Dk2), Rgba::rgb(0x44546A));
        assert_eq!((t.fills.len(), t.lines.len(), t.effects.len(), t.bg_fills.len()), (2, 1, 1, 1));
        assert_eq!(Theme::parse(None).colour(SchemeSlot::Accent(1)), Rgba::rgb(0x156082));
    }

    #[test]
    fn the_colour_map_remaps_background_and_text() {
        let xml = r#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:clrSchemeMapping w:bg1="dark1" w:t1="light1"/></w:settings>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let m = ColourMap::parse(Some(doc.root_element()));
        assert_eq!(m.map(SchemeSlot::Bg1), SchemeSlot::Dk1);
        assert_eq!(m.map(SchemeSlot::Tx1), SchemeSlot::Lt1);
        assert_eq!(m.map(SchemeSlot::Accent(3)), SchemeSlot::Accent(3));
    }
}
