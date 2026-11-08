// The accessibility DOM: the engine's reading tree (the `a11y` request) mirrored as ARIA elements in the layer, out of
// sight but not hidden from assistive technology, and the editor's announcements in a polite live region. Elements
// are kept by node id, so a change replaces only the nodes that changed and a screen reader's reading position
// survives edits elsewhere. Activating an element (a screen reader's Enter in browse mode is a click) puts the caret
// there and focuses the editing element.
import type { Session } from '../editor.js';
import type { AccessibleId, AccessibleNode } from '../services/accessibility.js';
import { Kind, Method, type A11yUpdate, type HostRequest, type Message } from '../transport/protocol.js';
import type { Dom, Ledger } from './dom.js';

type Role = AccessibleNode['role'];

/** Each role's element and ARIA role. Inline nodes are spans; the rest are divs. */
const elements: Readonly<Record<Role, { tag: 'div' | 'span'; role?: string }>> = {
  document: { tag: 'div', role: 'document' },
  paragraph: { tag: 'div', role: 'paragraph' },
  heading: { tag: 'div', role: 'heading' },
  list: { tag: 'div', role: 'list' },
  listItem: { tag: 'div', role: 'listitem' },
  table: { tag: 'div', role: 'table' },
  row: { tag: 'div', role: 'row' },
  cell: { tag: 'div', role: 'cell' },
  columnHeader: { tag: 'div', role: 'columnheader' },
  rowHeader: { tag: 'div', role: 'rowheader' },
  image: { tag: 'span', role: 'img' },
  link: { tag: 'span', role: 'link' },
  textBox: { tag: 'div', role: 'group' },
  note: { tag: 'div', role: 'note' },
  noteRef: { tag: 'span', role: 'link' },
  comment: { tag: 'div', role: 'comment' },
  // Named groups inside the headers and footers region: the landmark key goes from region to region, not into each.
  header: { tag: 'div', role: 'group' },
  footer: { tag: 'div', role: 'group' },
  region: { tag: 'div', role: 'region' },
  text: { tag: 'span' },
};

/** Nodes that hold no caret position of their own: the rest take the focus, and focusing one starts editing there. */
const unfocused = new Set<Role>(['document', 'region', 'header', 'footer', 'list', 'table', 'row', 'text']);

// Out of sight, one line per block: wrapped at 1px, a screen reader using the screen's layout reads a word a line.
const hidden = 'position:absolute;left:0;top:0;width:1px;height:1px;overflow:hidden;clip-path:inset(50%);white-space:pre;';

/** Mounts on one page get distinct element ids: `aria-details` refers to them. */
let mounts = 0;

/** The mirror of one mount's tree. */
class Mirror {
  readonly #byId = new Map<AccessibleId, HTMLElement>();
  readonly #prefix = `twip-${++mounts}-`;

  readonly root: HTMLElement;
  readonly label: string;

  constructor(root: HTMLElement, label: string) {
    this.root = root;
    this.label = label;
  }

