export interface VisitorRepository {
  consumeChallenge(challengeHash: string, expiresAt: Date): Promise<boolean>;
  allowRateAttempt(rateKey: string, now: Date): Promise<boolean>;
  registerUniqueVisitor(visitorHash: string, now: Date): Promise<number>;
  getQualifiedVisitorCount(): Promise<number>;
  pruneExpired(now: Date): Promise<void>;
  isReady(): Promise<boolean>;
}
