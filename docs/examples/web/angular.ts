import { DestroyRef, Injectable, inject, signal } from '@angular/core';
import type { EditorState, HeadlessEditor, TwipEvents } from '@twip/web';

/** One editor's state as a signal, and its events, for the lifetime of the injector that provides it. */
@Injectable()
export class TwipStateService {
  private readonly destroyRef = inject(DestroyRef);
  private readonly current = signal<EditorState | null>(null);
  private editor: HeadlessEditor | null = null;

  /** The latest state; null until an editor is attached. */
  readonly state = this.current.asReadonly();

  /** Follows `editor`: `state` updates after every change. */
  attach(editor: HeadlessEditor): void {
    this.editor = editor;
    this.destroyRef.onDestroy(editor.subscribe((state) => this.current.set(state)));
  }

  /** Listens to one of the editor's events until the injector is destroyed. */
  on<K extends keyof TwipEvents>(type: K, listener: (event: TwipEvents[K]) => void): void {
    if (!this.editor) throw new Error('attach an editor first');
    this.destroyRef.onDestroy(this.editor.on(type, listener));
  }
}
