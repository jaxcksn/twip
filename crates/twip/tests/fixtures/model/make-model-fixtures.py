"""Writes the model fixtures into tests/fixtures/model/ (next to this file).

Small documents aimed at how formatting is stored rather than how it lays out: theme references, explicit-off and
automatic values, toggle inheritance, markup the reader does not model, namespace prefixes, and content copied
between documents. README.md lists every fixture and what it holds.

Standard library only, no randomness: the same bytes on every run. Every package validates against the OOXML schemas
(`tests/fidelity/validate`). Fonts are Calibri, Cambria, Times New Roman and MS Mincho, which the gates resolve to
their bundled stand-ins.
Run: python -I crates/twip/tests/fixtures/model/make-model-fixtures.py [out dir]
"""

import struct
import sys
import zipfile
import zlib
from pathlib import Path
from xml.sax.saxutils import escape

HERE = Path(__file__).resolve().parent
STAMP = (2026, 1, 1, 0, 0, 0)
XML_DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/"
CT = "application/vnd.openxmlformats-officedocument.wordprocessingml."
W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
NS = {
    "w": W,
    "r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    "wp": "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
    "a": "http://schemas.openxmlformats.org/drawingml/2006/main",
    "pic": "http://schemas.openxmlformats.org/drawingml/2006/picture",
    "m": "http://schemas.openxmlformats.org/officeDocument/2006/math",
    "mc": "http://schemas.openxmlformats.org/markup-compatibility/2006",
    "w14": "http://schemas.microsoft.com/office/word/2010/wordml",
    "w15": "http://schemas.microsoft.com/office/word/2012/wordml",
    "wp14": "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing",
}
NAMESPACES = " ".join(f'xmlns:{k}="{v}"' for k, v in NS.items()) + ' mc:Ignorable="w14 w15 wp14"'
SECT = '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>'

DOC_DEFAULTS = (
    '<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi" w:eastAsiaTheme="minorEastAsia" '
    'w:cstheme="minorBidi"/><w:sz w:val="22"/><w:szCs w:val="22"/><w:lang w:val="en-US" w:eastAsia="ja-JP" w:bidi="ar-SA"/></w:rPr></w:rPrDefault>'
    '<w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="259" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>'
)
BASE_STYLES = (
    '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>'
    '<w:style w:type="character" w:default="1" w:styleId="DefaultParagraphFont"><w:name w:val="Default Paragraph Font"/><w:uiPriority w:val="1"/><w:semiHidden/><w:unhideWhenUsed/></w:style>'
    '<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/><w:uiPriority w:val="99"/><w:semiHidden/><w:unhideWhenUsed/>'
    '<w:tblPr><w:tblInd w:w="0" w:type="dxa"/><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="108" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/>'
    '<w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>'
)


def styles(extra="", defaults=DOC_DEFAULTS, ns=NAMESPACES):
    """A styles part: document defaults, Normal, Default Paragraph Font and Normal Table, then `extra` styles."""
    return f"{XML_DECL}<w:styles {ns}>{defaults}{BASE_STYLES}{extra}</w:styles>"


def settings(extra="", compat=15):
    """A settings part; `extra` goes after defaultTabStop, in schema order."""
    return (
        f'{XML_DECL}<w:settings xmlns:w="{W}"><w:defaultTabStop w:val="720"/>{extra}<w:compat>'
        f'<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{compat}"/></w:compat></w:settings>'
    )


def theme(major="Cambria", minor="Calibri", east_asia="MS Mincho"):
    """A theme part with the Office colours and the given font scheme. `east_asia` is the minor East Asian face; an
    empty string leaves that slot empty."""
    colours = (
        ("dk1", '<a:sysClr val="windowText" lastClr="000000"/>'), ("lt1", '<a:sysClr val="window" lastClr="FFFFFF"/>'),
        ("dk2", '<a:srgbClr val="44546A"/>'), ("lt2", '<a:srgbClr val="E7E6E6"/>'), ("accent1", '<a:srgbClr val="4472C4"/>'),
        ("accent2", '<a:srgbClr val="ED7D31"/>'), ("accent3", '<a:srgbClr val="A5A5A5"/>'), ("accent4", '<a:srgbClr val="FFC000"/>'),
        ("accent5", '<a:srgbClr val="5B9BD5"/>'), ("accent6", '<a:srgbClr val="70AD47"/>'), ("hlink", '<a:srgbClr val="0563C1"/>'),
        ("folHlink", '<a:srgbClr val="954F72"/>'),
    )
    scheme = "".join(f"<a:{k}>{v}</a:{k}>" for k, v in colours)
    fill = '<a:solidFill><a:schemeClr val="phClr"/></a:solidFill>'
    line = f'<a:ln w="6350">{fill}</a:ln>'
    return (
        f'{XML_DECL}<a:theme xmlns:a="{NS["a"]}" name="Office Theme"><a:themeElements><a:clrScheme name="Office">{scheme}</a:clrScheme>'
        f'<a:fontScheme name="Office"><a:majorFont><a:latin typeface="{major}"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont>'
        f'<a:minorFont><a:latin typeface="{minor}"/><a:ea typeface="{east_asia}"/><a:cs typeface=""/></a:minorFont></a:fontScheme>'
        f'<a:fmtScheme name="Office"><a:fillStyleLst>{fill * 3}</a:fillStyleLst><a:lnStyleLst>{line * 3}</a:lnStyleLst>'
        '<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>'
        f'<a:bgFillStyleLst>{fill * 3}</a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>'
    )


def r(text, rpr=""):
    """A run of text; `rpr` is the inside of its w:rPr."""
    props = f"<w:rPr>{rpr}</w:rPr>" if rpr else ""
    return f'<w:r>{props}<w:t xml:space="preserve">{escape(text)}</w:t></w:r>'


def p(content="", ppr=""):
    """A paragraph; `ppr` is the inside of its w:pPr."""
    props = f"<w:pPr>{ppr}</w:pPr>" if ppr else ""
    return f"<w:p>{props}{content}</w:p>"


def document(body, sect=SECT, ns=NAMESPACES, head=""):
    """A main document part; `head` goes between w:document's start tag and w:body (w:background)."""
    return f"{XML_DECL}<w:document {ns}>{head}<w:body>{body}{sect}</w:body></w:document>"


def png(rgb=(0x44, 0x72, 0xC4), width=32, height=24):
    """A solid-colour PNG."""
    raw = b"".join(b"\x00" + bytes(rgb) * width for _ in range(height))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


