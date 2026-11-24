/** A length in points (1/72 inch). Every length in the API is in points. */
export type Points = number;

/** A colour as `#RRGGBB`. */
export type Color = `#${string}`;

/** Converts inches to points. */
export function inches(n: number): Points {
  return n * 72;
}

/** Converts centimetres to points. */
export function cm(n: number): Points {
  return (n * 72) / 2.54;
}

/** Converts millimetres to points. */
export function mm(n: number): Points {
  return (n * 72) / 25.4;
}

/** Returns `n` points: for call sites that state their unit. */
export function pt(n: number): Points {
  return n;
}

declare const brand: unique symbol;

/** A merge-designer element (a field or a content control). Stable while the document is open. */
export type ElementId = string & { readonly [brand]: 'element' };
/** A comment. Stable while the document is open. */
export type CommentId = string & { readonly [brand]: 'comment' };
/** A tracked revision. Stable while the document is open. */
export type RevisionId = string & { readonly [brand]: 'revision' };
/** A heading in the navigation list. */
export type HeadingId = string & { readonly [brand]: 'heading' };
/** A find match. */
export type MatchId = string & { readonly [brand]: 'match' };
/** A style. */
export type StyleId = string & { readonly [brand]: 'style' };
/**
 * A paragraph: a decimal string, stable while the document is open and the paragraph exists (a split keeps it on the
 * first half, a join keeps the first paragraph's), not saved in the file.
 */
export type ParagraphId = string & { readonly [brand]: 'paragraph' };
/**
 * A floating object: a decimal string, stable while the document is open and the object exists (undo restores it),
 * not saved in the file.
 */
export type FloatId = string & { readonly [brand]: 'float' };

/** A floating object, or one of the objects in a floating group. */
export interface ObjectId {
  /** The floating object. */
  float: FloatId;
  /** A group child's place: its index in the group, then in each group within it; absent for the floating object. */
  path?: number[];
}

/** A point in an object's box, as fractions of its width and height from its top left corner (beyond 0 to 1 outside). */
export interface BoxPoint {
  /** Across, from its left edge. */
  x: number;
  /** Down, from its top edge. */
  y: number;
}

/** A place in a paragraph's text. */
export interface TextPoint {
  /** The paragraph. */
  paragraph: ParagraphId;
  /**
   * The offset in the paragraph's text, in Unicode scalar values (not UTF-16 units): a field counts as its result, an
   * object or a note reference as one U+FFFC, and hidden and deleted text count. Past the end reads as the end.
   */
  offset: number;
}

/** A range of text. State gives the earlier end first; `setSelection` takes the ends either way round. */
export interface TextRange {
  /** Where it starts. */
  start: TextPoint;
  /** Where it ends. */
  end: TextPoint;
}

/** A rectangular block of a table's cells. */
export interface CellBlock {
  /** The start of the block's first cell's text: names the table. */
  start: TextPoint;
  /** The first row, from 0. */
  firstRow: number;
  /** The first grid column, from 0. */
  firstColumn: number;
  /** The last row. */
  lastRow: number;
  /** The last grid column. */
  lastColumn: number;
}

/** The way a line of text runs on the page: across it, or down or up it (vertical text). */
export type LineDirection = 'horizontal' | 'down' | 'up';

/** A point relative to the browser viewport, in CSS pixels, like `MouseEvent.clientX`. */
export interface ClientPoint {
  /** CSS pixels from the viewport's left edge. */
  clientX: number;
  /** CSS pixels from the viewport's top edge. */
  clientY: number;
}

/** The colours of what the editor draws over the document (`view.setOverlayColors`). The defaults are the light theme's; each theme has its own. The caret is always black or white, whichever stands out more on the page. */
export interface OverlayColors {
  /** The selection, blended under the text (multiply), so black text stays black. Default `'#9DC7F4'`, the accent's light tint. */
  selection: Color;
  /** A selected object's outline and handles. Default `'#0A74E4'`, the accent. */
  object: Color;
  /** The drop caret of a drag. Default `'#62656C'`. */
  drop: Color;
  /** Formatting marks and text boundaries. Default `'#0A74E4'`. */
  marks: Color;
  /** The markup pane's leader lines. Default `'#62656C'`. */
  leaders: Color;
}

/** What a mounted editor's frames have cost so far (`view.frameStats()`): for benchmarks and tests. */
export interface FrameStats {
  /** Frames drawn. */
  frames: number;
  /** Page recordings made: an unchanged page is recorded once. */
  pagesRecorded: number;
  /** Pages drawn again from their recordings. */
  pagesReused: number;
  /** The page recordings kept now, bytes. */
  recordingBytes: number;
  /** The last frame's time, ms. */
  lastFrameMs: number;
  /** The 95th percentile of the last 120 frames' times, ms. */
  p95FrameMs: number;
}

