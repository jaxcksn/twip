"""Writes the metafile drawing fixtures into tests/fixtures/render/ (next to this file).

metafile-wmf.docx: inline WMF pictures, placeable and plain, each built record by record: shapes, pens, hatches and
polygons; map modes, a window and a clipped group; text; embedded and device bitmaps; regions, a pattern brush and a
palette; the shapes picture again, cropped. ole-preview.docx: the previews of embedded objects (`w:object`), as EMF,
WMF and PNG, at their shapes' sizes.

metafile-emf.docx: inline EMF pictures, each built record by record here: shapes, pens and dashes; a clipped and
turned group; text; embedded bitmaps; gradients; a dual EMF and EMF+ file (its EMF+ records draw, the plain ones are
the same picture for readers without EMF+); and the shapes picture again, cropped; raster operations and masks;
regions, palettes and paths. Each EMF's device is 96 dpi and its logical unit a thousandth of an inch (a sixteenth in
the raster operations' picture, so its bitmaps' pixels, a unit each, show).

metafile-emfplus.docx: EMF+ pictures, built record by record: a dual file's shapes, pens, caps and curves (its plain
records a grey stand-in that must not show); brushes (hatches, linear and path gradients, a flipped texture) in an
EMF+-only file; transforms, a container and clips combined by every mode and by a region; text laid out in a rect,
and driver strings; pens (miters clipped and bevelled past their limit, a dashed line's and its dashes' caps,
compound strips mirrored and lopsided), a shown hotkey prefix and a copy under a clip; and images (a bitmap, a
skewed one, an EMF inside, and plain records after a GetDC), in the 96 dpi device's pixels.

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

    def __init__(self, w, h, per_inch=1000, setup=True):
        self.w, self.h = w, h
        self.body = b""
        self.count = 0
        if not setup:
            return
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


class Wmf:
    """A placeable WMF of `w` × `h` inches, drawn in thousandths of an inch (`per_inch` units to the inch)."""

    def __init__(self, w, h, per_inch=1000, placeable=True):
        self.w, self.h, self.per_inch, self.placeable = w, h, per_inch, placeable
        self.body = b""
        self.free = []
        self.used = 0
        self.longest = 3
        # The window the way a Win32 recorder writes it: anisotropic, the whole picture.
        self.w16(0x0103, 8).w16(0x020B, 0, 0).w16(0x020C, round(h * per_inch), round(w * per_inch))

    def rec(self, func, params=b""):
        params += b"\0" * (len(params) % 2)
        words = (len(params) + 6) // 2
        self.longest = max(self.longest, words)
        self.body += struct.pack("<IH", words, func) + params
        return self

    def w16(self, func, *v):
        """A record of 16-bit parameters, given in the order of the call's arguments reversed (as stored)."""
        return self.rec(func, struct.pack(f"<{len(v)}h", *v))

    def obj(self, func, params):
        """Creates an object at the lowest free index, which is returned."""
        self.rec(func, params)
        if self.free:
            return self.free.pop(0)
        self.used += 1
        return self.used - 1

    def select(self, i):
        return self.w16(0x012D, i)

    def delete(self, i):
        self.free = sorted(self.free + [i])
        return self.w16(0x01F0, i)

    def pen(self, style, width, colour):
        i = self.obj(0x02FA, struct.pack("<Hhh", style, width, 0) + struct.pack("<I", rgb(colour)))
        return self.select(i)

    def brush(self, style, colour, hatch=0):
        i = self.obj(0x02FC, struct.pack("<H", style) + struct.pack("<I", rgb(colour)) + struct.pack("<H", hatch))
        return self.select(i)

    def font(self, height, face="JetBrains Mono", weight=400, italic=0, underline=0, escapement=0):
        params = struct.pack("<hhhhh", height, 0, escapement, escapement, weight) + bytes([italic, underline, 0, 0, 0, 0, 0, 0])
        return self.select(self.obj(0x02FB, params + face.encode("latin-1").ljust(32, b"\0")))

    def colour(self, func, c):
        return self.rec(func, struct.pack("<I", rgb(c)))

    def rect(self, func, l, t, r, b):
        return self.w16(func, b, r, t, l)

    def poly(self, func, pts):
        return self.w16(func, len(pts), *[v for q in pts for v in q])

    def polypolygon(self, polys):
        pts = [q for poly in polys for q in poly]
        return self.w16(0x0538, len(polys), *[len(poly) for poly in polys], *[v for q in pts for v in q])

    def arc(self, func, box, start, end):
        l, t, r, b = box
        return self.w16(func, end[1], end[0], start[1], start[0], b, r, t, l)

    def text(self, x, y, s):
        chars = s.encode("latin-1")
        chars += b"\0" * (len(chars) % 2)
        return self.rec(0x0521, struct.pack("<h", len(s)) + chars + struct.pack("<hh", y, x))

    def ext_text(self, x, y, s, dx=None, options=0, box=None):
        chars = s.encode("latin-1")
        chars += b"\0" * (len(chars) % 2)
        params = struct.pack("<hhhH", y, x, len(s), options) + (struct.pack("<4h", *box) if box else b"") + chars
        if dx:
            params += struct.pack(f"<{len(s)}h", *([dx] * len(s)))
        return self.rec(0x0A32, params)

    def stretch_dib(self, dst, src, bmi, rows, rop=0x00CC0020, usage=0):
        """META_STRETCHDIB: `dst` and `src` are (x, y, w, h)."""
        params = struct.pack("<IH", rop, usage) + struct.pack("<8h", src[3], src[2], src[1], src[0], dst[3], dst[2], dst[1], dst[0])
        return self.rec(0x0F43, params + bmi + rows)

    def region(self, rects):
        """META_CREATEREGION of rects (left, top, right, bottom) that share their scans' rows; returns its index."""
        scans = {}
        for l, t, r, b in rects:
            scans.setdefault((t, b), []).append((l, r))
        body = b"".join(struct.pack("<HHH", 2 * len(runs), t, b) + b"".join(struct.pack("<HH", *run) for run in runs) + struct.pack("<H", 2 * len(runs))
                        for (t, b), runs in sorted(scans.items()))
        left, top = min(r[0] for r in rects), min(r[1] for r in rects)
        right, bottom = max(r[2] for r in rects), max(r[3] for r in rects)
        head = struct.pack("<HHIhhhhhhh", 0, 6, 1, 0, len(scans), 0, left, top, right, bottom)
        return self.obj(0x06FF, head + body)

    def bytes(self, name_seed=0):
        eof = struct.pack("<IH", 3, 0)
        words = (18 + len(self.body) + len(eof)) // 2
        std = struct.pack("<HHHIHIH", 1, 9, 0x300, words, self.used, self.longest, 0)
        if not self.placeable:
            return std + self.body + eof
        bbox = (0, 0, round(self.w * self.per_inch), round(self.h * self.per_inch))
        head = struct.pack("<IHhhhhHI", 0x9AC6CDD7, 0, *bbox, self.per_inch, 0)
        check = 0
        for k in range(0, 20, 2):
            check ^= struct.unpack_from("<H", head, k)[0]
        return head + struct.pack("<H", check) + std + self.body + eof


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

    # EmfPlusHeader (dual), a solid fill of a rect, a line (a pen object), and the end of the EMF+ records.
    plus(prec(0x4001, 1, u32(V, 1, 96, 96)), prec(0x401E, 0x09))
    plus(prec(0x400A, 0x8000, u32(0xFF70AD47, 1) + f32(9.6, 9.6, 268.8, 76.8)))
    plus(prec(0x4008, 0x200, pen_data(2.88, solid(0xFF385723))), prec(0x400D, 0, u32(2) + f32(9.6, 48, 278.4, 48)))
    plus(prec(0x4002, 0))
    e.r(SELECTOBJECT, NULL_PEN).brush(1, 0, 0x70AD47).r(RECTANGLE, 100, 100, 2900, 900)
    e.pen(2, 0, 30, 0x385723).r(MOVETOEX, 100, 500).r(LINETO, 2900, 500)
    return e.bytes()


