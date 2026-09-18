import { sql } from "drizzle-orm";
import type { Pool, PoolClient } from "../../../node_modules/@types/pg";

import type { VisitorRepository } from "@/application/ports/visitor-repository";

import { createDatabase } from "./db";

export class DrizzleVisitorRepository implements VisitorRepository {
  private readonly database;

  public constructor(private readonly pool: Pool) {
    this.database = createDatabase(pool);
  }

  public async consumeChallenge(challengeHash: string, expiresAt: Date): Promise<boolean> {
    const result = await this.pool.query(
      `insert into consumed_visit_challenges (challenge_hash, expires_at)
       values ($1, $2)
       on conflict (challenge_hash) do nothing
       returning challenge_hash`,
      [challengeHash, expiresAt],
    );
    return result.rowCount === 1;
  }

  public async allowRateAttempt(rateKey: string, now: Date): Promise<boolean> {
    const windowEndsAt = new Date(now.getTime() + 5 * 60 * 1000);
    const result = await this.pool.query(
      `insert into visitor_rate_limits (rate_key, window_ends_at, attempts)
       values ($1, $3, 1)
       on conflict (rate_key) do update
       set attempts = case
             when visitor_rate_limits.window_ends_at <= $2 then 1
             else visitor_rate_limits.attempts + 1
           end,
           window_ends_at = case
             when visitor_rate_limits.window_ends_at <= $2 then $3
             else visitor_rate_limits.window_ends_at
           end
       where visitor_rate_limits.window_ends_at <= $2 or visitor_rate_limits.attempts < 10
       returning attempts`,
      [rateKey, now, windowEndsAt],
    );
    return result.rowCount === 1;
  }

  public async registerUniqueVisitor(visitorHash: string, now: Date): Promise<number> {
    const client = await this.pool.connect();

    try {
      await client.query("begin");
      await this.insertOrTouchVisitor(client, visitorHash, now);
      const count = await client.query<{ total: string }>(
        "select count(*)::text as total from qualified_visitors",
      );
      await client.query("commit");
      return Number(count.rows[0].total);
    } catch (error) {
      await client.query("rollback");
      throw error;
    } finally {
      client.release();
    }
  }

  public async getQualifiedVisitorCount(): Promise<number> {
    const result = await this.pool.query<{ total: string }>(
      "select count(*)::text as total from qualified_visitors",
    );
    return Number(result.rows[0].total);
  }

  public async pruneExpired(now: Date): Promise<void> {
    await Promise.all([
      this.pool.query("delete from consumed_visit_challenges where expires_at <= $1", [now]),
      this.pool.query("delete from visitor_rate_limits where window_ends_at <= $1", [now]),
    ]);
  }

  public async isReady(): Promise<boolean> {
    try {
      await this.database.execute(sql`select 1`);
      return true;
    } catch {
      return false;
    }
  }

  private async insertOrTouchVisitor(client: PoolClient, visitorHash: string, now: Date): Promise<void> {
    const inserted = await client.query(
      `insert into qualified_visitors (visitor_hash, first_seen_at, last_seen_at)
       values ($1, $2, $2)
       on conflict (visitor_hash) do nothing
       returning visitor_hash`,
      [visitorHash, now],
    );
    if (inserted.rowCount === 0) {
      await client.query(
        "update qualified_visitors set last_seen_at = greatest(last_seen_at, $2) where visitor_hash = $1",
        [visitorHash, now],
      );
    }
  }
}