/** A rectangle relative to the browser viewport, in CSS pixels. */
export interface ClientRect {
  /** The left edge. */
  left: number;
  /** The top edge. */
  top: number;
  /** The width. */
  width: number;
  /** The height. */
  height: number;
}

/** A point relative to the editor's balloon layer, in CSS pixels: it scrolls and zooms with the document. */
export interface ContentPoint {
  /** CSS pixels from the layer's left edge. */
  x: number;
  /** CSS pixels from the layer's top edge. */
  y: number;
}

/** Who makes changes: recorded on transactions, revisions and comments. */
export interface Author {
  /** The display name. */
  name: string;
  /** The initials shown on comments. */
  initials?: string;
  /** The identity in a directory, written to the document with the author. */
  identity?: { provider: string; userId: string };
}

/** A recoverable fidelity issue found while reading or writing a document. Aggregated per code and part. */
export interface Warning {
  /** What kind of issue it is. */
  code:
    | 'unsupported-element'
    | 'image-undecodable'
    | 'picture-bullet-undecodable'
    | 'font-unresolved'
    | 'rewritten-content'
    | 'pdf-font-fallback'
    | 'pdf-archival-claim-dropped'
    | 'revision-id-missing'
    | 'range-unpaired'
    | 'author-missing'
    | 'comment-unanchored'
    | 'comment-reference-orphan'
    | 'unknown-preset'
    | 'glossary-unreadable'
    | 'duplicate-building-block'
    | 'sub-template-cycle'
    | 'sub-template-too-deep'
    | 'sub-template-near-match'
    | 'pdf-figure-without-alt';
  /** A readable description. */
  message: string;
  /** How many times it occurred. */
  count: number;
  /** Where it occurred, when known. */
  location?: { part: string; paragraph?: number };
}

/**
 * A custom document property: its value as text (a decimal number, `true` or `false`, or an ISO 8601 date and time
 * for those kinds).
 */
export interface CustomProperty {
  /** Its name. */
  name: string;
  /** What it holds. */
  kind: 'text' | 'number' | 'yesNo' | 'date';
  /** Its value. */
  value: string;
}

/** The document's properties and variables; a property the document lacks is absent. */
export interface DocumentProperties {
  /** The title. */
  title?: string;
  /** The subject. */
  subject?: string;
  /** The author. */
  author?: string;
  /** The keywords. */
  keywords?: string;
  /** The comments. */
  comments?: string;
  /** The category. */
  category?: string;
  /** The status. */
  status?: string;
  /** Who saved it last. */
  lastModifiedBy?: string;
  /** How many times it was saved. */
  revision?: string;
  /** When it was made (ISO 8601). */
  created?: string;
  /** When it was last saved (ISO 8601). */
  modified?: string;
  /** When it was last printed (ISO 8601). */
  lastPrinted?: string;
  /** The company. */
  company?: string;
  /** The manager. */
  manager?: string;
  /** The base relative hyperlinks resolve against. */
  hyperlinkBase?: string;
  /** The custom properties. */
  custom: CustomProperty[];
  /** The document variables. */
  variables: { name: string; value?: string }[];
}

/** Where the selection is on screen, client px over the pages in view. */
export interface SelectionBounds {
  /** Each selected line's or cell's box; empty for a caret. */
  rects: ClientRect[];
  /** Each selected object's frame: its box before rotation, the rotation (degrees clockwise), the upright box around it. */
  frames: { rect: ClientRect; rotation: Degrees; bounds: ClientRect }[];
  /** The box around every rect and frame; absent when nothing selected is in view. */
  bounds?: ClientRect;
}

/** What a drag of the selection out of the editor carries. */
export interface DragSource {
  /** The content as a copy puts it on the clipboard: `text/plain`, `text/html` (objects drawn), a dragged object's picture. */
  data: ClipboardContent;
  /** The drop may move the text: the selection can be deleted. */
  canMove: boolean;
}

/** Clipboard content in the formats the engine reads and writes. */
export interface ClipboardContent {
  /** `text/plain`. */
  text: string;
  /** `text/html`. */
  html?: string;
  /** A token naming the engine's own copy: a paste in any editor of the page keeps everything. */
  internal?: string;
  /** A picture: a copy of an object's (`image/png`), or what the clipboard holds (PNG, JPEG, GIF, BMP, TIFF, SVG, EMF, WMF). */
  image?: { mime: string; data: Uint8Array };
}

