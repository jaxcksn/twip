import Foundation

// Commands from crates/twip/src/api/command.rs: one case per operation an application runs at the selection, with
// `try await session.run(.setBold(true))`. Their option types and enums are in CommandOptions.swift.
//
// Renames from Rust: `CommandId` -> `CommandID`; `Alignment` -> `ParagraphAlignment`; `HorizontalAlignment` ->
// `HorizontalAlign`; `Color` -> `RGBColor`; `Bytes` -> `Data`; `Degrees` -> `Double`; review's `Body` -> `TextBody`;
// `*Id` -> `*ID`. A command whose Rust options struct has at most five fields (and no nullable patch field) takes them
// as labeled associated values instead, so those structs (`MoveSelection`, `DeleteOptions`, `TableSize`, ...) have no
// Swift type; `insertSymbol`'s `char` is labeled `character`.

/// A command. Each case's documentation states its undo grouping: one step, typing (consecutive typing in one
/// paragraph joins one step), or view (no undo step and no revision).
///
/// On the wire a command is an object with its id as `type`: a single argument travels in `value`
/// (`{"type":"setBold","value":true}`) and options sit beside `type`
/// (`{"type":"insertTable","rows":3,"columns":2}`). Unset optional fields keep their current value.
public enum Command: Hashable, Sendable, Codable {
    /// Types `value` (non-empty) at the selection, replacing it. Undo: typing.
    case insertText(String)
    /// Starts a new paragraph (Enter). Undo: one step.
    case insertParagraph
    /// A line break (Shift+Enter). Undo: one step.
    case insertLineBreak
    /// A page break, in the body outside tables. Undo: one step.
    case insertPageBreak
    /// A column break, in the body outside tables. Undo: one step.
    case insertColumnBreak
    /// A section break of the given kind, in the body outside tables. Undo: one step.
    case insertSectionBreak(SectionBreakKind)
    /// A tab character. Undo: typing.
    case insertTab
    /// One character, optionally in a symbol font. Undo: one step.
    ///
    /// - Parameters:
    ///   - character: The character to insert.
    ///   - font: The symbol font (Symbol, Wingdings); `nil` keeps the font at the selection.
    case insertSymbol(character: String, font: String? = nil)
    /// Deletes backward or forward by a character or a word, or the selection. Undo: typing.
    ///
    /// - Parameters:
    ///   - direction: Backward (Backspace) or forward (Delete).
    ///   - unit: Default: character.
    case delete(direction: Direction, unit: DeleteUnit = .character)
    /// Cuts the selection to the clipboard. Undo: one step.
    case cut
    /// Copies the selection (also when read only). No undo step.
    case copy
    /// Pastes the system clipboard, formatted or as text. Undo: one step.
    ///
    /// - Parameters:
    ///   - as: Default: formatted.
    case paste(as: PasteAs = .formatted)
    /// Pastes `value` as plain text. Undo: one step.
    case pasteText(String)
    /// Undoes the last step. State value: the undo label.
    case undo
    /// Redoes the last undone step. State value: the redo label.
    case redo
    /// Redoes, or repeats the last repeatable action (F4). Undo: one step.
    case `repeat`
    /// Selects the whole story. Undo: view.
    case selectAll
    /// Selects the unit at the caret. Undo: view.
    case select(SelectUnit)
    /// Moves or extends the selection. Undo: view.
    ///
    /// - Parameters:
    ///   - by: The unit to move by.
    ///   - direction: Which way to move.
    ///   - extend: Extends the selection instead of moving the caret. Default: false.
    case moveSelection(by: MoveUnit, direction: Direction, extend: Bool = false)
    /// Moves to the next or previous cell; `next` from the last cell adds a row (then one step).
    case moveToCell(Order)
    /// Normal, extend (F8) or block (Ctrl+Shift+F8) selection. Undo: view.
    case setSelectionMode(SelectionMode)
    /// Back to an earlier edit location (Shift+F5). Undo: view.
    case goBack
    /// Leaves a header or footer, a text box, or object selection (Esc). Undo: view.
    case escape
    /// Selects the next or previous floating object. Undo: view.
    case selectNextObject(Order)
    /// Moves the caret to the start of page `value` (1…count). Undo: view.
    case goToPage(Int)
    /// Bold on or off. Undo: one step.
    case setBold(Bool)
    /// Italic on or off. Undo: one step.
    case setItalic(Bool)
    /// The underline style (`none` removes it). Undo: one step.
    case setUnderline(UnderlineStyle)
    /// Strikethrough on or off. Undo: one step.
    case setStrikethrough(Bool)
    /// Double strikethrough on or off. Undo: one step.
    case setDoubleStrikethrough(Bool)
    /// Subscript on or off. Undo: one step.
    case setSubscript(Bool)
    /// Superscript on or off. Undo: one step.
    case setSuperscript(Bool)
    /// Small capitals on or off. Undo: one step.
    case setSmallCaps(Bool)
    /// All capitals on or off. Undo: one step.
    case setAllCaps(Bool)
    /// Hidden text on or off. Undo: one step.
    case setHidden(Bool)
    /// The font family (non-empty). Undo: one step.
    case setFont(String)
    /// The font size (1…1638 pt, in 0.5 steps). Undo: one step.
    case setFontSize(Points)
    /// The next size up in the standard list. Undo: one step.
    case growFont
    /// The next size down in the standard list. Undo: one step.
    case shrinkFont
    /// The font colour, or automatic. Undo: one step.
    case setFontColor(AutoOr<RGBColor>)
    /// The highlight colour, or none (`null`). Undo: one step.
    case setHighlight(HighlightColor?)
    /// Character spacing, expanded when positive (−1584…1584 pt). Undo: one step.
    case setCharacterSpacing(Points)
    /// The raised (positive) or lowered position (−1584…1584 pt). Undo: one step.
    case setCharacterPosition(Points)
    /// Removes direct character formatting. Undo: one step.
    case clearFormatting
    /// Changes the case of the selected text. Undo: one step.
    case changeCase(CaseChange)
    /// Holds the selection's formatting for `pasteFormatting` (the format painter). Undo: view.
    case copyFormatting
    /// Applies the held formatting to the selection. Undo: one step.
    case pasteFormatting
    /// The selected paragraphs' alignment. Undo: one step.
    case setAlignment(ParagraphAlignment)
    /// The selected paragraphs' line spacing. Undo: one step.
    case setLineSpacing(LineSpacing)
    /// Space before and after the selected paragraphs (0…1584 pt, or auto). Undo: one step.
    ///
    /// - Parameters:
    ///   - before: Space before, or auto.
    ///   - after: Space after, or auto.
    case setParagraphSpacing(before: AutoOr<Points>? = nil, after: AutoOr<Points>? = nil)
    /// The selected paragraphs' indents. Undo: one step.
    ///
    /// - Parameters:
    ///   - left: The left indent.
    ///   - right: The right indent.
    ///   - firstLine: The first line's extra indent.
    ///   - hanging: The hanging indent of the lines after the first.
    case setIndents(left: Points? = nil, right: Points? = nil, firstLine: Points? = nil, hanging: Points? = nil)
    /// Indents the selected paragraphs to the next stop. Undo: one step.
    case increaseIndent
    /// Outdents the selected paragraphs to the previous stop. Undo: one step.
    case decreaseIndent
    /// An existing paragraph style. Undo: one step.
    case setParagraphStyle(StyleID)
    /// Changes a style to the selection's formatting: its paragraph's and its first run's direct formatting become the
    /// style's. Undo: one step.
    case updateStyleToSelection(StyleID)
    /// Deletes a style the document made: its paragraphs take the default paragraph style, its runs lose it, and the
    /// styles based on it take its base. Built-in styles can't be deleted. Undo: one step.
    case deleteStyle(StyleID)
    /// Sets a style's list flags: visibility, priority, gallery, lock. Undo: one step.
    ///
    /// - Parameters:
    ///   - id: The style.
    ///   - visibility: Whether it shows in style lists.
    ///   - priority: Its sort priority (`w:uiPriority`).
    ///   - quick: Shown in the Styles gallery.
    ///   - locked: Restricted while formatting is protected.
    case setStyleMeta(id: StyleID, visibility: StyleVisibility? = nil, priority: Int? = nil, quick: Bool? = nil, locked: Bool? = nil)
    /// Sets the document's default formatting (Manage Styles' Set Defaults). Undo: one step.
    case setDocumentDefaults(DocumentDefaults)
    /// Bullets, numbering, or no list (`null`). Undo: one step.
    case setList(ListKind?)
    /// One list level deeper or shallower, in a list. Undo: one step.
    case setListLevel(LevelChange)
    /// Defines the selection's list levels (Define New Bullet, Number Format, Multilevel List): for its whole list,
    /// from this paragraph on, or for the selected paragraphs. Outside a list, makes a new one. Undo: one step.
    ///
    /// - Parameters:
    ///   - levels: The levels, from the first.
    ///   - applyTo: What they apply to.
    ///   - name: The list's name (the ListNum field list name); none keeps it.
    case setListDefinition(levels: [ListLevel], applyTo: ListApplyTo = .wholeList, name: String? = nil)
    /// Set Numbering Value: starts a new list at a value, or continues the previous list. Undo: one step.
    ///
    /// - Parameters:
    ///   - mode: Start a new list, or continue the previous one.
    ///   - value: The value to start at (a new list), or to skip to (continuing, with `advance`).
    ///   - advance: Continuing: skip numbers to `value`.
    case setNumberingValue(mode: NumberingMode, value: Int, advance: Bool = false)
    /// Applies a list gallery entry (`listGalleries`): a library or recent entry defines the selection's list (the
    /// current level for bullets and numbering, every level for a multilevel list), or makes a new one; a document
    /// list or list style continues that list. Undo: one step.
    case applyListEntry(ListEntryID)
    /// Restarts the list at this paragraph, at its level's start. Undo: one step.
    case restartNumbering
    /// Continues the previous list from this paragraph. Undo: one step.
    case continueNumbering
    /// Adjust List Indents: the selection's list level's number position, text indent and what follows the number,
    /// for its whole list. Undo: one step.
    ///
    /// - Parameters:
    ///   - numberPosition: Where the number is, from the margin.
    ///   - textIndent: Where the text is, from the margin.
    ///   - follow: What follows the number.
    ///   - tab: The tab stop after the number; none uses the text indent.
    case adjustListIndents(numberPosition: Points, textIndent: Points, follow: FollowNumber, tab: Points? = nil)
    /// Keep with next, keep lines together, page break before and widow control. Undo: one step.
    ///
    /// - Parameters:
    ///   - keepWithNext: Keeps the paragraph on the same page as the next one.
    ///   - keepLinesTogether: Keeps the paragraph's lines on one page.
    ///   - pageBreakBefore: Starts the paragraph on a new page.
    ///   - widowControl: Prevents a single first or last line on a page.
    case setParagraphFlow(keepWithNext: Bool? = nil, keepLinesTogether: Bool? = nil, pageBreakBefore: Bool? = nil, widowControl: Bool? = nil)
    /// A box, a bottom border or none (a ½ pt single line). Undo: one step.
    case setParagraphBorders(BorderPreset)
    /// Paragraph shading, or none (`null`). Undo: one step.
    case setParagraphShading(RGBColor?)
    /// The selected paragraphs' border lines: each edge set, removed (`null`) or kept (absent). Undo: one step.
    case setParagraphBorderLines(ParagraphBorderLines)
    /// Shading of the selected paragraphs or text: its fill, pattern and pattern color. Undo: one step.
    ///
    /// - Parameters:
    ///   - target: The paragraphs or the text.
    ///   - fill: The fill, or none (no shading at all, with a clear pattern).
    ///   - pattern: The pattern.
    ///   - color: The pattern's color.
    case setShading(target: ShadingTarget, fill: NoneOr<RGBColor>, pattern: ShadingPattern = .clear, color: AutoOr<RGBColor> = .auto)
    /// The selected text's border: a box of one line, with a shadow or not, or none. Undo: one step.
    ///
    /// - Parameters:
    ///   - border: The line, or none.
    ///   - shadow: A shadow.
    case setTextBorder(border: Border?, shadow: Bool = false)
    /// The page borders of the selection's section, or of every section: each edge set, removed (`null`) or kept
    /// (absent), what they're measured from, and the pages they show on. Undo: one step.
    case setPageBorders(PageBordersChange)
    /// The frame the selection is in: its wrapping, size, position and distance from text. Undo: one step.
    case setFrame(FrameProperties)
    /// Removes the frame the selection is in: its paragraphs go back in the text, before the paragraph it was
    /// anchored to. Undo: one step.
    case removeFrame
    /// Adds a tab stop to the selected paragraphs, replacing one at the same position. Undo: one step.
    ///
    /// - Parameters:
    ///   - position: From the paragraph's left text edge (0…1584 pt).
    ///   - alignment: Default: left.
    ///   - leader: Default: none.
    case addTabStop(position: Points, alignment: TabAlignment? = nil, leader: TabLeader? = nil)
    /// Moves the stop at `from` (within 0.5 pt) to `to`. Undo: one step.
    ///
    /// - Parameters:
    ///   - from: The stop's current position.
    ///   - to: Its new position (0…1584 pt).
    case moveTabStop(from: Points, to: Points)
    /// Deletes the stop at `value`; an inherited stop is cleared in the paragraph. Undo: one step.
    case deleteTabStop(Points)
    /// Clears the paragraphs' own and inherited stops. Undo: one step.
    case clearTabStops
    /// The document's default tab spacing (1…1584 pt). Undo: one step.
    case setDefaultTabStop(Points)
    /// The selection's sections' margins (0…1584 pt; left + right + gutter < page width). Undo: one step.
    ///
    /// - Parameters:
    ///   - top: The top margin.
    ///   - bottom: The bottom margin.
    ///   - left: The left margin.
    ///   - right: The right margin.
    ///   - gutter: The gutter, added on the binding side.
    case setMargins(top: Points? = nil, bottom: Points? = nil, left: Points? = nil, right: Points? = nil, gutter: Points? = nil)
    /// Portrait or landscape. Undo: one step.
    case setOrientation(Orientation)
    /// The paper size, in portrait (36…1584 pt). Undo: one step.
    ///
    /// - Parameters:
    ///   - width: The paper width.
    ///   - height: The paper height.
    case setPaperSize(width: Points, height: Points)
    /// Text columns (1…45), their spacing and separator. Undo: one step.
    ///
    /// - Parameters:
    ///   - count: The number of text columns (1…45).
    ///   - spacing: The space between columns.
    ///   - separator: Draws a line between columns.
    case setColumns(count: Int, spacing: Points? = nil, separator: Bool? = nil)
    /// The header and footer distances from the page edge. Undo: one step.
    ///
    /// - Parameters:
    ///   - header: The header's distance from the top edge.
    ///   - footer: The footer's distance from the bottom edge.
    case setHeaderFooterDistance(header: Points? = nil, footer: Points? = nil)
    /// A different first page header and footer. Undo: one step.
    case setDifferentFirstPage(Bool)
    /// Different odd and even page headers and footers. Undo: one step.
    case setDifferentOddEvenPages(Bool)
    /// Moves a text column boundary (from the ruler). Undo: one step.
    ///
    /// - Parameters:
    ///   - column: The text column whose right boundary moves, from 0.
    ///   - to: The boundary's new position.
    case moveColumnBoundary(column: Int, to: Points)
    /// Edits the caret's page's header, creating it when the page has none. Undo: one step when it creates one.
    case editHeader
    /// Edits the caret's page's footer, creating it when the page has none. Undo: one step when it creates one.
    case editFooter
    /// Back to the body. Undo: view.
    case closeHeaderFooter
    /// A page number field at the caret, or in the header or footer. Undo: one step.
    ///
    /// - Parameters:
    ///   - format: What the field shows.
    ///   - where: Default: caret.
    ///   - alignment: The alignment in the header or footer; `nil` keeps the paragraph's.
    case insertPageNumber(format: PageNumberFormat, where: PageNumberPlace = .caret, alignment: HorizontalAlign? = nil)
    /// A table (1…32767 rows, 1…63 columns). Undo: one step.
    ///
    /// - Parameters:
    ///   - rows: The number of rows.
    ///   - columns: The number of columns.
    case insertTable(rows: Int, columns: Int)
    /// A picture (PNG, JPEG, GIF, BMP, TIFF). Undo: one step.
    ///
    /// - Parameters:
    ///   - data: The image file's bytes.
    ///   - altText: The alternative text.
    case insertPicture(data: Data, altText: String? = nil)
    /// A text box. Undo: one step.
    case insertTextBox
    /// A floating shape at the caret's paragraph, in front of the text, with the theme's default style. Undo: one step.
    case insertShape(ShapeInsert)
    /// A footnote at the caret. Undo: one step.
    case insertFootnote
    /// An endnote at the caret. Undo: one step.
    case insertEndnote
    /// A hyperlink to `address`; `text` replaces the selection (default: the selection, or the address). Undo: one step.
    ///
    /// - Parameters:
    ///   - address: The link's address (a URL or `#bookmark`).
    ///   - text: The text to show; `nil` uses the selection, or the address when collapsed.
    case insertHyperlink(address: String, text: String? = nil)
    /// Changes the link at the selection. Undo: one step.
    ///
    /// - Parameters:
    ///   - address: The new address.
    ///   - text: The new display text.
    case editHyperlink(address: String? = nil, text: String? = nil)
    /// Removes the link at the selection, keeping its text. Undo: one step.
    case removeHyperlink
    /// A field with `code`; result `{ id }`. With a selection and an empty code, wraps it. Undo: one step.
    ///
    /// - Parameters:
    ///   - code: The field code (`PAGE`, `DATE \@ "d MMMM yyyy"`).
    ///   - display: The result shown until the field updates.
    ///   - showCode: Shows the code instead of the result.
    case insertField(code: String, display: String? = nil, showCode: Bool? = nil)
    /// Rows above or below the selection (1…32767). Undo: one step.
    ///
    /// - Parameters:
    ///   - where: Above or below the selected rows.
    ///   - count: Default: 1.
    case insertRows(where: VerticalSide, count: Int = 1)
    /// Columns left or right of the selection (1…63). Undo: one step.
    ///
    /// - Parameters:
    ///   - where: Left or right of the selected columns.
    ///   - count: Default: 1.
    case insertColumns(where: HorizontalSide, count: Int = 1)
    /// Deletes the selected rows. Undo: one step.
    case deleteRows
    /// Deletes the selected columns. Undo: one step.
    case deleteColumns
    /// Deletes the table. Undo: one step.
    case deleteTable
    /// Deletes the selected cells, shifting the rest up or left. Undo: one step.
    case deleteCells(CellShift)
    /// Merges a rectangular block of cells. Undo: one step.
    case mergeCells
    /// Splits the selected cells (1…63 columns, 1…32767 rows). Undo: one step.
    ///
    /// - Parameters:
    ///   - columns: The number of columns to split each cell into.
    ///   - rows: Default: 1.
    case splitCells(columns: Int, rows: Int = 1)
    /// Splits the table above the selection. Undo: one step.
    case splitTable
    /// Gives the selected rows equal heights. Undo: one step.
    case distributeRows
    /// Gives the selected columns equal widths. Undo: one step.
    case distributeColumns
    /// Moves the selected rows up or down. Undo: one step.
    case moveRows(VerticalDirection)
    /// Fits the columns to their contents, the window, or fixed widths. Undo: one step.
    case autoFitColumns(AutoFit)
    /// The selected cells' vertical alignment. Undo: one step.
    case setCellAlignment(CellAlignment)
    /// Cell shading, or none (`null`). Undo: one step.
    case setCellShading(RGBColor?)
    /// Borders on the selected cells' edges. Undo: one step.
    ///
    /// - Parameters:
    ///   - edges: Which edges of the selected cells.
    ///   - border: Default: a ½ pt single automatic line.
    case setCellBorders(edges: BorderEdges, border: Border? = nil)
    /// The selected cells' margins (0…1584 pt). Undo: one step.
    ///
    /// - Parameters:
    ///   - top: The top margin.
    ///   - bottom: The bottom margin.
    ///   - left: The left margin.
    ///   - right: The right margin.
    case setCellMargins(top: Points? = nil, bottom: Points? = nil, left: Points? = nil, right: Points? = nil)
    /// Repeat the selected rows as header rows. Undo: one step.
    case setHeaderRow(Bool)
    /// The selected rows' height, or automatic. Undo: one step.
    case setRowHeight(AutoOr<RowHeight>)
    /// Whether the selected rows can break across pages. Undo: one step.
    case setRowCanBreak(Bool)
    /// The selected columns' width (1…1584 pt). Undo: one step.
    case setColumnWidth(Points)
    /// Moves a column boundary of the caret's row (from the rulers and cell edges). Undo: one step.
    ///
    /// - Parameters:
    ///   - index: The boundary's index, from 0.
    ///   - to: The boundary's new position.
    case moveTableColumnBoundary(index: Int, to: Points)
    /// Moves a row boundary of the caret's table. Undo: one step.
    ///
    /// - Parameters:
    ///   - index: The boundary's index, from 0.
    ///   - to: The boundary's new position.
    case moveTableRowBoundary(index: Int, to: Points)
    /// The table's preferred width. Undo: one step.
    case setTableWidth(TableWidth)
    /// The table's alignment on the page. Undo: one step.
    case setTableAlignment(HorizontalAlign)
    /// The table's indent (−1584…1584 pt). Undo: one step.
    case setTableIndent(Points)
    /// An existing table style. Undo: one step.
    case setTableStyle(StyleID)
    /// Which parts of the table style apply. Undo: one step.
    case setTableStyleOptions(TableStyleChange)
    /// Where the table is placed, and what it is measured from: a table without a position becomes positioned (text
    /// wraps around it); none (`null`) puts it back in the text. Undo: one step.
    case setTablePosition(TablePosition?)
    /// The selected object's size (1…1584 pt). Undo: one step.
    ///
    /// - Parameters:
    ///   - width: The width.
    ///   - height: The height.
    ///   - lockAspect: Keeps the aspect ratio when one side is set.
    case setObjectSize(width: Points? = nil, height: Points? = nil, lockAspect: Bool? = nil)
    /// The selected picture at its original size. Undo: one step.
    case resetObjectSize
    /// How text wraps around the selected object. Undo: one step.
    case setObjectWrap(ObjectWrap)
    /// Locks a floating object's anchor. Undo: one step.
    case setAnchorLocked(Bool)
    /// Where the selected floating object is, and what it is measured from. Undo: one step.
    ///
    /// - Parameters:
    ///   - horizontal: The horizontal place.
    ///   - vertical: The vertical place.
    ///   - allowOverlap: Other floating objects may overlap it.
    ///   - layoutInCell: In a table cell, it is placed and kept within the cell.
    case setObjectPosition(horizontal: HorizontalPosition? = nil, vertical: VerticalPosition? = nil, allowOverlap: Bool? = nil, layoutInCell: Bool? = nil)
    /// The sides text wraps on around the selected floating object, and its distance from the text. Undo: one step.
    ///
    /// - Parameters:
    ///   - side: The sides text wraps on.
    ///   - top: The distance from the text above.
    ///   - bottom: The distance from the text below.
    ///   - left: The distance from the text on the left.
    ///   - right: The distance from the text on the right.
    case setObjectWrapOptions(side: WrapSide? = nil, top: Points? = nil, bottom: Points? = nil, left: Points? = nil, right: Points? = nil)
    /// The selected object's alternative text. Undo: one step.
    ///
    /// - Parameters:
    ///   - text: The alternative text.
    ///   - decorative: Marks the object decorative (no alternative text).
    case setAltText(text: String, decorative: Bool? = nil)
    /// Deletes the selected object. Undo: one step.
    case deleteObject
    /// The selected shape's preset geometry; its text, fill, outline and effects stay and its adjust values reset.
    /// Undo: one step.
    case setShapePreset(ShapePreset)
    /// One of the selected shape's adjust values. Undo: one step.
    case setShapeAdjust(ShapeAdjust)
    /// The selected objects' fill (`fromStyle`: the style reference's again). Undo: one step.
    case setShapeFill(FillValue)
    /// The selected objects' outline; what isn't given is kept. Undo: one step.
    case setShapeLine(LineChange)
    /// The selected objects' shadows, glow, soft edge and blur; what isn't given is kept, `null` removes. Undo: one
    /// step.
    case setShapeEffects(EffectsChange)
    /// The selected shape's text insets, anchor, autofit, wrapping and direction. Undo: one step.
    case setShapeText(ShapeTextChange)
    /// The selected picture's transparency, greyscale, brightness and contrast. Undo: one step.
    case setPictureAdjust(PictureAdjustChange)
    /// The selected objects' rotation (0…360°, clockwise). Undo: one step.
    case setObjectRotation(Double)
    /// Flips the selected objects. Undo: one step.
    case flipObject(FlipAxis)
    /// Moves the selected floating object in the paint order, or behind or in front of the text. Undo: one step.
    case arrangeObject(Arrange)
    /// Groups the selected floating objects (two or more, one story). Undo: one step.
    case groupObjects
    /// Ungroups the selected group. Undo: one step.
    case ungroupObjects
    /// Selects the next or previous child of the selected group. Undo: view.
    case selectGroupChild(Order)
    /// Changes a field's code or result. Undo: one step.
    ///
    /// - Parameters:
    ///   - id: The field.
    ///   - code: The new field code.
    ///   - display: The new display text.
    case setField(id: ElementID, code: String? = nil, display: TextBody? = nil)
    /// The results the application computed; `token` joins the update a `fieldsUpdateRequested` started. Undo: one step.
    ///
    /// - Parameters:
    ///   - results: The results, one per field.
    ///   - token: The token of the `fieldsUpdateRequested` event whose step these results join.
    case setFieldResults(results: [FieldResult], token: Int? = nil)
    /// Updates fields (default: the selection's), asking the application for the ones it computes. Undo: one step.
    ///
    /// - Parameters:
    ///   - ids: The elements; `nil` means the selection's.
    case updateFields(ids: [ElementID]? = nil)
    /// Locks or unlocks fields. Undo: one step.
    ///
    /// - Parameters:
    ///   - locked: Locks (`true`) or unlocks the fields.
    ///   - ids: The fields; `nil` means the selection's.
    case setFieldsLocked(locked: Bool, ids: [ElementID]? = nil)
    /// Replaces fields with their results. Undo: one step.
    ///
    /// - Parameters:
    ///   - ids: The elements; `nil` means the selection's.
    case unlinkFields(ids: [ElementID]? = nil)
    /// Shows field codes or results. Undo: view.
    ///
    /// - Parameters:
    ///   - show: Shows codes (`true`) or results.
    ///   - ids: The fields; `nil` means the selection's.
    case showFieldCodes(show: Bool, ids: [ElementID]? = nil)
    /// What design mode shows for each field code. Undo: view.
    ///
    /// - Parameters:
    ///   - labels: The label design mode shows for each field code.
    ///   - merge: Keeps labels for codes not listed. Default: false.
    case setFieldLabels(labels: [String: String], merge: Bool = false)
    /// Which fields are sub-template references: replaces the rules; an empty list turns sub-templates off. Undo: view.
    ///
    /// - Parameters:
    ///   - rules: The rules, replacing those set before.
    case setSubTemplateRules(rules: [ReferenceRule])
    /// The application's own field types (compared without case), replacing those set before: their fields keep their
    /// results, show their labels in design mode and are never read as bookmark references. Undo: view.
    ///
    /// - Parameters:
    ///   - types: The field types (a code's first word), replacing those set before; an empty list clears them.
    case setApplicationFieldTypes(types: [String])
    /// Shows references' sub-template bodies in their place, or their fields (default: every reference). Undo: view.
    ///
    /// - Parameters:
    ///   - ids: The reference fields; `nil` means every reference.
    ///   - expanded: Their bodies show in their place (else their fields do).
    case setSubTemplateView(ids: [ElementID]? = nil, expanded: Bool)
    /// Edits a sub-template's body in this editor, on its own pages; the template shows again on Close Sub-template.
    /// Undo: view.
    ///
    /// - Parameters:
    ///   - name: Its name.
    ///   - category: Its building block category; default: the rules' categories, in order.
    case editSubTemplate(name: String, category: String? = nil)
    /// Back to the template from a sub-template, at the template's selection from before. Undo: view.
    case closeSubTemplate
    /// A new sub-template, empty or with a DOCX's body. Undo: one step.
    ///
    /// - Parameters:
    ///   - name: Its name: unique in its category.
    ///   - category: Its building block category; default: the first rule's.
    ///   - from: A DOCX whose body becomes its body; default: one empty paragraph.
    ///   - copyOf: A sub-template in the same category whose body (formatting, fields, controls, pictures, references) is copied; not with `from`.
    case createSubTemplate(name: String, category: String? = nil, from: Data? = nil, copyOf: String? = nil)
    /// Renames a sub-template, and optionally every reference to it. Undo: one step.
    ///
    /// - Parameters:
    ///   - name: Its name now.
    ///   - to: Its new name: unique in its category.
    ///   - category: Its building block category, where several rules name categories.
    ///   - updateReferences: Renames the references too, in the template and in other sub-templates. Default: false.
    case renameSubTemplate(name: String, to: String, category: String? = nil, updateReferences: Bool = false)
    /// Removes a sub-template; its references no longer resolve. Undo: one step.
    ///
    /// - Parameters:
    ///   - name: Its name.
    ///   - category: Its building block category; default: the rules' categories, in order.
    case removeSubTemplate(name: String, category: String? = nil)
    /// The template's body becomes a DOCX's body, on its last section's page; its sub-templates, data parts,
    /// settings, styles, headers and footers stay. Closes a sub-template being edited first. Undo: one step.
    ///
    /// - Parameters:
    ///   - from: The DOCX whose body becomes the template's.
    case replaceBody(from: Data)
    /// A content control at the selection; result `{ id }`. Undo: one step.
    case insertControl(InsertControl)
    /// Changes a content control's properties. Undo: one step.
    case setControl(SetControl)
    /// Removes a content control, keeping or deleting its content. Undo: one step.
    ///
    /// - Parameters:
    ///   - id: The control.
    ///   - keepContent: Keeps the control's content in the document. Default: false.
    case removeControl(id: ElementID, keepContent: Bool = false)
    /// Fills a control with exactly one of text, an image or a document. Undo: one step.
    ///
    /// - Parameters:
    ///   - id: The control.
    ///   - text: Plain text.
    ///   - image: An image, fitted to the control's box.
    ///   - docx: DOCX content.
    case fillControl(id: ElementID, text: String? = nil, image: Data? = nil, docx: Data? = nil)
    /// Checks or clears a checkbox. Undo: one step.
    ///
    /// - Parameters:
    ///   - id: The checkbox control.
    ///   - checked: Checks (`true`) or unchecks it.
    case setCheckbox(id: ElementID, checked: Bool)
    /// Wraps the selection in a repeating section; result `{ id }`. Undo: one step.
    ///
    /// - Parameters:
    ///   - tag: The section's tag.
    ///   - title: The section's title.
    case wrapInRepeatingSection(tag: String? = nil, title: String? = nil)
    /// Removes a field, or a control with its content. Undo: one step.
    case removeElement(ElementID)
    /// Replaces text in element codes, tags or titles, case-sensitive; result `{ changed }`. Undo: one step.
    ///
    /// - Parameters:
    ///   - find: The text to find (case-sensitive).
    ///   - replace: Its replacement, at every occurrence.
    ///   - in: Which property to search.
    ///   - kind: Fields or controls only; `nil` means both.
    ///   - ids: The elements to search; `nil` means every one.
    case replaceInElements(find: String, replace: String, in: ElementField, kind: ElementClass? = nil, ids: [ElementID]? = nil)
    /// Rewrites a font family's name in the document; result `{ changed, skipped }`. Undo: one step.
    ///
    /// - Parameters:
    ///   - from: The family to rewrite, case-insensitive.
    ///   - to: The family it becomes.
    ///   - scope: The whole document (the default) or the selection.
    ///   - bold: Only bold (`true`) or regular-weight (`false`) text; `nil` for both.
    ///   - italic: Only italic (`true`) or upright (`false`) text; `nil` for both.
    case replaceFont(from: String, to: String, scope: ReplaceScope = .document, bold: Bool? = nil, italic: Bool? = nil)
    /// Wavy marks with a tooltip on elements. Undo: view.
    ///
    /// - Parameters:
    ///   - marks: The marks to draw.
    ///   - clear: Removes the earlier marks first. Default: false.
    case markElements(marks: [ElementMark], clear: Bool = false)
    /// The tag prefixes whose picture controls keep their pictures (compared without case), replacing those set before:
    /// editing can't delete, paste over or replace such a picture; `fillControl` and `setControl` still change it, and
    /// in design mode deleting it removes its whole control. Undo: view.
    ///
    /// - Parameters:
    ///   - tagPrefixes: The tag prefixes, replacing those set before; an empty list protects none.
    case setProtectedPictureControls(tagPrefixes: [String])
    /// Selects a picture control (the selected one when none is named) and emits `pictureControlActivated`, in any
    /// mode. Undo: none.
    ///
    /// - Parameters:
    ///   - element: The picture control; the selected one when `nil`.
    case activatePictureControl(element: ElementID? = nil)
    /// Writes a custom XML data part (well-formed); result `{ itemId }`. Undo: one step.
    ///
    /// - Parameters:
    ///   - namespace: The part's root namespace.
    ///   - itemID: The part's item id, where several share the namespace.
    ///   - xml: The part's XML (well-formed).
    case writeDataPart(namespace: String, itemID: String? = nil, xml: String)
    /// Removes a custom XML data part. Undo: one step.
    ///
    /// - Parameters:
    ///   - namespace: The part's root namespace.
    ///   - itemID: The part's item id, where several share the namespace.
    case removeDataPart(namespace: String, itemID: String? = nil)
    /// Track changes on or off. Undo: one step.
    case setTrackChanges(Bool)
    /// How revisions show. Undo: view.
    case setMarkupView(MarkupView)
    /// Accepts a revision (a move as a pair). Undo: one step.
    case acceptRevision(RevisionID)
    /// Rejects a revision (a move as a pair). Undo: one step.
    case rejectRevision(RevisionID)
    /// Accepts the revisions in the selection. Undo: one step.
    case acceptRevisionsInSelection
    /// Rejects the revisions in the selection. Undo: one step.
    case rejectRevisionsInSelection
    /// Accepts every revision, or one author's. Undo: one step.
    ///
    /// - Parameters:
    ///   - author: The author; `nil` means every author.
    case acceptAllRevisions(author: String? = nil)
    /// Rejects every revision, or one author's. Undo: one step.
    ///
    /// - Parameters:
    ///   - author: The author; `nil` means every author.
    case rejectAllRevisions(author: String? = nil)
    /// Selects the next or previous revision. Undo: view.
    case selectRevision(Order)
    /// A comment on the selection, or the word at the caret; result `{ id }`. Undo: one step.
    case addComment(TextBody)
    /// A reply in a thread; result `{ id }`. Undo: one step.
    ///
    /// - Parameters:
    ///   - to: The comment replied to.
    ///   - body: The reply's text.
    case replyToComment(to: CommentID, body: TextBody)
    /// Changes a comment's body. Undo: one step.
    ///
    /// - Parameters:
    ///   - id: The comment.
    ///   - body: Its new text.
    case editComment(id: CommentID, body: TextBody)
    /// Deletes a reply, or the whole thread when it's the first comment. Undo: one step.
    case deleteComment(CommentID)
    /// Marks a thread done or open. Undo: one step.
    ///
    /// - Parameters:
    ///   - id: The thread's first comment.
    ///   - done: Resolves (`true`) or reopens the thread.
    case resolveComment(id: CommentID, done: Bool)
    /// The thread the application shows as active, or none. Undo: view.
    case setActiveComment(CommentID?)
    /// Selects and reveals a comment's range. Undo: view.
    case selectComment(CommentID)
    /// The colour an author's revisions and comments show in. Undo: view.
    ///
    /// - Parameters:
    ///   - author: The author's name.
    ///   - color: The color their revisions and comments are drawn in.
    case setAuthorColor(author: String, color: RGBColor)
    /// The zoom (10…500 percent), or a fit. Undo: view.
    case setZoom(Zoom)
    /// Formatting marks, gridlines and text boundaries. Undo: view.
    case setViewOptions(ViewOptions)
    /// Editing or design mode. Undo: view.
    case setMode(Mode)
    /// What a click on the ruler adds; answers the tab type picker. Undo: view.
    case setRulerTabType(TabSelector)
    /// Asks for the tab type picker (`popoverRequested`). Undo: view.
    case showTabTypes
    /// Asks for the options of the tab stop nearest the caret (`popoverRequested`). Undo: view.
    case showTabStop
    /// What typing corrects. Undo: none (a session setting).
    case setAutoCorrect(AutoCorrectChange)
    /// Puts back what was typed in place of the correction just made. Undo: one step.
    case revertAutoCorrect
    /// Reverts the correction just made and never makes it again. Undo: one step (the revert).
    case neverAutoCorrect
    /// Asks for the options of the correction just made (`popoverRequested`). Undo: view.
    case showAutoCorrectOptions
    /// Dismisses the popover asked for (`popoverClosed`): what an application that closes it itself, on Esc or a
    /// click elsewhere, sends. With none open it does nothing. Undo: view.
    case closePopover
    /// Says the formatting at the caret to screen readers (paragraph style, font and size, emphasis, colour,
    /// highlight) as an announcement; changes nothing. Undo: view.
    case describeFormatting

