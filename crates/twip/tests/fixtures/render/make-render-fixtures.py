"""Writes the render fixtures into tests/fixtures/render/ (next to this file).

Documents for the render gate, which snapshots every page of these (two of every other fixture's). They name
JetBrains Mono, the one face with real glyphs in the gates' fonts (crates/twip-graphics/tests/fonts), so the pixels
show glyph shape; no other fixture names it. The package helpers come from ../layout/make-layout-fixtures.py.
Standard library only, no randomness: the same bytes on every run.
Run: python -I crates/twip/tests/fixtures/render/make-render-fixtures.py [out dir]
"""

import importlib.util
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("make_layout_fixtures", HERE.parent / "layout" / "make-layout-fixtures.py")
lf = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(lf)

r, p, font, B, I = lf.r, lf.p, lf.font, lf.mf.B, lf.mf.I
MONO = font("JetBrains Mono")
SAMPLE = "Sphinx of black quartz, judge my vow 0123456789"


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


FIXTURES = {"text-glyphs": text_glyphs}


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else HERE
    out.mkdir(parents=True, exist_ok=True)
    for name, build in FIXTURES.items():
        d = lf.Pkg()
        body, sect_pr = build(d)
        d.write(out / f"{name}.docx", body, sect_pr)
    print(f"wrote {len(FIXTURES)} fixtures to {out}")


if __name__ == "__main__":
    main()
