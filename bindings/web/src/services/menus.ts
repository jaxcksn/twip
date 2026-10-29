import type { CommandId } from '../commands.js';
import type { Session } from '../editor.js';
import { Method } from '../transport/protocol.js';
import type { ClientPoint, CommentId, ElementId, RevisionId } from '../types.js';

/** The context menu as a typed model: the application draws it. */
export interface Menus {
  /** The menu for the selection, as the context-menu key would open it. */
  contextMenu(): Promise<ContextMenu>;
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
  | { kind: 'headerFooter' };

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
  | { kind: 'submenu'; label: string; items: MenuItem[] }
  | { kind: 'separator' };

declare const menuAction: unique symbol;
/** An opaque item action, only passed back to {@link Menus.invoke}. */
export type MenuAction = { readonly [menuAction]: true };

/** The menus over a session. */
export function createMenus(s: Session): Menus {
  return {
    contextMenu: () => s.call(Method.ContextMenu),
    invoke: (action) => s.commit(Method.Invoke, action),
  };
}
