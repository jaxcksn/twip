"""Writes the layout feature fixtures into tests/fixtures/layout/ (next to this file).

One document per layout feature of the roadmap's layout phase, small and aimed so the feature moves something the
layout snapshot records; a feature that lays out differently in compat <= 14 and compat 15 comes as a `-c12` / `-c15`
(or `-c14` / `-c15`) pair with the same body. `compat-2003`, `compat-2007`, `compat-2010` and `compat-2013` are one
body in each compatibility mode. README.md lists every fixture and what its snapshot shows. hyph-en-US.dic is a small
pattern set the layout gate supplies as the host's en-US hyphenation patterns.

Standard library only, no randomness: the same bytes on every run. Every package is plain OOXML that Word opens
without a repair prompt. Fonts: Calibri, Times New Roman and MS Mincho (CJK text, 漢 only), which the gate resolves
to its bundled stand-in fonts. The text helpers and STYLES come from ../make-fixtures.py.
Run: python -I crates/twip/tests/fixtures/layout/make-layout-fixtures.py [out dir]
"""

import importlib.util
import sys
import zipfile
from pathlib import Path
from xml.sax.saxutils import escape

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("make_fixtures", HERE.parent / "make-fixtures.py")
mf = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(mf)

r, p, P, filler, font, png = mf.r, mf.p, mf.P, mf.filler, mf.font, mf.png
B, BR, TAB, PAGE_BREAK, HEADING = mf.B, mf.BR, mf.TAB, mf.PAGE_BREAK, mf.HEADING
EMU, XML_DECL, REL, CT, STAMP, STYLES = mf.EMU, mf.XML_DECL, mf.REL, mf.CT, mf.STAMP, mf.STYLES

NS = dict(mf.NS)
NS.update({
    "wpg": "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup",
    "wpc": "http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas",
    "c": "http://schemas.openxmlformats.org/drawingml/2006/chart",
    "v": "urn:schemas-microsoft-com:vml",
    "o": "urn:schemas-microsoft-com:office:office",
    "w10": "urn:schemas-microsoft-com:office:word",
})
NAMESPACES = " ".join(f'xmlns:{k}="{v}"' for k, v in NS.items()) + ' mc:Ignorable="w14 w15 wp14"'
TEXT_WIDTH = 9360
CJK = "漢"  # the one CJK character the stand-in CJK font has
MINCHO = '<w:rFonts w:ascii="MS Mincho" w:hAnsi="MS Mincho" w:eastAsia="MS Mincho" w:hint="eastAsia"/>'
TIGHT_SPACING = '<w:spacing w:after="0" w:line="240" w:lineRule="auto"/>'


def run(content, rpr=""):
    return f"<w:r>{f'<w:rPr>{rpr}</w:rPr>' if rpr else ''}{content}</w:r>"


def bdr(side, val="single", sz=4, space=0, extra=""):
    return f'<w:{side} w:val="{val}" w:sz="{sz}" w:space="{space}" w:color="000000"{extra}/>'


def pbdr(*sides, **kw):
    return "<w:pBdr>" + "".join(bdr(s, **kw) for s in sides) + "</w:pBdr>"


# ---- tables -------------------------------------------------------------------------------------------------------

TBL_BORDERS = "<w:tblBorders>" + "".join(bdr(s) for s in ("top", "left", "bottom", "right", "insideH", "insideV")) + "</w:tblBorders>"


def cell(content, width, tcpr_after_w="", tail=""):
    """A cell; `tcpr_after_w` goes after tcW (gridSpan to textDirection), `tail` after that (vAlign, hideMark)."""
    if not content or content.endswith("</w:tbl>"):
        content += "<w:p/>"
    if not content.startswith("<"):
        content = P(content)
    return f'<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/>{tcpr_after_w}{tail}</w:tcPr>{content}</w:tc>'


def row(cells, trpr="", ex=""):
    ex = f"<w:tblPrEx>{ex}</w:tblPrEx>" if ex else ""
    return f"<w:tr>{ex}{f'<w:trPr>{trpr}</w:trPr>' if trpr else ''}{''.join(cells)}</w:tr>"


def table(rows, widths, style="", ind=None, jc="", spacing=None, borders=TBL_BORDERS, fixed=True, mar="", pos="", look=""):
    """A table, its tblPr in schema order."""
    pr = f'<w:tblStyle w:val="{style}"/>' if style else ""
    pr += pos
    pr += f'<w:tblW w:w="{sum(widths)}" w:type="dxa"/>'
    pr += f'<w:jc w:val="{jc}"/>' if jc else ""
    pr += f'<w:tblCellSpacing w:w="{spacing}" w:type="dxa"/>' if spacing is not None else ""
    pr += f'<w:tblInd w:w="{ind}" w:type="dxa"/>' if ind is not None else ""
    pr += borders
    pr += '<w:tblLayout w:type="fixed"/>' if fixed else ""
    pr += f"<w:tblCellMar>{mar}</w:tblCellMar>" if mar else ""
    pr += look
    grid = "".join(f'<w:gridCol w:w="{w}"/>' for w in widths)
    return f"<w:tbl><w:tblPr>{pr}</w:tblPr><w:tblGrid>{grid}</w:tblGrid>{''.join(rows)}</w:tbl>"


def simple(data, widths, **kw):
    return table([row([cell(c, w) for c, w in zip(rw, widths)]) for rw in data], widths, **kw)


# ---- floats -------------------------------------------------------------------------------------------------------

SQUARE = '<wp:wrapSquare wrapText="bothSides"/>'
NONE = "<wp:wrapNone/>"
TOP_BOTTOM = "<wp:wrapTopAndBottom/>"


def poly(kind, points, side="bothSides"):
    pts = "".join(f'<wp:{"start" if i == 0 else "lineTo"} x="{x}" y="{y}"/>' for i, (x, y) in enumerate(points))
    return f'<wp:wrap{kind} wrapText="{side}"><wp:wrapPolygon edited="1">{pts}</wp:wrapPolygon></wp:wrap{kind}>'


def pos(axis, rel, at):
    """wp:positionH/V: `at` an offset in inches, a raw EMU int (int), an alignment name, or ('pct', thousandths)."""
    if isinstance(at, str):
        value = f"<wp:align>{at}</wp:align>"
    elif isinstance(at, tuple):
        value = f"<wp14:pct{'PosHOffset' if axis == 'H' else 'PosVOffset'}>{at[1]}</wp14:pct{'PosHOffset' if axis == 'H' else 'PosVOffset'}>"
    elif isinstance(at, int):
        value = f"<wp:posOffset>{at}</wp:posOffset>"
    else:
        value = f"<wp:posOffset>{round(at * EMU)}</wp:posOffset>"
    return f'<wp:position{axis} relativeFrom="{rel}">{value}</wp:position{axis}>'


