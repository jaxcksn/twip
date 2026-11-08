import type { EditorState, HeadlessEditor } from '@twip/web';

export function bindToolbar(editor: HeadlessEditor, bold: HTMLButtonElement, undo: HTMLButtonElement, status: HTMLElement) {
  // Called now with the current state, then after every change. One immutable snapshot each time.
  const render = (state: EditorState) => {
    const command = state.commands.setBold;
    bold.disabled = !command.enabled;
    bold.ariaPressed = state.formatting.bold === 'mixed' ? 'mixed' : String(state.formatting.bold === true);
    undo.disabled = !state.history.canUndo;
    undo.title = state.history.undoLabel ? `Undo ${state.history.undoLabel}` : 'Undo';
    status.textContent = `Page ${state.pages.current} of ${state.pages.count}${state.modified ? ' (modified)' : ''}`;
  };
  const unsubscribe = editor.subscribe(render);

  // Setters take a value: the toolbar passes the opposite of what the state shows.
  bold.onclick = () => editor.commands.setBold(editor.getState().formatting.bold !== true);
  undo.onclick = () => editor.commands.undo();
  return unsubscribe;
}
