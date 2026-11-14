import type { CommandId } from '../commands.js';
import type { Session } from '../editor.js';
import { Method } from '../transport/protocol.js';
import type { ClientPoint, CommentId, ElementId, RevisionId } from '../types.js';
import type { ContextMenu } from '../wire.generated.js';

export type { ContextMenu, ContextMenuEvent, MenuItem, MenuTarget } from '../wire.generated.js';

/** The context menu as a typed model: the application draws it. */
export interface Menus {
  /** The menu for the selection, as the context-menu key would open it. */
  contextMenu(): Promise<ContextMenu>;
  /** Runs an item: one transaction. */
  invoke(action: MenuAction): Promise<void>;
}

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
