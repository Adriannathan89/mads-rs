"use client";

import Link from "next/link";
import { useEffect, useState } from "react";

export type VisitorCounterState =
  | { readonly kind: "loading" }
  | { readonly kind: "available"; readonly total: number }
  | { readonly kind: "unavailable" };

declare global {
  interface Window {
    __madsVisitorCounterState?: VisitorCounterState;
  }
}

export function VisitorCounter({ state }: Readonly<{ state?: VisitorCounterState }>) {
  const [current, setCurrent] = useState<VisitorCounterState>(state ?? { kind: "loading" });

  useEffect(() => {
    if (state) {
      return;
    }
    if (window.__madsVisitorCounterState) {
      setCurrent(window.__madsVisitorCounterState);
    }
    const update = (event: Event) => setCurrent((event as CustomEvent<VisitorCounterState>).detail);
    window.addEventListener("mads-visitor-counter", update);
    return () => window.removeEventListener("mads-visitor-counter", update);
  }, [state]);

  return (
    <div className="visitor-counter" aria-live="polite">
      {current.kind === "available" ? <p>{current.total.toLocaleString()} developers have started with MADS.</p> : null}
      {current.kind === "unavailable" ? <p>Visitor count unavailable</p> : null}
      {current.kind === "loading" ? <p>Preparing a private visitor count…</p> : null}
      <Link className="primary-link" href="/docs/introduction/quick-start">Get started</Link>
    </div>
  );
}