/** A footnote or endnote, as a notes list shows it. */
export interface NoteInfo {
  /** Footnote or endnote. */
  kind: 'footnote' | 'endnote';
  /** Its id as the document numbers it: stable while the document is open. */
  id: number;
  /** Its reference mark: its custom mark, else its number as laid out (empty before layout reaches it). */
  mark: string;
  /** Its mark is a custom one. */
  customMark: boolean;
  /** Its text, plain (fields as their results), at most 1,024 characters and then `…`. */
  text: string;
  /** The page showing its reference; absent while nothing refers to it or layout hasn't reached it. */
  page?: number;
}

/** One note separator's text: its plain text (paragraphs `\n` apart), and whether it is the built-in line. */
export interface NoteSeparatorState {
  /** Its plain text. */
  text: string;
  /** It is the built-in line. */
  builtin: boolean;
}

/** The separators of footnotes or endnotes. */
export interface NoteSeparators {
  /** Above the notes on the page they start. */
  separator: NoteSeparatorState;
  /** Above the notes continued from the page before. */
  continuationSeparator: NoteSeparatorState;
  /** Below notes that continue on the next page; absent when there is none. */
  continuationNotice?: NoteSeparatorState;
}

/** An editable range, as Restrict Editing lists it: the permissions over one stretch of the document. */
export interface EditableRangeInfo {
  /** Its permissions' ids, one per editor. */
  ids: string[];
  /** Who may edit it. */
  editors: ({ group: string } | { user: string })[];
  /** The page it starts on; absent before layout reaches it. */
  page?: number;
}

/**
 * A page thumbnail: the pixels (RGBA, premultiplied, rows packed, the size asked for: `new ImageData(pixels, w, h)`
 * draws them), and the size the page is drawn at, centred in them.
 */
export interface PageThumbnail {
  /** The pixels. */
  pixels: Uint8ClampedArray;
  /** The page's drawn width. */
  width: number;
  /** The page's drawn height. */
  height: number;
}

/** The undo and redo lists. */
export interface HistoryLabels {
  /** The steps undo would undo, most recent first. */
  undo: string[];
  /** The steps redo would redo, next first. */
  redo: string[];
  /** There are more undo steps than listed. */
  moreUndo: boolean;
  /** There are more redo steps than listed. */
  moreRedo: boolean;
}

/** One cell of the theme colour grid. */
export interface ThemeSwatch {
  /** The colour as commands take it: the theme colour with its tint or shade. */
  colour: ColourValue;
  /** What it shows. */
  rgb: Color;
  /** Its name: `'Accent 1'`, `'Accent 1, Lighter 40%'`. */
  label: string;
}

/** A column of the theme colour grid: the theme colour, then its five lighter and darker variants. */
export interface ThemeColumn {
  /** The theme colour. */
  theme: ThemeColour;
  /** The theme colour itself, then its variants, top to bottom. */
  swatches: ThemeSwatch[];
}

/** Word, character, paragraph, line and page counts, computed on request. */
export interface WordCount {
  /** Words. */
  words: number;
  /** Characters, without spaces. */
  characters: number;
  /** Characters, with spaces. */
  charactersWithSpaces: number;
  /** Paragraphs. */
  paragraphs: number;
  /** Lines. */
  lines: number;
  /** Pages. */
  pages: number;
}

/** An angle in degrees, clockwise. */
export type Degrees = number;