    /// This command's id: its `type` on the wire.
    public var id: CommandID {
        switch self {
        case .insertText: .insertText
        case .insertParagraph: .insertParagraph
        case .insertLineBreak: .insertLineBreak
        case .insertPageBreak: .insertPageBreak
        case .insertColumnBreak: .insertColumnBreak
        case .insertSectionBreak: .insertSectionBreak
        case .insertTab: .insertTab
        case .insertSymbol: .insertSymbol
        case .delete: .delete
        case .cut: .cut
        case .copy: .copy
        case .paste: .paste
        case .pasteText: .pasteText
        case .undo: .undo
        case .redo: .redo
        case .`repeat`: .`repeat`
        case .selectAll: .selectAll
        case .select: .select
        case .moveSelection: .moveSelection
        case .moveToCell: .moveToCell
        case .setSelectionMode: .setSelectionMode
        case .goBack: .goBack
        case .escape: .escape
        case .selectNextObject: .selectNextObject
        case .goToPage: .goToPage
        case .setBold: .setBold
        case .setItalic: .setItalic
        case .setUnderline: .setUnderline
        case .setStrikethrough: .setStrikethrough
        case .setDoubleStrikethrough: .setDoubleStrikethrough
        case .setSubscript: .setSubscript
        case .setSuperscript: .setSuperscript
        case .setSmallCaps: .setSmallCaps
        case .setAllCaps: .setAllCaps
        case .setHidden: .setHidden
        case .setFont: .setFont
        case .setFontSize: .setFontSize
        case .growFont: .growFont
        case .shrinkFont: .shrinkFont
        case .setFontColor: .setFontColor
        case .setHighlight: .setHighlight
        case .setCharacterSpacing: .setCharacterSpacing
        case .setCharacterPosition: .setCharacterPosition
        case .clearFormatting: .clearFormatting
        case .changeCase: .changeCase
        case .copyFormatting: .copyFormatting
        case .pasteFormatting: .pasteFormatting
        case .setAlignment: .setAlignment
        case .setLineSpacing: .setLineSpacing
        case .setParagraphSpacing: .setParagraphSpacing
        case .setIndents: .setIndents
        case .increaseIndent: .increaseIndent
        case .decreaseIndent: .decreaseIndent
        case .setParagraphStyle: .setParagraphStyle
        case .updateStyleToSelection: .updateStyleToSelection
        case .deleteStyle: .deleteStyle
        case .setStyleMeta: .setStyleMeta
        case .setDocumentDefaults: .setDocumentDefaults
        case .setList: .setList
        case .setListLevel: .setListLevel
        case .setListDefinition: .setListDefinition
        case .setNumberingValue: .setNumberingValue
        case .applyListEntry: .applyListEntry
        case .restartNumbering: .restartNumbering
        case .continueNumbering: .continueNumbering
        case .adjustListIndents: .adjustListIndents
        case .setParagraphFlow: .setParagraphFlow
        case .setParagraphBorders: .setParagraphBorders
        case .setParagraphShading: .setParagraphShading
        case .setParagraphBorderLines: .setParagraphBorderLines
        case .setShading: .setShading
        case .setTextBorder: .setTextBorder
        case .setPageBorders: .setPageBorders
        case .setFrame: .setFrame
        case .removeFrame: .removeFrame
        case .addTabStop: .addTabStop
        case .moveTabStop: .moveTabStop
        case .deleteTabStop: .deleteTabStop
        case .clearTabStops: .clearTabStops
        case .setDefaultTabStop: .setDefaultTabStop
        case .setMargins: .setMargins
        case .setOrientation: .setOrientation
        case .setPaperSize: .setPaperSize
        case .setColumns: .setColumns
        case .setHeaderFooterDistance: .setHeaderFooterDistance
        case .setDifferentFirstPage: .setDifferentFirstPage
        case .setDifferentOddEvenPages: .setDifferentOddEvenPages
        case .moveColumnBoundary: .moveColumnBoundary
        case .editHeader: .editHeader
        case .editFooter: .editFooter
        case .closeHeaderFooter: .closeHeaderFooter
        case .insertPageNumber: .insertPageNumber
        case .insertTable: .insertTable
        case .insertPicture: .insertPicture
        case .insertTextBox: .insertTextBox
        case .insertShape: .insertShape
        case .insertFootnote: .insertFootnote
        case .insertEndnote: .insertEndnote
        case .insertHyperlink: .insertHyperlink
        case .editHyperlink: .editHyperlink
        case .removeHyperlink: .removeHyperlink
        case .insertField: .insertField
        case .insertRows: .insertRows
        case .insertColumns: .insertColumns
        case .deleteRows: .deleteRows
        case .deleteColumns: .deleteColumns
        case .deleteTable: .deleteTable
        case .deleteCells: .deleteCells
        case .mergeCells: .mergeCells
        case .splitCells: .splitCells
        case .splitTable: .splitTable
        case .distributeRows: .distributeRows
        case .distributeColumns: .distributeColumns
        case .moveRows: .moveRows
        case .autoFitColumns: .autoFitColumns
        case .setCellAlignment: .setCellAlignment
        case .setCellShading: .setCellShading
        case .setCellBorders: .setCellBorders
        case .setCellMargins: .setCellMargins
        case .setHeaderRow: .setHeaderRow
        case .setRowHeight: .setRowHeight
        case .setRowCanBreak: .setRowCanBreak
        case .setColumnWidth: .setColumnWidth
        case .moveTableColumnBoundary: .moveTableColumnBoundary
        case .moveTableRowBoundary: .moveTableRowBoundary
        case .setTableWidth: .setTableWidth
        case .setTableAlignment: .setTableAlignment
        case .setTableIndent: .setTableIndent
        case .setTableStyle: .setTableStyle
        case .setTableStyleOptions: .setTableStyleOptions
        case .setTablePosition: .setTablePosition
        case .setObjectSize: .setObjectSize
        case .resetObjectSize: .resetObjectSize
        case .setObjectWrap: .setObjectWrap
        case .setAnchorLocked: .setAnchorLocked
        case .setObjectPosition: .setObjectPosition
        case .setObjectWrapOptions: .setObjectWrapOptions
        case .setAltText: .setAltText
        case .deleteObject: .deleteObject
        case .setShapePreset: .setShapePreset
        case .setShapeAdjust: .setShapeAdjust
        case .setShapeFill: .setShapeFill
        case .setShapeLine: .setShapeLine
        case .setShapeEffects: .setShapeEffects
        case .setShapeText: .setShapeText
        case .setPictureAdjust: .setPictureAdjust
        case .setObjectRotation: .setObjectRotation
        case .flipObject: .flipObject
        case .arrangeObject: .arrangeObject
        case .groupObjects: .groupObjects
        case .ungroupObjects: .ungroupObjects
        case .selectGroupChild: .selectGroupChild
        case .setField: .setField
        case .setFieldResults: .setFieldResults
        case .updateFields: .updateFields
        case .setFieldsLocked: .setFieldsLocked
        case .unlinkFields: .unlinkFields
        case .showFieldCodes: .showFieldCodes
        case .setFieldLabels: .setFieldLabels
        case .setSubTemplateRules: .setSubTemplateRules
        case .setApplicationFieldTypes: .setApplicationFieldTypes
        case .setSubTemplateView: .setSubTemplateView
        case .editSubTemplate: .editSubTemplate
        case .closeSubTemplate: .closeSubTemplate
        case .createSubTemplate: .createSubTemplate
        case .renameSubTemplate: .renameSubTemplate
        case .removeSubTemplate: .removeSubTemplate
        case .replaceBody: .replaceBody
        case .insertControl: .insertControl
        case .setControl: .setControl
        case .removeControl: .removeControl
        case .fillControl: .fillControl
        case .setCheckbox: .setCheckbox
        case .wrapInRepeatingSection: .wrapInRepeatingSection
        case .removeElement: .removeElement
        case .replaceInElements: .replaceInElements
        case .replaceFont: .replaceFont
        case .markElements: .markElements
        case .setProtectedPictureControls: .setProtectedPictureControls
        case .activatePictureControl: .activatePictureControl
        case .writeDataPart: .writeDataPart
        case .removeDataPart: .removeDataPart
        case .setTrackChanges: .setTrackChanges
        case .setMarkupView: .setMarkupView
        case .acceptRevision: .acceptRevision
        case .rejectRevision: .rejectRevision
        case .acceptRevisionsInSelection: .acceptRevisionsInSelection
        case .rejectRevisionsInSelection: .rejectRevisionsInSelection
        case .acceptAllRevisions: .acceptAllRevisions
        case .rejectAllRevisions: .rejectAllRevisions
        case .selectRevision: .selectRevision
        case .addComment: .addComment
        case .replyToComment: .replyToComment
        case .editComment: .editComment
        case .deleteComment: .deleteComment
        case .resolveComment: .resolveComment
        case .setActiveComment: .setActiveComment
        case .selectComment: .selectComment
        case .setAuthorColor: .setAuthorColor
        case .setZoom: .setZoom
        case .setViewOptions: .setViewOptions
        case .setMode: .setMode
        case .setRulerTabType: .setRulerTabType
        case .showTabTypes: .showTabTypes
        case .showTabStop: .showTabStop
        case .setAutoCorrect: .setAutoCorrect
        case .revertAutoCorrect: .revertAutoCorrect
        case .neverAutoCorrect: .neverAutoCorrect
        case .showAutoCorrectOptions: .showAutoCorrectOptions
        case .closePopover: .closePopover
        case .describeFormatting: .describeFormatting
        }
    }

