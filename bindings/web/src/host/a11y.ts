// The accessibility DOM: the engine's tree (the `a11y` request) mirrored as ARIA elements in the layer, out of
// sight, and the editor's announcements in a polite live region.
import type { Session } from '../editor.js';
import type { AccessibleNode } from '../services/accessibility.js';
import { Kind, type HostRequest, type Message } from '../transport/protocol.js';
import type { Dom, Ledger } from './dom.js';

const roles: Readonly<Record<AccessibleNode['role'], string>> = {
  document: 'document',
  page: 'region',
  paragraph: 'paragraph',
  heading: 'heading',
  list: 'list',
  listItem: 'listitem',
  table: 'table',
  row: 'row',
  cell: 'cell',
  image: 'img',
  link: 'link',
  textBox: 'group',
  note: 'note',
  header: 'region',
  footer: 'region',
};

const hidden = 'position:absolute;left:0;top:0;width:1px;height:1px;overflow:hidden;clip-path:inset(50%);white-space:nowrap;';

function build(node: AccessibleNode): HTMLElement {
  const e = document.createElement('div');
  e.setAttribute('role', roles[node.role] ?? 'group');
  if (node.name) e.setAttribute('aria-label', node.name);
  if (node.level !== undefined) e.setAttribute('aria-level', String(node.level));
  const marks = node.marks ?? [];
  if (marks.includes('misspelling')) e.setAttribute('aria-invalid', 'spelling');
  const review = marks.filter((m) => m !== 'misspelling');
  if (review.length) e.setAttribute('aria-description', review.join(' '));
  if (node.text) e.append(node.text);
  for (const child of node.children) e.append(build(child));
  return e;
}

export function startA11y(s: Session, dom: Dom, ledger: Ledger): void {
  const tree = ledger.node(dom.layer, document.createElement('div'));
  tree.dataset.twip = 'a11y';
  tree.style.cssText = hidden;
  const live = ledger.node(dom.layer, document.createElement('div'));
  live.dataset.twip = 'live';
  live.setAttribute('aria-live', 'polite');
  live.setAttribute('role', 'status');
  live.style.cssText = hidden;

  const onMessage = (m: Message) => {
    const r = m.payload as HostRequest;
    if (m.kind === Kind.HostRequest && r.type === 'a11y') tree.replaceChildren(build(r.tree));
  };
  s.hostListeners.add(onMessage);
  s.disposers.push(() => s.hostListeners.delete(onMessage));
  // A repeated announcement is read again: the region empties first.
  let cancel: (() => void) | undefined;
  s.disposers.push(
    s.on('announcement', ({ text }) => {
      live.textContent = '';
      cancel?.();
      cancel = ledger.timeout(() => (live.textContent = text), 50);
    }),
  );
}