/** A preset geometry by its ECMA-376 name (`'wedgeRectCallout'`): the 187 presets, in alphabetical order. */
export type ShapePreset =
  | 'accentBorderCallout1'
  | 'accentBorderCallout2'
  | 'accentBorderCallout3'
  | 'accentCallout1'
  | 'accentCallout2'
  | 'accentCallout3'
  | 'actionButtonBackPrevious'
  | 'actionButtonBeginning'
  | 'actionButtonBlank'
  | 'actionButtonDocument'
  | 'actionButtonEnd'
  | 'actionButtonForwardNext'
  | 'actionButtonHelp'
  | 'actionButtonHome'
  | 'actionButtonInformation'
  | 'actionButtonMovie'
  | 'actionButtonReturn'
  | 'actionButtonSound'
  | 'arc'
  | 'bentArrow'
  | 'bentConnector2'
  | 'bentConnector3'
  | 'bentConnector4'
  | 'bentConnector5'
  | 'bentUpArrow'
  | 'bevel'
  | 'blockArc'
  | 'borderCallout1'
  | 'borderCallout2'
  | 'borderCallout3'
  | 'bracePair'
  | 'bracketPair'
  | 'callout1'
  | 'callout2'
  | 'callout3'
  | 'can'
  | 'chartPlus'
  | 'chartStar'
  | 'chartX'
  | 'chevron'
  | 'chord'
  | 'circularArrow'
  | 'cloud'
  | 'cloudCallout'
  | 'corner'
  | 'cornerTabs'
  | 'cube'
  | 'curvedConnector2'
  | 'curvedConnector3'
  | 'curvedConnector4'
  | 'curvedConnector5'
  | 'curvedDownArrow'
  | 'curvedLeftArrow'
  | 'curvedRightArrow'
  | 'curvedUpArrow'
  | 'decagon'
  | 'diagStripe'
  | 'diamond'
  | 'dodecagon'
  | 'donut'
  | 'doubleWave'
  | 'downArrow'
  | 'downArrowCallout'
  | 'ellipse'
  | 'ellipseRibbon'
  | 'ellipseRibbon2'
  | 'flowChartAlternateProcess'
  | 'flowChartCollate'
  | 'flowChartConnector'
  | 'flowChartDecision'
  | 'flowChartDelay'
  | 'flowChartDisplay'
  | 'flowChartDocument'
  | 'flowChartExtract'
  | 'flowChartInputOutput'
  | 'flowChartInternalStorage'
  | 'flowChartMagneticDisk'
  | 'flowChartMagneticDrum'
  | 'flowChartMagneticTape'
  | 'flowChartManualInput'
  | 'flowChartManualOperation'
  | 'flowChartMerge'
  | 'flowChartMultidocument'
  | 'flowChartOfflineStorage'
  | 'flowChartOffpageConnector'
  | 'flowChartOnlineStorage'
  | 'flowChartOr'
  | 'flowChartPredefinedProcess'
  | 'flowChartPreparation'
  | 'flowChartProcess'
  | 'flowChartPunchedCard'
  | 'flowChartPunchedTape'
  | 'flowChartSort'
  | 'flowChartSummingJunction'
  | 'flowChartTerminator'
  | 'foldedCorner'
  | 'frame'
  | 'funnel'
  | 'gear6'
  | 'gear9'
  | 'halfFrame'
  | 'heart'
  | 'heptagon'
  | 'hexagon'
  | 'homePlate'
  | 'horizontalScroll'
  | 'irregularSeal1'
  | 'irregularSeal2'
  | 'leftArrow'
  | 'leftArrowCallout'
  | 'leftBrace'
  | 'leftBracket'
  | 'leftCircularArrow'
  | 'leftRightArrow'
  | 'leftRightArrowCallout'
  | 'leftRightCircularArrow'
  | 'leftRightRibbon'
  | 'leftRightUpArrow'
  | 'leftUpArrow'
  | 'lightningBolt'
  | 'line'
  | 'lineInv'
  | 'mathDivide'
  | 'mathEqual'
  | 'mathMinus'
  | 'mathMultiply'
  | 'mathNotEqual'
  | 'mathPlus'
  | 'moon'
  | 'noSmoking'
  | 'nonIsoscelesTrapezoid'
  | 'notchedRightArrow'
  | 'octagon'
  | 'parallelogram'
  | 'pentagon'
  | 'pie'
  | 'pieWedge'
  | 'plaque'
  | 'plaqueTabs'
  | 'plus'
  | 'quadArrow'
  | 'quadArrowCallout'
  | 'rect'
  | 'ribbon'
  | 'ribbon2'
  | 'rightArrow'
  | 'rightArrowCallout'
  | 'rightBrace'
  | 'rightBracket'
  | 'round1Rect'
  | 'round2DiagRect'
  | 'round2SameRect'
  | 'roundRect'
  | 'rtTriangle'
  | 'smileyFace'
  | 'snip1Rect'
  | 'snip2DiagRect'
  | 'snip2SameRect'
  | 'snipRoundRect'
  | 'squareTabs'
  | 'star10'
  | 'star12'
  | 'star16'
  | 'star24'
  | 'star32'
  | 'star4'
  | 'star5'
  | 'star6'
  | 'star7'
  | 'star8'
  | 'straightConnector1'
  | 'stripedRightArrow'
  | 'sun'
  | 'swooshArrow'
  | 'teardrop'
  | 'trapezoid'
  | 'triangle'
  | 'upArrow'
  | 'upArrowCallout'
  | 'upDownArrow'
  | 'upDownArrowCallout'
  | 'uturnArrow'
  | 'verticalScroll'
  | 'wave'
  | 'wedgeEllipseCallout'
  | 'wedgeRectCallout'
  | 'wedgeRoundRectCallout';

/** A theme colour, in the theme's order. */
export type ThemeColour =
  | 'text1'
  | 'background1'
  | 'text2'
  | 'background2'
  | 'accent1'
  | 'accent2'
  | 'accent3'
  | 'accent4'
  | 'accent5'
  | 'accent6'
  | 'hyperlink'
  | 'followedHyperlink';

