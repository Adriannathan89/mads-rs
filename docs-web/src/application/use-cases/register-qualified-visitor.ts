import { createHmac } from "node:crypto";

import type { ChallengeCodec } from "@/application/ports/challenge-codec";
import type { VisitorRepository } from "@/application/ports/visitor-repository";
import type { BrowserEvidence } from "@/domain/visitor";
import { BrowserVisitPolicy } from "@/infrastructure/security/browser-visit-policy";
import { verifyProof } from "@/infrastructure/security/proof-of-work";

export interface QualifiedVisitInput {
  readonly evidence: BrowserEvidence;
  readonly challenge: string;
  readonly nonce: number;
  readonly existingVisitorCookie: string | undefined;
}

export type QualifiedVisitResult =
  | { readonly kind: "counted"; readonly total: number; readonly visitorCookie: string }
  | { readonly kind: "known"; readonly total: number }
  | { readonly kind: "rejected" };

export interface RegisterQualifiedVisitorOptions {
  readonly policy: BrowserVisitPolicy;
  readonly codec: ChallengeCodec;
  readonly repository: VisitorRepository;
  readonly rateLimitSecret: string;
  readonly clock?: () => Date;
}

function dailyRateKey(clientAddress: string, now: Date, secret: string): string {
  const day = now.toISOString().slice(0, 10);
  return createHmac("sha256", secret).update(`${day}:${clientAddress}`).digest("base64url");
}

export class RegisterQualifiedVisitor {
  private readonly clock: () => Date;

  public constructor(private readonly options: RegisterQualifiedVisitorOptions) {
    this.clock = options.clock ?? (() => new Date());
  }

  public async execute(input: QualifiedVisitInput): Promise<QualifiedVisitResult> {
    if (this.options.policy.evaluate(input.evidence) !== "accepted" || !input.evidence.clientAddress) {
      return { kind: "rejected" };
    }

    const now = this.clock();
    const challenge = this.options.codec.verify(input.challenge, now);
    if (!challenge || !(await verifyProof(challenge, input.nonce, now))) {
      return { kind: "rejected" };
    }

    const rateKey = dailyRateKey(input.evidence.clientAddress, now, this.options.rateLimitSecret);
    if (!(await this.options.repository.allowRateAttempt(rateKey, now))) {
      return { kind: "rejected" };
    }

    if (!(await this.options.repository.consumeChallenge(this.options.codec.hashChallenge(input.challenge), challenge.expiresAt))) {
      return { kind: "rejected" };
    }

    const existing = input.existingVisitorCookie
      ? this.options.codec.verifyVisitorCookie(input.existingVisitorCookie)
      : null;
    if (existing) {
      return { kind: "known", total: await this.options.repository.getQualifiedVisitorCount() };
    }

    const visitorCookie = this.options.codec.issueVisitorCookie();
    const verifiedCookie = this.options.codec.verifyVisitorCookie(visitorCookie);
    if (!verifiedCookie) {
      return { kind: "rejected" };
    }

    const total = await this.options.repository.registerUniqueVisitor(
      this.options.codec.hashVisitorId(verifiedCookie.id),
      now,
    );
    await this.options.repository.pruneExpired(now);
    return { kind: "counted", total, visitorCookie };
  }
}
