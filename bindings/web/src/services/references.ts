import type { CrossReferenceKind, PasteAs, TableOfContentsOptions } from '../commands.js';
import type { Session } from '../editor.js';
import { Method } from '../transport/protocol.js';
import type { ParagraphId } from '../types.js';

/** Tables of contents, captions and cross-references: what a References tab and its panes read. */
export interface References {
  /**
   * The items a cross-reference of `kind` can point to, as Word's Cross-reference dialog lists them (bookmarks by
   * name, the rest in document order): `commands.insertCrossReference` names one by its place here.
   */
  crossReferenceTargets(kind: CrossReferenceKind): Promise<CrossReferenceTarget[]>;
  /** The labels a caption can take: Equation, Figure and Table, then `custom` (the host's), then the document's own. */
  captionLabels(custom?: string[]): Promise<string[]>;
  /**
   * The document's tables of contents and of figures, and the one the selection is in: for References, Update Table
   * and a form prefilled from the table at the caret. Ask on selection change (it walks the document).
   */
  tablesOfContents(): Promise<TablesOfContents>;
  /** The entries a table with `options` would collect now (`{}`: Word's default), for a preview. */
  tocEntries(options: TableOfContentsOptions): Promise<TocEntryPreview[]>;
}

/** An item a cross-reference can point to. */
export interface CrossReferenceTarget {
  /** What the list shows: a numbered item's number and text, a heading's text, a bookmark's name, a note's text, a caption. */
  text: string;
  /** Its number: a list number, a heading's, a note's as laid out. */
  number?: string;
  /** A heading's level. */
  level?: number;
  /** A bookmark's name. */
  bookmark?: string;
  /** A note's id, as `notes` lists it. */
  note?: number;
}

/** A table of contents or of figures in the document. */
export interface TableOfContentsInfo {
  /** Its place among them, in document order: what `index` names in the commands. */
  index: number;
  /** It collects captions: a table of figures. */
  figures: boolean;
  /** What it collects and how its entries read, from its code. */
  options: TableOfContentsOptions;
  /** Updating and removing it are refused: its field is locked, or a control around it locks its content. */
  locked: boolean;
  /** The paragraph its field starts in. */
  firstParagraph: ParagraphId;
  /** The paragraph its field ends in. */
  lastParagraph: ParagraphId;
}

/** The document's tables of contents and of figures. */
export interface TablesOfContents {
  /** Each one, in document order. */
  tables: TableOfContentsInfo[];
  /** The one the selection is in, by `index`; absent when it is in none. */
  here?: number;
}

/** An entry a table of contents would collect. */
export interface TocEntryPreview {
  /** 1 to 9. */
  level: number;
  /** Its text. */
  text: string;
  /** It shows a page number. */
  pageNumber: boolean;
}

/** What the clipboard can paste as at the selection: Paste Special's list. */
export interface PasteChoices {
  /** In Word's order; empty where nothing can be pasted. */
  choices: PasteAs[];
  /** The one Ctrl+V takes; absent when the clipboard holds nothing to paste. */
  default?: PasteAs;
}

/** References over a session. */
export function createReferences(s: Session): References {
  return {
    crossReferenceTargets: (kind) => s.call(Method.CrossReferenceTargets, kind),
    captionLabels: (custom) => s.call(Method.CaptionLabels, custom ?? []),
    tablesOfContents: () => s.call(Method.TablesOfContents),
    tocEntries: (options) => s.call(Method.TocEntries, options),
  };
}
