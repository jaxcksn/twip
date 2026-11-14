import type { Op } from '../commands.js';
import type { Session } from '../editor.js';
import type { SelectionSummary } from '../state.js';
import { Method } from '../transport/protocol.js';
import type { ClientPoint, ClientRect, ElementId, Points } from '../types.js';
import type {
  Block,
  ControlKind,
  ControlOptions,
  DataBinding,
  ElementInfo,
  SubTemplateInfo,
} from '../wire.generated.js';

export type {
  Block,
  ControlKind,
  ControlOptions,
  DataBinding,
  ElementInfo,
  PlaceholderImage,
  ReferenceResolution,
  ReferenceRule,
  SubTemplateInfo,
  SubTemplateReference,
} from '../wire.generated.js';

/** The merge designer's elements: fields and content controls. Changing them is done with the commands. */
export interface Elements {
  /** The elements, in document order. */
  list(filter?: {
    kind?: 'field' | ControlKind;
    match?: { in: 'code' | 'tag' | 'title'; text: string };
    story?: SelectionSummary['story'];
    /** Only the elements in this sub-template's body. */
    subTemplate?: string;
  }): Promise<ElementInfo[]>;
  /** One element in detail. */
  get(id: ElementId): Promise<ElementDetail>;
  /** The body (or an element) as blocks. */
  structure(options?: { root?: ElementId; depth?: number }): Promise<Block[]>;
  /** Selects an element; enters a header or footer when needed. */
  select(id: ElementId, options?: { reveal?: boolean }): Promise<void>;
  /**
   * An element's boxes on screen, one per line it spans, for anchoring your own chrome to it: none while it is out of
   * view. Rejects with `unavailable` for an element the document doesn't have.
   */
  boxes(id: ElementId): Promise<ClientRect[]>;
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

/** The document's custom XML data parts. Writing and removing them is done with commands. */
export interface DataParts {
  /** Every data part. */
  list(): Promise<{ namespace: string | null; itemId: string }[]>;
  /** The data parts with a namespace or an item id. */
  get(options: { namespace?: string; itemId?: string }): Promise<{ namespace: string | null; itemId: string; xml: string }[]>;
}

/**
 * The template's sub-templates, and editing one in this editor. While one is edited, `state.editing` names it and
 * every command, query and event concerns its body; `editingTargetChanged` fires on each switch.
 */
export interface SubTemplates {
  /** The sub-templates the rules declare, in glossary order. */
  list(): Promise<SubTemplateInfo[]>;
  /** Edits a sub-template's body on its own pages (`category`: when several rules name categories). */
  edit(name: string, category?: string): Promise<void>;
  /** Back to the template, at its selection from before. */
  close(): Promise<void>;
  /**
   * A new sub-template: empty, with the body of the DOCX `from`, or a copy of the sub-template `copyOf` in the same
   * category (its formatting, fields, controls, pictures and references). One undo step.
   */
  create(name: string, options?: { category?: string; from?: Uint8Array; copyOf?: string }): Promise<void>;
  /**
   * A standalone DOCX of a sub-template's body, with the document's styles, numbering, theme and fonts and without
   * its headers, footers or other sub-templates; `create(name, { from })` reads it back unchanged.
   */
  export(name: string, options?: { category?: string }): Promise<Uint8Array>;
  /** Renames a sub-template; `updateReferences` renames every reference to it too. One undo step. */
  rename(name: string, to: string, options?: { category?: string; updateReferences?: boolean }): Promise<void>;
  /** Removes a sub-template; its references no longer resolve. One undo step. */
  remove(name: string, options?: { category?: string }): Promise<void>;
}

/** The sub-templates over a session. */
export function createSubTemplates(s: Session): SubTemplates {
  return {
    list: () => s.call(Method.SubTemplates),
    edit: async (name, category) => {
      await s.run({ type: 'editSubTemplate', name, ...(category === undefined ? {} : { category }) } as Op);
    },
    close: async () => {
      await s.run({ type: 'closeSubTemplate' } as Op);
    },
    create: async (name, options) => {
      await s.run({ type: 'createSubTemplate', name, ...options } as Op);
    },
    rename: async (name, to, options) => {
      await s.run({ type: 'renameSubTemplate', name, to, ...options } as Op);
    },
    remove: async (name, options) => {
      await s.run({ type: 'removeSubTemplate', name, ...options } as Op);
    },
    export: async (name, options) => {
      const docx = await s.call<string>(Method.SubTemplateExport, { name, ...options });
      return Uint8Array.from(atob(docx), (c) => c.charCodeAt(0));
    },
  };
}

/** The elements over a session. */
export function createElements(s: Session): Elements {
  return {
    list: (filter) => s.call(Method.Elements, filter ?? null),
    get: (id) => s.call(Method.Element, id),
    structure: (options) => s.call(Method.Structure, options ?? null),
    select: (id, options) => s.call(Method.SelectElement, { id, reveal: options?.reveal ?? false }),
    boxes: async (id) => {
      const boxes = await s.call<ClientRect[]>(Method.ElementBoxes, id);
      return s.clientRect ? boxes.map(s.clientRect) : boxes;
    },
  };
}

/** The data parts over a session. */
export function createDataParts(s: Session): DataParts {
  return {
    list: async () => (await s.call<{ namespace: string | null; itemId: string }[]>(Method.DataParts)).map(({ namespace, itemId }) => ({ namespace, itemId })),
    get: (options) => s.call(Method.DataParts, options),
  };
}