# ---------------- EMF+ ----------------

V = 0xDBC01002  # EmfPlusGraphicsVersion


def f32(*v):
    return struct.pack(f"<{len(v)}f", *v)


def u32(*v):
    return struct.pack(f"<{len(v)}I", *v)


def prec(kind, flags, data=b""):
    """An EMF+ record: its size padded to four bytes, its data size the data's own."""
    return struct.pack("<HHII", kind, flags, 12 + len(data) + (-len(data) % 4), len(data)) + data + b"\0" * (-len(data) % 4)


def solid(argb):
    return u32(V, 0, argb)


def pen_data(width, brush, flags=0, optional=b"", unit=0):
    """An EmfPlusPen: `flags` (PenData) and their optional data, in `brush`."""
    return u32(V, 0, flags, unit) + f32(width) + optional + brush


def utf16(s):
    return s.encode("utf-16-le")


def font_data(em, face="JetBrains Mono", style=0):
    return u32(V) + f32(em) + u32(2, style, 0, len(face)) + utf16(face)


def path_data(pts, types):
    return u32(V, len(pts), 0) + b"".join(f32(*q) for q in pts) + bytes(types)


def bitmap_data(w, h, argb_rows):
    """A 32-bit ARGB EmfPlusBitmap: rows top down, each pixel (a, r, g, b)."""
    px = b"".join(bytes([b, g, r, a]) for row in argb_rows for (a, r, g, b) in row)
    return u32(V, 1, w, h, w * 4, 0x0026200A, 0) + px


