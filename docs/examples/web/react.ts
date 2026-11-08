import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { Editor, type EditorState, type HeadlessEditor, type MountOptions } from '@twip/web';

/** The editor's latest state. The component re-renders after every change; null until there is an editor. */
export function useTwipState(editor: HeadlessEditor | null): EditorState | null {
  const subscribe = useCallback((onChange: () => void) => editor?.subscribe(onChange) ?? (() => {}), [editor]);
  return useSyncExternalStore(subscribe, () => editor?.getState() ?? null);
}

/** Mounts an editor in the element `ref` points at, and destroys it on unmount. */
export function useTwipEditor(options?: MountOptions) {
  const ref = useRef<HTMLDivElement>(null);
  const [editor, setEditor] = useState<Editor | null>(null);
  useEffect(() => {
    const abort = new AbortController();
    const mounting = Editor.mount(ref.current!, { ...options, signal: abort.signal });
    mounting.then(setEditor, () => {}); // an aborted mount rejects 'aborted'
    return () => {
      abort.abort();
      void mounting.then((mounted) => mounted.destroy(), () => {});
    };
  }, []); // mount once: change settings through the editor, not the options
  return { ref, editor };
}