class Package:
    """One package. `parts` are (name, bytes or str, content type or None); `rels` are the main document's
    relationships (id, type suffix after REL, target)."""

    def __init__(self):
        self.parts = []
        self.rels = []
        self.defaults = []

    def part(self, name, data, content_type=None, rel=None):
        """Adds a part under word/; `rel` (a type suffix) also relates it to the main document. Returns the id."""
        self.parts.append((f"word/{name}", data, content_type))
        if rel is None:
            return None
        rid = f"rId{len(self.rels) + 1}"
        self.rels.append((rid, rel, name))
        return rid

    def write(self, path, doc, styles_xml=None, settings_xml=None, theme_xml=None):
        """Writes the package with its main document and the given standard parts."""
        if styles_xml is not None:
            self.part("styles.xml", styles_xml, f"{CT}styles+xml", "styles")
        self.part("settings.xml", settings_xml if settings_xml is not None else settings(), f"{CT}settings+xml", "settings")
        if theme_xml is not None:
            self.part("theme/theme1.xml", theme_xml, "application/vnd.openxmlformats-officedocument.theme+xml", "theme")
        files = [("word/document.xml", doc, f"{CT}document.main+xml")] + self.parts
        overrides = "".join(f'<Override PartName="/{n}" ContentType="{c}"/>' for n, _, c in files if c)
        defaults = "".join(f'<Default Extension="{e}" ContentType="{c}"/>' for e, c in self.defaults)
        content_types = (
            f'{XML_DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            f'<Default Extension="xml" ContentType="application/xml"/>{defaults}{overrides}</Types>'
        )
        rels = '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        package_rels = f'{XML_DECL}{rels}<Relationship Id="rId1" Type="{REL}officeDocument" Target="word/document.xml"/></Relationships>'
        doc_rels = f"{XML_DECL}{rels}" + "".join(f'<Relationship Id="{i}" Type="{REL}{t}" Target="{target}"/>' for i, t, target in self.rels) + "</Relationships>"
        entries = [("[Content_Types].xml", content_types), ("_rels/.rels", package_rels), ("word/_rels/document.xml.rels", doc_rels)]
        entries += [(n, d) for n, d, _ in files]
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
            for name, data in entries:
                info = zipfile.ZipInfo(name, STAMP)
                info.create_system = 0  # the same bytes whatever OS writes the archive
                info.external_attr = 0
                z.writestr(info, data.encode("utf-8") if isinstance(data, str) else data, zipfile.ZIP_DEFLATED)


FIXTURES = []


# -- theme references --


def _theme_references_cell(text, width, tcpr=""):
    return f'<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/>{tcpr}</w:tcPr>{p(r(text))}</w:tc>'


def _theme_references_table(rows, widths, tblpr):
    grid = "".join(f'<w:gridCol w:w="{w}"/>' for w in widths)
    body = "".join("<w:tr>" + "".join(_theme_references_cell(t, w, c) for (t, c), w in zip(row, widths)) + "</w:tr>" for row in rows)
    return f"<w:tbl><w:tblPr>{tblpr}</w:tblPr><w:tblGrid>{grid}</w:tblGrid>{body}</w:tbl>"


