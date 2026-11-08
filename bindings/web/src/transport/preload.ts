// Starting the engine before any editor: the download, the compile and, in the worker mode, the worker.
import { useEngine } from '../fonts/library.js';
import { directTransport } from './direct.js';
import { chooseExecution, workerTransport } from './worker.js';

const loads = new Map<'main' | 'worker', Promise<void>>();

/**
 * Downloads and compiles the engine and starts it, so the first {@link Editor.mount}, {@link HeadlessEditor.create} or
 * {@link FontLibrary} call doesn't wait for it. Call it as early as the page knows it will show an editor, with no
 * document and no container. Calls share one load per execution mode, and a failed load is tried again by the next
 * call or editor.
 *
 * `execution` is the mode to prepare, as {@link MountOptions.execution} picks it (default `'auto'`: the worker where the
 * page supports one). The engine it starts is the one every later editor in that mode uses; a preloaded worker stays
 * until the first editor's session has started and closed, as any editor's worker does. Call {@link configure} first
 * if the package's files are served from elsewhere: the folder can't change after this.
 *
 * @throws {@link EditorError} `unsupported` for `execution: 'worker'` where the page can't run workers; the engine's
 * load error when the module fails to download or compile.
 */
export function preload(options: { execution?: 'auto' | 'main' | 'worker' } = {}): Promise<void> {
  let mode: 'main' | 'worker';
  try {
    mode = chooseExecution(options.execution);
  } catch (e) {
    return Promise.reject(e);
  }
  let load = loads.get(mode);
  if (!load) {
    load =
      mode === 'main'
        ? directTransport().then((t) => void useEngine(t))
        : Promise.resolve(useEngine(workerTransport())).then(() => workerTransport().preload());
    load.catch(() => loads.delete(mode));
    loads.set(mode, load);
  }
  return load;
}