    public init(from decoder: any Decoder) throws {
        let t = try TaggedDecoder(decoder)
        switch t.tag {
        case "insertText": self = .insertText(try t.value() as String)
        case "insertParagraph": self = .insertParagraph
        case "insertLineBreak": self = .insertLineBreak
        case "insertPageBreak": self = .insertPageBreak
        case "insertColumnBreak": self = .insertColumnBreak
        case "insertSectionBreak": self = .insertSectionBreak(try t.value() as SectionBreakKind)
        case "insertTab": self = .insertTab
        case "insertSymbol": self = .insertSymbol(
                character: try t["char"] as String,
                font: try t.optional("font") as String?)
        case "delete": self = .delete(
                direction: try t["direction"] as Direction,
                unit: try t.optional("unit") ?? .character)
        case "cut": self = .cut
        case "copy": self = .copy
        case "paste": self = .paste(
                as: try t.optional("as") ?? .formatted)
        case "pasteText": self = .pasteText(try t.value() as String)
        case "undo": self = .undo
        case "redo": self = .redo
        case "repeat": self = .`repeat`
        case "selectAll": self = .selectAll
        case "select": self = .select(try t.value() as SelectUnit)
        case "moveSelection": self = .moveSelection(
                by: try t["by"] as MoveUnit,
                direction: try t["direction"] as Direction,
                extend: try t.optional("extend") ?? false)
        case "moveToCell": self = .moveToCell(try t.value() as Order)
        case "setSelectionMode": self = .setSelectionMode(try t.value() as SelectionMode)
        case "goBack": self = .goBack
        case "escape": self = .escape
        case "selectNextObject": self = .selectNextObject(try t.value() as Order)
        case "goToPage": self = .goToPage(try t.value() as Int)
        case "setBold": self = .setBold(try t.value() as Bool)
        case "setItalic": self = .setItalic(try t.value() as Bool)
        case "setUnderline": self = .setUnderline(try t.value() as UnderlineStyle)
        case "setStrikethrough": self = .setStrikethrough(try t.value() as Bool)
        case "setDoubleStrikethrough": self = .setDoubleStrikethrough(try t.value() as Bool)
        case "setSubscript": self = .setSubscript(try t.value() as Bool)
        case "setSuperscript": self = .setSuperscript(try t.value() as Bool)
        case "setSmallCaps": self = .setSmallCaps(try t.value() as Bool)
        case "setAllCaps": self = .setAllCaps(try t.value() as Bool)
        case "setHidden": self = .setHidden(try t.value() as Bool)
        case "setFont": self = .setFont(try t.value() as String)
        case "setFontSize": self = .setFontSize(try t.value() as Points)
        case "growFont": self = .growFont
        case "shrinkFont": self = .shrinkFont
        case "setFontColor": self = .setFontColor(try t.value() as AutoOr<RGBColor>)
        case "setHighlight": self = .setHighlight(try t.value() as HighlightColor?)
        case "setCharacterSpacing": self = .setCharacterSpacing(try t.value() as Points)
        case "setCharacterPosition": self = .setCharacterPosition(try t.value() as Points)
        case "clearFormatting": self = .clearFormatting
        case "changeCase": self = .changeCase(try t.value() as CaseChange)
        case "copyFormatting": self = .copyFormatting
        case "pasteFormatting": self = .pasteFormatting
        case "setAlignment": self = .setAlignment(try t.value() as ParagraphAlignment)
        case "setLineSpacing": self = .setLineSpacing(try t.spread() as LineSpacing)
        case "setParagraphSpacing": self = .setParagraphSpacing(
                before: try t.optional("before") as AutoOr<Points>?,
                after: try t.optional("after") as AutoOr<Points>?)
        case "setIndents": self = .setIndents(
                left: try t.optional("left") as Points?,
                right: try t.optional("right") as Points?,
                firstLine: try t.optional("firstLine") as Points?,
                hanging: try t.optional("hanging") as Points?)
        case "increaseIndent": self = .increaseIndent
        case "decreaseIndent": self = .decreaseIndent
        case "setParagraphStyle": self = .setParagraphStyle(try t.value() as StyleID)
        case "updateStyleToSelection": self = .updateStyleToSelection(try t.value() as StyleID)
        case "deleteStyle": self = .deleteStyle(try t.value() as StyleID)
        case "setStyleMeta": self = .setStyleMeta(
                id: try t["id"] as StyleID,
                visibility: try t.optional("visibility") as StyleVisibility?,
                priority: try t.optional("priority") as Int?,
                quick: try t.optional("quick") as Bool?,
                locked: try t.optional("locked") as Bool?)
        case "setDocumentDefaults": self = .setDocumentDefaults(try t.spread() as DocumentDefaults)
        case "setList": self = .setList(try t.value() as ListKind?)
        case "setListLevel": self = .setListLevel(try t.value() as LevelChange)
        case "setListDefinition": self = .setListDefinition(
                levels: try t["levels"] as [ListLevel],
                applyTo: try t.optional("applyTo") ?? .wholeList,
                name: try t.optional("name") as String?)
        case "setNumberingValue": self = .setNumberingValue(
                mode: try t["mode"] as NumberingMode,
                value: try t["value"] as Int,
                advance: try t.optional("advance") ?? false)
        case "applyListEntry": self = .applyListEntry(try t.value() as ListEntryID)
        case "restartNumbering": self = .restartNumbering
        case "continueNumbering": self = .continueNumbering
        case "adjustListIndents": self = .adjustListIndents(
                numberPosition: try t["numberPosition"] as Points,
                textIndent: try t["textIndent"] as Points,
                follow: try t["follow"] as FollowNumber,
                tab: try t.optional("tab") as Points?)
        case "setParagraphFlow": self = .setParagraphFlow(
                keepWithNext: try t.optional("keepWithNext") as Bool?,
                keepLinesTogether: try t.optional("keepLinesTogether") as Bool?,
                pageBreakBefore: try t.optional("pageBreakBefore") as Bool?,
                widowControl: try t.optional("widowControl") as Bool?)
        case "setParagraphBorders": self = .setParagraphBorders(try t.value() as BorderPreset)
        case "setParagraphShading": self = .setParagraphShading(try t.value() as RGBColor?)
        case "setParagraphBorderLines": self = .setParagraphBorderLines(try t.spread() as ParagraphBorderLines)
        case "setShading": self = .setShading(
                target: try t["target"] as ShadingTarget,
                fill: try t["fill"] as NoneOr<RGBColor>,
                pattern: try t.optional("pattern") ?? .clear,
                color: try t.optional("color") ?? .auto)
        case "setTextBorder": self = .setTextBorder(
                border: try t["border"] as Border?,
                shadow: try t.optional("shadow") ?? false)
        case "setPageBorders": self = .setPageBorders(try t.spread() as PageBordersChange)
        case "setFrame": self = .setFrame(try t.spread() as FrameProperties)
        case "removeFrame": self = .removeFrame
        case "addTabStop": self = .addTabStop(
                position: try t["position"] as Points,
                alignment: try t.optional("alignment") as TabAlignment?,
                leader: try t.optional("leader") as TabLeader?)
        case "moveTabStop": self = .moveTabStop(
                from: try t["from"] as Points,
                to: try t["to"] as Points)
        case "deleteTabStop": self = .deleteTabStop(try t.value() as Points)
        case "clearTabStops": self = .clearTabStops
        case "setDefaultTabStop": self = .setDefaultTabStop(try t.value() as Points)
        case "setMargins": self = .setMargins(
                top: try t.optional("top") as Points?,
                bottom: try t.optional("bottom") as Points?,
                left: try t.optional("left") as Points?,
                right: try t.optional("right") as Points?,
                gutter: try t.optional("gutter") as Points?)
        case "setOrientation": self = .setOrientation(try t.value() as Orientation)
        case "setPaperSize": self = .setPaperSize(
                width: try t["width"] as Points,
                height: try t["height"] as Points)
        case "setColumns": self = .setColumns(
                count: try t["count"] as Int,
                spacing: try t.optional("spacing") as Points?,
                separator: try t.optional("separator") as Bool?)
        case "setHeaderFooterDistance": self = .setHeaderFooterDistance(
                header: try t.optional("header") as Points?,
                footer: try t.optional("footer") as Points?)
        case "setDifferentFirstPage": self = .setDifferentFirstPage(try t.value() as Bool)
        case "setDifferentOddEvenPages": self = .setDifferentOddEvenPages(try t.value() as Bool)
        case "moveColumnBoundary": self = .moveColumnBoundary(
                column: try t["column"] as Int,
                to: try t["to"] as Points)
        case "editHeader": self = .editHeader
        case "editFooter": self = .editFooter
        case "closeHeaderFooter": self = .closeHeaderFooter
        case "insertPageNumber": self = .insertPageNumber(
                format: try t["format"] as PageNumberFormat,
                where: try t.optional("where") ?? .caret,
                alignment: try t.optional("alignment") as HorizontalAlign?)
        case "insertTable": self = .insertTable(
                rows: try t["rows"] as Int,
                columns: try t["columns"] as Int)
        case "insertPicture": self = .insertPicture(
                data: try t["data"] as Data,
                altText: try t.optional("altText") as String?)
        case "insertTextBox": self = .insertTextBox
        case "insertShape": self = .insertShape(try t.spread() as ShapeInsert)
        case "insertFootnote": self = .insertFootnote
        case "insertEndnote": self = .insertEndnote
        case "insertHyperlink": self = .insertHyperlink(
                address: try t["address"] as String,
                text: try t.optional("text") as String?)
        case "editHyperlink": self = .editHyperlink(
                address: try t.optional("address") as String?,
                text: try t.optional("text") as String?)
        case "removeHyperlink": self = .removeHyperlink
        case "insertField": self = .insertField(
                code: try t["code"] as String,
                display: try t.optional("display") as String?,
                showCode: try t.optional("showCode") as Bool?)
        case "insertRows": self = .insertRows(
                where: try t["where"] as VerticalSide,
                count: try t.optional("count") ?? 1)
        case "insertColumns": self = .insertColumns(
                where: try t["where"] as HorizontalSide,
                count: try t.optional("count") ?? 1)
        case "deleteRows": self = .deleteRows
        case "deleteColumns": self = .deleteColumns
        case "deleteTable": self = .deleteTable
        case "deleteCells": self = .deleteCells(try t.value() as CellShift)
        case "mergeCells": self = .mergeCells
        case "splitCells": self = .splitCells(
                columns: try t["columns"] as Int,
                rows: try t.optional("rows") ?? 1)
        case "splitTable": self = .splitTable
        case "distributeRows": self = .distributeRows
        case "distributeColumns": self = .distributeColumns
        case "moveRows": self = .moveRows(try t.value() as VerticalDirection)
        case "autoFitColumns": self = .autoFitColumns(try t.value() as AutoFit)
        case "setCellAlignment": self = .setCellAlignment(try t.value() as CellAlignment)
        case "setCellShading": self = .setCellShading(try t.value() as RGBColor?)
        case "setCellBorders": self = .setCellBorders(
                edges: try t["edges"] as BorderEdges,
                border: try t.optional("border") as Border?)
        case "setCellMargins": self = .setCellMargins(
                top: try t.optional("top") as Points?,
                bottom: try t.optional("bottom") as Points?,
                left: try t.optional("left") as Points?,
                right: try t.optional("right") as Points?)
        case "setHeaderRow": self = .setHeaderRow(try t.value() as Bool)
        case "setRowHeight": self = .setRowHeight(try t.value() as AutoOr<RowHeight>)
        case "setRowCanBreak": self = .setRowCanBreak(try t.value() as Bool)
        case "setColumnWidth": self = .setColumnWidth(try t.value() as Points)
        case "moveTableColumnBoundary": self = .moveTableColumnBoundary(
                index: try t["index"] as Int,
                to: try t["to"] as Points)
        case "moveTableRowBoundary": self = .moveTableRowBoundary(
                index: try t["index"] as Int,
                to: try t["to"] as Points)
        case "setTableWidth": self = .setTableWidth(try t.spread() as TableWidth)
        case "setTableAlignment": self = .setTableAlignment(try t.value() as HorizontalAlign)
        case "setTableIndent": self = .setTableIndent(try t.value() as Points)
        case "setTableStyle": self = .setTableStyle(try t.value() as StyleID)
        case "setTableStyleOptions": self = .setTableStyleOptions(try t.spread() as TableStyleChange)
        case "setTablePosition": self = .setTablePosition(try t.value() as TablePosition?)
        case "setObjectSize": self = .setObjectSize(
                width: try t.optional("width") as Points?,
                height: try t.optional("height") as Points?,
                lockAspect: try t.optional("lockAspect") as Bool?)
        case "resetObjectSize": self = .resetObjectSize
        case "setObjectWrap": self = .setObjectWrap(try t.value() as ObjectWrap)
        case "setAnchorLocked": self = .setAnchorLocked(try t.value() as Bool)
        case "setObjectPosition": self = .setObjectPosition(
                horizontal: try t.optional("horizontal") as HorizontalPosition?,
                vertical: try t.optional("vertical") as VerticalPosition?,
                allowOverlap: try t.optional("allowOverlap") as Bool?,
                layoutInCell: try t.optional("layoutInCell") as Bool?)
        case "setObjectWrapOptions": self = .setObjectWrapOptions(
                side: try t.optional("side") as WrapSide?,
                top: try t.optional("top") as Points?,
                bottom: try t.optional("bottom") as Points?,
                left: try t.optional("left") as Points?,
                right: try t.optional("right") as Points?)
        case "setAltText": self = .setAltText(
                text: try t["text"] as String,
                decorative: try t.optional("decorative") as Bool?)
        case "deleteObject": self = .deleteObject
        case "setShapePreset": self = .setShapePreset(try t.value() as ShapePreset)
        case "setShapeAdjust": self = .setShapeAdjust(try t.spread() as ShapeAdjust)
        case "setShapeFill": self = .setShapeFill(try t.value() as FillValue)
        case "setShapeLine": self = .setShapeLine(try t.spread() as LineChange)
        case "setShapeEffects": self = .setShapeEffects(try t.spread() as EffectsChange)
        case "setShapeText": self = .setShapeText(try t.spread() as ShapeTextChange)
        case "setPictureAdjust": self = .setPictureAdjust(try t.spread() as PictureAdjustChange)
        case "setObjectRotation": self = .setObjectRotation(try t.value() as Double)
        case "flipObject": self = .flipObject(try t.value() as FlipAxis)
        case "arrangeObject": self = .arrangeObject(try t.value() as Arrange)
        case "groupObjects": self = .groupObjects
        case "ungroupObjects": self = .ungroupObjects
        case "selectGroupChild": self = .selectGroupChild(try t.value() as Order)
        case "setField": self = .setField(
                id: try t["id"] as ElementID,
                code: try t.optional("code") as String?,
                display: try t.optional("display") as TextBody?)
        case "setFieldResults": self = .setFieldResults(
                results: try t["results"] as [FieldResult],
                token: try t.optional("token") as Int?)
        case "updateFields": self = .updateFields(
                ids: try t.optional("ids") as [ElementID]?)
        case "setFieldsLocked": self = .setFieldsLocked(
                locked: try t["locked"] as Bool,
                ids: try t.optional("ids") as [ElementID]?)
        case "unlinkFields": self = .unlinkFields(
                ids: try t.optional("ids") as [ElementID]?)
        case "showFieldCodes": self = .showFieldCodes(
                show: try t["show"] as Bool,
                ids: try t.optional("ids") as [ElementID]?)
        case "setFieldLabels": self = .setFieldLabels(
                labels: try t["labels"] as [String: String],
                merge: try t.optional("merge") ?? false)
        case "setSubTemplateRules": self = .setSubTemplateRules(
                rules: try t["rules"] as [ReferenceRule])
        case "setApplicationFieldTypes": self = .setApplicationFieldTypes(
                types: try t["types"] as [String])
        case "setSubTemplateView": self = .setSubTemplateView(
                ids: try t.optional("ids") as [ElementID]?,
                expanded: try t["expanded"] as Bool)
        case "editSubTemplate": self = .editSubTemplate(
                name: try t["name"] as String,
                category: try t.optional("category") as String?)
        case "closeSubTemplate": self = .closeSubTemplate
        case "createSubTemplate": self = .createSubTemplate(
                name: try t["name"] as String,
                category: try t.optional("category") as String?,
                from: try t.optional("from") as Data?,
                copyOf: try t.optional("copyOf") as String?)
        case "renameSubTemplate": self = .renameSubTemplate(
                name: try t["name"] as String,
                to: try t["to"] as String,
                category: try t.optional("category") as String?,
                updateReferences: try t.optional("updateReferences") ?? false)
        case "removeSubTemplate": self = .removeSubTemplate(
                name: try t["name"] as String,
                category: try t.optional("category") as String?)
        case "replaceBody": self = .replaceBody(
                from: try t["from"] as Data)
        case "insertControl": self = .insertControl(try t.spread() as InsertControl)
        case "setControl": self = .setControl(try t.spread() as SetControl)
        case "removeControl": self = .removeControl(
                id: try t["id"] as ElementID,
                keepContent: try t.optional("keepContent") ?? false)
        case "fillControl": self = .fillControl(
                id: try t["id"] as ElementID,
                text: try t.optional("text") as String?,
                image: try t.optional("image") as Data?,
                docx: try t.optional("docx") as Data?)
        case "setCheckbox": self = .setCheckbox(
                id: try t["id"] as ElementID,
                checked: try t["checked"] as Bool)
        case "wrapInRepeatingSection": self = .wrapInRepeatingSection(
                tag: try t.optional("tag") as String?,
                title: try t.optional("title") as String?)
        case "removeElement": self = .removeElement(try t.value() as ElementID)
        case "replaceInElements": self = .replaceInElements(
                find: try t["find"] as String,
                replace: try t["replace"] as String,
                in: try t["in"] as ElementField,
                kind: try t.optional("kind") as ElementClass?,
                ids: try t.optional("ids") as [ElementID]?)
        case "replaceFont": self = .replaceFont(
                from: try t["from"] as String,
                to: try t["to"] as String,
                scope: try t.optional("scope") ?? .document,
                bold: try t.optional("bold") as Bool?,
                italic: try t.optional("italic") as Bool?)
        case "markElements": self = .markElements(
                marks: try t["marks"] as [ElementMark],
                clear: try t.optional("clear") ?? false)
        case "setProtectedPictureControls": self = .setProtectedPictureControls(
                tagPrefixes: try t["tagPrefixes"] as [String])
        case "activatePictureControl": self = .activatePictureControl(
                element: try t.optional("element") as ElementID?)
        case "writeDataPart": self = .writeDataPart(
                namespace: try t["namespace"] as String,
                itemID: try t.optional("itemId") as String?,
                xml: try t["xml"] as String)
        case "removeDataPart": self = .removeDataPart(
                namespace: try t["namespace"] as String,
                itemID: try t.optional("itemId") as String?)
        case "setTrackChanges": self = .setTrackChanges(try t.value() as Bool)
        case "setMarkupView": self = .setMarkupView(try t.value() as MarkupView)
        case "acceptRevision": self = .acceptRevision(try t.value() as RevisionID)
        case "rejectRevision": self = .rejectRevision(try t.value() as RevisionID)
        case "acceptRevisionsInSelection": self = .acceptRevisionsInSelection
        case "rejectRevisionsInSelection": self = .rejectRevisionsInSelection
        case "acceptAllRevisions": self = .acceptAllRevisions(
                author: try t.optional("author") as String?)
        case "rejectAllRevisions": self = .rejectAllRevisions(
                author: try t.optional("author") as String?)
        case "selectRevision": self = .selectRevision(try t.value() as Order)
        case "addComment": self = .addComment(try t.value() as TextBody)
        case "replyToComment": self = .replyToComment(
                to: try t["to"] as CommentID,
                body: try t["body"] as TextBody)
        case "editComment": self = .editComment(
                id: try t["id"] as CommentID,
                body: try t["body"] as TextBody)
        case "deleteComment": self = .deleteComment(try t.value() as CommentID)
        case "resolveComment": self = .resolveComment(
                id: try t["id"] as CommentID,
                done: try t["done"] as Bool)
        case "setActiveComment": self = .setActiveComment(try t.value() as CommentID?)
        case "selectComment": self = .selectComment(try t.value() as CommentID)
        case "setAuthorColor": self = .setAuthorColor(
                author: try t["author"] as String,
                color: try t["color"] as RGBColor)
        case "setZoom": self = .setZoom(try t.value() as Zoom)
        case "setViewOptions": self = .setViewOptions(try t.spread() as ViewOptions)
        case "setMode": self = .setMode(try t.value() as Mode)
        case "setRulerTabType": self = .setRulerTabType(try t.value() as TabSelector)
        case "showTabTypes": self = .showTabTypes
        case "showTabStop": self = .showTabStop
        case "setAutoCorrect": self = .setAutoCorrect(try t.spread() as AutoCorrectChange)
        case "revertAutoCorrect": self = .revertAutoCorrect
        case "neverAutoCorrect": self = .neverAutoCorrect
        case "showAutoCorrectOptions": self = .showAutoCorrectOptions
        case "closePopover": self = .closePopover
        case "describeFormatting": self = .describeFormatting
        default: throw t.unknown()
        }
    }