/**
 * What a colour transform does: `tint` and `shade` go towards white and black by `1 - value`; `lumMod`, `satMod`,
 * `hueMod` and `alphaMod` multiply the luminance, saturation, hue and opacity by `value`; `lumOff`, `satOff` and
 * `hueOff` add `value` (degrees for `hueOff`); `alpha` sets the opacity.
 */
export type TransformKind = 'tint' | 'shade' | 'lumMod' | 'lumOff' | 'satMod' | 'satOff' | 'hueMod' | 'hueOff' | 'alpha' | 'alphaMod';

/** One colour transform. */
export interface ColourTransform {
  /** What it does. */
  kind: TransformKind;
  /** How much: a fraction (0.75), degrees for `hueOff`. */
  value: number;
}

/** A colour as written: exactly one base (a theme colour, an RGB value or a preset colour name), then its transforms. */
export interface ColourValue {
  /** A theme colour. */
  theme?: ThemeColour;
  /** An RGB colour. */
  rgb?: Color;
  /** A preset colour by its CSS name (`'cornflowerBlue'`). */
  preset?: string;
  /** The transforms, applied in order. */
  transforms?: ColourTransform[];
}

/** A gradient's shape: along a direction, circles or rectangles from the centre, or following the shape's outline. */
export type GradientShape = 'linear' | 'radial' | 'rectangular' | 'path';

/** A gradient stop. */
export interface GradientStop {
  /** 0 to 1 along the gradient. */
  position: number;
  /** The colour there. */
  colour: ColourValue;
}

/**
 * A fill: none, one colour, a gradient of two or more stops (a linear one's `angle`, 0 left to right), a two-colour
 * pattern by its ECMA-376 name (`'pct50'`, `'dkDnDiag'`), a picture (PNG, JPEG, GIF, BMP; tiled at its own size, or
 * stretched), or `fromStyle`: the style reference's fill, no explicit one. A gradient's `focus` is where it starts, as
 * the fractions (0 to 1) of the shape's left, top, right and bottom edges it is inset from (`path` gradients; default
 * the centre).
 */
export type FillValue =
  | { type: 'none' }
  | { type: 'solid'; colour: ColourValue }
  | { type: 'gradient'; kind: GradientShape; angle?: Degrees; stops: GradientStop[]; focus?: [number, number, number, number] }
  | { type: 'pattern'; preset: string; foreground: ColourValue; background: ColourValue }
  | {
      type: 'picture';
      image: Uint8Array;
      /** Stretched over the box inset by these fractions from each edge (negative: beyond it); ignored when tiled. */
      stretch?: Insets;
      /** Tiled instead of stretched (`{}`: at its own size). */
      tile?: Tile;
      /** Turns with the shape; default true. */
      rotateWithShape?: boolean;
    }
  | { type: 'fromStyle' };

/** Fractions of a box from each edge. */
export interface Insets {
  /** From the left edge. */
  left: number;
  /** From the top edge. */
  top: number;
  /** From the right edge. */
  right: number;
  /** From the bottom edge. */
  bottom: number;
}

/** A place in a rectangle: a corner, an edge's middle or its centre. */
export type RectAlign = 'topLeft' | 'top' | 'topRight' | 'left' | 'center' | 'right' | 'bottomLeft' | 'bottom' | 'bottomRight';

/** How tiles mirror their neighbours. */
export type TileMirror = 'none' | 'horizontal' | 'vertical' | 'both';

/** A picture fill's tiling; what isn't given is the default. */
export interface Tile {
  /** The tiles' offset across, −1584 to 1584; default 0. */
  offsetX?: Points;
  /** The tiles' offset down; default 0. */
  offsetY?: Points;
  /** The tile's width as a share of the image's own, 0.01 to 100; default 1. */
  scaleX?: number;
  /** The tile's height as a share of the image's own; default 1. */
  scaleY?: number;
  /** Where the first tile is placed; default `'topLeft'`. */
  align?: RectAlign;
  /** How tiles mirror their neighbours; default `'none'`. */
  mirror?: TileMirror;
}

/** A picture's recolouring (Picture Color, Recolor). */
export type Recolor =
  | { kind: 'none' }
  | { kind: 'grayscale' }
  | { kind: 'blackAndWhite'; threshold: number }
  | { kind: 'duotone'; dark: ColourValue; light: ColourValue };

/** A picture's adjustments as they read. */
export interface PictureAdjustState {
  /** Lighter (positive, to 1) or darker (to −1). */
  brightness: number;
  /** More (positive, to 1) or less contrast (to −1). */
  contrast: number;
  /** How see-through it is: 0 opaque to 1 invisible. */
  transparency: number;
  /** Its recolouring. */
  recolor: Recolor;
}

