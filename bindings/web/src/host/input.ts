// The editing element: an editable element in the layer, kept at the caret (for the IME window), that has the focus
// while the editor does. While focused it holds the text around the caret (the engine's caret window: the caret's
// paragraph and its neighbours, in their laid-out lines) with the selection in it, so a screen reader reads
// characters, words, lines and paragraphs as the caret moves and echoes edits. Every edit is cancelled and goes to the
// engine: keys (`twip_key`) when the engine claims them, typed text through `beforeinput` (`twip_text`); focus goes to
// `twip_focus`, and a blur never changes the selection. F6 (Shift+F6), or Escape then Tab, leaves the editor.
import type { Session } from '../editor.js';
import { Kind, Method, type CaretWindow, type HostRequest, type Message, type WindowPoint, type WindowSegment } from '../transport/protocol.js';
import type { Dom, Ledger } from './dom.js';
import { codeOf, keyNumber, modsOf } from './keys.js';

/** Keys that edit or move, with Ctrl or Shift or neither (not Alt, which the browser keeps): the engine's. */
const editingKeys = new Set([
  'Backspace', 'Delete', 'Enter', 'NumpadEnter', 'Tab', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Home', 'End',
  'PageUp', 'PageDown', 'Escape',
]);

/** The clipboard's keys: the browser turns them into `copy`, `cut` and `paste` events (host/clipboard.ts). */
const clipboardKeys = new Set(['ctrl+c', 'ctrl+x', 'ctrl+v', 'ctrl+shift+v', 'ctrl+insert', 'shift+insert', 'shift+delete']);

/** The modifier keys, by `KeyboardEvent.key`. */
const modifierKeys = new Set(['Alt', 'Control', 'Shift', 'Meta']);

/** The keys that ask for a dialog (`dialogRequested`): the engine's, though `shortcuts.lookup` names none of them. */
const dialogKeys = new Set(['ctrl+d']);

/** The key's names in shortcut strings, by `KeyboardEvent.code`, besides `KeyB` as `B` and `Digit1` as `1`. */
const names: Readonly<Record<string, string>> = {
  Minus: '-', Equal: '=', BracketLeft: '[', BracketRight: ']', Backslash: '\\', Semicolon: ';', Quote: "'", Backquote: '`',
  Comma: ',', Period: '.', Slash: '/', ArrowLeft: 'Left', ArrowRight: 'Right', ArrowUp: 'Up', ArrowDown: 'Down',
  NumpadEnter: 'Enter',
};

/** A keydown as the shortcut strings spell it: `'Ctrl+Shift+>'`, by the key's code and by the character it made. */
function chords(e: KeyboardEvent, code: string): string[] {
  // shortcut: Cmd is read as Ctrl, so the Windows shortcut strings serve the Mac too.
  const mods = `${e.ctrlKey || e.metaKey ? 'Ctrl+' : ''}${e.altKey ? 'Alt+' : ''}${e.shiftKey ? 'Shift+' : ''}`;
  const keys = new Set([names[code] ?? code.replace(/^Key|^Digit/, ''), code]);
  if (e.key.length === 1) keys.add(e.key.toUpperCase());
  return [...keys].map((k) => mods + k);
}

/** What the editing element tells screen readers about leaving it. */
export const LEAVE_HINT = 'Press F6, or Escape then Tab, to leave the editor';

/** Elements that can be Tab stops. */
const STOPS = 'a[href],area[href],button,input,select,textarea,iframe,summary,[tabindex],[contenteditable=""],[contenteditable="true"]';

/**
 * F6 and Shift+F6, as in Word (the next pane): the focus goes to the next or previous Tab stop outside `editor`. Screen
 * readers keep the first Escape for themselves (NVDA's leaves focus mode), so Escape then Tab doesn't get out with one.
 */
