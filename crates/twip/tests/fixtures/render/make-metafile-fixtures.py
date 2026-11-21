"""Writes the metafile drawing fixtures into tests/fixtures/render/ (next to this file).

metafile-emf.docx: inline EMF pictures, each built record by record here: shapes, pens and dashes; a clipped and
turned group; text; embedded bitmaps; gradients; a dual EMF and EMF+ file whose plain records draw; and the shapes
picture again, cropped; raster operations and masks; regions, palettes and paths. Each EMF's device is 96 dpi and
its logical unit a thousandth of an inch (a sixteenth in the raster operations' picture, so its bitmaps' pixels, a
unit each, show).

Standard library only, no randomness: the same bytes on every run. The package helpers come from
../layout/make-layout-fixtures.py. The metafiles' text names JetBrains Mono, the one face with real glyphs in the
gates' fonts; the labels between them are Calibri, which the gates draw with their stand-in.
Run: python -I crates/twip/tests/fixtures/render/make-metafile-fixtures.py [out dir]
"""

import importlib.util
import math
import struct
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("make_layout_fixtures", HERE.parent / "layout" / "make-layout-fixtures.py")
lf = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(lf)

P, p, r, EMU = lf.P, lf.p, lf.r, lf.EMU

# EMR_* record types
HEADER, POLYBEZIER, POLYGON, POLYLINE, EOF = 1, 2, 3, 4, 14
SETWINDOWEXTEX, SETWINDOWORGEX, SETVIEWPORTEXTEX, SETVIEWPORTORGEX = 9, 10, 11, 12
SETMAPMODE, SETBKMODE, SETPOLYFILLMODE, SETTEXTALIGN, SETTEXTCOLOR, SETBKCOLOR = 17, 18, 19, 22, 24, 25
MOVETOEX, EXCLUDECLIPRECT, INTERSECTCLIPRECT, SAVEDC, RESTOREDC = 27, 29, 30, 33, 34
SETWORLDTRANSFORM, SELECTOBJECT, CREATEPEN, CREATEBRUSHINDIRECT, DELETEOBJECT = 35, 37, 38, 39, 40
ELLIPSE, RECTANGLE, ROUNDRECT, ARC, PIE, LINETO = 42, 43, 44, 45, 47, 54
BEGINPATH, ENDPATH, CLOSEFIGURE, STROKEANDFILLPATH, SELECTCLIPPATH = 59, 60, 61, 63, 67
GDICOMMENT, STRETCHDIBITS, EXTCREATEFONTINDIRECTW, EXTTEXTOUTW = 70, 81, 82, 84
SETROP2, BITBLT, MASKBLT, PLGBLT, SETDIBITSTODEVICE, TRANSPARENTBLT = 20, 76, 78, 79, 80, 116
SELECTPALETTE, CREATEPALETTE, FILLRGN, FRAMERGN, INVERTRGN, EXTSELECTCLIPRGN = 48, 49, 71, 72, 73, 75
STROKEPATH, CREATEMONOBRUSH = 64, 93
POLYBEZIER16, POLYGON16, EXTCREATEPEN, SMALLTEXTOUT, ALPHABLEND, GRADIENTFILL = 85, 86, 95, 108, 114, 118
NULL_PEN, BLACK_PEN = 0x80000008, 0x80000007


def i32(*v):
    return b"".join(struct.pack("<i", x) if x < 2**31 else struct.pack("<I", x) for x in v)


def rgb(c):
    """A COLORREF from 0xRRGGBB."""
    return (c >> 16) | (c & 0xFF00) | (c & 0xFF) << 16


