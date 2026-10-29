// The clipboard and external drops. The input element's `copy`, `cut` and `paste` events go to the session's copy and
// paste, with text, HTML and the engine's internal format (so a copy pastes with its formatting into another editor
// on the page). A drop of text from outside the editor pastes at the drop point, a drop of an image inserts it.
import type { Op } from '../commands.js';
import type { Session } from '../editor.js';
import { EditorError } from '../errors.js';
import { type ClipboardData, Kind, type HostRequest, type Message, Method } from '../transport/protocol.js';
import type { Dom, Ledger } from './dom.js';

/** The last copy on the page: the system clipboard has no place for its token, so a paste brings it back (the engine
 * pastes the copy it names only while the text is still that copy's). */
let lastCopy: ClipboardData | undefined;

/** The clipboard of a mounted editor. */
export interface Clipboard {
  /** `commands.paste`: reads the system clipboard (`not-allowed` when the browser refuses), then pastes. */
  paste(options?: { as?: 'formatted' | 'text' }): Promise<void>;
}

function escapeHtml(text: string): string {
  return text.replace(/[&<>]/g, (c) => `&${{ '&': 'amp', '<': 'lt', '>': 'gt' }[c]};`).replace(/\n/g, '<br>');
}

export function startClipboard(
  s: Session,
  dom: Dom,
  input: HTMLTextAreaElement,
  ledger: Ledger,
  report: (e: unknown) => void,
  toViewport: (e: { clientX: number; clientY: number }) => { x: number; y: number },
): Clipboard {
  const write = (data: Promise<ClipboardData | null>) => {
    // Without `ClipboardItem` (older Firefox) the event's synchronous text is the copy.
    if (typeof ClipboardItem === 'undefined' || !navigator.clipboard?.write) return;
    const blob = (type: string, pick: (d: ClipboardData) => string) =>
      data.then((d) => {
        if (!d) throw new Error('nothing was copied');
        return new Blob([pick(d)], { type });
      });
    let item: ClipboardItem;
    try {
      item = new ClipboardItem({
        'text/plain': blob('text/plain', (d) => d.text),
        'text/html': blob('text/html', (d) => d.html ?? escapeHtml(d.text)),
      });
    } catch {
      // A `ClipboardItem` that takes no promises: the synchronous text stands.
      return;
    }
    // With nothing copied the write fails, and the clipboard keeps what it had.
    navigator.clipboard.write([item]).catch((e: unknown) => data.then((d) => d && report(e), () => {}));
  };
  const onMessage = (m: Message) => {
    const r = m.payload as HostRequest;
    if (m.kind !== Kind.HostRequest || r.type !== 'clipboard') return;
    // A copy the engine asks for (a menu's).
    lastCopy = r.data;
    write(Promise.resolve(r.data));
  };
  s.hostListeners.add(onMessage);
  s.disposers.push(() => s.hostListeners.delete(onMessage));

  // Queued now, so keys pressed after it run after it.
  const copy = (cut: boolean): Promise<ClipboardData | null> =>
    s.call<ClipboardData | null>(Method.Copy, { cut }).then((d) => {
      if (d) lastCopy = d;
      return d;
    });
  const onCopy = (cut: boolean) => (e: ClipboardEvent) => {
    e.preventDefault();
    // The engine answers later (in worker mode, from another thread), so the event gets the selection's plain text
    // now, mirrored in the state: it is the copy where the formatted write below can't run.
    const text = s.state.selection.text;
    if (text) e.clipboardData?.setData('text/plain', text);
    const data = copy(cut);
    data.catch(report);
    // The write starts inside the event, which keeps the user's activation; the data follows from the engine.
    write(data);
  };
  ledger.listen(input, 'copy', onCopy(false));
  ledger.listen(input, 'cut', onCopy(true));

  const lines = (text: string) => text.replace(/\r\n/g, '\n');
  const paste = async (data: ClipboardData, asText: boolean) => {
    const text = lines(data.text);
    if (!text) return;
    const internal = lastCopy?.internal;
    await s.call(Method.Paste, { data: { ...data, text, ...(internal ? { internal } : {}) }, asText });
  };
  // Ctrl+Shift+V pastes as text.
  let plain = false;
  ledger.listen(input, 'keydown', (e: KeyboardEvent) => {
    plain = (e.ctrlKey || e.metaKey) && e.shiftKey && e.code === 'KeyV';
  });
  ledger.listen(input, 'paste', (e: ClipboardEvent) => {
    e.preventDefault();
    const t = e.clipboardData;
    if (!t) return;
    const html = t.getData('text/html');
    paste({ text: t.getData('text/plain'), ...(html ? { html } : {}) }, plain).catch(report);
    plain = false;
  });

  // External drags: text and images from outside the editor, shown with the drop caret.
  const external = (e: DragEvent) => !!e.dataTransfer && ['Files', 'text/plain', 'text/html'].some((t) => e.dataTransfer!.types.includes(t));
  ledger.listen(dom.scroller, 'dragover', (e: DragEvent) => {
    if (!external(e)) return;
    e.preventDefault();
    e.dataTransfer!.dropEffect = 'copy';
    s.call(Method.HostDragOver, toViewport(e)).catch(report);
  });
  ledger.listen(dom.scroller, 'dragleave', (e: DragEvent) => {
    if (!external(e) || dom.scroller.contains(e.relatedTarget as Node | null)) return;
    s.call(Method.HostDragLeave).catch(report);
  });
  ledger.listen(dom.scroller, 'drop', (e: DragEvent) => {
    if (!external(e)) return;
    e.preventDefault();
    const t = e.dataTransfer!;
    const at = toViewport(e);
    const image = [...t.files].find((f) => f.type.startsWith('image/'));
    const op: Promise<Op | null> = image
      ? image.arrayBuffer().then((b) => ({ type: 'insertPicture', data: new Uint8Array(b) }))
      : Promise.resolve(t.getData('text/plain') ? { type: 'pasteText', value: t.getData('text/plain') } : null);
    op.then((command) => (command ? s.commit(Method.HostDrop, { at, command }) : s.call(Method.HostDragLeave))).catch(report);
  });

  return {
    paste: async (options) => {
      let items: ClipboardItems;
      try {
        items = await navigator.clipboard.read();
      } catch (cause) {
        throw new EditorError('not-allowed', 'the browser refused to read the clipboard', { cause });
      }
      const read = async (type: string) => {
        const item = items.find((i) => i.types.includes(type));
        return item ? (await item.getType(type)).text() : '';
      };
      const [text, html] = await Promise.all([read('text/plain'), read('text/html')]);
      await s.guard(() => paste({ text, ...(html ? { html } : {}) }, options?.as === 'text'));
    },
  };
}
