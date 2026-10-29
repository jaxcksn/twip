// The input element: a hidden textarea in the layer, kept at the caret (for the IME window), that has the focus
// while the editor does. Keys go to the engine (`twip_key`) when the engine claims them, typed text arrives through
// `beforeinput` (`twip_text`), and focus goes to `twip_focus`; a blur never changes the selection.
import type { Session } from '../editor.js';
import { Kind, type HostRequest, type Message } from '../transport/protocol.js';
import type { Dom, Ledger } from './dom.js';
import { keyNumber, modsOf } from './keys.js';

/** Keys that edit or move, with Ctrl or Shift or neither (not Alt, which the browser keeps): the engine's. */
const editingKeys = new Set([
  'Backspace', 'Delete', 'Enter', 'NumpadEnter', 'Tab', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Home', 'End',
  'PageUp', 'PageDown', 'Escape',
]);

/** The clipboard's keys: the browser turns them into `copy`, `cut` and `paste` events (host/clipboard.ts). */
const clipboardKeys = new Set(['ctrl+c', 'ctrl+x', 'ctrl+v', 'ctrl+shift+v', 'ctrl+insert', 'shift+insert', 'shift+delete']);

/** The keys that ask for a dialog (`dialogRequested`): the engine's, though `shortcuts.lookup` names none of them. */
const dialogKeys = new Set(['ctrl+d']);

/** The key's names in shortcut strings, by `KeyboardEvent.code`, besides `KeyB` as `B` and `Digit1` as `1`. */
const names: Readonly<Record<string, string>> = {
  Minus: '-', Equal: '=', BracketLeft: '[', BracketRight: ']', Backslash: '\\', Semicolon: ';', Quote: "'", Backquote: '`',
  Comma: ',', Period: '.', Slash: '/', ArrowLeft: 'Left', ArrowRight: 'Right', ArrowUp: 'Up', ArrowDown: 'Down',
  NumpadEnter: 'Enter',
};

/** A keydown as the shortcut strings spell it: `'Ctrl+Shift+>'`, by the key's code and by the character it made. */
function chords(e: KeyboardEvent): string[] {
  // shortcut: Cmd is read as Ctrl, so the Windows shortcut strings serve the Mac too.
  const mods = `${e.ctrlKey || e.metaKey ? 'Ctrl+' : ''}${e.altKey ? 'Alt+' : ''}${e.shiftKey ? 'Shift+' : ''}`;
  const keys = new Set([names[e.code] ?? e.code.replace(/^Key|^Digit/, ''), e.code]);
  if (e.key.length === 1) keys.add(e.key.toUpperCase());
  return [...keys].map((k) => mods + k);
}

/** The input element of a mounted editor. */
export interface Input {
  readonly element: HTMLTextAreaElement;
  focus(): void;
}

/**
 * Starts keyboard and text input. `claims` says whether a chord is a command's, an application shortcut or a host
 * key (`shortcuts.lookup`): those keys are the editor's, and their browser defaults (Save, Print, Find) don't run.
 */
export function startInput(s: Session, dom: Dom, ledger: Ledger, report: (e: unknown) => void, claims: (chord: string) => boolean): Input {
  const input = ledger.node(dom.layer, document.createElement('textarea'));
  input.dataset.twip = 'input';
  input.setAttribute('aria-label', 'Document');
  input.setAttribute('aria-multiline', 'true');
  input.setAttribute('autocomplete', 'off');
  input.setAttribute('autocorrect', 'off');
  input.setAttribute('autocapitalize', 'off');
  input.spellcheck = false;
  input.style.cssText =
    'position:absolute;left:0;top:0;width:1px;height:1em;opacity:0;border:0;padding:0;margin:0;resize:none;overflow:hidden;' +
    'white-space:pre;outline:none;caret-color:transparent;pointer-events:none;';

  ledger.listen(input, 'keydown', (e: KeyboardEvent) => {
    if (e.isComposing || e.keyCode === 229) return;
    const key = keyNumber(e.code);
    if (!key) return;
    const all = chords(e);
    if (all.some((c) => clipboardKeys.has(c.toLowerCase()))) return;
    // The menu keys open the application's menu only when it listens; otherwise the browser's opens.
    const menu = e.code === 'ContextMenu' || all.includes('Shift+F10');
    if (menu && !s.listening('contextMenu')) return;
    const claimed = all.some((c) => claims(c) || dialogKeys.has(c.toLowerCase()));
    if (!menu && !(!e.altKey && editingKeys.has(e.code)) && !claimed) return;
    e.preventDefault();
    s.input({ type: 'key', key, mods: modsOf(e) }).catch(report);
  });
  ledger.listen(input, 'beforeinput', (e: InputEvent) => {
    if (e.inputType !== 'insertText' && e.inputType !== 'insertReplacementText') return;
    e.preventDefault();
    const text = e.data ?? e.dataTransfer?.getData('text/plain');
    if (text) s.input({ type: 'text', text }).catch(report);
  });
  const focused = (focused: boolean) => () => s.input({ type: 'focus', focused }).catch(report);
  ledger.listen(input, 'focus', focused(true));
  ledger.listen(input, 'blur', focused(false));

  // The caret request: the input element follows the caret, so the IME's window opens beside it.
  const onMessage = (m: Message) => {
    const r = m.payload as HostRequest;
    if (m.kind !== Kind.HostRequest || r.type !== 'caret') return;
    input.style.left = `${r.x}px`;
    input.style.top = `${r.y}px`;
    input.style.height = `${Math.max(1, r.height)}px`;
    input.style.fontSize = `${Math.max(1, r.height * 0.8)}px`;
  };
  s.hostListeners.add(onMessage);
  s.disposers.push(() => s.hostListeners.delete(onMessage));

  return { element: input, focus: () => input.focus({ preventScroll: true }) };
}