class Pkg:
    """One package: document, styles, settings, and the parts a fixture adds."""

    def __init__(self, compat=15):
        self.compat = compat  # compatibilityMode value; None writes no compatSetting
        self.ids = 0
        self.rels = []  # (id, type, target)
        self.parts = []  # (name, bytes or str, content type)
        self.defaults = []  # (extension, content type)
        self.footnotes, self.endnotes = [], []
        self.styles = STYLES
        self.numbering = None
        self.settings_head = ""  # settings before defaultTabStop (mirrorMargins, gutterAtTop)
        self.settings_mid = ""  # after defaultTabStop (hyphenation, evenAndOddHeaders, book fold)
        self.footnote_pr = self.endnote_pr = ""  # settings footnotePr / endnotePr properties
        self.compat_opts = ""  # w:compat children, in schema order
        self.compat_settings = ""  # w:compatSetting elements after compatibilityMode
        self.settings_tail = ""  # after w:compat (decimalSymbol)
        self.web = None  # webSettings divs
        self.image_rid = None

    def uid(self):
        self.ids += 1
        return self.ids

    def rel(self, kind, target):
        rid = f"rId{len(self.rels) + 10}"
        self.rels.append((rid, REL + kind, target))
        return rid

    def image(self):
        if self.image_rid is None:
            self.image_rid = self.rel("image", "media/image1.png")
            self.parts.append(("word/media/image1.png", png(mf.COLOURS[0]), None))
            self.defaults.append(("png", "image/png"))
        return self.image_rid

    # -- drawings --
    def anchor(self, graphic, w, h, hpos, vpos, wrap=SQUARE, dist=(0, 0, 114300, 114300), behind=False, overlap=True, in_cell=True,
               tail="", name="Shape", req=None):
        """A floating drawing run. Sizes in inches (or raw EMU ints); `tail` follows the graphic (wp14 relative sizes);
        `req` wraps it in mc:AlternateContent requiring that namespace."""
        i = self.uid()
        cx = w if isinstance(w, int) else round(w * EMU)
        cy = h if isinstance(h, int) else round(h * EMU)
        t, b, left, right = dist
        drawing = (
            f'<w:drawing><wp:anchor distT="{t}" distB="{b}" distL="{left}" distR="{right}" simplePos="0" relativeHeight="{251658240 + i * 1024}" '
            f'behindDoc="{int(behind)}" locked="0" layoutInCell="{int(in_cell)}" allowOverlap="{int(overlap)}"><wp:simplePos x="0" y="0"/>'
            f'{pos("H", *hpos)}{pos("V", *vpos)}<wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/>{wrap}'
            f'<wp:docPr id="{i}" name="{name} {i}"/><wp:cNvGraphicFramePr/>{graphic(i, cx, cy)}{tail}</wp:anchor></w:drawing>'
        )
        if req:
            drawing = f'<mc:AlternateContent><mc:Choice Requires="{req}">{drawing}</mc:Choice></mc:AlternateContent>'
        return f"<w:r>{drawing}</w:r>"

    def pic_graphic(self, rot=0):
        rid = self.image()
        rot = f' rot="{rot}"' if rot else ""

        def g(i, cx, cy):
            return (
                '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic>'
                f'<pic:nvPicPr><pic:cNvPr id="{i}" name="image1.png"/><pic:cNvPicPr/></pic:nvPicPr>'
                f'<pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
                f'<pic:spPr><a:xfrm{rot}><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>'
                '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic>'
            )
        return g

    def picture(self, w, h, hpos, vpos, rot=0, **kw):
        return self.anchor(self.pic_graphic(rot), w, h, hpos, vpos, name="Picture", **kw)

    def inline_picture(self, w, h):
        i, rid = self.uid(), self.image()
        cx, cy = round(w * EMU), round(h * EMU)
        return (
            f'<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/>'
            f'<wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="{i}" name="Picture {i}"/><wp:cNvGraphicFramePr/>'
            f"{self.pic_graphic()(i, cx, cy)}</wp:inline></w:drawing></w:r>"
        )

    @staticmethod
    def wsp(i, cx, cy, text=None, fill="2E75B6", line=True, body="", rot=0, insets=None):
        """A wps:wsp shape; `text` (block XML) makes it a text box."""
        fill_xml = f'<a:solidFill><a:srgbClr val="{fill}"/></a:solidFill>' if fill else "<a:noFill/>"
        ln = '<a:ln w="6350"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>' if line else "<a:ln><a:noFill/></a:ln>"
        rot = f' rot="{rot}"' if rot else ""
        nv = '<wps:cNvSpPr txBox="1"/>' if text is not None else "<wps:cNvSpPr/>"
        txbx = f"<wps:txbx><w:txbxContent>{text}</w:txbxContent></wps:txbx>" if text is not None else ""
        ins = insets or 'lIns="91440" tIns="45720" rIns="91440" bIns="45720"'
        return (
            f'<wps:wsp>{nv}<wps:spPr><a:xfrm{rot}><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>'
            f'<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{fill_xml}{ln}</wps:spPr>{txbx}'
            f'<wps:bodyPr rot="0" vert="horz" wrap="square" {ins} anchor="t" anchorCtr="0"{body}><a:noAutofit/></wps:bodyPr></wps:wsp>'
        )

    def shape(self, w, h, hpos, vpos, fill="ED7D31", rot=0, **kw):
        def g(i, cx, cy):
            return ('<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">'
                    f"{self.wsp(i, cx, cy, fill=fill, rot=rot)}</a:graphicData></a:graphic>")
        return self.anchor(g, w, h, hpos, vpos, req="wps", **kw)

    def textbox(self, content, w, h, hpos, vpos, fill="FFFFFF", line=True, body_attrs="", autofit="<a:noAutofit/>", insets=None, **kw):
        def g(i, cx, cy):
            shape = self.wsp(i, cx, cy, text=content, fill=fill, line=line, body=body_attrs, insets=insets).replace("<a:noAutofit/>", autofit)
            return f'<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">{shape}</a:graphicData></a:graphic>'
        return self.anchor(g, w, h, hpos, vpos, name="Text Box", req="wps", **kw)

    def group(self, w, h, hpos, vpos, **kw):
        def g(i, cx, cy):
            half = cx // 2
            kids = "".join(
                f'<wps:wsp><wps:cNvPr id="{i * 10 + k}" name="Part {k}"/><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="{x}" y="{y}"/>'
                f'<a:ext cx="{half}" cy="{cy // 2}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>'
                f'<a:solidFill><a:srgbClr val="{c}"/></a:solidFill></wps:spPr><wps:bodyPr/></wps:wsp>'
                for k, (x, y, c) in enumerate([(0, 0, "70AD47"), (half, cy // 2, "7F3FBF")], 1)
            )
            return (
                '<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:wgp>'
                f'<wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/><a:chOff x="0" y="0"/>'
                f'<a:chExt cx="{cx}" cy="{cy}"/></a:xfrm></wpg:grpSpPr>{kids}</wpg:wgp></a:graphicData></a:graphic>'
            )
        return self.anchor(g, w, h, hpos, vpos, name="Group", req="wpg", **kw)

    def canvas(self, w, h, hpos, vpos, **kw):
        def g(i, cx, cy):
            return (
                '<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas"><wpc:wpc>'
                '<wpc:bg><a:noFill/></wpc:bg><wpc:whole><a:ln><a:noFill/></a:ln></wpc:whole>'
                f'<wps:wsp><wps:cNvPr id="{i * 10}" name="Canvas shape"/><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="{cx // 4}" y="{cy // 4}"/>'
                f'<a:ext cx="{cx // 2}" cy="{cy // 2}"/></a:xfrm><a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom>'
                '<a:solidFill><a:srgbClr val="2E75B6"/></a:solidFill></wps:spPr><wps:bodyPr/></wps:wsp></wpc:wpc></a:graphicData></a:graphic>'
            )
        return self.anchor(g, w, h, hpos, vpos, name="Canvas", req="wpc", **kw)

    def chart(self, w, h, hpos, vpos, **kw):
        n = sum(1 for part in self.parts if part[0].startswith("word/charts/")) + 1
        rid = self.rel("chart", f"charts/chart{n}.xml")
        pts = "".join(f'<c:pt idx="{k}"><c:v>{v}</c:v></c:pt>' for k, v in enumerate((3, 5, 2)))
        self.parts.append((
            f"word/charts/chart{n}.xml",
            f'{XML_DECL}<c:chartSpace xmlns:c="{NS["c"]}" xmlns:a="{NS["a"]}" xmlns:r="{NS["r"]}"><c:chart><c:autoTitleDeleted val="1"/>'
            '<c:plotArea><c:layout/><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/>'
            f'<c:ser><c:idx val="0"/><c:order val="0"/><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="3"/>{pts}'
            '</c:numLit></c:val></c:ser><c:axId val="1001"/><c:axId val="1002"/></c:barChart>'
            '<c:catAx><c:axId val="1001"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/>'
            '<c:crossAx val="1002"/></c:catAx><c:valAx><c:axId val="1002"/><c:scaling><c:orientation val="minMax"/></c:scaling>'
            '<c:delete val="0"/><c:axPos val="l"/><c:crossAx val="1001"/></c:valAx></c:plotArea><c:plotVisOnly val="1"/></c:chart></c:chartSpace>',
            "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
        ))

        def g(i, cx, cy):
            return (f'<a:graphic><a:graphicData uri="{NS["c"]}"><c:chart r:id="{rid}"/></a:graphicData></a:graphic>')
        return self.anchor(g, w, h, hpos, vpos, name="Chart", **kw)

    def vml(self, style, inner="", kind="rect", attrs='fillcolor="#7f3fbf"'):
        i = self.uid()
        return f'<w:r><w:pict><v:{kind} id="Shape {i}" o:spid="_x0000_s{1024 + i}" style="{style}" {attrs}>{inner}</v:{kind}></w:pict></w:r>'

    # -- notes and stories --
    def footnote(self, text):
        n = len(self.footnotes) + 1
        ref = '<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteRef/></w:r>'
        self.footnotes.append(f'<w:footnote w:id="{n}">{p(ref + r(" " + text), mf.FOOTNOTE_TEXT)}</w:footnote>')
        return f'<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="{n}"/></w:r>'

    def endnote(self, text):
        n = len(self.endnotes) + 1
        ref = '<w:r><w:rPr><w:rStyle w:val="EndnoteReference"/></w:rPr><w:endnoteRef/></w:r>'
        self.endnotes.append(f'<w:endnote w:id="{n}">{p(ref + r(" " + text), mf.ENDNOTE_TEXT)}</w:endnote>')
        return f'<w:r><w:rPr><w:rStyle w:val="EndnoteReference"/></w:rPr><w:endnoteReference w:id="{n}"/></w:r>'

    def story(self, kind, content, which="default"):
        tag = "hdr" if kind == "header" else "ftr"
        name = f"{kind}{sum(1 for part in self.parts if part[0].startswith('word/' + kind)) + 1}.xml"
        self.parts.append((f"word/{name}", f"{XML_DECL}<w:{tag} {NAMESPACES}>{content}</w:{tag}>", f"{CT}{kind}+xml"))
        return f'<w:{kind}Reference w:type="{which}" r:id="{self.rel(kind, name)}"/>'

    # -- the package --
    def write(self, path, body, sect_pr):
        if body.endswith("</w:tbl>"):
            body += "<w:p/>"
        document = f"{XML_DECL}<w:document {NAMESPACES}><w:body>{body}{sect_pr}</w:body></w:document>"
        files = [("word/document.xml", document, f"{CT}document.main+xml"), ("word/styles.xml", self.styles, f"{CT}styles+xml")]
        rels = [("rIdStyles", REL + "styles", "styles.xml"), ("rIdSettings", REL + "settings", "settings.xml")]
        if self.numbering:
            files.append(("word/numbering.xml", self.numbering, f"{CT}numbering+xml"))
            rels.append(("rIdNumbering", REL + "numbering", "numbering.xml"))
        if self.web is not None:
            web = f'{XML_DECL}<w:webSettings xmlns:w="{NS["w"]}">{self.web}</w:webSettings>'
            files.append(("word/webSettings.xml", web, f"{CT}webSettings+xml"))
            rels.append(("rIdWebSettings", REL + "webSettings", "webSettings.xml"))
        notes_pr = {"footnote": "", "endnote": ""}
        for kind, notes in (("footnote", self.footnotes), ("endnote", self.endnotes)):
            if notes:
                separators = (
                    f'<w:{kind} w:type="separator" w:id="-1"><w:p><w:pPr>{TIGHT_SPACING}</w:pPr><w:r><w:separator/></w:r></w:p></w:{kind}>'
                    f'<w:{kind} w:type="continuationSeparator" w:id="0"><w:p><w:pPr>{TIGHT_SPACING}</w:pPr><w:r><w:continuationSeparator/></w:r></w:p></w:{kind}>'
                )
                files.append((f"word/{kind}s.xml", f"{XML_DECL}<w:{kind}s {NAMESPACES}>{separators}{''.join(notes)}</w:{kind}s>", f"{CT}{kind}s+xml"))
                rels.append((f"rId{kind.capitalize()}s", REL + f"{kind}s", f"{kind}s.xml"))
                notes_pr[kind] = f'<w:{kind} w:id="-1"/><w:{kind} w:id="0"/>'
        fn = self.footnote_pr + notes_pr["footnote"]
        en = self.endnote_pr + notes_pr["endnote"]
        mode = (f'<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{self.compat}"/>'
                if self.compat is not None else "")
        settings = (
            f'{XML_DECL}<w:settings xmlns:w="{NS["w"]}">{self.settings_head}<w:defaultTabStop w:val="720"/>{self.settings_mid}'
            f'{f"<w:footnotePr>{fn}</w:footnotePr>" if fn else ""}{f"<w:endnotePr>{en}</w:endnotePr>" if en else ""}'
            f"<w:compat>{self.compat_opts}{mode}{self.compat_settings}</w:compat>{self.settings_tail}</w:settings>"
        )
        files.append(("word/settings.xml", settings, f"{CT}settings+xml"))
        files += self.parts
        rels += self.rels
        overrides = "".join(f'<Override PartName="/{name}" ContentType="{ctype}"/>' for name, _, ctype in files if ctype)
        defaults = "".join(f'<Default Extension="{e}" ContentType="{c}"/>' for e, c in self.defaults)
        content_types = (
            f'{XML_DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            f'<Default Extension="xml" ContentType="application/xml"/>{defaults}{overrides}</Types>'
        )
        relationships = '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        package_rels = f'{XML_DECL}{relationships}<Relationship Id="rId1" Type="{REL}officeDocument" Target="word/document.xml"/></Relationships>'
        document_rels = (f"{XML_DECL}{relationships}" + "".join(f'<Relationship Id="{i}" Type="{t}" Target="{target}"/>' for i, t, target in rels)
                         + "</Relationships>")
        entries = [("[Content_Types].xml", content_types), ("_rels/.rels", package_rels), ("word/_rels/document.xml.rels", document_rels)]
        entries += [(name, data) for name, data, _ in files]
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
            for name, data in entries:
                info = zipfile.ZipInfo(name, STAMP)
                info.create_system = 0  # the same bytes whatever OS writes the archive
                info.external_attr = 0
                z.writestr(info, data.encode("utf-8") if isinstance(data, str) else data, zipfile.ZIP_DEFLATED)


def sect(refs="", fn="", en="", kind="", w=12240, h=15840, mar=(1440, 1440, 1440, 1440), gutter=0, borders="", pgnum="", cols="",
         no_endnote=False, direction="", landscape=False):
    """A sectPr in schema order; mar is (top, right, bottom, left)."""
    top, right, bottom, left = mar
    orient = ' w:orient="landscape"' if landscape else ""
    out = f"<w:sectPr>{refs}"
    out += f"<w:footnotePr>{fn}</w:footnotePr>" if fn else ""
    out += f"<w:endnotePr>{en}</w:endnotePr>" if en else ""
    out += f'<w:type w:val="{kind}"/>' if kind else ""
    out += f'<w:pgSz w:w="{w}" w:h="{h}"{orient}/>'
    out += f'<w:pgMar w:top="{top}" w:right="{right}" w:bottom="{bottom}" w:left="{left}" w:header="720" w:footer="720" w:gutter="{gutter}"/>'
    out += borders + pgnum + (cols or '<w:cols w:space="720"/>')
    out += "<w:noEndnote/>" if no_endnote else ""
    out += f'<w:textDirection w:val="{direction}"/>' if direction else ""
    return out + "</w:sectPr>"


def section_break(sect_pr, text=""):
    """A paragraph ending a section."""
    return f"<w:p><w:pPr>{sect_pr}</w:pPr>{r(text) if text else ''}</w:p>"


def heading(text):
    return P(text, HEADING)


# ---- 1.2 paragraph and table geometry -----------------------------------------------------------------------------


def para_borders(d):
    box = dict(sz=8, space=4)
    sides = ("top", "left", "bottom", "right")
    body = [
        heading("Paragraph borders"),
        P("Plain paragraph before the bordered ones."),
        P("One paragraph with a top and a bottom border, 1pt wide with 4pt space: its first line moves down by 5pt and the "
          "next paragraph by 5pt more.", pbdr("top", "bottom", **box)),
        P("Plain paragraph between."),
        P("First of three paragraphs with the same borders: one box, so the top border is above this one only. " + filler(20, 1), pbdr(*sides, **box)),
        P("Second of the three: no border above or below it inside the box.", pbdr(*sides, **box)),
        P("Third of the three: the bottom border is below this one only.", pbdr(*sides, **box)),
        P("Same borders but a left indent: a box edge differs, so a new box starts here.", '<w:pBdr>' + "".join(bdr(s, **box) for s in sides) + "</w:pBdr><w:ind w:left=\"720\"/>"),
        P("Between borders: the first of two paragraphs with a between border.", pbdr("top", "left", "bottom", "right", "between", sz=4, space=6)),
        P("The second: the between line and its space come above this one.", pbdr("top", "left", "bottom", "right", "between", sz=4, space=6)),
        P("A double border is three times its width: 0.75pt lines with a gap, 2pt space.", pbdr("top", "bottom", val="double", sz=6, space=2)),
        P("A shadowed border adds one more width below its bottom line.", '<w:pBdr>' + bdr("top", sz=12, space=1) + bdr("bottom", sz=12, space=1, extra=' w:shadow="1"') + "</w:pBdr>"),
        P(filler(60, 2)),
    ]
    body += [P(filler(70, 10 + k)) for k in range(4)]
    body.append(p(r("A box split by a page break: this paragraph opens it and its page break leaves it open. " + filler(20, 3)) + PAGE_BREAK
                  + r("Continued after the break: no top border on the new page."), pbdr(*sides, **box)))
    body.append(P("Last paragraph of the box: its bottom border closes it.", pbdr(*sides, **box)))
    body.append(P("Plain paragraph after the box."))
    return "".join(body), sect()


def border_space(d):
    body = [heading("Border w:space")]
    for space in (0, 1, 4, 24, 31, 32, 40, 60, 255):
        body.append(P(f"Top and bottom borders with w:space {space} (kept as {space & 31}pt).", pbdr("top", "bottom", sz=4, space=space)))
        body.append(P("Plain paragraph between."))
    body.append(P("Left and right borders with w:space 12 and 40 (8pt): the text keeps its indents, the lines drawn outside them.",
                  "<w:pBdr>" + bdr("left", space=12) + bdr("right", space=40) + "</w:pBdr>"))
    page_borders = ('<w:pgBorders w:offsetFrom="text">' + bdr("top", sz=12, space=4) + bdr("left", sz=12, space=31) + bdr("bottom", sz=12, space=40)
                    + bdr("right", sz=12, space=255) + "</w:pgBorders>")
    body.append(section_break(sect(borders=page_borders), "Page borders measured from the text: w:space 4, 31, 40 (clamped to 31) and 255 (31)."))
    page2 = '<w:pgBorders w:offsetFrom="page">' + "".join(bdr(s, sz=12, space=24) for s in ("top", "left", "bottom", "right")) + "</w:pgBorders>"
    body.append(P("Page borders measured from the page edge, 24pt in. " + filler(40, 4)))
    return "".join(body), sect(kind="nextPage", borders=page2)


def table_geometry(d):
    thick = "<w:tblBorders>" + "".join(bdr(s, sz=24) for s in ("top", "left", "bottom", "right", "insideH", "insideV")) + "</w:tblBorders>"
    no_mar = '<w:left w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/>'
    wide_mar = '<w:top w:w="72" w:type="dxa"/><w:left w:w="288" w:type="dxa"/><w:bottom w:w="72" w:type="dxa"/><w:right w:w="144" w:type="dxa"/>'
    left_cell = '<w:tcBorders><w:left w:val="single" w:sz="48" w:space="0" w:color="000000"/></w:tcBorders>'
    odd = [1001, 2333, 1777]
    nested = simple([["Nested A", "Nested B"]], [1500, 1500], ind=144)
    body = [
        heading("Table geometry"),
        P("Default borders (0.5pt), default cell margins; compat 15 puts the outer border edge on the margin, compat 12 "
          "pulls the table left by the first cell's left margin:"),
        simple([["Cell one", "Cell two", "Cell three"]], [3120, 3120, 3120]),
        P("3pt borders and no cell margins: the pull is half the border width."),
        simple([["Thick one", "Thick two"]], [4680, 4680], borders=thick, mar=no_mar),
        P("Indent 720 and margins 288 / 144:"),
        simple([["Indented", "Wide margins"]], [3000, 3000], ind=720, mar=wide_mar),
        P("Odd grid widths: 1001, 2333 and 1777 twips:"),
        simple([["1001", "2333", "1777"]], odd),
        P("A first cell with its own 6pt left border:"),
        table([row([cell("Own border", 3000, left_cell), cell("Table border", 3000)])], [3000, 3000]),
        P("Centred and right-aligned tables:"),
        simple([["Centred", "table"]], [2000, 2000], jc="center"),
        P("Between."),
        simple([["Right", "table"]], [2000, 2000], jc="right"),
        P("A nested table, indented 144 in its cell:"),
        table([row([cell(P("Outer cell") + nested, 4000), cell("Second outer cell", 3000)])], [4000, 3000]),
        P("Paragraph after the tables."),
    ]
    return "".join(body), sect()


def html_divs(d):
    def div(i, left, right, top, bottom, border="", children=""):
        b = f"<w:divBdr>{border}</w:divBdr>" if border else ""
        return (f'<w:div w:id="{i}"><w:marLeft w:val="{left}"/><w:marRight w:val="{right}"/><w:marTop w:val="{top}"/>'
                f'<w:marBottom w:val="{bottom}"/>{b}{f"<w:divsChild>{children}</w:divsChild>" if children else ""}</w:div>')

    outer_border = bdr("top", sz=12, space=6) + bdr("left", sz=12, space=6) + bdr("bottom", sz=12, space=6) + bdr("right", sz=12, space=6)
    inner = div(102, 720, 0, 120, 0)
    d.web = f"<w:divs>{div(101, 1440, 720, 240, 240, outer_border, inner)}{div(103, 360, 360, 0, 480)}</w:divs>"

    def in_div(text, i):
        return P(text, f'<w:spacing w:after="0"/><w:divId w:val="{i}"/>')

    body = [
        heading("HTML divisions"),
        P("Plain paragraph with no space after.", '<w:spacing w:after="0"/>'),
        in_div("In division 101: 72pt left and 36pt right margins, 12pt top margin, a 1.5pt border with 6pt space. " + filler(30, 1), 101),
        in_div("Still in division 101. " + filler(20, 2), 101),
        in_div("In division 102, a child of 101: another 36pt on the left, 6pt more at its top. " + filler(25, 3), 102),
        in_div("Back in 101, after the child ends.", 101),
        in_div("Division 103 starts right after 101 ends: 18pt on each side, 24pt below. " + filler(20, 4), 103),
        P("Outside every division."),
        P("An id with no division is ignored.", '<w:divId w:val="999"/>'),
    ]
    return "".join(body), sect()


def decimal_tab(d):
    d.settings_tail = '<w:decimalSymbol w:val=","/><w:listSeparator w:val=";"/>'
    tabs = '<w:tabs><w:tab w:val="decimal" w:pos="4320"/></w:tabs><w:spacing w:after="0"/>'
    body = [heading("Decimal tabs"), P("The settings say the decimal symbol is a comma; the stop still aligns on the period.")]
    for number in ("1,234.56", "3,5", "12.5", "7", "1.000,25", "0.125", "no digits"):
        body.append(p(TAB + r(number), tabs))
    return "".join(body), sect()


# ---- 1.3 float positioning ----------------------------------------------------------------------------------------


def float_sources(d):
    text = lambda k: r(filler(60, k))  # noqa: E731
    col = lambda x: ("column", x)  # noqa: E731
    top = ("paragraph", 0.0)
    box = d.textbox(P("Text box"), 1.4, 0.6, col(4.8), top)
    body = [
        heading("One float model for every source"),
        p(d.picture(1.4, 0.8, col(0.0), top) + text(1)),
        p(d.shape(1.4, 0.8, col(4.8), top) + text(2)),
        p(box + text(3)),
        p(d.group(1.6, 0.9, col(0.0), top) + text(4)),
        p(d.canvas(1.6, 0.9, col(4.6), top) + text(5)),
        p(d.chart(1.8, 1.2, col(0.0), top) + text(6)),
        p(d.vml("position:absolute;margin-left:324pt;margin-top:0;width:100pt;height:58pt;z-index:1;"
                "mso-position-horizontal-relative:margin;mso-position-vertical-relative:paragraph", '<w10:wrap type="square"/>') + text(7)),
        P(filler(40, 8)),
    ]
    return "".join(body), sect()


def float_twips(d):
    top = ("paragraph", 0.0)
    body = [
        heading("Float rectangles in whole twips"),
        P("Offsets and sizes that aren't whole twips (635 EMU each): the offsets are truncated to twips."),
        p(d.picture(1000001, 600007, ("column", 170688), ("paragraph", 1234)) + r("170688 EMU across (268.8 twips, 268). " + filler(50, 1))),
        p(d.picture(1300633, 500311, ("margin", 3000317), ("paragraph", 63499)) + r("3000317 EMU from the margin. " + filler(50, 2))),
        p(d.shape(911111, 455555, ("page", 914399), top) + r("One EMU short of an inch from the page edge. " + filler(50, 3))),
        p(d.vml("position:absolute;margin-left:100.37pt;margin-top:3.33pt;width:72.61pt;height:30.07pt;z-index:1;"
                "mso-position-horizontal-relative:margin;mso-position-vertical-relative:paragraph", '<w10:wrap type="square"/>')
          + r("A VML float at 100.37pt by 3.33pt. " + filler(50, 4))),
        P(filler(40, 5)),
    ]
    return "".join(body), sect()


def float_margin_areas(d):
    sq = lambda hpos, vpos, wrap=NONE: d.shape(0.5, 0.4, hpos, vpos, wrap=wrap)  # noqa: E731
    page1 = [
        sq(("leftMargin", "center"), ("paragraph", 0.0)),
        sq(("rightMargin", 0.1), ("paragraph", 0.0)),
        sq(("insideMargin", "center"), ("paragraph", 0.6)),
        sq(("outsideMargin", "center"), ("paragraph", 0.6)),
        sq(("page", "left"), ("paragraph", 1.2)),
        sq(("page", "right"), ("paragraph", 1.2)),
        sq(("margin", "center"), ("topMargin", "center")),
        sq(("margin", 1.0), ("bottomMargin", 0.2)),
        sq(("margin", 2.0), ("insideMargin", "center")),
        sq(("margin", 3.0), ("outsideMargin", "center")),
    ]
    body = [
        heading("Floats relative to the margin areas"),
        p("".join(page1) + r("Ten wrapNone shapes against the margin areas, the page and the inside and outside margins. " + filler(40, 1))),
        p(sq(("column", "left"), ("paragraph", 0.0), SQUARE) + sq(("column", "right"), ("paragraph", 0.0), SQUARE)
          + r("Left and right of the column, in a paragraph indented 36pt and 72pt: compat 14 counts from the indents. " + filler(60, 2)),
          '<w:ind w:left="720" w:right="1440"/>'),
        P(filler(80, 3)),
        p(PAGE_BREAK + r("Page 2: inside and outside swap sides.")),
        p(sq(("insideMargin", "center"), ("paragraph", 0.3)) + sq(("outsideMargin", "center"), ("paragraph", 0.3))
          + sq(("margin", 2.0), ("insideMargin", "center")) + sq(("margin", 3.0), ("outsideMargin", "center"))
          + sq(("insideMargin", "inside"), ("paragraph", 1.0)) + sq(("outsideMargin", "outside"), ("paragraph", 1.0))
          + r(filler(60, 4))),
    ]
    return "".join(body), sect(mar=(1440, 1080, 1440, 1800))


def float_columns(d):
    cols = '<w:cols w:num="2" w:space="720"/>'
    body = [heading("Floats in columns")]
    body += [P(filler(60, k)) for k in range(4)]
    body.append(p(d.picture(1.6, 2.4, ("column", 0.2), ("paragraph", 0.3)) + r("A tall float anchored near the column bottom: "
                  "compat 15 moves it up to the column bottom, or the anchor paragraph to the next column. " + filler(40, 5))))
    body += [P(filler(50, 10 + k)) for k in range(3)]
    body.append(p(d.picture(1.2, 1.0, ("page", 3.6), ("paragraph", 0.0)) + r("A float anchored in the second column that "
                  "reaches back over the first column wraps both columns' text. " + filler(30, 6))))
    body += [P(filler(50, 20 + k)) for k in range(3)]
    body.append(p(PAGE_BREAK))
    body += [P(filler(70, 30 + k)) for k in range(4)]
    body.append(p(d.picture(1.6, 6.5, ("column", 0.0), ("paragraph", 0.0)) + r("A float taller than the rest of the column: "
                  "compat 15 can't move it up, so the anchor paragraph moves to the next column. " + filler(30, 7))))
    body += [P(filler(50, 40 + k)) for k in range(2)]
    return "".join(body), sect(cols=cols)


def float_story_areas(d):
    header = p(d.picture(1.0, 0.6, ("page", 5.5), ("page", 0.3)) + r("Header text wraps the header's own float, not the body's. " + filler(20, 1)), "<w:pStyle w:val=\"Header\"/>")
    footer = p(d.shape(1.0, 0.5, ("page", 1.5), ("page", 10.0)) + r("Footer text beside the footer's float. " + filler(16, 2)), "<w:pStyle w:val=\"Footer\"/>")
    refs = d.story("header", header) + d.story("footer", footer)
    body = [
        p(d.shape(1.2, 0.5, ("page", 2.0), ("page", 0.35), fill="70AD47") + r("A body float drawn over the header area: "
          "the header text doesn't wrap it. " + filler(30, 3))),
        p(d.picture(1.5, 1.0, ("column", 2.0), ("paragraph", 0.2)) + r("A body float that the body text wraps. " + filler(60, 4))),
        P(filler(80, 5)),
        P(filler(80, 6)),
    ]
    return "".join(body), sect(refs=refs)


def header_float_wrap(d):
    header = p(d.picture(1.8, 1.4, ("page", 3.3), ("page", 3.5)) + d.shape(1.0, 0.6, ("page", 6.0), ("page", 6.5), wrap=NONE)
               + r("Header"), "<w:pStyle w:val=\"Header\"/>")
    footer = p(d.shape(1.5, 0.8, ("margin", "right"), ("page", 8.5), wrap=TOP_BOTTOM) + r("Footer"), "<w:pStyle w:val=\"Footer\"/>")
    refs = d.story("header", header) + d.story("footer", footer)
    body = [heading("Header and footer floats wrap the body")]
    body += [P(filler(90, k)) for k in range(9)]
    return "".join(body), sect(refs=refs)


# ---- 1.4 float sizing and wrapping --------------------------------------------------------------------------------


def float_relative_size(d):
    rel = lambda h, v: (  # noqa: E731
        (f'<wp14:sizeRelH relativeFrom="{h[0]}"><wp14:pctWidth>{h[1]}</wp14:pctWidth></wp14:sizeRelH>' if h else "")
        + (f'<wp14:sizeRelV relativeFrom="{v[0]}"><wp14:pctHeight>{v[1]}</wp14:pctHeight></wp14:sizeRelV>' if v else ""))
    top = ("paragraph", 0.0)
    body = [
        heading("Relative size and position"),
        p(d.textbox(P("50% of the margin width"), 1.0, 0.5, ("margin", "left"), top, tail=rel(("margin", 50000), None)) + r(filler(50, 1))),
        p(d.textbox(P("30% of the page, 10% high"), 1.0, 0.5, ("page", "right"), top, tail=rel(("page", 30000), ("page", 10000))) + r(filler(50, 2))),
        p(d.textbox(P("At 25% of the margin"), 1.2, 0.5, ("margin", ("pct", 25000)), top) + r(filler(50, 3))),
        p(d.textbox(P("Left margin, 80%"), 1.0, 0.5, ("leftMargin", "left"), top, tail=rel(("leftMargin", 80000), None), wrap=NONE)
          + r("A box 80% of the left margin wide, in that margin. " + filler(40, 4))),
        p(d.picture(1.0, 0.75, ("margin", "center"), top, tail=rel(("margin", 40000), ("margin", 5000))) + r("A picture 40% of the margin "
          "wide keeps its aspect ratio (its relative height doesn't apply). " + filler(40, 5))),
        p(d.vml("position:absolute;margin-left:0;margin-top:0;width:100pt;height:40pt;z-index:1;mso-width-percent:400;mso-left-percent:300;"
                "mso-position-horizontal-relative:margin;mso-position-vertical-relative:paragraph;mso-width-relative:margin",
                '<w10:wrap type="square"/>') + r("A VML box 40% of the margin wide at 30% of it. " + filler(40, 6))),
        P(filler(40, 7)),
    ]
    return "".join(body), sect()


def wrap_polygon(d):
    triangle = [(0, 0), (0, 21600), (21600, 21600), (0, 0)]
    notch = [(0, 0), (21600, 0), (21600, 21600), (10800, 10800), (0, 21600), (0, 0)]
    dist = (228600, 228600, 114300, 114300)
    body = [
        heading("Tight and through wrapping around the polygon"),
        p(d.picture(2.0, 2.0, ("column", 0.0), ("paragraph", 0.0), wrap=poly("Tight", triangle), dist=dist)
          + r("Tight wrap around a triangle on the left: each line starts at the diagonal, not at the box's right edge. " + filler(90, 1))),
        P(filler(40, 2)),
        p(d.picture(2.0, 2.0, ("column", 4.5), ("paragraph", 0.0), wrap=poly("Through", notch), dist=dist)
          + r("Through wrap around a notched polygon on the right: the lines reach into the notch. " + filler(90, 3))),
        P(filler(40, 4)),
        p(d.picture(1.6, 1.2, ("column", 2.4), ("paragraph", 0.0), wrap=poly("Tight", [(0, 0), (0, 21600), (21600, 21600), (21600, 0), (0, 0)]),
                    dist=dist) + r("A rectangular polygon with 18pt distances above and below: compat 14 drops them. " + filler(70, 5))),
        P(filler(40, 6)),
    ]
    return "".join(body), sect()


def clear_break(d):
    flt = lambda x, w=1.8: d.picture(w, 1.0, ("column", x), ("paragraph", 0.0))  # noqa: E731
    clr = lambda kind: f'<w:r><w:br w:clear="{kind}"/></w:r>'  # noqa: E731
    cases = [
        ("left", [flt(0.0)], "A float on the left, then a break that clears left."),
        ("right", [flt(4.7)], "A float on the right, then a break that clears right."),
        ("all", [flt(0.0), flt(4.7)], "Floats on both sides, then a break that clears all."),
        ("right", [flt(2.3)], "A centred float, this text left of it, a break that clears right: compat 15 clears, compat 14 doesn't."),
        ("left", [flt(2.3)], "A centred float, a break that clears left from the text left of it."),
        ("none", [flt(0.0)], "A float on the left, then a plain break."),
    ]
    body = [heading("Text-wrapping breaks")]
    for k, (kind, floats, text) in enumerate(cases):
        brk = BR if kind == "none" else clr(kind)
        body.append(p("".join(floats) + r(text) + brk + r("After the break. " + filler(20, k))))
        body.append(P(filler(30, 10 + k)))
    return "".join(body), sect()


def suppress_overlap(d):
    frame = lambda x, y, extra="": (  # noqa: E731
        f'<w:framePr w:w="2880" w:h="720" w:hRule="exact" w:hSpace="180" w:vSpace="0" w:wrap="around" w:vAnchor="text" '
        f'w:hAnchor="margin" w:x="{x}" w:y="{y}"/><w:pBdr>{"".join(bdr(s) for s in ("top", "left", "bottom", "right"))}</w:pBdr>{extra}')
    tight = lambda mode: P(f"textboxTightWrap {mode}: the body wraps this box's lines. " + filler(8, 1),  # noqa: E731
                           f'{TIGHT_SPACING}<w:textboxTightWrap w:val="{mode}"/>')
    bare = d.textbox(tight("allLines") + P("Short line.", TIGHT_SPACING) + tight("firstAndLastLine") + P("Middle line of three.", TIGHT_SPACING)
                     + P("Last line.", TIGHT_SPACING), 2.6, 1.6, ("column", 0.0), ("paragraph", 0.0), fill=None, line=False,
                     wrap=poly("Tight", [(0, 0), (0, 21600), (21600, 21600), (21600, 0), (0, 0)]), insets='lIns="0" tIns="0" rIns="0" bIns="0"')
    body = [
        heading("Frames that may not overlap"),
        P("The first frame.", frame(1440, 0)),
        P("The second frame, over the first, with suppressOverlap: it moves.", frame(2880, 360, "<w:suppressOverlap/>")),
        P(filler(80, 1)),
        P("A third frame, overlapping without the flag, stays.", frame(5760, 0)),
        P("A fourth frame over it, no flag either.", frame(6480, 200)),
        P(filler(60, 2)),
        p(d.picture(1.5, 0.8, ("column", 1.0), ("paragraph", 0.0), overlap=False) + d.picture(1.5, 0.8, ("column", 1.6), ("paragraph", 0.3), overlap=False)
          + r("Two pictures with allowOverlap 0: the later one moves. " + filler(60, 3))),
        P(filler(40, 4)),
        p(bare + r("A bare text box (no fill, no outline) with tight wrap: the body wraps around its lines, the selected lines "
                   "around their ink. " + filler(90, 5))),
    ]
    return "".join(body), sect()


# ---- 1.5 text box autofit and columns -----------------------------------------------------------------------------


def textbox_autofit_columns(d):
    long = "".join(P(filler(30, k), TIGHT_SPACING) for k in range(3))
    body = [
        heading("Text box autofit and columns"),
        p(d.textbox(long, 3.0, 1.2, ("column", 0.0), ("paragraph", 0.0), autofit='<a:normAutofit fontScale="50000" lnSpcReduction="20000"/>')
          + r("The box on the left shrinks text on overflow (50% font scale, 20% line spacing reduction): its text stays full size. " + filler(60, 1))),
        P(filler(30, 2)),
        p(d.textbox(long, 3.0, 2.0, ("column", 3.3), ("paragraph", 0.0), body_attrs=' numCol="2" spcCol="91440"')
          + r("The box on the right asks for two columns: its text is in one. " + filler(60, 3))),
        P(filler(40, 4)),
    ]
    return "".join(body), sect()


# ---- 1.6 table rows and exceptions --------------------------------------------------------------------------------


def tblprex(d):
    widths = [3000, 3000]
    thick = "<w:tblBorders>" + "".join(bdr(s, sz=24) for s in ("top", "left", "bottom", "right", "insideH", "insideV")) + "</w:tblBorders>"
    mar = '<w:tblCellMar><w:left w:w="432" w:type="dxa"/><w:right w:w="432" w:type="dxa"/></w:tblCellMar>'
    rows = [
        row([cell("Row 1, the table's own properties", 3000), cell("", 3000)]),
        row([cell("Row 2: 3pt borders from its exceptions", 3000), cell("", 3000)], ex=thick),
        row([cell("Row 3: 0.3in cell margins", 3000), cell("right", 3000)], ex=mar),
        row([cell("Row 4: centred by its exceptions", 3000), cell("", 3000)], ex='<w:jc w:val="center"/>'),
        row([cell("Row 5: cell spacing and shading", 3000), cell("", 3000)],
            ex='<w:tblCellSpacing w:w="72" w:type="dxa"/><w:shd w:val="clear" w:color="auto" w:fill="D9D9D9"/>'),
        row([cell("Row 6: back to the table's", 3000), cell("", 3000)]),
    ]
    auto_rows = [
        row([cell("Autofit at indent 513", 2500), cell("", 2500)]),
        row([cell("Exception indent 0 alone: ignored", 2500), cell("", 2500)], ex='<w:tblInd w:w="0" w:type="dxa"/>'),
        row([cell("Indent 0 with fixed layout: its own", 2500), cell("", 2500)], ex='<w:tblInd w:w="0" w:type="dxa"/><w:tblLayout w:type="fixed"/>'),
    ]
    body = [
        heading("Table property exceptions"),
        table(rows, widths),
        P("An autofit table whose rows carry indents:"),
        table(auto_rows, [2500, 2500], ind=513, fixed=False),
        P("Paragraph after the tables."),
    ]
    return "".join(body), sect()


def hidden_rows(d):
    vanish = "<w:vanish/>"
    hidden_p = lambda text: p(r(text, vanish), f"<w:rPr>{vanish}</w:rPr>")  # noqa: E731
    thick = "<w:tblBorders>" + "".join(bdr(s, sz=18) for s in ("top", "left", "bottom", "right", "insideH", "insideV")) + "</w:tblBorders>"

    def t(rows):
        return table(rows, [3000, 3000], borders=thick)

    hrow = lambda text: row([cell(hidden_p(text), 3000), cell(hidden_p("hidden"), 3000)], "<w:hidden/>")  # noqa: E731
    vis = lambda text: row([cell(text, 3000), cell("", 3000)])  # noqa: E731
    body = [
        heading("Hidden rows"),
        P("A hidden middle row: every character, mark and the row mark hidden."),
        t([vis("Row 1"), hrow("Row 2, hidden"), vis("Row 3")]),
        P("A hidden first row:"),
        t([hrow("Row 1, hidden"), vis("Row 2"), vis("Row 3")]),
        P("A hidden last row: compat 15 still reserves the bottom border, compat 12 doesn't."),
        t([vis("Row 1"), vis("Row 2"), hrow("Row 3, hidden")]),
        P("trPr/w:hidden alone keeps the row:"),
        t([vis("Row 1"), row([cell("Row 2, row mark hidden only", 3000), cell("", 3000)], "<w:hidden/>"), vis("Row 3")]),
        P("Paragraph after the tables."),
    ]
    return "".join(body), sect()


def hidemark(d):
    hm = "<w:hideMark/>"
    empty = lambda w, mark=True: cell("<w:p/>", w, "", hm if mark else "")  # noqa: E731
    small = '<w:p><w:pPr><w:rPr><w:sz w:val="8"/></w:rPr></w:pPr></w:p>'
    body = [
        heading("Hidden cell marks"),
        P("A row of empty cells marked hideMark collapses to its margins and borders:"),
        table([row([empty(2000), empty(2000), empty(2000)])], [2000, 2000, 2000]),
        P("The same row without hideMark keeps one line:"),
        table([row([empty(2000, False), empty(2000, False), empty(2000, False)])], [2000, 2000, 2000]),
        P("A cell with text and an empty last paragraph, marked: the last paragraph takes no line."),
        table([row([cell(P("Text") + "<w:p/>", 3000, "", hm), cell(P("Text") + "<w:p/>", 3000)])], [3000, 3000]),
        P("A tall exact row keeps its height; a 4pt mark beside text:"),
        table([row([empty(2000), cell(small, 2000, "", hm), cell("Text", 2000)], '<w:trHeight w:val="600" w:hRule="exact"/>')], [2000, 2000, 2000]),
        P("Paragraph after the tables."),
    ]
    return "".join(body), sect()


# ---- 1.7 notes, sections and page numbers -------------------------------------------------------------------------


def note_props(d):
    d.endnote_pr = '<w:pos w:val="sectEnd"/>'
    s1 = sect(fn='<w:pos w:val="beneathText"/><w:numFmt w:val="lowerRoman"/><w:numStart w:val="3"/>',
              en='<w:numFmt w:val="upperRoman"/>')
    s2 = sect(fn='<w:numFmt w:val="upperLetter"/><w:numRestart w:val="eachPage"/>',
              en='<w:numFmt w:val="lowerLetter"/><w:numStart w:val="5"/><w:numRestart w:val="eachPage"/>', kind="nextPage")
    body = [
        heading("Note properties in sections"),
        p(r("Section 1: footnotes beneath the text, lower-roman from iii") + d.footnote("Footnote iii.") + r(", and an endnote")
          + d.endnote("Endnote I, at the section end.") + r(". " + filler(40, 1))),
        p(r("A second footnote") + d.footnote("Footnote iv.") + r(" and a second endnote") + d.endnote("Endnote II.") + r(".")),
        section_break(s1, "Section 1 ends here; its endnotes follow."),
        p(r("Section 2: upper-letter footnotes restarting on each page") + d.footnote("Footnote A on page 2.") + r(", an endnote")
          + d.endnote("Endnote e, referenced on page 2.") + r(". " + filler(30, 2))),
    ]
    body += [P(filler(110, 10 + k)) for k in range(5)]
    body.append(p(r("A footnote on the next page") + d.footnote("Footnote A again: the numbering restarted.") + r(", and an endnote")
                  + d.endnote("Section 2's endnotes are lower-letter from e, restarting on each page.") + r(". " + filler(30, 3))))
    return "".join(body), s2


def no_endnote(d):
    d.endnote_pr = '<w:pos w:val="sectEnd"/>'
    body = [
        heading("Sections without endnotes"),
        p(r("Section 1 has noEndnote: its endnote") + d.endnote("Endnote 1, carried to the end of section 2.") + r(" waits for the next section. " + filler(30, 1))),
        section_break(sect(no_endnote=True, kind="continuous"), "End of section 1."),
        p(r("Section 2 takes endnotes: its own") + d.endnote("Endnote 2, section 2's own.") + r(" and section 1's. " + filler(30, 2))),
        section_break(sect(kind="continuous"), "End of section 2."),
        p(r("Section 3, the last, has noEndnote too: it still takes what is left") + d.endnote("Endnote 3, at the document end.") + r(". " + filler(20, 3))),
    ]
    return "".join(body), sect(no_endnote=True, kind="continuous")


def mirror_indents(d):
    mirrored = '<w:ind w:left="1440" w:right="720" w:firstLine="360"/><w:mirrorIndents/>'
    hanging = '<w:ind w:left="1440" w:right="360" w:hanging="720"/><w:mirrorIndents/>'
    plain = '<w:ind w:left="1440" w:right="720" w:firstLine="360"/>'
    body = [heading("Mirrored indents")]
    for page in range(3):
        if page:
            body.append(p(PAGE_BREAK))
        body.append(P(f"Page {page + 1}, mirrorIndents with left 72pt, right 36pt, first line 18pt. " + filler(50, page), mirrored))
        body.append(P("A hanging indent mirrored: left 72pt, right 18pt, hanging 36pt. " + filler(50, 10 + page), hanging))
        body.append(P("The same indents without mirrorIndents. " + filler(40, 20 + page), plain))
    return "".join(body), sect()


def book_fold(d):
    d.settings_mid = "<w:bookFoldPrinting/>"
    body = [heading("Book fold printing")]
    body += [P(filler(110, k)) for k in range(6)]
    body.append(section_break(sect(w=15840, h=12240, landscape=True, mar=(1440, 2160, 1440, 1440), gutter=720), "End of the landscape section."))
    body.append(P("Portrait section with a gutter: book fold drops it. " + filler(60, 9)))
    return "".join(body), sect(kind="nextPage", gutter=720)


def chapter_numbers(d):
    d.numbering = (
        f"{XML_DECL}<w:numbering {NAMESPACES}><w:abstractNum w:abstractNumId=\"0\"><w:multiLevelType w:val=\"multilevel\"/>"
        '<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:pStyle w:val="Heading1"/><w:lvlText w:val="%1."/>'
        '<w:lvlJc w:val="left"/><w:pPr><w:ind w:left="432" w:hanging="432"/></w:pPr></w:lvl></w:abstractNum>'
        '<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>'
    )
    d.styles = STYLES.replace('<w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="240" w:after="0"/><w:outlineLvl w:val="0"/></w:pPr>',
                              '<w:pPr><w:keepNext/><w:keepLines/><w:numPr><w:numId w:val="1"/></w:numPr><w:spacing w:before="240" w:after="0"/>'
                              '<w:outlineLvl w:val="0"/></w:pPr>')
    page_field = '<w:fldSimple w:instr=" PAGE "><w:r><w:t>1</w:t></w:r></w:fldSimple>'
    footer = p(r("Page ") + page_field + r(" and w:pgNum ") + run("<w:pgNum/>"), '<w:pStyle w:val="Footer"/><w:jc w:val="center"/>')
    refs = d.story("footer", footer)
    body = [p(r("Before the first heading: w:pgNum shows ") + run("<w:pgNum/>") + r("."))]
    body += [heading("Alpha"), P(filler(40, 1)), p(r("In Alpha: ") + run("<w:pgNum/>")), heading("Beta"), P(filler(40, 2)),
             p(r("In Beta: ") + run("<w:pgNum/>") + r(" and a PAGE field ") + page_field)]
    body += [P(filler(110, 10 + k)) for k in range(4)]
    body.append(section_break(sect(refs=refs, pgnum='<w:pgNumType w:chapStyle="1"/>'), "End of section 1 (hyphen separator)."))
    body += [heading("Gamma"), P(filler(40, 3)), p(r("In Gamma, an em dash separator: ") + run("<w:pgNum/>"))]
    return "".join(body), sect(refs=refs, kind="nextPage", pgnum='<w:pgNumType w:start="1" w:chapStyle="1" w:chapSep="emDash"/>')


# ---- 1.8 missing line content -------------------------------------------------------------------------------------


def ruby(d):
    def rb(base, text, base_rpr="", rt_rpr="", align="center"):
        return (
            f'<w:r><w:ruby><w:rubyPr><w:rubyAlign w:val="{align}"/><w:hps w:val="11"/><w:hpsRaise w:val="20"/><w:hpsBaseText w:val="22"/>'
            f'<w:lid w:val="ja-JP"/></w:rubyPr><w:rt>{r(text, rt_rpr + '<w:sz w:val="11"/>')}</w:rt>'
            f"<w:rubyBase>{r(base, base_rpr)}</w:rubyBase></w:ruby></w:r>"
        )
    body = [
        heading("Ruby"),
        p(r("Latin base text with an annotation: ") + rb("Twip", "layout") + r(" and on with the line. " + filler(20, 1))),
        p(r("A CJK base: ") + rb(CJK * 2, "kanji", MINCHO) + r(" then text; ") + rb(CJK, "kan", MINCHO, align="left") + r(" left-aligned ruby.")),
        p(r("A long annotation over a short base: ") + rb("ab", "a long annotation") + r(" " + filler(30, 2))),
        P("A paragraph without ruby, for the line pitch."),
    ]
    return "".join(body), sect()


def legacy_list(d):
    def lvl(i, jc, legacy, space, indent, left, hanging):
        leg = f'<w:legacy w:legacy="1" w:legacySpace="{space}" w:legacyIndent="{indent}"/>' if legacy else ""
        return (f'<w:lvl w:ilvl="{i}"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%{i + 1}."/>{leg}'
                f'<w:lvlJc w:val="{jc}"/><w:pPr><w:ind w:left="{left}" w:hanging="{hanging}"/></w:pPr></w:lvl>')
    levels = [lvl(0, "left", True, 120, 720, 720, 360), lvl(1, "left", True, 360, 360, 720, 360), lvl(2, "center", True, 120, 720, 1440, 360),
              lvl(3, "right", True, 120, 720, 1440, 360)]
    plain = [lvl(0, "left", False, 0, 0, 720, 360)]
    d.numbering = (
        f"{XML_DECL}<w:numbering {NAMESPACES}>"
        f'<w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="multilevel"/>{"".join(levels)}</w:abstractNum>'
        f'<w:abstractNum w:abstractNumId="1"><w:multiLevelType w:val="multilevel"/>{"".join(plain)}</w:abstractNum>'
        '<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num></w:numbering>'
    )
    item = lambda lv, num, text: P(text, f'<w:numPr><w:ilvl w:val="{lv}"/><w:numId w:val="{num}"/></w:numPr><w:spacing w:after="0"/>')  # noqa: E731
    body = [
        heading("Legacy list levels"),
        item(0, 1, "Legacy level, space 6pt, indent 36pt: the text 36pt after the number's start. " + filler(20, 1)),
        item(0, 1, "Second item at that level."),
        item(1, 1, "Legacy level, space 18pt, indent 18pt: number width plus 18pt."),
        item(2, 1, "Legacy level, centred number."),
        item(3, 1, "Legacy level, right-aligned number."),
        P("The same indents without w:legacy, for comparison:"),
        item(0, 2, "A normal level: the tab after the number goes to the hanging indent. " + filler(20, 2)),
    ]
    return "".join(body), sect()


def fit_text(d):
    fit = lambda text, val, i: r(text, f'<w:fitText w:val="{val}" w:id="{i}"/>')  # noqa: E731
    body = [
        p(r("S00 ") + fit("abcdef", 2880, 1) + r("|end of the first line, which is special in compat 15.")),
        heading("Fitted text"),
        p(r("Expanded: ") + fit("abc def", 2880, 2) + r("| then the rest of the line. " + filler(20, 1))),
        p(r("Squeezed: ") + fit("a region much wider than its target", 1440, 3) + r("| " + filler(20, 2))),
        p(r("Two runs, one region: ") + fit("first ", 3600, 4) + fit("second", 3600, 4) + r("| " + filler(20, 3))),
        p(r("S00 ") + fit("abcdefghij", 30, 5) + r("|end"), '<w:ind w:right="7200"/>'),
        p(r("S00 ") + fit("abcdef", 2880, 6) + r("|end"), '<w:ind w:right="5500"/>'),
        p(r("A region wider than the line: ") + fit("x" * 10, 12000, 7) + r(" after it.")),
    ]
    return "".join(body), sect()


def spec_vanish(d):
    vanish = "<w:rPr><w:vanish/><w:specVanish/></w:rPr>"
    body = [
        heading("Style separators"),
        P("Centred heading, indents 36pt, its mark vanished: ", f'<w:ind w:left="720" w:right="720"/><w:jc w:val="center"/>{vanish}'),
        P("the next paragraph, left-aligned and indented 72pt with an 18pt first line, joins it on one line. " + filler(70, 1),
          '<w:ind w:left="1440" w:firstLine="360"/>'),
        P("A bordered run-in heading. ", f'{pbdr("top", "bottom", sz=4, space=1)}{vanish}'),
        P("Its paragraph follows without borders: compat 15 boxes the whole joined paragraph. " + filler(50, 2)),
        P("A mark with specVanish but no vanish ends its paragraph.", "<w:rPr><w:specVanish/></w:rPr>"),
        P("The next paragraph starts its own line."),
        p("", vanish),
        P("An empty paragraph before a vanished mark gives nothing: this paragraph keeps its own properties.", '<w:jc w:val="right"/>'),
    ]
    return "".join(body), sect()


def date_time(d):
    blocks = ["dayShort", "dayLong", "monthShort", "monthLong", "yearShort", "yearLong"]
    body = [heading("Run date and time elements")]
    for b in blocks:
        body.append(p(r(f"{b}: [") + run(f"<w:{b}/>") + r("] then text."), '<w:spacing w:after="0"/>'))
    body.append(p(r("All six in one run: ") + run("".join(f"<w:{b}/>" for b in blocks)) + r("|")))
    body.append(p(r("They break lines by their width: " + filler(16, 1) + " ") + run("<w:dayLong/><w:t xml:space=\"preserve\"> </w:t><w:monthLong/>"
                                                                                   "<w:t xml:space=\"preserve\"> </w:t><w:yearLong/>") + r(" " + filler(20, 2))))
    return "".join(body), sect()


def number_forms(d):
    w14 = lambda inner: r("0123456789 1111111111 " + filler(6, 1), inner)  # noqa: E731
    body = [
        heading("OpenType number forms, spacing and stylistic sets"),
        p(w14("")),
        p(w14('<w14:numForm w14:val="oldStyle"/>')),
        p(w14('<w14:numForm w14:val="lining"/>')),
        p(w14('<w14:numSpacing w14:val="proportional"/>')),
        p(w14('<w14:numSpacing w14:val="tabular"/>')),
        p(w14('<w14:stylisticSets><w14:styleSet w14:id="1"/><w14:styleSet w14:id="2"/></w14:stylisticSets>')),
    ]
    return "".join(body), sect()


# ---- 1.9 compatibility modes and options --------------------------------------------------------------------------


def compat_body(d):
    """One body that shows the mode differences: justified breaking, page-top space, tab stops past the indent, lines
    beside floats, table position, table-style font size, multiple spacing with a picture, minimum wrap gaps, notes in
    columns, floats in cells and a rotated float."""
    d.styles = STYLES.replace('<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>',
                              '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/>'
                              '<w:rPr><w:sz w:val="24"/><w:szCs w:val="24"/></w:rPr></w:style>').replace(
        "</w:styles>",
        '<w:style w:type="table" w:styleId="Big"><w:name w:val="Big"/><w:basedOn w:val="TableNormal"/><w:rPr><w:sz w:val="32"/></w:rPr>'
        '<w:tblPr><w:tblBorders>' + "".join(bdr(s) for s in ("top", "left", "bottom", "right", "insideH", "insideV")) + "</w:tblBorders></w:tblPr></w:style></w:styles>")
    words = ("Representative international administration responsibilities organisational considerations "
             "notwithstanding comprehensive interpretation documentation ")
    jc = '<w:jc w:val="both"/>'
    flt = d.picture(1.6, 1.5, ("column", 0.0), ("paragraph", 0.0))
    tight = poly("Tight", [(0, 0), (0, 21600), (21600, 21600), (21600, 0), (0, 0)])
    cell_float = d.picture(0.8, 0.5, ("column", 0.1), ("paragraph", 0.1), in_cell=False)
    contextual = '<w:pStyle w:val="ListParagraph"/>'
    body = [
        heading("Compatibility modes"),
        P("Justified: compat 15 may shrink spaces to fit one more word on a line. " + filler(100, 1), jc),
        p(r("A justified line ending in a line break " + filler(8, 2)) + BR + r("and the line after it."), jc),
        p(r("Tabs: a left stop past the right indent ") + TAB + r("then text") + TAB + r("again."), '<w:tabs><w:tab w:val="left" w:pos="9900"/></w:tabs>'),
        p(r("A right stop past the right indent") + TAB + r("right text"), '<w:tabs><w:tab w:val="right" w:pos="10080"/></w:tabs>'),
        p(r("1.") + TAB + r("A hanging indent with a tab after the number: the indent is a tab stop unless an option says not."),
          '<w:ind w:left="720" w:hanging="720"/>'),
        p(flt + r("A float on the left, then a word too long for the gap beside it: ") + r("Pneumonoultramicroscopicsilicovolcanoconiosis" * 2)
          + r(" " + filler(30, 3))),
        p(d.picture(1.0, 1.0, ("column", 0.0), ("paragraph", 0.0), wrap=tight)
          + r("Beside a tight float: compat 14 leaves a 9pt minimum gap, compat 15 18pt. " + filler(40, 4)), '<w:ind w:left="1440" w:hanging="720"/>'),
        P("A table, its cells in the table style's size (16pt) only in compat 12 and 11:"),
        table([row([cell("Cell one", 3000), cell("Cell two", 3000)])], [3000, 3000], style="Big", ind=0),
        P("A table whose first cell holds a float with layoutInCell 0, and contextual spacing in its cells:"),
        table([row([cell(p(cell_float + r("Float in a cell")), 3000), cell(P("Same style", contextual) + P("Same style", contextual), 3000)])],
              [3000, 3000]),
        p(d.picture(2.0, 0.6, ("column", 4.0), ("paragraph", 0.0), rot=5400000) + r("A float turned 90 degrees: compat 12 wraps its "
          "unturned frame, the other modes the turned box. " + filler(50, 5))),
        p(r("A line with a picture ") + d.inline_picture(0.3, 0.2) + r(" and 1.5 line spacing; its mark is 24pt. " + filler(30, 6)),
          '<w:spacing w:after="0" w:line="360" w:lineRule="auto"/><w:rPr><w:sz w:val="48"/></w:rPr>'),
        P("Exactly 20pt lines: the options move the baseline and the room below it. " + filler(30, 7), '<w:spacing w:after="0" w:line="400" w:lineRule="exact"/>'),
        P("Automatic spacing before and after.", '<w:spacing w:before="100" w:beforeAutospacing="1" w:after="100" w:afterAutospacing="1"/>'),
        P("Automatic spacing again.", '<w:spacing w:before="100" w:beforeAutospacing="1" w:after="100" w:afterAutospacing="1"/>'),
        p(r("Small caps, ") + r("Raised Text", '<w:smallCaps/><w:position w:val="6"/>') + r(", and spaced ") + r("letters", '<w:spacing w:val="15"/>')
          + r(" " + filler(30, 8))),
        p(r("A paragraph ending in a page break.") + PAGE_BREAK),
        P("Page-break-before with 24pt space before: compat 15 drops it at the page top.", '<w:pageBreakBefore/><w:spacing w:before="480"/>'),
        P("An exact line at the top of a page after a break. " + filler(20, 9), '<w:spacing w:after="0" w:line="480" w:lineRule="exact"/>'),
        P(filler(60, 10)),
        section_break(sect(), ""),
    ]
    note_text = "A long note that runs on: " + filler(90, 11)
    body += [p(r(filler(40, 12 + k)) + d.footnote(note_text if k == 0 else f"Note {k + 1}.")) for k in range(3)]
    body += [P((words * 3).strip(), jc) for _ in range(6)]
    body.append(section_break(sect(cols='<w:cols w:num="2" w:space="720"/>', kind="continuous"), ""))
    body.append(P("A one-column section after the columns: compat 14 balances them unless noColumnBalance."))
    return "".join(body), sect(kind="continuous")


def compat_mode(mode, word_settings):
    def build(d):
        d.compat = mode
        if word_settings:
            d.compat_settings = "".join(
                f'<w:compatSetting w:name="{n}" w:uri="http://schemas.microsoft.com/office/word" w:val="1"/>'
                for n in ("overrideTableStyleFontSizeAndJustification", "enableOpenTypeFeatures", "doNotFlipMirrorIndents"))
        return compat_body(d)
    return build


COMPAT_OPTIONS = ("useSingleBorderforContiguousCells", "noTabHangInd", "noLeading", "noColumnBalance", "noExtraLineSpacing",
                  "doNotExpandShiftReturn", "spacingInWholePoints", "showBreaksInFrames", "suppressBottomSpacing", "suppressTopSpacing",
                  "suppressSpacingAtTopOfPage", "suppressSpBfAfterPgBrk", "mwSmallCaps", "noSpaceRaiseLower",
                  "doNotUseHTMLParagraphAutoSpacing", "growAutofit", "doNotUseIndentAsNumberingTabStop", "allowSpaceOfSameStyleInTable",
                  "doNotSuppressIndentation", "splitPgBreakAndParaMark")


def compat_options(mode):
    def build(d):
        d.compat = mode
        d.compat_opts = "".join(f"<w:{o}/>" for o in COMPAT_OPTIONS)
        return compat_body(d)
    return build


# ---- 1.10 automatic hyphenation -----------------------------------------------------------------------------------

HYPH_WORDS = ("representative hyphenation international administration responsibilities organisational considerations "
              "notwithstanding comprehensive interpretation documentation telecommunication characteristically ")


def hyphenation(d):
    d.settings_mid = '<w:autoHyphenation/><w:consecutiveHyphenLimit w:val="2"/><w:hyphenationZone w:val="720"/><w:doNotHyphenateCaps/>'
    narrow = '<w:ind w:right="4320"/>'
    body = [
        heading("Automatic hyphenation"),
        P("Hyphenated, at most two hyphens in a row, a 36pt zone (compat 14): " + HYPH_WORDS * 2, narrow),
        P("CAPITALS AREN'T HYPHENATED: " + HYPH_WORDS.upper(), narrow),
        p(r("A word marked noProof isn't: ") + r("representative documentation ", "<w:noProof/>") + r(HYPH_WORDS), narrow),
    ]
    body += [P(HYPH_WORDS * 3, '<w:jc w:val="both"/>') for _ in range(7)]
    return "".join(body), sect()


def hyphenation_suppress(d):
    d.settings_mid = "<w:autoHyphenation/>"
    narrow = '<w:ind w:right="4320"/>'
    body = [
        heading("Don't hyphenate"),
        P("Hyphenated: " + HYPH_WORDS * 2, narrow),
        P("suppressAutoHyphens: " + HYPH_WORDS * 2, f"<w:suppressAutoHyphens/>{narrow}"),
    ]
    return "".join(body), sect()


def hyphen_patterns():
    """A small en-US pattern set: a break may come before a consonant followed by a vowel, and between doubled
    consonants; nothing inside the common endings."""
    vowels, consonants = "aeiouy", "bcdfghjklmnpqrstvwxz"
    pats = [f"1{c}{v}" for c in consonants for v in vowels]
    pats += [f"{c}1{c}" for c in consonants]
    pats += ["2tion", "2sion", "2ble", "2ture", "2ty."]
    return "UTF-8\nLEFTHYPHENMIN 2\nRIGHTHYPHENMIN 3\n" + "\n".join(pats) + "\n"


# ---- 1.11 vertical text -------------------------------------------------------------------------------------------


def vertical_directions(d):
    body = []
    for k, direction in enumerate(("tbRl", "btLr", "lrTbV", "tbRlV", "tbLrV")):
        body.append(P(f"Section {k + 1}, textDirection {direction}. " + filler(40, k)))
        body.append(P("Second paragraph. " + CJK * 6 + " " + filler(20, 10 + k)))
        body.append(section_break(sect(direction=direction, kind="nextPage" if k else ""), "End of the section."))
    frame = ('<w:framePr w:w="1440" w:h="2880" w:hRule="exact" w:wrap="around" w:vAnchor="text" w:hAnchor="margin" w:x="0" w:y="0"/>'
             '<w:textDirection w:val="tbRl"/>')
    body.append(P("A frame paragraph with its own tbRl direction. " + filler(12, 1), frame))
    body.append(P("A body paragraph with textDirection tbRl outside a frame stays horizontal. " + filler(40, 2), '<w:textDirection w:val="tbRl"/>'))
    return "".join(body), sect(kind="nextPage")


def vertical_page(d):
    header = P("Header of a vertical section", '<w:pStyle w:val="Header"/>')
    footer = P("Footer of a vertical section", '<w:pStyle w:val="Footer"/>')
    refs = d.story("header", header) + d.story("footer", footer)
    body = [heading("A vertical page")]
    body += [P(filler(70, k)) for k in range(8)]
    return "".join(body), sect(refs=refs, direction="tbRl", mar=(1000, 2000, 3000, 4000), gutter=360, cols='<w:cols w:num="2" w:space="720"/>')


def vertical_glyphs(d):
    body = [
        P("Latin text turned with the line, " + CJK * 4 + " upright, digits 2026 and Latin again. " + filler(20, 1)),
        p(r("Run of CJK: ") + r(CJK * 12, MINCHO) + r(" after it.")),
        P("Mixed " + CJK + "a" + CJK + "b" + CJK + "c, " + filler(16, 2)),
    ]
    first = section_break(sect(direction="tbRl"), "End of the tbRl section.")
    body2 = [P("tbLrV: lines from the left, " + CJK * 4 + " upright. " + filler(20, 3))]
    return "".join(body) + first + "".join(body2), sect(direction="tbLrV", kind="nextPage")


def vertical_model(d):
    td = lambda v: f'<w:textDirection w:val="{v}"/>'  # noqa: E731
    text = "vertical cell text " + filler(8, 1)
    cells = [cell(text, 1400, td("tbRl")), cell(text, 1400, td("btLr")), cell(text + " " + CJK * 3, 1400, td("tbRlV")),
             cell(text, 1400, td("tbLrV")), cell("A horizontal cell beside them.", 2000)]
    body = [
        heading("One vertical model for cells and text boxes"),
        table([row(cells, '<w:trHeight w:val="2400" w:hRule="exact"/>')], [1400, 1400, 1400, 1400, 2000]),
        P("An auto-height row: vertical cells add nothing to its height."),
        table([row([cell("tbRl in an auto row", 1400, td("tbRl")), cell("Horizontal", 3000)])], [1400, 3000]),
        p(d.textbox(P("A vert text box: lines run down, stacked from the right. " + filler(10, 2)), 1.2, 2.0, ("column", 0.0), ("paragraph", 0.0),
                    body_attrs="").replace('vert="horz"', 'vert="vert"')
          + d.textbox(P("A vert270 text box runs up. " + filler(10, 3)), 1.2, 2.0, ("column", 4.0), ("paragraph", 0.0)).replace('vert="horz"', 'vert="vert270"')
          + r(filler(80, 4))),
        P(filler(40, 5)),
    ]
    return "".join(body), sect()


# ---- the set ------------------------------------------------------------------------------------------------------

TWIN = ("c12", "c15")
FIXTURES = [
    # (name, build, modes): modes None writes one document in compat 15
    ("para-borders", para_borders, None),
    ("border-space", border_space, None),
    ("table-geometry", table_geometry, TWIN),
    ("html-divs", html_divs, None),
    ("decimal-tab", decimal_tab, None),
    ("float-sources", float_sources, TWIN),
    ("float-twips", float_twips, None),
    ("float-margin-areas", float_margin_areas, ("c14", "c15")),
    ("float-columns", float_columns, ("c14", "c15")),
    ("float-story-areas", float_story_areas, None),
    ("header-float-wrap", header_float_wrap, None),
    ("float-relative-size", float_relative_size, None),
    ("wrap-polygon", wrap_polygon, ("c14", "c15")),
    ("clear-break", clear_break, ("c14", "c15")),
    ("suppress-overlap", suppress_overlap, ("c14", "c15")),
    ("textbox-autofit-columns", textbox_autofit_columns, None),
    ("tblprex", tblprex, TWIN),
    ("hidden-rows", hidden_rows, TWIN),
    ("hidemark", hidemark, None),
    ("note-props", note_props, TWIN),
    ("no-endnote", no_endnote, None),
    ("mirror-indents", mirror_indents, TWIN),
    ("book-fold", book_fold, None),
    ("chapter-page-numbers", chapter_numbers, None),
    ("ruby", ruby, None),
    ("legacy-list", legacy_list, None),
    ("fit-text", fit_text, TWIN),
    ("spec-vanish", spec_vanish, TWIN),
    ("date-time-elements", date_time, None),
    ("number-forms", number_forms, None),
    ("compat-options", None, TWIN),
    ("hyphenation", hyphenation, TWIN),
    ("hyphenation-suppress", hyphenation_suppress, None),
    ("vertical-directions", vertical_directions, None),
    ("vertical-page", vertical_page, None),
    ("vertical-glyphs", vertical_glyphs, None),
    ("vertical-model", vertical_model, TWIN),
]
COMPAT_MODES = [("compat-2003", 11, False), ("compat-2007", 12, False), ("compat-2010", 14, True), ("compat-2013", 15, True)]


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else HERE
    out.mkdir(parents=True, exist_ok=True)
    jobs = []
    for name, build, modes in FIXTURES:
        for mode in modes or (None,):
            compat = int(mode[1:]) if mode else 15
            b = build or compat_options(compat)
            jobs.append((f"{name}-{mode}" if mode else name, b, compat))
    jobs += [(name, compat_mode(mode, word), mode) for name, mode, word in COMPAT_MODES]
    for name, build, compat in jobs:
        d = Pkg(compat)
        body, sect_pr = build(d)
        d.write(out / f"{name}.docx", body, sect_pr)
    (out / "hyph-en-US.dic").write_bytes(hyphen_patterns().encode("utf-8"))
    print(f"wrote {len(jobs)} fixtures and hyph-en-US.dic to {out}")


if __name__ == "__main__":
    main()
