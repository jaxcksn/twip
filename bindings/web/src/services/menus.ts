import type { CommandId } from '../commands.js';
import type { Session } from '../editor.js';
import { Method } from '../transport/protocol.js';
import type { ClientPoint, CommentId, ElementId, RevisionId } from '../types.js';

/** The context menu as a typed model: the application draws it. */
export interface Menus {
  /**
   * The menu for the selection, as the context-menu key would open it. `synonyms`, your thesaurus's words for
   * {@link Menus.wordAtSelection} (at most 8 show; an empty list is an empty submenu), add a Synonyms submenu.
   */
  contextMenu(options?: { synonyms?: string[] }): Promise<ContextMenu>;
  /** The word the selection is in or selects, for your thesaurus; `null` unless it is within one word in one paragraph. */
  wordAtSelection(): Promise<{ word: string; language: string } | null>;
  /** Runs an item: one transaction. */
  invoke(action: MenuAction): Promise<void>;
}

/** The `contextMenu` event: the user asked for a menu. */
export interface ContextMenuEvent {
  /** The menu to show. */
  menu: ContextMenu;
  /** Where to show it. */
  at: ClientPoint;
  /** A right-click, or the keyboard. */
  via: 'pointer' | 'keyboard';
}

/** A context menu. */
export interface ContextMenu {
  /** What the menu is for. */
  target: MenuTarget;
  /** The items, in order. */
  items: MenuItem[];
}

/** What a context menu is for. */
export type MenuTarget =
  | { kind: 'text' }
  | { kind: 'misspelling'; word: string; language: string }
  | { kind: 'link'; address: string }
  | { kind: 'table' }
  | { kind: 'picture' }
  | { kind: 'textBox' }
  | { kind: 'element'; id: ElementId }
  | { kind: 'revision'; id: RevisionId }
  | { kind: 'comment'; id: CommentId }
  | { kind: 'headerFooter' }
  /** Where a right-button drag of text dropped it: Move Here, Copy Here, Link Here, Create Hyperlink Here, Cancel. */
  | { kind: 'drop' };

/** A context menu item. */
export type MenuItem =
  | {
      kind: 'command';
      command: CommandId;
      label: string;
      enabled: boolean;
      checked?: boolean;
      shortcut?: string;
      action: MenuAction;
    }
  | { kind: 'suggestion'; label: string; action: MenuAction }
  /** A submenu; `inline` ones (Paste Options) show as a row of icons in the menu itself. */
  | { kind: 'submenu'; label: string; items: MenuItem[]; inline?: boolean }
  /** An action that is not one command: opening or copying a link, a drop menu choice, several commands at once, a setting. */
  | { kind: 'action'; label: string; enabled: boolean; checked?: boolean; action: MenuAction }
  | { kind: 'separator' };

declare const menuAction: unique symbol;
/** An opaque item action, only passed back to {@link Menus.invoke}. */
export type MenuAction = { readonly [menuAction]: true };

/** The menus over a session. */
export function createMenus(s: Session): Menus {
  return {
    contextMenu: (options) => s.call(Method.ContextMenu, options ?? null),
    wordAtSelection: () => s.call(Method.WordAtSelection),
    invoke: (action) => s.commit(Method.Invoke, action),
  };
}
