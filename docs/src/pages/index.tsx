import Link from '@docusaurus/Link';
import Layout from '@theme/Layout';
import type { ReactNode } from 'react';
import styles from './index.module.css';

// The T of the wordmark and its caret, drawn at display size.
function Mark(): ReactNode {
  return (
    <svg className={styles.mark} viewBox="0 0 690 777" role="img" aria-label="T and caret">
      <path
        fill="currentColor"
        d="M129.675,722.277l305.051,0l0,-21.349l-118.491,-14.912l-68.069,0l-118.491,14.912l0,21.349Zm83.169,0l138.672,0c-4.238,-83.652 -4.787,-219.723 -4.787,-330.369l0,-46.063c0,-108.018 0.55,-239.811 4.787,-323.463l-138.123,-0c4.238,83.652 4.787,215.445 4.787,323.463l0,46.063c0,110.646 -1.1,246.718 -5.337,330.369Zm-212.844,-496.332l21.349,0l77.268,-190.851l-39.157,14.912l445.991,0l-39.157,-14.912l77.268,190.851l21.349,0l-12.713,-203.564l-539.485,0l-12.713,203.564Z"
      />
      <rect className={styles.caret} x="620" y="0" width="60" height="777" />
    </svg>
  );
}

function Heading({ index, eyebrow, title, children }: { index: string; eyebrow: string; title: ReactNode; children: ReactNode }): ReactNode {
  return (
    <div className={styles.heading}>
      <div>
        <div className={styles.eyebrow}>
          {index} / {eyebrow}
        </div>
        <h2>{title}</h2>
      </div>
      <p>{children}</p>
    </div>
  );
}

const engine = [
  ['Layout', 'Line for line and page for page, the way a word processor lays a document out.'],
  ['Caret and selection', 'Hit testing, keyboard and pointer input, and the accessibility DOM.'],
  ['Editing', 'Commands, undo and redo, tracked changes and comments on the document model.'],
  ['Drawing', 'Its own 2D engine, twip-graphics: a CPU reference, the GPU through wgpu, and PDF.'],
];

const host = [
  ['Toolbar', 'Formatting state for every control, recomputed as the selection moves.'],
  ['Menus and dialogs', 'Context menus, find and replace, spelling, and font and paragraph dialogs as models.'],
  ['Panes and rulers', 'Review balloons, rulers and navigation, ready for your own components.'],
  ['Your product', 'The workflow around the page: forms, approvals, templates, whatever it needs to be.'],
];

const sdks = [
  { name: 'Web', pkg: 'npm i @twip/web', note: 'Browsers and Node, through WebAssembly.', to: '/guides/web' },
  { name: '.NET', pkg: 'dotnet add package Twip.Core', note: 'Windows and Linux, Native AOT compatible.', to: '/guides/dotnet' },
  { name: 'Rust', pkg: 'cargo add twip', note: 'The engine itself, and the contract every SDK follows.', to: '/guides/rust' },
];

const pipeline = ['DOCX', 'Document model', 'Layout', 'Page fragments', 'Drawing', 'twip-graphics'];

export default function Home(): ReactNode {
  return (
    <Layout description="A DOCX layout, editing and rendering engine for document experiences that don't fit inside a word processor.">
      <main className={styles.home}>
        <section className={`${styles.hero} ${styles.shell}`}>
          <div>
            <div className={styles.eyebrow}>01 / Layout. Edit. Render.</div>
            <h1>
              You build the interface.
              <br />
              Twip handles the <em>page.</em>
            </h1>
            <p>
              A DOCX layout, editing and rendering engine for document experiences that don’t fit inside a word
              processor. Twip owns the page; your application owns everything around it.
            </p>
            <div className={styles.links}>
              <Link className={styles.button} to="/installation">
                Get started <span>→</span>
              </Link>
              <Link className={styles.tinyLink} to="/introduction">
                Read the introduction <b>↗</b>
              </Link>
            </div>
          </div>
          <aside className={styles.aside}>
            <Mark />
            <div className={styles.caption}>
              The caret is the line between the two: everything inside the page is Twip’s, everything around it is
              yours.
            </div>
            <div className={styles.micro}>01 / PAGE &nbsp;→&nbsp; 02 / INTERFACE</div>
          </aside>
        </section>

        <section className={`${styles.section} ${styles.shell}`}>
          <Heading index="02" eyebrow="Division of labor" title="Two sides of the caret.">
            Twip is the engine under an editor, not the whole application. It computes the models behind the chrome, and
            you draw the chrome.
          </Heading>
          <div className={styles.split}>
            <article className={styles.card}>
              <div className={styles.micro}>TWIP / THE PAGE</div>
              <dl>
                {engine.map(([k, v]) => (
                  <div key={k}>
                    <dt>{k}</dt>
                    <dd>{v}</dd>
                  </div>
                ))}
              </dl>
            </article>
            <article className={`${styles.card} ${styles.dark}`}>
              <div className={styles.micro}>YOUR APPLICATION / THE INTERFACE</div>
              <dl>
                {host.map(([k, v]) => (
                  <div key={k}>
                    <dt>{k}</dt>
                    <dd>{v}</dd>
                  </div>
                ))}
              </dl>
            </article>
          </div>
        </section>

        <section className={`${styles.section} ${styles.shell}`}>
          <Heading index="03" eyebrow="Integrations" title="One engine, three integrations.">
            Each SDK is written by hand for its language. A shared suite of contract scripts runs against all of them, so
            they behave the same.
          </Heading>
          <div className={styles.sdks}>
            {sdks.map((s) => (
              <Link key={s.name} className={styles.sdk} to={s.to}>
                <span className={styles.sdkTop}>
                  <span className={styles.micro}>{s.name.toUpperCase()} / GUIDE</span>
                  <span className={styles.arrow} aria-hidden="true">
                    →
                  </span>
                </span>
                <span className={styles.sdkName}>{s.name}</span>
                <code>{s.pkg}</code>
                <span className={styles.note}>{s.note}</span>
              </Link>
            ))}
          </div>
        </section>

        <section className={`${styles.section} ${styles.shell}`}>
          <Heading index="04" eyebrow="Pipeline" title="From DOCX to pixels.">
            Fragments are immutable and positionless. After an edit, layout restarts from the page before the change and
            reuses every page after it once they match.
          </Heading>
          <ol className={styles.pipeline}>
            {pipeline.map((step, i) => (
              <li key={step}>
                <span className={styles.micro}>{String(i + 1).padStart(2, '0')}</span>
                <span>{step}</span>
              </li>
            ))}
          </ol>
          <Link className={styles.tinyLink} to="/architecture">
            Architecture and performance <b>↗</b>
          </Link>
        </section>

        <section className={`${styles.section} ${styles.shell}`}>
          <Heading index="05" eyebrow="Status" title="Early, and honest about it.">
            Twip is in its initial development phase (0.x). The API will change between minor versions until 1.0.
          </Heading>
          <div className={styles.links}>
            <Link className={styles.tinyLink} to="/roadmap">
              Roadmap <b>↗</b>
            </Link>
            <Link className={styles.tinyLink} to="/guides/compatibility">
              Compatibility <b>↗</b>
            </Link>
            <Link className={styles.tinyLink} href="https://github.com/jaxcksn/twip">
              Source on GitHub <b>↗</b>
            </Link>
          </div>
        </section>
      </main>
    </Layout>
  );
}
