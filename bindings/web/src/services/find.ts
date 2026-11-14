import type { Session } from '../editor.js';
import { Method } from '../transport/protocol.js';
import type { MatchId } from '../types.js';
import type { FindMatch, FindQuery, FindResult } from '../wire.generated.js';

export type { FindMatch, FindQuery, FindResult } from '../wire.generated.js';

/** Find and replace, over the text of the current markup view. */
export interface Find {
  /** Selects and reveals the next match. */
  next(query: FindQuery): Promise<FindResult>;
  /** Selects and reveals the previous match. */
  previous(query: FindQuery): Promise<FindResult>;
  /** Every match, for a navigation pane's results list. */
  all(query: FindQuery): Promise<FindMatch[]>;
  /** Selects and reveals a match from {@link Find.all}. */
  select(match: MatchId): Promise<void>;
  /** Replaces the current match, then finds the next. */
  replace(query: FindQuery, replacement: string): Promise<FindResult>;
  /** Replaces every match: one transaction. */
  replaceAll(query: FindQuery, replacement: string): Promise<{ count: number }>;
}

/** Find and replace over a session. */
export function createFind(s: Session): Find {
  return {
    next: (query) => s.call(Method.Find, { query, direction: 'next' }),
    previous: (query) => s.call(Method.Find, { query, direction: 'previous' }),
    all: (query) => s.call(Method.FindAll, query),
    select: (match) => s.call(Method.SelectMatch, match),
    replace: (query, replacement) => s.call(Method.Replace, { query, with: replacement }),
    replaceAll: (query, replacement) => s.call(Method.ReplaceAll, { query, with: replacement }),
  };
}
