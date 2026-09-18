"use client";

import { useEffect } from "react";

import type { VisitorCounterState } from "./visitor-counter";

interface ChallengeResponse {
  readonly challenge: string;
  readonly difficulty: number;
}

function solveInWorker(challenge: ChallengeResponse): Promise<number> {
  return new Promise((resolve, reject) => {
    if (typeof Worker === "undefined") {
      reject(new Error("Web Workers are unavailable"));
      return;
    }
    const worker = new Worker("/visitor-proof.worker.js");
    const timeout = window.setTimeout(() => {
      worker.terminate();
      reject(new Error("Visitor proof timed out"));
    }, 60_000);
    worker.onmessage = (event: MessageEvent<{ nonce?: number; error?: string }>) => {
      window.clearTimeout(timeout);
      worker.terminate();
      if (typeof event.data.nonce === "number") {
        resolve(event.data.nonce);
      } else {
        reject(new Error(event.data.error ?? "Visitor proof failed"));
      }
    };
    worker.onerror = () => {
      window.clearTimeout(timeout);
      worker.terminate();
      reject(new Error("Visitor proof worker failed"));
    };
    worker.postMessage(challenge);
  });
}

function publish(state: VisitorCounterState) {
  window.__madsVisitorCounterState = state;
  window.dispatchEvent(new CustomEvent<VisitorCounterState>("mads-visitor-counter", { detail: state }));
}

async function qualifyVisit(): Promise<void> {
  const challengeResponse = await fetch("/api/visitor-challenge", { cache: "no-store", credentials: "same-origin" });
  if (!challengeResponse.ok) {
    throw new Error("Visitor challenge was rejected");
  }
  const challenge = (await challengeResponse.json()) as ChallengeResponse;
  const nonce = await solveInWorker(challenge);
  const response = await fetch("/api/visitor", {
    method: "POST",
    cache: "no-store",
    credentials: "same-origin",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ challenge: challenge.challenge, nonce, webdriver: navigator.webdriver === true }),
  });
  if (!response.ok) {
    throw new Error("Visitor registration was rejected");
  }
  const result = (await response.json()) as { total: number };
  publish({ kind: "available", total: result.total });
}

export function VisitorTracker() {
  useEffect(() => {
    if (window.__madsVisitorCounterState) {
      return;
    }
    void qualifyVisit().catch(() => publish({ kind: "unavailable" }));
  }, []);

  return null;
}
