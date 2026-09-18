import { describe, expect, it } from "vitest";

import type { VisitorRepository } from "@/application/ports/visitor-repository";
import type { BrowserEvidence } from "@/domain/visitor";
import { RegisterQualifiedVisitor } from "@/application/use-cases/register-qualified-visitor";
import { BrowserVisitPolicy } from "@/infrastructure/security/browser-visit-policy";
import { HmacChallengeCodec } from "@/infrastructure/security/hmac-challenge-codec";
import { solveProof } from "@/infrastructure/security/proof-of-work";

const fixedNow = new Date("2026-09-17T00:00:00Z");

const browserEvidence = (overrides: Partial<BrowserEvidence> = {}): BrowserEvidence => ({
  host: "docs.madsrs.com",
  origin: "https://docs.madsrs.com",
  fetchSite: "same-origin",
  fetchMode: "cors",
  userAgent: "Mozilla/5.0 Chrome/140.0 Safari/537.36",
  webdriver: false,
  clientAddress: "203.0.113.25",
  ...overrides,
});

class MemoryVisitorRepository implements VisitorRepository {
  public consumed = true;
  private visitors = new Set<string>();

  public async consumeChallenge(): Promise<boolean> {
    return this.consumed;
  }

  public async allowRateAttempt(): Promise<boolean> {
    return true;
  }

  public async registerUniqueVisitor(visitorHash: string): Promise<number> {
    this.visitors.add(visitorHash);
    return this.visitors.size;
  }

  public async getQualifiedVisitorCount(): Promise<number> {
    return this.visitors.size;
  }

  public async pruneExpired(): Promise<void> {}

  public async isReady(): Promise<boolean> {
    return true;
  }
}

describe("RegisterQualifiedVisitor", () => {
  const codec = new HmacChallengeCodec({
    challengeSecret: "challenge-secret-for-tests",
    cookieSecret: "cookie-secret-for-tests",
    difficulty: 12,
  });
  const repository = new MemoryVisitorRepository();
  const useCase = new RegisterQualifiedVisitor({
    policy: new BrowserVisitPolicy(["docs.madsrs.com"]),
    codec,
    repository,
    rateLimitSecret: "rate-limit-secret-for-tests",
    clock: () => fixedNow,
  });

  const validRegistrationInput = async () => {
    const challenge = codec.issue(fixedNow);
    return {
      evidence: browserEvidence(),
      challenge: challenge.token,
      nonce: await solveProof(challenge),
      existingVisitorCookie: undefined,
    };
  };

  it("registers a valid first visit and returns a signed cookie plus total", async () => {
    const result = await useCase.execute(await validRegistrationInput());

    expect(result).toEqual({ kind: "counted", total: 1, visitorCookie: expect.any(String) });
  });

  it("does not mint a cookie or increase the total for a replayed challenge", async () => {
    repository.consumed = false;

    await expect(useCase.execute(await validRegistrationInput())).resolves.toEqual({ kind: "rejected" });
  });

  it("recognizes an existing valid cookie without incrementing the count", async () => {
    repository.consumed = true;
    const cookie = codec.issueVisitorCookie();

    await expect(
      useCase.execute({ ...(await validRegistrationInput()), existingVisitorCookie: cookie }),
    ).resolves.toEqual({ kind: "known", total: 1 });
  });
});