/** How a line is drawn along its length: one line, two equal ones, thick then thin, thin then thick, or three. */
export type Compound = 'single' | 'double' | 'thickThin' | 'thinThick' | 'triple';

/** A line's ends: square at the end, rounded, or square half the width past the end. */
export type LineCap = 'flat' | 'round' | 'square';

/** How a line's segments join: rounded, cut off, or pointed. */
export type LineJoin = 'round' | 'bevel' | 'miter';

/** An arrowhead's shape: a filled triangle, a notched triangle, a diamond, an oval or an open V. */
export type ArrowKind = 'triangle' | 'stealth' | 'diamond' | 'oval' | 'arrow';

/** An arrowhead's width or length. */
export type ArrowSize = 'small' | 'medium' | 'large';

/** An arrowhead. */
export interface Arrowhead {
  /** Its shape. */
  kind: ArrowKind;
  /** Its width; default `'medium'`. */
  width?: ArrowSize;
  /** Its length; default `'medium'`. */
  length?: ArrowSize;
}

/** A line's arrowheads. */
export interface LineEnds {
  /** At the start. */
  head?: Arrowhead;
  /** At the end. */
  tail?: Arrowhead;
}

/** An outline as written. */
export interface LineValue {
  /** The width (0.75 when the document gives none). */
  width: Points;
  /** `none`, `solid` or `gradient`. */
  fill: FillValue;
  /** The ECMA-376 dash name (`'solid'`, `'dash'`, `'sysDot'`…); `'custom'` for a custom dash. */
  dash: string;
  /** Single or several lines. */
  compound: Compound;
  /** The ends. */
  cap: LineCap;
  /** The joins. */
  join: LineJoin;
  /** The arrowheads. */
  ends: LineEnds;
}

/** `setShapeLine`'s change; what isn't given is kept. */
export interface LineChange {
  /** The width, 0 to 1584. */
  width?: Points;
  /** `none`, `solid` or `gradient`; `fromStyle` removes the whole explicit outline. */
  fill?: FillValue;
  /** The ECMA-376 dash name. */
  dash?: string;
  /** Single or several lines. */
  compound?: Compound;
  /** The ends. */
  cap?: LineCap;
  /** The joins. */
  join?: LineJoin;
  /** The arrowheads: both ends are replaced, and an end not given is removed. */
  ends?: LineEnds;
}

/** A shadow. */
export interface Shadow {
  /** Its colour, with its transparency as an `alpha` transform. */
  colour: ColourValue;
  /** How far it blurs. */
  blur: Points;
  /** How far from the shape. */
  distance: Points;
  /** Which way, clockwise from the right. */
  direction: Degrees;
  /** An outer shadow's width as a share of the shape's (negative flips it); default 1. An inner shadow ignores it. */
  scaleX?: number;
  /** An outer shadow's height as a share of the shape's; default 1. */
  scaleY?: number;
  /** An outer shadow's slant across, −90 to 90; default 0. */
  skewX?: Degrees;
  /** An outer shadow's slant down; default 0. */
  skewY?: Degrees;
  /** The point its scale and skew are about; default `'bottom'`. */
  align?: RectAlign;
  /** An outer shadow turns with the shape; default false. */
  rotateWithShape?: boolean;
}

/** A reflection: the shape mirrored beside itself, fading away from it. */
export interface Reflection {
  /** How much of the shape shows before it has faded away, 0 to 1. */
  size: number;
  /** How see-through it is where it starts, 0 to 1. */
  transparency: number;
  /** How far it blurs, 0 to 100; default 0. */
  blur?: Points;
  /** How far from the shape, 0 to 1584; default 0. */
  distance?: Points;
  /** Which way it is offset, clockwise from the right; default 90 (below). */
  direction?: Degrees;
  /** Which way it fades; default 90. */
  fadeDirection?: Degrees;
  /** Its width as a share of the shape's (negative flips it); default 1. */
  scaleX?: number;
  /** Its height as a share of the shape's; default −1 (flipped top to bottom). */
  scaleY?: number;
  /** Its slant across; default 0. */
  skewX?: Degrees;
  /** Its slant down; default 0. */
  skewY?: Degrees;
  /** The point its scale and skew are about; default `'bottomLeft'`. */
  align?: RectAlign;
  /** It turns with the shape; default false. */
  rotateWithShape?: boolean;
}

/** A glow. */
export interface Glow {
  /** Its colour. */
  colour: ColourValue;
  /** How far it reaches. */
  radius: Points;
}