// ponytail: document order and tabIndex >= 0, not positive tabindex order; follow it if a page relies on that.
function leave(from: HTMLElement, editor: HTMLElement, back: boolean): void {
  const stops = [...document.querySelectorAll<HTMLElement>(STOPS)].filter(
    (e) => !editor.contains(e) && e.tabIndex >= 0 && !(e as HTMLButtonElement).disabled && e.getClientRects().length > 0,
  );
  const after = (e: HTMLElement) => (from.compareDocumentPosition(e) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
  (back ? stops.filter((e) => !after(e)).at(-1) : stops.find(after))?.focus();
}

/** The input element of a mounted editor. */
export interface Input {
  readonly element: HTMLElement;
  focus(): void;
}

type Paragraph = CaretWindow['paragraphs'][number];

/** A segment's element: plain text, or an element carrying its marks and language. */
function segment(s: WindowSegment): Node {
  const marks = s.marks ?? [];
  if (!marks.length && !s.lang) return document.createTextNode(s.text);
  const e = document.createElement(marks.includes('insertion') ? 'ins' : marks.includes('deletion') ? 'del' : 'span');
  if (marks.includes('misspelling')) e.setAttribute('aria-invalid', 'spelling');
  if (s.author && (marks.includes('insertion') || marks.includes('deletion'))) e.setAttribute('aria-description', `by ${s.author}`);
  if (s.lang) e.lang = s.lang;
  e.textContent = s.text;
  return e;
}

/**
 * A window paragraph's element: one block, so a screen reader's paragraph is the paragraph, with its lines where the
 * canvas has them. Each line is a span that doesn't wrap, with a break point (`<wbr>`) after it; the element is 1 px
 * wide, so the browser breaks at every break point and nowhere else: its lines are the canvas's. A `<br>` would end a
 * paragraph for the screen reader too. A list item's label comes first.
 */
function paragraph(p: Paragraph): HTMLElement {
  const e = document.createElement('div');
  e.dataset.twipId = String(p.id);
  if (p.role === 'heading') {
    e.setAttribute('role', 'heading');
    e.setAttribute('aria-level', String(p.level ?? 1));
  }
  if (p.label) {
    const label = document.createElement('span');
    label.dataset.twipLabel = '';
    label.style.whiteSpace = 'pre';
    label.textContent = `${p.label} `;
    e.append(label);
  }
  p.lines.forEach((line, i) => {
    if (i > 0) e.append(document.createElement('wbr'));
    const l = document.createElement('span');
    l.dataset.twipLine = '';
    l.style.whiteSpace = 'pre';
    for (const s of line.segments) l.append(segment(s));
    e.append(l);
  });
  // An empty paragraph still has a line to put the caret on.
  if (!e.textContent || e.textContent === (p.label ? `${p.label} ` : '')) e.append(document.createElement('br'));
  return e;
}

/** The text nodes of a paragraph's element that count for offsets: not its label's. */
function texts(e: HTMLElement): Text[] {
  const out: Text[] = [];
  const walker = document.createTreeWalker(e, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => ((n.parentElement?.closest('[data-twip-label]') ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT)),
  });
  for (let n = walker.nextNode(); n; n = walker.nextNode()) out.push(n as Text);
  return out;
}

/** The DOM place of a window place. */
function domPoint(paragraphs: HTMLElement[], p: WindowPoint): { node: Node; offset: number } | undefined {
  const e = paragraphs[p.paragraph];
  if (!e) return undefined;
  let left = p.offset;
  const nodes = texts(e);
  for (const t of nodes) {
    if (left <= t.length) return { node: t, offset: left };
    left -= t.length;
  }
  const last = nodes.at(-1);
  // An empty paragraph: before its `<br>`, after its label.
  return last ? { node: last, offset: last.length } : { node: e, offset: e.querySelector('[data-twip-label]') ? 1 : 0 };
}

