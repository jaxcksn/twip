---
title: Document format fidelity
sidebar_position: 18
---

# Document format fidelity

What Twip does with every WordprocessingML element: the `w` namespace of ECMA-376 and the Word 2010 to Word 2021
extensions (`w14`, `w15`, `w16cex`, `w16cid`, `w16cur`, `w16se`). DrawingML and VML are listed by feature, at the end.

The document model holds a document's content and formatting: what a reader sees or prints, and what Word's dialogs
set. Settings that are about the application (saving, mail merge, the user interface, web page output, editing
history ids) aren't part of the document model: they're kept on save as they were read, and have no API.

- **API**: in the model, and settable through the API.
- **Model**: in the model, but not settable yet. The chunk adds the API.
- **Kept**: saved as read while its paragraph isn't edited, but not in the model. The chunk puts it in the model, so
  edits keep it too.
- **Lost**: read, but lost on save, on some edits or on every save. The chunk fixes it.
- **Laid out**: in the model, because layout follows it, with no API because Word has no setting for it.
- **Application**: an application setting, kept on save as read. Not in the model by design.
- **After 1.0**: right-to-left or East Asian (see the [roadmap](../roadmap#later)). Kept on save.
- **Not in 1.0**: equations and charts. Kept on save, reported as unsupported content.

Chunk numbers are [roadmap](../roadmap#docx) chunks.

## Story structure

| Construct | Elements | Status |
|---|---|---|
| Parts and stories | `w:document`, `w:body`, `w:hdr`, `w:ftr`, `w:footnotes`, `w:endnotes`, `w:comments`, `w:settings`, `w:styles`, `w:numbering`, `w:fonts`, `w:webSettings`, `w:glossaryDocument`, `w:txbxContent` | API |
| Paragraphs and runs | `w:p`, `w:r`, `w:pPr`, `w:rPr`, `w:t` | API |
| Hyperlinks | `w:hyperlink` | API; `tooltip`, `tgtFrame` and `history` are Lost when a link is rewritten, 2.3 |
| Header and footer references | `w:headerReference`, `w:footerReference` | API for editing; link to previous and removal are Model, 2.6 |
| Section properties | `w:sectPr` | API (see [Sections](#sections)) |
| Unknown elements | anything not listed here | Between runs: Kept as an opaque node, moved with its text, and reported as unsupported content. Inside a run: Lost when its paragraph is pasted or written from the model, 2.3 |

## Run content

| Construct | Elements | Status |
|---|---|---|
| Text, tabs and breaks | `w:t`, `w:tab`, `w:br`, `w:cr`, `w:noBreakHyphen`, `w:softHyphen` | API |
| Absolute position tabs | `w:ptab` | Laid out; inserting one (Insert Alignment Tab) is 2.6 |
| Symbols | `w:sym` | API to insert; the font is Lost when an edited paragraph is rewritten, 2.3 |
| Symbol extensions | `w16se:symEx` | Kept, its fallback character shown; Lost when cut and pasted, 2.3 |
| Page number in a run | `w:pgNum` | Laid out; inserting one is API (`insertPageNumber` inserts a PAGE field) |
| Date blocks | `w:dayShort`, `w:dayLong`, `w:monthShort`, `w:monthLong`, `w:yearShort`, `w:yearLong` | Laid out; Word inserts them only from old documents |
| Deleted text | `w:delText`, `w:delInstrText` | API (track changes) |
| Ruby | `w:ruby`, `w:rubyPr`, `w:rubyAlign`, `w:hps`, `w:hpsRaise`, `w:hpsBaseText`, `w:rubyBase`, `w:rt`, `w:lid`, `w:dirty` | After 1.0 (laid out, not settable) |
| Rendered page breaks | `w:lastRenderedPageBreak` | Application (Word's layout cache) |
| Proofing marks | `w:proofErr` | Application (Word's spelling state) |
| Ink | `w:contentPart`, `w14:contentPart`, `w14:nvContentPartPr`, `w14:cNvPr`, `w14:cNvContentPartPr`, `w14:xfrm` | Drawn from its fallback picture; Kept, Lost when pasted, 2.3 |
| Movies (Word 97) | `w:movie` | Kept; Lost when pasted, 2.3 |
| Equations | `w:oMath` (the run property) | Not in 1.0 (an equation is kept as an opaque node and reported as unsupported content) |

## Fields

| Construct | Elements | Status |
|---|---|---|
| Fields | `w:fldChar`, `w:fldSimple`, `w:instrText` | API (`insertField`, `setField`, `updateFields`); more computed kinds in 2.11 |
| Field data | `w:fldData` | Application (binary data Word keeps for a field) |
| Legacy form fields | `w:ffData`, `w:name`, `w:enabled`, `w:calcOnExit`, `w:entryMacro`, `w:exitMacro`, `w:helpText`, `w:statusText`, `w:checkBox`, `w:size`, `w:sizeAuto`, `w:default`, `w:checked`, `w:ddList`, `w:result`, `w:listEntry`, `w:textInput`, `w:type`, `w:maxLength`, `w:format` | Lost when a field is rewritten; check box and drop-down drawn. Model and API, 2.12 |
| Form field shading | `w:doNotShadeFormData` | 2.12 |
| Document variables | `w:docVars`, `w:docVar` | Kept; Model and API, with DOCVARIABLE, 2.11 |

## Character formatting

| Construct | Elements | Status |
|---|---|---|
| Fonts and size | `w:rFonts`, `w:sz` | API |
| Bold, italic, caps, strike | `w:b`, `w:i`, `w:caps`, `w:smallCaps`, `w:strike`, `w:dstrike` | API |
| Colour, highlight, shading, border | `w:color`, `w:highlight`, `w:shd`, `w:bdr` | API |
| Position, spacing, super- and subscript | `w:position`, `w:spacing`, `w:vertAlign` | API |
| Hidden text | `w:vanish` | API |
| Underline | `w:u` | API for the style; its colour is Lost on new runs, Model and API, 2.4 |
| Character style | `w:rStyle` | Model (style dialog only), 2.4 |
| Scale and kerning | `w:w`, `w:kern` | Model, 2.4 |
| Effects | `w:emboss`, `w:imprint`, `w:outline`, `w:shadow` | Model, 2.4 |
| Do not check spelling | `w:noProof` | Model, 2.4 |
| Language | `w:lang` | Model, 2.4 |
| Fit text | `w:fitText` | Laid out; Model, 2.4 |
| OpenType features | `w14:ligatures`, `w14:numForm`, `w14:numSpacing`, `w14:stylisticSets`, `w14:styleSet`, `w14:cntxtAlts` | Model, 2.4 |
| Text effects | `w14:glow`, `w14:shadow`, `w14:reflection`, `w14:textOutline`, `w14:textFill`, `w14:scene3d`, `w14:props3d` | Kept; Model and API, 2.4 (drawn in 3.5) |
| Text effect colours and lines | `w14:solidFill`, `w14:gradFill`, `w14:noFill`, `w14:gsLst`, `w14:gs`, `w14:lin`, `w14:path`, `w14:fillToRect`, `w14:srgbClr`, `w14:schemeClr`, `w14:alpha`, `w14:tint`, `w14:shade`, `w14:hueMod`, `w14:sat`, `w14:satMod`, `w14:satOff`, `w14:lum`, `w14:lumMod`, `w14:lumOff`, `w14:prstDash`, `w14:round`, `w14:bevel`, `w14:miter`, `w14:camera`, `w14:lightRig`, `w14:rot`, `w14:bevelT`, `w14:bevelB`, `w14:extrusionClr`, `w14:contourClr` | Kept; part of text effects, 2.4 |
| Text animation (Word 97) | `w:effect` | Kept: Word 2007 and later neither show nor set it |
| Hidden in web view | `w:webHidden` | Kept: only Web Layout hides it |
| Style separator | `w:specVanish` | Laid out; kept, checked by the gate in 2.4 |
| Complex scripts and right-to-left | `w:bCs`, `w:iCs`, `w:szCs`, `w:cs`, `w:rtl`, `w:bdo`, `w:dir` | After 1.0 |
| East Asian | `w:eastAsianLayout`, `w:em`, `w:snapToGrid` | After 1.0 |

## Paragraph formatting

| Construct | Elements | Status |
|---|---|---|
| Style, alignment, indents, spacing | `w:pStyle`, `w:jc`, `w:ind`, `w:spacing` | API; mirror indents are Model, 2.5 |
| Pagination | `w:keepNext`, `w:keepLines`, `w:pageBreakBefore`, `w:widowControl` | API |
| Borders and shading | `w:pBdr`, `w:top`, `w:left`, `w:bottom`, `w:right`, `w:between`, `w:bar`, `w:start`, `w:end`, `w:shd` | API |
| Tabs | `w:tabs`, `w:tab` | API |
| Lists | `w:numPr`, `w:ilvl`, `w:numId` | API |
| Frames and drop caps | `w:framePr`, `w:suppressOverlap` | API; creating a drop cap is 2.5 |
| Contextual spacing | `w:contextualSpacing` | Model, 2.5 |
| Mirror indents | `w:mirrorIndents` | Model, 2.5 |
| Outline level | `w:outlineLvl` | Model, 2.5 |
| Collapsed by default | `w15:collapsed` | Kept, 2.5 |
| Text direction | `w:textDirection` | Model (paragraphs, sections, cells), 2.5, 2.6 and 2.9 |
| Text box tight wrap | `w:textboxTightWrap` | Laid out; Model, 2.10 |
| Don't hyphenate | `w:suppressAutoHyphens` | Laid out; Model, 2.5 |
| Line numbers | `w:suppressLineNumbers` | Laid out; Model, 2.6 |
| HTML divisions | `w:divId`, `w:divs`, `w:div`, `w:divBdr`, `w:divsChild`, `w:blockQuote`, `w:bodyDiv`, `w:marLeft`, `w:marRight`, `w:marTop`, `w:marBottom` | Laid out (from pasted HTML; Word has no setting for them) |
| Conditional formatting | `w:cnfStyle` | Kept in paragraphs and rows; Lost in rewritten cells, 2.3 |
| Bidirectional and East Asian | `w:bidi`, `w:adjustRightInd`, `w:kinsoku`, `w:wordWrap`, `w:overflowPunct`, `w:topLinePunct`, `w:autoSpaceDE`, `w:autoSpaceDN`, `w:textAlignment` | After 1.0 |

## Styles and numbering

| Construct | Elements | Status |
|---|---|---|
| Styles | `w:style`, `w:name`, `w:aliases`, `w:basedOn`, `w:next`, `w:link`, `w:autoRedefine`, `w:hidden`, `w:uiPriority`, `w:semiHidden`, `w:unhideWhenUsed`, `w:qFormat`, `w:locked`, `w:personal`, `w:personalCompose`, `w:personalReply`, `w:tblStylePr`, `w:tblStyleRowBandSize`, `w:tblStyleColBandSize` | API (style dialog, `setStyleMeta`); band sizes are Model, 2.9 |
| Defaults | `w:docDefaults`, `w:rPrDefault`, `w:pPrDefault` | API (`setDocumentDefaults`) |
| Latent styles | `w:latentStyles`, `w:lsdException` | Application (the style gallery's list) |
| Lists | `w:abstractNum`, `w:nsid`, `w:multiLevelType`, `w:tmpl`, `w:styleLink`, `w:numStyleLink`, `w:lvl`, `w:start`, `w:numFmt`, `w:lvlRestart`, `w:isLgl`, `w:suff`, `w:lvlText`, `w:lvlPicBulletId`, `w:lvlJc`, `w:num`, `w:abstractNumId`, `w:lvlOverride`, `w:startOverride`, `w:numPicBullet` | API (`setListDefinition`) |
| Legacy list levels | `w:legacy` | Laid out (Word 97 lists; Word has no setting) |
| List cleanup id | `w:numIdMacAtCleanup` | Application |

## Sections {#sections}

| Construct | Elements | Status |
|---|---|---|
| Page size, margins, borders | `w:pgSz`, `w:pgMar`, `w:pgBorders` | API |
| Page borders around headers | `w:bordersDoNotSurroundHeader`, `w:bordersDoNotSurroundFooter` | Laid out; Model, 2.6 |
| Columns | `w:cols`, `w:col` | API for equal columns; presets and unequal widths are Model, 2.6 |
| Title page | `w:titlePg` | API |
| Odd and even headers | `w:evenAndOddHeaders` | Model (`setDifferentOddEvenPages` is unsupported), 2.6 |
| Section type | `w:type` | Model, 2.6 |
| Page numbering | `w:pgNumType` | Model, 2.6 |
| Line numbering | `w:lnNumType` | Model, 2.6 |
| Vertical alignment | `w:vAlign` | Model for sections, 2.6; API for cells |
| Mirror margins, book fold, gutter | `w:mirrorMargins`, `w:bookFoldPrinting`, `w:bookFoldRevPrinting`, `w:bookFoldPrintingSheets`, `w:gutterAtTop` | Model, 2.6 |
| Notes in a section | `w:footnotePr`, `w:endnotePr`, `w:pos`, `w:numStart`, `w:numRestart`, `w15:footnoteColumns` | Model, 2.7 |
| No endnotes | `w:noEndnote` | Laid out; Model, 2.7 |
| Form protection | `w:formProt` | Kept, 2.14 |
| Printer | `w:paperSrc`, `w:printerSettings` | Application |
| Document grid, gutter on the right | `w:docGrid`, `w:rtlGutter` | After 1.0 (line pitch laid out) |

## Tables

| Construct | Elements | Status |
|---|---|---|
| Tables, rows, cells | `w:tbl`, `w:tblPr`, `w:tblGrid`, `w:gridCol`, `w:tr`, `w:trPr`, `w:tc`, `w:tcPr` | API |
| Table properties | `w:tblStyle`, `w:tblW`, `w:tblInd`, `w:tblBorders`, `w:tblCellMar`, `w:tblLook`, `w:tblLayout`, `w:tblpPr`, `w:tblOverlap` | API |
| Cell spacing | `w:tblCellSpacing` | Model, 2.9 |
| Alt text | `w:tblCaption`, `w:tblDescription` | Kept, 2.9 |
| Row properties | `w:trHeight`, `w:cantSplit`, `w:tblHeader` | API |
| Row grid | `w:gridBefore`, `w:gridAfter`, `w:wBefore`, `w:wAfter` | Laid out; patched by column edits in 2.3 |
| Hidden rows | `w:hidden` | Laid out; Model, 2.9 |
| Row exceptions | `w:tblPrEx` | Laid out; Lost on new rows, 2.3; Model, 2.9 |
| Cell properties | `w:tcW`, `w:gridSpan`, `w:vMerge`, `w:tcBorders`, `w:insideH`, `w:insideV`, `w:tcMar` | API; percent widths are 2.9 |
| Horizontal merge (Word 97) | `w:hMerge` | Lost in rewritten rows, 2.3 |
| Diagonal borders | `w:tl2br`, `w:tr2bl` | Kept; Model, 2.9 (drawn in 3.6) |
| Cell text | `w:noWrap`, `w:tcFitText`, `w:hideMark` | Laid out; Model, 2.9 |
| Default table style | `w:defaultTableStyle` | Lost on new tables, 2.9 |
| Right-to-left tables | `w:bidiVisual` | After 1.0 |

## Notes and comments

| Construct | Elements | Status |
|---|---|---|
| Footnotes and endnotes | `w:footnote`, `w:endnote`, `w:footnoteReference`, `w:endnoteReference`, `w:footnoteRef`, `w:endnoteRef`, `w:separator`, `w:continuationSeparator` | API to insert; custom marks are Lost on rewrite (2.3), settings and conversion are 2.7 |
| Comments | `w:comment`, `w:commentRangeStart`, `w:commentRangeEnd`, `w:commentReference`, `w:annotationRef`, `w15:commentsEx`, `w15:commentEx`, `w16cid:commentsIds`, `w16cid:commentId`, `w16cex:commentsExtensible`, `w16cex:commentExtensible`, `w16cex:extLst`, `w16cur:ext` | API |
| People | `w15:people`, `w15:person`, `w15:presenceInfo` | API (authors) |

## Revisions

| Construct | Elements | Status |
|---|---|---|
| Insertions, deletions, moves | `w:ins`, `w:del`, `w:moveFrom`, `w:moveTo`, `w:moveFromRangeStart`, `w:moveFromRangeEnd`, `w:moveToRangeStart`, `w:moveToRangeEnd` | API |
| Property changes | `w:rPrChange`, `w:pPrChange`, `w:sectPrChange`, `w:tblPrChange`, `w:trPrChange`, `w:tcPrChange`, `w:tblPrExChange`, `w:tblGridChange`, `w:numberingChange` | API |
| Cell revisions | `w:cellIns`, `w:cellDel`, `w:cellMerge` | API |
| Tracking settings | `w:trackRevisions`, `w:doNotTrackMoves`, `w:doNotTrackFormatting`, `w:revisionView` | API |
| Revision save ids | `w:rsids`, `w:rsidRoot`, `w:rsid` | Application |

## Bookmarks and permissions

| Construct | Elements | Status |
|---|---|---|
| Bookmarks | `w:bookmarkStart`, `w:bookmarkEnd` | Kept; Lost when pasted or written from the model, 2.3; API, 2.8 |
| Editing permissions | `w:permStart`, `w:permEnd` | Kept as an opaque node; Model and API, 2.14 |

## Content controls

| Construct | Elements | Status |
|---|---|---|
| Controls | `w:sdt`, `w:sdtPr`, `w:sdtContent`, `w:sdtEndPr`, `w:alias`, `w:tag`, `w:id`, `w:lock`, `w:placeholder`, `w:docPart`, `w:showingPlcHdr` | API |
| Kinds | `w:text`, `w:richText`, `w:comboBox`, `w:dropDownList`, `w:listItem`, `w:date`, `w:dateFormat`, `w:storeMappedDataAs`, `w:calendar`, `w:picture`, `w:group`, `w14:checkbox`, `w14:checked`, `w14:checkedState`, `w14:uncheckedState`, `w15:repeatingSection`, `w15:repeatingSectionItem` | API; check box symbols are 2.13 |
| Building block galleries | `w:docPartObj`, `w:docPartList`, `w:docPartGallery`, `w:docPartCategory`, `w:docPartUnique` | Model; inserting from a gallery is 2.13 |
| Citations, bibliography, equations | `w:citation`, `w:bibliography`, `w:equation` | Kept as the control kind; their content is ordinary content |
| Data binding | `w:dataBinding`, `w15:dataBinding` | Model; evaluated both ways in 2.13 |
| Temporary | `w:temporary` | Kept, 2.13 |
| Appearance and colour | `w15:appearance`, `w15:color` | Kept, 2.13 |
| Repeating section options | `w15:sectionTitle`, `w15:doNotAllowInsertDeleteSection` | Kept, 2.13 |
| Entity picker | `w14:entityPicker` | Kept (SharePoint metadata) |
| Web extensions | `w15:webExtensionCreated`, `w15:webExtensionLinked` | Application |

## Custom XML markup

Word 2010 and later remove custom XML markup when they open a document, so it is kept but not put in the model.

| Construct | Elements | Status |
|---|---|---|
| Custom XML elements | `w:customXml`, `w:customXmlPr`, `w:attr` | Kept (content inside is read as ordinary content); Lost when cut and pasted, 2.3 |
| Custom XML revisions | `w:customXmlInsRangeStart`, `w:customXmlInsRangeEnd`, `w:customXmlDelRangeStart`, `w:customXmlDelRangeEnd`, `w:customXmlMoveFromRangeStart`, `w:customXmlMoveFromRangeEnd`, `w:customXmlMoveToRangeStart`, `w:customXmlMoveToRangeEnd`, `w14:customXmlConflictInsRangeStart`, `w14:customXmlConflictInsRangeEnd`, `w14:customXmlConflictDelRangeStart`, `w14:customXmlConflictDelRangeEnd`, `w14:conflictIns`, `w14:conflictDel`, `w14:conflictMode` | Kept as an opaque node |

## Embedded content

| Construct | Elements | Status |
|---|---|---|
| Drawings | `w:drawing`, `w:pict` | API (see [DrawingML and VML](#drawing)) |
| OLE objects | `w:object`, `w:objectEmbed`, `w:objectLink` | Preview picture API; the object is Lost when rewritten, opaque node in 2.2. Editing OLE objects is in the backlog |
| ActiveX controls | `w:control` | Kept, drawn from its preview picture; Lost when pasted, 2.3 |
| Imported content | `w:altChunk`, `w:altChunkPr`, `w:matchSrc` | Kept between unedited blocks; opaque block, 2.2. Drawing it is deferred with HTML paste |
| Subdocuments | `w:subDoc` | Kept as an opaque node, reported as unsupported content. Drawing them is in the backlog |
| Page background | `w:background`, `w:displayBackgroundShape` | Page colour in 3.7 |

## Building blocks

| Construct | Elements | Status |
|---|---|---|
| Glossary entries | `w:docParts`, `w:docPartPr`, `w:docPartBody`, `w:category`, `w:gallery`, `w:behaviors`, `w:behavior`, `w:description`, `w:types`, `w:guid` | Model (placeholders and sub-templates); inserting one is 2.13 |

## Document settings

| Construct | Elements | Status |
|---|---|---|
| Tabs and hyphenation | `w:defaultTabStop`, `w:autoHyphenation`, `w:consecutiveHyphenLimit`, `w:hyphenationZone`, `w:doNotHyphenateCaps` | API |
| Protection | `w:documentProtection`, `w:writeProtection`, `w:autoFormatOverride`, `w:styleLockTheme`, `w:styleLockQFSet` | Model (tracking and formatting enforced, never written), 2.14 |
| Compatibility | `w:compat`, `w:compatSetting` and every option in the [compatibility guide](compatibility) (`w:adjustLineHeightInTable`, `w:alignTablesRowByRow`, `w:allowSpaceOfSameStyleInTable`, `w:applyBreakingRules`, `w:autofitToFirstFixedWidthCell`, `w:autoSpaceLikeWord95`, `w:balanceSingleByteDoubleByteWidth`, `w:cachedColBalance`, `w:convMailMergeEsc`, `w:displayHangulFixedWidth`, `w:doNotAutofitConstrainedTables`, `w:doNotBreakConstrainedForcedTable`, `w:doNotBreakWrappedTables`, `w:doNotExpandShiftReturn`, `w:doNotLeaveBackslashAlone`, `w:doNotSnapToGridInCell`, `w:doNotSuppressIndentation`, `w:doNotSuppressParagraphBorders`, `w:doNotUseEastAsianBreakRules`, `w:doNotUseHTMLParagraphAutoSpacing`, `w:doNotUseIndentAsNumberingTabStop`, `w:doNotVertAlignCellWithSp`, `w:doNotVertAlignInTxbx`, `w:doNotWrapTextWithPunct`, `w:footnoteLayoutLikeWW8`, `w:forgetLastTabAlignment`, `w:growAutofit`, `w:layoutRawTableWidth`, `w:layoutTableRowsApart`, `w:lineWrapLikeWord6`, `w:mwSmallCaps`, `w:noColumnBalance`, `w:noExtraLineSpacing`, `w:noLeading`, `w:noSpaceRaiseLower`, `w:noTabHangInd`, `w:printBodyTextBeforeHeader`, `w:printColBlack`, `w:selectFldWithFirstOrLastChar`, `w:shapeLayoutLikeWW8`, `w:showBreaksInFrames`, `w:spaceForUL`, `w:spacingInWholePoints`, `w:splitPgBreakAndParaMark`, `w:subFontBySize`, `w:suppressBottomSpacing`, `w:suppressSpacingAtTopOfPage`, `w:suppressSpBfAfterPgBrk`, `w:suppressTopSpacing`, `w:suppressTopSpacingWP`, `w:swapBordersFacingPages`, `w:truncateFontHeightsLikeWP6`, `w:ulTrailSpace`, `w:underlineTabInNumList`, `w:useAltKinsokuLineBreakRules`, `w:useAnsiKerningPairs`, `w:useFELayout`, `w:useNormalStyleForList`, `w:usePrinterMetrics`, `w:useSingleBorderforContiguousCells`, `w:useWord2002TableStyleRules`, `w:useWord97LineBreakRules`, `w:wpJustification`, `w:wpSpaceWidth`, `w:wrapTrailSpaces`, `w:uiCompat97To2003`) | Laid out |
| Theme colour mapping | `w:clrSchemeMapping` | Laid out |
| Number separators | `w:decimalSymbol`, `w:listSeparator` | Application (the author's locale; read by `=` fields in 2.11) |
| East Asian settings | `w:noPunctuationKerning`, `w:characterSpacingControl`, `w:strictFirstAndLastChars`, `w:noLineBreaksBefore`, `w:noLineBreaksAfter`, `w:themeFontLang` | After 1.0 |
| Alignment of borders and edges | `w:alignBordersAndEdges` | Kept (backlog) |
| View and proofing | `w:view`, `w:zoom`, `w:doNotDisplayPageBoundaries`, `w:hideSpellingErrors`, `w:hideGrammaticalErrors`, `w:activeWritingStyle`, `w:proofState`, `w:formsDesign`, `w:stylePaneFormatFilter`, `w:stylePaneSortMethod`, `w:clickAndTypeStyle`, `w:readModeInkLockDown`, `w:showEnvelope`, `w:showXMLTags`, `w:alwaysShowPlaceholderText` | Application |
| Saving and privacy | `w:removePersonalInformation`, `w:removeDateAndTime`, `w:embedTrueTypeFonts`, `w:embedSystemFonts`, `w:saveSubsetFonts`, `w:saveFormsData`, `w:savePreviewPicture`, `w:doNotAutoCompressPictures`, `w:doNotIncludeSubdocsInStats`, `w:forceUpgrade`, `w:summaryLength`, `w:updateFields` | Application |
| Printing | `w:printPostScriptOverText`, `w:printFractionalCharacterWidth`, `w:printFormsData`, `w:printTwoOnOne` | Application |
| Templates and XML | `w:attachedTemplate`, `w:linkStyles`, `w:documentType`, `w:attachedSchema`, `w:doNotValidateAgainstSchema`, `w:saveInvalidXml`, `w:ignoreMixedContent`, `w:doNotDemarcateInvalidXml`, `w:saveXmlDataOnly`, `w:useXSLTWhenSaving`, `w:saveThroughXslt`, `w:alwaysMergeEmptyNamespace` | Application |
| Drawing grid | `w:drawingGridHorizontalSpacing`, `w:drawingGridVerticalSpacing`, `w:displayHorizontalDrawingGridEvery`, `w:displayVerticalDrawingGridEvery`, `w:doNotUseMarginsForDrawingGridOrigin`, `w:drawingGridHorizontalOrigin`, `w:drawingGridVerticalOrigin` | Application |
| Shape defaults | `w:shapeDefaults`, `w:hdrShapeDefaults` | Application |
| Captions | `w:captions`, `w:caption`, `w:autoCaptions`, `w:autoCaption` | Application (Insert Caption is 4.1) |
| Document ids and options | `w14:docId`, `w15:docId`, `w14:defaultImageDpi`, `w14:discardImageEditingData`, `w15:chartTrackingRefBased`, `w14:extLst` | Application |
| Mail merge | `w:mailMerge`, `w:mainDocumentType`, `w:linkToQuery`, `w:dataType`, `w:connectString`, `w:query`, `w:dataSource`, `w:headerSource`, `w:doNotSuppressBlankLines`, `w:destination`, `w:addressFieldName`, `w:mailSubject`, `w:mailAsAttachment`, `w:viewMergedData`, `w:activeRecord`, `w:checkErrors`, `w:odso`, `w:udl`, `w:table`, `w:src`, `w:colDelim`, `w:fHdr`, `w:fieldMapData`, `w:column`, `w:mappedName`, `w:dynamicAddress`, `w:recipients`, `w:recipientData`, `w:active`, `w:uniqueTag` | Application |

## Fonts and web page settings

| Construct | Elements | Status |
|---|---|---|
| Font table | `w:font`, `w:altName`, `w:panose1`, `w:charset`, `w:family`, `w:pitch`, `w:sig`, `w:notTrueType` | Laid out (font categories for fallback) |
| Embedded fonts | `w:embedRegular`, `w:embedBold`, `w:embedItalic`, `w:embedBoldItalic` | Application (embedded fonts aren't planned: the host supplies fonts) |
| Web page options | `w:optimizeForBrowser`, `w:relyOnVML`, `w:allowPNG`, `w:doNotRelyOnCSS`, `w:doNotSaveAsSingleFile`, `w:doNotOrganizeInFolder`, `w:doNotUseLongFileNames`, `w:pixelsPerInch`, `w:targetScreenSz`, `w:encoding` | Application |
| Framesets | `w:frameset`, `w:frame`, `w:framesetSplitbar`, `w:frameLayout`, `w:scrollbar`, `w:noResizeAllowed`, `w:linkedToFile`, `w:noBorder`, `w:flatBorders`, `w:marW`, `w:marH`, `w:sz`, `w:sourceFileName` | Application (web page layout) |

## Document properties

| Construct | Part | Status |
|---|---|---|
| Core, extended and custom properties | `docProps/core.xml`, `docProps/app.xml`, `docProps/custom.xml` | Kept; Model and API, and updated on save, 2.11 |

## DrawingML and VML {#drawing}

| Feature | Status |
|---|---|
| Inline and floating pictures, shapes, text boxes, groups, canvases | API |
| Crop (`a:srcRect`) | Laid out; Lost when a picture is rewritten, Model and API, 2.10 |
| Linked pictures (`r:link`) | Lost (drawn empty), 2.10 |
| Set transparent colour (`a:clrChange`) | Kept, 2.10 |
| Relative size and position (`wp14:sizeRelH`, `wp14:pctPosHOffset`, ...) | Laid out; Model, 2.10 |
| Text box columns and autofit (`wps:bodyPr@numCol`, `a:normAutofit`) | Model; columns API in 2.10 |
| A shape's VML fallback (`mc:Fallback`) | Lost when the shape is edited, 2.3 |
| Alternate content (`mc:AlternateContent`, `Requires`) | Laid out: the first choice whose namespaces the engine reads, else the fallback |
| Charts (`c:chart`) | Not in 1.0 |
| SmartArt, WordArt | Drawn from their fallback; editing in the backlog |