    public func encode(to encoder: any Encoder) throws {
        let t = TaggedEncoder(encoder)
        switch self {
        case .insertText(let v): try t.tag("insertText", value: v)
        case .insertParagraph: try t.tag("insertParagraph")
        case .insertLineBreak: try t.tag("insertLineBreak")
        case .insertPageBreak: try t.tag("insertPageBreak")
        case .insertColumnBreak: try t.tag("insertColumnBreak")
        case .insertSectionBreak(let v): try t.tag("insertSectionBreak", value: v)
        case .insertTab: try t.tag("insertTab")
        case .insertSymbol(let character, let font):
            try t.tag("insertSymbol", ["char": character, "font": font])
        case .delete(let direction, let unit):
            try t.tag("delete", ["direction": direction, "unit": unit])
        case .cut: try t.tag("cut")
        case .copy: try t.tag("copy")
        case .paste(let `as`):
            try t.tag("paste", ["as": `as`])
        case .pasteText(let v): try t.tag("pasteText", value: v)
        case .undo: try t.tag("undo")
        case .redo: try t.tag("redo")
        case .`repeat`: try t.tag("repeat")
        case .selectAll: try t.tag("selectAll")
        case .select(let v): try t.tag("select", value: v)
        case .moveSelection(let by, let direction, let extend):
            try t.tag("moveSelection", ["by": by, "direction": direction, "extend": extend])
        case .moveToCell(let v): try t.tag("moveToCell", value: v)
        case .setSelectionMode(let v): try t.tag("setSelectionMode", value: v)
        case .goBack: try t.tag("goBack")
        case .escape: try t.tag("escape")
        case .selectNextObject(let v): try t.tag("selectNextObject", value: v)
        case .goToPage(let v): try t.tag("goToPage", value: v)
        case .setBold(let v): try t.tag("setBold", value: v)
        case .setItalic(let v): try t.tag("setItalic", value: v)
        case .setUnderline(let v): try t.tag("setUnderline", value: v)
        case .setStrikethrough(let v): try t.tag("setStrikethrough", value: v)
        case .setDoubleStrikethrough(let v): try t.tag("setDoubleStrikethrough", value: v)
        case .setSubscript(let v): try t.tag("setSubscript", value: v)
        case .setSuperscript(let v): try t.tag("setSuperscript", value: v)
        case .setSmallCaps(let v): try t.tag("setSmallCaps", value: v)
        case .setAllCaps(let v): try t.tag("setAllCaps", value: v)
        case .setHidden(let v): try t.tag("setHidden", value: v)
        case .setFont(let v): try t.tag("setFont", value: v)
        case .setFontSize(let v): try t.tag("setFontSize", value: v)
        case .growFont: try t.tag("growFont")
        case .shrinkFont: try t.tag("shrinkFont")
        case .setFontColor(let v): try t.tag("setFontColor", value: v)
        case .setHighlight(let v): try t.tag("setHighlight", value: v)
        case .setCharacterSpacing(let v): try t.tag("setCharacterSpacing", value: v)
        case .setCharacterPosition(let v): try t.tag("setCharacterPosition", value: v)
        case .clearFormatting: try t.tag("clearFormatting")
        case .changeCase(let v): try t.tag("changeCase", value: v)
        case .copyFormatting: try t.tag("copyFormatting")
        case .pasteFormatting: try t.tag("pasteFormatting")
        case .setAlignment(let v): try t.tag("setAlignment", value: v)
        case .setLineSpacing(let v): try t.tag("setLineSpacing", spread: v)
        case .setParagraphSpacing(let before, let after):
            try t.tag("setParagraphSpacing", ["before": before, "after": after])
        case .setIndents(let left, let right, let firstLine, let hanging):
            try t.tag("setIndents", ["left": left, "right": right, "firstLine": firstLine, "hanging": hanging])
        case .increaseIndent: try t.tag("increaseIndent")
        case .decreaseIndent: try t.tag("decreaseIndent")
        case .setParagraphStyle(let v): try t.tag("setParagraphStyle", value: v)
        case .updateStyleToSelection(let v): try t.tag("updateStyleToSelection", value: v)
        case .deleteStyle(let v): try t.tag("deleteStyle", value: v)
        case .setStyleMeta(let id, let visibility, let priority, let quick, let locked):
            try t.tag("setStyleMeta", ["id": id, "visibility": visibility, "priority": priority, "quick": quick, "locked": locked])
        case .setDocumentDefaults(let v): try t.tag("setDocumentDefaults", spread: v)
        case .setList(let v): try t.tag("setList", value: v)
        case .setListLevel(let v): try t.tag("setListLevel", value: v)
        case .setListDefinition(let levels, let applyTo, let name):
            try t.tag("setListDefinition", ["levels": levels, "applyTo": applyTo, "name": name])
        case .setNumberingValue(let mode, let value, let advance):
            try t.tag("setNumberingValue", ["mode": mode, "value": value, "advance": advance])
        case .applyListEntry(let v): try t.tag("applyListEntry", value: v)
        case .restartNumbering: try t.tag("restartNumbering")
        case .continueNumbering: try t.tag("continueNumbering")
        case .adjustListIndents(let numberPosition, let textIndent, let follow, let tab):
            try t.tag("adjustListIndents", ["numberPosition": numberPosition, "textIndent": textIndent, "follow": follow, "tab": tab])
        case .setParagraphFlow(let keepWithNext, let keepLinesTogether, let pageBreakBefore, let widowControl):
            try t.tag("setParagraphFlow", ["keepWithNext": keepWithNext, "keepLinesTogether": keepLinesTogether, "pageBreakBefore": pageBreakBefore, "widowControl": widowControl])
        case .setParagraphBorders(let v): try t.tag("setParagraphBorders", value: v)
        case .setParagraphShading(let v): try t.tag("setParagraphShading", value: v)
        case .setParagraphBorderLines(let v): try t.tag("setParagraphBorderLines", spread: v)
        case .setShading(let target, let fill, let pattern, let color):
            try t.tag("setShading", ["target": target, "fill": fill, "pattern": pattern, "color": color])
        case .setTextBorder(let border, let shadow):
            try t.tag("setTextBorder", ["border": border, "shadow": shadow])
        case .setPageBorders(let v): try t.tag("setPageBorders", spread: v)
        case .setFrame(let v): try t.tag("setFrame", spread: v)
        case .removeFrame: try t.tag("removeFrame")
        case .addTabStop(let position, let alignment, let leader):
            try t.tag("addTabStop", ["position": position, "alignment": alignment, "leader": leader])
        case .moveTabStop(let from, let to):
            try t.tag("moveTabStop", ["from": from, "to": to])
        case .deleteTabStop(let v): try t.tag("deleteTabStop", value: v)
        case .clearTabStops: try t.tag("clearTabStops")
        case .setDefaultTabStop(let v): try t.tag("setDefaultTabStop", value: v)
        case .setMargins(let top, let bottom, let left, let right, let gutter):
            try t.tag("setMargins", ["top": top, "bottom": bottom, "left": left, "right": right, "gutter": gutter])
        case .setOrientation(let v): try t.tag("setOrientation", value: v)
        case .setPaperSize(let width, let height):
            try t.tag("setPaperSize", ["width": width, "height": height])
        case .setColumns(let count, let spacing, let separator):
            try t.tag("setColumns", ["count": count, "spacing": spacing, "separator": separator])
        case .setHeaderFooterDistance(let header, let footer):
            try t.tag("setHeaderFooterDistance", ["header": header, "footer": footer])
        case .setDifferentFirstPage(let v): try t.tag("setDifferentFirstPage", value: v)
        case .setDifferentOddEvenPages(let v): try t.tag("setDifferentOddEvenPages", value: v)
        case .moveColumnBoundary(let column, let to):
            try t.tag("moveColumnBoundary", ["column": column, "to": to])
        case .editHeader: try t.tag("editHeader")
        case .editFooter: try t.tag("editFooter")
        case .closeHeaderFooter: try t.tag("closeHeaderFooter")
        case .insertPageNumber(let format, let `where`, let alignment):
            try t.tag("insertPageNumber", ["format": format, "where": `where`, "alignment": alignment])
        case .insertTable(let rows, let columns):
            try t.tag("insertTable", ["rows": rows, "columns": columns])
        case .insertPicture(let data, let altText):
            try t.tag("insertPicture", ["data": data, "altText": altText])
        case .insertTextBox: try t.tag("insertTextBox")
        case .insertShape(let v): try t.tag("insertShape", spread: v)
        case .insertFootnote: try t.tag("insertFootnote")
        case .insertEndnote: try t.tag("insertEndnote")
        case .insertHyperlink(let address, let text):
            try t.tag("insertHyperlink", ["address": address, "text": text])
        case .editHyperlink(let address, let text):
            try t.tag("editHyperlink", ["address": address, "text": text])
        case .removeHyperlink: try t.tag("removeHyperlink")
        case .insertField(let code, let display, let showCode):
            try t.tag("insertField", ["code": code, "display": display, "showCode": showCode])
        case .insertRows(let `where`, let count):
            try t.tag("insertRows", ["where": `where`, "count": count])
        case .insertColumns(let `where`, let count):
            try t.tag("insertColumns", ["where": `where`, "count": count])
        case .deleteRows: try t.tag("deleteRows")
        case .deleteColumns: try t.tag("deleteColumns")
        case .deleteTable: try t.tag("deleteTable")
        case .deleteCells(let v): try t.tag("deleteCells", value: v)
        case .mergeCells: try t.tag("mergeCells")
        case .splitCells(let columns, let rows):
            try t.tag("splitCells", ["columns": columns, "rows": rows])
        case .splitTable: try t.tag("splitTable")
        case .distributeRows: try t.tag("distributeRows")
        case .distributeColumns: try t.tag("distributeColumns")
        case .moveRows(let v): try t.tag("moveRows", value: v)
        case .autoFitColumns(let v): try t.tag("autoFitColumns", value: v)
        case .setCellAlignment(let v): try t.tag("setCellAlignment", value: v)
        case .setCellShading(let v): try t.tag("setCellShading", value: v)
        case .setCellBorders(let edges, let border):
            try t.tag("setCellBorders", ["edges": edges, "border": border])
        case .setCellMargins(let top, let bottom, let left, let right):
            try t.tag("setCellMargins", ["top": top, "bottom": bottom, "left": left, "right": right])
        case .setHeaderRow(let v): try t.tag("setHeaderRow", value: v)
        case .setRowHeight(let v): try t.tag("setRowHeight", value: v)
        case .setRowCanBreak(let v): try t.tag("setRowCanBreak", value: v)
        case .setColumnWidth(let v): try t.tag("setColumnWidth", value: v)
        case .moveTableColumnBoundary(let index, let to):
            try t.tag("moveTableColumnBoundary", ["index": index, "to": to])
        case .moveTableRowBoundary(let index, let to):
            try t.tag("moveTableRowBoundary", ["index": index, "to": to])
        case .setTableWidth(let v): try t.tag("setTableWidth", spread: v)
        case .setTableAlignment(let v): try t.tag("setTableAlignment", value: v)
        case .setTableIndent(let v): try t.tag("setTableIndent", value: v)
        case .setTableStyle(let v): try t.tag("setTableStyle", value: v)
        case .setTableStyleOptions(let v): try t.tag("setTableStyleOptions", spread: v)
        case .setTablePosition(let v): try t.tag("setTablePosition", value: v)
        case .setObjectSize(let width, let height, let lockAspect):
            try t.tag("setObjectSize", ["width": width, "height": height, "lockAspect": lockAspect])
        case .resetObjectSize: try t.tag("resetObjectSize")
        case .setObjectWrap(let v): try t.tag("setObjectWrap", value: v)
        case .setAnchorLocked(let v): try t.tag("setAnchorLocked", value: v)
        case .setObjectPosition(let horizontal, let vertical, let allowOverlap, let layoutInCell):
            try t.tag("setObjectPosition", ["horizontal": horizontal, "vertical": vertical, "allowOverlap": allowOverlap, "layoutInCell": layoutInCell])
        case .setObjectWrapOptions(let side, let top, let bottom, let left, let right):
            try t.tag("setObjectWrapOptions", ["side": side, "top": top, "bottom": bottom, "left": left, "right": right])
        case .setAltText(let text, let decorative):
            try t.tag("setAltText", ["text": text, "decorative": decorative])
        case .deleteObject: try t.tag("deleteObject")
        case .setShapePreset(let v): try t.tag("setShapePreset", value: v)
        case .setShapeAdjust(let v): try t.tag("setShapeAdjust", spread: v)
        case .setShapeFill(let v): try t.tag("setShapeFill", value: v)
        case .setShapeLine(let v): try t.tag("setShapeLine", spread: v)
        case .setShapeEffects(let v): try t.tag("setShapeEffects", spread: v)
        case .setShapeText(let v): try t.tag("setShapeText", spread: v)
        case .setPictureAdjust(let v): try t.tag("setPictureAdjust", spread: v)
        case .setObjectRotation(let v): try t.tag("setObjectRotation", value: v)
        case .flipObject(let v): try t.tag("flipObject", value: v)
        case .arrangeObject(let v): try t.tag("arrangeObject", value: v)
        case .groupObjects: try t.tag("groupObjects")
        case .ungroupObjects: try t.tag("ungroupObjects")
        case .selectGroupChild(let v): try t.tag("selectGroupChild", value: v)
        case .setField(let id, let code, let display):
            try t.tag("setField", ["id": id, "code": code, "display": display])
        case .setFieldResults(let results, let token):
            try t.tag("setFieldResults", ["results": results, "token": token])
        case .updateFields(let ids):
            try t.tag("updateFields", ["ids": ids])
        case .setFieldsLocked(let locked, let ids):
            try t.tag("setFieldsLocked", ["locked": locked, "ids": ids])
        case .unlinkFields(let ids):
            try t.tag("unlinkFields", ["ids": ids])
        case .showFieldCodes(let show, let ids):
            try t.tag("showFieldCodes", ["show": show, "ids": ids])
        case .setFieldLabels(let labels, let merge):
            try t.tag("setFieldLabels", ["labels": labels, "merge": merge])
        case .setSubTemplateRules(let rules):
            try t.tag("setSubTemplateRules", ["rules": rules])
        case .setApplicationFieldTypes(let types):
            try t.tag("setApplicationFieldTypes", ["types": types])
        case .setSubTemplateView(let ids, let expanded):
            try t.tag("setSubTemplateView", ["ids": ids, "expanded": expanded])
        case .editSubTemplate(let name, let category):
            try t.tag("editSubTemplate", ["name": name, "category": category])
        case .closeSubTemplate: try t.tag("closeSubTemplate")
        case .createSubTemplate(let name, let category, let from, let copyOf):
            try t.tag("createSubTemplate", ["name": name, "category": category, "from": from, "copyOf": copyOf])
        case .renameSubTemplate(let name, let to, let category, let updateReferences):
            try t.tag("renameSubTemplate", ["name": name, "to": to, "category": category, "updateReferences": updateReferences])
        case .removeSubTemplate(let name, let category):
            try t.tag("removeSubTemplate", ["name": name, "category": category])
        case .replaceBody(let from):
            try t.tag("replaceBody", ["from": from])
        case .insertControl(let v): try t.tag("insertControl", spread: v)
        case .setControl(let v): try t.tag("setControl", spread: v)
        case .removeControl(let id, let keepContent):
            try t.tag("removeControl", ["id": id, "keepContent": keepContent])
        case .fillControl(let id, let text, let image, let docx):
            try t.tag("fillControl", ["id": id, "text": text, "image": image, "docx": docx])
        case .setCheckbox(let id, let checked):
            try t.tag("setCheckbox", ["id": id, "checked": checked])
        case .wrapInRepeatingSection(let tag, let title):
            try t.tag("wrapInRepeatingSection", ["tag": tag, "title": title])
        case .removeElement(let v): try t.tag("removeElement", value: v)
        case .replaceInElements(let find, let replace, let `in`, let kind, let ids):
            try t.tag("replaceInElements", ["find": find, "replace": replace, "in": `in`, "kind": kind, "ids": ids])
        case .replaceFont(let from, let to, let scope, let bold, let italic):
            try t.tag("replaceFont", ["from": from, "to": to, "scope": scope, "bold": bold, "italic": italic])
        case .markElements(let marks, let clear):
            try t.tag("markElements", ["marks": marks, "clear": clear])
        case .setProtectedPictureControls(let tagPrefixes):
            try t.tag("setProtectedPictureControls", ["tagPrefixes": tagPrefixes])
        case .activatePictureControl(let element):
            try t.tag("activatePictureControl", ["element": element])
        case .writeDataPart(let namespace, let itemID, let xml):
            try t.tag("writeDataPart", ["namespace": namespace, "itemId": itemID, "xml": xml])
        case .removeDataPart(let namespace, let itemID):
            try t.tag("removeDataPart", ["namespace": namespace, "itemId": itemID])
        case .setTrackChanges(let v): try t.tag("setTrackChanges", value: v)
        case .setMarkupView(let v): try t.tag("setMarkupView", value: v)
        case .acceptRevision(let v): try t.tag("acceptRevision", value: v)
        case .rejectRevision(let v): try t.tag("rejectRevision", value: v)
        case .acceptRevisionsInSelection: try t.tag("acceptRevisionsInSelection")
        case .rejectRevisionsInSelection: try t.tag("rejectRevisionsInSelection")
        case .acceptAllRevisions(let author):
            try t.tag("acceptAllRevisions", ["author": author])
        case .rejectAllRevisions(let author):
            try t.tag("rejectAllRevisions", ["author": author])
        case .selectRevision(let v): try t.tag("selectRevision", value: v)
        case .addComment(let v): try t.tag("addComment", value: v)
        case .replyToComment(let to, let body):
            try t.tag("replyToComment", ["to": to, "body": body])
        case .editComment(let id, let body):
            try t.tag("editComment", ["id": id, "body": body])
        case .deleteComment(let v): try t.tag("deleteComment", value: v)
        case .resolveComment(let id, let done):
            try t.tag("resolveComment", ["id": id, "done": done])
        case .setActiveComment(let v): try t.tag("setActiveComment", value: v)
        case .selectComment(let v): try t.tag("selectComment", value: v)
        case .setAuthorColor(let author, let color):
            try t.tag("setAuthorColor", ["author": author, "color": color])
        case .setZoom(let v): try t.tag("setZoom", value: v)
        case .setViewOptions(let v): try t.tag("setViewOptions", spread: v)
        case .setMode(let v): try t.tag("setMode", value: v)
        case .setRulerTabType(let v): try t.tag("setRulerTabType", value: v)
        case .showTabTypes: try t.tag("showTabTypes")
        case .showTabStop: try t.tag("showTabStop")
        case .setAutoCorrect(let v): try t.tag("setAutoCorrect", spread: v)
        case .revertAutoCorrect: try t.tag("revertAutoCorrect")
        case .neverAutoCorrect: try t.tag("neverAutoCorrect")
        case .showAutoCorrectOptions: try t.tag("showAutoCorrectOptions")
        case .closePopover: try t.tag("closePopover")
        case .describeFormatting: try t.tag("describeFormatting")
        }
    }
}

