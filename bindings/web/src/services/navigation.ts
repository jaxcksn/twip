import type { BorderLine, ListLevel, NestedOp } from '../commands.js';
import type { Session } from '../editor.js';
import type { CharacterFormatting, ParagraphFormatting } from '../state.js';
import { Method } from '../transport/protocol.js';
import type { Color, HeadingId, Points, StyleId } from '../types.js';
import type { TabStopInfo } from './ruler.js';

/** The document's headings and bookmarks, for a navigation pane and the Bookmark dialog. */
export interface Navigation {
  /** The headings, in document order. */
  headings(): Promise<Heading[]>;
  /** The bookmarks, hidden ones too, in document order. Go to one with the `goToBookmark` command. */
  bookmarks(): Promise<BookmarkInfo[]>;
  /** Selects and reveals a heading. */
  select(heading: HeadingId): Promise<void>;
  /**
   * The heading the selection is under, as a navigation pane highlights it (the nearest listed one at or before it);
   * `null` before the first heading or outside the body. Ask on `selectionChanged`.
   */
  currentHeading(): Promise<HeadingId | null>;
}

/** A heading. */
export interface Heading {
  /** The heading. */
  id: HeadingId;
  /** Its outline level, from 1. */
  level: number;
  /** Its text. */
  text: string;
  /** Its page. */
  page: number;
}

/** A bookmark. */
export interface BookmarkInfo {
  /** Its name. */
  name: string;
  /** Hidden: Word's own (`_Toc`, `_Ref`), named with a leading `_`. */
  hidden: boolean;
  /** Its text holds nothing: it marks a place. */
  empty: boolean;
}

/** The document's styles, for a style gallery. */
export interface Styles {
  /** The styles, optionally of one type. */
  list(options?: { type?: 'paragraph' | 'character' | 'table' | 'list' }): Promise<StyleInfo[]>;
  /** A style's gallery tile: its sample text in the style's character formatting, scaled to fit, as PNG. */
  tile(id: StyleId, size: PreviewSize): Promise<PreviewPicture>;
  /**
   * A style as it is defined, without a dialog: the document's, or a built-in one it doesn't define yet
   * (`defined: false`); `null` for an id that is neither.
   */
  definition(id: StyleId): Promise<StyleDefinition | null>;
  /** Every built-in style, defined by the document or not, with its list flags (`setStyleMeta` sets them). */
  builtIn(): Promise<BuiltInStyle[]>;
  /** The document's default formatting (`setDefaultFormatting` sets it); `null` without a styles part. */
  documentDefaults(): Promise<DefaultsDefinition | null>;
}

/** A style's type as Word's style lists show it: a paragraph style linked to a character style is one linked style. */
export type StyleCategory = 'paragraph' | 'linked' | 'character' | 'table' | 'list';

/** The parts of a table a table style formats. */
export type TableStylePart =
  | 'wholeTable'
  | 'headerRow'
  | 'totalRow'
  | 'firstColumn'
  | 'lastColumn'
  | 'oddBandedRows'
  | 'evenBandedRows'
  | 'oddBandedColumns'
  | 'evenBandedColumns'
  | 'topLeftCell'
  | 'topRightCell'
  | 'bottomLeftCell'
  | 'bottomRightCell';

/** The list a paragraph style numbers its paragraphs with: a list's id (`lists.definition()`) and a level, 0 to 8. */
export interface StyleList {
  /** The list. */
  list: number;
  /** The level, 0 to 8. */
  level: number;
}

/** A style change: what isn't given is kept, and `null` removes. */
export interface StyleChange {
  /** The name. A built-in style keeps its own: the new one becomes an alias. */
  name?: string;
  /** The style it is based on (of its own type), or none. */
  basedOn?: StyleId | null;
  /** The paragraph style Enter starts after a paragraph in it; `null` for itself. Paragraph styles only. */
  next?: StyleId | null;
  /** The style of the other type it is linked to, or none; the other style links back. */
  link?: StyleId | null;
  /** Formatting a paragraph in it directly changes the style. Paragraph styles only. */
  autoUpdate?: boolean;
  /** Shown in the Styles gallery. */
  quick?: boolean;
  /** The list its paragraphs are numbered with, or none. Paragraph styles only. */
  list?: StyleList | null;
  /** A list style's levels, from the first. */
  listLevels?: ListLevel[];
  /** Its own character and paragraph formatting goes first (a table style's table, row and cell properties too), so `formatting` gives all of it. */
  clear?: boolean;
  /**
   * Formatting ops run on the style as on text: character ones (a character style takes only these), paragraph ones,
   * and for a table style's whole table `setCellShading`, `setShading` (cells or the table), `setCellBorders`,
   * `setCellAlignment`, `setCellMargins`, `setRowCanBreak`, `setTableAlignment` and `setTableIndent`.
   */
  formatting?: NestedOp[];
  /** A table style's parts' formatting (no table alignment or indent in a part). */
  parts?: { part: TableStylePart; clear?: boolean; formatting?: NestedOp[] }[];
  /** A table style's rows in each band, 1 to 32767. */
  rowBand?: number;
  /** A table style's columns in each band, 1 to 32767. */
  columnBand?: number;
}

