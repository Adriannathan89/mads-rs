export interface RuntimeConfig {
  readonly databaseUrl: string;
  readonly visitorCookieSecret: string;
  readonly visitorChallengeSecret: string;
  readonly visitorRateLimitSecret: string;
  readonly allowedHosts: readonly string[];
  readonly proofDifficulty: number;
}

const developmentDefaults = {
  databaseUrl: "postgresql://mads_docs:change-me@localhost:5432/mads_docs",
  visitorCookieSecret: "development-cookie-secret-not-for-production",
  visitorChallengeSecret: "development-challenge-secret-not-for-production",
  visitorRateLimitSecret: "development-rate-limit-secret-not-for-production",
  allowedHosts: ["localhost"],
  proofDifficulty: 12,
} as const;

function required(
  environment: NodeJS.ProcessEnv,
  key: keyof NodeJS.ProcessEnv,
  fallback: string,
  production: boolean,
): string {
  const value = environment[key]?.trim();

  if (value) {
    return value;
  }

  if (production) {
    throw new Error(`${key} is required in production`);
  }

  return fallback;
}

function parseAllowedHosts(value: string | undefined, production: boolean): string[] {
  const hosts = (value ?? (production ? "" : developmentDefaults.allowedHosts.join(",")))
    .split(",")
    .map((host) => host.trim().toLowerCase())
    .filter(Boolean);

  if (hosts.length === 0) {
    throw new Error("VISITOR_ALLOWED_HOSTS is required in production");
  }

  return [...new Set(hosts)];
}

function parseProofDifficulty(value: string | undefined, production: boolean): number {
  const rawValue = value ?? (production ? undefined : String(developmentDefaults.proofDifficulty));

  if (!rawValue) {
    throw new Error("VISITOR_PROOF_DIFFICULTY is required in production");
  }

  const difficulty = Number(rawValue);

  if (!Number.isInteger(difficulty) || difficulty < 12 || difficulty > 24) {
    throw new Error("VISITOR_PROOF_DIFFICULTY must be an integer between 12 and 24");
  }

  return difficulty;
}

export function getRuntimeConfig(environment: NodeJS.ProcessEnv): RuntimeConfig {
  const production = environment.NODE_ENV === "production";
  const config: RuntimeConfig = {
    databaseUrl: required(environment, "DATABASE_URL", developmentDefaults.databaseUrl, production),
    visitorCookieSecret: required(
      environment,
      "VISITOR_COOKIE_SECRET",
      developmentDefaults.visitorCookieSecret,
      production,
    ),
    visitorChallengeSecret: required(
      environment,
      "VISITOR_CHALLENGE_SECRET",
      developmentDefaults.visitorChallengeSecret,
      production,
    ),
    visitorRateLimitSecret: required(
      environment,
      "VISITOR_RATE_LIMIT_SECRET",
      developmentDefaults.visitorRateLimitSecret,
      production,
    ),
    allowedHosts: parseAllowedHosts(environment.VISITOR_ALLOWED_HOSTS, production),
    proofDifficulty: parseProofDifficulty(environment.VISITOR_PROOF_DIFFICULTY, production),
  };

  const secrets = new Set([
    config.visitorCookieSecret,
    config.visitorChallengeSecret,
    config.visitorRateLimitSecret,
  ]);

  if (secrets.size !== 3) {
    throw new Error("Visitor secrets must be distinct");
  }

  return config;
}
