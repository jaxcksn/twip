"""Writes the editor test fixtures into tests/fixtures/editor/ (next to this file).

One FX-<ID>.docx per fixture family (see that folder's README.md for what each exercises). Standard library only and
no randomness: the same bytes on every run. Every package is plain OOXML that Word opens without a repair prompt.
Fonts are limited to Calibri, Times New Roman, Arial and Segoe UI Emoji.
Run: python -I crates/twip/tests/fixtures/make-fixtures.py [out dir]
"""

import struct
import sys
import zipfile
import zlib
from pathlib import Path
from xml.sax.saxutils import escape

EMU = 914400  # per inch
TEXT_WIDTH = 9360  # twips between 1" margins on a Letter page
CELL_MARGINS = 216  # default left + right cell margins (108 each)
STAMP = (2026, 1, 1, 0, 0, 0)

NS = {
    "w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    "r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    "wp": "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
    "a": "http://schemas.openxmlformats.org/drawingml/2006/main",
    "pic": "http://schemas.openxmlformats.org/drawingml/2006/picture",
    "wps": "http://schemas.microsoft.com/office/word/2010/wordprocessingShape",
    "mc": "http://schemas.openxmlformats.org/markup-compatibility/2006",
    "w14": "http://schemas.microsoft.com/office/word/2010/wordml",
    "w15": "http://schemas.microsoft.com/office/word/2012/wordml",
    "wp14": "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing",
}
NAMESPACES = " ".join(f'xmlns:{k}="{v}"' for k, v in NS.items()) + ' mc:Ignorable="w14 w15 wp14"'
XML_DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/"
CT = "application/vnd.openxmlformats-officedocument.wordprocessingml."

WORDS = (
    "the board finds that presenter colleague budget meeting order record products advisor partner inventory forecast "
    "update proposal distribution account section relevant hereby granted denied filed served notice group vendors "
    "agreement contract logistics savings claim relief practical marketing onboarding chapter pilot team results "
    "schedule promotion support plan receipt export region state division network regional corporate staff ledger "
    "entry date scheduled maintenance requirement newsletter brief estimate profitable trend accessibility standard "
    "review usage management insights suggestions tax fact matter cause action"
).split()


def filler(n, start=0):
    """n words of office-flavoured text, schedule-cased, the same for the same arguments."""
    out = []
    for i in range(n):
        word = WORDS[(start * 13 + i * 7) % len(WORDS)]
        out.append(word.capitalize() if i % 12 == 0 else word)
        if i % 12 == 11 or i == n - 1:
            out[-1] += "."
    return " ".join(out)


# ---- runs and paragraphs ------------------------------------------------------------------------------------------


def font(name):
    return f'<w:rFonts w:ascii="{name}" w:hAnsi="{name}" w:eastAsia="{name}" w:cs="{name}"/>'


B, I, U = "<w:b/>", "<w:i/>", '<w:u w:val="single"/>'
EMOJI = font("Segoe UI Emoji")
TAB = "<w:r><w:tab/></w:r>"
BR = "<w:r><w:br/></w:r>"
PAGE_BREAK = '<w:r><w:br w:type="page"/></w:r>'
SOFT_HYPHEN = "<w:r><w:softHyphen/></w:r>"
NB_HYPHEN = "<w:r><w:noBreakHyphen/></w:r>"
HEADING = '<w:pStyle w:val="Heading1"/>'
FOOTNOTE_TEXT = '<w:pStyle w:val="FootnoteText"/>'
ENDNOTE_TEXT = '<w:pStyle w:val="EndnoteText"/>'


def r(text, rpr=""):
    rpr = f"<w:rPr>{rpr}</w:rPr>" if rpr else ""
    return f'<w:r>{rpr}<w:t xml:space="preserve">{escape(text)}</w:t></w:r>'


def p(content="", ppr=""):
    if not content and not ppr:
        return "<w:p/>"
    return f"<w:p>{f'<w:pPr>{ppr}</w:pPr>' if ppr else ''}{content}</w:p>"


def P(text, ppr=""):
    return p(r(text), ppr)


def numbered(level, num_id):
    return f'<w:pStyle w:val="ListParagraph"/><w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="{num_id}"/></w:numPr>'


# ---- tables -------------------------------------------------------------------------------------------------------

BORDERS = "".join(
    f'<w:{side} w:val="single" w:sz="4" w:space="0" w:color="000000"/>' for side in ("top", "left", "bottom", "right", "insideH", "insideV")
)


def tc(content, width, tcpr=""):
    """A cell. A cell must end with a paragraph, so one is added after a trailing table (or for empty content)."""
    if not content or content.endswith("</w:tbl>"):
        content += "<w:p/>"
    return f'<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/>{tcpr}</w:tcPr>{content}</w:tc>'


def tr(cells, trpr=""):
    return f"<w:tr>{f'<w:trPr>{trpr}</w:trPr>' if trpr else ''}{''.join(cells)}</w:tr>"


def tbl(rows, widths, fixed=True):
    grid = "".join(f'<w:gridCol w:w="{w}"/>' for w in widths)
    layout = '<w:tblLayout w:type="fixed"/>' if fixed else ""
    return (
        f'<w:tbl><w:tblPr><w:tblW w:w="{sum(widths)}" w:type="dxa"/><w:tblBorders>{BORDERS}</w:tblBorders>{layout}</w:tblPr>'
        f"<w:tblGrid>{grid}</w:tblGrid>{''.join(rows)}</w:tbl>"
    )


def grid(data, widths, fixed=True, header=False):
    """A table from rows of cell contents (block XML, or plain str for a one-paragraph cell)."""
    rows = []
    for n, row in enumerate(data):
        cells = [tc(c if c.startswith("<") or not c else P(c), w) for c, w in zip(row, widths)]
        rows.append(tr(cells, "<w:tblHeader/>" if header and n == 0 else ""))
    return tbl(rows, widths, fixed)


