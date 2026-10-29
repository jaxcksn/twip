/** The stable code of an {@link EditorError}. */
export type ErrorCode =
  | 'destroyed'
  | 'invalid-argument'
  | 'unavailable'
  | 'read-only'
  | 'import-failed'
  | 'export-failed'
  | 'aborted'
  | 'not-allowed'
  | 'unsupported'
  | 'internal';

/** Why an operation failed. Every rejected promise of the API rejects with one. */
export class EditorError extends Error {
  /** The stable code: branch on this, not on the message. */
  readonly code: ErrorCode;
  /** Structured details, for some codes. */
  readonly details?: unknown;
  /** A fault: the editor rejects new operations and stays destroyable. */
  readonly fatal: boolean;

  /** Creates an error. Applications don't usually need to. */
  constructor(code: ErrorCode, message: string, options?: { details?: unknown; fatal?: boolean; cause?: unknown }) {
    super(message, options?.cause === undefined ? undefined : { cause: options.cause });
    this.name = 'EditorError';
    this.code = code;
    if (options?.details !== undefined) this.details = options.details;
    this.fatal = options?.fatal ?? false;
  }
}

/** The error a member outside what this build implements rejects or throws with. */
export function unsupported(what: string): EditorError {
  return new EditorError('unsupported', `${what} is not supported yet`);
}

/** A rejected promise with {@link unsupported}. */
export function rejectUnsupported(what: string): Promise<never> {
  return Promise.reject(unsupported(what));
}
