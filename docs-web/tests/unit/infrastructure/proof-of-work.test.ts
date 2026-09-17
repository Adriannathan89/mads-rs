import { describe, expect, it } from "vitest";

import type { VisitChallenge } from "@/domain/visitor";
import { solveProof, verifyProof } from "@/infrastructure/security/proof-of-work";

describe("proof of work", () => {
  const challenge: VisitChallenge = {
    id: "challenge-id",
    token: "signed-challenge-token",
    issuedAt: new Date("2026-09-17T00:00:00Z"),
    expiresAt: new Date("2026-09-17T00:05:00Z"),
    difficulty: 12,
  };

  it("accepts a valid proof and rejects an expired or insufficient proof", async () => {
    const nonce = await solveProof(challenge);

    await expect(verifyProof(challenge, nonce, new Date("2026-09-17T00:01:00Z"))).resolves.toBe(true);
    await expect(verifyProof({ ...challenge, expiresAt: new Date(0) }, nonce)).resolves.toBe(false);
    await expect(verifyProof(challenge, -1)).resolves.toBe(false);
  });
});