def split(width, n):
    """n column widths summing to width."""
    return [width // n + (1 if i < width % n else 0) for i in range(n)]


# ---- pictures, text boxes, notes, headers -------------------------------------------------------------------------

SQUARE = '<wp:wrapSquare wrapText="bothSides"/>'
TIGHT = (
    '<wp:wrapTight wrapText="bothSides"><wp:wrapPolygon edited="0"><wp:start x="0" y="0"/><wp:lineTo x="0" y="21600"/>'
    '<wp:lineTo x="21600" y="21600"/><wp:lineTo x="21600" y="0"/><wp:lineTo x="0" y="0"/></wp:wrapPolygon></wp:wrapTight>'
)
TOP_BOTTOM = "<wp:wrapTopAndBottom/>"
NONE = "<wp:wrapNone/>"

COLOURS = [(0x2E, 0x75, 0xB6), (0xED, 0x7D, 0x31), (0x70, 0xAD, 0x47), (0x7F, 0x3F, 0xBF)]


def png(rgb, width=64, height=48):
    """A minimal solid-colour RGB PNG."""
    rows = (b"\x00" + bytes(rgb) * width) * height

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")


def position(axis, relative, at):
    """wp:positionH/V; `at` is an offset in inches or an alignment name."""
    value = f"<wp:align>{at}</wp:align>" if isinstance(at, str) else f"<wp:posOffset>{round(at * EMU)}</wp:posOffset>"
    return f'<wp:position{axis} relativeFrom="{relative}">{value}</wp:position{axis}>'


def anchor_at(wrap=SQUARE, h=("column", 0.0), v=("paragraph", 0.0), behind=False, locked=False, in_cell=True):
    return dict(wrap=wrap, h=h, v=v, behind=behind, locked=locked, in_cell=in_cell)


class Doc:
    def __init__(self):
        self.ids = 0
        self.rels = []  # (id, type, target), in creation order
        self.images = {}  # colour index -> relationship id
        self.parts = []  # (name, xml, content type) for headers and footers
        self.footnotes, self.endnotes = [], []
        self.numbering = False
        self.even_and_odd = False
        self.custom_xml = []  # (xml, ds:itemID) per custom XML part (data parts)
        self.glossary = False  # a glossary part with the default content-control placeholder

    def uid(self):
        self.ids += 1
        return self.ids

    def rel(self, kind, target):
        rid = f"rId{len(self.rels) + 10}"
        self.rels.append((rid, REL + kind, target))
        return rid

    def _drawing(self, i, name, cx, cy, graphic, float_at):
        doc_pr = f'<wp:docPr id="{i}" name="{name} {i}"/>'
        if float_at is None:
            return (
                f'<w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/>'
                f'<wp:effectExtent l="0" t="0" r="0" b="0"/>{doc_pr}<wp:cNvGraphicFramePr/>{graphic}</wp:inline></w:drawing>'
            )
        f = float_at
        return (
            f'<w:drawing><wp:anchor distT="45720" distB="45720" distL="114300" distR="114300" simplePos="0" '
            f'relativeHeight="{251658240 + i * 1024}" behindDoc="{int(f["behind"])}" locked="{int(f["locked"])}" '
            f'layoutInCell="{int(f["in_cell"])}" allowOverlap="1"><wp:simplePos x="0" y="0"/>'
            f'{position("H", *f["h"])}{position("V", *f["v"])}<wp:extent cx="{cx}" cy="{cy}"/>'
            f'<wp:effectExtent l="0" t="0" r="0" b="0"/>{f["wrap"]}{doc_pr}<wp:cNvGraphicFramePr/>{graphic}</wp:anchor></w:drawing>'
        )

    def picture(self, w=1.0, h=0.75, colour=0, float_at=None):
        """A picture run: inline, or floating when float_at (from anchor_at) is given. Sizes in inches."""
        if colour not in self.images:
            self.images[colour] = self.rel("image", f"media/image{colour + 1}.png")
        i = self.uid()
        cx, cy = round(w * EMU), round(h * EMU)
        graphic = (
            '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic>'
            f'<pic:nvPicPr><pic:cNvPr id="{i}" name="image{colour + 1}.png"/><pic:cNvPicPr/></pic:nvPicPr>'
            f'<pic:blipFill><a:blip r:embed="{self.images[colour]}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
            f'<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>'
            '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic>'
        )
        return f"<w:r>{self._drawing(i, 'Picture', cx, cy, graphic, float_at)}</w:r>"

    def textbox(self, content, w, h, float_at):
        """A floating DrawingML text box (wps:wsp) holding block content, inside mc:AlternateContent as Word writes it."""
        i = self.uid()
        cx, cy = round(w * EMU), round(h * EMU)
        graphic = (
            '<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp>'
            f'<wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>'
            '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill>'
            '<a:ln w="6350"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></wps:spPr>'
            f"<wps:txbx><w:txbxContent>{content}</w:txbxContent></wps:txbx>"
            '<wps:bodyPr rot="0" vert="horz" wrap="square" lIns="91440" tIns="45720" rIns="91440" bIns="45720" anchor="t" '
            'anchorCtr="0"><a:noAutofit/></wps:bodyPr></wps:wsp></a:graphicData></a:graphic>'
        )
        drawing = self._drawing(i, "Text Box", cx, cy, graphic, float_at)
        return f'<w:r><mc:AlternateContent><mc:Choice Requires="wps">{drawing}</mc:Choice></mc:AlternateContent></w:r>'

    def footnote(self, text):
        n = len(self.footnotes) + 1
        ref = '<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteRef/></w:r>'
        self.footnotes.append(f'<w:footnote w:id="{n}">{p(ref + r(" " + text), FOOTNOTE_TEXT)}</w:footnote>')
        return f'<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="{n}"/></w:r>'

    def endnote(self, text):
        n = len(self.endnotes) + 1
        ref = '<w:r><w:rPr><w:rStyle w:val="EndnoteReference"/></w:rPr><w:endnoteRef/></w:r>'
        self.endnotes.append(f'<w:endnote w:id="{n}">{p(ref + r(" " + text), ENDNOTE_TEXT)}</w:endnote>')
        return f'<w:r><w:rPr><w:rStyle w:val="EndnoteReference"/></w:rPr><w:endnoteReference w:id="{n}"/></w:r>'

    def story(self, kind, which, content):
        """A header or footer part; returns its sectPr reference."""
        tag = "hdr" if kind == "header" else "ftr"
        name = f"{kind}{sum(1 for part in self.parts if part[0].startswith(kind)) + 1}.xml"
        self.parts.append((name, f"{XML_DECL}<w:{tag} {NAMESPACES}>{content}</w:{tag}>", f"{CT}{kind}+xml"))
        return f'<w:{kind}Reference w:type="{which}" r:id="{self.rel(kind, name)}"/>'

    def write(self, path, body, sect_pr=None):
        if body.endswith("</w:tbl>"):
            body += "<w:p/>"  # the body's last block must be a paragraph
        document = f"{XML_DECL}<w:document {NAMESPACES}><w:body>{body}{sect_pr or sect()}</w:body></w:document>"
        styles = STYLES.replace("</w:styles>", PLACEHOLDER_STYLE + "</w:styles>") if self.glossary else STYLES
        files = [("word/document.xml", document, f"{CT}document.main+xml"), ("word/styles.xml", styles, f"{CT}styles+xml")]
        rels = [("rIdStyles", REL + "styles", "styles.xml"), ("rIdSettings", REL + "settings", "settings.xml")]
        notes_pr = ""
        if self.numbering:
            files.append(("word/numbering.xml", NUMBERING, f"{CT}numbering+xml"))
            rels.append(("rIdNumbering", REL + "numbering", "numbering.xml"))
        for kind, notes in (("footnote", self.footnotes), ("endnote", self.endnotes)):
            if notes:
                separators = (
                    f'<w:{kind} w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:{kind}>'
                    f'<w:{kind} w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:{kind}>'
                )
                files.append((f"word/{kind}s.xml", f"{XML_DECL}<w:{kind}s {NAMESPACES}>{separators}{''.join(notes)}</w:{kind}s>", f"{CT}{kind}s+xml"))
                rels.append((f"rId{kind.capitalize()}s", REL + f"{kind}s", f"{kind}s.xml"))
                notes_pr += f'<w:{kind}Pr><w:{kind} w:id="-1"/><w:{kind} w:id="0"/></w:{kind}Pr>'
        settings = (
            f'{XML_DECL}<w:settings xmlns:w="{NS["w"]}"><w:defaultTabStop w:val="720"/>'
            f'{"<w:evenAndOddHeaders/>" if self.even_and_odd else ""}{notes_pr}'
            '<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>'
        )
        files.append(("word/settings.xml", settings, f"{CT}settings+xml"))
        files += [(f"word/{name}", xml, ctype) for name, xml, ctype in self.parts]
        rels += self.rels
        raw = []  # parts with a Default content type (no Override)
        for n, (xml, item_id) in enumerate(self.custom_xml, 1):
            props = f'{XML_DECL}<ds:datastoreItem ds:itemID="{item_id}" xmlns:ds="{DS}"><ds:schemaRefs/></ds:datastoreItem>'
            files.append((f"customXml/itemProps{n}.xml", props, "application/vnd.openxmlformats-officedocument.customXmlProperties+xml"))
            item_rels = (
                f'{XML_DECL}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                f'<Relationship Id="rId1" Type="{REL}customXmlProps" Target="itemProps{n}.xml"/></Relationships>'
            )
            raw += [(f"customXml/item{n}.xml", xml), (f"customXml/_rels/item{n}.xml.rels", item_rels)]
            rels.append((f"rIdCustomXml{n}", REL + "customXml", f"../customXml/item{n}.xml"))
        if self.glossary:
            files.append(("word/glossary/document.xml", GLOSSARY, f"{CT}document.glossary+xml"))
            rels.append(("rIdGlossary", REL + "glossaryDocument", "glossary/document.xml"))
        overrides = "".join(f'<Override PartName="/{name}" ContentType="{ctype}"/>' for name, _, ctype in files)
        content_types = (
            f'{XML_DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/>'
            f"{overrides}</Types>"
        )
        package_rels = (
            f'{XML_DECL}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            f'<Relationship Id="rId1" Type="{REL}officeDocument" Target="word/document.xml"/></Relationships>'
        )
        document_rels = (
            f'{XML_DECL}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            + "".join(f'<Relationship Id="{i}" Type="{t}" Target="{target}"/>' for i, t, target in rels)
            + "</Relationships>"
        )
        entries = [("[Content_Types].xml", content_types), ("_rels/.rels", package_rels), ("word/_rels/document.xml.rels", document_rels)]
        entries += [(name, xml) for name, xml, _ in files] + raw
        entries = [(name, text.encode("utf-8")) for name, text in entries]
        entries += [(f"word/media/image{c + 1}.png", png(COLOURS[c])) for c in sorted(self.images)]
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
            for name, data in entries:
                info = zipfile.ZipInfo(name, STAMP)
                info.create_system = 0  # the same bytes whatever OS writes the archive
                info.external_attr = 0
                z.writestr(info, data, zipfile.ZIP_DEFLATED)


def sect(refs="", landscape=False, title_page=False, kind=""):
    """A sectPr: Letter, 1" margins."""
    size = '<w:pgSz w:w="15840" w:h="12240" w:orient="landscape"/>' if landscape else '<w:pgSz w:w="12240" w:h="15840"/>'
    kind = f'<w:type w:val="{kind}"/>' if kind else ""
    return (
        f"<w:sectPr>{refs}{kind}{size}"
        '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>'
        f'<w:cols w:space="720"/>{"<w:titlePg/>" if title_page else ""}</w:sectPr>'
    )


STYLES = f"""{XML_DECL}
<w:styles xmlns:w="{NS["w"]}">
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:eastAsia="Calibri" w:cs="Calibri"/><w:sz w:val="22"/><w:szCs w:val="22"/><w:lang w:val="en-US"/></w:rPr></w:rPrDefault>
<w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="259" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="240" w:after="0"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/><w:szCs w:val="32"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="ListParagraph"><w:name w:val="List Paragraph"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:ind w:left="720"/><w:contextualSpacing/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Header"><w:name w:val="header"/><w:basedOn w:val="Normal"/><w:pPr><w:tabs><w:tab w:val="center" w:pos="4680"/><w:tab w:val="right" w:pos="9360"/></w:tabs><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Footer"><w:name w:val="footer"/><w:basedOn w:val="Normal"/><w:pPr><w:tabs><w:tab w:val="center" w:pos="4680"/><w:tab w:val="right" w:pos="9360"/></w:tabs><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="FootnoteText"><w:name w:val="footnote text"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:rPr><w:sz w:val="20"/><w:szCs w:val="20"/></w:rPr></w:style>
<w:style w:type="character" w:styleId="FootnoteReference"><w:name w:val="footnote reference"/><w:rPr><w:vertAlign w:val="superscript"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="EndnoteText"><w:name w:val="endnote text"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:rPr><w:sz w:val="20"/><w:szCs w:val="20"/></w:rPr></w:style>
<w:style w:type="character" w:styleId="EndnoteReference"><w:name w:val="endnote reference"/><w:rPr><w:vertAlign w:val="superscript"/></w:rPr></w:style>
<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/><w:tblPr><w:tblInd w:w="0" w:type="dxa"/><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="108" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>
</w:styles>"""


GLOSSARY = f"""{XML_DECL}
<w:glossaryDocument xmlns:w="{NS["w"]}"><w:docParts><w:docPart><w:docPartPr><w:name w:val="DefaultPlaceholder_-1854013440"/><w:category><w:name w:val="General"/><w:gallery w:val="placeholder"/></w:category><w:types><w:type w:val="bbPlcHdr"/></w:types><w:behaviors><w:behavior w:val="content"/></w:behaviors><w:guid w:val="{{11111111-2222-4333-8444-555555555555}}"/></w:docPartPr><w:docPartBody><w:p><w:r><w:rPr><w:rStyle w:val="PlaceholderText"/></w:rPr><w:t>Click or tap here to enter text.</w:t></w:r></w:p></w:docPartBody></w:docPart></w:docParts></w:glossaryDocument>"""


PLACEHOLDER_STYLE = '<w:style w:type="character" w:styleId="PlaceholderText"><w:name w:val="Placeholder Text"/><w:rPr><w:color w:val="808080"/></w:rPr></w:style>'


def _levels(formats):
    out = []
    for lvl in range(9):
        fmt, text, rpr = formats[lvl % len(formats)]
        if fmt == "decimal":
            text = ".".join(f"%{k + 1}" for k in range(lvl + 1)) + "."
        out.append(
            f'<w:lvl w:ilvl="{lvl}"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/><w:lvlText w:val="{text}"/><w:lvlJc w:val="left"/>'
            f'<w:pPr><w:ind w:left="{720 * (lvl + 1)}" w:hanging="{360 if fmt == "bullet" else 360 + 216 * lvl}"/></w:pPr>{rpr}</w:lvl>'
        )
    return "".join(out)


_ARIAL = f"<w:rPr>{font('Arial')}</w:rPr>"
NUMBERING = (
    f"{XML_DECL}<w:numbering {NAMESPACES}>"
    f'<w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="multilevel"/>{_levels([("decimal", "", "")])}</w:abstractNum>'
    '<w:abstractNum w:abstractNumId="1"><w:multiLevelType w:val="hybridMultilevel"/>'
    f'{_levels([("bullet", chr(0x2022), _ARIAL), ("bullet", chr(0x2013), _ARIAL), ("bullet", chr(0x25AA), _ARIAL)])}</w:abstractNum>'
    '<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>'
    '<w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>'
    '<w:num w:numId="3"><w:abstractNumId w:val="0"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="1"/></w:lvlOverride></w:num>'
    "</w:numbering>"
)
DECIMAL, BULLET, DECIMAL_RESTART = 1, 2, 3


# ---- fixtures -----------------------------------------------------------------------------------------------------


def fx_text(d):
    tabs = '<w:tabs><w:tab w:val="left" w:pos="2880"/><w:tab w:val="center" w:pos="5760"/><w:tab w:val="right" w:pos="9000"/></w:tabs>'
    mixed = (
        r("Plain, ") + r("bold", B) + r(", ") + r("italic", I) + r(", ") + r("underlined", U) + r(", ") + r("bold italic", B + I)
        + r(", ") + r("Arial", font("Arial")) + r(" and ") + r("Times New Roman 16 pt", font("Times New Roman") + '<w:sz w:val="32"/><w:szCs w:val="32"/>')
        + r(", ") + r("dark red", '<w:color w:val="C00000"/>') + r(", E = mc") + r("2", '<w:vertAlign w:val="superscript"/>')
        + r(", H") + r("2", '<w:vertAlign w:val="subscript"/>') + r("O, mid-word for") + r("matt", B) + r("ing", I) + r(" changes. ")
        + r(filler(40, 3))
    )
    body = [
        P("FX-TEXT", HEADING),
        P("Short paragraph."),
        P(filler(110, 1)),
        p(mixed),
        P("Trailing spaces at the end of this paragraph:     "),
        P(filler(14, 5) + "            " + filler(40, 9) + "      "),
        P("Multiple    spaces    between    words,  and  two  here."),
        P("A long word: " + "Pneumonoultramicroscopicsilicovolcanoconiosis" * 5 + " and then ordinary words to finish the paragraph."),
        p(),
        p(),
        P("The two paragraphs above are empty."),
        p(r("Line one") + BR + r("Line two, after a line break") + BR + BR + r("Line four, after an empty line")),
        p(BR),
        p(r("Name") + TAB + r("Centred") + TAB + r("Right"), tabs),
        p(TAB + r("Starts with a tab") + TAB + TAB + r("after two tabs")),
        p(r("Default stops:") + TAB + r("a") + TAB + r("b") + TAB + r("c")),
        P(filler(60, 11), '<w:jc w:val="both"/>'),
        P("Centred paragraph.", '<w:jc w:val="center"/>'),
        P("Right-aligned paragraph.", '<w:jc w:val="right"/>'),
        P(filler(45, 13), '<w:ind w:firstLine="720"/>'),
        P(filler(45, 17), '<w:ind w:left="720" w:hanging="720"/>'),
        P("Last paragraph."),
    ]
    return "".join(body)


def fx_pages(d):
    body = [P("Part one", HEADING)]
    body += [P(filler(110, i)) for i in range(9)]
    body.append(p(r("Text before a page break.") + PAGE_BREAK + r("Text after the page break, at the top of the next page.")))
    body += [P(filler(110, 20 + i)) for i in range(5)]
    body += [P(f"Keep-with-next chain, item {k + 1}: " + filler(18, k), "<w:keepNext/>") for k in range(4)]
    body.append(P("End of the keep-with-next chain. " + filler(80, 7)))
    body += [P(filler(110, 30 + i)) for i in range(2)]
    body.append(P("Keep lines together: " + filler(160, 9), "<w:keepLines/>"))
    body.append(P("Widow control on: " + filler(150, 21), "<w:widowControl/>"))
    body.append(P("Widow control off: " + filler(150, 23), '<w:widowControl w:val="0"/>'))
    body += [P(filler(110, 40 + i)) for i in range(3)]
    body.append(P("Page break before this paragraph.", "<w:pageBreakBefore/>"))
    body += [P(filler(110, 50 + i)) for i in range(4)]
    body.append(P("Last paragraph of section 1 (portrait); a next-page section break follows.", sect()))
    body.append(P("Part two, section 2 (landscape)", HEADING))
    body += [P(filler(110, 60 + i)) for i in range(5)]
    return "".join(body), sect(landscape=True, kind="nextPage")


def fx_unicode(d):
    def line(label, *runs):
        return p(r(label + ": ", B) + "".join(runs))

    emoji = lambda text: r(text, EMOJI)  # noqa: E731
    body = [
        P("FX-UNICODE", HEADING),
        line("Combining marks", r("é ạ̈ ñ ố Z̵ q̣̇ café Amélie Zöe")),
        line("Precomposed, for comparison", r("é ä ñ café Amélie Zoë")),
        line("Emoji ZWJ sequences", emoji("\U0001F468‍\U0001F469‍\U0001F467‍\U0001F466 \U0001F469\U0001F3FD‍\U0001F4BB "
                                          "\U0001F3F3️‍\U0001F308 \U0001F9D1‍\U0001F91D‍\U0001F9D1")),
        line("Emoji with skin-tone modifiers and variation selectors", emoji("\U0001F44D\U0001F3FB \U0001F44D\U0001F3FF ✌️ ❤️")),
        line("Flags", emoji("\U0001F1FA\U0001F1F8 \U0001F1EF\U0001F1F5 \U0001F1EC\U0001F1E7 \U0001F1E9\U0001F1EA")),
        line("CJK", r("日本語のテキスト。中文文本。한국어 텍스트.")),
        line("Surrogate pairs, not emoji", r("\U0001D49C\U0001D4B7\U0001D4B8 \U0001D518\U0001D52B\U0001D526\U0001D520 \U00020000\U00020001\U00020002 \U00010437")),
        line("Soft hyphens", r(filler(16, 2) + " extra"), SOFT_HYPHEN, r("ordi"), SOFT_HYPHEN, r("narily"), r(" long"), SOFT_HYPHEN,
             r("winded"), r(" incom"), SOFT_HYPHEN, r("pre"), SOFT_HYPHEN, r("hensibilities " + filler(20, 4))),
        line("Non-breaking spaces and hyphens", r(("10 kg, Mr. Smith, 2026 AD, section " + "12, ") * 4 + "pre"),
             NB_HYPHEN, r("pilot and non"), NB_HYPHEN, r("breaking.")),
        p(r("Mixed: text ") + emoji("\U0001F468‍\U0001F469‍\U0001F467") + r(" with é and 日本 and ")
          + emoji("\U0001F1FA\U0001F1F8") + r(" and \U0001D49C inline, then " + filler(30, 6))),
    ]
    return "".join(body)


def fx_lists(d):
    d.numbering = True
    dec = [(0, "First item"), (0, "Second item, long enough to wrap: " + filler(40, 1)), (1, "Sub-item a"), (1, "Sub-item b"),
           (2, "Sub-sub-item i"), (2, "Sub-sub-item ii"), (1, "Sub-item c"), (0, ""), (0, "Item after the empty item")]
    bul = [(0, "Bullet one"), (1, "Nested bullet"), (2, "Third-level bullet"), (1, ""), (1, "After the empty bullet"), (0, "Bullet two")]
    body = [P("FX-LISTS", HEADING), P("Numbered, multi-level:")]
    body += [P(text, numbered(lvl, DECIMAL)) if text else p("", numbered(lvl, DECIMAL)) for lvl, text in dec]
    body.append(P("Bulleted, multi-level:"))
    body += [P(text, numbered(lvl, BULLET)) if text else p("", numbered(lvl, BULLET)) for lvl, text in bul]
    body.append(P("A list inside a table cell:"))
    cell_list = "".join(P(t, numbered(l, DECIMAL_RESTART)) for l, t in [(0, "Cell item one"), (1, "Cell sub-item"), (0, "Cell item two")])
    cell_bullets = P("Bullets in a cell:") + "".join(P(t, numbered(l, BULLET)) for l, t in [(0, "Cell bullet"), (1, "Nested cell bullet")])
    body.append(grid([[cell_list, cell_bullets], ["Plain cell", "Another plain cell"]], split(TEXT_WIDTH, 2)))
    body.append(P("Paragraph after the table."))
    return "".join(body)


def fx_tbl_basic(d):
    w3 = split(TEXT_WIDTH, 3)
    several = P("First paragraph of the cell.") + P("Second paragraph.") + p() + P("Fourth paragraph, after an empty one.")
    body = [
        grid([["A1, the document starts with this table", "", "C1"], ["", several, ""], ["A3", "B3", ""]], w3, fixed=False),
        P("Paragraph between the first and second tables."),
        grid([[f"T2 R{r + 1} C{c + 1}" for c in range(3)] for r in range(3)], w3, fixed=False),
        grid([[f"T3 R{r + 1} C{c + 1}" for c in range(2)] for r in range(2)], [3000, 3000], fixed=False),
        P("The second and third tables above are adjacent in the XML, with no paragraph between them (Word joins them into one table)."),
        grid([[f"T4 R{r + 1} C{c + 1}" for c in range(3)] for r in range(2)], w3, fixed=False),
        p(),
        grid([[f"T5 R{r + 1} C{c + 1}" for c in range(3)] for r in range(2)], w3, fixed=False),
        P("The fourth and fifth tables above are separated only by an empty paragraph."),
        P("Paragraph before the last table."),
        grid([[f"End R{r + 1} C{c + 1}" for c in range(3)] for r in range(3)], w3, fixed=False),
        p(),
    ]
    return "".join(body)


def fx_tbl_merged(d):
    w = TEXT_WIDTH // 6
    widths = [w] * 6

    def cell(text, span=1, merge=""):
        tcpr = (f'<w:gridSpan w:val="{span}"/>' if span > 1 else "") + (f'<w:vMerge w:val="{merge}"/>' if merge == "restart" else "<w:vMerge/>" if merge else "")
        return tc(P(text) if text else "", w * span, tcpr)

    def before_after(before, after):
        out = (f'<w:gridBefore w:val="{before}"/>' if before else "") + (f'<w:gridAfter w:val="{after}"/>' if after else "")
        return out + (f'<w:wBefore w:w="{w * before}" w:type="dxa"/>' if before else "") + (f'<w:wAfter w:w="{w * after}" w:type="dxa"/>' if after else "")

    rows = [
        tr([cell(f"Col {c + 1}") for c in range(6)]),
        tr([cell("Span 2", 2), cell("Span 2", 2), cell("1"), cell("1")]),
        tr([cell("Vertical merge over 3 rows", merge="restart"), cell("1"), cell("Span 3", 3), cell("1")]),
        tr([cell("", merge="continue"), cell("Span 2 + vertical merge over 2 rows", 2, "restart"), cell("1"), cell("1"), cell("1")]),
        tr([cell("", merge="continue"), cell("", 2, "continue"), cell("1"), cell("Span 2", 2)]),
        tr([cell("gridBefore 1 / gridAfter 1"), cell("1"), cell("1"), cell("1")], before_after(1, 1)),
        tr([cell("gridBefore 2", 2), cell("1"), cell("1")], before_after(2, 0)),
        tr([cell("gridAfter 3"), cell("1"), cell("1")], before_after(0, 3)),
        tr([cell("Span 3, taller: " + filler(30, 2), 3), cell("Span 3", 3)]),
        tr([cell("Span 6", 6)]),
    ]
    body = [P("FX-TBL-MERGED", HEADING), tbl(rows, widths), P("Paragraph after the table.")]
    return "".join(body)


def fx_tbl_pages(d):
    rest = TEXT_WIDTH - 1800
    widths = [1800, rest // 2, rest - rest // 2]
    rows = [tr([tc(P(t), w) for t, w in zip(("No.", "Repeated header row", "Second column"), widths)], "<w:tblHeader/>")]
    for n in range(1, 33):
        if n == 26:
            content = "".join(P(f"Row {n} cannot split, paragraph {k + 1}: " + filler(8, k)) for k in range(6))
            rows.append(tr([tc(P(str(n)), widths[0]), tc(content, widths[1]), tc(P("This row has cantSplit."), widths[2])], "<w:cantSplit/>"))
        elif n % 7 == 0:
            content = "".join(P(f"Row {n}, paragraph {k + 1}: " + filler(4, n + k)) for k in range(4))
            rows.append(tr([tc(P(str(n)), widths[0]), tc(content, widths[1]), tc(P("A tall row that may split across pages."), widths[2])]))
        else:
            rows.append(tr([tc(P(str(n)), widths[0]), tc(P(filler(3, n)), widths[1]), tc(P(filler(2, n + 3)), widths[2])]))
    body = [P("FX-TBL-PAGES", HEADING), P("A table of about three pages follows."), tbl(rows, widths), P("Paragraph after the table.")]
    return "".join(body)


def fx_nest_2(d):
    outer = split(TEXT_WIDTH, 2)
    inner_w = split(outer[0] - CELL_MARGINS, 2)
    inner = lambda tag: grid([[f"{tag} R{r + 1} C{c + 1}" for c in range(2)] for r in range(2)], inner_w)  # noqa: E731
    body = [
        P("FX-NEST-2", HEADING),
        grid([
            [P("Text before the inner table.") + inner("Inner A") + P("Text after the inner table."), inner("Only content")],
            ["Plain outer cell", P("Outer text, then an inner table of 3 rows:") + grid([[f"Inner B R{r + 1}", "x"] for r in range(3)], inner_w)],
        ], outer),
        P("Paragraph after the outer table."),
    ]
    return "".join(body)


def fx_nest_3(d):
    outer = split(TEXT_WIDTH, 2)

    def level3(width):
        return grid([[f"L3 R{r + 1} C{c + 1}" for c in range(2)] for r in range(2)], split(width, 2))

    def level2(width, tag):
        w2 = split(width, 2)
        return grid([[P(f"{tag}: L2 before L3") + level3(w2[0] - CELL_MARGINS) + P("L2 after L3"), f"{tag} L2 R1 C2"], [f"{tag} L2 R2 C1", f"{tag} L2 R2 C2"]], w2)

    def level2_plain(width, tag):
        return grid([[f"{tag} R{r + 1} C{c + 1}" for c in range(2)] for r in range(2)], split(width, 2))

    inner = outer[0] - CELL_MARGINS
    rows = [
        tr([
            tc(P("Level 1, before") + level2(inner, "A") + P("Level 1, after"), outer[0]),
            tc(P("Two sibling inner tables:") + level2_plain(inner, "Sibling 1") + P("Between the siblings") + level2_plain(inner, "Sibling 2"), outer[1]),
        ]),
        tr([tc(P("Horizontally merged cell with an inner table:") + level2(TEXT_WIDTH - CELL_MARGINS, "Merged"), TEXT_WIDTH, '<w:gridSpan w:val="2"/>')]),
    ]
    body = [P("FX-NEST-3", HEADING), tbl(rows, outer), P("Paragraph after the outer table.")]
    return "".join(body)


def deep(level, depth, width):
    """A 2x2 table at `level` whose first cell nests the next level, down to `depth`."""
    first, second = width - 900, 900
    inner = deep(level + 1, depth, first - CELL_MARGINS) if level < depth else ""
    cell = P(f"Level {level}, before") + inner + P(f"Level {level}, after")
    return grid([[cell, f"L{level}"], [f"Level {level}, row 2", "x"]], [first, second])


def fx_nest_deep(depth):
    def build(d):
        return P(f"FX-NEST-DEEP, nesting depth {depth}", HEADING) + deep(1, depth, TEXT_WIDTH) + P("Paragraph after the outer table.")

    return build


def fx_nest_mixed(d):
    d.numbering = True
    outer = [5400, 3960]
    w2 = split(outer[0] - CELL_MARGINS, 2)
    level3 = grid([[P("L3 list:") + P("L3 numbered", numbered(0, DECIMAL_RESTART)) + P("L3 sub", numbered(1, DECIMAL_RESTART)), "L3 C2"]],
                  split(w2[1] - CELL_MARGINS, 2))
    float_pic = d.picture(0.9, 0.6, 1, anchor_at(SQUARE, ("column", 0.0), ("paragraph", 0.0)))
    box = d.textbox(P("Text box in a nested cell."), 1.3, 0.7, anchor_at(SQUARE, ("column", 0.0), ("paragraph", 0.0)))
    level2 = grid([
        ["".join(P(t, numbered(l, DECIMAL)) for l, t in [(0, "Numbered in L2"), (1, "Sub-item"), (0, "Second")])
         + p(r("Inline ") + d.picture(0.8, 0.6, 0) + r(" picture.")),
         p(float_pic + r("Floating picture anchored in this cell. " + filler(30, 3)))],
        [p(box + r("Paragraph holding a text box anchor. " + filler(30, 5))),
         P("Bullet", numbered(0, BULLET)) + P("Nested bullet", numbered(1, BULLET)) + level3],
    ], w2)
    long_w = split(TEXT_WIDTH - CELL_MARGINS, 2)
    long_inner = grid([[f"Long nested table, row {n + 1}", filler(3, n)] for n in range(32)], long_w, header=True)
    rows = [
        tr([tc(P("Outer cell with a nested table:") + level2 + P("After the nested table."), outer[0]), tc(P(filler(40, 1)), outer[1])]),
        tr([tc(P("A nested table long enough to span a page:") + long_inner, TEXT_WIDTH, '<w:gridSpan w:val="2"/>')]),
    ]
    return P("FX-NEST-MIXED", HEADING) + tbl(rows, outer) + P("Paragraph after the outer table.")


def fx_obj(d):
    pic = d.picture
    body = [
        P("FX-OBJ", HEADING),
        p(pic(0.6, 0.45, 0) + r(" Inline picture at the start of the line. " + filler(30, 1))),
        p(r(filler(9, 2) + " ") + pic(0.6, 0.45, 1) + r(" inline picture in the middle of the line. " + filler(25, 3))),
        p(r("Inline picture at the end of the line: ") + pic(0.6, 0.45, 2)),
        p(pic(1.5, 1.0, 3)),
    ]
    floats = [
        ("Square wrap", anchor_at(SQUARE, ("column", 0.0), ("paragraph", 0.0))),
        ("Tight wrap", anchor_at(TIGHT, ("column", 4.5), ("paragraph", 0.1))),
        ("Top and bottom wrap", anchor_at(TOP_BOTTOM, ("column", 1.5), ("paragraph", 0.3))),
        ("Behind text (wrapNone, behindDoc)", anchor_at(NONE, ("column", 2.0), ("paragraph", 0.1), behind=True)),
        ("In front of text (wrapNone)", anchor_at(NONE, ("column", 3.0), ("paragraph", 0.1))),
        ("Locked anchor, right-aligned to the margin", anchor_at(SQUARE, ("margin", "right"), ("paragraph", 0.0), locked=True)),
    ]
    for n, (label, at) in enumerate(floats):
        body.append(p(pic(1.4, 1.0, n % 4, at) + r(f"{label}. ", B) + r(filler(110, 10 + n))))
    overlap = pic(1.4, 1.0, 0, anchor_at(SQUARE, ("column", 1.0), ("paragraph", 0.0))) + pic(1.4, 1.0, 1, anchor_at(SQUARE, ("column", 1.7), ("paragraph", 0.5)))
    body.append(p(overlap + r("Two overlapping floats. ", B) + r(filler(110, 20))))
    body.append(p(pic(1.4, 0.75, 2, anchor_at(SQUARE, ("column", 3.0), ("page", 9.1))) + r("Float near the page bottom (9.1 in from the page top). ", B) + r(filler(60, 21))))
    body += [P(filler(110, 30 + n)) for n in range(3)]
    return "".join(body)


def fx_obj_cell(d):
    outer = split(TEXT_WIDTH, 2)
    inner_w = split(outer[0] - CELL_MARGINS, 2)
    at = lambda: anchor_at(SQUARE, ("column", 0.0), ("paragraph", 0.0))  # noqa: E731
    nested = grid([
        [p(r("Nested inline ") + d.picture(0.6, 0.45, 2) + r(" picture.")), p(d.picture(0.7, 0.5, 3, at()) + r("Nested float. " + filler(25, 4)))],
        ["Nested R2 C1", p(r("Nested, inline alone:")) + p(d.picture(0.6, 0.45, 0))],
    ], inner_w)
    body = [
        P("FX-OBJ-CELL", HEADING),
        grid([
            [p(d.picture(0.8, 0.6, 0) + r(" inline picture at the start of a cell. " + filler(10, 1))),
             p(d.picture(1.0, 0.7, 1, at()) + r("Floating picture anchored in the cell. " + filler(40, 2)))],
            [P("Nested table with pictures:") + nested, p(r(filler(8, 3) + " ") + d.picture(0.6, 0.45, 2) + r(" " + filler(8, 5)))],
        ], outer),
        P("Paragraph after the table."),
    ]
    return "".join(body)


def fx_stories(d):
    d.even_and_odd = True
    page = '<w:fldSimple w:instr=" PAGE "><w:r><w:t>1</w:t></w:r></w:fldSimple>'
    style = lambda s: f'<w:pStyle w:val="{s}"/>'  # noqa: E731
    refs = (
        d.story("header", "default", P("Default (odd-page) header", style("Header")))
        + d.story("header", "first", P("First-page header", style("Header")))
        + d.story("header", "even", P("Even-page header", style("Header")))
        + d.story("footer", "default", p(r("Default footer, page ") + page, style("Footer")))
        + d.story("footer", "first", P("First-page footer", style("Footer")))
        + d.story("footer", "even", p(r("Even-page footer, page ") + page, style("Footer")))
    )
    box = d.textbox(P("Text box, first paragraph.") + P("Text box, second paragraph."), 2.0, 0.9, anchor_at(SQUARE, ("margin", "right"), ("paragraph", 0.0)))
    body = [P("FX-STORIES", HEADING), p(box + r("This paragraph anchors a text box. " + filler(60, 1)))]
    for n in range(16):
        content = r(filler(100, n + 2))
        if n % 3 == 0:
            content += d.footnote(f"Footnote {len(d.footnotes) + 1}: " + filler(14, n))
        if n % 5 == 2:
            content += d.endnote(f"Endnote {len(d.endnotes) + 1}: " + filler(12, n))
        body.append(p(content))
    return "".join(body), sect(refs, title_page=True)


def large(d, nested):
    widths = [2360] + [1000] * 7
    l2 = [1444, 700]
    tight = '<w:spacing w:after="0"/>'
    rows = [tr([tc(P(f"Column {c + 1}", tight), w) for c, w in enumerate(widths)], "<w:tblHeader/>")]
    for n in range(1, 201):
        first = P(f"Row {n}, first column", tight)
        if nested and n % 10 == 0:
            level3 = grid([[f"L3 {n}", "x"]], split(l2[0] - CELL_MARGINS, 2))
            first += grid([[P("L2") + level3, "y"]], l2)
        rows.append(tr([tc(first, widths[0])] + [tc(P(f"{n}.{c} value", tight), w) for c, w in enumerate(widths[1:], 2)]))
    return tbl(rows, widths) + p()


def fx_empty(d):
    return p()


def fx_empty_table(d):
    return grid([[f"R{r + 1} C{c + 1}" for c in range(3)] for r in range(2)], split(TEXT_WIDTH, 3)) + p()


def fx_end_table(d):
    return P("FX-END-TABLE", HEADING) + P(filler(40, 1)) + grid([[f"R{r + 1} C{c + 1}" for c in range(3)] for r in range(3)], split(TEXT_WIDTH, 3)) + p()


def fx_exact_row(d):
    widths = split(TEXT_WIDTH, 3)
    height = lambda twips, rule: f'<w:trHeight w:val="{twips}" w:hRule="{rule}"/>'  # noqa: E731
    rows = [
        tr([tc(P(t), w) for t, w in zip(("Row", "Content", "Height rule"), widths)]),
        tr([tc(P("2"), widths[0]), tc(P("Three paragraphs:") + P("second") + P("third"), widths[1]), tc(P("exact 360 twips, clipped"), widths[2])], height(360, "exact")),
        tr([tc(P("3"), widths[0]), tc(P(filler(40, 2)), widths[1]), tc(P("exact 240 twips, wrapped text clipped"), widths[2])], height(240, "exact")),
        tr([tc(P("4"), widths[0]), tc(p(d.picture(1.0, 1.0, 0)), widths[1]), tc(P("exact 600 twips, 1 in picture clipped"), widths[2])], height(600, "exact")),
        tr([tc(P("5"), widths[0]), tc(P("Short"), widths[1]), tc(P("atLeast 720 twips, for contrast"), widths[2])], height(720, "atLeast")),
        tr([tc(P("6"), widths[0]), tc(P("Short"), widths[1]), tc(P("exact 720 twips, fits"), widths[2])], height(720, "exact")),
    ]
    return P("FX-EXACT-ROW", HEADING) + tbl(rows, widths) + P("Paragraph after the table.")


def fx_repeat_region(d):
    widths = [3600, 2880, 2880]
    sdt_id = iter(range(1001, 2000))

    def checkbox(checked):
        glyph = "☒" if checked else "☐"
        return (
            f'<w:sdt><w:sdtPr><w:id w:val="{next(sdt_id)}"/><w14:checkbox><w14:checked w14:val="{int(checked)}"/>'
            '<w14:checkedState w14:val="2612" w14:font="Segoe UI Emoji"/><w14:uncheckedState w14:val="2610" w14:font="Segoe UI Emoji"/>'
            f"</w14:checkbox></w:sdtPr><w:sdtContent>{r(glyph, EMOJI)}</w:sdtContent></w:sdt>"
        )

    def item(n):
        row = tr([tc(P(f"Item {n}"), widths[0]), tc(P(f"2026-0{n}-01"), widths[1]), tc(p(checkbox(n == 2) + r(" done")), widths[2])])
        return f'<w:sdt><w:sdtPr><w:id w:val="{next(sdt_id)}"/><w15:repeatingSectionItem/></w:sdtPr><w:sdtContent>{row}</w:sdtContent></w:sdt>'

    section = (
        f'<w:sdt><w:sdtPr><w:alias w:val="Items"/><w:tag w:val="items"/><w:id w:val="{next(sdt_id)}"/><w15:repeatingSection/></w:sdtPr>'
        f'<w:sdtContent>{"".join(item(n) for n in (1, 2, 3))}</w:sdtContent></w:sdt>'
    )
    picture_cc = f'<w:sdt><w:sdtPr><w:id w:val="{next(sdt_id)}"/><w:picture/></w:sdtPr><w:sdtContent>{d.picture(1.0, 0.75, 1)}</w:sdtContent></w:sdt>'
    header = tr([tc(P(t), w) for t, w in zip(("Name", "Date", "Done"), widths)], "<w:tblHeader/>")
    footer = tr([tc(P("Picture content control:"), widths[0]), tc(p(picture_cc), widths[1] + widths[2], '<w:gridSpan w:val="2"/>')])
    grid_xml = "".join(f'<w:gridCol w:w="{w}"/>' for w in widths)
    table = (
        f'<w:tbl><w:tblPr><w:tblW w:w="{sum(widths)}" w:type="dxa"/><w:tblBorders>{BORDERS}</w:tblBorders><w:tblLayout w:type="fixed"/></w:tblPr>'
        f"<w:tblGrid>{grid_xml}</w:tblGrid>{header}{section}{footer}</w:tbl>"
    )
    return P("FX-REPEAT-REGION", HEADING) + P("Rows 2-4 are a repeating section content control.") + table + P("Paragraph after the table.")


def _nest_copy(d, build, n):
    """A table fixture's content one level (n = 2) or two levels (n = 3) down: its heading stays in the body, the rest
    goes in the cell of a 1x1 wrapper table (inside a second wrapper for n = 3), its own tables built that much narrower;
    then "Paragraph after the wrapper table." Paragraph numbers 0..N-1 stay; tables and depths move by n - 1
    (tests/scenarios/matrix-tbl/generate.py)."""
    global TEXT_WIDTH
    base = TEXT_WIDTH
    TEXT_WIDTH = base - (n - 1) * CELL_MARGINS
    try:
        body = build(d)
    finally:
        TEXT_WIDTH = base
    head = ""
    if body.startswith("<w:p>") and HEADING in body[: body.index("</w:p>")]:
        k = body.index("</w:p>") + len("</w:p>")
        head, body = body[:k], body[k:]
    inner = body
    width = base - (n - 1) * CELL_MARGINS
    for level in range(n - 1):
        w = width + level * CELL_MARGINS
        inner = tbl([tr([tc(inner, w)])], [w])
    return head + inner + P("Paragraph after the wrapper table.")


def fx_obj_lock(d):
    # FX-OBJ with every picture's aspect ratio locked, as word processors write pictures they insert.
    return fx_obj(d).replace("<wp:cNvGraphicFramePr/>", '<wp:cNvGraphicFramePr><a:graphicFrameLocks xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" noChangeAspect="1"/></wp:cNvGraphicFramePr>')


def fx_hdr_table(d):
    # A header holding a table (US15-2, LTB-19): it shows on every page, three pages of body under it.
    style = '<w:pStyle w:val="Header"/>'
    widths = split(TEXT_WIDTH, 2)
    header = P("Header before the table.", style) + grid([[f"H R{r + 1} C{c + 1}" for c in range(2)] for r in range(2)], widths) + P("Header after the table.", style)
    refs = d.story("header", "default", header)
    body = [P("FX-HDR-TABLE", HEADING)] + [P(filler(120, n + 1)) for n in range(18)]
    return "".join(body), sect(refs)


# The body paragraphs before a story copy's content (story paragraphs come after the body's in the dump order).
STORY_COPY_BODY = 3


def _story_copy(d, content, where):
    """A fixture holding `content` (a table fixture's body) in the default header or in a text box, after a short body."""
    body = [P(f"Body paragraph {n + 1}.") for n in range(STORY_COPY_BODY)]
    if where == "header":
        return "".join(body), sect(d.story("header", "default", content))
    box = d.textbox(content, 6.5, 8.5, anchor_at(NONE, ("margin", 0.0), ("margin", 0.0)))
    body[0] = p(box + r("Body paragraph 1."))
    return "".join(body)


def _tbl_basic_head(d):
    # FX-TBL-BASIC's content through its third table (and the paragraph after it): a header-sized copy.
    full = fx_tbl_basic(d)
    k = full.index("The second and third tables above")
    return full[: full.index("</w:p>", k) + len("</w:p>")]


def fx_tbl_basic_hdr(d):
    return _story_copy(d, _tbl_basic_head(d), "header")


def fx_tbl_basic_box(d):
    return _story_copy(d, _tbl_basic_head(d), "box")


def fx_nest_2_hdr(d):
    return _story_copy(d, fx_nest_2(d), "header")


def fx_nest_2_box(d):
    return _story_copy(d, fx_nest_2(d), "box")


def fx_tbl_noborder(d):
    # A table without borders (View > Gridlines shows its cell edges dashed).
    widths = split(TEXT_WIDTH, 3)
    rows = "".join(tr([tc(P(f"R{r + 1} C{c + 1}"), widths[c]) for c in range(3)]) for r in range(3))
    grid = "".join(f'<w:gridCol w:w="{w}"/>' for w in widths)
    table = f'<w:tbl><w:tblPr><w:tblW w:w="{sum(widths)}" w:type="dxa"/></w:tblPr><w:tblGrid>{grid}</w:tblGrid>{rows}</w:tbl>'
    return P("FX-TBL-NOBORDER", HEADING) + P("A table without borders:") + table + P("After the table.")


def fx_controls(d):
    # Content controls at block, run and cell level: a rich-text control around two paragraphs, a
    # plain-text control inside a paragraph, and a control around a table cell.
    sdt_id = iter(range(3001, 4000))

    def pr(alias, tag, kind=""):
        return f'<w:sdtPr><w:alias w:val="{alias}"/><w:tag w:val="{tag}"/><w:id w:val="{next(sdt_id)}"/>{kind}</w:sdtPr>'

    block = f'<w:sdt>{pr("Block", "blk", "<w:richText/>")}<w:sdtContent>{P("Inside one")}{P("Inside two")}</w:sdtContent></w:sdt>'
    inline = p(r("Name: ") + f'<w:sdt>{pr("Name", "name", "<w:text/>")}<w:sdtContent>{r("John Doe")}</w:sdtContent></w:sdt>' + r(" end of line."))
    widths = split(TEXT_WIDTH, 2)
    cell = f'<w:sdt>{pr("Cell", "cell")}<w:sdtContent>{tc(P("In the cell control"), widths[1])}</w:sdtContent></w:sdt>'
    rows = [
        tr([tc(P("R1 C1"), widths[0])]).replace("</w:tr>", cell + "</w:tr>"),
        tr([tc(P("R2 C1"), widths[0]), tc(P("R2 C2"), widths[1])]),
    ]
    return P("FX-CONTROLS", HEADING) + P("Before the controls.") + block + inline + tbl(rows, widths) + P("After the table.")


# ---- merge templates -----------------------------------------------------------------------------------------------

DS = "http://schemas.openxmlformats.org/officeDocument/2006/customXml"
PLACEHOLDER_PART = "DefaultPlaceholder_-1854013440"


def fld(code, result="", lock=False, dirty=False, rpr=""):
    """A complex field: begin, the code (a list of text pieces and nested field XML), separate, the result runs, end.
    No result: begin, code, separate, end (an empty result)."""
    attrs = (' w:fldLock="1"' if lock else "") + (' w:dirty="1"' if dirty else "")
    rp = f"<w:rPr>{rpr}</w:rPr>" if rpr else ""
    char = lambda kind, a="": f'<w:r>{rp}<w:fldChar w:fldCharType="{kind}"{a}/></w:r>'  # noqa: E731
    parts = [code] if isinstance(code, str) else code
    instr = "".join(
        x if x.startswith("<") else f'<w:r>{rp}<w:instrText xml:space="preserve">{escape(x)}</w:instrText></w:r>' for x in parts
    )
    res = r(result, rpr) if result else ""
    return char("begin", attrs) + instr + char("separate") + res + char("end")


def mf(name, switches="", result=None, **kw):
    """A MERGEFIELD showing «Column» (Word's mail-merge display) unless another result is given."""
    shown = f"«{name.split('.')[-1]}»" if result is None else result
    return fld(f" MERGEFIELD {name}{(' ' + switches) if switches else ''} ", shown, **kw)


class Sdt:
    """Content-control XML with unique w:id values."""

    def __init__(self, start):
        self.ids = iter(range(start, start + 1000))

    def pr(self, kind="", alias="", tag="", lock="", extra=""):
        a = f'<w:alias w:val="{alias}"/>' if alias else ""
        t = f'<w:tag w:val="{tag}"/>' if tag else ""
        lk = f'<w:lock w:val="{lock}"/>' if lock else ""
        return f'<w:sdtPr>{a}{t}<w:id w:val="{next(self.ids)}"/>{lk}{extra}{kind}</w:sdtPr>'

    def wrap(self, content, **kw):
        return f"<w:sdt>{self.pr(**kw)}<w:sdtContent>{content}</w:sdtContent></w:sdt>"


def region(sdt, items, alias="", tag=""):
    """A w15:repeatingSection around items (each the XML of one item's rows or blocks)."""
    inner = "".join(sdt.wrap(i, kind="<w15:repeatingSectionItem/>") for i in items)
    return sdt.wrap(inner, kind="<w15:repeatingSection/>", alias=alias, tag=tag)


def table_xml(rows_xml, widths):
    """A table whose rows are given as XML (so rows can sit inside content controls)."""
    grid_xml = "".join(f'<w:gridCol w:w="{w}"/>' for w in widths)
    return (
        f'<w:tbl><w:tblPr><w:tblW w:w="{sum(widths)}" w:type="dxa"/><w:tblBorders>{BORDERS}</w:tblBorders><w:tblLayout w:type="fixed"/></w:tblPr>'
        f"<w:tblGrid>{grid_xml}</w:tblGrid>{rows_xml}</w:tbl>"
    )


def custom_part(d, namespace, root, inner):
    """A custom XML part; returns its ds:itemID (deterministic per document)."""
    item_id = "{%08X-0000-4000-8000-%012X}" % (len(d.custom_xml) + 1, 3003)
    d.custom_xml.append((f'{XML_DECL}<{root} xmlns="{namespace}">{inner}</{root}>', item_id))
    return item_id


def fx_merge_fields(d):
    style = lambda s: f'<w:pStyle w:val="{s}"/>'  # noqa: E731
    refs = d.story("header", "default", p(r("Store: ") + mf("Store.Name"), style("Header"))) + d.story(
        "footer", "default", p(r("Page ") + fld(" PAGE ", "1") + r(" of ") + fld(" NUMPAGES ", "1"), style("Footer"))
    )
    nested_if = fld([' IF ', mf("Statement.Status"), ' = "Paid" "', mf("Statement.Paid_Date", '\\@ "M/d/yyyy"'), '" "" '], "«Paid_Date»")
    simple = '<w:fldSimple w:instr=" MERGEFIELD Recipient.Last_Name "><w:r><w:t>«Last_Name»</w:t></w:r></w:fldSimple>'
    inner_w = split(TEXT_WIDTH - 2 * CELL_MARGINS, 2)
    depth3 = grid([[p(r("Depth 3: ") + mf("Detail.Code")), "x"]], split(inner_w[0] - CELL_MARGINS, 2))
    depth2 = grid([[P("Depth 2") + depth3, "y"]], inner_w)
    depth1 = grid([[P("Depth 1") + depth2]], [TEXT_WIDTH - CELL_MARGINS])
    box = d.textbox(p(r("Box: ") + mf("Manager.Badge")), 2.0, 0.6, anchor_at(SQUARE, ("margin", "right"), ("paragraph", 0.0)))
    switches = ['\\# "$,0.00"', '\\@ "M/d/yyyy"', "\\* Upper", "\\* MERGEFORMAT", '\\# "0.00" \\* MERGEFORMAT']
    renames = [p("".join(r(f"Calc {k * 5 + i + 1}: ") + mf("Calc.Old_Name", switches[i]) + r(" ") for i in range(5))) for k in range(8)]
    body = [
        P("FX-MERGE-FIELDS", HEADING),
        p(r("Cost: ") + mf("Statement.Cost_Amount", '\\# "$,0.00"') + r(" due ") + mf("Statement.Due_Date", '\\@ "M/d/yyyy"') + r(".")),
        p(r("Shipment ") + mf("Shipment.Shipment_Number", "\\* Upper \\* MERGEFORMAT") + r(" issued.")),
        p(r("Paid: ") + nested_if + r(" (IF with nested MERGEFIELDs).")),
        p(r("Simple field: ") + simple + r(".")),
        p(r("Locked: ") + mf("Store.Name", lock=True) + r("; dirty: ") + mf("Store.City", dirty=True) + r(".")),
        p(r("Empty result: [") + mf("Statement.Manager", result="") + r("] end.")),
        p(r("Footnoted field") + d.footnote("Note with ") + r(".")),
        p(box + r("This paragraph anchors a text box holding a field. " + filler(30, 1))),
        depth1,
    ] + renames
    # The footnote's text holds a field too.
    d.footnotes[-1] = d.footnotes[-1].replace(r(" Note with "), r(" Note with ") + mf("Recipient.First_Name"))
    return "".join(body), sect(refs)


def fx_merge_controls(d):
    s = Sdt(5001)
    d.glossary = True
    item_id = custom_part(d, "urn:store:form-data", "data", "<store>Corporate Store</store>")
    binding = f'<w:dataBinding w:prefixMappings="xmlns:ns0=\'urn:store:form-data\'" w:xpath="/ns0:data[1]/ns0:store[1]" w:storeItemID="{item_id}"/>'
    checkbox = (
        '<w14:checkbox><w14:checked w14:val="0"/><w14:checkedState w14:val="2612" w14:font="Segoe UI Emoji"/>'
        '<w14:uncheckedState w14:val="2610" w14:font="Segoe UI Emoji"/></w14:checkbox>'
    )
    date = '<w:date w:fullDate="2026-01-01T00:00:00Z"><w:dateFormat w:val="M/d/yyyy"/><w:lid w:val="en-US"/><w:storeMappedDataAs w:val="dateTime"/><w:calendar w:val="gregorian"/></w:date>'
    dropdown = '<w:dropDownList><w:listItem w:displayText="Paid" w:value="Paid"/><w:listItem w:displayText="Unpaid" w:value="Unpaid"/></w:dropDownList>'
    placeholder = f'<w:placeholder><w:docPart w:val="{PLACEHOLDER_PART}"/></w:placeholder><w:showingPlcHdr/>'
    hint = r("Click or tap here to enter text.", '<w:rStyle w:val="PlaceholderText"/>')
    widths = split(TEXT_WIDTH, 2)
    cell = s.wrap(tc(P("Cell control"), widths[1]), alias="Cell", tag="Cell/One")
    row = s.wrap(tr([tc(P("Row control C1"), widths[0]), tc(P("Row control C2"), widths[1])]), alias="Row", tag="Row/One")
    rows = tr([tc(P("R1 C1"), widths[0])]).replace("</w:tr>", cell + "</w:tr>") + row
    body = [
        P("FX-MERGE-CONTROLS", HEADING),
        p(r("Name: ") + s.wrap(r("John Doe"), kind="<w:text/>", alias="Recipient name", tag="Recipient/Name") + r(".")),
        p(r("Address: ") + s.wrap(r("1 Main St"), kind='<w:text w:multiLine="1"/>', alias="Address", tag="Recipient/Address") + r(".")),
        s.wrap(P("Rich block one.") + P("Rich block two."), kind="<w:richText/>", alias="Agreement", tag="Content:Complaint"),
        p(r("Date: ") + s.wrap(r("1/1/2026"), kind=date, alias="Date", tag="Statement/Date") + r(".")),
        p(r("Done: ") + s.wrap(r("☐", EMOJI), kind=checkbox, alias="Done", tag="Check:Done") + r(" done")),
        p(r("Barcode: ") + s.wrap(d.picture(1.5, 0.5, 1), kind="<w:picture/>", alias="Shipment barcode", tag="Barcode:7")),
        p(r("Status: ") + s.wrap(r("Paid"), kind=dropdown, alias="Status", tag="Statement/Status") + r(".")),
        s.wrap(P("Grouped paragraph."), kind="<w:group/>", alias="Group", tag="Group/One"),
        table_xml(rows, widths),
        p(r("Locked control: ") + s.wrap(r("cannot delete"), kind="<w:text/>", tag="Lock/Control", lock="sdtLocked")),
        p(r("Locked content: ") + s.wrap(r("cannot edit"), kind="<w:text/>", tag="Lock/Content", lock="contentLocked")),
        p(r("Locked both: ") + s.wrap(r("neither"), kind="<w:text/>", tag="Lock/Both", lock="sdtContentLocked")),
        p(r("Placeholder: ") + s.wrap(hint, kind="<w:text/>", tag="Recipient/Middle", extra=placeholder)),
        p(r("Bound: ") + s.wrap(r("Corporate Store"), kind="<w:text/>", tag="Store/Bound", extra=binding)),
        p(r("Custom XML: ") + f'<w:customXml w:uri="urn:store:markup" w:element="presenter">{r("Doe")}</w:customXml>' + r(".")),
        P("After the controls."),
    ]
    return "".join(body)


def fx_merge_regions(d):
    s = Sdt(6001)
    widths = [3600, 2880, 2880]
    inner_w = split(widths[1] - CELL_MARGINS, 2)

    def inner_row(n):
        return tr([tc(p(mf("Count.Code")), inner_w[0]), tc(P(f"Count {n}"), inner_w[1])])

    inner = table_xml(tr([tc(P("Code"), inner_w[0]), tc(P("Count"), inner_w[1])]) + region(s, [inner_row(n) for n in (1, 2)], "Counts", "Detail.Counts"), inner_w)

    def row(n):
        middle = P(f"Detail {n}") + (inner if n == 1 else "")
        return tr([tc(p(mf("Detail.Description")), widths[0]), tc(middle, widths[1]), tc(p(mf("Detail.Cost", '\\# "$,0.00"')), widths[2])])

    head = tr([tc(P(t), w) for t, w in zip(("Detail", "Detail", "Cost"), widths)], "<w:tblHeader/>")
    details = table_xml(head + region(s, [row(n) for n in (1, 2, 3)], "Details", "Statement.Details"), widths)
    paras = region(s, [p(r("Name: ") + mf("Owner.Name")) + p(r("Role: ") + mf("Owner.Role"))], "Holders", "Statement.Holders")
    labels_w = split(TEXT_WIDTH, 3)
    labels = table_xml(region(s, [tr([tc(p(mf("Label.Address")), labels_w[0]), tc("", labels_w[1]), tc("", labels_w[2])])], "Labels", "Labels.Address"), labels_w)
    body = [
        P("FX-MERGE-REGIONS", HEADING),
        P("Rows of the details table repeat; the first detail's detail cell holds a table whose rows repeat too."),
        details,
        P("Two paragraphs repeat as one item:"),
        paras,
        P("A labels row: one row, three cells, content only in the first."),
        labels,
        P("After the regions."),
    ]
    return "".join(body)


def fx_merge_data(d):
    s = Sdt(7001)
    custom_part(d, "urn:store:form-definition", "definition",
                '<calculatedFields><field name="Cost_Plus_Fee" type="Arithmetic">Statement.Cost_Amount + 25</field></calculatedFields>'
                '<filters template="Details"><filter field="Detail.Status" op="!=" value="Cancelled"/></filters>'
                '<outputDirector enabled="1"><fileName>{MERGEFIELD Shipment.Shipment_Number}</fileName></outputDirector>')
    data_id = custom_part(d, "urn:store:form-data", "data", "<store>Corporate Store</store>")
    binding = f'<w:dataBinding w:prefixMappings="xmlns:ns0=\'urn:store:form-data\'" w:xpath="/ns0:data[1]/ns0:store[1]" w:storeItemID="{data_id}"/>'
    body = [
        P("FX-MERGE-DATA", HEADING),
        p(r("Store: ") + s.wrap(r("Corporate Store"), kind="<w:text/>", tag="Store/Bound", extra=binding) + r(".")),
        p(r("Cost plus fee: ") + mf("Calc.Cost_Plus_Fee", '\\# "$,0.00"') + r(".")),
        P("The definition part holds calculated fields, filters and output-director settings."),
    ]
    return "".join(body)


def _merge_large(d, plain):
    s = Sdt(8001)
    names = ["Statement.Cost_Amount", "Recipient.Last_Name", "Shipment.Shipment_Number", "Calc.Old_Name"]
    out = [P("FX-MERGE-LARGE" + ("-PLAIN" if plain else ""), HEADING)]
    n = 0
    for k in range(400):
        content = r(filler(12, k) + " ")
        for i in range(5):
            name = names[n % 4]
            shown = f"«{name.split('.')[-1]}»"
            if plain:
                content += r(shown)
            elif n % 2 == 0:
                content += mf(name)
            else:
                content += s.wrap(r(shown), kind="<w:text/>", tag=name)
            content += r(" " + filler(8, k + i) + " ")
            n += 1
        out.append(p(content))
    return "".join(out)


def _inv(title, *blocks):
    return lambda d: P(title, HEADING) + "".join(b(d) if callable(b) else b for b in blocks)


def _inv_data(namespace, root, inner):
    def add(d):
        custom_part(d, namespace, root, inner)
        return ""
    return add


def _inv_picture(tag, alias):
    return lambda d: p(r(alias + ": ") + Sdt(9001).wrap(d.picture(1.5, 0.5, 1), kind="<w:picture/>", alias=alias, tag=tag))


def _inv_regions(d):
    return fx_merge_regions(d)


# One template per merge feature (field switches, regions, data parts, ...), into editor/merge-set/.
MERGE_SET = {
    "inv-1.9-field-picker": _inv("1.9 Field picker", p(r("Name: ") + mf("Recipient.Last_Name") + r(", ") + mf("Recipient.First_Name"))),
    "inv-2.1-switches": _inv("2.1-2.4, 2.6 Fields with switches", p(mf("Statement.Cost_Amount", '\\# "$,0.00;($,0.00)"') + r(" ") + mf("Statement.Date", '\\@ "MMMM d, yyyy"') + r(" ") + mf("Statement.Time", '\\@ "h:mm AM/PM"') + r(" ") + mf("Shipment.Shipment_Number", "\\* Upper") + r(" ") + mf("Recipient.Middle", '\\b " " \\f ","'))),
    "inv-2.5-default-format": _inv("2.5 Default number format", p(mf("Statement.Cost_Amount", '\\# ",0.00;-,0.00"') + r(" ") + mf("Statement.Weight", '\\# "0;-0"'))),
    "inv-2.7-word-fields": _inv("2.7 Word fields", p(fld([' IF ', mf("Statement.Status"), ' = "Paid" "Paid in full" "Balance due" '], "Balance due") + r(" ") + fld(' DATE \\@ "M/d/yyyy" ', "1/1/2026") + r(" ") + fld(" PAGE ", "1") + r(" of ") + fld(" NUMPAGES ", "1"))),
    "inv-2.8-format-by-type": _inv("2.8 Format by type", p(mf("Statement.Date", '\\@ "M/d/yyyy"') + r(" ") + mf("Statement.Cost_Amount", '\\# "$,0.00"'))),
    "inv-3-system-fields": _inv("3 System fields", p(mf("System.City_Name") + r(" ") + mf("System.User_Name") + r(" ") + mf("System.Date_Time", '\\@ "M/d/yyyy h:mm:ss AM/PM"'))),
    "inv-4.1-subtemplates": _inv_regions,
    "inv-4.2-rename": _inv("4.2, 6.5 Rename cascade", *[p(mf("Calc.Old_Name", sw)) for sw in ('\\# "$,0.00"', "\\* Upper", "")], _inv_data("urn:store:form-definition", "definition", '<calculatedFields><field name="Old_Name" type="Expression">IIF(Statement.Cost_Amount &gt; 0, 1, 0)</field></calculatedFields>')),
    "inv-4.6-filters": _inv("4.3-4.11 Filters and sorts", lambda d: fx_merge_regions(d), _inv_data("urn:store:form-definition", "definition", '<filters template="Details" join="AND"><filter field="Detail.Status" op="!=" value="Cancelled"/></filters><sort template="Details"><key field="Detail.Sequence" dir="ASC"/></sort>')),
    "inv-4b-labels": _inv("4b Labels grid", lambda d: table_xml(region(Sdt(9101), [tr([tc(p(mf("Label.Address")), 3120), tc("", 3120), tc("", 3120)])], "Labels", "Labels.Address"), [3120, 3120, 3120])),
    "inv-5-aggregates": _inv("5 COUNT and SUM", p(r("Details: ") + fld(" MERGECOUNT Details ", "«COUNT»") + r(", total ") + fld(' MERGESUM Details=Detail.Cost \\# "$,0.00" ', "«SUM»"))),
    "inv-6-calculated": _inv("6 Calculated fields", p(mf("Calc.Cost_Plus_Fee", '\\# "$,0.00"') + r(" ") + mf("Calc.Due_Date_Plus_30", '\\@ "M/d/yyyy"')), _inv_data("urn:store:form-definition", "definition", '<calculatedFields><field name="Cost_Plus_Fee" type="Arithmetic">Statement.Cost_Amount + 25</field><field name="Due_Date_Plus_30" type="Date">Statement.Due_Date + 30 Days</field></calculatedFields>')),
    "inv-7-agreement": _inv("7 Agreement language", lambda d: Sdt(9201).wrap(P("Agreement language goes here."), kind="<w:richText/>", alias="Agreement", tag="Content:Store.Statement.Product_Agreement_Language")),
    "inv-8-photo-logo": _inv("8, 3.4 Photo and logo", _inv_picture("Photo:Name_Photo.Photo", "Recipient photo"), _inv_picture("Logo:Store", "Store logo")),
    "inv-9-barcode": _inv("9 Barcodes", _inv_picture("Barcode:1", "Shipment barcode")),
    "inv-10-output-director": _inv("10 Output director", P("Output-director settings are in the definition part."), _inv_data("urn:store:form-definition", "definition", '<outputDirector enabled="1"><title>{MERGEFIELD Shipment.Shipment_Number}</title><fileName>Shipment {MERGEFIELD Shipment.Shipment_Number}</fileName><email>{MERGEFIELD Recipient.Email}</email><entity type="Store.Statement" field="Statement.Id"/></outputDirector>')),
    "inv-11.2-sections": lambda d: (P("11.2 Headers, footers, sections", HEADING) + p(mf("Recipient.Last_Name")), sect(d.story("header", "default", p(mf("Store.Name"))) + d.story("footer", "default", p(r("Page ") + fld(" PAGE ", "1"))))),
    "inv-12.1-validation": _inv("12.1, 12.2 Validation", p(mf("Statement.Cost_Amount") + r(" ") + mf("Unknown.Column") + r(" ") + mf(""))),
    "inv-12.5-preview": _inv("12.5 Preview", p(r("Dear ") + mf("Recipient.First_Name") + r(" ") + mf("Recipient.Last_Name") + r(",")), P(filler(40, 3))),
}


def fx_placeholder(d):
    """A one-page form with a signature picture placeholder: a picture control showing its placeholder image, tagged
    with the box the filled image fits into. `<w:picture />` is written with the space, as tests patch that text."""
    s = Sdt(9500)
    sig = s.wrap(d.picture(2719705 / EMU, 503555 / EMU, 3), kind="<w:picture />", alias="Recipient Sig",
                 tag="Placeholder:Signature.Recipient Sig;fit=contain;box=214.15x39.65", extra="<w:showingPlcHdr/>")
    return (
        P("Delivery Receipt", HEADING)
        + P(filler(60, 2))
        + P(filler(45, 5))
        + p(r("Received by: ") + sig + r("\t"))
        + P("Date: 1/1/2026")
    )


def fx_row_clip(d):
    """A line drawn just below an exact-height row whose paragraph it clips (hit testing must pick the drawn line),
    then a paragraph three tables deep."""
    clipped = tr([tc(P(filler(40, 4)), TEXT_WIDTH)], '<w:trHeight w:val="288" w:hRule="exact"/>')
    below = tr([tc(P("Main Office, 100 Market Street, Springfield 00000."), TEXT_WIDTH)], '<w:trHeight w:val="480" w:hRule="atLeast"/>')
    inner = grid([[P(filler(16, 6))]], [TEXT_WIDTH - 2 * CELL_MARGINS])
    middle = grid([[inner]], [TEXT_WIDTH - CELL_MARGINS])
    return tbl([clipped, below], [TEXT_WIDTH]) + P("") + grid([[middle]], [TEXT_WIDTH])


def fx_grid_wide(d):
    """An address table whose cells (5400) are wider than its grid's first column (1692) and narrower than the grid
    (7200): the first row is one cell plus a gridAfter, the others span both columns. Autofit, narrow page margins."""
    cell = '<w:tcBorders><w:top w:val="nil"/><w:left w:val="nil"/><w:bottom w:val="nil"/><w:right w:val="nil"/></w:tcBorders>' \
           '<w:tcMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tcMar>'
    span = '<w:gridSpan w:val="2"/>' + cell
    rows = [
        tr([tc(P("Main Office"), 5400, cell)], '<w:gridAfter w:val="1"/><w:wAfter w:w="5508" w:type="dxa"/>'),
        tr([tc(P("100 Market Street"), 5400, span)], '<w:trHeight w:val="195" w:hRule="atLeast"/>'),
        tr([tc(P(""), 5400, span)], '<w:trHeight w:val="225" w:hRule="atLeast"/>'),
        tr([tc(P(""), 5400, span)], '<w:trHeight w:val="120" w:hRule="atLeast"/>'),
    ]
    table = (
        '<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblInd w:w="0" w:type="dxa"/>'
        f'<w:tblBorders>{BORDERS}</w:tblBorders><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/></w:tblCellMar></w:tblPr>'
        f'<w:tblGrid><w:gridCol w:w="1692"/><w:gridCol w:w="5508"/></w:tblGrid>{"".join(rows)}</w:tbl>'
    )
    margins = ('<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="144" w:right="527" w:bottom="144" w:left="612" '
               'w:header="720" w:footer="720" w:gutter="0"/><w:cols w:space="720"/></w:sectPr>')
    return table + P(""), margins


FIXTURES = {
    "FX-ROW-CLIP": fx_row_clip,
    "FX-GRID-WIDE": fx_grid_wide,
    "FX-PLACEHOLDER": fx_placeholder,
    "FX-MERGE-FIELDS": fx_merge_fields,
    "FX-MERGE-CONTROLS": fx_merge_controls,
    "FX-MERGE-REGIONS": fx_merge_regions,
    "FX-MERGE-DATA": fx_merge_data,
    "FX-MERGE-LARGE": lambda d: _merge_large(d, False),
    "FX-MERGE-LARGE-PLAIN": lambda d: _merge_large(d, True),
    "FX-CONTROLS": fx_controls,
    "FX-TBL-NOBORDER": fx_tbl_noborder,
    "FX-HDR-TABLE": fx_hdr_table,
    "FX-OBJ-LOCK": fx_obj_lock,
    **{f"{name}-L{n}": (lambda d, b=build, n=n: _nest_copy(d, b, n)) for name, build in (("FX-TBL-BASIC", fx_tbl_basic), ("FX-TBL-MERGED", fx_tbl_merged), ("FX-NEST-2", fx_nest_2), ("FX-TBL-PAGES", fx_tbl_pages)) for n in (2, 3)},
    "FX-TBL-BASIC-HDR": fx_tbl_basic_hdr,
    "FX-TBL-BASIC-BOX": fx_tbl_basic_box,
    "FX-NEST-2-HDR": fx_nest_2_hdr,
    "FX-NEST-2-BOX": fx_nest_2_box,
    "FX-TEXT": fx_text,
    "FX-PAGES": fx_pages,
    "FX-UNICODE": fx_unicode,
    "FX-LISTS": fx_lists,
    "FX-TBL-BASIC": fx_tbl_basic,
    "FX-TBL-MERGED": fx_tbl_merged,
    "FX-TBL-PAGES": fx_tbl_pages,
    "FX-NEST-2": fx_nest_2,
    "FX-NEST-3": fx_nest_3,
    "FX-NEST-DEEP": fx_nest_deep(6),
    "FX-NEST-DEEP7": fx_nest_deep(7),
    "FX-NEST-MIXED": fx_nest_mixed,
    "FX-OBJ": fx_obj,
    "FX-OBJ-CELL": fx_obj_cell,
    "FX-STORIES": fx_stories,
    "FX-TBL-LARGE": lambda d: large(d, False),
    "FX-TBL-LARGE-NESTED": lambda d: large(d, True),
    "FX-EMPTY": fx_empty,
    "FX-EMPTY-TABLE": fx_empty_table,
    "FX-END-TABLE": fx_end_table,
    "FX-EXACT-ROW": fx_exact_row,
    "FX-REPEAT-REGION": fx_repeat_region,
}


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent / "editor"
    out.mkdir(parents=True, exist_ok=True)
    sets = [(out, FIXTURES), (out / "merge-set", MERGE_SET)]
    for folder, builds in sets:
        folder.mkdir(parents=True, exist_ok=True)
        for name, build in builds.items():
            d = Doc()
            result = build(d)
            body, sect_pr = result if isinstance(result, tuple) else (result, None)
            d.write(folder / f"{name}.docx", body, sect_pr)
    print(f"wrote {len(FIXTURES)} fixtures to {out} and {len(MERGE_SET)} to {out / 'merge-set'}")


if __name__ == "__main__":
    main()