def theme_references(out):
    """Theme colours with tints and shades on text, underlines, shading, borders, a table style and the page background,
    and theme fonts in all four slots, direct and in styles, including an empty East Asian slot over a named family."""
    fonts_major = '<w:rFonts w:asciiTheme="majorHAnsi" w:hAnsiTheme="majorHAnsi" w:eastAsiaTheme="minorEastAsia" w:cstheme="majorBidi"/>'
    fonts_minor = '<w:rFonts w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi" w:eastAsiaTheme="minorEastAsia" w:cstheme="minorBidi"/>'
    side = lambda name, attrs: f'<w:{name} w:val="single" w:sz="12" w:space="0" {attrs}/>'
    tbl_sides = "".join(side(s, 'w:color="8EAADB" w:themeColor="accent1" w:themeTint="99"') for s in ("top", "left", "bottom", "right", "insideH", "insideV"))
    extra = (
        # Paragraph style: major theme fonts and a shaded accent colour
        '<w:style w:type="paragraph" w:customStyle="1" w:styleId="ThemeHeading"><w:name w:val="Theme Heading"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/>'
        '<w:pPr><w:pBdr><w:bottom w:val="single" w:sz="8" w:space="1" w:color="2F5496" w:themeColor="accent1" w:themeShade="BF"/></w:pBdr><w:spacing w:before="120" w:after="120"/></w:pPr>'
        '<w:rPr><w:rFonts w:asciiTheme="majorHAnsi" w:hAnsiTheme="majorHAnsi" w:eastAsiaTheme="majorEastAsia" w:cstheme="majorBidi"/>'
        '<w:color w:val="2F5496" w:themeColor="accent1" w:themeShade="BF"/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:style>'
        # Character style: minor theme fonts, a shaded colour and a tinted underline colour
        '<w:style w:type="character" w:customStyle="1" w:styleId="ThemeAccent"><w:name w:val="Theme Accent"/><w:basedOn w:val="DefaultParagraphFont"/><w:qFormat/>'
        f'<w:rPr>{fonts_minor}<w:color w:val="C45911" w:themeColor="accent2" w:themeShade="BF"/>'
        '<w:u w:val="single" w:color="8EAADB" w:themeColor="accent1" w:themeTint="99"/></w:rPr></w:style>'
        # Paragraph style naming an explicit East Asian family
        '<w:style w:type="paragraph" w:customStyle="1" w:styleId="MinchoBody"><w:name w:val="Mincho Body"/><w:basedOn w:val="Normal"/><w:qFormat/>'
        '<w:rPr><w:rFonts w:eastAsia="MS Mincho"/></w:rPr></w:style>'
        # Table style: tinted borders, accent header row, tinted banding
        '<w:style w:type="table" w:customStyle="1" w:styleId="ThemeGrid"><w:name w:val="Theme Grid"/><w:basedOn w:val="TableNormal"/><w:uiPriority w:val="39"/>'
        f'<w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:tblPr><w:tblBorders>{tbl_sides}</w:tblBorders></w:tblPr>'
        '<w:tblStylePr w:type="firstRow"><w:rPr><w:b/><w:bCs/><w:color w:val="FFFFFF" w:themeColor="background1"/></w:rPr>'
        '<w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="4472C4" w:themeFill="accent1"/></w:tcPr></w:tblStylePr>'
        '<w:tblStylePr w:type="band1Horz"><w:tcPr><w:tcBorders><w:bottom w:val="single" w:sz="12" w:space="0" w:color="2F5496" w:themeColor="accent1" w:themeShade="BF"/></w:tcBorders>'
        '<w:shd w:val="clear" w:color="auto" w:fill="D9E2F3" w:themeFill="accent1" w:themeFillTint="33"/></w:tcPr></w:tblStylePr></w:style>'
    )
    ja = "日本語のテキスト"
    styled_table = _theme_references_table(
        [[("Header in background 1 on accent 1", ""), ("From the table style", "")],
         [("Band in accent 1 lighter 80%", ""), ("Bottom in accent 1 darker 25%", "")],
         [("Unbanded row", ""), ("Borders in accent 1 lighter 40%", "")]],
        (4680, 4680),
        '<w:tblStyle w:val="ThemeGrid"/><w:tblW w:w="9360" w:type="dxa"/>'
        '<w:tblLook w:val="0420" w:firstRow="1" w:lastRow="0" w:firstColumn="0" w:lastColumn="0" w:noHBand="0" w:noVBand="1"/>',
    )
    cell_a = "".join(side(s, 'w:color="C45911" w:themeColor="accent2" w:themeShade="BF"') for s in ("top", "left", "bottom", "right"))
    cell_b = "".join(side(s, 'w:color="A8D08D" w:themeColor="accent6" w:themeTint="99"') for s in ("top", "left", "bottom", "right"))
    direct_table = _theme_references_table(
        [[("Cell borders in accent 2 darker 25%", f"<w:tcBorders>{cell_a}</w:tcBorders>"),
          ("Cell borders in accent 6 lighter 40%, fill accent 4 lighter 60%",
           f'<w:tcBorders>{cell_b}</w:tcBorders><w:shd w:val="clear" w:color="auto" w:fill="FFD966" w:themeFill="accent4" w:themeFillTint="66"/>')]],
        (4680, 4680),
        '<w:tblW w:w="9360" w:type="dxa"/><w:tblLook w:val="0000" w:firstRow="0" w:lastRow="0" w:firstColumn="0" w:lastColumn="0" w:noHBand="0" w:noVBand="0"/>',
    )
    pbdr = (
        '<w:pBdr><w:top w:val="single" w:sz="8" w:space="1" w:color="4472C4" w:themeColor="accent1"/>'
        '<w:left w:val="single" w:sz="8" w:space="4" w:color="C45911" w:themeColor="accent2" w:themeShade="BF"/>'
        '<w:bottom w:val="single" w:sz="8" w:space="1" w:color="A8D08D" w:themeColor="accent6" w:themeTint="99"/>'
        '<w:right w:val="single" w:sz="8" w:space="4" w:color="000000" w:themeColor="text1"/></w:pBdr>'
    )
    body = "".join((
        p(r("Theme references"), '<w:pStyle w:val="ThemeHeading"/>'),
        # Text colour
        p(r("Text in accent 1 darker 25% (themeShade)", '<w:color w:val="2F5496" w:themeColor="accent1" w:themeShade="BF"/>')),
        p(r("Text in accent 2 lighter 40% (themeTint)", '<w:color w:val="F4B083" w:themeColor="accent2" w:themeTint="99"/>')),
        p(r("Text in text 1 lighter 35%", '<w:color w:val="595959" w:themeColor="text1" w:themeTint="A6"/>')),
        p(r("Text in accent 6, no tint or shade", '<w:color w:val="70AD47" w:themeColor="accent6"/>')),
        # Underline colour
        p(r("Underline in accent 2 darker 25%", '<w:u w:val="single" w:color="C45911" w:themeColor="accent2" w:themeShade="BF"/>')),
        p(r("Double underline in accent 1 lighter 40%", '<w:u w:val="double" w:color="8EAADB" w:themeColor="accent1" w:themeTint="99"/>')),
        # Run shading
        p(r("Run fill in accent 4 lighter 60%", '<w:shd w:val="clear" w:color="auto" w:fill="FFD966" w:themeFill="accent4" w:themeFillTint="66"/>')),
        p(r("Run 25% pattern in accent 1 darker 25% over accent 1 lighter 80%",
            '<w:shd w:val="pct25" w:color="2F5496" w:themeColor="accent1" w:themeShade="BF" w:fill="D9E2F3" w:themeFill="accent1" w:themeFillTint="33"/>')),
        # Paragraph shading
        p(r("Paragraph fill in accent 6 lighter 80%"), '<w:shd w:val="clear" w:color="auto" w:fill="E2EFD9" w:themeFill="accent6" w:themeFillTint="33"/>'),
        p(r("Paragraph fill in accent 4 darker 25%"), '<w:shd w:val="clear" w:color="auto" w:fill="BF8F00" w:themeFill="accent4" w:themeFillShade="BF"/>'),
        # Run borders
        p(r("Run border in accent 1", '<w:bdr w:val="single" w:sz="4" w:space="0" w:color="4472C4" w:themeColor="accent1"/>')),
        p(r("Run border in accent 5 darker 50%", '<w:bdr w:val="single" w:sz="8" w:space="0" w:color="1F4E79" w:themeColor="accent5" w:themeShade="80"/>')),
        # Paragraph borders
        p(r("Paragraph borders: top accent 1, left accent 2 darker, bottom accent 6 lighter, right text 1"), pbdr),
        # Theme fonts, direct
        p(r("Direct major fonts in all four slots, East Asian minor: ", fonts_major) + r(ja, fonts_major)),
        p(r("Direct minor fonts in all four slots: ", fonts_minor) + r(ja, fonts_minor)),
        p(r("Complex script run in the major bidi theme font", fonts_major + "<w:cs/>")),
        # Theme fonts, styles
        p(r("Character style with minor theme fonts and theme colours: ") + r("styled " + ja, '<w:rStyle w:val="ThemeAccent"/>')),
        p(r("Paragraph style with major theme fonts " + ja), '<w:pStyle w:val="ThemeHeading"/>'),
        # Empty East Asian theme slot
        p(r("Empty major East Asian slot over a named family on the same run: ", '<w:rFonts w:eastAsia="MS Mincho" w:eastAsiaTheme="majorEastAsia"/>')
          + r(ja, '<w:rFonts w:eastAsia="MS Mincho" w:eastAsiaTheme="majorEastAsia"/>')),
        p(r("Empty major East Asian slot over a style's named family: ", '<w:rFonts w:eastAsiaTheme="majorEastAsia"/>')
          + r(ja, '<w:rFonts w:eastAsiaTheme="majorEastAsia"/>'), '<w:pStyle w:val="MinchoBody"/>'),
        # Tables
        p(r("Table style with theme borders and fills")),
        styled_table,
        p(r("Direct cell borders and fill in theme colours"), '<w:spacing w:before="160"/>'),
        direct_table,
        p(r("Page background in accent 5 lighter 80%"), '<w:spacing w:before="160"/>'),
    ))
    head = '<w:background w:color="DEEAF6" w:themeColor="accent5" w:themeTint="33"/>'
    settings_xml = settings().replace("<w:defaultTabStop", "<w:displayBackgroundShape/><w:defaultTabStop", 1)
    Package().write(out / "theme-references.docx", document(body, head=head), styles(extra), settings_xml, theme())


