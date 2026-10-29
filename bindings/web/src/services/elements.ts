import type { Op } from '../commands.js';
import type { Session } from '../editor.js';
import type { SelectionSummary } from '../state.js';
import { Method } from '../transport/protocol.js';
import type { ClientPoint, ElementId } from '../types.js';

/** A content control's kind. */
export type ControlKind =
  | 'richText'
  | 'plainText'
  | 'picture'
  | 'checkbox'
  | 'date'
  | 'dropDown'
  | 'comboBox'
  | 'group'
  | 'repeatingSection';

/** The merge designer's elements: fields and content controls. Changing them is done with the commands. */
export interface Elements {
  /** The elements, in document order. */
  list(filter?: {
    kind?: 'field' | ControlKind;
    match?: { in: 'code' | 'tag' | 'title'; text: string };
    story?: SelectionSummary['story'];
  }): Promise<ElementInfo[]>;
  /** One element in detail. */
  get(id: ElementId): Promise<ElementDetail>;
  /** The body (or an element) as blocks. */
  structure(options?: { root?: ElementId; depth?: number }): Promise<Block[]>;
  /** Selects an element; enters a header or footer when needed. */
  select(id: ElementId, options?: { reveal?: boolean }): Promise<void>;
  /** The unfilled picture placeholders, in document order (the headers and footers after the body). */
  placeholders(): Promise<PlaceholderInfo[]>;
}

/** An unfilled picture placeholder. */
export interface PlaceholderInfo {
  /** Its key: what `fillPlaceholder` takes. */
  key: string;
  /** The control it is in. */
  element: ElementId | null;
}

/** {@link Elements} on a mounted editor: adds drop targets and hit testing. */
export interface MountedElements extends Elements {
  /** What is under a point, without moving the selection. */
  hitTest(at: ClientPoint): Promise<{ element: ElementId | null; editable: boolean }>;
  /** An external drag (a field list beside the editor): shows the drop caret. */
  dragOver(at: ClientPoint): void;
  /** The external drag left the editor. */
  dragLeave(): void;
  /** Drops at the point: runs `op` there, as one transaction. */
  drop(at: ClientPoint, op: Op): Promise<void>;
}

/** An element. */
export interface ElementInfo {
  /** The element. */
  id: ElementId;
  /** Its kind. */
  kind: 'field' | ControlKind;
  /** A field's code. */
  code?: string;
  /** A control's tag. */
  tag?: string;
  /** A control's title. */
  title?: string;
  /** The result or content text, up to 200 characters. */
  display: string;
  /** The story it is in. */
  story: SelectionSummary['story'];
  /** The element it is in. */
  parent: ElementId | null;
  /** Locked against editing. */
  locked: boolean;
}

/** An element in detail. */
export interface ElementDetail extends ElementInfo {
  /** A control's lock. */
  lock: 'none' | 'control' | 'content' | 'both';
  /** A control's placeholder text. */
  placeholder?: string;
  /** A control's data binding. */
  binding?: DataBinding;
  /** A control's options. */
  options?: ControlOptions;
  /** The elements inside it. */
  children: ElementId[];
}

/** A content control's binding to a custom XML part. */
export interface DataBinding {
  /** The XPath of the bound node. */
  xpath: string;
  /** The data part's item id. */
  storeItemId?: string;
  /** The XPath's namespace prefixes. */
  prefixMappings?: string;
}

/** A content control's options, by kind: checkbox, date, list, or plain text. */
export type ControlOptions =
  | { checked?: boolean }
  | { format?: string; locale?: string }
  | { items: { text: string; value: string }[] }
  | { multiline?: boolean };

/** A block of the document structure. */
export type Block =
  | { kind: 'paragraph'; textLength: number; elements: ElementId[] }
  | {
      kind: 'table';
      rows: {
        cells: { span: number; verticalMerge: 'none' | 'start' | 'continue'; empty: boolean; blocks: Block[] }[];
      }[];
    };

/** The document's custom XML data parts. Writing and removing them is done with commands. */
export interface DataParts {
  /** Every data part. */
  list(): Promise<{ namespace: string | null; itemId: string }[]>;
  /** The data parts with a namespace or an item id. */
  get(options: { namespace?: string; itemId?: string }): Promise<{ namespace: string | null; itemId: string; xml: string }[]>;
}

/** The elements over a session. */
export function createElements(s: Session): Elements {
  return {
    list: (filter) => s.call(Method.Elements, filter ?? null),
    get: (id) => s.call(Method.Element, id),
    structure: (options) => s.call(Method.Structure, options ?? null),
    select: (id, options) => s.call(Method.SelectElement, { id, reveal: options?.reveal ?? false }),
    placeholders: () => s.call(Method.Placeholders),
  };
}

/** The data parts over a session. */
export function createDataParts(s: Session): DataParts {
  return {
    list: async () => (await s.call<{ namespace: string | null; itemId: string }[]>(Method.DataParts)).map(({ namespace, itemId }) => ({ namespace, itemId })),
    get: (options) => s.call(Method.DataParts, options),
  };
}
