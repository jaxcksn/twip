import type { Session } from '../editor.js';
import { Method } from '../transport/protocol.js';
import type { Color, HeadingId, Points, StyleId } from '../types.js';

/** The document's headings, for a navigation pane. */
export interface Navigation {
  /** The headings, in document order. */
  headings(): Promise<Heading[]>;
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

/** The document's styles, for a style gallery. */
export interface Styles {
  /** The styles, optionally of one type. */
  list(options?: { type?: 'paragraph' | 'character' | 'table' }): Promise<StyleInfo[]>;
}

/** A style. */
export interface StyleInfo {
  /** The style. */
  id: StyleId;
  /** Its display name. */
  name: string;
  /** Its type. */
  type: 'paragraph' | 'character' | 'table';
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
}

/** The headings over a session. */
export function createNavigation(s: Session): Navigation {
  return {
    headings: () => s.call(Method.Headings),
    select: (heading) => s.call(Method.SelectHeading, heading),
  };
}

/** The styles over a session. */
export function createStyles(s: Session): Styles {
  return { list: (options) => s.call(Method.Styles, options?.type ?? null) };
}