FIXTURES.append(theme_references)


# -- explicit-off --


def _explicit_off_style(kind, sid, name, ppr="", rpr=""):
    pp = f"<w:pPr>{ppr}</w:pPr>" if ppr else ""
    rp = f"<w:rPr>{rpr}</w:rPr>" if rpr else ""
    based = '<w:basedOn w:val="Normal"/>' if kind == "paragraph" else '<w:basedOn w:val="DefaultParagraphFont"/>'
    return f'<w:style w:type="{kind}" w:customStyle="1" w:styleId="{sid}"><w:name w:val="{name}"/>{based}<w:qFormat/>{pp}{rp}</w:style>'


def _explicit_off_tabbed(before, after, ppr):
    run = f'<w:r><w:t xml:space="preserve">{escape(before)}</w:t><w:tab/><w:t xml:space="preserve">{escape(after)}</w:t></w:r>'
    return p(run, ppr)


def explicit_off(out):
    """Explicit off, none and automatic values over styles that turn them on, each next to an unset control."""
    s = _explicit_off_style
    extra = (
        s("paragraph", "BoldPara", "Bold Para", rpr="<w:b/>")
        + s("character", "BoldChar", "Bold Char", rpr="<w:b/>")
        + s("character", "UnderlineChar", "Underline Char", rpr='<w:u w:val="single"/>')
        + s("character", "ColourChar", "Colour Char", rpr='<w:color w:val="C00000"/>')
        + s("character", "ShadedChar", "Shaded Char", rpr='<w:shd w:val="clear" w:color="auto" w:fill="9DC3E6"/>')
        + s("paragraph", "ShadedPara", "Shaded Para", ppr='<w:shd w:val="clear" w:color="auto" w:fill="C5E0B3"/>')
        + s("paragraph", "ListPara", "List Para", ppr='<w:numPr><w:numId w:val="1"/></w:numPr>')
        + s("paragraph", "TabPara", "Tab Para", ppr='<w:tabs><w:tab w:val="left" w:pos="5040"/></w:tabs>')
        + s("paragraph", "IndentPara", "Indent Para", ppr='<w:ind w:left="1440" w:firstLine="720"/>')
        + s("character", "HiddenChar", "Hidden Char", rpr="<w:vanish/>")
        + s("paragraph", "KeepNextPara", "Keep Next Para", ppr="<w:keepNext/>")
    )
    numbering = (
        f'{XML_DECL}<w:numbering {NAMESPACES}><w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="singleLevel"/>'
        '<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/>'
        '<w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum>'
        '<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>'
    )
    ps = lambda sid: f'<w:pStyle w:val="{sid}"/>'
    rs = lambda sid: f'<w:rStyle w:val="{sid}"/>'
    body = [
        # Bold over a paragraph style
        p(r("Bold paragraph style, nothing direct"), ps("BoldPara")),
        p(r('Bold off (val="0") over a bold paragraph style', '<w:b w:val="0"/>'), ps("BoldPara")),
        p(r('Bold off (val="false") over a bold paragraph style', '<w:b w:val="false"/>'), ps("BoldPara")),
        # Bold over a character style
        p(r("Bold character style, nothing direct", rs("BoldChar"))),
        p(r('Bold off (val="0") over a bold character style', rs("BoldChar") + '<w:b w:val="0"/>')),
        p(r('Bold off (val="false") over a bold character style', rs("BoldChar") + '<w:b w:val="false"/>')),
        # Underline
        p(r("Underlined character style, nothing direct", rs("UnderlineChar"))),
        p(r("Underline none over an underlined style", rs("UnderlineChar") + '<w:u w:val="none"/>')),
        # Colour
        p(r("Coloured character style, nothing direct", rs("ColourChar"))),
        p(r("Colour auto over a coloured style", rs("ColourChar") + '<w:color w:val="auto"/>')),
        # Highlight: a style's rPr may not hold w:highlight, so the on state is direct
        p(r("Highlight unset, nothing direct")),
        p(r("Highlight yellow, direct", '<w:highlight w:val="yellow"/>')),
        p(r("Highlight none, direct", '<w:highlight w:val="none"/>')),
        # Run shading
        p(r("Shaded character style, nothing direct", rs("ShadedChar"))),
        p(r("Run shading nil over a shaded style", rs("ShadedChar") + '<w:shd w:val="nil"/>')),
        p(r("Run shading clear, fill auto, over a shaded style", rs("ShadedChar") + '<w:shd w:val="clear" w:color="auto" w:fill="auto"/>')),
        # Paragraph shading
        p(r("Shaded paragraph style, nothing direct"), ps("ShadedPara")),
        p(r("Paragraph shading nil over a shaded style"), ps("ShadedPara") + '<w:shd w:val="nil"/>'),
        p(r("Paragraph shading clear, fill auto, over a shaded style"), ps("ShadedPara") + '<w:shd w:val="clear" w:color="auto" w:fill="auto"/>'),
        # Numbering
        p(r("List paragraph style, nothing direct"), ps("ListPara")),
        p(r("Numbering id 0 over a list paragraph style"), ps("ListPara") + '<w:numPr><w:numId w:val="0"/></w:numPr>'),
        # Tab stops
        _explicit_off_tabbed("Style tab, nothing direct", "after", ps("TabPara")),
        _explicit_off_tabbed("Style tab cleared", "after", ps("TabPara") + '<w:tabs><w:tab w:val="clear" w:pos="5040"/></w:tabs>'),
        # Indentation
        p(r("Indented paragraph style, nothing direct"), ps("IndentPara")),
        p(r("Indent left 0, first line 0, over an indented style"), ps("IndentPara") + '<w:ind w:left="0" w:firstLine="0"/>'),
        # Hidden
        p(r("Hidden character style, nothing direct: ") + r("hidden text", rs("HiddenChar"))),
        p(r("Hidden off over a hidden character style: ") + r("shown text", rs("HiddenChar") + '<w:vanish w:val="0"/>')),
        # Keep with next
        p(r("Keep-with-next style, nothing direct"), ps("KeepNextPara")),
        p(r("Keep with next off over a keep-with-next style"), ps("KeepNextPara") + '<w:keepNext w:val="0"/>'),
    ]
    pkg = Package()
    pkg.part("numbering.xml", numbering, f"{CT}numbering+xml", "numbering")
    pkg.write(out / "explicit-off.docx", document("".join(body)), styles(extra), settings(), theme())


FIXTURES.append(explicit_off)


# -- style-toggles --

