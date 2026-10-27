---
title: Fields
sidebar_position: 4
---

A field (`w:fldChar` begin, `w:instrText`, separate, cached result, end; or `w:fldSimple`, which behaves the same) either shows its cached result or is computed. A computed field's value replaces the cached result: shape and break it in place, with its own width, so a longer value can re-break its line. Field codes and markers take no space.

### FIELD-1. Which fields are computed {#field-1}

| Fields | What to show |
|---|---|
| `PAGE`, `NUMPAGES`, `SECTIONPAGES`, `SECTION`, `PAGEREF` | the value at layout: `PAGE` on the page the field lands on, `SECTION` the section's number; `NUMPAGES`, `SECTIONPAGES` and `PAGEREF` need passes ([FIELD-6](field#field-6)) |
| `REF`, `SEQ`, `STYLEREF`, `DATE`, `TIME`, `IF` | the value computed once, in document order, before layout ([FIELD-2](field#field-2)); `STYLEREF` in a header or footer is computed per page |
| everything else: `=` formulas, `MERGEFIELD`, `DOCPROPERTY`, `AUTHOR`, `FILENAME`, `NUMWORDS`, `TOC`, unknown fields | the cached result |

- A `TOC` shows its cached entries, but the `PAGEREF` fields nested in them are computed.
- `w:dirty="true"` changes nothing: fields not computed above still show their cached results.
- A computed field that gets no value (a `REF` or `PAGEREF` naming a missing bookmark, an `IF` it can't evaluate) shows its cached result text.

### FIELD-2. Field values computed before layout {#field-2}

- **`REF bookmark`**: the text from the bookmark's start to the end of the paragraph it starts in (fields in it as they show).
- **`SEQ name`**: one counter per name, in document order. Each `SEQ` adds 1 and shows the value; `\r n` sets the value to n; `\c` shows the current value without adding; `\h` adds 1 and shows nothing. Number switches format the value ([FIELD-3](field#field-3)).
- **`STYLEREF "style"`**: match the style name against style ids, ignoring case and spaces ("Heading 1" finds `Heading1`), and show that paragraph's text.
  - In the body: the nearest paragraph with that style at or before the field's paragraph, else the first one after it.
  - In a header or footer: the first paragraph with that style on the page, else the last one on an earlier page, else the first one on a later page.
- **`IF a op b "then" "else"`** with literal operands: numbers compare numerically, anything else as text; `op` is `=`, `<>`, `<`, `>`, `<=` or `>=`. Show "then" or "else" (empty when absent).
- **`DATE`, `TIME`**: the current clock in UTC, formatted by the `\@` picture (default `M/d/yyyy` for both). Picture letters: `yyyy`/`yy`, `MMMM`/`MMM`/`MM`/`M`, `dddd`/`ddd`/`dd`/`d`, `HH`/`H` (24-hour), `hh`/`h` (12-hour), `mm`/`m`, `ss`/`s`; English month and day names; anything else is copied.

### FIELD-3. Number formats {#field-3}

Format a numeric value by its switches. With the value 14:

| Switch | Result |
|---|---|
| `\* Arabic` | 14 |
| `\* roman` | xiv |
| `\* ROMAN`, `\* Roman` | XIV |
| `\* alphabetic` | n |
| `\* ALPHABETIC`, `\* Alphabetic` | N |
| `\* ArabicDash` | - 14 - |
| `\* CardText` | fourteen |
| `\* OrdText` | fourteenth |
| `\* Ordinal` | 14th |
| `\* Hex` | E |
| `\# "000"` | 014 (zero-padded to the number of `0`s) |

- A plain `PAGE` uses the section's `w:pgNumType/@w:fmt` ([LIST-9](list#list-9) formats); a format switch overrides it. Other fields default to decimal.
- The first format switch wins. A case switch (`\* Caps` capitalises each space-separated part, `\* FirstCap` the first letter, `\* Upper`, `\* Lower`) counts only after a format switch: `\* Caps \* CardText` gives "fourteen", `\* CardText \* Caps` gives "Fourteen".
- `\* MERGEFORMAT` and `\* CHARFORMAT` don't change the text.

### FIELD-4. Result formatting {#field-4}

Draw a computed field's value in the formatting of the field code's first run. With `\* MERGEFORMAT`, use the cached result's first run instead (the code's when the result is empty). When the field has no such run at all, use its end marker's run. A field showing its cached result keeps each result run's own formatting.

### FIELD-5. A table of contents {#field-5}

A `TOC` is ordinary paragraphs, usually in the `TOC1`–`TOC9` styles with a right tab stop and dot leader ([TAB-5](tab#tab-5), [TAB-9](tab#tab-9)). Lay them out like any other paragraphs; only their nested `PAGEREF` values change ([FIELD-1](field#field-1)).

In the `TOC` field's result, a run whose character style (`w:rStyle`) is `Hyperlink` shows without that style ([STYLE-1](style#style-1) skips it); other character styles and the run's direct formatting show as usual. The run keeps its `w:rStyle` in the document.

### FIELD-6. Passes for page-dependent values {#field-6}

`NUMPAGES`, `SECTIONPAGES` and `PAGEREF` in the body and notes depend on the pagination they influence, and so do header and footer heights ([HDR-10](hdr#hdr-10)). Lay out in passes:

1. Pass 1 shows these fields' cached results. Compose each page's header and footer with that pass's final values.
2. After each pass, compare: the values the pass produced against those it used, and each page's composed header and footer heights against those its body was laid out with. If both agree, the layout is done.
3. Otherwise lay out again with the new values, up to 4 passes.
4. If pass 4 still disagrees, run one stabilizing pass: each page reserves the largest header and footer height composed for it in passes 1–4, header and footer content is cut to whole lines within that reservation, and fields keep pass 4's values. Record that the layout was stabilized, not converged.

Most documents finish after one pass: a header that shows only the page number keeps its height when the number changes.
