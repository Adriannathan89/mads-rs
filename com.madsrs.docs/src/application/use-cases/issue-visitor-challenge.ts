import type { ChallengeCodec } from "@/application/ports/challenge-codec";
import type { BrowserEvidence, VisitChallenge } from "@/domain/visitor";
import { BrowserVisitPolicy } from "@/infrastructure/security/browser-visit-policy";

export class IssueVisitorChallenge {
  public constructor(
    private readonly policy: BrowserVisitPolicy,
    private readonly codec: ChallengeCodec,
    private readonly clock: () => Date = () => new Date(),
  ) {}

  public execute(evidence: BrowserEvidence): VisitChallenge | null {
    return this.policy.evaluate(evidence) === "accepted" ? this.codec.issue(this.clock()) : null;
  }
}
