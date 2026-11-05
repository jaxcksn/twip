import type { CommandId } from '../commands.js';
import type { Session } from '../editor.js';
import { EditorError } from '../errors.js';
import type { HostCommand } from '../events.js';
import { Method } from '../transport/protocol.js';

/** The document as an accessibility tree. The mounted editor manages its own accessibility DOM. */
export interface Accessibility {
  /** The tree. */
  tree(): Promise<AccessibleNode>;
  /**
   * Puts the caret at the start of a node's content and scrolls it into view: a note reference's note, a comment's
   * commented text. Rejects with `'invalid-argument'` when no node has the id, and `'unavailable'` when the node holds
   * no caret position (a region).
   */
  go(id: AccessibleId): Promise<void>;
  /** The kinds of announcements the editor makes; the others are neither said nor emitted. Default: all. */
  setAnnouncements(kinds: AnnouncementKind[]): Promise<void>;
}

/**
 * A node's identity: the same across edits while the thing it names exists (a paragraph, table, note, comment or
 * story keeps its id when other content changes).
 */
export type AccessibleId = number;

/** A node of the accessibility tree. */
export interface AccessibleNode {
  /** Its identity. */
  id: AccessibleId;
  /** Its role. `region`: a named group after the body (headers and footers, footnotes, endnotes, comments). */
  role:
    | 'document'
    | 'paragraph'
    | 'heading'
    | 'list'
    | 'listItem'
    | 'table'
    | 'row'
    | 'cell'
    | 'columnHeader'
    | 'rowHeader'
    | 'image'
    | 'link'
    | 'textBox'
    | 'note'
    | 'noteRef'
    | 'comment'
    | 'header'
    | 'footer'
    | 'region'
    | 'text';
  /** Its accessible name. */
  name?: string;
  /** A text node's text. */
  text?: string;
  /** A heading's level, a list item's nesting level, or a table's nesting level, from 1. */
  level?: number;
  /** A numbered list item's number as shown (`3.`, `a)`). */
  label?: string;
  /** A list item's position in its list. */
  position?: ListPosition;
  /** A cell's place in its table. */
  cell?: CellPlace;
  /** Review and spelling marks on a text node. */
  marks?: ('insertion' | 'deletion' | 'misspelling')[];
  /** Whose marks or comment it is. */
  author?: string;
  /** Its language (BCP 47), where it differs from its parent's. */
  lang?: string;
  /** The comment the text is in the range of. */
  details?: AccessibleId;
  /** The note a note reference points to; for a sub-template body shown in place, its reference's paragraph. */
  target?: AccessibleId;
  /** Its children. */
  children: AccessibleNode[];
}

/** Where a list item is in its list: its index (from 1) among the items at its level, and how many there are. */
export interface ListPosition {
  /** Its index, from 1. */
  index: number;
  /** The number of items. */
  count: number;
}

/** Where a cell is in its table's grid: its row and column (from 1) and how many it spans. */
export interface CellPlace {
  /** Its row, from 1. */
  row: number;
  /** Its first grid column, from 1. */
  col: number;
  /** How many rows it spans. */
  rowSpan: number;
  /** How many grid columns it spans. */
  colSpan: number;
}

/** What an announcement is about. */
export type AnnouncementKind =
  | 'table'
  | 'list'
  | 'page'
  | 'review'
  | 'comment'
  | 'spelling'
  | 'formatting'
  | 'mode'
  | 'readOnly'
  | 'command'
  | 'selection';

/** Keyboard shortcuts: the application's own, and what a key does now. */
export interface Shortcuts {
  /** Claims a key for the application: pressing it emits `shortcut`. Throws 'unavailable' on a collision, naming it. */
  register(shortcut: { id: string; keys: string | { windows: string; mac: string } }): () => void;
  /** What a key does now: a command, an application shortcut, a host command, or nothing. */
  lookup(
    keys: string,
  ): { kind: 'command'; id: CommandId } | { kind: 'shortcut'; id: string } | { kind: 'host'; command: HostCommand } | null;
}

/** The accessibility tree over a session. */
export function createAccessibility(s: Session): Accessibility {
  return { tree: () => s.call(Method.AccessibleTree), go: (id) => s.call(Method.AccessibleGo, id),
    setAnnouncements: (kinds) => s.call(Method.AnnouncementsSet, kinds),
  };
}

/** The keys the application owns (the engine's host keys), by their Windows shortcut. */
const hostKeys: Record<string, HostCommand> = {
  'Ctrl+S': 'save',
  F12: 'saveAs',
  'Ctrl+P': 'print',
  'Ctrl+O': 'open',
  'Ctrl+N': 'new',
  'Ctrl+F': 'find',
  'Ctrl+H': 'replace',
  'Ctrl+G': 'goTo',
  'Ctrl+K': 'insertHyperlink',
  F7: 'nextMisspelling',
  'Ctrl+Shift+F': 'focusFont',
  'Ctrl+Shift+P': 'focusFontSize',
  F1: 'help',
};

/** A shortcut in one spelling: modifiers sorted, case ignored. `'Ctrl+Alt+S'`, `'⌥⌘S'`. */
function normalize(keys: string): string {
  const glyphs = /^[⌘⌥⇧⌃]*/u.exec(keys)![0];
  const parts = glyphs ? [...glyphs, keys.slice(glyphs.length)] : keys.split(/\+(?=.)/);
  const key = parts.pop()!.toLowerCase();
  return [...parts.map((p) => p.toLowerCase()).sort(), key].join('+');
}

/**
 * Shortcuts over a session. Lookups are answered here, from the catalog, the host keys and the registrations, so
 * they can be synchronous; registrations also go to the engine, which presses them.
 */
export function createShortcuts(s: Session): Shortcuts {
  const registered = new Map<string, { windows: string; mac: string }>();
  const lookup: Shortcuts['lookup'] = (keys) => {
    const key = normalize(keys);
    const matches = (k: string) => normalize(k) === key;
    for (const [id, k] of registered) if (matches(k.windows) || matches(k.mac)) return { kind: 'shortcut', id };
    for (const c of s.catalog) if ([...c.shortcuts.windows, ...c.shortcuts.mac].some(matches)) return { kind: 'command', id: c.id };
    const host = Object.entries(hostKeys).find(([k]) => matches(k));
    return host ? { kind: 'host', command: host[1] } : null;
  };
  const report = (call: Promise<unknown>) =>
    call.catch((e: unknown) => s.emit('error', e instanceof EditorError ? e : new EditorError('internal', String(e), { cause: e })));
  return {
    register: ({ id, keys }) => {
      const both = typeof keys === 'string' ? { windows: keys, mac: keys } : keys;
      if (s.destroyed) throw new EditorError('destroyed', 'the editor was destroyed');
      for (const k of new Set([both.windows, both.mac])) {
        const taken = lookup(k);
        if (taken) throw new EditorError('unavailable', `${k} is already ${taken.kind === 'host' ? `the ${taken.command} key` : `bound to ${taken.id}`}`);
      }
      registered.set(id, both);
      report(s.call(Method.ShortcutsRegister, { id, keys }));
      return () => {
        if (registered.get(id) !== both) return;
        registered.delete(id);
        if (!s.destroyed) report(s.call(Method.ShortcutsUnregister, id));
      };
    },
    lookup,
  };
}