class Plus:
    """A metafile of EMF+ records (`w` × `h` inches, drawn in its 96 dpi device's pixels), dual (its plain records
    then drawn by `fallback`) or EMF+ only."""

    def __init__(self, w, h, dual=False):
        self.e = Emf(w, h, setup=False)
        # Anti-aliased, as Office writes them (GDI+'s default smoothing is none).
        self.pending = [prec(0x4001, 1 if dual else 0, u32(V, 1, 96, 96)), prec(0x401E, 0x09)]

    def add(self, *records):
        self.pending += records
        return self

    def flush(self):
        """The records so far, as one comment."""
        if self.pending:
            data = u32(0x2B464D45) + b"".join(self.pending)
            self.e.rec(GDICOMMENT, u32(len(data)) + data)
            self.pending = []
        return self

    def obj(self, kind, ih, data):
        return self.add(prec(0x4008, kind << 8 | ih, data))

    def fill_rect(self, brush, x, y, w, h, colour=True):
        return self.add(prec(0x400A, 0x8000 if colour else 0, u32(brush, 1) + f32(x, y, w, h)))

    def bytes(self):
        self.add(prec(0x4002, 0)).flush()
        return self.e.bytes()


def plus_shapes():
    """Dual: shapes, pens, caps and curves in EMF+; in plain records a grey box crossed out, which must not show."""
    m = Plus(3, 1.5, dual=True)
    m.fill_rect(0xFF70AD47, 10, 10, 80, 50).add(prec(0x400E, 0x8000, u32(0x804472C4) + f32(55, 30, 80, 55)))
    m.obj(2, 0, pen_data(3, solid(0xFFC00000), 0x20, u32(1))).add(prec(0x400D, 0, u32(5) + f32(150, 20, 175, 60, 200, 20, 225, 60, 250, 20)))
    m.obj(2, 1, pen_data(5, solid(0xFF7030A0), 0x2 | 0x4, u32(2, 2))).add(prec(0x4012, 1, f32(180, 180) + f32(150, 75, 60, 60)))
    m.add(prec(0x4010, 0x8000, u32(0xFFFFC000) + f32(-30, 240) + f32(10, 80, 55, 55)))
    m.obj(2, 2, pen_data(2, solid(0xFF000000))).add(prec(0x4011, 2, f32(-30, 240) + f32(10, 80, 55, 55)))
    m.obj(2, 3, pen_data(3, solid(0xFF2E75B6), 0x8, u32(2))).add(prec(0x4018, 3, f32(0.5) + u32(0, 3, 4) + f32(80, 130, 100, 85, 120, 130, 140, 85)))
    # A Bézier with an arrow at its end (an adjustable arrow cap) and an anchor at its start.
    arrow = u32(V, 1) + f32(3, 3, 0) + u32(1, 0, 0, 0) + f32(10, 1, 0, 0, 0, 0)
    optional = u32(0x14, 0xFF) + u32(len(arrow)) + arrow
    m.obj(2, 4, pen_data(2, solid(0xFF385723), 0x2 | 0x4 | 0x1000, optional)).add(prec(0x4019, 4, u32(4) + f32(220, 80, 240, 140, 260, 80, 280, 135)))
    m.add(prec(0x4016, 0x8000, u32(0xFFED7D31) + f32(0.6) + u32(4) + f32(160, 100, 185, 90, 200, 120, 170, 138)))
    m.flush()
    m.e.r(SELECTOBJECT, NULL_PEN).brush(1, 0, 0xA0A0A0).r(RECTANGLE, 0, 0, 3000, 1500)
    m.e.pen(2, 0, 40, 0x000000).r(MOVETOEX, 0, 0).r(LINETO, 3000, 1500).r(MOVETOEX, 0, 1500).r(LINETO, 3000, 0)
    return m.bytes()


