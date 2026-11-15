export type {
  Arrange,
  ArrowKind,
  ArrowSize,
  Arrowhead,
  Author,
  ClientPoint,
  ClientRect,
  ColourTransform,
  ColourValue,
  Compound,
  ContentPoint,
  EffectsChange,
  EffectsValue,
  FillState,
  FillValue,
  FlipAxis,
  FrameStats,
  Glow,
  GradientShape,
  GradientStop,
  LineCap,
  LineChange,
  LineEnds,
  LineJoin,
  LineState,
  LineValue,
  PictureAdjustChange,
  Shadow,
  ShapeAdjust,
  ShapeGalleryGroup,
  ShapeInsert,
  ShapePreset,
  ShapeState,
  ShapeTextChange,
  ShapeTextState,
  TextAnchor,
  TextAutofit,
  TextDirection,
  ThemeColour,
  TransformKind,
  Warning,
  WordCount,
} from './wire.generated.js';
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

/** An angle in degrees, clockwise. */
export type Degrees = number;