/** `createStyle`'s options. */
export interface NewStyle {
  /** Its type. */
  category: StyleCategory;
  /** It starts from the selection's formatting, based on its paragraph's style (New Style from Formatting). */
  fromSelection?: boolean;
  /** It is applied once made: to the selected paragraphs, text or table. */
  apply?: boolean;
  /** Its name (none: the first free "Style N"), base and formatting. */
  change?: StyleChange;
}

/** A table style's borders. */
export interface TableBorders {
  /** The top edge. */
  top?: BorderLine;
  /** The left edge. */
  left?: BorderLine;
  /** The bottom edge. */
  bottom?: BorderLine;
  /** The right edge. */
  right?: BorderLine;
  /** Between rows. */
  insideHorizontal?: BorderLine;
  /** Between columns. */
  insideVertical?: BorderLine;
  /** Each cell's top left to bottom right diagonal. */
  diagonalDown?: BorderLine;
  /** Each cell's top right to bottom left diagonal. */
  diagonalUp?: BorderLine;
}

/** A table style's part as its cells show it: the whole table's formatting with the part's over it. */
export interface TablePartDefinition {
  /** The part. */
  part: TableStylePart;
  /** The style or a style it is based on defines the part. */
  defined: boolean;
  /** Text's character formatting. */
  character: CharacterFormatting;
  /** Text's paragraph formatting. */
  paragraph: ParagraphFormatting;
  /** The cells' fill. */
  fill?: Color;
  /** The cells' vertical alignment. */
  cellAlignment: 'top' | 'center' | 'bottom';
  /** The borders: the table's for the whole table, the cells' for a part. */
  borders: TableBorders;
  /** The cell margins: the table's default ones for the whole table. */
  cellMargins: {
    /** The top margin. */
    top?: Points;
    /** The bottom margin. */
    bottom?: Points;
    /** The left margin. */
    left?: Points;
    /** The right margin. */
    right?: Points;
  };
  /** Rows may break across pages. */
  rowCanBreak: boolean;
}

/** A style as `styles.definition` reads it. */
export interface StyleDefinition {
  /** Its id. */
  id: StyleId;
  /** Its name as style lists show it. */
  name: string;
  /** Its other names. */
  aliases: string[];
  /** Its type. */
  category: StyleCategory;
  /** Built into Word. */
  builtIn: boolean;
  /** The document defines it; a built-in style it doesn't is added when first used. */
  defined: boolean;
  /** The style it is based on. */
  basedOn?: StyleId;
  /** The style of the paragraph after one in it (absent: itself). */
  next?: StyleId;
  /** The style it is linked to. */
  link?: StyleId;
  /** Formatting its paragraphs directly changes it. */
  autoUpdate: boolean;
  /** Shown in the Styles gallery. */
  quick: boolean;
  /** Whether style lists show it. */
  visibility: 'show' | 'hideUntilUsed' | 'hide';
  /** Its sort priority. */
  priority: number;
  /** Restricted while formatting is protected. */
  locked: boolean;
  /** The list its paragraphs are numbered with. */
  list?: StyleList;
  /** Text in it, through its base and the document defaults (a table style's whole table). */
  character: CharacterFormatting;
  /** Text in it's paragraph formatting. */
  paragraph: ParagraphFormatting;
  /** Text in it's tab stops. */
  tabs: TabStopInfo[];
  /** What it sets itself: `character` where it does, `null` where it inherits. */
  ownCharacter: CharacterFormatting;
  /** What it sets itself: `paragraph` where it does, `null` where it inherits. */
  ownParagraph: ParagraphFormatting;
  /** Word's description of it: its formatting over its base's. */
  description: string;
  /** A table style's table formatting and its 13 parts, the whole table first. */
  table?: {
    /** The table's alignment. */
    alignment: 'left' | 'center' | 'right';
    /** The table's indent. */
    indent: Points;
    /** The table's own fill, behind its cells. */
    fill?: Color;
    /** Rows in each band. */
    rowBand: number;
    /** Columns in each band. */
    columnBand: number;
    /** Every part, the whole table first. */
    parts: TablePartDefinition[];
  };
  /** A list style's levels, in full. */
  listLevels: ListLevel[];
}

