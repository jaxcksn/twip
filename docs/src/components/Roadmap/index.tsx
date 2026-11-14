import type { ReactNode } from 'react';
import useBrokenLinks from '@docusaurus/useBrokenLinks';
import styles from './styles.module.css';

// Chunk "1.4" is anchored as #c1-4.
const anchor = (id: string) => `c${id.replace('.', '-')}`;

type PhaseItem = { n: string; title: string; href: string; range: string; blurb: string; done?: boolean };

/** The four phases as a track, ending at 1.0.0. */
export function Phases({ items }: { items: PhaseItem[] }) {
  return (
    <ol className={styles.track}>
      {items.map((p) => (
        <li key={p.n} className={styles.stop}>
          <a href={p.href} className={styles.stopLink}>
            <span className={styles.numeral}>{p.n}</span>
            <span className={styles.stopTitle}>{p.title}</span>
          </a>
          <span className={styles.range}>
            {p.range}
            {p.done && <span className={styles.flag}>Done</span>}
          </span>
          <span className={styles.blurb}>{p.blurb}</span>
        </li>
      ))}
      <li className={`${styles.stop} ${styles.finish}`}>
        <span className={styles.numeral}>1.0</span>
        <span className={styles.stopTitle}>Stable</span>
        <span className={styles.blurb}>Semantic versioning from here on.</span>
      </li>
    </ol>
  );
}

type ChunkProps = {
  id: string;
  title: string;
  deps?: string[];
  gate?: boolean;
  done?: boolean;
  children: ReactNode;
};

/** One chunk: one feature pull request. */
export function Chunk({ id, title, deps = [], gate = false, done = false, children }: ChunkProps) {
  useBrokenLinks().collectAnchor(anchor(id));
  return (
    <section id={anchor(id)} className={`${styles.chunk} ${gate ? styles.gate : ''}`}>
      <header className={styles.head}>
        <a href={`#${anchor(id)}`} className={styles.id}>
          {id}
        </a>
        <span className={styles.title}>{title}</span>
        {gate && <span className={styles.flag}>Gate</span>}
        {done && <span className={styles.flag}>Done</span>}
      </header>
      <div className={styles.body}>{children}</div>
      {deps.length > 0 && (
        <footer className={styles.deps}>
          Depends on{' '}
          {deps.map((d, i) => (
            <span key={d}>
              {i > 0 && ', '}
              <a href={`#${anchor(d)}`}>{d}</a>
            </span>
          ))}
        </footer>
      )}
    </section>
  );
}