_STYLE_TOGGLES_DEFAULTS = (
    '<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi" w:eastAsiaTheme="minorEastAsia" '
    'w:cs="Times New Roman"/><w:b/><w:sz w:val="22"/><w:szCs w:val="22"/><w:lang w:val="en-US" w:eastAsia="ja-JP" w:bidi="ar-SA"/></w:rPr></w:rPrDefault>'
    '<w:pPrDefault><w:pPr><w:spacing w:after="80" w:line="259" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>'
)


def _style_toggles_style(kind, sid, name, rpr, extra=""):
    """A paragraph or character style; `extra` goes between w:name and w:rPr (basedOn, link, uiPriority)."""
    return f'<w:style w:type="{kind}" w:styleId="{sid}"><w:name w:val="{name}"/>{extra}<w:qFormat/><w:rPr>{rpr}</w:rPr></w:style>'


def _style_toggles_styles():
    """Bold paragraph and character styles, a three-level character chain, a linked pair, complex-script styles and
    a table style with a bold first row."""
    s = _style_toggles_style
    border = "".join(f'<w:{e} w:val="single" w:sz="4" w:space="0" w:color="000000"/>' for e in ("top", "left", "bottom", "right", "insideH", "insideV"))
    return styles(
        s("paragraph", "BoldPara", "Bold Para", "<w:b/>", '<w:basedOn w:val="Normal"/>')
        + s("character", "BoldChar", "Bold Char", "<w:b/>", '<w:basedOn w:val="DefaultParagraphFont"/>')
        + s("character", "ChainOne", "Chain One", "<w:b/>", '<w:basedOn w:val="DefaultParagraphFont"/>')
        + s("character", "ChainTwo", "Chain Two", "<w:i/>", '<w:basedOn w:val="ChainOne"/>')
        + s("character", "ChainThree", "Chain Three", "<w:b/>", '<w:basedOn w:val="ChainTwo"/>')
        + s("paragraph", "LinkedHeading", "Linked Heading", "<w:b/>", '<w:basedOn w:val="Normal"/><w:link w:val="LinkedHeadingChar"/>')
        + s("character", "LinkedHeadingChar", "Linked Heading Char", "<w:b/>", '<w:basedOn w:val="DefaultParagraphFont"/><w:link w:val="LinkedHeading"/>')
        + s("paragraph", "CsPara", "CS Para", "<w:bCs/>", '<w:basedOn w:val="Normal"/>')
        + s("character", "CsBoldItalic", "CS Bold Italic", "<w:bCs/><w:iCs/>", '<w:basedOn w:val="DefaultParagraphFont"/>')
        + '<w:style w:type="table" w:styleId="HeaderTable"><w:name w:val="Header Table"/><w:basedOn w:val="TableNormal"/><w:uiPriority w:val="39"/>'
        f'<w:tblPr><w:tblBorders>{border}</w:tblBorders></w:tblPr>'
        '<w:tblStylePr w:type="firstRow"><w:rPr><w:b/></w:rPr></w:tblStylePr></w:style>',
        defaults=_STYLE_TOGGLES_DEFAULTS,
    )


def _style_toggles_numbering():
    """One single-level decimal list whose label run is bold."""
    return (
        f'{XML_DECL}<w:numbering {NAMESPACES}><w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="singleLevel"/>'
        '<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/>'
        '<w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr><w:rPr><w:b/></w:rPr></w:lvl></w:abstractNum>'
        '<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>'
    )


def _style_toggles_case(text, para_style="", char_style="", direct="", ppr=""):
    """One labelled case: a paragraph in `para_style` holding one run in `char_style` with `direct` run formatting."""
    pstyle = f'<w:pStyle w:val="{para_style}"/>' if para_style else ""
    rstyle = f'<w:rStyle w:val="{char_style}"/>' if char_style else ""
    return p(r(text, rstyle + direct), pstyle + ppr)


def _style_toggles_table():
    """A two-by-two table in the bold first-row table style, Normal and bold-paragraph-style cells in each row."""
    c = _style_toggles_case

    def cell(*paras):
        return f'<w:tc><w:tcPr><w:tcW w:w="4680" w:type="dxa"/></w:tcPr>{"".join(paras)}</w:tc>'

    first = cell(c("First row, Normal paragraph"), c("First row, bold character style", char_style="BoldChar")) + cell(
        c("First row, bold paragraph style", "BoldPara"), c("First row, bold paragraph style, direct bold", "BoldPara", direct="<w:b/>")
    )
    body = cell(c("Body row, Normal paragraph")) + cell(c("Body row, bold paragraph style", "BoldPara"))
    return (
        '<w:tbl><w:tblPr><w:tblStyle w:val="HeaderTable"/><w:tblW w:w="9360" w:type="dxa"/>'
        '<w:tblLook w:val="0420" w:firstRow="1" w:lastRow="0" w:firstColumn="0" w:lastColumn="0" w:noHBand="0" w:noVBand="1"/></w:tblPr>'
        '<w:tblGrid><w:gridCol w:w="4680"/><w:gridCol w:w="4680"/></w:tblGrid>'
        f"<w:tr>{first}</w:tr><w:tr>{body}</w:tr></w:tbl>"
    )


