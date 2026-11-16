import type { Session } from '../editor.js';
import { Method } from '../transport/protocol.js';
import type { Color, HeadingId, Points, StyleId } from '../types.js';

/** The document's headings and bookmarks, for a navigation pane and the Bookmark dialog. */
export interface Navigation {
  /** The headings, in document order. */
  headings(): Promise<Heading[]>;
  /** The bookmarks, hidden ones too, in document order. Go to one with the `goToBookmark` command. */
  bookmarks(): Promise<BookmarkInfo[]>;
  /** Selects and reveals a heading. */
  select(heading: HeadingId): Promise<void>;
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
}

/** The headings over a session. */
export function createNavigation(s: Session): Navigation {
  return {
    headings: () => s.call(Method.Headings),
    bookmarks: () => s.call(Method.Bookmarks),
    select: (heading) => s.call(Method.SelectHeading, heading),
  };
}

/** The styles over a session. */
export function createStyles(s: Session): Styles {
  return {
    list: (options) => s.call(Method.Styles, options?.type ?? null),
    tile: async (id, size) => toPicture(await s.call(Method.StyleTile, { id, size: { scale: 1, ...size } })),
  };
}