def plus_brushes():
    """EMF+ only: hatches, linear and path gradients, and a flipped texture."""
    m = Plus(3, 1.5)
    for i, style in enumerate((0, 3, 5, 9, 36, 52)):
        m.obj(1, 0, u32(V, 1, style, 0xFF1F3864, 0xFFFFF2CC)).fill_rect(0, 8 + i * 22, 8, 18, 40, colour=False)
    # Red to blue, most of the way in its first half.
    m.obj(1, 1, u32(V, 4, 0x08, 0) + f32(8, 54, 130, 30) + u32(0xFFC00000, 0xFF2E75B6, 0, 0) + u32(3) + f32(0, 0.5, 1, 0, 0.8, 1))
    m.fill_rect(1, 8, 54, 130, 30, colour=False)
    # Preset colours, turned.
    turn = math.radians(30)
    xf = f32(math.cos(turn), math.sin(turn), -math.sin(turn), math.cos(turn), 0, 0)
    m.obj(1, 2, u32(V, 4, 0x02 | 0x04, 1) + f32(0, 0, 40, 30) + u32(0, 0, 0, 0) + xf + u32(3) + f32(0, 0.5, 1) + u32(0xFFFFC000, 0xFF70AD47, 0xFF7030A0))
    m.fill_rect(2, 8, 90, 130, 22, colour=False)
    # A square path gradient, its corners each a colour, and an ellipse's with focus scales.
    square = u32(V, 3, 0, 4, 0xFFFFFFFF) + f32(185, 38) + u32(4, 0xFFC00000, 0xFF70AD47, 0xFF2E75B6, 0xFFFFC000) + u32(4) + f32(150, 8, 220, 8, 220, 68, 150, 68)
    m.obj(1, 3, square).fill_rect(3, 150, 8, 70, 60, colour=False)
    k = 0.5523
    ellipse = path_data([(255, 8), (255 + 25 * k, 8), (280, 38 - 30 * k), (280, 38), (280, 38 + 30 * k), (255 + 25 * k, 68), (255, 68),
                         (255 - 25 * k, 68), (230, 38 + 30 * k), (230, 38), (230, 38 - 30 * k), (255 - 25 * k, 8), (255, 8)],
                        [0] + [3] * 11 + [0x83])
    m.obj(1, 4, u32(V, 3, 0x01 | 0x40, 4, 0xFFFFFFFF) + f32(255, 38) + u32(1, 0xFF7030A0) + u32(len(ellipse)) + ellipse + u32(2) + f32(0.4, 0.2))
    m.add(prec(0x400E, 4, u32(4) + f32(230, 8, 50, 60)))
    # A glow: a path gradient fading to transparent, over a dark bar.
    m.fill_rect(0xFF404040, 150, 100, 130, 10)
    glow = u32(V, 3, 0, 4, 0xFFFFC000) + f32(215, 105) + u32(1, 0x00FFC000) + u32(4) + f32(150, 80, 280, 80, 280, 130, 150, 130)
    m.obj(1, 5, glow).fill_rect(5, 150, 80, 130, 50, colour=False)
    # A 4 × 4 texture, mirrored both ways as it tiles.
    o, g, w = (255, 0x2E, 0x75, 0xB6), (255, 0x70, 0xAD, 0x47), (255, 255, 255, 255)
    tile = bitmap_data(4, 4, [[o, o, g, w], [o, g, w, w], [g, w, w, w], [w, w, w, w]])
    m.obj(1, 6, u32(V, 2, 0x02, 3) + f32(3, 0, 0, 3, 8, 116) + tile).fill_rect(6, 8, 116, 130, 24, colour=False)
    return m.bytes()


def plus_transforms():
    """Transforms, a container and clips: every combine mode, and a region."""
    m = Plus(3, 1.5)
    # Saved, turned about (45, 45) and drawn aliased, restored.
    m.add(prec(0x4025, 0, u32(1)), prec(0x401E, 0), prec(0x402D, 0, f32(45, 45)), prec(0x402F, 0, f32(25)), prec(0x402D, 0, f32(-45, -45)))
    m.fill_rect(0xFFC00000, 20, 20, 50, 50).fill_rect(0xC0FFC000, 35, 35, 20, 20).add(prec(0x4026, 0, u32(1)))
    # A container drawing its 100 × 100 units into 60 × 60.
    m.add(prec(0x4027, 2, f32(95, 10, 60, 60, 0, 0, 100, 100) + u32(2)))
    m.add(prec(0x400E, 0x8000, u32(0xFF2E75B6) + f32(0, 0, 100, 100))).fill_rect(0xFFFFFFFF, 25, 25, 50, 50).add(prec(0x4029, 0, u32(2)))
    # Clips: each pair of overlapping rects combined by a mode, then filled.
    for i, mode in enumerate((1, 2, 3, 4, 5)):
        x = 170 + i * 22
        m.add(prec(0x4032, 0, f32(x, 10, 14, 30)), prec(0x4032, mode << 8, f32(x + 6, 18, 14, 30)))
        m.fill_rect(0xFF7030A0, x - 2, 6, 26, 46).add(prec(0x4031, 0))
    # A region: a rect exclusive-or an ellipse path, as the clip of a hatch.
    k = 0.5523
    ellipse = path_data([(225, 75), (225 + 50 * k, 75), (275, 105 - 30 * k), (275, 105), (275, 105 + 30 * k), (225 + 50 * k, 135), (225, 135),
                         (225 - 50 * k, 135), (175, 105 + 30 * k), (175, 105), (175, 105 - 30 * k), (225 - 50 * k, 75), (225, 75)],
                        [0] + [3] * 11 + [0x83])
    region = u32(V, 2, 3, 0x10000000) + f32(170, 70, 60, 70) + u32(0x10000001, len(ellipse)) + ellipse
    m.obj(4, 3, region).add(prec(0x4034, 3)).obj(1, 4, u32(V, 1, 5, 0xFF385723, 0xFFE2F0D9)).fill_rect(4, 160, 60, 125, 85, colour=False).add(prec(0x4031, 0))
    # A star path as the clip, excluded from a rect.
    star = [(85 + 35 * math.sin(i * 4 * math.pi / 5), 110 - 35 * math.cos(i * 4 * math.pi / 5)) for i in range(5)]
    m.obj(3, 5, path_data(star, [0, 1, 1, 1, 0x81])).add(prec(0x4032, 0, f32(40, 75, 90, 65)), prec(0x4033, 4 << 8 | 5))
    m.fill_rect(0xFFED7D31, 40, 75, 90, 65).add(prec(0x4031, 0))
    return m.bytes()