/// A command's id: its catalog id, its key in the state's command map, and its command's `type`, all one spelling
/// (`setBold`).
public enum CommandID: String, Hashable, Sendable, Codable, CaseIterable, CodingKeyRepresentable {
    /// Types `value` (non-empty) at the selection, replacing it. Undo: typing.
    case insertText
    /// Starts a new paragraph (Enter). Undo: one step.
    case insertParagraph
    /// A line break (Shift+Enter). Undo: one step.
    case insertLineBreak
    /// A page break, in the body outside tables. Undo: one step.
    case insertPageBreak
    /// A column break, in the body outside tables. Undo: one step.
    case insertColumnBreak
    /// A section break of the given kind, in the body outside tables. Undo: one step.
    case insertSectionBreak
    /// A tab character. Undo: typing.
    case insertTab
    /// One character, optionally in a symbol font. Undo: one step.
    case insertSymbol
    /// Deletes backward or forward by a character or a word, or the selection. Undo: typing.
    case delete
    /// Cuts the selection to the clipboard. Undo: one step.
    case cut
    /// Copies the selection (also when read only). No undo step.
    case copy
    /// Pastes the system clipboard, formatted or as text. Undo: one step.
    case paste
    /// Pastes `value` as plain text. Undo: one step.
    case pasteText
    /// Undoes the last step. State value: the undo label.
    case undo
    /// Redoes the last undone step. State value: the redo label.
    case redo
    /// Redoes, or repeats the last repeatable action (F4). Undo: one step.
    case `repeat`
    /// Selects the whole story. Undo: view.
    case selectAll
    /// Selects the unit at the caret. Undo: view.
    case select
    /// Moves or extends the selection. Undo: view.
    case moveSelection
    /// Moves to the next or previous cell; `next` from the last cell adds a row (then one step).
    case moveToCell
    /// Normal, extend (F8) or block (Ctrl+Shift+F8) selection. Undo: view.
    case setSelectionMode
    /// Back to an earlier edit location (Shift+F5). Undo: view.
    case goBack
    /// Leaves a header or footer, a text box, or object selection (Esc). Undo: view.
    case escape
    /// Selects the next or previous floating object. Undo: view.
    case selectNextObject
    /// Moves the caret to the start of page `value` (1…count). Undo: view.
    case goToPage
    /// Bold on or off. Undo: one step.
    case setBold
    /// Italic on or off. Undo: one step.
    case setItalic
    /// The underline style (`none` removes it). Undo: one step.
    case setUnderline
    /// Strikethrough on or off. Undo: one step.
    case setStrikethrough
    /// Double strikethrough on or off. Undo: one step.
    case setDoubleStrikethrough
    /// Subscript on or off. Undo: one step.
    case setSubscript
    /// Superscript on or off. Undo: one step.
    case setSuperscript
    /// Small capitals on or off. Undo: one step.
    case setSmallCaps
    /// All capitals on or off. Undo: one step.
    case setAllCaps
    /// Hidden text on or off. Undo: one step.
    case setHidden
    /// The font family (non-empty). Undo: one step.
    case setFont
    /// The font size (1…1638 pt, in 0.5 steps). Undo: one step.
    case setFontSize
    /// The next size up in the standard list. Undo: one step.
    case growFont
    /// The next size down in the standard list. Undo: one step.
    case shrinkFont
    /// The font colour, or automatic. Undo: one step.
    case setFontColor
    /// The highlight colour, or none (`null`). Undo: one step.
    case setHighlight
    /// Character spacing, expanded when positive (−1584…1584 pt). Undo: one step.
    case setCharacterSpacing
    /// The raised (positive) or lowered position (−1584…1584 pt). Undo: one step.
    case setCharacterPosition
    /// Removes direct character formatting. Undo: one step.
    case clearFormatting
    /// Changes the case of the selected text. Undo: one step.
    case changeCase
    /// Holds the selection's formatting for `pasteFormatting` (the format painter). Undo: view.
    case copyFormatting
    /// Applies the held formatting to the selection. Undo: one step.
    case pasteFormatting
    /// The selected paragraphs' alignment. Undo: one step.
    case setAlignment
    /// The selected paragraphs' line spacing. Undo: one step.
    case setLineSpacing
    /// Space before and after the selected paragraphs (0…1584 pt, or auto). Undo: one step.
    case setParagraphSpacing
    /// The selected paragraphs' indents. Undo: one step.
    case setIndents
    /// Indents the selected paragraphs to the next stop. Undo: one step.
    case increaseIndent
    /// Outdents the selected paragraphs to the previous stop. Undo: one step.
    case decreaseIndent
    /// An existing paragraph style. Undo: one step.
    case setParagraphStyle
    /// Changes a style to the selection's formatting: its paragraph's and its first run's direct formatting become the
    /// style's. Undo: one step.
    case updateStyleToSelection
    /// Deletes a style the document made: its paragraphs take the default paragraph style, its runs lose it, and the
    /// styles based on it take its base. Built-in styles can't be deleted. Undo: one step.
    case deleteStyle
    /// Sets a style's list flags: visibility, priority, gallery, lock. Undo: one step.
    case setStyleMeta
    /// Sets the document's default formatting (Manage Styles' Set Defaults). Undo: one step.
    case setDocumentDefaults
    /// Bullets, numbering, or no list (`null`). Undo: one step.
    case setList
    /// One list level deeper or shallower, in a list. Undo: one step.
    case setListLevel
    /// Defines the selection's list levels (Define New Bullet, Number Format, Multilevel List): for its whole list,
    /// from this paragraph on, or for the selected paragraphs. Outside a list, makes a new one. Undo: one step.
    case setListDefinition
    /// Set Numbering Value: starts a new list at a value, or continues the previous list. Undo: one step.
    case setNumberingValue
    /// Applies a list gallery entry (`listGalleries`): a library or recent entry defines the selection's list (the
    /// current level for bullets and numbering, every level for a multilevel list), or makes a new one; a document
    /// list or list style continues that list. Undo: one step.
    case applyListEntry
    /// Restarts the list at this paragraph, at its level's start. Undo: one step.
    case restartNumbering
    /// Continues the previous list from this paragraph. Undo: one step.
    case continueNumbering
    /// Adjust List Indents: the selection's list level's number position, text indent and what follows the number,
    /// for its whole list. Undo: one step.
    case adjustListIndents
    /// Keep with next, keep lines together, page break before and widow control. Undo: one step.
    case setParagraphFlow
    /// A box, a bottom border or none (a ½ pt single line). Undo: one step.
    case setParagraphBorders
    /// Paragraph shading, or none (`null`). Undo: one step.
    case setParagraphShading
    /// The selected paragraphs' border lines: each edge set, removed (`null`) or kept (absent). Undo: one step.
    case setParagraphBorderLines
    /// Shading of the selected paragraphs or text: its fill, pattern and pattern color. Undo: one step.
    case setShading
    /// The selected text's border: a box of one line, with a shadow or not, or none. Undo: one step.
    case setTextBorder
    /// The page borders of the selection's section, or of every section: each edge set, removed (`null`) or kept
    /// (absent), what they're measured from, and the pages they show on. Undo: one step.
    case setPageBorders
    /// The frame the selection is in: its wrapping, size, position and distance from text. Undo: one step.
    case setFrame
    /// Removes the frame the selection is in: its paragraphs go back in the text, before the paragraph it was
    /// anchored to. Undo: one step.
    case removeFrame
    /// Adds a tab stop to the selected paragraphs, replacing one at the same position. Undo: one step.
    case addTabStop
    /// Moves the stop at `from` (within 0.5 pt) to `to`. Undo: one step.
    case moveTabStop
    /// Deletes the stop at `value`; an inherited stop is cleared in the paragraph. Undo: one step.
    case deleteTabStop
    /// Clears the paragraphs' own and inherited stops. Undo: one step.
    case clearTabStops
    /// The document's default tab spacing (1…1584 pt). Undo: one step.
    case setDefaultTabStop
    /// The selection's sections' margins (0…1584 pt; left + right + gutter < page width). Undo: one step.
    case setMargins
    /// Portrait or landscape. Undo: one step.
    case setOrientation
    /// The paper size, in portrait (36…1584 pt). Undo: one step.
    case setPaperSize
    /// Text columns (1…45), their spacing and separator. Undo: one step.
    case setColumns
    /// The header and footer distances from the page edge. Undo: one step.
    case setHeaderFooterDistance
    /// A different first page header and footer. Undo: one step.
    case setDifferentFirstPage
    /// Different odd and even page headers and footers. Undo: one step.
    case setDifferentOddEvenPages
    /// Moves a text column boundary (from the ruler). Undo: one step.
    case moveColumnBoundary
    /// Edits the caret's page's header, creating it when the page has none. Undo: one step when it creates one.
    case editHeader
    /// Edits the caret's page's footer, creating it when the page has none. Undo: one step when it creates one.
    case editFooter
    /// Back to the body. Undo: view.
    case closeHeaderFooter
    /// A page number field at the caret, or in the header or footer. Undo: one step.
    case insertPageNumber
    /// A table (1…32767 rows, 1…63 columns). Undo: one step.
    case insertTable
    /// A picture (PNG, JPEG, GIF, BMP, TIFF). Undo: one step.
    case insertPicture
    /// A text box. Undo: one step.
    case insertTextBox
    /// A floating shape at the caret's paragraph, in front of the text, with the theme's default style. Undo: one step.
    case insertShape
    /// A footnote at the caret. Undo: one step.
    case insertFootnote
    /// An endnote at the caret. Undo: one step.
    case insertEndnote
    /// A hyperlink to `address`; `text` replaces the selection (default: the selection, or the address). Undo: one step.
    case insertHyperlink
    /// Changes the link at the selection. Undo: one step.
    case editHyperlink
    /// Removes the link at the selection, keeping its text. Undo: one step.
    case removeHyperlink
    /// A field with `code`; result `{ id }`. With a selection and an empty code, wraps it. Undo: one step.
    case insertField
    /// Rows above or below the selection (1…32767). Undo: one step.
    case insertRows
    /// Columns left or right of the selection (1…63). Undo: one step.
    case insertColumns
    /// Deletes the selected rows. Undo: one step.
    case deleteRows
    /// Deletes the selected columns. Undo: one step.
    case deleteColumns
    /// Deletes the table. Undo: one step.
    case deleteTable
    /// Deletes the selected cells, shifting the rest up or left. Undo: one step.
    case deleteCells
    /// Merges a rectangular block of cells. Undo: one step.
    case mergeCells
    /// Splits the selected cells (1…63 columns, 1…32767 rows). Undo: one step.
    case splitCells
    /// Splits the table above the selection. Undo: one step.
    case splitTable
    /// Gives the selected rows equal heights. Undo: one step.
    case distributeRows
    /// Gives the selected columns equal widths. Undo: one step.
    case distributeColumns
    /// Moves the selected rows up or down. Undo: one step.
    case moveRows
    /// Fits the columns to their contents, the window, or fixed widths. Undo: one step.
    case autoFitColumns
    /// The selected cells' vertical alignment. Undo: one step.
    case setCellAlignment
    /// Cell shading, or none (`null`). Undo: one step.
    case setCellShading
    /// Borders on the selected cells' edges. Undo: one step.
    case setCellBorders
    /// The selected cells' margins (0…1584 pt). Undo: one step.
    case setCellMargins
    /// Repeat the selected rows as header rows. Undo: one step.
    case setHeaderRow
    /// The selected rows' height, or automatic. Undo: one step.
    case setRowHeight
    /// Whether the selected rows can break across pages. Undo: one step.
    case setRowCanBreak
    /// The selected columns' width (1…1584 pt). Undo: one step.
    case setColumnWidth
    /// Moves a column boundary of the caret's row (from the rulers and cell edges). Undo: one step.
    case moveTableColumnBoundary
    /// Moves a row boundary of the caret's table. Undo: one step.
    case moveTableRowBoundary
    /// The table's preferred width. Undo: one step.
    case setTableWidth
    /// The table's alignment on the page. Undo: one step.
    case setTableAlignment
    /// The table's indent (−1584…1584 pt). Undo: one step.
    case setTableIndent
    /// An existing table style. Undo: one step.
    case setTableStyle
    /// Which parts of the table style apply. Undo: one step.
    case setTableStyleOptions
    /// Where the table is placed, and what it is measured from: a table without a position becomes positioned (text
    /// wraps around it); none (`null`) puts it back in the text. Undo: one step.
    case setTablePosition
    /// The selected object's size (1…1584 pt). Undo: one step.
    case setObjectSize
    /// The selected picture at its original size. Undo: one step.
    case resetObjectSize
    /// How text wraps around the selected object. Undo: one step.
    case setObjectWrap
    /// Locks a floating object's anchor. Undo: one step.
    case setAnchorLocked
    /// Where the selected floating object is, and what it is measured from. Undo: one step.
    case setObjectPosition
    /// The sides text wraps on around the selected floating object, and its distance from the text. Undo: one step.
    case setObjectWrapOptions
    /// The selected object's alternative text. Undo: one step.
    case setAltText
    /// Deletes the selected object. Undo: one step.
    case deleteObject
    /// The selected shape's preset geometry; its text, fill, outline and effects stay and its adjust values reset.
    /// Undo: one step.
    case setShapePreset
    /// One of the selected shape's adjust values. Undo: one step.
    case setShapeAdjust
    /// The selected objects' fill (`fromStyle`: the style reference's again). Undo: one step.
    case setShapeFill
    /// The selected objects' outline; what isn't given is kept. Undo: one step.
    case setShapeLine
    /// The selected objects' shadows, glow, soft edge and blur; what isn't given is kept, `null` removes. Undo: one
    /// step.
    case setShapeEffects
    /// The selected shape's text insets, anchor, autofit, wrapping and direction. Undo: one step.
    case setShapeText
    /// The selected picture's transparency, greyscale, brightness and contrast. Undo: one step.
    case setPictureAdjust
    /// The selected objects' rotation (0…360°, clockwise). Undo: one step.
    case setObjectRotation
    /// Flips the selected objects. Undo: one step.
    case flipObject
    /// Moves the selected floating object in the paint order, or behind or in front of the text. Undo: one step.
    case arrangeObject
    /// Groups the selected floating objects (two or more, one story). Undo: one step.
    case groupObjects
    /// Ungroups the selected group. Undo: one step.
    case ungroupObjects
    /// Selects the next or previous child of the selected group. Undo: view.
    case selectGroupChild
    /// Changes a field's code or result. Undo: one step.
    case setField
    /// The results the application computed; `token` joins the update a `fieldsUpdateRequested` started. Undo: one step.
    case setFieldResults
    /// Updates fields (default: the selection's), asking the application for the ones it computes. Undo: one step.
    case updateFields
    /// Locks or unlocks fields. Undo: one step.
    case setFieldsLocked
    /// Replaces fields with their results. Undo: one step.
    case unlinkFields
    /// Shows field codes or results. Undo: view.
    case showFieldCodes
    /// What design mode shows for each field code. Undo: view.
    case setFieldLabels
    /// Which fields are sub-template references: replaces the rules; an empty list turns sub-templates off. Undo: view.
    case setSubTemplateRules
    /// The application's own field types (compared without case), replacing those set before: their fields keep their
    /// results, show their labels in design mode and are never read as bookmark references. Undo: view.
    case setApplicationFieldTypes
    /// Shows references' sub-template bodies in their place, or their fields (default: every reference). Undo: view.
    case setSubTemplateView
    /// Edits a sub-template's body in this editor, on its own pages; the template shows again on Close Sub-template.
    /// Undo: view.
    case editSubTemplate
    /// Back to the template from a sub-template, at the template's selection from before. Undo: view.
    case closeSubTemplate
    /// A new sub-template, empty or with a DOCX's body. Undo: one step.
    case createSubTemplate
    /// Renames a sub-template, and optionally every reference to it. Undo: one step.
    case renameSubTemplate
    /// Removes a sub-template; its references no longer resolve. Undo: one step.
    case removeSubTemplate
    /// The template's body becomes a DOCX's body, on its last section's page; its sub-templates, data parts,
    /// settings, styles, headers and footers stay. Closes a sub-template being edited first. Undo: one step.
    case replaceBody
    /// A content control at the selection; result `{ id }`. Undo: one step.
    case insertControl
    /// Changes a content control's properties. Undo: one step.
    case setControl
    /// Removes a content control, keeping or deleting its content. Undo: one step.
    case removeControl
    /// Fills a control with exactly one of text, an image or a document. Undo: one step.
    case fillControl
    /// Checks or clears a checkbox. Undo: one step.
    case setCheckbox
    /// Wraps the selection in a repeating section; result `{ id }`. Undo: one step.
    case wrapInRepeatingSection
    /// Removes a field, or a control with its content. Undo: one step.
    case removeElement
    /// Replaces text in element codes, tags or titles, case-sensitive; result `{ changed }`. Undo: one step.
    case replaceInElements
    /// Rewrites a font family's name in the document; result `{ changed, skipped }`. Undo: one step.
    case replaceFont
    /// Wavy marks with a tooltip on elements. Undo: view.
    case markElements
    /// The tag prefixes whose picture controls keep their pictures (compared without case), replacing those set before:
    /// editing can't delete, paste over or replace such a picture; `fillControl` and `setControl` still change it, and
    /// in design mode deleting it removes its whole control. Undo: view.
    case setProtectedPictureControls
    /// Selects a picture control (the selected one when none is named) and emits `pictureControlActivated`, in any
    /// mode. Undo: none.
    case activatePictureControl
    /// Writes a custom XML data part (well-formed); result `{ itemId }`. Undo: one step.
    case writeDataPart
    /// Removes a custom XML data part. Undo: one step.
    case removeDataPart
    /// Track changes on or off. Undo: one step.
    case setTrackChanges
    /// How revisions show. Undo: view.
    case setMarkupView
    /// Accepts a revision (a move as a pair). Undo: one step.
    case acceptRevision
    /// Rejects a revision (a move as a pair). Undo: one step.
    case rejectRevision
    /// Accepts the revisions in the selection. Undo: one step.
    case acceptRevisionsInSelection
    /// Rejects the revisions in the selection. Undo: one step.
    case rejectRevisionsInSelection
    /// Accepts every revision, or one author's. Undo: one step.
    case acceptAllRevisions
    /// Rejects every revision, or one author's. Undo: one step.
    case rejectAllRevisions
    /// Selects the next or previous revision. Undo: view.
    case selectRevision
    /// A comment on the selection, or the word at the caret; result `{ id }`. Undo: one step.
    case addComment
    /// A reply in a thread; result `{ id }`. Undo: one step.
    case replyToComment
    /// Changes a comment's body. Undo: one step.
    case editComment
    /// Deletes a reply, or the whole thread when it's the first comment. Undo: one step.
    case deleteComment
    /// Marks a thread done or open. Undo: one step.
    case resolveComment
    /// The thread the application shows as active, or none. Undo: view.
    case setActiveComment
    /// Selects and reveals a comment's range. Undo: view.
    case selectComment
    /// The colour an author's revisions and comments show in. Undo: view.
    case setAuthorColor
    /// The zoom (10…500 percent), or a fit. Undo: view.
    case setZoom
    /// Formatting marks, gridlines and text boundaries. Undo: view.
    case setViewOptions
    /// Editing or design mode. Undo: view.
    case setMode
    /// What a click on the ruler adds; answers the tab type picker. Undo: view.
    case setRulerTabType
    /// Asks for the tab type picker (`popoverRequested`). Undo: view.
    case showTabTypes
    /// Asks for the options of the tab stop nearest the caret (`popoverRequested`). Undo: view.
    case showTabStop
    /// What typing corrects. Undo: none (a session setting).
    case setAutoCorrect
    /// Puts back what was typed in place of the correction just made. Undo: one step.
    case revertAutoCorrect
    /// Reverts the correction just made and never makes it again. Undo: one step (the revert).
    case neverAutoCorrect
    /// Asks for the options of the correction just made (`popoverRequested`). Undo: view.
    case showAutoCorrectOptions
    /// Dismisses the popover asked for (`popoverClosed`): what an application that closes it itself, on Esc or a
    /// click elsewhere, sends. With none open it does nothing. Undo: view.
    case closePopover
    /// Says the formatting at the caret to screen readers (paragraph style, font and size, emphasis, colour,
    /// highlight) as an announcement; changes nothing. Undo: view.
    case describeFormatting
}

