import Link from "next/link";

export default function NotFound() {
  return (
    <main className="landing-shell">
      <section className="landing-card">
        <p className="eyebrow">404</p>
        <h1>That documentation page does not exist.</h1>
        <Link className="primary-link" href="/docs/introduction/what-is-mads">
          Go to the introduction
        </Link>
      </section>
    </main>
  );
}
