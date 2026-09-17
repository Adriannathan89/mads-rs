import Link from "next/link";

export default function HomePage() {
  return (
    <main className="landing-shell">
      <section className="landing-card" aria-labelledby="mads-title">
        <p className="eyebrow">MADS for Rust</p>
        <h1 id="mads-title">Build modular Rust services with a framework that stays out of your way.</h1>
        <p>
          Start with the guide, understand the runtime, and use the API reference when you need
          the details.
        </p>
        <Link className="primary-link" href="/docs/getting-started/introduction">
          Read the documentation
        </Link>
      </section>
    </main>
  );
}