/// A zoom: a percentage (10…500), or a fit the host computes. On the wire: the number, or the fit's name.
public enum Zoom: Hashable, Sendable, Codable {
    /// A percentage.
    case percent(Double)
    /// A fit.
    case fit(ZoomFit)

    public init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        if let p = try? c.decode(Double.self) { self = .percent(p) } else { self = .fit(try c.decode(ZoomFit.self)) }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .percent(let p): try c.encode(p)
        case .fit(let f): try c.encode(f)
        }
    }
}

/// What a command returns besides its commit, for the commands that create something.
///
/// The wire doesn't say which kind of id an `{ id }` is, so one decodes as `element`; a comment's id is the same
/// string as `.comment` would carry.
public enum CommandResult: Hashable, Sendable, Codable {
    /// A field, control or repeating section made: `{ id }`.
    case element(id: ElementID)
    /// A comment or reply made: `{ id }`.
    case comment(id: CommentID)
    /// A data part written: `{ itemId }`.
    case dataPart(itemID: String)
    /// What `replaceFont` rewrote: `{ changed, skipped }`.
    ///
    /// - Parameters:
    ///   - changed: The places rewritten: runs, paragraph marks, symbol characters, styles, document defaults,
    ///     numbering levels, theme font slots and font table entries.
    ///   - skipped: The places inside locked content controls, left as they were.
    case fontReplaced(changed: Int, skipped: Int)
    /// Elements changed by `replaceInElements`: `{ changed }`, how many.
    case changed(Int)

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: WireKey.self)
        if let id = try c.decodeIfPresent(ElementID.self, forKey: "id") {
            self = .element(id: id)
        } else if let item = try c.decodeIfPresent(String.self, forKey: "itemId") {
            self = .dataPart(itemID: item)
        } else if let skipped = try c.decodeIfPresent(Int.self, forKey: "skipped") {
            self = .fontReplaced(changed: try c.decode(Int.self, forKey: "changed"), skipped: skipped)
        } else {
            self = .changed(try c.decode(Int.self, forKey: "changed"))
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: WireKey.self)
        switch self {
        case .element(let id): try c.encode(id, forKey: "id")
        case .comment(let id): try c.encode(id, forKey: "id")
        case .dataPart(let item): try c.encode(item, forKey: "itemId")
        case .fontReplaced(let changed, let skipped):
            try c.encode(changed, forKey: "changed")
            try c.encode(skipped, forKey: "skipped")
        case .changed(let changed): try c.encode(changed, forKey: "changed")
        }
    }
}
