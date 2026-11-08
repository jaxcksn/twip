import { Component, DestroyRef, ElementRef, afterNextRender, computed, inject, viewChild } from '@angular/core';
import { Editor } from '@twip/web';
import { TwipStateService } from './angular.js';

@Component({
  selector: 'app-document',
  providers: [TwipStateService],
  template: `
    <div role="toolbar">
      <button [disabled]="!bold()?.enabled" [attr.aria-pressed]="isBold()" (click)="toggleBold()">Bold</button>
    </div>
    <div #host style="height: 80vh"></div>
    <footer>Page {{ twip.state()?.pages.current }} of {{ twip.state()?.pages.count }}</footer>
  `,
})
export class DocumentComponent {
  protected readonly twip = inject(TwipStateService);
  private readonly host = viewChild.required<ElementRef<HTMLElement>>('host');
  private editor: Editor | null = null;

  protected readonly bold = computed(() => this.twip.state()?.commands.setBold);
  protected readonly isBold = computed(() => this.twip.state()?.formatting.bold === true);

  constructor() {
    const abort = new AbortController();
    inject(DestroyRef).onDestroy(() => {
      abort.abort();
      void this.editor?.destroy();
    });
    afterNextRender(async () => {
      this.editor = await Editor.mount(this.host().nativeElement, { signal: abort.signal });
      this.twip.attach(this.editor);
      this.twip.on('hostCommand', ({ command }) => console.log('host command', command));
    });
  }

  protected toggleBold() {
    void this.editor?.commands.setBold(!this.isBold());
  }
}