def plus_text():
    """Text: a string wrapped and centred in its rect, one in a gradient, a driver string and vertical text."""
    m = Plus(3, 1.25)
    m.obj(6, 0, font_data(13)).obj(6, 1, font_data(20, style=1))
    centred = u32(V, 0, 0x409, 1, 1, 0, 0) + f32(0) + u32(0) + f32(0, 0, 1) + u32(0, 0, 0)
    m.obj(7, 2, centred).add(prec(0x4032, 0, f32(0, 0, 288, 120)))
    m.fill_rect(0xFFDEEBF7, 8, 8, 120, 64)
    s = "EMF+ text wraps and centres in its rect"
    m.add(prec(0x401C, 0x8000, u32(0xFF1F3864, 2, len(s)) + f32(8, 8, 120, 64) + utf16(s)))
    m.obj(1, 3, u32(V, 4, 0, 0) + f32(140, 0, 140, 30) + u32(0xFFC00000, 0xFF2E75B6, 0, 0))
    s = "Gradient"
    m.add(prec(0x401C, 1, u32(3, 0xFFFFFFFF, len(s)) + f32(140, 10, 0, 0) + utf16(s)))
    glyphs = "Spaced"
    pos = b"".join(f32(140 + 20 * i, 70 + 4 * i) for i in range(len(glyphs)))
    m.add(prec(0x4036, 0x8000, u32(0xFF385723, 1, 0, len(glyphs)) + b"".join(struct.pack("<H", ord(c)) for c in glyphs) + pos))
    vertical = u32(V, 0x2, 0x409, 0, 0, 0, 0) + f32(0) + u32(0) + f32(0, 0, 1) + u32(0, 0, 0)
    s = "Down"
    m.obj(7, 4, vertical).add(prec(0x401C, 0x8000, u32(0xFF7030A0, 4, len(s)) + f32(255, 70, 25, 50) + utf16(s)))
    return m.bytes()


def plus_pens():
    """Pens: miter joins clipped and bevelled past their limit, a dashed line's own and its dashes' caps, compound
    strips mirrored and not; a hotkey prefix shown; a copy under a unioned clip."""
    m = Plus(3, 1.25)
    # A miter (clipped at its limit of 2) and a clipped miter (bevelled past it).
    m.obj(2, 0, pen_data(8, solid(0xFF2E75B6), 0x8 | 0x10, u32(0) + f32(2))).add(prec(0x400D, 0, u32(4) + f32(10, 50, 30, 10, 50, 50, 70, 10)))
    m.obj(2, 1, pen_data(8, solid(0xFFC00000), 0x8 | 0x10, u32(3) + f32(2))).add(prec(0x400D, 1, u32(4) + f32(80, 50, 100, 10, 120, 50, 140, 10)))
    # Dashes with triangle caps, the line's ends round (start) and square (end), half transparent.
    m.obj(2, 2, pen_data(8, solid(0x80385723), 0x2 | 0x4 | 0x20 | 0x40, u32(2, 1, 1, 3))).add(prec(0x400D, 2, u32(2) + f32(160, 20, 280, 20)))
    # Compound: mirrored (thin, thick, thin) and not (thick to the left, thin to the right), along a bend.
    mirrored = u32(6) + f32(0, 0.15, 0.35, 0.65, 0.85, 1)
    m.obj(2, 3, pen_data(14, solid(0xFF7030A0), 0x400, mirrored)).add(prec(0x400D, 3, u32(3) + f32(160, 45, 220, 45, 250, 75)))
    lopsided = u32(4) + f32(0, 0.6, 0.8, 1)
    m.obj(2, 4, pen_data(14, solid(0xFFED7D31), 0x400, lopsided)).add(prec(0x400D, 4, u32(3) + f32(10, 75, 70, 75, 100, 105)))
    # A hotkey prefix shown: the S underlined.
    m.obj(6, 5, font_data(14)).obj(7, 6, u32(V, 0, 0x409, 0, 0, 0, 0) + f32(0) + u32(1) + f32(0, 0, 1) + u32(0, 0, 0))
    s = "&Save && close"
    m.add(prec(0x401C, 0x8005, u32(0xFF1F3864, 6, len(s)) + f32(115, 85, 0, 0) + utf16(s)))
    # Under a clip of two unioned rects, a half-transparent fill copied over a red bar: the bar shows through it only
    # outside the clip.
    m.fill_rect(0xFFC00000, 230, 85, 50, 30)
    m.add(prec(0x4032, 0, f32(232, 88, 20, 24)), prec(0x4032, 2 << 8, f32(258, 88, 20, 24)), prec(0x4023, 1))
    m.fill_rect(0x402E75B6, 225, 80, 60, 40).add(prec(0x4023, 0), prec(0x4031, 0))
    return m.bytes()


