---
title: Fonts
sidebar_position: 7
---

# Fonts

Layout measures text with real font files, so the fonts available decide where lines break and pages end. Every
editor takes a `Fonts` catalog when it's created.

- **Native (.NET, Rust):** `Fonts.System()` loads the fonts installed on the machine. A document that names a font
  the machine doesn't have falls back to a similar one, and its lines may break differently.
- **Linux:** documents mostly use Calibri, Cambria, Arial, Times New Roman and Courier New, which Linux doesn't
  ship. Install their metric-compatible stand-ins (same widths, so the same line breaks): Carlito, Caladea and
  Liberation (`fonts-crosextra-carlito fonts-crosextra-caladea fonts-liberation` on Debian and Ubuntu). Twip uses
  them for those names automatically.
- **Browser:** there are no system fonts. Create an empty catalog with `new Fonts()` and add the font files your
  documents use with `addFont(bytes)` (TrueType, OpenType or a collection; `false` if the bytes aren't one).

Add fonts before creating editors: an editor keeps the catalog it was created with. One catalog can serve every
editor in your app.

Font files are licensed separately from Twip. Check that you may ship the fonts you add.
