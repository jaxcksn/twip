import type { ListEntryId, ListLevel } from '../commands.js';
import type { StyleId } from '../types.js';
import type { Session } from '../editor.js';
import { Method } from '../transport/protocol.js';
import { type PreviewPicture, type PreviewSize, toPicture } from './navigation.js';

/** A list gallery entry: its id, a short label (its first levels' numbers or its bullet), and its levels. */
export interface ListEntry {
  /** The entry, for the `applyListEntry` command. */
  id: ListEntryId;
  /** A short label, such as `1. a. i.` or a bullet's character. */
  label: string;
  /** Its levels. */
  levels: ListLevel[];
}

/** A section of a list gallery: the built-in library, the lists used in this session, the document's lists, or its list styles. */
export interface ListGallerySection {
  /** Which section. */
  kind: 'library' | 'recentlyUsed' | 'document' | 'listStyles';
  /** Its entries. */
  entries: ListEntry[];
}

/** A run of characters a font has: code points `first` to `last`. */
export interface CharRange {
  /** The first code point. */
  first: number;
  /** The last code point. */
  last: number;
}

/** The list galleries, and the characters of fonts for a symbol picker. */
export interface Lists {
  /** A gallery's sections. Apply an entry with the `applyListEntry` command. */
  galleries(kind: 'bullet' | 'numbering' | 'multilevel'): Promise<ListGallerySection[]>;
  /** An entry's preview as PNG: three items, or three levels of a multilevel list. */
  preview(id: ListEntryId, size: PreviewSize): Promise<PreviewPicture>;
  /** The characters a font has, as runs of code points in its Basic Multilingual Plane. */
  fontCharacters(family: string): Promise<CharRange[]>;
  /** A character in a font as PNG, drawn to fit. */
  characterPicture(family: string, character: string, size: PreviewSize): Promise<PreviewPicture>;
  /**
   * The list of the paragraph the selection's focus is in, its nine levels in full (`setListDefinition` sets them);
   * `null` outside a list.
   */
  definition(): Promise<ListDefinition | null>;
}

/** A list as `lists.definition` reads it. */
export interface ListDefinition {
  /** The list instance. Paragraphs number together when their lists share a definition, whatever their ids. */
  id: number;
  /** Its name, which ListNum fields name it by. */
  name?: string;
  /** The list style it is defined by. */
  style?: StyleId;
  /** Whether the current level is a bullet. */
  bullet: boolean;
  /** The current level, 0 to 8. */
  level: number;
  /** Its nine levels. */
  levels: ListLevel[];
}

/** The lists over a session. */
export function createLists(s: Session): Lists {
  return {
    galleries: (kind) => s.call(Method.ListGalleries, kind),
    preview: async (id, size) => toPicture(await s.call(Method.ListEntryPreview, { id, size: { scale: 1, ...size } })),
    fontCharacters: (family) => s.call(Method.FontCharacters, family),
    characterPicture: async (family, character, size) =>
      toPicture(await s.call(Method.CharacterPicture, { family, character, size: { scale: 1, ...size } })),
    definition: () => s.call(Method.ListDefinition),
  };
}
