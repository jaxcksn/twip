import type { Session } from '../editor.js';
import { Method } from '../transport/protocol.js';
import type { Color, HeadingId, Points, StyleId } from '../types.js';
import type { Heading, PreviewPicture, PreviewSize, StyleInfo } from '../wire.generated.js';

export type { Heading, PreviewPicture, PreviewSize, StyleInfo } from '../wire.generated.js';

/** The document's headings, for a navigation pane. */
export interface Navigation {
  /** The headings, in document order. */
  headings(): Promise<Heading[]>;
  /** Selects and reveals a heading. */
  select(heading: HeadingId): Promise<void>;
}

/** The document's styles, for a style gallery. */
export interface Styles {
  /** The styles, optionally of one type. */
  list(options?: { type?: 'paragraph' | 'character' | 'table' | 'list' }): Promise<StyleInfo[]>;
  /** A style's gallery tile: its sample text in the style's character formatting, scaled to fit, as PNG. */
  tile(id: StyleId, size: PreviewSize): Promise<PreviewPicture>;
}

/** A picture as it travels: its PNG in base64. */
export function toPicture(p: { png: string; width: number; height: number }): PreviewPicture {
  return { png: Uint8Array.from(atob(p.png), (c) => c.charCodeAt(0)), width: p.width, height: p.height };
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
  return {
    list: (options) => s.call(Method.Styles, options?.type ?? null),
    tile: async (id, size) => toPicture(await s.call(Method.StyleTile, { id, size: { scale: 1, ...size } })),
  };
}
