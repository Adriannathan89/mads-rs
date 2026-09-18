import type { VisitChallenge, VisitorCookie } from "@/domain/visitor";

export interface ChallengeCodec {
  issue(now: Date): VisitChallenge;
  verify(token: string, now: Date): VisitChallenge | null;
  hashChallenge(token: string): string;
  issueVisitorCookie(): string;
  verifyVisitorCookie(value: string): VisitorCookie | null;
  hashVisitorId(id: string): string;
}