def plus_images():
    """Images: a bitmap, a skewed copy, an EMF inside, and plain records after a GetDC."""
    m = Plus(3, 1.5)
    r, b, c = (255, 0xC0, 0, 0), (255, 0x2E, 0x75, 0xB6), (128, 255, 0xC0, 0)
    m.obj(5, 0, bitmap_data(4, 4, [[r, b, r, b], [b, c, c, r], [r, c, c, b], [b, r, b, r]]))
    m.add(prec(0x4021, 5), prec(0x401A, 0, u32(0xFFFFFFFF, 2) + f32(0, 0, 4, 4) + f32(8, 8, 60, 60)))
    m.add(prec(0x401B, 0, u32(0xFFFFFFFF, 2) + f32(0, 0, 4, 4) + u32(3) + f32(85, 20, 145, 8, 95, 68)))
    inner = shapes()
    m.obj(5, 1, u32(V, 2, 3, len(inner)) + inner).add(prec(0x4021, 2), prec(0x401A, 1, u32(0xFFFFFFFF, 2) + f32(0, 0, 288, 144) + f32(160, 8, 120, 60)))
    m.add(prec(0x4004, 0)).flush()
    m.e.r(SELECTOBJECT, NULL_PEN).brush(1, 2, 0x7030A0, 3).r(RECTANGLE, 8, 80, 140, 136)
    m.e.pen(2, 0, 2, 0x000000).brush(3, 0, 0xFFC000).r(ELLIPSE, 160, 80, 280, 136)
    return m.bytes()


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


def wmf_shapes():
    e = Wmf(3, 1.5)
    e.pen(0, 20, 0x1F3864).brush(0, 0xC00000).rect(0x041B, 100, 100, 700, 600)
    e.pen(0, 30, 0x2E75B6).brush(0, 0xBDD7EE).rect(0x0418, 850, 100, 1550, 650)
    e.pen(2, 0, 0x000000).poly(0x0325, [(100, 800), (400, 1300), (700, 800), (1000, 1300)])
    e.w16(0x0102, 1).pen(0, 10, 0x385723).brush(2, 0x70AD47, 5).w16(0x061C, 150, 200, 650, 2400, 100, 1700)
    e.pen(0, 25, 0x7030A0).brush(0, 0xFFC000).arc(0x081A, (2500, 100, 2900, 500), (2900, 300), (2700, 100))
    # A star filled by the alternate rule, then by the winding rule beside it, and a polypolygon.
    star = [(round(500 + 300 * math.sin(i * 4 * math.pi / 5)), round(1050 - 300 * math.cos(i * 4 * math.pi / 5))) for i in range(5)]
    e.pen(0, 0, 0x000000).brush(0, 0xED7D31).w16(0x0106, 1).poly(0x0324, [(x + 1200, y) for x, y in star])
    e.w16(0x0106, 2).brush(0, 0xED7D31).poly(0x0324, [(x + 1900, y) for x, y in star])
    e.pen(0, 15, 0xC00000).brush(1, 0).polypolygon([[(2500, 800), (2900, 800), (2900, 1000), (2500, 1000)], [(2600, 850), (2800, 850), (2700, 950)]])
    e.arc(0x0817, (2500, 1050, 2900, 1450), (2900, 1250), (2500, 1250))
    return e.bytes()


def wmf_window():
    """Map modes and a clipped group, in twips."""
    e = Wmf(3, 1.5, per_inch=1440)
    e.pen(5, 0, 0).brush(0, 0xDEEBF7).rect(0x041B, 0, 0, 4320, 2160)
    # An exclusion of the big square, then an intersection inside it: the clip is empty, and the ellipse is not drawn.
    e.w16(0x001E).rect(0x0415, 288, 288, 1872, 1872).rect(0x0416, 864, 864, 1296, 1296)
    e.brush(0, 0x2E75B6).rect(0x0418, 0, 0, 2160, 2160)
    e.w16(0x0127, -1)
    # MM_LOMETRIC: tenths of a millimetre, y up from the window's origin set to the picture's bottom.
    e.w16(0x0103, 2).w16(0x020B, 0, 0).brush(0, 0x70AD47).rect(0x041B, 500, -100, 700, -300)
    # An anisotropic window of 100 × 100 on the right-hand 1.5 in, flipped down to up.
    e.w16(0x0103, 8).w16(0x020C, 100, 100).w16(0x020E, 108, 144)
    e.w16(0x020B, 0, 0).w16(0x020D, 0, 144)
    e.brush(0, 0xC00000).rect(0x041B, 10, 10, 90, 90).brush(0, 0xFFC000).rect(0x0418, 25, 25, 75, 75)
    return e.bytes()