def style_toggles(out):
    """Toggle properties at every level over bold document defaults: paragraph and character styles alone and together, a three-level character chain, a table style's bold first row, a linked style pair, a bold list label, complex-script bCs and iCs, and direct formatting over each."""
    c = _style_toggles_case
    off, on = '<w:b w:val="0"/>', "<w:b/>"
    num = '<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>'
    body = [
        c("Document defaults and styles"),
        c("Normal text over bold document defaults"),
        c("Bold paragraph style alone", "BoldPara"),
        c("Bold character style alone", char_style="BoldChar"),
        c("Bold paragraph style and bold character style together", "BoldPara", "BoldChar"),
        c("Direct bold off over bold document defaults", direct=off),
        c("Direct bold over bold document defaults", direct=on),
        c("Direct bold over a bold paragraph style", "BoldPara", direct=on),
        c("Direct bold off over a bold paragraph style", "BoldPara", direct=off),
        c("Direct bold over a bold character style", char_style="BoldChar", direct=on),
        c("Direct bold off over a bold character style", char_style="BoldChar", direct=off),
        c("Direct bold over bold paragraph and character styles together", "BoldPara", "BoldChar", on),
        c("Direct bold off over bold paragraph and character styles together", "BoldPara", "BoldChar", off),
        c("Character style chain"),
        c("Chain level one: bold", char_style="ChainOne"),
        c("Chain level two: italic, based on level one", char_style="ChainTwo"),
        c("Chain level three: bold again, based on level two", char_style="ChainThree"),
        c("Chain level three in a bold paragraph style", "BoldPara", "ChainThree"),
        c("Direct bold and direct italic off over chain level three", char_style="ChainThree", direct=on + '<w:i w:val="0"/>'),
        c("Table style first row"),
        _style_toggles_table(),
        c("Linked styles"),
        c("Linked character style alone", char_style="LinkedHeadingChar"),
        c("Linked paragraph style alone", "LinkedHeading"),
        c("Linked character style in its linked paragraph style", "LinkedHeading", "LinkedHeadingChar"),
        c("Direct bold off over the linked character style", char_style="LinkedHeadingChar", direct=off),
        c("Numbering"),
        c("List item: bold label level, Normal paragraph", ppr=num),
        c("List item: bold label level, bold paragraph style", "BoldPara", ppr=num),
        c("List item: bold label level, paragraph mark direct bold off", ppr=num + f"<w:rPr>{off}</w:rPr>"),
        c("Complex script"),
        c("Complex-script run, no bCs anywhere", direct="<w:cs/>"),
        c("Complex-script run in a bCs and iCs character style", char_style="CsBoldItalic", direct="<w:cs/>"),
        c("Complex-script run in a bCs paragraph style", "CsPara", direct="<w:cs/>"),
        c("Complex-script run, bCs paragraph style and bCs iCs character style together", "CsPara", "CsBoldItalic", "<w:cs/>"),
        c("Complex-script run, direct bCs over bCs paragraph and character styles", "CsPara", "CsBoldItalic", "<w:bCs/><w:cs/>"),
        c("Complex-script run, direct bCs and iCs off over the character style", char_style="CsBoldItalic", direct='<w:bCs w:val="0"/><w:iCs w:val="0"/><w:cs/>'),
        c("Complex-script run, direct b without bCs", direct="<w:b/><w:cs/>"),
        c("Right-to-left run in the bCs and iCs character style", char_style="CsBoldItalic", direct="<w:rtl/>"),
        c("Right-to-left run in a bCs paragraph style and the bCs iCs character style", "CsPara", "CsBoldItalic", "<w:rtl/>"),
        c("Right-to-left run, direct bCs and iCs", direct="<w:bCs/><w:iCs/><w:rtl/>"),
    ]
    pkg = Package()
    pkg.part("numbering.xml", _style_toggles_numbering(), f"{CT}numbering+xml", "numbering")
    pkg.write(out / "style-toggles.docx", document("".join(body)), styles_xml=_style_toggles_styles(), theme_xml=theme())


FIXTURES.append(style_toggles)


# -- unknown-attributes --

_UNKNOWN_ATTRIBUTES_TX = "http://example.com/twip/ext"
_UNKNOWN_ATTRIBUTES_TC = "http://example.com/twip/choice"
_UNKNOWN_ATTRIBUTES_NS = (
    NAMESPACES.replace(' mc:Ignorable="w14 w15 wp14"', "")
    + f' xmlns:tx="{_UNKNOWN_ATTRIBUTES_TX}" xmlns:tc="{_UNKNOWN_ATTRIBUTES_TC}" mc:Ignorable="w14 w15 wp14 tx"'
)


def _unknown_attributes_table():
    cell = lambda tcpr, text: f"<w:tc><w:tcPr>{tcpr}</w:tcPr>{p(r(text))}</w:tc>"
    return (
        '<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/>'
        '<w:left w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/>'
        '<w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="auto"/></w:tblBorders>'
        '<w:tblLook w:val="04A0" w:firstRow="1" w:lastRow="0" w:firstColumn="1" w:lastColumn="0" w:noHBand="0" w:noVBand="1"/>'
        '<w:tblCaption w:val="Totals"/><w:tblDescription w:val="Table with caption and description"/></w:tblPr>'
        '<w:tblGrid><w:gridCol w:w="4680"/><w:gridCol w:w="4680"/></w:tblGrid><w:tr>'
        + cell('<w:cnfStyle w:val="100000000000"/><w:tcW w:w="4680" w:type="dxa"/>', "Cell with a cnfStyle child")
        + cell('<w:tcW w:w="4680" w:type="dxa"/>', "Table with caption and description")
        + "</w:tr></w:tbl>"
    )


def unknown_attributes(out):
    """Unread attributes and children on read properties, ignorable markup, an mc fallback and one invalid jc."""
    w14_3d = '<w14:props3d w14:extrusionH="0" w14:contourW="0" w14:prstMaterial="none"/>'
    control = (
        '<w:sdt><w:sdtPr><w:alias w:val="Note"/><w:tag w:val="note"/><w:id w:val="101"/>'
        "<w15:webExtensionLinked/><w15:webExtensionCreated/><w:text/></w:sdtPr>"
        f"<w:sdtContent>{r('Content control with web extension children')}</w:sdtContent></w:sdt>"
    )
    alternate = (
        '<mc:AlternateContent><mc:Choice Requires="tc">' + r("Choice text Word skips", '<w:color w:val="C00000"/>') + "</mc:Choice>"
        "<mc:Fallback>" + r("Fallback text Word shows") + "</mc:Fallback></mc:AlternateContent>"
    )
    body = [
        # Unread attribute on a read child
        p(r("Language with a bidi attribute the reader skips", '<w:lang w:val="en-US" w:eastAsia="ja-JP" w:bidi="he-IL"/>')),
        # Extension-namespace attributes on read elements
        p(r("Colour and centring with extension attributes", '<w:color w:val="2E74B5" tx:tone="cool"/>'), '<w:spacing w:after="240" tx:rhythm="loose"/><w:jc w:val="center" tx:align="optical"/>'),
        # Unread run, paragraph, table, cell and control children
        p(r("Run with webHidden, effect and props3d children", '<w:webHidden/><w:effect w:val="shimmer"/>' + w14_3d)),
        p(r("Paragraph with adjustRightInd and cnfStyle children"), '<w:adjustRightInd w:val="0"/><w:cnfStyle w:val="000000000000"/>'),
        _unknown_attributes_table(),
        p(control),
        # Invalid enumeration literal
        p(r("Justification sideways, not a schema value"), '<w:jc w:val="sideways"/>'),
        # Ignorable extension elements
        p(r("Run with an ignorable extension element", '<w:i/><tx:glint tx:val="soft"/>')),
        p(r("Paragraph with an ignorable element ") + '<tx:marker tx:id="1"/>' + r("between its runs")),
        # Alternate content
        p(alternate),
        # Remainder beside read properties
        p(r(
            "Bold run whose unread children and attributes sit beside read ones",
            '<w:b/><w:webHidden/><w:color w:val="2E74B5" tx:tone="cool"/><w:sz w:val="24"/><w:effect w:val="sparkle"/>'
            '<w:lang w:val="en-US" w:bidi="he-IL"/>' + w14_3d + '<tx:glint tx:val="soft"/>',
        )),
        p(r("Section with paperSrc and bidi children")),
    ]
    sect = (
        '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>'
        '<w:paperSrc w:first="1" w:other="1"/><w:bidi w:val="0"/></w:sectPr>'
    )
    ns = _UNKNOWN_ATTRIBUTES_NS
    Package().write(out / "unknown-attributes.docx", document("".join(body), sect=sect, ns=ns), styles(), theme_xml=theme())


