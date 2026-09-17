import Image from "next/image";

import { VisitorCounter } from "./visitor-counter";

export function HomeHero() {
  return (
    <main className="home-shell">
      <section className="home-hero" aria-labelledby="mads-title">
        <div>
          <p className="eyebrow">MADS for Rust</p>
          <h1 id="mads-title">A clear composition model for production Rust services.</h1>
          <p className="home-summary">
            Learn the module graph, conventional runtime, optional infrastructure, and Cargo-native tools that make MADS applications explicit.
          </p>
          <VisitorCounter />
        </div>
        <Image className="home-logo" src="/mads.png" alt="MADS logo" width={320} height={320} priority />
      </section>
      <section className="privacy-note" aria-label="Visitor counter privacy">
        <h2>A private, self-hosted counter</h2>
        <p>
          The counter uses a signed anonymous cookie plus challenge and daily rate-limit hashes. It does not store raw IP addresses or user agents. Sophisticated browser bots can still imitate browser evidence and solve proof-of-work, so this is a best-effort count rather than an identity system.
        </p>
      </section>
    </main>
  );
}