def wmf_text():
    e = Wmf(3, 1)
    e.w16(0x0102, 1).colour(0x0209, 0x1F3864).w16(0x012E, 24)
    e.font(-160).text(100, 300, "Metafile text")
    e.colour(0x0209, 0xC00000).font(-120, weight=700, italic=1, underline=1).ext_text(100, 600, "Bold italic with spacing", dx=70)
    e.w16(0x012E, 6).colour(0x0209, 0x385723).font(-100, escapement=900).text(2700, 780, "Turned")
    e.w16(0x012E, 24).w16(0x0102, 2).colour(0x0201, 0xFFC000).colour(0x0209, 0x7030A0).font(-110)
    e.ext_text(1500, 100, "Boxed", options=2, box=(1480, 40, 2200, 240))
    return e.bytes()


def wmf_bitmaps():
    e = Wmf(3, 1.5)
    red, blue = b"\x00\x00\xff", b"\xff\x00\x00"
    bmi, rows = Emf.dib(None, 4, 4, checker(4, 4, red, blue))
    e.stretch_dib((100, 100, 1300, 1300), (0, 0, 4, 4), bmi, rows)
    # An 8-bit DIB through its own colour table, then the same bits drawn bottom half only.
    table = bytes([0xC0, 0x00, 0x00, 0, 0x2E, 0x75, 0xB6, 0, 0x70, 0xAD, 0x47, 0, 0xFF, 0xC0, 0x00, 0])
    table = b"".join(bytes([t[2], t[1], t[0], 0]) for t in (table[i:i + 4] for i in range(0, 16, 4)))
    bmi, rows = Emf.dib(None, 4, 2, bytes([0, 1, 2, 3, 3, 2, 1, 0]), 8, table)
    e.stretch_dib((1500, 100, 700, 350), (0, 0, 4, 2), bmi, rows)
    e.stretch_dib((1500, 500, 700, 350), (0, 1, 4, 1), bmi, rows)
    # DIBBITBLT and DIBSTRETCHBLT of a green diamond, with PATBLT stripes beneath.
    e.pen(5, 0, 0).brush(2, 0x7030A0, 4)
    e.rec(0x061D, struct.pack("<I", 0x00F00021) + struct.pack("<4h", 400, 700, 950, 1500))
    diamond = b"".join(b"\x50\xb0\x00" if abs(x - 3.5) + abs(y - 3.5) < 4 else b"\xff\xff\xff" for y in range(8) for x in range(8))
    bmi, rows = Emf.dib(None, 8, 8, diamond)
    e.rec(0x0B41, struct.pack("<I8h", 0x00CC0020, 8, 8, 0, 0, 500, 500, 100, 2300) + bmi + rows)
    # A device bitmap, monochrome: a stripe pattern in the text and background colours.
    e.colour(0x0209, 0xC00000).colour(0x0201, 0xFFC000)
    ddb = struct.pack("<hhhhBB", 0, 8, 8, 2, 1, 1) + bytes([0xF0, 0, 0xF0, 0, 0xF0, 0, 0xF0, 0, 0x0F, 0, 0x0F, 0, 0x0F, 0, 0x0F, 0])
    e.rec(0x0B23, struct.pack("<I8h", 0x00CC0020, 8, 8, 0, 0, 500, 500, 800, 2300) + ddb)
    return e.bytes()


def wmf_regions():
    e = Wmf(3, 2)
    e.pen(5, 0, 0)
    # FillRegion and FrameRegion of an L; InvertRegion over a half black square.
    ell = e.region([(100, 100, 1100, 500), (100, 500, 500, 1000)])
    e.brush(0, 0x2E75B6)
    e.w16(0x0228, ell, e.used - 1)
    e.brush(0, 0xC00000)
    e.w16(0x0429, ell, e.used - 1, 40, 40)
    e.brush(0, 0x000000).rect(0x041B, 1300, 100, 1700, 500)
    sq = e.region([(1250, 50, 1750, 550)])
    e.w16(0x012A, sq)
    # A clip region of two rects: a bar filled across it.
    clip = e.region([(1900, 100, 2300, 500), (2400, 300, 2800, 700)])
    e.w16(0x012C, clip).brush(0, 0x70AD47).rect(0x041B, 1800, 0, 2900, 800)
    e.w16(0x012C, 0)
    # A pattern brush from a DIB, and a palette selected with a DIB indexing it.
    bmi, rows = Emf.dib(None, 8, 8, checker(8, 8, b"\x00\x00\xc0", b"\xff\xc0\x00"))
    e.select(e.obj(0x0142, struct.pack("<HH", 5, 0) + bmi + rows))
    e.rect(0x041B, 100, 1100, 900, 1450)
    entries = bytes([0xC0, 0, 0, 0, 0xFF, 0xC0, 0, 0, 0x70, 0xAD, 0x47, 0, 0x2E, 0x75, 0xB6, 0])
    pal = e.obj(0x00F7, struct.pack("<HH", 0x300, 4) + entries)
    e.w16(0x0234, pal)
    bmi, rows = Emf.dib(None, 4, 1, bytes([0, 1, 2, 3]), 8, struct.pack("<4H", 3, 2, 1, 0), 4)
    e.stretch_dib((1050, 1100, 850, 150), (0, 0, 4, 1), bmi, rows, usage=1)
    # Rop2 of the pen over what is there.
    e.w16(0x0104, 7).brush(0, 0xFFFFFF).rect(0x041B, 2000, 1100, 2900, 1450)
    return e.bytes()