/** The window place of a DOM place, or none outside the element's paragraphs. */
function windowPoint(paragraphs: HTMLElement[], node: Node, offset: number): WindowPoint | undefined {
  const k = paragraphs.findIndex((e) => e.contains(node));
  const e = paragraphs[k];
  if (!e) return undefined;
  // The text from the paragraph's start to the place, without the label (always first).
  const r = document.createRange();
  r.setStart(e, 0);
  r.setEnd(node, offset);
  const label = e.querySelector('[data-twip-label]')?.textContent?.length ?? 0;
  return { paragraph: k, offset: Math.max(0, r.toString().length - label) };
}

const same = (a: WindowPoint | undefined, b: WindowPoint | undefined) => a?.paragraph === b?.paragraph && a?.offset === b?.offset;

/**
 * Starts keyboard and text input. `claims` says whether a chord is a command's, an application shortcut or a host
 * key (`shortcuts.lookup`): those keys are the editor's, and their browser defaults (Save, Print, Find) don't run.
 * `label` is the editor's accessible name.
 */
export function startInput(
  s: Session,
  dom: Dom,
  ledger: Ledger,
  report: (e: unknown) => void,
  claims: (chord: string) => boolean,
  label: string,
): Input {
  const input = ledger.node(dom.layer, document.createElement('div'));
  input.dataset.twip = 'input';
  input.contentEditable = 'true';
  input.setAttribute('role', 'textbox');
  input.setAttribute('aria-multiline', 'true');
  input.setAttribute('aria-label', label);
  input.setAttribute('aria-description', LEAVE_HINT);
  input.setAttribute('autocomplete', 'off');
  input.setAttribute('autocorrect', 'off');
  input.setAttribute('autocapitalize', 'off');
  input.spellcheck = false;
  input.style.cssText =
    'position:absolute;left:0;top:0;width:1px;height:1em;opacity:0;border:0;padding:0;margin:0;overflow:hidden;' +
    'white-space:normal;overflow-wrap:normal;outline:none;caret-color:transparent;pointer-events:none;';

  // The window: paragraphs by id, with what they were built from, so only changed ones are replaced.
  const built = new Map<number, { e: HTMLElement; from: string }>();
  let shown: HTMLElement[] = [];
  let latest: CaretWindow | undefined;
  let composing = false;
  // The selection the host last set, so the selectionchange it causes isn't taken for the screen reader's.
  let set: { anchor: WindowPoint; focus: WindowPoint } | undefined;

  const render = (w: CaretWindow) => {
    input.lang = w.lang;
    if (w.readOnly) input.setAttribute('aria-readonly', 'true');
    else input.removeAttribute('aria-readonly');
    // The text is there only while the element has the focus: in browse mode a screen reader then finds the document
    // once, in the structure mirror, and not again (the caret's paragraphs) in the editing element.
    if (document.activeElement !== input) return;
    const next = w.paragraphs.map((p) => {
      const from = JSON.stringify(p);
      const old = built.get(p.id);
      if (old?.from === from) return old.e;
      const e = paragraph(p);
      built.set(p.id, { e, from });
      return e;
    });
    const keep = new Set(next);
    for (const child of [...input.children]) if (!keep.has(child as HTMLElement)) child.remove();
    for (const [id, b] of built) if (!keep.has(b.e)) built.delete(id);
    // Only elements out of place move: the others stay where the screen reader is reading.
    next.forEach((e, i) => {
      if (input.children[i] !== e) input.insertBefore(e, input.children[i] ?? null);
    });
    shown = next;
    const a = domPoint(shown, w.anchor);
    const f = domPoint(shown, w.focus);
    if (!a || !f) return;
    set = { anchor: w.anchor, focus: w.focus };
    getSelection()?.setBaseAndExtent(a.node, a.offset, f.node, f.offset);
  };

  // Escape, then Tab or Shift+Tab: the browser moves the focus out; Escape still goes to the engine.
  let escaped = false;
  // A modifier pressed or let go alone (Alt mid-drag): the engine runs the drag or hover again with it.
  const modifier = (e: KeyboardEvent) => {
    if (!modifierKeys.has(e.key) || e.repeat) return false;
    s.input({ type: 'modifiers', mods: modsOf(e) }).catch(report);
    return true;
  };
  ledger.listen(input, 'keyup', modifier);
  ledger.listen(input, 'keydown', (e: KeyboardEvent) => {
    if (e.isComposing || e.keyCode === 229 || modifier(e)) return;
    const code = codeOf(e);
    if (code === 'F6' && !e.ctrlKey && !e.altKey && !e.metaKey) {
      e.preventDefault();
      escaped = false;
      leave(input, dom.scroller, e.shiftKey);
      return;
    }
    const leaving = escaped && code === 'Tab' && !e.ctrlKey && !e.altKey && !e.metaKey;
    escaped = code === 'Escape' && !e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey;
    if (leaving) return;
    const key = keyNumber(code);
    if (!key) return;
    const all = chords(e, code);
    if (all.some((c) => clipboardKeys.has(c.toLowerCase()))) return;
    // The menu keys open the application's menu only when it listens; otherwise the browser's opens.
    const menu = code === 'ContextMenu' || all.includes('Shift+F10');
    if (menu && !s.listening('contextMenu')) return;
    const claimed = all.some((c) => claims(c) || dialogKeys.has(c.toLowerCase()));
    if (!menu && !(!e.altKey && editingKeys.has(code)) && !claimed) return;
    e.preventDefault();
    s.input({ type: 'key', key, mods: modsOf(e) }).catch(report);
  });
  // Nothing edits the element itself: typed text goes to the engine, and the engine's window comes back. Composition
  // can't be cancelled; the window is put back after it ends.
  ledger.listen(input, 'beforeinput', (e: InputEvent) => {
    if (e.inputType === 'insertCompositionText') return;
    e.preventDefault();
    if (e.inputType !== 'insertText' && e.inputType !== 'insertReplacementText') return;
    const text = e.data ?? e.dataTransfer?.getData('text/plain');
    if (text) s.input({ type: 'text', text }).catch(report);
  });
  ledger.listen(input, 'compositionstart', () => (composing = true));
  ledger.listen(input, 'compositionend', () => {
    composing = false;
    // The browser put the composition in the paragraphs: build them again.
    built.clear();
    if (latest) render(latest);
  });
  const focused = (focused: boolean) => () => {
    if (focused && latest) render(latest);
    if (!focused) {
      input.replaceChildren();
      built.clear();
      shown = [];
    }
    s.input({ type: 'focus', focused }).catch(report);
  };
  ledger.listen(input, 'focus', focused(true));
  ledger.listen(input, 'blur', focused(false));

  // A selection the host didn't set: the screen reader moved the caret (reading continuously). The engine follows.
  ledger.listen(document as unknown as HTMLElement, 'selectionchange', () => {
    if (document.activeElement !== input || composing || s.destroyed) return;
    const sel = getSelection();
    if (!sel?.anchorNode || !sel.focusNode) return;
    const anchor = windowPoint(shown, sel.anchorNode, sel.anchorOffset);
    const focus = windowPoint(shown, sel.focusNode, sel.focusOffset);
    if (!anchor || !focus || (same(anchor, set?.anchor) && same(focus, set?.focus))) return;
    set = { anchor, focus };
    s.call(Method.HostCaretWindowSelect, { anchor, focus }).catch(report);
  });

  const onMessage = (m: Message) => {
    if (m.kind !== Kind.HostRequest) return;
    const r = m.payload as HostRequest;
    // The caret request: the element follows the caret, so the IME's window opens beside it.
    if (r.type === 'caret') {
      input.style.left = `${r.x}px`;
      input.style.top = `${r.y}px`;
      input.style.height = `${Math.max(1, r.height)}px`;
      input.style.fontSize = `${Math.max(1, r.height * 0.8)}px`;
    } else if (r.type === 'caret_text') {
      latest = r.window;
      if (!composing) render(r.window);
    }
  };
  s.hostListeners.add(onMessage);
  s.disposers.push(() => s.hostListeners.delete(onMessage));

  return { element: input, focus: () => input.focus({ preventScroll: true }) };
}