FIXTURES.append(unknown_attributes)


# -- namespace dependencies --


def _namespace_dependencies_run(x, text, rpr=""):
    """A run in the prefix `x` (`x:`, or empty for the default namespace)."""
    props = f"<{x}rPr>{rpr}</{x}rPr>" if rpr else ""
    return f'<{x}r>{props}<{x}t xml:space="preserve">{escape(text)}</{x}t></{x}r>'


def _namespace_dependencies_para(x, content, ppr="", attrs=""):
    props = f"<{x}pPr>{ppr}</{x}pPr>" if ppr else ""
    return f"<{x}p{attrs}>{props}{content}</{x}p>"


def namespace_dependencies(out):
    """The main namespace under x:, w: and the default namespace across the document, header and footer, with w14, w15
    and r bound on elements below the part roots."""
    R, P = _namespace_dependencies_run, _namespace_dependencies_para
    w14, w15, mc, rel = NS["w14"], NS["w15"], NS["mc"], NS["r"]
    outline = '<w14:textOutline w14:w="9525" w14:cap="rnd" w14:cmpd="sng" w14:algn="ctr"><w14:solidFill><w14:srgbClr w14:val="C00000"/></w14:solidFill><w14:prstDash w14:val="solid"/><w14:round/></w14:textOutline>'
    ligatures = '<w14:ligatures w14:val="standardContextual"/>'
    mincho = '<x:rFonts x:ascii="MS Mincho" x:eastAsia="MS Mincho" x:hAnsi="MS Mincho"/>'
    x = "x:"
    body = "".join([
        # Main namespace under x: on the root
        P(x, R(x, "Main namespace bound to x on the document root")),
        P(x, R(x, "Bold and centred under the x prefix", "<x:b/>"), '<x:jc x:val="center"/>'),
        # Default namespace for one paragraph, attributes still under x:
        f'<p xmlns="{W}"><pPr><jc x:val="right"/></pPr><r><rPr><i/></rPr><t xml:space="preserve">Default namespace on this paragraph, italic and right aligned</t></r></p>',
        # Main namespace under w: bound on the paragraph
        f'<w:p xmlns:w="{W}"><w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t xml:space="preserve">Main namespace bound to w on this paragraph, underlined</w:t></w:r></w:p>',
        # w14 bound on the paragraph
        P(x, R(x, "Ligatures and a red text outline, w14 bound on the paragraph", "<x:sz x:val=\"28\"/>" + outline + ligatures),
          attrs=f' xmlns:w14="{w14}" xmlns:mc="{mc}" mc:Ignorable="w14"'),
        # w14 bound on the content control
        P(x, R(x, "Check box with w14 bound on the content control ")
          + f'<x:sdt xmlns:w14="{w14}" xmlns:mc="{mc}" mc:Ignorable="w14"><x:sdtPr><x:id x:val="1"/><w14:checkbox><w14:checked w14:val="0"/>'
          '<w14:checkedState w14:val="2612" w14:font="MS Mincho"/><w14:uncheckedState w14:val="2610" w14:font="MS Mincho"/></w14:checkbox></x:sdtPr>'
          f'<x:sdtContent>{R(x, chr(0x2610), mincho)}</x:sdtContent></x:sdt>'),
    ])
    sect = (
        f'<x:sectPr xmlns:r="{rel}"><x:headerReference x:type="default" r:id="rId1"/><x:footerReference x:type="default" r:id="rId2"/>'
        '<x:pgSz x:w="12240" x:h="15840"/><x:pgMar x:top="1440" x:right="1440" x:bottom="1440" x:left="1440" x:header="720" x:footer="720" x:gutter="0"/></x:sectPr>'
    )
    doc = f'{XML_DECL}<x:document xmlns:x="{W}"><x:body>{body}{sect}</x:body></x:document>'
    # Header: w: on the root, w14 and w15 bound below it
    w = "w:"
    header = (
        f'{XML_DECL}<w:hdr xmlns:w="{W}">'
        + P(w, R(w, "Header under w: on the root"))
        + P(w, f'<w:r xmlns:w14="{w14}" xmlns:mc="{mc}" mc:Ignorable="w14"><w:rPr><w:b/>{outline}{ligatures}</w:rPr>'
            '<w:t xml:space="preserve">Header ligatures and outline, w14 bound on the run</w:t></w:r>')
        + P(w, R(w, "Header heading level 1 collapsed, w15 bound on the paragraph"), '<w:outlineLvl w:val="0"/><w15:collapsed/>',
            attrs=f' xmlns:w15="{w15}" xmlns:mc="{mc}" mc:Ignorable="w15"')
        + "</w:hdr>"
    )
    # Footer: the main namespace as the default namespace throughout
    footer = (
        f'{XML_DECL}<ftr xmlns="{W}">'
        + P("", R("", "Footer in the default namespace, bold", "<b/>"))
        + P("", R("", "Footer italic, no attributes anywhere", "<i/>"))
        + "</ftr>"
    )
    pkg = Package()
    pkg.part("header1.xml", header, f"{CT}header+xml", "header")
    pkg.part("footer1.xml", footer, f"{CT}footer+xml", "footer")
    pkg.write(out / "namespace-dependencies.docx", doc, styles(), theme_xml=theme())


FIXTURES.append(namespace_dependencies)


# -- foreign content copy --


_FOREIGN_CONTENT_COPY_EMU = (914400, 685800)


def _foreign_content_copy_picture(rid):
    cx, cy = _FOREIGN_CONTENT_COPY_EMU
    return (
        '<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0">'
        f'<wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="1" name="Picture 1"/>'
        '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic>'
        '<pic:nvPicPr><pic:cNvPr id="1" name="blue.png"/><pic:cNvPicPr/></pic:nvPicPr>'
        f'<pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
        f'<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>'
        '</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>'
    )


