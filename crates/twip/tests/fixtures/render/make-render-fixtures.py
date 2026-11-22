"""Writes the rendering feature fixtures into tests/fixtures/render/ (next to this file).

One document per drawing feature, holding every value the feature reads, so a change to how it is drawn shows in
that document's render and PDF snapshots and nowhere else. The packages are the layout generator's (Pkg, sect, the
text helpers); the XML is what Word writes, in schema order.

Standard library only, no randomness: the same bytes on every run. Needs Python 3.12 or newer, like the layout
generator it builds on. Run: python3 -I crates/twip/tests/fixtures/render/make-render-fixtures.py [out dir]
"""

import importlib.util
import struct
import sys
import zlib
from pathlib import Path
from xml.sax.saxutils import escape

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("make_layout_fixtures", HERE.parent / "layout" / "make-layout-fixtures.py")
lf = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(lf)
mf = lf.mf

r, p, P, filler, fld = mf.r, mf.p, mf.P, mf.filler, mf.fld
Pkg, sect, section_break, cell, row = lf.Pkg, lf.sect, lf.section_break, lf.cell, lf.row
NS, XML_DECL, REL, EMU = lf.NS, lf.XML_DECL, lf.REL, lf.EMU

TIGHT_CELL = '<w:spacing w:after="0"/>'
TIGHT = '<w:spacing w:after="40" w:line="240" w:lineRule="auto"/>'
ACCENT = "1F4E79"

THEME = (
    f'{XML_DECL}<a:theme xmlns:a="{NS["a"]}" name="Office Theme"><a:themeElements><a:clrScheme name="Office">'
    '<a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>'
    '<a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2>'
    '<a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2>'
    '<a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4>'
    '<a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6>'
    '<a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme>'
    '<a:fontScheme name="Office"><a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont>'
    '<a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>'
    '<a:fmtScheme name="Office"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill>'
    '<a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst>'
    '<a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln>'
    '<a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln>'
    '<a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst>'
    '<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle>'
    '<a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>'
    '<a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill>'
    '<a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>'
)


def pkg():
    """A package with the theme part, so `themeColor` and `schemeClr` resolve. The body font is JetBrains Mono, the one
    face with real glyphs in the render gates' fonts, so the pixels show glyph shape."""
    d = Pkg(15)
    d.styles = d.styles.replace('w:ascii="Calibri" w:hAnsi="Calibri" w:eastAsia="Calibri" w:cs="Calibri"', 'w:ascii="JetBrains Mono" w:hAnsi="JetBrains Mono" w:eastAsia="JetBrains Mono" w:cs="JetBrains Mono"', 1)
    d.parts.append(("word/theme/theme1.xml", THEME, "application/vnd.openxmlformats-officedocument.theme+xml"))
    d.rels.append(("rIdTheme", REL + "theme", "theme/theme1.xml"))
    return d


def add_styles(d, *styles):
    d.styles = d.styles.replace("</w:styles>", "".join(styles) + "</w:styles>")


run = r


def line(content, ppr=""):
    """A tightly spaced paragraph."""
    return p(content, ppr + TIGHT)


def heading(text):
    return P(text, mf.HEADING)


