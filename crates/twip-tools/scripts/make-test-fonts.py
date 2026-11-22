"""Builds the test font kit (crates/twip/tests/fonts): the open, metric-compatible stand-ins for Office's core families,
subset and renamed, so the engine's tests lay out with the same faces on every machine.

    python -I crates/twip-tools/scripts/make-test-fonts.py <liberation-fonts-ttf-2.1.5> <carlito> <caladea> <out>

Sources: Liberation 2.1.5 (https://github.com/liberationfonts/liberation-fonts, SIL OFL 1.1), Carlito 1.103
(https://github.com/googlefonts/carlito, SIL OFL 1.1), Caladea 1.002 (https://github.com/huertatipografica/Caladea,
Apache 2.0). Each folder holds the family's Regular, Bold, Italic and BoldItalic TTFs. The OFL's Reserved Font Names
(Liberation, Carlito) can't name a modified font, so every family gets a Twip Test name.
"""

import os
import sys

from fontTools import subset
from fontTools.ttLib import TTFont

# (source folder index, source file prefix, source family, kit family, metrics of)
FAMILIES = [
    (0, "LiberationSans", "Liberation Sans", "Twip Test Sans", "Arial"),
    (0, "LiberationSerif", "Liberation Serif", "Twip Test Serif", "Times New Roman"),
    (0, "LiberationMono", "Liberation Mono", "Twip Test Mono", "Courier New"),
    (1, "Carlito", "Carlito", "Twip Test Body", "Calibri"),
    (2, "Caladea", "Caladea", "Twip Test Heading", "Cambria"),
]
STYLES = ["Regular", "Bold", "Italic", "BoldItalic"]

# Latin, IPA and spacing modifiers, combining marks, Greek, Cyrillic, Latin Extended Additional, punctuation and
# symbols, box drawing and shapes, ligatures, and the replacement characters.
UNICODES = (
    "0000-036F,0370-03FF,0400-052F,1E00-1EFF,2000-206F,2070-209F,20A0-20CF,2100-218F,2190-23FF,2460-24FF,2500-26FF,"
    "FB00-FB06,FFFC-FFFD"
)


def build(src, prefix, old, new, style, out):
    font = TTFont(os.path.join(src, f"{prefix}-{style}.ttf"))
    options = subset.Options()
    options.layout_features = ["*"]
    options.legacy_kern = True
    options.name_IDs = ["*"]
    options.name_languages = ["*"]
    options.notdef_outline = True
    options.glyph_names = False
    # Glyph instructions and the hinting tables go, except VDMX: the engine's line heights read it.
    options.hinting = False
    options.hinting_tables = ["cvt ", "cvar", "fpgm", "prep", "hdmx", "LTSH"]
    sub = subset.Subsetter(options)
    sub.populate(unicodes=subset.parse_unicodes(UNICODES))
    sub.subset(font)
    old_ps, new_ps = old.replace(" ", ""), new.replace(" ", "")
    for record in font["name"].names:
        if record.nameID in (1, 3, 4, 6, 16, 18, 21):
            text = record.toUnicode().replace(old, new).replace(old_ps, new_ps)
            record.string = text
    path = os.path.join(out, f"{new_ps}-{style}.ttf")
    font.save(path)
    return path


def main():
    *sources, out = sys.argv[1:]
    if len(sources) != 3:
        sys.exit(__doc__)
    os.makedirs(out, exist_ok=True)
    for folder, prefix, old, new, _ in FAMILIES:
        for style in STYLES:
            path = build(sources[folder], prefix, old, new, style, out)
            print(f"{path}: {os.path.getsize(path)} bytes")


if __name__ == "__main__":
    main()
