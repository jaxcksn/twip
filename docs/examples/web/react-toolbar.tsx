import { useTwipEditor, useTwipState } from './react.js';

export function DocumentView({ document }: { document: Uint8Array }) {
  const { ref, editor } = useTwipEditor({ document });
  const state = useTwipState(editor);
  const bold = state?.commands.setBold;
  return (
    <div className="document-view">
      <div role="toolbar">
        <button
          disabled={!bold?.enabled}
          aria-pressed={state?.formatting.bold === true}
          onClick={() => editor?.commands.setBold(state?.formatting.bold !== true)}
        >
          Bold
        </button>
        <button disabled={!state?.history.canUndo} onClick={() => editor?.commands.undo()}>
          Undo
        </button>
      </div>
      <div ref={ref} style={{ height: '80vh' }} />
      <footer>{state && `Page ${state.pages.current} of ${state.pages.count}`}</footer>
    </div>
  );
}
