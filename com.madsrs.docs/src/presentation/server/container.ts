import "server-only";

import { GetVisitorCount } from "@/application/use-cases/get-visitor-count";
import { IssueVisitorChallenge } from "@/application/use-cases/issue-visitor-challenge";
import { RegisterQualifiedVisitor } from "@/application/use-cases/register-qualified-visitor";
import { DrizzleVisitorRepository } from "@/infrastructure/persistence/drizzle-visitor-repository";
import { createPostgresPool } from "@/infrastructure/persistence/db";
import { BrowserVisitPolicy } from "@/infrastructure/security/browser-visit-policy";
import { HmacChallengeCodec } from "@/infrastructure/security/hmac-challenge-codec";

import { getRuntimeConfig } from "./runtime-config";

export interface VisitorContainer {
  readonly issueVisitorChallenge: IssueVisitorChallenge;
  readonly registerQualifiedVisitor: RegisterQualifiedVisitor;
  readonly getVisitorCount: GetVisitorCount;
  readonly visitorRepository: DrizzleVisitorRepository;
}

let container: VisitorContainer | undefined;

export function getVisitorContainer(): VisitorContainer {
  if (container) {
    return container;
  }

  const config = getRuntimeConfig(process.env);
  const visitorRepository = new DrizzleVisitorRepository(createPostgresPool(config.databaseUrl));
  const codec = new HmacChallengeCodec({
    challengeSecret: config.visitorChallengeSecret,
    cookieSecret: config.visitorCookieSecret,
    difficulty: config.proofDifficulty,
  });
  const policy = new BrowserVisitPolicy(config.allowedHosts);
  container = {
    issueVisitorChallenge: new IssueVisitorChallenge(policy, codec),
    registerQualifiedVisitor: new RegisterQualifiedVisitor({
      policy,
      codec,
      repository: visitorRepository,
      rateLimitSecret: config.visitorRateLimitSecret,
    }),
    getVisitorCount: new GetVisitorCount(visitorRepository),
    visitorRepository,
  };
  return container;
}
