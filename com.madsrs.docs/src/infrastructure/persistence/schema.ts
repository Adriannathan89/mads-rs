import { integer, pgTable, text, timestamp } from "drizzle-orm/pg-core";

export const qualifiedVisitors = pgTable("qualified_visitors", {
  visitorHash: text("visitor_hash").primaryKey(),
  firstSeenAt: timestamp("first_seen_at", { withTimezone: true }).notNull(),
  lastSeenAt: timestamp("last_seen_at", { withTimezone: true }).notNull(),
});

export const visitorRateLimits = pgTable("visitor_rate_limits", {
  rateKey: text("rate_key").primaryKey(),
  windowEndsAt: timestamp("window_ends_at", { withTimezone: true }).notNull(),
  attempts: integer("attempts").notNull(),
});

export const consumedVisitChallenges = pgTable("consumed_visit_challenges", {
  challengeHash: text("challenge_hash").primaryKey(),
  expiresAt: timestamp("expires_at", { withTimezone: true }).notNull(),
});
