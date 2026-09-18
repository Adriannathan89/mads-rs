import { randomUUID } from "node:crypto";

import { afterAll, beforeEach, describe, expect, it } from "vitest";

import { createPostgresPool } from "@/infrastructure/persistence/db";
import { DrizzleVisitorRepository } from "@/infrastructure/persistence/drizzle-visitor-repository";

const databaseUrl = process.env.DATABASE_URL;
const describeDatabase = databaseUrl ? describe : describe.skip;

describeDatabase("DrizzleVisitorRepository", () => {
  const pgPool = createPostgresPool(databaseUrl ?? "");
  const repository = new DrizzleVisitorRepository(pgPool);
  const now = new Date("2026-09-17T00:00:00Z");
  const future = new Date("2026-09-17T00:05:00Z");
  const past = new Date("2026-09-16T23:59:00Z");

  const selectRawVisitorColumns = async () => {
    const result = await pgPool.query<{ column_name: string }>(
      "select column_name from information_schema.columns where table_name = 'qualified_visitors' order by ordinal_position",
    );
    return result.rows.map((row) => row.column_name);
  };

  beforeEach(async () => {
    await pgPool.query("delete from consumed_visit_challenges");
    await pgPool.query("delete from visitor_rate_limits");
    await pgPool.query("delete from qualified_visitors");
  });

  afterAll(async () => {
    await pgPool.end();
  });

  it("consumes a challenge once and counts a visitor once under concurrent inserts", async () => {
    const challengeHash = `challenge-${randomUUID()}`;
    await expect(repository.consumeChallenge(challengeHash, future)).resolves.toBe(true);
    await expect(repository.consumeChallenge(challengeHash, future)).resolves.toBe(false);

    await Promise.all(Array.from({ length: 8 }, () => repository.registerUniqueVisitor("visitor-a", now)));

    await expect(repository.getQualifiedVisitorCount()).resolves.toBe(1);
  });

  it("stores hashes only and prunes expired challenge and rate-limit rows", async () => {
    await repository.allowRateAttempt("daily-rate-key", now);
    await repository.consumeChallenge("expired-challenge", past);
    await repository.pruneExpired(now);

    await expect(selectRawVisitorColumns()).resolves.toEqual([
      "visitor_hash",
      "first_seen_at",
      "last_seen_at",
    ]);
  });
});
