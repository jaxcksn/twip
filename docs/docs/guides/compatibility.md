---
title: Compatibility options
sidebar_position: 17
---

# Compatibility options

A document's compatibility mode (`w:compatSetting` `compatibilityMode` in its settings) says which version of Word's
layout it was made for: 15 is Word 2013 and later, 14 Word 2010, 12 Word 2007 and 11 Word 2003. Twip lays out mode 15
one way and modes 14 and below another, as Word does. A document without the setting is mode 12. Word clears most of the
options below when it opens a mode 15 document, and so does Twip: those options act only in modes 14 and below.

Every `w:compat` child and `w:compatSetting` is read and kept: a document saves with the options it was opened with.
The table says what each one does to layout.

- **Laid out**: Twip lays the document out as Word does with the option.
- **No effect in Word**: Word reads the option but no line, row or page moves for it (it is for printing, mail merge,
  the user interface, or Word drops it).
- **Not laid out yet**: the option changes Word's layout and Twip ignores it.
- **East Asian, after 1.0**: the option changes East Asian text, which comes after 1.0 (see the [roadmap](../roadmap)).

## `w:compat` options

| Option | What Twip does |
|---|---|
| `adjustLineHeightInTable` | Laid out: lines in table cells snap to the document grid. |
| `alignTablesRowByRow` | Laid out: each row of a centred or right-aligned table is aligned by its own cells. |
| `allowSpaceOfSameStyleInTable` | Laid out, modes 14 and below: contextual spacing doesn't apply in table cells. |
| `applyBreakingRules` | Laid out: East Asian line-breaking rules in modes 12 and below (always on from mode 14). |
| `autofitToFirstFixedWidthCell` | No effect in Word. |
| `autoSpaceLikeWord95` | East Asian, after 1.0. |
| `balanceSingleByteDoubleByteWidth` | Laid out: runs of spaces widen to the East Asian font's character width. |
| `cachedColBalance` | No effect in Word. |
| `convMailMergeEsc` | No effect in Word (mail merge). |
| `displayHangulFixedWidth` | East Asian, after 1.0. |
| `doNotAutofitConstrainedTables` | Not laid out yet: it changes the width of an autofit table beside a float, and tables beside floats aren't laid out yet. |
| `doNotBreakConstrainedForcedTable` | No effect in Word. |
| `doNotBreakWrappedTables` | Laid out, modes 14 and below: a floating table isn't split across pages. |
| `doNotExpandShiftReturn` | Laid out, mode 15: a justified line ending in a line break isn't justified (in modes 14 and below it never is). |
| `doNotLeaveBackslashAlone` | East Asian, after 1.0. |
| `doNotSnapToGridInCell` | East Asian, after 1.0. |
| `doNotSuppressIndentation` | Laid out, modes 14 and below: a line beside a float drops the indent of the side the float cuts. |
| `doNotSuppressParagraphBorders` | No effect in Word. |
| `doNotUseEastAsianBreakRules` | East Asian, after 1.0. |
| `doNotUseHTMLParagraphAutoSpacing` | Laid out: automatic paragraph spacing is 5pt and the spaces between paragraphs add up. |
| `doNotUseIndentAsNumberingTabStop` | Laid out, modes 14 and below: the hanging indent is no tab stop after a list number. |
| `doNotVertAlignCellWithSp` | Laid out, modes 14 and below: a cell holding a float is top-aligned. |
| `doNotVertAlignInTxbx` | Laid out, modes 14 and below: a text box's text stays at its top. |
| `doNotWrapTextWithPunct` | East Asian, after 1.0. |
| `footnoteLayoutLikeWW8` | Not laid out yet: notes are placed by the mode's usual rules. |
| `forgetLastTabAlignment` | No effect in Word. |
| `growAutofit` | Laid out: an autofit table may grow wider than the room it is in. |
| `layoutRawTableWidth` | Not laid out yet: it changes whether a table fits beside a float, and tables beside floats aren't laid out yet. |
| `layoutTableRowsApart` | No effect in Word. |
| `lineWrapLikeWord6` | East Asian, after 1.0. |
| `mwSmallCaps` | Laid out, modes 14 and below: small capitals at the Macintosh sizes. |
| `noColumnBalance` | Laid out, modes 14 and below: a section before a continuous break isn't balanced. |
| `noExtraLineSpacing` | Laid out: an exact line's baseline sits at the line's ascent. |
| `noLeading` | Laid out, modes 14 and below: lines take no external leading. |
| `noSpaceRaiseLower` | Laid out, modes 14 and below: raised and lowered text doesn't make its line taller. |
| `noTabHangInd` | Laid out, modes 14 and below: the hanging indent is no tab stop. |
| `printBodyTextBeforeHeader` | No effect in Word (printing). |
| `printColBlack` | No effect in Word (printing). |
| `selectFldWithFirstOrLastChar` | No effect in Word (selection). |
| `shapeLayoutLikeWW8` | No effect in Word. |
| `showBreaksInFrames` | Laid out, modes 14 and below: page and column breaks in a frame end its line. |
| `spaceForUL` | East Asian, after 1.0. |
| `spacingInWholePoints` | Laid out, modes 14 and below: character spacing in whole points. |
| `splitPgBreakAndParaMark` | Laid out, modes 14 and below: the mark after a paragraph's last page break starts the next page as an empty line. |
| `subFontBySize` | East Asian, after 1.0. |
| `suppressBottomSpacing` | Laid out: an exact line's room below its text isn't needed at the bottom of a page. |
| `suppressSpacingAtTopOfPage` | Laid out, modes 14 and below: an at-least line at the top of a page loses its height beyond the text. |
| `suppressSpBfAfterPgBrk` | Laid out, modes 14 and below: the text after a paragraph's leading page break loses its space before. |
| `suppressTopSpacing` | Laid out: an exact or at-least line at the top of a page rises. |
| `suppressTopSpacingWP` | Laid out, modes 14 and below: line heights of Latin fonts from the font size. |
| `swapBordersFacingPages` | Not laid out yet: paragraph borders aren't swapped on facing pages. Lines don't move for it. |
| `truncateFontHeightsLikeWP6` | Not laid out yet: line heights from untruncated font heights. |
| `ulTrailSpace` | Laid out: trailing spaces are underlined. |
| `underlineTabInNumList` | Not laid out yet: the tab after an underlined list number isn't underlined. Lines don't move for it. |
| `useAltKinsokuLineBreakRules` | East Asian, after 1.0. |
| `useAnsiKerningPairs` | No effect in Word. |
| `useFELayout` | No effect in Word (Word drops it). |
| `useNormalStyleForList` | No effect in Word (list formatting in the user interface). |
| `usePrinterMetrics` | No effect in Word. |
| `useSingleBorderforContiguousCells` | Laid out, modes 14 and below: a shared cell edge takes one cell's border without comparing them. |
| `useWord2002TableStyleRules` | Laid out, modes 14 and below: table styles apply as in Word 2002. |
| `useWord97LineBreakRules` | Laid out, modes 14 and below: East Asian runs are measured as Word 97 measured them. |
| `wpJustification` | Laid out, modes 14 and below: justified lines as WordPerfect justifies them. |
| `wpSpaceWidth` | No effect in Word. |
| `wrapTrailSpaces` | No effect in Word. |

## `w:compatSetting`

| Setting | What Twip does |
|---|---|
| `compatibilityMode` | Laid out: the mode, above. |
| `overrideTableStyleFontSizeAndJustification` | Laid out: turns off the modes 14 and below table style size and alignment exceptions. |
| `enableOpenTypeFeatures` | Laid out: the run's OpenType features (ligatures, number forms and spacing, stylistic sets) in modes 14 and below; mode 15 always has them. |
| `doNotFlipMirrorIndents` | No effect in Word. |
| `differentiateMultirowTableHeaders` | No effect in Word. |
| `allowHyphenationAtTrackBottom` | No effect in Word. |
| `useWord2013TrackBottomHyphenation` | Not laid out yet: automatic hyphenation of a column's last line. |
| `allowTextAfterFloatingTableBreak` | Not laid out yet: text beside the part of a floating table split onto the next page. |