def wmf_plain():
    """A file without a placeable header: sized by its window, a unit a pixel."""
    e = Wmf(2, 1, per_inch=96, placeable=False)
    e.pen(0, 4, 0x1F3864).brush(0, 0xBDD7EE).rect(0x041B, 8, 8, 184, 88).pen(0, 2, 0xC00000)
    e.poly(0x0325, [(16, 80), (60, 16), (100, 80), (144, 16), (176, 80)])
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


def metafile_wmf(d):
    d.defaults.append(("wmf", "image/x-wmf"))
    body = ""
    pictures = [("Shapes, pens, hatches and polygons", wmf_shapes(), 3, 1.5), ("Map modes, a window and a clipped group", wmf_window(), 3, 1.5),
                ("Text", wmf_text(), 3, 1), ("Bitmaps", wmf_bitmaps(), 3, 1.5), ("Regions, a pattern brush and a palette", wmf_regions(), 3, 2),
                ("A file without a placeable header", wmf_plain(), 2, 1)]
    rids = []
    for n, (label, data, w, h) in enumerate(pictures, 1):
        name = f"image{n}.wmf"
        rid = d.rel("image", f"media/{name}")
        rids.append(rid)
        d.parts.append((f"word/media/{name}", data, None))
        body += P(label) + p(picture(d, rid, name, w, h))
    body += P("The shapes, cropped") + p(picture(d, rids[0], "image1.wmf", 1.5, 1.5, '<a:srcRect l="25000" r="25000"/>'))
    return body, lf.sect()


def ole_object(d, rid, ole_rid, shape_id, w, h, prog):
    """An embedded object: its preview picture `rid` in a VML shape of w × h points, and the object part."""
    i = d.uid()
    return (
        '<w:r><w:object w:dxaOrig="%d" w:dyaOrig="%d">' % (round(w * 20), round(h * 20))
        + f'<v:shape id="_x0000_i{1024 + i}" type="#_x0000_t75" style="width:{w}pt;height:{h}pt" o:ole="">'
        + f'<v:imagedata r:id="{rid}" o:title=""/></v:shape>'
        + f'<o:OLEObject Type="Embed" ProgID="{prog}" ShapeID="_x0000_i{1024 + i}" DrawAspect="Content" ObjectID="_{1000000 + i}" r:id="{ole_rid}"/>'
        "</w:object></w:r>"
    )


def ole_preview(d):
    d.defaults += [("emf", "image/x-emf"), ("wmf", "image/x-wmf"), ("bin", "application/vnd.openxmlformats-officedocument.oleObject")]
    body = ""
    # Each preview's shape has its own size; most differ from the picture's frame (3 × 1.5 in, or 2 × 1 in).
    previews = [("An EMF preview, at its frame's size", "emf", shapes(), 216, 108), ("A WMF preview, at its frame's size", "wmf", wmf_shapes(), 216, 108),
                ("An EMF preview in a square shape", "emf", shapes(), 108, 108), ("A WMF preview in a smaller shape", "wmf", wmf_plain(), 108, 54),
                ("A WMF preview in a taller shape", "wmf", wmf_shapes(), 72, 144), ("A PNG preview", "png", lf.png(lf.mf.COLOURS[0]), 108, 108)]
    for n, (label, ext, data, w, h) in enumerate(previews, 1):
        rid = d.rel("image", f"media/preview{n}.{ext}")
        ole = d.rel("oleObject", f"embeddings/oleObject{n}.bin")
        d.parts.append((f"word/media/preview{n}.{ext}", data, None))
        d.parts.append((f"word/embeddings/oleObject{n}.bin", bytes(range(64)), None))
        body += P(label) + p(ole_object(d, rid, ole, n, w, h, "Visio.Drawing.11"))
    return body, lf.sect()


def metafile_emfplus(d):
    d.defaults.append(("emf", "image/x-emf"))
    body = ""
    pictures = [("A dual file: shapes, pens, caps and curves", plus_shapes(), 3, 1.5), ("Brushes, EMF+ only", plus_brushes(), 3, 1.5),
                ("Transforms, a container, clips and a region", plus_transforms(), 3, 1.5), ("Text", plus_text(), 3, 1.25),
                ("Pens, a hotkey and a copy under a clip", plus_pens(), 3, 1.25),
                ("Images, and plain records after a GetDC", plus_images(), 3, 1.5)]
    for n, (label, data, w, h) in enumerate(pictures, 1):
        name = f"image{n}.emf"
        rid = d.rel("image", f"media/{name}")
        d.parts.append((f"word/media/{name}", data, None))
        body += P(label) + p(picture(d, rid, name, w, h))
    return body, lf.sect()


FIXTURES = [("metafile-emf", metafile_emf), ("metafile-wmf", metafile_wmf), ("ole-preview", ole_preview), ("metafile-emfplus", metafile_emfplus)]


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
