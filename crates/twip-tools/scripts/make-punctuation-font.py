"""Builds the East Asian punctuation font (crates/twip/src/east-asian-punctuation.otf): the characters the engine's East
Asian shared-character rule covers that MS Mincho has, with Noto Serif JP's outlines (SIL OFL 1.1) and MS Mincho's
advance widths, so text lays out as it does in MS Mincho without shipping MS Mincho.

Run once, on a machine with MS Mincho installed (Windows' Japanese supplemental fonts):

    python -I crates/twip-tools/scripts/make-punctuation-font.py <MSMINCHO.TTF> <NotoSerifJP-Regular.otf> <out.otf>

Noto Serif JP: https://github.com/notofonts/noto-cjk (Serif/SubsetOTF/JP). Also writes <out>.widths.json: each
character's advance in 1/1000 em.
"""

import json
import sys

from fontTools import subset
from fontTools.pens.t2CharStringPen import T2CharStringPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from fontTools.ttLib.scaleUpem import scale_upem

NAME = "Twip East Asian Punctuation"


def shared(cp):
    """The engine's shared-character ranges (layout/text.rs `shared_with_east_asia`)."""
    if cp in (0xA1, 0xA4, 0xA7, 0xA8, 0xAA, 0xAD, 0xAF, 0xD7, 0xF7):
        return True
    if 0xB0 <= cp <= 0xB4 or 0xB6 <= cp <= 0xBA or 0xBC <= cp <= 0xBF:
        return True
    if 0x200B <= cp <= 0x200F or 0x2028 <= cp <= 0x202E:
        return False
    return 0x2000 <= cp <= 0x2BFF


def alternates(font, tag):
    """Single substitutions of a GSUB feature: glyph → alternate."""
    out = {}
    gsub = font["GSUB"].table
    for rec in gsub.FeatureList.FeatureRecord:
        if rec.FeatureTag != tag:
            continue
        for i in rec.Feature.LookupListIndex:
            for st in gsub.LookupList.Lookup[i].SubTable:
                st = getattr(st, "ExtSubTable", st)
                out.update(getattr(st, "mapping", {}) or {})
    return out


def main(mincho_path, noto_path, out_path):
    mincho, noto = TTFont(mincho_path), TTFont(noto_path)
    mcmap, ncmap = mincho.getBestCmap(), noto.getBestCmap()
    mupem, nupem = mincho["head"].unitsPerEm, noto["head"].unitsPerEm
    full, half = alternates(noto, "fwid"), alternates(noto, "hwid")
    want = {}  # code point → (glyph, advance in Noto units)
    claimed = {}  # glyph → advance
    for cp in sorted(c for c in mcmap if shared(c) and c in ncmap):
        advance = round(mincho["hmtx"][mcmap[cp]][0] * nupem / mupem)
        glyph = ncmap[cp]
        if noto["hmtx"][glyph][0] != advance:
            alt = (full if advance == nupem else half if advance * 2 == nupem else {}).get(glyph)
            glyph = alt or glyph
        # A glyph two characters share keeps the first one's width; the other is left out.
        if claimed.setdefault(glyph, advance) != advance:
            continue
        want[cp] = (glyph, mincho["hmtx"][mcmap[cp]][0])

    # Keep only those glyphs, mapped from their characters.
    for table in noto["cmap"].tables:
        if table.isUnicode():
            table.cmap = {cp: g for cp, (g, _) in want.items()}
    options = subset.Options()
    options.layout_features = []
    options.hinting = False
    options.desubroutinize = True
    options.name_IDs = [0, 1, 2, 3, 4, 5, 6, 16]
    options.drop_tables += ["BASE", "VORG", "DSIG", "GPOS", "GSUB", "GDEF"]
    options.retain_gids = False
    options.notdef_outline = True
    sub = subset.Subsetter(options)
    sub.populate(glyphs={g for g, _ in want.values()})
    sub.subset(noto)
    # MS Mincho's units per em, so metrics derived in font units round as MS Mincho's do.
    scale_upem(noto, mupem)

    # Each glyph gets MS Mincho's advance, its outline centred in it.
    cff = noto["CFF "].cff
    top = cff[cff.fontNames[0]]
    chars = top.CharStrings
    cmap = noto.getBestCmap()
    done = set()
    for cp, (_, advance) in want.items():
        glyph = cmap[cp]
        if glyph in done:
            continue
        done.add(glyph)
        old, lsb = noto["hmtx"][glyph]
        dx = (advance - old) / 2
        cs = chars[glyph]
        pen = T2CharStringPen(advance, noto.getGlyphSet())
        cs.draw(TransformPen(pen, (1, 0, 0, 1, dx, 0)))
        new = pen.getCharString(private=cs.private, globalSubrs=cs.globalSubrs)
        chars[glyph] = new
        noto["hmtx"][glyph] = (advance, round(lsb + dx))
    noto["hhea"].advanceWidthMax = max(a for a, _ in noto["hmtx"].metrics.values())

    # MS Mincho's vertical metrics and code pages: lines are as tall as with MS Mincho.
    for attr in ("ascent", "descent", "lineGap"):
        setattr(noto["hhea"], attr, getattr(mincho["hhea"], attr))
    mo, no = mincho["OS/2"], noto["OS/2"]
    for attr in ("sTypoAscender", "sTypoDescender", "sTypoLineGap", "usWinAscent", "usWinDescent"):
        setattr(no, attr, getattr(mo, attr))
    no.fsSelection = (no.fsSelection & ~0x80) | (mo.fsSelection & 0x80)
    no.ulCodePageRange1, no.ulCodePageRange2 = mo.ulCodePageRange1, mo.ulCodePageRange2

    # Names: never a family a document names.
    name = noto["name"]
    for rec in list(name.names):
        if rec.nameID in (1, 3, 4, 6, 16):
            name.removeNames(nameID=rec.nameID)
    for nid, value in ((1, NAME), (3, NAME), (4, NAME), (6, NAME.replace(" ", "")), (16, NAME)):
        name.setName(value, nid, 3, 1, 0x409)
    cff.fontNames[0] = NAME.replace(" ", "")
    top.FullName, top.FamilyName = NAME, NAME
    noto.save(out_path)
    widths = {f"{cp:04X}": round(advance * 1000 / mupem) for cp, (_, advance) in want.items()}
    with open(out_path.rsplit(".", 1)[0] + ".widths.json", "w", encoding="utf-8", newline="\n") as f:
        json.dump(widths, f, indent=0, sort_keys=True)
        f.write("\n")
    print(f"{out_path}: {len(want)} characters")


if __name__ == "__main__":
    main(*sys.argv[1:4])