/** The document's default formatting as `styles.documentDefaults` reads it. */
export interface DefaultsDefinition {
  /** Text's character formatting before any style's. */
  character: CharacterFormatting;
  /** Text's paragraph formatting before any style's. */
  paragraph: ParagraphFormatting;
  /** The default tab stops of every paragraph. */
  tabs: TabStopInfo[];
  /** What the defaults set: `character` where they do, `null` where the application's apply. */
  ownCharacter: CharacterFormatting;
  /** What the defaults set: `paragraph` where they do, `null` where the application's apply. */
  ownParagraph: ParagraphFormatting;
}

/** A built-in style, defined by the document or not, with the flags style lists use. */
export interface BuiltInStyle {
  /** Its id. */
  id: StyleId;
  /** Its name as style lists show it. */
  name: string;
  /** Its type. */
  category: StyleCategory;
  /** The document defines it. */
  defined: boolean;
  /** Its sort priority. */
  priority: number;
  /** Whether style lists show it. */
  visibility: 'show' | 'hideUntilUsed' | 'hide';
  /** Shown in the Styles gallery. */
  quick: boolean;
  /** Restricted while formatting is protected. */
  locked: boolean;
}

/** A preview picture's size: CSS px, and device pixels per px (default 1). */
export interface PreviewSize {
  /** The width, CSS px. */
  width: number;
  /** The height, CSS px. */
  height: number;
  /** Device pixels per CSS px (the device pixel ratio). Default 1. */
  scale?: number;
}

/** A picture the editor drew: a PNG of `width` x `height` device pixels. */
export interface PreviewPicture {
  /** The PNG. */
  png: Uint8Array;
  /** Its width, device pixels. */
  width: number;
  /** Its height, device pixels. */
  height: number;
}

/** A picture as it travels: its PNG in base64. */
export function toPicture(p: { png: string; width: number; height: number }): PreviewPicture {
  return { png: Uint8Array.from(atob(p.png), (c) => c.charCodeAt(0)), width: p.width, height: p.height };
}

/** A style. */
export interface StyleInfo {
  /** The style. */
  id: StyleId;
  /** Its display name. */
  name: string;
  /** Its type. */
  type: 'paragraph' | 'character' | 'table' | 'list';
  /** Built in. */
  builtIn: boolean;
  /** Shown in a style gallery (`w:qFormat`). */
  quick: boolean;
  /** The sort priority. */
  priority: number;
  /** Hidden from lists. */
  hidden: boolean;
  /** The style it is based on. */
  basedOn: StyleId | null;
  /** For drawing a gallery tile. */
  preview: { font: string; size: Points; bold: boolean; italic: boolean; color: Color | 'auto' };
  /** Its other names; absent when it has none. */
  aliases?: string[];
  /** The linked character or paragraph style. */
  link: StyleId | null;
  /** Whether it shows in style lists: `hideUntilUsed` shows it once a paragraph or run uses it. */
  visibility: 'show' | 'hideUntilUsed' | 'hide';
  /** Restricted: unavailable while the document's formatting is protected. */
  locked: boolean;
  /**
   * How many times the main document uses it: paragraphs for a paragraph style, stretches of runs for a character
   * style, 0 for table and list styles (`selectStyleInstances` selects them).
   */
  uses: number;
}

/** The headings over a session. */
export function createNavigation(s: Session): Navigation {
  return {
    headings: () => s.call(Method.Headings),
    bookmarks: () => s.call(Method.Bookmarks),
    select: (heading) => s.call(Method.SelectHeading, heading),
    currentHeading: () => s.call(Method.CurrentHeading),
  };
}

/** The styles over a session. */
export function createStyles(s: Session): Styles {
  return {
    list: (options) => s.call(Method.Styles, options?.type ?? null),
    tile: async (id, size) => toPicture(await s.call(Method.StyleTile, { id, size: { scale: 1, ...size } })),
    definition: (id) => s.call(Method.StyleDefinition, id),
    builtIn: () => s.call(Method.BuiltInStyles),
    documentDefaults: () => s.call(Method.DocumentDefaults),
  };
}
