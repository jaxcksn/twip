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
    | 'sub-template-near-match';
  /** A readable description. */
  message: string;
  /** How many times it occurred. */
  count: number;
  /** Where it occurred, when known. */
  location?: { part: string; paragraph?: number };
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
 * stretched), or `fromStyle`: the style reference's fill, no explicit one.
 */
export type FillValue =
  | { type: 'none' }
  | { type: 'solid'; colour: ColourValue }
  | { type: 'gradient'; kind: GradientShape; angle?: Degrees; stops: GradientStop[] }
  | { type: 'pattern'; preset: string; foreground: ColourValue; background: ColourValue }
  | { type: 'picture'; image: Uint8Array; tile?: boolean }
  | { type: 'fromStyle' };

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
}

/** Where text sits in a shape vertically. */
export type TextAnchor = 'top' | 'middle' | 'bottom';

/** How a shape fits its text: it keeps its size, or its height follows the text. */
export type TextAutofit = 'none' | 'shape';

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
