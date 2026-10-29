import type { Session } from '../editor.js';
import { Method } from '../transport/protocol.js';
import type { MatchId } from '../types.js';

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

/** What to find. */
export interface FindQuery {
  /** The text. */
  text: string;
  /** Case-sensitive. Default false. */
  matchCase?: boolean;
  /** Whole words only. Default false. */
  wholeWords?: boolean;
}

/** The result of a find step. */
export interface FindResult {
  /** A match was found and selected. */
  found: boolean;
  /** The search wrapped around the document's end. */
  wrapped: boolean;
}

/** A match in a results list. */
export interface FindMatch {
  /** The match, for {@link Find.select}. */
  id: MatchId;
  /** Some text before it. */
  before: string;
  /** The matched text. */
  text: string;
  /** Some text after it. */
  after: string;
  /** Its page. */
  page: number;
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
