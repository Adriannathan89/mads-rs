CREATE TABLE "consumed_visit_challenges" (
	"challenge_hash" text PRIMARY KEY NOT NULL,
	"expires_at" timestamp with time zone NOT NULL
);
--> statement-breakpoint
CREATE TABLE "qualified_visitors" (
	"visitor_hash" text PRIMARY KEY NOT NULL,
	"first_seen_at" timestamp with time zone NOT NULL,
	"last_seen_at" timestamp with time zone NOT NULL
);
--> statement-breakpoint
CREATE TABLE "visitor_rate_limits" (
	"rate_key" text PRIMARY KEY NOT NULL,
	"window_ends_at" timestamp with time zone NOT NULL,
	"attempts" integer NOT NULL
);