def pattern_png(a, b, size=8):
    """A size x size two-colour checker PNG with 2x2 cells, the tile of a pattern or tiled picture fill."""
    rows = b"".join(
        b"\x00" + b"".join(bytes(a if ((x // 2) + (y // 2)) % 2 else b) for x in range(size)) for y in range(size)
    )

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")


def diagonal_png(size=8, width=3):
    """The 8 x 8 black-and-white tile of Word's wide upward diagonal pattern: black where x + y falls in `width` of every
    `size` diagonals. Word recolours black with the fill colour and white with color2."""
    rows = b"".join(
        b"\x00" + b"".join(b"\x00\x00\x00" if (x + y) % size < width else b"\xff\xff\xff" for x in range(size)) for y in range(size)
    )

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")


def extra_image(d, name, data):
    """Adds an image part under word/media and returns its relationship id."""
    if ("png", "image/png") not in d.defaults:
        d.defaults.append(("png", "image/png"))
    d.parts.append((f"word/media/{name}", data, None))
    return d.rel("image", f"media/{name}")


# ---- underlines ---------------------------------------------------------------------------------------------------

UNDERLINES = [
    "single", "words", "double", "thick", "dotted", "dottedHeavy", "dash", "dashedHeavy", "dashLong", "dashLongHeavy",
    "dotDash", "dashDotHeavy", "dotDotDash", "dashDotDotHeavy", "wave", "wavyHeavy", "wavyDouble", "none",
]


def underlines(d):
    out = [heading("Underline styles")]
    for val in UNDERLINES:
        out.append(line(
            run(f"{val:<18}")
            + run("Automatic colour sample", f'<w:u w:val="{val}"/>')
            + run("  ")
            + run("Coloured sample", f'<w:u w:val="{val}" w:color="C00000"/>')
        ))
    # `words` underlines only the words: tabs and trailing spaces stay bare.
    out.append(line(run("words, tabs and spaces", "<w:b/>")))
    out.append(line(
        run("alpha", '<w:u w:val="words"/>') + '<w:r><w:rPr><w:u w:val="words"/></w:rPr><w:tab/><w:t xml:space="preserve">beta gamma   </w:t></w:r>'
        + '<w:r><w:rPr><w:u w:val="words"/></w:rPr><w:tab/><w:t xml:space="preserve">delta    </w:t></w:r>' + run("end", '<w:u w:val="words"/>')
    ))
    out.append(line(
        run("alpha", '<w:u w:val="single"/>') + '<w:r><w:rPr><w:u w:val="single"/></w:rPr><w:tab/><w:t xml:space="preserve">beta gamma   </w:t></w:r>'
        + '<w:r><w:rPr><w:u w:val="single"/></w:rPr><w:tab/><w:t xml:space="preserve">delta    </w:t></w:r>' + run("end", '<w:u w:val="single"/>')
    ))
    return "".join(out), sect()


# ---- character effects --------------------------------------------------------------------------------------------

EFFECTS = ["emboss", "imprint", "outline", "shadow"]


def char_effects(d):
    out = [heading("Character effects")]
    for shade, label in ((None, "white"), ("D9D9D9", "shaded")):
        ppr = f'<w:shd w:val="clear" w:color="auto" w:fill="{shade}"/>' if shade else ""
        for eff in EFFECTS:
            e = f"<w:{eff}/>"
            runs = [
                run(f"{eff} {label} ", ""),
                run("Plain", e),
                run("  "),
                run("Bold", f"<w:b/>{e}"),
                run("  "),
                run("Colour", f'{e}<w:color w:val="C00000"/>'),
                run("  "),
                run("Underline", f'{e}<w:u w:val="single"/>'),
                run("  "),
                run("All", f'<w:b/>{e}<w:color w:val="2E75B6"/><w:sz w:val="32"/><w:szCs w:val="32"/><w:u w:val="single"/>'),
            ]
            out.append(p("".join(runs), ppr + '<w:spacing w:before="60" w:after="60"/>'))
    return "".join(out), sect()


# ---- text effects -------------------------------------------------------------------------------------------------

GLOW = '<w14:glow w14:rad="101600"><w14:schemeClr w14:val="accent1"><w14:alpha w14:val="40000"/><w14:satMod w14:val="175000"/></w14:schemeClr></w14:glow>'
SHADOW = (
    '<w14:shadow w14:blurRad="50800" w14:dist="38100" w14:dir="2700000" w14:sx="100000" w14:sy="100000" w14:kx="0" w14:ky="0" w14:algn="tl">'
    '<w14:srgbClr w14:val="000000"><w14:alpha w14:val="60000"/></w14:srgbClr></w14:shadow>'
)
REFLECTION = (
    '<w14:reflection w14:blurRad="6350" w14:stA="52000" w14:stPos="0" w14:endA="300" w14:endPos="35000" w14:dist="0" w14:dir="5400000" '
    'w14:fadeDir="5400000" w14:sx="100000" w14:sy="-100000" w14:kx="0" w14:ky="0" w14:algn="bl"/>'
)
OUTLINE = (
    '<w14:textOutline w14:w="9525" w14:cap="flat" w14:cmpd="sng" w14:algn="ctr"><w14:solidFill><w14:srgbClr w14:val="C00000"/></w14:solidFill>'
    '<w14:prstDash w14:val="solid"/><w14:round/></w14:textOutline>'
)
FILL_SOLID = '<w14:textFill><w14:solidFill><w14:srgbClr w14:val="2E75B6"/></w14:solidFill></w14:textFill>'
FILL_GRADIENT = (
    '<w14:textFill><w14:gradFill><w14:gsLst><w14:gs w14:pos="0"><w14:srgbClr w14:val="C00000"/></w14:gs>'
    '<w14:gs w14:pos="100000"><w14:srgbClr w14:val="2E75B6"/></w14:gs></w14:gsLst><w14:lin w14:ang="5400000" w14:scaled="0"/></w14:gradFill></w14:textFill>'
)
FILL_NONE = '<w14:textFill><w14:noFill/></w14:textFill>'
SCENE = (
    '<w14:scene3d><w14:camera w14:prst="orthographicFront"/><w14:lightRig w14:rig="threePt" w14:dir="t"/></w14:scene3d>'
    '<w14:props3d w14:extrusionH="57150"><w14:bevelT w14:w="63500" w14:h="25400"/></w14:props3d>'
)
TEXT_EFFECTS = [
    ("glow", GLOW), ("shadow", SHADOW), ("reflection", REFLECTION), ("outline", OUTLINE), ("fill-solid", FILL_SOLID),
    ("fill-gradient", FILL_GRADIENT), ("fill-none", FILL_NONE), ("scene3d", SCENE),
    ("all", GLOW + SHADOW + REFLECTION + OUTLINE + FILL_GRADIENT + SCENE),
]
BIG = '<w:b/><w:sz w:val="44"/><w:szCs w:val="44"/>'


def text_effects(d):
    styles = []
    out = [heading("Text effects")]
    for name, xml in TEXT_EFFECTS:
        styles.append(
            f'<w:style w:type="character" w:customStyle="1" w:styleId="Fx{name.title().replace("-", "")}"><w:name w:val="Fx {name}"/>'
            f"<w:basedOn w:val=\"DefaultParagraphFont\"/><w:qFormat/><w:rPr>{xml}</w:rPr></w:style>"
        )
    d.styles = d.styles.replace(f'<w:styles xmlns:w="{NS["w"]}">', f'<w:styles xmlns:w="{NS["w"]}" xmlns:mc="{NS["mc"]}" xmlns:w14="{NS["w14"]}" mc:Ignorable="w14">', 1)
    add_styles(d, '<w:style w:type="character" w:default="1" w:styleId="DefaultParagraphFont"><w:name w:val="Default Paragraph Font"/><w:uiPriority w:val="1"/><w:semiHidden/></w:style>', *styles)
    for name, xml in TEXT_EFFECTS:
        direct = run(f"{name} direct", BIG + xml)
        styled = run(f"{name} style", f'<w:rStyle w:val="Fx{name.title().replace("-", "")}"/>{BIG}')
        out.append(p(direct + run("   ") + styled, '<w:spacing w:before="120" w:after="240"/>'))
    return "".join(out), sect()


# ---- shading patterns ---------------------------------------------------------------------------------------------

SHD_VALUES = (
    "nil clear solid horzStripe vertStripe reverseDiagStripe diagStripe horzCross diagCross thinHorzStripe thinVertStripe "
    "thinReverseDiagStripe thinDiagStripe thinHorzCross thinDiagCross pct5 pct10 pct12 pct15 pct20 pct25 pct30 pct35 pct37 "
    "pct40 pct45 pct50 pct55 pct60 pct62 pct65 pct70 pct75 pct80 pct85 pct87 pct90 pct95"
).split()
PAIRS = (("C00000", "FFFF00"), ("2E75B6", "A9D18E"))


def shading_patterns(d):
    widths = [1560] + [1300] * 6
    small = '<w:sz w:val="18"/><w:szCs w:val="18"/>'

    def shd(val, pair):
        return f'<w:shd w:val="{val}" w:color="{pair[0]}" w:fill="{pair[1]}"/>'

    rows = []
    for val in SHD_VALUES:
        cells = [cell(P(val, '<w:spacing w:after="0"/>'), widths[0])]
        for pair in PAIRS:
            cells.append(cell(p(run("Run text", small + shd(val, pair)), '<w:spacing w:after="0"/>'), 1300))
            cells.append(cell(p(run("Paragraph", small), shd(val, pair) + '<w:spacing w:after="0"/>'), 1300))
            cells.append(cell(p(run("Cell", small), '<w:spacing w:after="0"/>'), 1300, shd(val, pair)))
        rows.append(row(cells, "<w:cantSplit/>"))
    grid = "".join(f'<w:gridCol w:w="{w}"/>' for w in widths)
    pr = f'<w:tblW w:w="{sum(widths)}" w:type="dxa"/>{lf.TBL_BORDERS}<w:tblLayout w:type="fixed"/>'
    body = heading("Shading patterns") + f"<w:tbl><w:tblPr>{pr}</w:tblPr><w:tblGrid>{grid}</w:tblGrid>{''.join(rows)}</w:tbl>"
    return body, sect()


# ---- table shading ------------------------------------------------------------------------------------------------


def table_style():
    border = lambda side: f'<w:{side} w:val="single" w:sz="4" w:space="0" w:color="{ACCENT}"/>'  # noqa: E731
    sides = "".join(border(s) for s in ("top", "left", "bottom", "right", "insideH", "insideV"))
    fill = lambda c: f'<w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="{c}"/></w:tcPr>'  # noqa: E731
    return (
        '<w:style w:type="table" w:styleId="BandedGrid"><w:name w:val="Banded Grid"/><w:basedOn w:val="TableNormal"/><w:uiPriority w:val="59"/>'
        f'<w:tblPr><w:tblStyleRowBandSize w:val="1"/><w:tblStyleColBandSize w:val="1"/><w:tblBorders>{sides}</w:tblBorders></w:tblPr>'
        f'<w:tblStylePr w:type="firstRow"><w:rPr><w:b/><w:color w:val="FFFFFF"/></w:rPr>{fill(ACCENT)}</w:tblStylePr>'
        f'<w:tblStylePr w:type="lastRow"><w:rPr><w:b/></w:rPr>{fill("FFD966")}</w:tblStylePr>'
        f'<w:tblStylePr w:type="band1Vert">{fill("DEEAF6")}</w:tblStylePr>'
        f'<w:tblStylePr w:type="band1Horz">{fill("E2EFDA")}</w:tblStylePr>'
        f'<w:tblStylePr w:type="band2Horz">{fill("FCE4D6")}</w:tblStylePr></w:style>'
    )


def shaded_table(rows, widths, shd="", style="", spacing=None, caption=""):
    pr = f'<w:tblStyle w:val="{style}"/>' if style else ""
    pr += f'<w:tblW w:w="{sum(widths)}" w:type="dxa"/>'
    pr += f'<w:tblCellSpacing w:w="{spacing}" w:type="dxa"/>' if spacing is not None else ""
    if not style:
        pr += lf.TBL_BORDERS
    pr += shd + '<w:tblLayout w:type="fixed"/>'
    if style:
        pr += '<w:tblLook w:val="0060" w:firstRow="1" w:lastRow="1" w:firstColumn="0" w:lastColumn="0" w:noHBand="0" w:noVBand="0"/>'
    grid = "".join(f'<w:gridCol w:w="{w}"/>' for w in widths)
    return f"<w:tbl><w:tblPr>{pr}</w:tblPr><w:tblGrid>{grid}</w:tblGrid>{''.join(rows)}</w:tbl>" + p()


def text_row(texts, widths, ex="", fills=None):
    cells = []
    for i, (t, w) in enumerate(zip(texts, widths)):
        f = fills[i] if fills else None
        pr = f'<w:shd w:val="clear" w:color="auto" w:fill="{f}"/>' if f else ""
        cells.append(cell(P(t, '<w:spacing w:after="0"/>'), w, pr))
    return row(cells, ex=ex)


def table_shading(d):
    add_styles(d, table_style())
    w = [2340] * 4
    rows = lambda n: [text_row([f"R{i} C{j}" for j in range(4)], w) for i in range(n)]  # noqa: E731
    fill = lambda c: f'<w:shd w:val="clear" w:color="auto" w:fill="{c}"/>'  # noqa: E731
    out = [heading("Table shading")]
    out.append(P("Style with first row, last row, banded rows and columns"))
    out.append(shaded_table(rows(6), w, style="BandedGrid"))
    out.append(P("tblPr shd"))
    out.append(shaded_table(rows(3), w, shd=fill("FFF2CC")))
    out.append(P("tblPrEx shd on the middle row"))
    mid = [text_row([f"R{i} C{j}" for j in range(4)], w, ex=fill("BDD7EE") if i == 1 else "") for i in range(3)]
    out.append(shaded_table(mid, w, shd=fill("FFF2CC")))
    out.append(P("Direct cell shading over the table shading and a pattern"))
    direct = [
        text_row(["a", "b", "c", "d"], w, fills=["C6E0B4", None, "F4B183", None]),
        row([
            cell(P("pct25", '<w:spacing w:after="0"/>'), 2340, '<w:shd w:val="pct25" w:color="C00000" w:fill="FFFFFF"/>'),
            cell(P("none over table", '<w:spacing w:after="0"/>'), 2340),
            cell(P("diagCross", '<w:spacing w:after="0"/>'), 2340, '<w:shd w:val="diagCross" w:color="2E75B6" w:fill="FFFF00"/>'),
            cell(P("nil", '<w:spacing w:after="0"/>'), 2340, '<w:shd w:val="nil" w:color="auto" w:fill="auto"/>'),
        ]),
    ]
    out.append(shaded_table(direct, w, shd=fill("FFF2CC")))
    out.append(P("Cell spacing on a shaded table"))
    out.append(shaded_table([text_row([f"S{i}{j}" for j in range(4)], w, fills=["E2EFDA"] * 4) for i in range(3)], w, shd=fill("8EAADB"), spacing=60))
    return "".join(out), sect()


# ---- borders ------------------------------------------------------------------------------------------------------

LINE_STYLES = (
    "single thick double dotted dashed dotDash dotDotDash triple thinThickSmallGap thickThinSmallGap thinThickThinSmallGap "
    "thinThickMediumGap thickThinMediumGap thinThickThinMediumGap thinThickLargeGap thickThinLargeGap thinThickThinLargeGap "
    "wave doubleWave dashSmallGap dashDotStroked threeDEmboss threeDEngrave outset inset"
).split()


# Widths (eighths of a point) Word's dialog offers for the wave and stroked styles; the others take 1.5 pt.
WIDTHS = {"wave": 6, "doubleWave": 6, "dashDotStroked": 24}


def side(name, val, sz=None, space=4, extra=""):
    sz = sz or WIDTHS.get(val, 12)
    return f'<w:{name} w:val="{val}" w:sz="{sz}" w:space="{space}" w:color="{ACCENT}"{extra}/>'


def box(val, extra=""):
    return "<w:pBdr>" + "".join(side(s, val, extra=extra) for s in ("top", "left", "bottom", "right")) + "</w:pBdr>"


def borders(d):
    out = [heading("Borders")]
    for val in LINE_STYLES:
        out.append(p(run(val), box(val) + '<w:spacing w:before="80" w:after="80"/><w:ind w:left="240" w:right="240"/>'))
    # w:shadow on a paragraph box, on text, and (via the section) on the page.
    out.append(p(run("Paragraph box with shadow"), box("single", ' w:shadow="1"') + '<w:spacing w:before="160" w:after="160"/><w:ind w:left="240" w:right="240"/>'))
    out.append(line(
        run("Text border with shadow ") + run("shadowed run", f'<w:bdr w:val="single" w:sz="8" w:space="0" w:color="{ACCENT}" w:shadow="1"/>')
        + run(" and a plain one ") + run("plain run", f'<w:bdr w:val="single" w:sz="8" w:space="0" w:color="{ACCENT}"/>')
    ))
    # A grid of cells, one line style each, kept apart by cell spacing so no border collapses into its neighbour.
    widths = [1800] * 5
    cells = []
    for val in LINE_STYLES:
        bd = "<w:tcBorders>" + "".join(side(s, val, space=0) for s in ("top", "left", "bottom", "right")) + "</w:tcBorders>"
        cells.append(cell(P(val, '<w:spacing w:after="0"/><w:jc w:val="center"/>'), 1800, bd))
    cells += [cell(P(""), 1800)] * (-len(cells) % 5)
    grid_rows = [row(cells[i:i + 5], "<w:cantSplit/>") for i in range(0, len(cells), 5)]
    nob = "<w:tblBorders>" + "".join(f'<w:{s} w:val="nil"/>' for s in ("top", "left", "bottom", "right", "insideH", "insideV")) + "</w:tblBorders>"
    out.append(P("Table cell borders, one line style per cell"))
    out.append(lf.table(grid_rows, widths, spacing=60, borders=nob))
    # Whole-table borders in the four styles that are not plain lines.
    for val in ("wave", "doubleWave", "dashDotStroked", "dashSmallGap"):
        tb = "<w:tblBorders>" + "".join(side(s, val, space=0) for s in ("top", "left", "bottom", "right", "insideH", "insideV")) + "</w:tblBorders>"
        out.append(P(f"tblBorders {val}"))
        out.append(lf.simple([["one", "two"], ["three", "four"]], [2400, 2400], borders=tb))
    page = "<w:pgBorders w:offsetFrom=\"page\">" + "".join(side(s, "single", 12, 24, ' w:shadow="1"') for s in ("top", "left", "bottom", "right")) + "</w:pgBorders>"
    return "".join(out), sect(borders=page)


# ---- diagonal cell borders ----------------------------------------------------------------------------------------


def diag(val="single", sz=None, color=ACCENT, which=("tl2br", "tr2bl")):
    sz = sz or WIDTHS.get(val, 12)
    return "".join(f'<w:{d} w:val="{val}" w:sz="{sz}" w:space="0" w:color="{color}"/>' for d in which)


def tc_borders(inner):
    return f"<w:tcBorders>{inner}</w:tcBorders>"


def diagonal_style():
    """A table style whose first row and first column carry a diagonal each."""
    sides = "".join(lf.bdr(s) for s in ("top", "left", "bottom", "right", "insideH", "insideV"))
    part = lambda kind, d: f'<w:tblStylePr w:type="{kind}"><w:tcPr>{tc_borders(d)}</w:tcPr></w:tblStylePr>'  # noqa: E731
    return (
        '<w:style w:type="table" w:styleId="DiagonalGrid"><w:name w:val="Diagonal Grid"/><w:basedOn w:val="TableNormal"/><w:uiPriority w:val="59"/>'
        f"<w:tblPr><w:tblBorders>{sides}</w:tblBorders></w:tblPr>"
        + part("firstRow", diag("single", 8, "C00000", ("tl2br",)))
        + part("firstCol", diag("dashed", 8, "2E75B6", ("tr2bl",)))
        + "</w:style>"
    )


def diagonals(d):
    add_styles(d, diagonal_style())
    tight = '<w:spacing w:after="0"/>'
    tall = '<w:trHeight w:val="700"/><w:cantSplit/>'
    out = [heading("Diagonal cell borders")]
    # One diagonal, the other, both; over shading and text.
    w = [2340] * 4
    out.append(P("tl2br, tr2bl, both, both over shading"))
    cells = [
        cell(P("Down", tight), 2340, tc_borders(diag(which=("tl2br",)))),
        cell(P("Up", tight), 2340, tc_borders(diag(which=("tr2bl",)))),
        cell(P("Both diagonals over text", tight), 2340, tc_borders(diag())),
        cell(P("Shaded", tight), 2340, tc_borders(diag(color="C00000")) + '<w:shd w:val="clear" w:color="auto" w:fill="FFE699"/>'),
    ]
    out.append(lf.table([row(cells, tall)], w))
    # Every line style.
    widths = [1872] * 5
    cells = [cell(P(val, tight + '<w:jc w:val="center"/>'), 1872, tc_borders(diag(val))) for val in LINE_STYLES]
    cells += [cell(P(""), 1872)] * (-len(cells) % 5)
    out.append(P("Both diagonals in each line style"))
    out.append(lf.table([row(cells[i:i + 5], tall) for i in range(0, len(cells), 5)], widths))
    # Merged cells: across two columns and down three rows, across columns only, down rows only.
    w = [1560] * 6
    plain = lambda t: cell(P(t, tight), 1560)  # noqa: E731
    m = tc_borders(diag("double", 4, "C00000"))
    rows = [
        row([cell(P("Merged 2 x 3", tight), 3120, '<w:gridSpan w:val="2"/><w:vMerge w:val="restart"/>' + m),
             cell(P("Across", tight), 3120, '<w:gridSpan w:val="2"/>' + tc_borders(diag("dotted", 8))),
             cell(P("Down", tight), 1560, '<w:vMerge w:val="restart"/>' + tc_borders(diag("thick", 8))), plain("a")], tall),
        row([cell(P(""), 3120, '<w:gridSpan w:val="2"/><w:vMerge/>'), plain("b"), plain("c"), cell(P(""), 1560, "<w:vMerge/>"), plain("d")], tall),
        row([cell(P(""), 3120, '<w:gridSpan w:val="2"/><w:vMerge/>'), plain("e"), plain("f"), plain("g"), plain("h")], tall),
    ]
    out.append(P("Merged cells: two columns by three rows, two columns, two rows"))
    out.append(lf.table(rows, w))
    # Thick cell borders around a diagonal, and a spaced table's cells.
    thick = "".join(lf.bdr(s, sz=36) for s in ("top", "left", "bottom", "right"))
    out.append(P("Thick cell borders"))
    out.append(lf.table([row([cell(P("4.5 pt sides", tight), 2340, tc_borders(thick + diag("single", 4, "000000"))), plain("x")], tall)], [2340, 1560]))
    out.append(P("Cell spacing"))
    out.append(lf.table([row([cell(P("Spaced", tight), 2340, tc_borders(diag())), cell(P("Spaced", tight), 2340, tc_borders(diag("dashSmallGap", 12)))], tall)],
                        [2340, 2340], spacing=100))
    # A table style's conditional formats.
    look = '<w:tblLook w:val="00A0" w:firstRow="1" w:lastRow="0" w:firstColumn="1" w:lastColumn="0" w:noHBand="1" w:noVBand="1"/>'
    out.append(P("Table style: the first row's tl2br, the first column's tr2bl"))
    out.append(lf.table([row([plain(f"S{i}{j}") for j in range(3)], tall) for i in range(3)], [1560] * 3, style="DiagonalGrid", borders="", look=look))
    return "".join(out), sect()


# ---- watermarks ---------------------------------------------------------------------------------------------------

SHAPETYPE_136 = (
    '<v:shapetype id="_x0000_t136" coordsize="21600,21600" o:spt="136" adj="10800" path="m@7,l@8,m@5,21600l@6,21600e"><v:formulas>'
    '<v:f eqn="sum #0 0 10800"/><v:f eqn="prod #0 2 1"/><v:f eqn="sum 21600 0 @1"/><v:f eqn="sum 0 0 @2"/><v:f eqn="sum 21600 0 @3"/>'
    '<v:f eqn="if @0 @3 0"/><v:f eqn="if @0 21600 @1"/><v:f eqn="if @0 0 @2"/><v:f eqn="if @0 @4 21600"/><v:f eqn="mid @5 @6"/>'
    '<v:f eqn="mid @8 @5"/><v:f eqn="mid @7 @8"/><v:f eqn="mid @6 @7"/><v:f eqn="sum @6 0 @5"/></v:formulas>'
    '<v:path textpathok="t" o:connecttype="custom" o:connectlocs="@9,0;@10,10800;@11,21600;@12,10800" o:connectangles="270,180,90,0"/>'
    '<v:textpath on="t" fitshape="t"/><v:handles><v:h position="#0,bottomRight" xrange="6629,14971"/></v:handles>'
    '<o:lock v:ext="edit" text="t" shapetype="t"/></v:shapetype>'
)
SHAPETYPE_75 = (
    '<v:shapetype id="_x0000_t75" coordsize="21600,21600" o:spt="75" o:preferrelative="t" path="m@4@5l@4@11@9@11@9@5xe" filled="f" stroked="f">'
    '<v:stroke joinstyle="miter"/><v:formulas><v:f eqn="if lineDrawn pixelLineWidth 0"/><v:f eqn="sum @0 1 0"/><v:f eqn="sum 0 0 @1"/>'
    '<v:f eqn="prod @2 1 2"/><v:f eqn="prod @3 21600 pixelWidth"/><v:f eqn="prod @3 21600 pixelHeight"/><v:f eqn="sum @0 0 1"/>'
    '<v:f eqn="prod @6 1 2"/><v:f eqn="prod @7 21600 pixelWidth"/><v:f eqn="sum @8 21600 0"/><v:f eqn="prod @7 21600 pixelHeight"/>'
    '<v:f eqn="sum @10 21600 0"/></v:formulas><v:path o:extrusionok="f" gradientshapeok="t" o:connecttype="rect"/>'
    '<o:lock v:ext="edit" aspectratio="t"/></v:shapetype>'
)
CENTRED = (
    "mso-position-horizontal:center;mso-position-horizontal-relative:margin;"
    "mso-position-vertical:center;mso-position-vertical-relative:margin"
)


def watermark_text(n, text, w, h, rotation):
    rot = f"rotation:{rotation};" if rotation else ""
    return (
        '<w:p><w:pPr><w:pStyle w:val="Header"/></w:pPr><w:r><w:rPr><w:noProof/></w:rPr><w:pict>'
        f'{SHAPETYPE_136}<v:shape id="PowerPlusWaterMarkObject{357476640 + n}" o:spid="_x0000_s{2048 + n}" type="#_x0000_t136" alt="" '
        f'style="position:absolute;margin-left:0;margin-top:0;width:{w}pt;height:{h}pt;{rot}z-index:-251658752;{CENTRED}" '
        f'o:allowincell="f" fillcolor="silver" stroked="f"><v:fill opacity=".5"/>'
        f'<v:textpath style="font-family:&quot;Calibri&quot;;font-size:1pt" string="{text}"/><w10:wrap anchorx="margin" anchory="margin"/>'
        "</v:shape></w:pict></w:r></w:p>"
    )


def watermark_picture(n, rid, w, h, washout):
    wash = ' gain="19661f" blacklevel="22938f"' if washout else ""
    return (
        '<w:p><w:pPr><w:pStyle w:val="Header"/></w:pPr><w:r><w:rPr><w:noProof/></w:rPr><w:pict>'
        f'{SHAPETYPE_75}<v:shape id="WordPictureWatermark{357476650 + n}" o:spid="_x0000_s{2060 + n}" type="#_x0000_t75" alt="" '
        f'style="position:absolute;margin-left:0;margin-top:0;width:{w}pt;height:{h}pt;z-index:-251657728;{CENTRED}" o:allowincell="f">'
        f'<v:imagedata r:id="{rid}" o:title="watermark"{wash}/><w10:wrap anchorx="margin" anchory="margin"/></v:shape></w:pict></w:r></w:p>'
    )


def header_with_rels(d, content, rels=()):
    """A header part (and its relationships part when `rels` is given); returns the headerReference."""
    n = sum(1 for part in d.parts if part[0].startswith("word/header")) + 1
    ref = d.story("header", content)
    if rels:
        body = "".join(f'<Relationship Id="{i}" Type="{REL}{t}" Target="{target}"/>' for i, t, target in rels)
        d.parts.append((
            f"word/_rels/header{n}.xml.rels",
            f'{XML_DECL}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{body}</Relationships>',
            None,
        ))
    return ref


def watermark_text_doc(d):
    refs = [
        header_with_rels(d, watermark_text(1, "DRAFT", 527.85, 131.95, 315)),
        header_with_rels(d, watermark_text(2, "DRAFT", 468, 117, 0)),
    ]
    body = [heading("Diagonal DRAFT"), P(filler(70, 1)), P(filler(40, 2)), section_break(sect(refs[0], kind="nextPage"))]
    body += [heading("Horizontal DRAFT"), P(filler(70, 3)), P(filler(40, 4))]
    return "".join(body), sect(refs[1])


def watermark_picture_doc(d):
    d.image()
    refs = []
    for i, washout in enumerate((True, False), 1):
        # The header refers to the shared image part by its own relationship.
        refs.append(header_with_rels(d, watermark_picture(i, "rId1", 300, 225, washout), [("rId1", "image", "media/image1.png")]))
    body = [heading("Picture watermark with washout"), P(filler(70, 5)), P(filler(40, 6)), section_break(sect(refs[0], kind="nextPage"))]
    body += [heading("Picture watermark without washout"), P(filler(70, 7)), P(filler(40, 8))]
    return "".join(body), sect(refs[1])


# ---- page backgrounds ---------------------------------------------------------------------------------------------


PAGE_TINT = 'w:color="F4B183" w:themeColor="accent2" w:themeTint="99"'
PAGE_TINT_LABEL = "Page colour with a theme tint"


def page_body(label):
    return heading(label) + P(filler(80, 1)) + P(filler(60, 2))


def background(d, attrs="", vml="", shape=True):
    d.doc_head = f"<w:background {attrs}>{vml}</w:background>" if (attrs or vml) else ""
    if shape:
        d.settings_head += "<w:displayBackgroundShape/>"


def page_colour_with(attrs, label, shape=True):
    def build(d):
        background(d, attrs, shape=shape)
        return page_body(label), sect()
    return build


def vml_background(fill, fillcolor=None):
    colour = f' fillcolor="{fillcolor}"' if fillcolor else ""
    return f'<v:background id="_x0000_s1025" o:bwmode="white"{colour} o:targetscreensize="1024,768">{fill}</v:background>'


def page_gradient(d):
    fill = '<v:fill type="gradient" color2="#2e75b6" angle="-45" focus="50%" method="sigma"/>'
    background(d, 'w:color="FFFFFF"', vml_background(fill))
    return page_body("Gradient page background"), sect()


def page_gradient_stops(d):
    fill = '<v:fill type="gradient" color2="#2e75b6" colors="0 #ffffff;.5 #bdd7ee;1 #2e75b6" angle="90" focus="100%" method="sigma"/>'
    background(d, 'w:color="FFFFFF"', vml_background(fill))
    return page_body("Gradient page background with colour stops"), sect()


def page_pattern(d):
    # Foreground (fillcolor, repeated in w:background) and background (color2) recolour the black-and-white tile.
    rid = extra_image(d, "pattern1.png", diagonal_png())
    fill = f'<v:fill r:id="{rid}" o:title="Wide upward diagonal" color2="#ffe699" type="pattern"/>'
    background(d, 'w:color="2E75B6"', vml_background(fill, "#2e75b6"))
    return page_body("Pattern page background"), sect()


def page_picture(d):
    rid = extra_image(d, "tile1.png", pattern_png((0xED, 0x7D, 0x31), (0xFF, 0xE6, 0xCC)))
    fill = f'<v:fill r:id="{rid}" o:title="tile" recolor="t" rotate="t" type="tile"/>'
    background(d, 'w:color="FFFFFF"', vml_background(fill))
    return page_body("Tiled picture page background"), sect()


# ---- picture effects ----------------------------------------------------------------------------------------------


def blip_picture(d, blip, effects="", name="Picture", size=(1.6, 1.2)):
    i = d.uid()
    cx, cy = round(size[0] * EMU), round(size[1] * EMU)
    return (
        f'<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/>'
        f'<wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="{i}" name="{name} {i}"/><wp:cNvGraphicFramePr/>'
        '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic>'
        f'<pic:nvPicPr><pic:cNvPr id="{i}" name="image{i}.png"/><pic:cNvPicPr/></pic:nvPicPr>'
        f'<pic:blipFill>{blip}<a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
        f'<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{effects}</pic:spPr>'
        "</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
    )


def vml_image(d, attrs, with_type):
    i = d.uid()
    rid = d.image()
    return (
        f'<w:r><w:pict>{SHAPETYPE_75 if with_type else ""}'
        f'<v:shape id="_x0000_i{1024 + i}" type="#_x0000_t75" style="width:115.2pt;height:86.4pt">'
        f'<v:imagedata r:id="{rid}" o:title=""{attrs}/></v:shape></w:pict></w:r>'
    )


def picture_effects(d):
    d.image()
    rid = d.image_rid
    embed = f'r:embed="{rid}"'
    link = d.rel("image", "linked-picture.png")
    d.external.add(link)
    clr = '<a:clrChange><a:clrFrom><a:srgbClr val="2E75B6"/></a:clrFrom><a:clrTo><a:srgbClr val="2E75B6"><a:alpha val="0"/></a:srgbClr></a:clrTo></a:clrChange>'
    refl = '<a:effectLst><a:reflection blurRad="6350" stA="50000" endA="300" endPos="35000" dir="5400000" sy="-100000" algn="bl" rotWithShape="0"/></a:effectLst>'
    out = [heading("Picture effects")]
    out.append(P("DrawingML: colour change, reflection, luminance"))
    out.append(line(
        blip_picture(d, f"<a:blip {embed}>{clr}</a:blip>") + run("  ")
        + blip_picture(d, f"<a:blip {embed}/>", refl) + run("  ")
        + blip_picture(d, f'<a:blip {embed}><a:lum bright="40000" contrast="-30000"/></a:blip>')
    ))
    out.append(P("VML imagedata: gain and blacklevel, grayscale, bilevel"))
    shapes = []
    for k, attrs in enumerate((' gain="19661f" blacklevel="22938f"', ' gain="26214f" blacklevel="-6554f"', ' grayscale="t"', ' bilevel="t"')):
        # The shapetype is declared once, in the first VML picture.
        shapes.append(vml_image(d, attrs, k == 0) + run("  "))
    out.append(line("".join(shapes)))
    out.append(P("A blip with only r:link, and one with r:link and r:embed"))
    out.append(line(
        blip_picture(d, f'<a:blip r:link="{link}"/>', name="Linked") + run("  ")
        + blip_picture(d, f'<a:blip r:embed="{rid}" r:link="{link}"/>', name="Linked and embedded")
    ))
    return "".join(out), sect()


# ---- JPEG XR ------------------------------------------------------------------------------------------------------

# Pictures jxrlib encoded (through the imagecodecs Python package), shared with twip-graphics' tests.
JXR = HERE.parents[3] / "twip-graphics" / "tests" / "images"


def wdp_image(d, name):
    """Adds a JPEG XR part under word/media, as Word names them, and returns its relationship id."""
    if ("wdp", "image/vnd.ms-photo") not in d.defaults:
        d.defaults.append(("wdp", "image/vnd.ms-photo"))
    d.parts.append((f"word/media/{name}.wdp", (JXR / f"{name}.jxr").read_bytes(), None))
    return d.rel("image", f"media/{name}.wdp")


def jpeg_xr(d):
    rgb, gray, alpha, pre = (wdp_image(d, n) for n in ("rgb-420", "gray", "alpha", "premultiplied"))
    d.image()
    png = d.image_rid
    pencil = (
        '<a:extLst><a:ext uri="{BEBA8EAE-BF5A-486C-A8C5-ECC9F3942E4B}">'
        '<a14:imgProps xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main">'
        f'<a14:imgLayer r:embed="{gray}"><a14:imgEffect><a14:artisticPencilSketch/></a14:imgEffect></a14:imgLayer>'
        "</a14:imgProps></a:ext></a:extLst>"
    )
    shaded = '<w:shd w:val="clear" w:color="auto" w:fill="A0C4E8"/>'
    out = [heading("JPEG XR pictures")]
    out.append(P("RGB, lossy 4:2:0 with both overlap levels; grey, lossy"))
    out.append(line(
        blip_picture(d, f'<a:blip r:embed="{rgb}"/>', size=(1.2, 1.0)) + run("  ")
        + blip_picture(d, f'<a:blip r:embed="{gray}"/>', size=(0.94, 0.78))
    ))
    out.append(P("RGBA: interleaved alpha, and premultiplied alpha in its own plane, on a shaded paragraph"))
    out.append(line(
        blip_picture(d, f'<a:blip r:embed="{alpha}"/>', size=(1.7, 0.8)) + run("  ")
        + blip_picture(d, f'<a:blip r:embed="{pre}"/>', size=(1.7, 0.8)),
        shaded,
    ))
    out.append(P("A PNG whose artistic effect keeps its source as a JPEG XR layer: the PNG is drawn"))
    out.append(line(blip_picture(d, f'<a:blip r:embed="{png}">{pencil}</a:blip>', size=(0.8, 0.6))))
    return "".join(out), sect()


# ---- accessible PDF -----------------------------------------------------------------------------------------------

DECORATIVE = (
    '<a:extLst><a:ext uri="{C183D7F6-B498-43B3-948B-1728B52AA6E4}">'
    '<adec:decorative xmlns:adec="http://schemas.microsoft.com/office/drawing/2017/decorative" val="1"/></a:ext></a:extLst>'
)
HYPERLINK_STYLE = (
    '<w:style w:type="character" w:styleId="Hyperlink"><w:name w:val="Hyperlink"/><w:basedOn w:val="DefaultParagraphFont"/><w:uiPriority w:val="99"/>'
    '<w:unhideWhenUsed/><w:rPr><w:color w:val="0563C1"/><w:u w:val="single"/></w:rPr></w:style>'
    '<w:style w:type="character" w:default="1" w:styleId="DefaultParagraphFont"><w:name w:val="Default Paragraph Font"/><w:uiPriority w:val="1"/><w:semiHidden/></w:style>'
    '<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/>'
    '<w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="160" w:after="0"/><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:sz w:val="26"/><w:szCs w:val="26"/></w:rPr></w:style>'
    '<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/>'
    '<w:pPr><w:spacing w:after="120"/></w:pPr><w:rPr><w:sz w:val="52"/><w:szCs w:val="52"/></w:rPr></w:style>'
)


def pdf_accessible(d):
    add_styles(d, HYPERLINK_STYLE)
    link = d.rel("hyperlink", "https://example.com/report")
    d.external.add(link)
    linked = lambda text, attrs: f'<w:hyperlink {attrs} w:history="1"><w:r><w:rPr><w:rStyle w:val="Hyperlink"/></w:rPr><w:t>{escape(text)}</w:t></w:r></w:hyperlink>'  # noqa: E731
    out = [P("Accessible document", '<w:pStyle w:val="Title"/>'), heading("Links and cross references")]
    out.append(p(
        run("An external link to ") + linked("the report", f'r:id="{link}"') + run(", an internal link to ")
        + linked("the table", 'w:anchor="results"') + run(".")
    ))
    out.append(p(
        run("See ") + fld(" REF results \\h ", "Table results") + run(" on page ") + fld(" PAGEREF results \\h ", "1") + run(".")
    ))
    out.append(P("Heading by outline level", '<w:keepNext/><w:outlineLvl w:val="1"/><w:spacing w:before="160" w:after="0"/>'))
    out.append(P("Body text under a heading made only by its outline level."))
    out.append(P("Heading two by style", '<w:pStyle w:val="Heading2"/>'))
    out.append(p(run("A sentence with a footnote") + d.footnote("The footnote text.") + run(" and an endnote") + d.endnote("The endnote text.") + run(".")))
    out.append(p('<w:bookmarkStart w:id="0" w:name="results"/>' + run("Table results") + '<w:bookmarkEnd w:id="0"/>', "<w:keepNext/>"))
    # A table with a header row, header cell ids, tblCaption and tblDescription.
    widths = [3120, 3120, 3120]

    def hcell(text, ident):
        return (f'<w:tc w:id="{ident}"><w:tcPr><w:tcW w:w="3120" w:type="dxa"/></w:tcPr>{P(text, TIGHT_CELL)}</w:tc>')

    def dcell(text, *ids):
        refs = "<w:headers>" + "".join(f'<w:header w:val="{i}"/>' for i in ids) + "</w:headers>"
        return cell(P(text, '<w:spacing w:after="0"/>'), 3120, tail=refs)

    rows = [row([hcell("Region", "h1"), hcell("Q1", "h2"), hcell("Q2", "h3")], "<w:tblHeader/>")]
    for k, name in enumerate(("North", "South")):
        rows.append(row([dcell(name, "h1"), dcell(str(10 + k), "h2"), dcell(str(20 + k), "h3")]))
    grid = "".join(f'<w:gridCol w:w="{w}"/>' for w in widths)
    tbl_pr = (
        f'<w:tblW w:w="{sum(widths)}" w:type="dxa"/>{lf.TBL_BORDERS}<w:tblLayout w:type="fixed"/>'
        '<w:tblCaption w:val="Results"/><w:tblDescription w:val="Quarterly results by region"/>'
    )
    out.append(f"<w:tbl><w:tblPr>{tbl_pr}</w:tblPr><w:tblGrid>{grid}</w:tblGrid>{''.join(rows)}</w:tbl>")
    out.append(P("Figures", '<w:pStyle w:val="Heading2"/>'))
    figure = d.inline_picture(1.6, 1.2)
    out.append(p(figure.replace('name="Picture ', 'descr="A blue rectangle used as a figure" name="Picture ', 1)))
    out.append(p(d.inline_picture(1.6, 1.2).replace('/><wp:cNvGraphicFramePr/>', f'>{DECORATIVE}</wp:docPr><wp:cNvGraphicFramePr/>', 1)))
    floating = d.picture(1.2, 0.9, ("margin", 3.4), ("paragraph", 0.0))
    floating = floating.replace('name="Picture ', 'descr="A floating blue rectangle" name="Picture ', 1)
    out.append(p(floating + run("Text beside a floating picture with alternative text. " + filler(30, 4))))
    return "".join(out), sect()


MONO = lf.font("JetBrains Mono")
SAMPLE = "Sphinx of black quartz, judge my vow 0123456789"
B, I = mf.B, mf.I


def text_glyphs(d):
    sizes = [8, 9, 10, 11, 12, 14, 18, 24, 36, 48, 72]
    body = [p(r(f"{s} pt " + (SAMPLE if s <= 24 else "Rag & 08"), f'{MONO}<w:sz w:val="{s * 2}"/>')) for s in sizes]
    body.append(p(r("Regular, ", MONO) + r("bold, ", MONO + B) + r("italic, ", MONO + I) + r("bold italic.", MONO + B + I)))
    colours = ["C00000", "2E75B6", "00B050", "7030A0", "FFC000"]
    body.append(p("".join(r(f"{c} ", f'{MONO}<w:color w:val="{c}"/><w:sz w:val="28"/>') for c in colours)))
    turned = lf.cell(p(r("Turned run", MONO)), 1200, '<w:textDirection w:val="btLr"/>')
    body.append(lf.table([lf.row([turned, lf.cell("Beside the turned cell.", 3000)], '<w:trHeight w:val="1800" w:hRule="exact"/>')],
                         [1200, 3000]))
    body.append(p(r("End.", MONO)))
    return "".join(body), lf.sect()


FIXTURES = [
    ("text-glyphs", text_glyphs),
    ("underlines", underlines),
    ("char-effects", char_effects),
    ("text-effects", text_effects),
    ("shading-patterns", shading_patterns),
    ("table-shading", table_shading),
    ("borders", borders),
    ("diagonals", diagonals),
    ("watermark-text", watermark_text_doc),
    ("watermark-picture", watermark_picture_doc),
    ("page-colour", page_colour_with(PAGE_TINT, PAGE_TINT_LABEL)),
    ("page-colour-shade", page_colour_with('w:color="2F5597" w:themeColor="accent1" w:themeShade="BF"', "Page colour with a theme shade")),
    ("page-colour-no-shape", page_colour_with(PAGE_TINT, PAGE_TINT_LABEL, shape=False)),
    ("page-gradient", page_gradient),
    ("page-gradient-stops", page_gradient_stops),
    ("page-pattern", page_pattern),
    ("page-picture", page_picture),
    ("picture-effects", picture_effects),
    ("jpeg-xr", jpeg_xr),
    ("pdf-accessible", pdf_accessible),
]


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else HERE
    out.mkdir(parents=True, exist_ok=True)
    for name, build in FIXTURES:
        d = Pkg() if name == "text-glyphs" else pkg()
        body, sect_pr = build(d)
        d.write(out / f"{name}.docx", body, sect_pr)
    print(f"wrote {len(FIXTURES)} fixtures to {out}")


if __name__ == "__main__":
    main()