def _foreign_content_copy_source(out):
    pkg = Package()
    pkg.defaults.append(("png", "image/png"))
    image = pkg.part("media/image1.png", png(width=96, height=72), rel="image")
    extra = (
        # Paragraph style the destination lacks
        '<w:style w:type="paragraph" w:customStyle="1" w:styleId="SourceQuote"><w:name w:val="Source Quote"/><w:basedOn w:val="Normal"/><w:qFormat/>'
        '<w:pPr><w:ind w:left="720" w:right="720"/></w:pPr><w:rPr><w:rFonts w:ascii="Cambria" w:hAnsi="Cambria"/><w:i/><w:iCs/><w:color w:val="2F5496"/></w:rPr></w:style>'
        # Character style the destination lacks
        '<w:style w:type="character" w:customStyle="1" w:styleId="SourceMark"><w:name w:val="Source Mark"/><w:basedOn w:val="DefaultParagraphFont"/><w:qFormat/>'
        '<w:rPr><w:b/><w:bCs/><w:color w:val="C45911"/><w:shd w:val="clear" w:color="auto" w:fill="FBE4D5"/></w:rPr></w:style>'
    )
    math = (
        '<m:oMath><m:r><m:t>x</m:t></m:r><m:r><m:t>=</m:t></m:r>'
        '<m:f><m:num><m:r><m:t>a</m:t></m:r><m:r><m:t>+</m:t></m:r><m:r><m:t>b</m:t></m:r></m:num>'
        '<m:den><m:sSup><m:e><m:r><m:t>c</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:den></m:f></m:oMath>'
    )
    three_d = (
        '<w14:scene3d><w14:camera w14:prst="orthographicFront"/><w14:lightRig w14:rig="threePt" w14:dir="t"/></w14:scene3d>'
        '<w14:props3d w14:extrusionH="57150" w14:contourW="12700" w14:prstMaterial="warmMatte">'
        '<w14:extrusionClr><w14:srgbClr w14:val="4472C4"/></w14:extrusionClr><w14:contourClr><w14:srgbClr w14:val="2F5496"/></w14:contourClr></w14:props3d>'
    )
    control = (
        '<w:sdt><w:sdtPr><w:alias w:val="Customer name"/><w:tag w:val="customer"/><w:id w:val="417"/>'
        '<w15:color w:val="70AD47"/><w15:appearance w15:val="tags"/><w15:webExtensionLinked/><w:text/></w:sdtPr>'
        f'<w:sdtContent>{r("Tagged plain-text control")}</w:sdtContent></w:sdt>'
    )
    body = "".join((
        p(r("Content to copy")),
        p(r("Inline Office Math equation: ") + math),
        p(r("Editable range in a read-only document: ") + '<w:permStart w:id="1" w:edGrp="everyone"/>'
          + r("these words may be edited") + '<w:permEnd w:id="1"/>' + r(", these may not.")),
        p(r("Run with a 3-D scene and extrusion", three_d)),
        p(r("Content control with a colour and tag appearance: ") + control),
        p(r("Inline picture from a media part: ") + _foreign_content_copy_picture(image)),
        p(r("Paragraph in Source Quote, a style the destination lacks"), '<w:pStyle w:val="SourceQuote"/>'),
        p(r("Run in ") + r("Source Mark", '<w:rStyle w:val="SourceMark"/>') + r(", a character style the destination lacks")),
    ))
    settings_xml = (
        f'{XML_DECL}<w:settings xmlns:w="{W}">'
        '<w:documentProtection w:edit="readOnly" w:enforcement="1"/><w:defaultTabStop w:val="720"/><w:compat>'
        '<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>'
    )
    pkg.write(out / "foreign-content-copy-source.docx", document(body), styles(extra), settings_xml, theme())


def _foreign_content_copy_destination(out):
    pkg = Package()
    rel_ns = NS["r"]
    # Header: the main namespace as the default, attributes through x
    header = (
        f'{XML_DECL}<hdr xmlns="{W}" xmlns:x="{W}" xmlns:r="{rel_ns}">'
        '<p><pPr><pStyle x:val="DestinationHeader"/></pPr><r><t xml:space="preserve">Destination header: paste target</t></r></p></hdr>'
    )
    hid = pkg.part("header1.xml", header, f"{CT}header+xml", "header")
    extra = (
        '<w:style w:type="paragraph" w:customStyle="1" w:styleId="DestinationBody"><w:name w:val="Destination Body"/><w:basedOn w:val="Normal"/><w:qFormat/>'
        '<w:pPr><w:spacing w:after="240"/></w:pPr><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman"/></w:rPr></w:style>'
        '<w:style w:type="paragraph" w:customStyle="1" w:styleId="DestinationHeader"><w:name w:val="Destination Header"/><w:basedOn w:val="Normal"/>'
        '<w:pPr><w:spacing w:after="0"/><w:jc w:val="right"/></w:pPr><w:rPr><w:color w:val="595959"/><w:sz w:val="18"/><w:szCs w:val="18"/></w:rPr></w:style>'
        '<w:style w:type="character" w:customStyle="1" w:styleId="DestinationStrong"><w:name w:val="Destination Strong"/><w:basedOn w:val="DefaultParagraphFont"/><w:qFormat/>'
        '<w:rPr><w:b/><w:bCs/><w:caps/></w:rPr></w:style>'
    )

    def xp(text, style, run_style=None):
        rpr = f'<x:rPr><x:rStyle x:val="{run_style}"/></x:rPr>' if run_style else ""
        return f'<x:p><x:pPr><x:pStyle x:val="{style}"/></x:pPr><x:r>{rpr}<x:t xml:space="preserve">{escape(text)}</x:t></x:r></x:p>'

    # Body: the main namespace as x; m, w14 and the drawing namespaces not declared
    body = "".join((
        xp("Destination document", "DestinationBody", "DestinationStrong"),
        xp("Body paste target: the main namespace is bound to x, and math, Word 2010 and drawing namespaces are not declared.", "DestinationBody"),
        xp("Second paste target, after the first.", "DestinationBody"),
    ))
    sect = (
        f'<x:sectPr><x:headerReference x:type="default" r:id="{hid}"/><x:pgSz x:w="12240" x:h="15840"/>'
        '<x:pgMar x:top="1440" x:right="1440" x:bottom="1440" x:left="1440" x:header="720" x:footer="720" x:gutter="0"/></x:sectPr>'
    )
    doc = f'{XML_DECL}<x:document xmlns:x="{W}" xmlns:r="{rel_ns}"><x:body>{body}{sect}</x:body></x:document>'
    pkg.write(out / "foreign-content-copy-destination.docx", doc, styles(extra), None, theme())


def foreign_content_copy(out):
    """A source with an inline equation, an editable range under read-only protection, a 3-D run, a content control with unmodelled properties, a picture and styles the destination lacks; a destination with other prefixes, its own styles and a header."""
    _foreign_content_copy_source(out)
    _foreign_content_copy_destination(out)


FIXTURES.append(foreign_content_copy)



def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else HERE
    out.mkdir(parents=True, exist_ok=True)
    for build in FIXTURES:
        build(out)
    print(f"wrote {len(FIXTURES)} fixture builders' packages to {out}")


if __name__ == "__main__":
    main()