/** A shape's effects. */
export interface EffectsValue {
  /** A shadow outside the shape. */
  outerShadow?: Shadow;
  /** A shadow inside the shape. */
  innerShadow?: Shadow;
  /** A glow around the shape. */
  glow?: Glow;
  /** Edges faded over this distance. */
  softEdge?: Points;
  /** The whole shape blurred by this radius. */
  blur?: Points;
  /** A reflection. */
  reflection?: Reflection;
}

/** `setShapeEffects`'s change: each effect given replaces that effect, `null` removes it, what isn't given is kept. */
export interface EffectsChange {
  /** A shadow outside the shape. */
  outerShadow?: Shadow | null;
  /** A shadow inside the shape. */
  innerShadow?: Shadow | null;
  /** A glow around the shape. */
  glow?: Glow | null;
  /** Edges faded over this distance. */
  softEdge?: Points | null;
  /** The whole shape blurred by this radius. */
  blur?: Points | null;
  /** A reflection. */
  reflection?: Reflection | null;
}

/** A text outline: an outline without arrowheads. */
export interface TextOutline {
  /** The width, 0 to 1584. */
  width: Points;
  /** `none`, `solid` or `gradient`. */
  fill: FillValue;
  /** The ECMA-376 dash name (`'solid'`, `'dash'`, `'sysDot'`…). */
  dash: string;
  /** Single or several lines. */
  compound: Compound;
  /** The ends of its dashes. */
  cap: LineCap;
  /** The joins. */
  join: LineJoin;
}

/** A text shadow. */
export interface TextShadow {
  /** Its colour, with its transparency as an `alpha` transform. */
  colour: ColourValue;
  /** Its size against the text's, percent, 1 to 200. */
  size: number;
  /** How far it blurs, 0 to 100. */
  blur: Points;
  /** How far from the text, 0 to 200. */
  distance: Points;
  /** Which way, clockwise from the right. */
  direction: Degrees;
}

/** A text reflection: the text mirrored below its baseline, fading out. */
export interface TextReflection {
  /** Its transparency where it starts, percent, 0 to 100. */
  transparency: number;
  /** How much of the text it shows before it has faded out, percent, 0 to 100. */
  size: number;
  /** How far it blurs, 0 to 100. */
  blur: Points;
  /** How far below the text, 0 to 100. */
  distance: Points;
}

/** Text effects as written; each one absent is not set. */
export interface TextEffects {
  /** The letters' fill in place of the font colour: `none` (hollow), `solid` or `gradient`. */
  fill?: FillValue;
  /** The letters' outline. */
  outline?: TextOutline;
  /** A shadow behind the letters. */
  shadow?: TextShadow;
  /** A glow around the letters. */
  glow?: Glow;
  /** A reflection under the letters. */
  reflection?: TextReflection;
}

/**
 * `setTextEffects`' change: each effect given replaces the text's own, `null` turns it off (over a style's too; a fill
 * is only removed, so a style's fill shows), what isn't given is kept.
 */
export interface TextEffectsChange {
  /** The letters' fill: `none` (hollow), `solid` or `gradient` of two to ten stops. */
  fill?: FillValue | null;
  /** The letters' outline. */
  outline?: TextOutline | null;
  /** A shadow behind the letters. */
  shadow?: TextShadow | null;
  /** A glow around the letters, 0 to 150 pt. */
  glow?: Glow | null;
  /** A reflection under the letters. */
  reflection?: TextReflection | null;
}

/** Which way a text watermark runs. */
export type WatermarkLayout = 'diagonal' | 'horizontal';

/** A text watermark; what isn't given is Word's default (the theme's body font, the document's language, automatic size, silver, 50% transparent, diagonal). */
export interface TextWatermark {
  /** Marks a text watermark. */
  kind: 'text';
  /** The text. */
  text: string;
  /** The font; absent is the theme's body font. */
  font?: string;
  /** The text's language (a BCP 47 tag such as `en-US`); absent is the document's. */
  language?: string;
  /** The size in points, 1 to 1584 (1 is 1 pt); absent is automatic: as large as fits the page. */
  size?: Points;
  /** The colour. */
  color?: Color;
  /** Percent, 0 (opaque) to 100. */
  transparency?: number;
  /** Diagonal or horizontal. */
  layout?: WatermarkLayout;
  /** Bold. */
  bold?: boolean;
  /** Italic. */
  italic?: boolean;
}

/** A picture watermark. */
export interface PictureWatermark {
  /** Marks a picture watermark. */
  kind: 'picture';
  /** The image (PNG, JPEG, GIF, BMP, TIFF, JPEG XR, SVG, EMF or WMF); empty keeps the current watermark's, and is empty in the state. */
  image: Uint8Array;
  /** Percent of the picture's own size; absent fits it to the margins. */
  scale?: number;
  /** Washed out, as Word's default is. */
  washout?: boolean;
}