  /** The element id of node `id`, for `aria-details`. */
  #elementId(id: AccessibleId): string {
    return this.#prefix + id;
  }

  #build(node: AccessibleNode): HTMLElement {
    const { tag, role } = elements[node.role] ?? elements.paragraph;
    // Insertions and deletions are ins and del, which screen readers report as such.
    const marks = node.marks ?? [];
    const e = document.createElement(marks.includes('insertion') ? 'ins' : marks.includes('deletion') ? 'del' : tag);
    e.dataset.twipId = String(node.id);
    e.id = this.#elementId(node.id);
    if (role) e.setAttribute('role', role);
    const name = node.role === 'document' ? this.label : node.name;
    // A note reference holds no text of its own: its name is its text (a name alone, Chrome's NVDA reads as a letter).
    if (name && node.role === 'noteRef' && !node.text && !node.children.length) e.append(name);
    else if (name && node.role !== 'text') e.setAttribute('aria-label', name);
    if (node.level !== undefined && (node.role === 'heading' || node.role === 'listItem')) e.setAttribute('aria-level', String(node.level));
    if (node.position) {
      e.setAttribute('aria-posinset', String(node.position.index));
      e.setAttribute('aria-setsize', String(node.position.count));
    }
    if (node.cell) {
      e.setAttribute('aria-colindex', String(node.cell.col));
      if (node.cell.colSpan > 1) e.setAttribute('aria-colspan', String(node.cell.colSpan));
      if (node.cell.rowSpan > 1) e.setAttribute('aria-rowspan', String(node.cell.rowSpan));
    }
    if (marks.includes('misspelling')) e.setAttribute('aria-invalid', 'spelling');
    if (node.author && (marks.includes('insertion') || marks.includes('deletion'))) e.setAttribute('aria-description', `by ${node.author}`);
    if (node.lang) e.lang = node.lang;
    if (node.details !== undefined) e.setAttribute('aria-details', this.#elementId(node.details));
    // Focusable, as a screen reader's activation (browse mode Enter) focuses the element before it clicks.
    if (!unfocused.has(node.role)) e.tabIndex = -1;
    if (node.label) e.append(Object.assign(document.createElement('span'), { textContent: `${node.label} ` }));
    if (node.text) e.append(node.text);
    for (const child of node.children) e.append(this.#build(child));
    // A row's index: its cells'.
    if (node.role === 'row') {
      const row = node.children.find((c) => c.cell)?.cell?.row;
      if (row !== undefined) e.setAttribute('aria-rowindex', String(row));
    }
    this.#byId.set(node.id, e);
    return e;
  }

  #forget(e: Element): void {
    for (const x of [e, ...e.querySelectorAll('[data-twip-id]')]) {
      const id = Number((x as HTMLElement).dataset.twipId);
      if (this.#byId.get(id) === x) this.#byId.delete(id);
    }
  }

  update(update: A11yUpdate): void {
    if ('full' in update) {
      this.#byId.clear();
      this.root.replaceChildren(this.#build(update.full));
      return;
    }
    // Replaced nodes not in the tree yet wait for the `children` change that places them.
    const detached = new Map<AccessibleId, HTMLElement>();
    for (const change of update.patch) {
      if (change.type === 'replace') {
        const old = this.#byId.get(change.node.id);
        if (old) this.#forget(old);
        const e = this.#build(change.node);
        if (old) old.replaceWith(e);
        else detached.set(change.node.id, e);
        continue;
      }
      const parent = this.#byId.get(change.parent);
      if (!parent) continue;
      const wanted = change.ids.map((id) => detached.get(id) ?? this.#byId.get(id)).filter((e): e is HTMLElement => !!e);
      const keep = new Set(wanted);
      for (const child of [...parent.children]) {
        if (!keep.has(child as HTMLElement)) {
          this.#forget(child);
          child.remove();
        }
      }
      // Only elements out of place move: the others stay where the screen reader may be reading.
      wanted.forEach((e, i) => {
        if (parent.children[i] !== e) parent.insertBefore(e, parent.children[i] ?? null);
      });
    }
  }
}

/** How long an announcement waits after emptying the live region. */
const LIVE_MS = 50;

/** How long after the focus comes into the editing element an announcement waits: past the focus's own speech. */
const AFTER_FOCUS_MS = 600;

/** How long after a mirror node's activation a press on the editor is the activation's own mouse click. */
const ACTIVATION_MS = 500;

/** The mirror's part in pointer input. */
export interface A11y {
  /** Whether a mirror node was just activated: a press now is the screen reader's click, and places no caret. */
  activating(): boolean;
}

/** Mirrors the tree and speaks announcements. `label` is the editor's accessible name; `focus` focuses the editing element. */
export function startA11y(s: Session, dom: Dom, ledger: Ledger, label: string, focus: () => void, report: (e: unknown) => void): A11y {
  const tree = ledger.node(dom.layer, document.createElement('div'));
  tree.dataset.twip = 'a11y';
  tree.style.cssText = hidden;
  const live = ledger.node(dom.layer, document.createElement('div'));
  live.dataset.twip = 'live';
  live.setAttribute('aria-live', 'polite');
  live.setAttribute('role', 'status');
  live.style.cssText = hidden;
  const mirror = new Mirror(tree, label);

  const onMessage = (m: Message) => {
    const r = m.payload as HostRequest;
    if (m.kind === Kind.HostRequest && r.type === 'a11y') mirror.update(r.update);
  };
  s.hostListeners.add(onMessage);
  s.disposers.push(() => s.hostListeners.delete(onMessage));

  // Activating a node starts editing there. A screen reader's activation focuses the node, then clicks it, or clicks
  // with the mouse where it is: that lands on the canvas, and is the activation's, not a click to place the caret.
  let activated = -Infinity;
  const activate = (e: Event) => {
    // Regions and the document hold no caret position: the nearest node inside one that does.
    const at = (e.target as Element | null)?.closest<HTMLElement>('[data-twip-id][tabindex]');
    if (!at || s.destroyed) return;
    e.preventDefault();
    activated = performance.now();
    s.call(Method.AccessibleGo, Number(at.dataset.twipId)).then(focus, report);
  };
  ledger.listen(tree, 'click', activate);
  ledger.listen(tree, 'focusin', activate);

  // A repeated announcement is read again: the region empties first. One made as the focus comes into the editing
  // element waits for the screen reader to start saying the focus (NVDA does about 150 ms later, and cancels what it
  // was saying): it is then said after it, not cut off by it ("Editing sub-template Violation" from a button).
  let cancel: (() => void) | undefined;
  let focusedAt = -Infinity;
  ledger.listen(dom.layer, 'focusin', (e: FocusEvent) => {
    if ((e.target as Element).matches('[data-twip="input"]')) focusedAt = performance.now();
  });
  s.disposers.push(
    s.on('announcement', ({ text }) => {
      live.textContent = '';
      cancel?.();
      const wait = Math.max(LIVE_MS, focusedAt + AFTER_FOCUS_MS - performance.now());
      cancel = ledger.timeout(() => (live.textContent = text), wait);
    }),
  );
  return { activating: () => performance.now() - activated < ACTIVATION_MS };
}