class Emf:
    """An EMF of `w` × `h` inches, on a 96 dpi device, drawn in thousandths of an inch."""

    def __init__(self, w, h, per_inch=1000):
        self.w, self.h = w, h
        self.body = b""
        self.count = 0
        self.r(SETMAPMODE, 8)
        self.r(SETWINDOWEXTEX, round(w * per_inch), round(h * per_inch))
        self.r(SETVIEWPORTEXTEX, round(w * 96), round(h * 96))

    def rec(self, kind, body=b""):
        body += b"\0" * (-len(body) % 4)
        self.body += struct.pack("<II", kind, len(body) + 8) + body
        self.count += 1
        return self

    def r(self, kind, *v):
        return self.rec(kind, i32(*v))

    def pen(self, ih, style, width, colour):
        return self.r(CREATEPEN, ih, style, width, 0, rgb(colour)).r(SELECTOBJECT, ih)

    def geometric_pen(self, ih, style, width, colour, dashes=()):
        """EMR_EXTCREATEPEN: a geometric pen (PS_GEOMETRIC), with a user style's dashes."""
        return self.r(EXTCREATEPEN, ih, 0, 0, 0, 0, 0x10000 | style, width, 0, rgb(colour), 0, len(dashes), *dashes).r(SELECTOBJECT, ih)

    def brush(self, ih, style, colour, hatch=0):
        return self.r(CREATEBRUSHINDIRECT, ih, style, rgb(colour), hatch).r(SELECTOBJECT, ih)

    def points(self, kind, pts, short=False):
        fmt = "<hh" if short else "<ii"
        return self.rec(kind, i32(0, 0, 0, 0, len(pts)) + b"".join(struct.pack(fmt, *q) for q in pts))

    def font(self, ih, height, face="JetBrains Mono", weight=400, italic=0, underline=0, escapement=0):
        logfont = i32(height, 0, escapement, escapement, weight) + bytes([italic, underline, 0, 0, 0, 0, 0, 0])
        logfont += face.encode("utf-16-le").ljust(64, b"\0")
        return self.rec(EXTCREATEFONTINDIRECTW, i32(ih) + logfont.ljust(92, b"\0")).r(SELECTOBJECT, ih)

    def text(self, x, y, s, dx=None):
        """EMR_EXTTEXTOUTW at (x, y), with each character's advance when `dx` is given."""
        chars = s.encode("utf-16-le")
        chars += b"\0" * (-len(chars) % 4)
        off_string = 76
        off_dx = off_string + len(chars) if dx else 0
        body = i32(0, 0, 0, 0, 1, 0, 0, x, y, len(s), off_string, 0, 0, 0, 0, 0, off_dx) + chars
        if dx:
            body += i32(*([dx] * len(s)))
        return self.rec(EXTTEXTOUTW, body)

    def dib(self, w, h, bits, bpp=24, table=b"", used=None):
        """A BITMAPINFOHEADER (bottom up) with its colour table (`used` entries, RGBQUADs by default), and its rows,
        padded to four bytes."""
        row = (w * bpp + 7) // 8
        stride = (row + 3) & ~3
        rows = b"".join(bits[y * row:(y + 1) * row].ljust(stride, b"\0") for y in range(h))
        used = (len(table) // 4 if bpp <= 8 else 0) if used is None else used
        return struct.pack("<iiiHHiiiiii", 40, w, h, 1, bpp, 0, len(rows), 0, 0, used, 0) + table, rows

    def stretch_dibits(self, x, y, cx, cy, w, h, bits, rop=0x00CC0020, bpp=24, table=b"", usage=0, used=None):
        bmi, rows = self.dib(w, h, bits, bpp, table, used)
        head = i32(0, 0, 0, 0, x, y, 0, 0, w, h, 80, len(bmi), 80 + len(bmi), len(rows), usage, rop, cx, cy)
        return self.rec(STRETCHDIBITS, head + bmi + rows)

    def blit(self, kind, head, sources):
        """A blit record: its fixed fields (`head`, offsets as None), then each (BITMAPINFO, bits) appended, the
        Nones filled with their offsets and sizes in order."""
        at = 8 + sum(len(x) if isinstance(x, bytes) else 4 for x in head)
        fields = []
        for bmi, rows in sources:
            fields += [at, len(bmi), at + len(bmi), len(rows)]
            at += len(bmi) + len(rows)
        it = iter(fields)
        body = b"".join(x if isinstance(x, bytes) else i32(next(it) if x is None else x) for x in head)
        return self.rec(kind, body + b"".join(b + r for b, r in sources))

    def alpha_blend(self, x, y, cx, cy, w, h, bits, constant=255):
        """EMR_ALPHABLEND of a 32-bit premultiplied DIB, with AC_SRC_ALPHA."""
        bmi, rows = self.dib(w, h, bits, 32)
        xform = struct.pack("<6f", 1, 0, 0, 1, 0, 0)
        head = i32(0, 0, 0, 0, x, y, cx, cy) + bytes([0, 0, constant, 1]) + i32(0, 0) + xform
        head += i32(0, 0, 108, len(bmi), 108 + len(bmi), len(rows), w, h)
        return self.rec(ALPHABLEND, head + bmi + rows)

    def bytes(self):
        frame = (0, 0, round(self.w * 2540), round(self.h * 2540))
        bounds = (0, 0, round(self.w * 96) - 1, round(self.h * 96) - 1)
        eof = i32(EOF, 20, 0, 16, 20)
        size = 108 + len(self.body) + len(eof)
        header = i32(HEADER, 108, *bounds, *frame, 0x464D4520, 0x10000, size, self.count + 2)
        header += struct.pack("<HH", 16, 0) + i32(0, 0, 0, 1920, 1080, 508, 286, 0, 0, 0, 508000, 285750)
        return header + self.body + eof


def shapes():
    e = Emf(3, 1.5)
    e.pen(1, 0, 20, 0x1F3864).brush(2, 0, 0xC00000).r(RECTANGLE, 100, 100, 700, 600)
    e.geometric_pen(3, 1, 30, 0x2E75B6).brush(4, 0, 0xBDD7EE).r(ELLIPSE, 850, 100, 1550, 650)
    e.pen(5, 2, 0, 0x000000).points(POLYLINE, [(100, 800), (400, 1300), (700, 800), (1000, 1300)])
    e.r(SETBKMODE, 1).pen(6, 0, 10, 0x385723).brush(7, 2, 0x70AD47, 5).r(ROUNDRECT, 1700, 100, 2400, 650, 200, 150)
    e.geometric_pen(8, 7, 25, 0x7030A0, (100, 40, 20, 40)).brush(9, 0, 0xFFC000).r(PIE, 2500, 100, 2900, 500, 2900, 300, 2700, 100)
    e.r(SETPOLYFILLMODE, 1).r(SELECTOBJECT, BLACK_PEN).brush(10, 0, 0xED7D31)
    star = [(round(1600 + 300 * math.sin(i * 4 * math.pi / 5)), round(1050 - 300 * math.cos(i * 4 * math.pi / 5))) for i in range(5)]
    e.points(POLYGON16, star, short=True)
    e.pen(11, 0, 15, 0xC00000).points(POLYBEZIER16, [(2000, 1300), (2200, 700), (2600, 1400), (2900, 800)], short=True)
    e.r(ARC, 2000, 900, 2400, 1300, 2400, 1100, 2000, 1100)
    return e.bytes()


def clipped_group():
    e = Emf(3, 1.5)
    e.r(SELECTOBJECT, NULL_PEN).brush(1, 0, 0xDEEBF7).r(RECTANGLE, 0, 0, 3000, 1500)
    e.r(SAVEDC).r(INTERSECTCLIPRECT, 200, 200, 1300, 1300).r(EXCLUDECLIPRECT, 600, 600, 900, 900)
    e.brush(2, 0, 0x2E75B6).r(ELLIPSE, 0, 0, 1500, 1500)
    e.r(RESTOREDC, -1)
    # A path selected as the clip, under a turned world transform.
    turn = math.radians(15)
    e.rec(SETWORLDTRANSFORM, struct.pack("<6f", math.cos(turn), math.sin(turn), -math.sin(turn), math.cos(turn), 2200, 750))
    e.r(BEGINPATH).r(ELLIPSE, -500, -500, 500, 500).r(ENDPATH).r(SELECTCLIPPATH, 5)
    for i, c in enumerate((0xC00000, 0xFFC000, 0x70AD47, 0x7030A0)):
        e.brush(3 + i, 0, c).r(RECTANGLE, -600 + i * 300, -600, -300 + i * 300, 600)
    return e.bytes()


def text():
    e = Emf(3, 1)
    e.r(SETBKMODE, 1).r(SETTEXTCOLOR, rgb(0x1F3864)).r(SETTEXTALIGN, 24)
    e.font(1, -160).text(100, 300, "Metafile text", 90)
    e.r(SETTEXTCOLOR, rgb(0xC00000)).font(2, -120, weight=700, italic=1, underline=1).text(100, 600, "Bold italic")
    e.r(SETTEXTALIGN, 6).r(SETTEXTCOLOR, rgb(0x385723)).font(3, -100, escapement=900).text(2700, 780, "Turned")
    body = i32(1500, 900, 5, 0x300, 1, 0, 0) + b"Small"
    e.r(SETTEXTALIGN, 24).r(SETTEXTCOLOR, rgb(0x7030A0)).font(4, -110).rec(SMALLTEXTOUT, body)
    return e.bytes()


def bitmaps():
    e = Emf(3, 1.5)
    # A 4 × 4 checker of red and blue (rows bottom up, BGR).
    red, blue = b"\x00\x00\xff", b"\xff\x00\x00"
    checker = b"".join((red if (x + y) % 2 else blue) for y in range(4) for x in range(4))
    e.stretch_dibits(100, 100, 1300, 1300, 4, 4, checker)
    # An 8 × 1 green ramp of premultiplied alpha.
    ramp = b"".join(bytes([0, a, 0, a]) for a in (32, 64, 96, 128, 160, 192, 224, 255))
    e.r(SELECTOBJECT, NULL_PEN).brush(1, 2, 0x000000, 4).r(RECTANGLE, 1600, 100, 2900, 1400)
    e.alpha_blend(1600, 300, 1300, 900, 8, 1, ramp)
    return e.bytes()


def gradients():
    e = Emf(3, 1.5)

    def vertex(x, y, c):
        return i32(x, y) + struct.pack("<HHHH", (c >> 16) << 8, (c >> 8 & 0xFF) << 8, (c & 0xFF) << 8, 0)

    across = vertex(100, 100, 0xC00000) + vertex(1400, 650, 0x2E75B6) + vertex(100, 850, 0xFFC000) + vertex(1400, 1400, 0x70AD47)
    e.rec(GRADIENTFILL, i32(0, 0, 0, 0, 4, 2, 0) + across + i32(0, 1, 2, 3))
    down = vertex(1600, 100, 0xFFFFFF) + vertex(2900, 1400, 0x7030A0)
    e.rec(GRADIENTFILL, i32(0, 0, 0, 0, 2, 1, 1) + down + i32(0, 1))
    return e.bytes()


def dual():
    """A dual file: EMF+ records in comments, then the same picture in plain records."""
    e = Emf(3, 1)

    def plus(*records):
        data = struct.pack("<I", 0x2B464D45) + b"".join(records)
        return e.rec(GDICOMMENT, struct.pack("<I", len(data)) + data)

    # EmfPlusHeader (dual), a solid fill of rects, and the end of the EMF+ records.
    plus(struct.pack("<HHIIIIII", 0x4001, 1, 28, 16, 0xDBC01002, 1, 96, 96))
    plus(struct.pack("<HHIIIIffff", 0x400A, 0x8000, 32, 20, 0xFF70AD47, 1, 10, 10, 280, 80))
    plus(struct.pack("<HHII", 0x4002, 0, 12, 0))
    e.r(SELECTOBJECT, NULL_PEN).brush(1, 0, 0x70AD47).r(RECTANGLE, 100, 100, 2900, 900)
    e.pen(2, 0, 30, 0x385723).r(MOVETOEX, 100, 500).r(LINETO, 2900, 500)
    return e.bytes()


XFORM = struct.pack("<6f", 1, 0, 0, 1, 0, 0)
MONO = b"\0\0\0\0\xff\xff\xff\0"


def checker(w, h, a, b):
    """A `w` × `h` 24-bit checker of BGR colours `a` and `b`, bottom row first."""
    return b"".join(a if (x + y) % 2 else b for y in range(h) for x in range(w))


def raster_ops():
    """Raster operations in sixteenths of an inch: a bitmap's pixel a unit."""
    e = Emf(3, 2, per_inch=16)
    e.r(SELECTOBJECT, NULL_PEN)
    # Four stripes whose bits differ, then pens, a pattern and a bitmap combined with them.
    for i, c in enumerate((0x000000, 0xFFFFFF, 0x5AA50F, 0xC33C96)):
        e.brush(1 + i, 0, c).r(RECTANGLE, i * 6, 0, i * 6 + 6, 32)
    e.r(SETROP2, 7).brush(5, 0, 0xFFC000).r(RECTANGLE, 1, 1, 23, 6)
    e.r(SETROP2, 6).r(RECTANGLE, 1, 7, 23, 12).r(SETROP2, 13)
    e.brush(6, 2, 0xC00000, 5)
    e.blit(BITBLT, [0, 0, 0, 0, 1, 13, 22, 6, 0x005A0049, 0, 0, XFORM, 0, 0, 0, 0, 0, 0], [])
    e.stretch_dibits(1, 20, 22, 11, 4, 1, b"\x00\x00\xff\x00\xff\x00\xff\x00\x00\xff\xff\xff", rop=0x00660046)
    # MaskBlt: a checker copied where a disc's bits are set.
    src = e.dib(8, 8, checker(8, 8, b"\x00\x00\xc0", b"\xd6\x75\x2e"))
    disc = bytes(sum(0x80 >> x for x in range(8) if (x - 3.5) ** 2 + (y - 3.5) ** 2 < 14) for y in range(8))
    mask = e.dib(8, 8, disc, 1, MONO)
    e.blit(MASKBLT, [0, 0, 0, 0, 26, 1, 8, 8, 0xAACC0020, 0, 0, XFORM, 0, 0, None, None, None, None, 0, 0, 0, None, None, None, None],
           [src, mask])
    # TransparentBlt: a green diamond on white, white keyed out, stretched over the stripes' colours.
    e.brush(7, 0, 0x7030A0).r(RECTANGLE, 36, 1, 47, 6)
    diamond = b"".join(b"\x50\xb0\x00" if abs(x - 3.5) + abs(y - 3.5) < 4 else b"\xff\xff\xff" for y in range(8) for x in range(8))
    e.blit(TRANSPARENTBLT, [0, 0, 0, 0, 36, 1, 11, 11, 0xFFFFFF, 0, 0, XFORM, 0, 0, None, None, None, None, 8, 8], [e.dib(8, 8, diamond)])
    # PlgBlt onto a parallelogram; InvertRgn over a half-black square.
    src = e.dib(4, 4, checker(4, 4, b"\x00\xc0\xff", b"\x96\x3c\xc3"))
    e.blit(PLGBLT, [0, 0, 0, 0, 27, 14, 35, 12, 29, 22, 0, 0, 4, 4, XFORM, 0, 0, None, None, None, None, 0, 0, 0, 0, 0, 0, 0], [src])
    e.brush(8, 0, 0x000000).r(RECTANGLE, 38, 14, 42, 22)
    e.rec(INVERTRGN, i32(0, 0, 0, 0) + region([(37, 13, 47, 23)]))
    # SetDIBitsToDevice: an 8-row ramp of which the bits hold scans 2 to 5.
    ramp = b"".join(bytes([0, 40 * y, 255 - 30 * y]) * 8 for y in range(2, 6))
    bmi, _ = e.dib(8, 8, b"")
    rows = e.dib(8, 4, ramp)[1]
    e.rec(SETDIBITSTODEVICE, i32(0, 0, 0, 0, 27, 24, 0, 0, 8, 8, 76, len(bmi), 76 + len(bmi), len(rows), 0, 2, 4) + bmi + rows)
    return e.bytes()


def region(rects):
    """RGNDATA of `rects`, with its size first."""
    data = i32(32, 1, len(rects), 16 * len(rects), 0, 0, 0, 0) + b"".join(i32(*r) for r in rects)
    return i32(len(data)) + data


def regions_palettes_paths():
    e = Emf(3, 2)
    px = lambda *v: [round(x * 0.096) for x in v]  # thousandths of an inch to device pixels
    # FrameRgn: an L framed along its outer edge.
    e.r(SELECTOBJECT, NULL_PEN).brush(1, 0, 0xC00000)
    e.rec(FRAMERGN, i32(0, 0, 0, 0) + region([(100, 100, 1100, 500), (100, 500, 500, 1000)])[:4] + i32(1, 40, 40)
          + region([(100, 100, 1100, 500), (100, 500, 500, 1000)])[4:])
    # Clips by union and exclusive or.
    def clip(rects, mode):
        e.rec(EXTSELECTCLIPRGN, region([px(*r) for r in rects])[:4] + i32(mode) + region([px(*r) for r in rects])[4:])
    clip([(1200, 100, 1500, 400)], 5)
    clip([(1350, 250, 1700, 600)], 2)
    e.brush(2, 0, 0x2E75B6).r(RECTANGLE, 0, 0, 3000, 2000)
    clip([(1200, 650, 1500, 950)], 5)
    clip([(1350, 750, 1700, 1050)], 3)
    e.brush(3, 0, 0x70AD47).r(RECTANGLE, 0, 0, 3000, 2000)
    e.rec(EXTSELECTCLIPRGN, i32(0, 5))
    # A triangle shaded between red, green and blue.
    def vertex(x, y, c):
        return i32(x, y) + struct.pack("<HHHH", (c >> 16) << 8, (c >> 8 & 0xFF) << 8, (c & 0xFF) << 8, 0)
    e.rec(GRADIENTFILL, i32(0, 0, 0, 0, 3, 1, 2) + vertex(2000, 100, 0xFF0000) + vertex(2900, 100, 0x00FF00) + vertex(2000, 1000, 0x0000FF)
          + i32(0, 1, 2))
    # A monochrome brush in the text and background colours; a pen stroking a hatch.
    bmi, rows = e.dib(8, 8, bytes([0xF0, 0xF0, 0xF0, 0xF0, 0x0F, 0x0F, 0x0F, 0x0F]), 1, MONO)
    e.r(SETTEXTCOLOR, rgb(0xC00000)).r(SETBKCOLOR, rgb(0xFFC000))
    e.rec(CREATEMONOBRUSH, i32(4, 0, 32, len(bmi), 32 + len(bmi), len(rows)) + bmi + rows).r(SELECTOBJECT, 4)
    e.r(RECTANGLE, 100, 1100, 900, 1450)
    e.r(SETBKCOLOR, rgb(0xFFFFFF)).r(EXTCREATEPEN, 5, 0, 0, 0, 0, 0x10200, 120, 2, rgb(0x7030A0), 4, 0).r(SELECTOBJECT, 5)
    e.points(POLYLINE, [(1000, 1250), (1950, 1250)])
    # A palette selected: a DIB indexing it, a brush by index.
    e.rec(CREATEPALETTE, i32(6) + struct.pack("<HH", 0x300, 4) + bytes([0xC0, 0, 0, 0, 0xFF, 0xC0, 0, 0, 0x70, 0xAD, 0x47, 0, 0x2E, 0x75, 0xB6, 0]))
    e.r(SELECTPALETTE, 6).r(SELECTOBJECT, NULL_PEN)
    e.stretch_dibits(2050, 1100, 850, 150, 4, 1, bytes([0, 1, 2, 3]), bpp=8, table=struct.pack("<4H", 3, 2, 1, 0), usage=1, used=4)
    e.r(CREATEBRUSHINDIRECT, 7, 0, 0x01000001, 0).r(SELECTOBJECT, 7).r(RECTANGLE, 2050, 1300, 2900, 1450)
    # Text outlined through a path, and a line of characters each moved up or down.
    e.r(SETBKMODE, 1).r(SETTEXTALIGN, 24).font(8, -300, weight=700)
    e.r(BEGINPATH).text(100, 1850, "Path").r(ENDPATH)
    e.pen(9, 0, 15, 0x1F3864).brush(10, 0, 0xFFC000).r(STROKEANDFILLPATH, 0, 0, 0, 0)
    e.r(SETTEXTCOLOR, rgb(0x385723)).font(11, -200)
    chars = "Rises"
    body = i32(0, 0, 0, 0, 1, 0, 0, 1100, 1800, len(chars), 76, 0x2000, 0, 0, 0, 0, 76 + 12)
    body += chars.encode("utf-16-le").ljust(12, b"\0") + i32(*[v for k in range(len(chars)) for v in (130, 60 if k % 2 == 0 else -60)])
    e.rec(EXTTEXTOUTW, body)
    return e.bytes()


def picture(d, rid, name, w, h, crop=""):
    i = d.uid()
    cx, cy = round(w * EMU), round(h * EMU)
    return (
        f'<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/>'
        f'<wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="{i}" name="Picture {i}"/><wp:cNvGraphicFramePr/>'
        '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic>'
        f'<pic:nvPicPr><pic:cNvPr id="{i}" name="{name}"/><pic:cNvPicPr/></pic:nvPicPr>'
        f'<pic:blipFill><a:blip r:embed="{rid}"/>{crop}<a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
        f'<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>'
        '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic>'
        "</wp:inline></w:drawing></w:r>"
    )


def metafile_emf(d):
    d.defaults.append(("emf", "image/x-emf"))
    body = ""
    pictures = [("Shapes, pens and dashes", shapes(), 3, 1.5), ("A clipped and turned group", clipped_group(), 3, 1.5),
                ("Text", text(), 3, 1), ("Bitmaps", bitmaps(), 3, 1.5), ("Gradients", gradients(), 3, 1.5),
                ("A dual EMF and EMF+ file", dual(), 3, 1), ("Raster operations and masks", raster_ops(), 3, 2),
                ("Regions, palettes and paths", regions_palettes_paths(), 3, 2)]
    rids = []
    for n, (label, data, w, h) in enumerate(pictures, 1):
        name = f"image{n}.emf"
        rid = d.rel("image", f"media/{name}")
        rids.append(rid)
        d.parts.append((f"word/media/{name}", data, None))
        body += P(label) + p(picture(d, rid, name, w, h))
    body += P("The shapes, cropped") + p(picture(d, rids[0], "image1.emf", 1.5, 1.5, '<a:srcRect l="25000" r="25000"/>'))
    return body, lf.sect()


FIXTURES = [("metafile-emf", metafile_emf)]


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else HERE
    out.mkdir(parents=True, exist_ok=True)
    for name, build in FIXTURES:
        d = lf.Pkg()
        body, sect_pr = build(d)
        d.write(out / f"{name}.docx", body, sect_pr)
    print(f"wrote {len(FIXTURES)} fixtures to {out}")


if __name__ == "__main__":
    main()
