// IME composition on the input element goes to the engine (`twip_composition`); its end commits the text.
import type { Session } from '../editor.js';
import type { Ledger } from './dom.js';

const Phase = { start: 0, update: 1, end: 2 } as const;

export function startIme(s: Session, input: HTMLElement, ledger: Ledger, report: (e: unknown) => void): void {
  const compose = (phase: number, text: string) => s.input({ type: 'composition', phase, text }).catch(report);
  ledger.listen(input, 'compositionstart', () => compose(Phase.start, ''));
  ledger.listen(input, 'compositionupdate', (e: CompositionEvent) => compose(Phase.update, e.data));
  // The editing element puts its text back after the composition (host/input.ts).
  ledger.listen(input, 'compositionend', (e: CompositionEvent) => compose(Phase.end, e.data));
}