/** A watermark: text, or a picture. */
export type Watermark = TextWatermark | PictureWatermark;

/** Where text sits in a shape vertically. */
export type TextAnchor = 'top' | 'middle' | 'bottom';

/** How a shape fits its text: it keeps its size, or its height follows the text. */
export type TextAutofit = 'none' | 'shape' | 'shrinkText';

/**
 * Which way a shape's text runs: left to right; turned 90° or 270° clockwise; East Asian vertical (CJK upright,
 * other text turned); Mongolian vertical; letters stacked one per line, or stacked right to left.
 */
export type TextDirection =
  | 'horizontal'
  | 'vertical'
  | 'vertical270'
  | 'eastAsianVertical'
  | 'mongolianVertical'
  | 'stacked'
  | 'stackedRightToLeft';

/** A shape's text body as written. */
export interface ShapeTextState {
  /** Left, top, right, bottom. */
  insets: [Points, Points, Points, Points];
  /** Where the text sits. */
  anchor: TextAnchor;
  /** How the shape fits it. */
  autofit: TextAutofit;
  /** Lines wrap at the shape's width. */
  wrapText: boolean;
  /** Which way it runs. */
  direction: TextDirection;
  /** Its columns. */
  columns: TextColumns;
  /** The text stays upright when the shape turns ("Do not rotate text within shape"). */
  upright: boolean;
}

/** A text box's text columns: 1 to 16, 0 to 1584 apart. */
export interface TextColumns {
  /** How many. */
  count: number;
  /** The space between them. */
  spacing: Points;
}

/** `setShapeText`'s change; what isn't given is kept. */
export interface ShapeTextChange {
  /** Left, top, right, bottom, 0 to 1584. */
  insets?: [Points, Points, Points, Points];
  /** Where the text sits. */
  anchor?: TextAnchor;
  /** How the shape fits it. */
  autofit?: TextAutofit;
  /** Lines wrap at the shape's width. */
  wrapText?: boolean;
  /** Which way it runs. */
  direction?: TextDirection;
  /** Its columns. */
  columns?: TextColumns;
  /** The text stays upright when the shape turns. */
  upright?: boolean;
}

/** `setPictureAdjust`'s change; what isn't given is kept. */
export interface PictureAdjustChange {
  /** How see-through the picture is: 0 opaque to 1 invisible. */
  transparency?: number;
  /** Shown in shades of grey. */
  grayscale?: boolean;
  /** Lighter (positive) or darker, −1 to 1. */
  brightness?: number;
  /** More (positive) or less contrast, −1 to 1. */
  contrast?: number;
  /** Set Transparent Colour: the colour drawn see-through, or `'none'`. */
  transparentColor?: Color | 'none';
}

/** `insertShape`'s options. */
export interface ShapeInsert {
  /** The preset geometry. */
  preset: ShapePreset;
  /** The width; default 72. */
  width?: Points;
  /** The height; default 72, 0 for lines. */
  height?: Points;
}

/** `setShapeAdjust`'s options. */
export interface ShapeAdjust {
  /** The adjust value's index, in the preset's order. */
  index: number;
  /** Its new value, in the preset's units (usually 100000ths). */
  value: number;
}

/** Which way to flip: left to right, or top to bottom. */
export type FlipAxis = 'horizontal' | 'vertical';

/**
 * Where an object goes in the paint order: over the next object above it, under the next one below it, over or
 * under every object, or in front of or behind the text.
 */
export type Arrange = 'forward' | 'backward' | 'toFront' | 'toBack' | 'inFrontOfText' | 'behindText';

/** A fill as written: the shape's own, or its style reference's (the theme's, with the reference's colour). */
export type FillState = { explicit: FillValue } | { fromStyle: FillValue };

/** An outline as written: the shape's own, or its style reference's. */
export type LineState = { explicit: LineValue } | { fromStyle: LineValue };

/** The selected shape's properties, as written: theme colours stay theme colours with their transforms. */
export interface ShapeState {
  /** The preset geometry; `null` for a custom geometry. */
  preset: ShapePreset | null;
  /** The adjust values in effect, in the preset's order. */
  adjust: number[];
  /** The fill. */
  fill: FillState;
  /** The outline. */
  line: LineState;
  /** The effects. */
  effects: EffectsValue;
  /** The text body; `null` for pictures and groups. */
  text: ShapeTextState | null;
}

/** A group of Word's Insert \> Shapes gallery. */
export interface ShapeGalleryGroup {
  /** Its label (`'Basic Shapes'`). */
  label: string;
  /** Its presets, in Word's order. */
  presets: ShapePreset[];
}
