import Link from '@docusaurus/Link';
import useBaseUrl from '@docusaurus/useBaseUrl';
import Heading from '@theme/Heading';
import ThemedImage from '@theme/ThemedImage';
import styles from './Landing.module.css';

type Row = { eyebrow: string; title: string; text: string; to: string; cta: string; img: string; height: number; alt: string };

const ROWS: Row[] = [
  {
    eyebrow: 'Flows',
    title: 'An n8n-style canvas for the house.',
    text: 'Wire a trigger to checks, actions and waits. Drag nodes, set their parameters, save. Every run is recorded, so you can replay what it did, step by step, and see why a light did or did not turn on.',
    to: '/docs/users/flows',
    cta: 'How flows work',
    img: '/img/screens/flow-canvas.webp',
    height: 1800,
    alt: 'The flow canvas: a trigger connected to a check, an action and a delay.',
  },
  {
    eyebrow: 'Energy',
    title: 'See where the power goes, live.',
    text: 'Solar on top, the house in the centre, the grid on the left and consumers on an arc. Lines thicken and run faster with power, and follow import or export.',
    to: '/docs/users/web-ui#energy',
    cta: 'About the Energy tab',
    img: '/img/screens/energy.webp',
    height: 2176,
    alt: 'The Energy tab: live power flow between solar, grid, house and consumers, above the day chart of the grid.',
  },
];

function ShowRow({ row, flip }: { row: Row; flip: boolean }) {
  const dark = useBaseUrl(row.img);
  const light = useBaseUrl(row.img.replace(/\.webp$/, '-ivoire.webp'));
  return (
    <div className={flip ? `${styles.row} ${styles.rowFlip}` : styles.row}>
      <div className={styles.rowText}>
        <p className={styles.eyebrow}>{row.eyebrow}</p>
        <Heading as="h2" className={styles.rowTitle}>
          {row.title}
        </Heading>
        <p className={styles.rowBody}>{row.text}</p>
        <Link to={row.to} className={styles.cardLink}>
          {row.cta} <span aria-hidden="true">→</span>
        </Link>
      </div>
      <div className={styles.rowMedia}>
        <ThemedImage
          sources={{ dark, light }}
          loading="lazy"
          width={2880}
          height={row.height}
          alt={row.alt}
          className={styles.rowImg}
        />
      </div>
    </div>
  );
}

export default function Showcase() {
  return (
    <section className={styles.section}>
      <div className="container">
        {ROWS.map((r, i) => (
          <ShowRow key={r.title} row={r} flip={i % 2 === 1} />
        ))}
      </div>
    </section>
  );
}
