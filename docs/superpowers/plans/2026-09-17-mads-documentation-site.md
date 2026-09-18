# MADS Documentation Site Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and containerize a learner-first Next.js documentation site for MADS.rs, with a private PostgreSQL-backed, bot-resistant unique-visitor counter.

**Architecture:** `com.madsrs.docs/` is an isolated Next.js App Router application. React pages and route handlers delegate to application use-cases, which depend only on domain types and ports; filesystem MDX, HMAC proof-of-work, and PostgreSQL/Drizzle live behind infrastructure adapters. The site renders without analytics, while a client tracker progressively adds a qualified anonymous visit after page usability.

**Tech Stack:** Next.js 16.3.5 App Router, React, TypeScript, local MDX via `next-mdx-remote`, CSS custom-property design system, Drizzle ORM with `pg` and PostgreSQL, Node Web Crypto, Vitest, React Testing Library, Playwright, Docker Compose.

**Spec:** `docs/superpowers/specs/2026-09-17-mads-documentation-site-design.md`

## Global Constraints

- Create the application in `com.madsrs.docs/`; do not alter MADS Rust crates or their public APIs.
- Support Node.js 20.9 or newer and pin `next` to `16.3.5`; commit `package-lock.json` for reproducible image builds.
- Treat MADS v0.8.0 repository documentation and examples as the source of truth; never document unimplemented features.
- Copy `docs/mads.png` to `com.madsrs.docs/public/mads.png`; preserve the user-provided source asset.
- Apply clean architecture: presentation code must not issue SQL, and domain/application code must not import Next.js, React, `fs`, `pg`, Drizzle, or environment variables.
- Never store raw IP addresses, user-agent strings, page history, or raw visitor-cookie values in PostgreSQL.
- The counter is best-effort abuse-resistant; describe its limits plainly and do not claim it proves every visitor is human.
- Keep documentation readable if JavaScript is disabled, PostgreSQL is unavailable, or visitor registration fails.
- Production secrets are supplied by the deployment server only: `DATABASE_URL`, `VISITOR_COOKIE_SECRET`, `VISITOR_CHALLENGE_SECRET`, `VISITOR_RATE_LIMIT_SECRET`, `VISITOR_ALLOWED_HOSTS`, and `VISITOR_PROOF_DIFFICULTY`.
- Use versioned Drizzle SQL migrations (`generate` then `migrate`), not schema push, in production.
- The application image is deployed by the existing main-branch webhook; do not add a competing deployment webhook or CI workflow.

---

## File Structure

```text
com.madsrs.docs/
├── content/docs/
│   ├── introduction/{what-is-mads,installation,quick-start,project-anatomy}.mdx
│   ├── fundamentals/{modules,providers,lifecycle}.mdx
│   ├── build-an-api/{routes-and-controllers,validation,rest-errors,cors}.mdx
│   ├── configuration/{configuration,typed-configuration-and-secrets}.mdx
│   ├── data-and-security/{postgres-and-diesel,migrations,passport-jwt-cookies}.mdx
│   ├── tooling/{cli,features-and-compatibility,diagnostics-and-deployment}.mdx
│   └── guides/build-a-user-api.mdx
├── drizzle/                         # Generated SQL migration history
├── public/mads.png                  # Copy of docs/mads.png
├── src/
│   ├── app/
│   │   ├── api/{visitor-challenge,visitor,health/live,health/ready}/route.ts
│   │   ├── docs/[...slug]/page.tsx
│   │   ├── globals.css
│   │   ├── layout.tsx
│   │   ├── not-found.tsx
│   │   ├── page.tsx
│   │   ├── robots.ts
│   │   └── sitemap.ts
│   ├── application/
│   │   ├── ports/{challenge-codec,documentation-repository,visitor-repository}.ts
│   │   └── use-cases/{get-visitor-count,issue-visitor-challenge,read-documentation,register-qualified-visitor}.ts
│   ├── domain/{documentation,visitor}.ts
│   ├── infrastructure/
│   │   ├── content/file-system-documentation-repository.ts
│   │   ├── persistence/{db,drizzle-visitor-repository,migrate,schema}.ts
│   │   └── security/{browser-visit-policy,hmac-challenge-codec,proof-of-work}.ts
│   ├── presentation/
│   │   ├── components/{docs-layout,documentation-page,header,home-hero,mdx-components,mobile-nav,on-page-toc,search-dialog,sidebar,theme-toggle,visitor-counter,visitor-tracker}.tsx
│   │   ├── workers/visitor-proof.worker.ts
│   │   └── server/{container,request-evidence}.ts
│   └── test/{factories,setup}.ts
├── tests/{unit,components,integration,e2e}/
├── .dockerignore
├── .env.example
├── Dockerfile
├── docker-compose.yml
├── drizzle.config.ts
├── eslint.config.mjs
├── next.config.ts
├── package.json
├── playwright.config.ts
├── tsconfig.json
└── vitest.config.ts
```

### Task 1: Scaffold the isolated Next.js application and executable quality gates

**Files:**
- Create: `com.madsrs.docs/package.json`, `com.madsrs.docs/package-lock.json`, `com.madsrs.docs/tsconfig.json`, `com.madsrs.docs/next.config.ts`, `com.madsrs.docs/eslint.config.mjs`, `com.madsrs.docs/vitest.config.ts`, `com.madsrs.docs/src/test/setup.ts`
- Create: `com.madsrs.docs/src/app/layout.tsx`, `com.madsrs.docs/src/app/page.tsx`, `com.madsrs.docs/src/app/globals.css`, `com.madsrs.docs/src/app/not-found.tsx`
- Create: `com.madsrs.docs/src/presentation/server/runtime-config.ts`, `com.madsrs.docs/tests/unit/presentation/runtime-config.test.ts`
- Create: `com.madsrs.docs/.gitignore`, `com.madsrs.docs/.env.example`, `com.madsrs.docs/public/mads.png`

**Interfaces:**
- Produces: `RuntimeConfig` and `getRuntimeConfig(environment: NodeJS.ProcessEnv): RuntimeConfig`, consumed by persistence, security, route-handler composition, and Docker services.
- Produces: the App Router root layout and home route, consumed by all later page, route, and browser tests.

- [ ] **Step 1: Create the package and test-runner configuration**

Create a Node 20.9+ TypeScript project with exact scripts:

```json
{
  "scripts": {
    "dev": "next dev",
    "build": "next build",
    "start": "next start",
    "lint": "eslint .",
    "typecheck": "tsc --noEmit",
    "test": "vitest run --project unit --project components",
    "test:integration": "vitest run --project integration",
    "test:e2e": "playwright test",
    "db:generate": "drizzle-kit generate",
    "db:migrate": "tsx src/infrastructure/persistence/migrate.ts",
    "verify": "npm run lint && npm run typecheck && npm test && npm run build"
  },
  "engines": { "node": ">=20.9.0" }
}
```

Install `next@16.3.5`, React, React DOM, `next-mdx-remote`, `gray-matter`, `remark-gfm`, `rehype-slug`, `rehype-autolink-headings`, `rehype-pretty-code`, `shiki`, `drizzle-orm`, and `pg`; install TypeScript, ESLint, Vitest, Testing Library, JSDOM, Playwright, Drizzle Kit, `tsx`, `execa`, and type packages as development dependencies. Configure Vitest projects named `unit`, `components`, and `integration`, with aliases that match `@/*`.

- [ ] **Step 2: Write the failing runtime-configuration test**

Create `tests/unit/presentation/runtime-config.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { getRuntimeConfig } from "@/presentation/server/runtime-config";

describe("getRuntimeConfig", () => {
  it("uses safe local defaults only in development", () => {
    expect(getRuntimeConfig({ NODE_ENV: "development" })).toMatchObject({
      allowedHosts: ["localhost"],
      proofDifficulty: 12,
    });
  });

  it("rejects a production environment without every visitor secret", () => {
    expect(() => getRuntimeConfig({ NODE_ENV: "production" })).toThrow(
      "DATABASE_URL is required in production",
    );
  });
});
```

- [ ] **Step 3: Run the unit test to verify it fails for the missing module**

Run: `npx vitest run --project unit tests/unit/presentation/runtime-config.test.ts`

Expected: FAIL because `@/presentation/server/runtime-config` does not exist.

- [ ] **Step 4: Implement the runtime boundary and application shell**

Implement `RuntimeConfig` with `databaseUrl`, three distinct secrets,
`allowedHosts`, and a proof difficulty constrained to integers 12 through 24.
Development defaults must use local-only non-production values; production must
throw a named missing-variable error. Add a minimal semantic root layout,
metadata for `docs.madsrs.com`, global CSS variables, and a static home page.
Copy `../docs/mads.png` into `public/mads.png`. Keep the home page independent
of database access.

Use this `.env.example` contract without real values:

```dotenv
DATABASE_URL=postgresql://mads_docs:change-me@localhost:5432/mads_docs
VISITOR_COOKIE_SECRET=replace-with-a-32-byte-random-secret
VISITOR_CHALLENGE_SECRET=replace-with-a-different-32-byte-random-secret
VISITOR_RATE_LIMIT_SECRET=replace-with-a-third-32-byte-random-secret
VISITOR_ALLOWED_HOSTS=localhost,docs.madsrs.com
VISITOR_PROOF_DIFFICULTY=18
```

- [ ] **Step 5: Run the focused test and baseline quality gates**

Run: `npx vitest run --project unit tests/unit/presentation/runtime-config.test.ts`

Expected: PASS with two tests.

Run: `npm run lint && npm run typecheck && npm run build`

Expected: all commands exit 0 and the root page builds.

- [ ] **Step 6: Commit the scaffold**

```bash
git add com.madsrs.docs
git commit -m "feat(docs): scaffold Next.js documentation app"
```

### Task 2: Add the filesystem MDX documentation repository and navigation model

**Files:**
- Create: `com.madsrs.docs/src/domain/documentation.ts`
- Create: `com.madsrs.docs/src/application/ports/documentation-repository.ts`, `com.madsrs.docs/src/application/use-cases/read-documentation.ts`
- Create: `com.madsrs.docs/src/infrastructure/content/file-system-documentation-repository.ts`
- Create: `com.madsrs.docs/tests/unit/infrastructure/file-system-documentation-repository.test.ts`
- Create: `com.madsrs.docs/content/docs/introduction/{what-is-mads,installation,quick-start,project-anatomy}.mdx`
- Create: `com.madsrs.docs/content/docs/fundamentals/{modules,providers,lifecycle}.mdx`
- Create: `com.madsrs.docs/content/docs/build-an-api/{routes-and-controllers,validation,rest-errors,cors}.mdx`
- Create: `com.madsrs.docs/content/docs/configuration/{configuration,typed-configuration-and-secrets}.mdx`
- Create: `com.madsrs.docs/content/docs/data-and-security/{postgres-and-diesel,migrations,passport-jwt-cookies}.mdx`
- Create: `com.madsrs.docs/content/docs/tooling/{cli,features-and-compatibility,diagnostics-and-deployment}.mdx`
- Create: `com.madsrs.docs/content/docs/guides/build-a-user-api.mdx`

**Interfaces:**
- Produces: `DocumentationPage`, `DocumentationSummary`, `Heading`, and `SearchEntry` domain records.
- Produces: `DocumentationRepository.list()`, `.get(slug)`, and `.search(query)` for all presentation routes.

- [ ] **Step 1: Write the failing documentation repository tests**

Create tests around a temporary content directory:

```ts
it("lists pages in section and order, then resolves a slug", async () => {
  const repository = new FileSystemDocumentationRepository(contentDirectory);

  await expect(repository.list()).resolves.toMatchObject([
    { slug: ["introduction", "what-is-mads"], title: "What is MADS?", order: 1 },
  ]);
  await expect(repository.get(["introduction", "what-is-mads"])).resolves.toMatchObject({
    description: expect.stringContaining("Rust"),
    headings: [{ id: "why-mads", text: "Why MADS?", depth: 2 }],
  });
});

it("does not resolve a path outside the content directory", async () => {
  const repository = new FileSystemDocumentationRepository(contentDirectory);
  await expect(repository.get(["..", "Cargo"])).resolves.toBeNull();
});
```

- [ ] **Step 2: Run the repository tests to verify they fail**

Run: `npx vitest run --project unit tests/unit/infrastructure/file-system-documentation-repository.test.ts`

Expected: FAIL because the domain records and repository do not exist.

- [ ] **Step 3: Implement the domain records, port, use-case, and adapter**

Define the exact repository port:

```ts
export interface DocumentationRepository {
  list(): Promise<DocumentationSummary[]>;
  get(slug: readonly string[]): Promise<DocumentationPage | null>;
  search(query: string): Promise<SearchEntry[]>;
}
```

Parse frontmatter with required `title`, `description`, `section`, and positive
integer `order`; normalize slugs from relative paths; extract `h2` and `h3`
headings with deterministic GitHub-style IDs; reject files with malformed
frontmatter at build time. Obtain pages only from a catalog generated by the
adapter, never by joining unchecked URL text to a filesystem path.

`ReadDocumentation.execute(slug)` returns `{ page, navigation, previous,
next }`, with previous and next determined by section then order. Search must
match a lower-cased query across title, description, headings, and plain MDX
text and return title, slug, description, and matched heading text.

- [ ] **Step 4: Author the initial MDX source with complete frontmatter**

Give every listed file explicit frontmatter and a reader-facing purpose. Use
the following authoritative source mapping while writing each page:

- `what-is-mads`, `project-anatomy`, `modules`, `providers`, and `lifecycle`:
  root `README.md` and `docs/ARCHITECTURE.md`.
- `installation`, `quick-start`, and `cli`: root `README.md` and `docs/CLI.md`.
- `routes-and-controllers`, `validation`, `rest-errors`, and `cors`: root
  `README.md`, `docs/ARCHITECTURE.md`, and `docs/examples/application.md`.
- both configuration pages: root `README.md`, `docs/ARCHITECTURE.md`, and
  `docs/examples/application_clean_architecture.md`.
- PostgreSQL, migrations, and the user API guide: `docs/examples/application.md`
  and `docs/examples/modular_user_jwt.md`.
- Passport/JWT/cookies: `docs/examples/passport_jwt.md`.
- feature compatibility and diagnostics/deployment: `crates/mads/README.md`,
  `docs/CLI.md`, and `docs/ARCHITECTURE.md`.

The quick start must show `mads new my-app`, `cd my-app`, and `mads dev`. The
project-anatomy page must explain the generated `src/app/{mod,routes,controller,service}.rs`
shape. The guide must finish with a runnable `Mads::run::<AppModule>().await`
composition root and must label database, JWT, and cookies as opt-in rather
than starter defaults.

- [ ] **Step 5: Run focused documentation tests**

Run: `npx vitest run --project unit tests/unit/infrastructure/file-system-documentation-repository.test.ts`

Expected: PASS; every listed MDX page resolves and a traversal slug is absent.

- [ ] **Step 6: Commit documentation domain and source**

```bash
git add com.madsrs.docs/src/domain com.madsrs.docs/src/application com.madsrs.docs/src/infrastructure/content com.madsrs.docs/content com.madsrs.docs/tests/unit/infrastructure
git commit -m "feat(docs): add MDX documentation repository"
```

### Task 3: Implement the domain-level self-hosted visitor qualification policy

**Files:**
- Create: `com.madsrs.docs/src/domain/visitor.ts`
- Create: `com.madsrs.docs/src/application/ports/{challenge-codec,visitor-repository}.ts`
- Create: `com.madsrs.docs/src/infrastructure/security/{browser-visit-policy,hmac-challenge-codec,proof-of-work}.ts`
- Create: `com.madsrs.docs/tests/unit/infrastructure/{browser-visit-policy,hmac-challenge-codec,proof-of-work}.test.ts`

**Interfaces:**
- Produces: `BrowserEvidence`, `VisitChallenge`, `VisitSolution`, and `VisitorCookie` domain values.
- Produces: `ChallengeCodec.issue(now)`, `.verify(token, now)`, `.issueVisitorCookie()`, and `.verifyVisitorCookie(value)`.
- Produces: `BrowserVisitPolicy.evaluate(evidence): "accepted" | "rejected"`.

- [ ] **Step 1: Write failing security-policy tests**

Define this local test factory before the test cases so each rejection changes
only the evidence under test:

```ts
const browserEvidence = (overrides: Partial<BrowserEvidence> = {}): BrowserEvidence => ({
  host: "docs.madsrs.com",
  origin: "https://docs.madsrs.com",
  fetchSite: "same-origin",
  fetchMode: "cors",
  userAgent: "Mozilla/5.0 Chrome/140.0 Safari/537.36",
  webdriver: false,
  clientAddress: "203.0.113.25",
  ...overrides,
});
```

```ts
it("rejects Googlebot, missing origin, and automation evidence", () => {
  expect(policy.evaluate(browserEvidence({ userAgent: "Googlebot/2.1" }))).toBe("rejected");
  expect(policy.evaluate(browserEvidence({ origin: null }))).toBe("rejected");
  expect(policy.evaluate(browserEvidence({ webdriver: true }))).toBe("rejected");
});

it("accepts a same-origin browser fetch", () => {
  expect(policy.evaluate(browserEvidence())).toBe("accepted");
});

it("accepts a valid proof and rejects an expired or insufficient proof", async () => {
  const challenge = codec.issue(new Date("2026-09-17T00:00:00Z"));
  await expect(verifyProof(challenge, solvedNonce)).resolves.toBe(true);
  await expect(verifyProof({ ...challenge, expiresAt: new Date(0) }, solvedNonce)).resolves.toBe(false);
  await expect(verifyProof(challenge, 0)).resolves.toBe(false);
});
```

- [ ] **Step 2: Run security tests to verify they fail**

Run: `npx vitest run --project unit tests/unit/infrastructure/browser-visit-policy.test.ts tests/unit/infrastructure/hmac-challenge-codec.test.ts tests/unit/infrastructure/proof-of-work.test.ts`

Expected: FAIL because the policy, challenge codec, and proof verifier are absent.

- [ ] **Step 3: Implement signed challenges, proof validation, and evidence filtering**

Create an HMAC-SHA-256 token that embeds a random challenge ID, issue time,
expiry time five minutes later, and difficulty. Sign and verify with
`VISITOR_CHALLENGE_SECRET`, use constant-time signature comparison, and derive
the consumed-challenge key from a one-way HMAC of the token.

Define proof validity as a SHA-256 digest of `challenge.token + ":" + nonce`
with at least `difficulty` leading zero bits. Use a bounded non-negative
integer nonce. The browser worker searches for a nonce; server verification
performs one digest only.

`BrowserVisitPolicy` requires a configured matching host and origin, fetch site
`same-origin`, fetch mode `cors`, non-empty browser-like user agent, and
`webdriver === false`. Reject case-insensitive tokens including `bot`,
`crawler`, `spider`, `slurp`, `curl`, `wget`, `facebookexternalhit`, `discordbot`,
`slackbot`, and `headless`. It must fail closed when expected evidence is
missing.

Issue a random visitor ID plus HMAC signature with `VISITOR_COOKIE_SECRET`.
The cookie codec exposes its opaque ID only for server-side hashing and never
logs it.

- [ ] **Step 4: Run focused security tests**

Run: `npx vitest run --project unit tests/unit/infrastructure/browser-visit-policy.test.ts tests/unit/infrastructure/hmac-challenge-codec.test.ts tests/unit/infrastructure/proof-of-work.test.ts`

Expected: PASS; valid evidence and proof pass, each rejection case fails closed.

- [ ] **Step 5: Commit visitor qualification primitives**

```bash
git add com.madsrs.docs/src/domain/visitor.ts com.madsrs.docs/src/application/ports com.madsrs.docs/src/infrastructure/security com.madsrs.docs/tests/unit/infrastructure
git commit -m "feat(docs): add private visitor qualification policy"
```

### Task 4: Add Drizzle persistence, SQL migrations, and PostgreSQL integration coverage

**Files:**
- Create: `com.madsrs.docs/drizzle.config.ts`, `com.madsrs.docs/src/infrastructure/persistence/{schema,db,drizzle-visitor-repository,migrate}.ts`
- Create: `com.madsrs.docs/drizzle/0000_qualified_visitors.sql` and generated Drizzle metadata
- Create: `com.madsrs.docs/tests/integration/drizzle-visitor-repository.test.ts`

**Interfaces:**
- Produces: `VisitorRepository.consumeChallenge(challengeHash, expiresAt)`, `.allowRateAttempt(rateKey, now)`, `.registerUniqueVisitor(visitorHash, now)`, `.getQualifiedVisitorCount()`, and `.pruneExpired(now)`.
- Consumes: a PostgreSQL `DATABASE_URL`, `VisitChallenge` hash, visitor hash, and daily rate key from Task 3.

- [ ] **Step 1: Write the failing PostgreSQL repository integration test**

Create the test pool from `process.env.DATABASE_URL` and define this inspection
helper in the test file; it intentionally reads PostgreSQL's public catalog,
not application-private fields:

```ts
const selectRawVisitorColumns = async () => {
  const result = await pgPool.query<{ column_name: string }>(
    "select column_name from information_schema.columns where table_name = 'qualified_visitors' order by ordinal_position",
  );
  return result.rows.map((row) => row.column_name);
};
```

```ts
it("consumes a challenge once and counts a visitor once under concurrent inserts", async () => {
  const challengeHash = "challenge-a";
  await expect(repository.consumeChallenge(challengeHash, future)).resolves.toBe(true);
  await expect(repository.consumeChallenge(challengeHash, future)).resolves.toBe(false);

  await Promise.all(Array.from({ length: 8 }, () => repository.registerUniqueVisitor("visitor-a", now)));

  await expect(repository.getQualifiedVisitorCount()).resolves.toBe(1);
});

it("stores hashes only and prunes expired challenge and rate-limit rows", async () => {
  await repository.allowRateAttempt("daily-rate-key", now);
  await repository.consumeChallenge("expired-challenge", past);
  await repository.pruneExpired(now);
  await expect(selectRawVisitorColumns()).resolves.toEqual(["visitor_hash", "first_seen_at", "last_seen_at"]);
});
```

- [ ] **Step 2: Run the integration test to verify it fails before the adapter exists**

Run: `DATABASE_URL=postgresql://mads_docs:change-me@localhost:5432/mads_docs npm run test:integration -- tests/integration/drizzle-visitor-repository.test.ts`

Expected: FAIL because the schema, migration, and repository are absent.

- [ ] **Step 3: Define schema and generate the tracked migration**

Define exactly three tables:

```ts
qualifiedVisitors(visitorHash unique, firstSeenAt, lastSeenAt)
visitorRateLimits(rateKey unique, windowEndsAt, attempts)
consumedVisitChallenges(challengeHash unique, expiresAt)
```

Use PostgreSQL timestamps with time zones. Generate `0000_qualified_visitors.sql`
through Drizzle Kit and commit both SQL and metadata. Configure Drizzle Kit for
the PostgreSQL dialect, `src/infrastructure/persistence/schema.ts`, and the
`drizzle/` output directory. `migrate.ts` must call Drizzle's migration runner
and close its `pg` pool in `finally`.

- [ ] **Step 4: Implement the repository with atomic SQL semantics**

`consumeChallenge` inserts the challenge hash with `onConflictDoNothing` and
returns whether it inserted. `registerUniqueVisitor` inserts the visitor hash
with `onConflictDoNothing`, updates `last_seen_at` for an existing visitor,
then reads `COUNT(*)` inside one transaction. `allowRateAttempt` opens a short
window for a new key, increments a non-expired key, and returns false after ten
attempts in the five-minute window. `pruneExpired` deletes expired challenge
and rate rows.

Keep the repository in the infrastructure layer; return only primitive values
and domain records through its port. No logger may receive a raw cookie, raw
IP, or raw user-agent field.

- [ ] **Step 5: Run migration and integration verification**

Run: `npm run db:migrate`

Expected: exit 0 and the migration log records `0000_qualified_visitors` once.

Run: `npm run test:integration -- tests/integration/drizzle-visitor-repository.test.ts`

Expected: PASS against the disposable local PostgreSQL database.

- [ ] **Step 6: Commit persistence**

```bash
git add com.madsrs.docs/drizzle com.madsrs.docs/drizzle.config.ts com.madsrs.docs/src/infrastructure/persistence com.madsrs.docs/tests/integration
git commit -m "feat(docs): persist qualified visitor counts"
```

### Task 5: Add visitor application use-cases and private App Router endpoints

**Files:**
- Create: `com.madsrs.docs/src/application/use-cases/{get-visitor-count,issue-visitor-challenge,register-qualified-visitor}.ts`
- Create: `com.madsrs.docs/src/presentation/server/{container,request-evidence}.ts`
- Create: `com.madsrs.docs/src/app/api/visitor-challenge/route.ts`, `com.madsrs.docs/src/app/api/visitor/route.ts`
- Create: `com.madsrs.docs/src/app/api/health/live/route.ts`, `com.madsrs.docs/src/app/api/health/ready/route.ts`
- Create: `com.madsrs.docs/tests/unit/application/register-qualified-visitor.test.ts`, `com.madsrs.docs/tests/unit/presentation/visitor-routes.test.ts`

**Interfaces:**
- Produces: `IssueVisitorChallenge.execute(evidence)`, `RegisterQualifiedVisitor.execute(input)`, and `GetVisitorCount.execute()`.
- Produces: `GET /api/visitor-challenge`, `POST /api/visitor`, `GET /api/health/live`, and `GET /api/health/ready`.

- [ ] **Step 1: Write failing use-case and route tests**

Initialize a deterministic codec and fixed clock in `beforeEach`. Build a
valid proof in the test itself so it exercises the real domain verifier:

```ts
const validRegistrationInput = async (): Promise<QualifiedVisitInput> => {
  const challenge = codec.issue(fixedNow);
  return {
    evidence: {
      host: "docs.madsrs.com",
      origin: "https://docs.madsrs.com",
      fetchSite: "same-origin",
      fetchMode: "cors",
      userAgent: "Mozilla/5.0 Chrome/140.0 Safari/537.36",
      webdriver: false,
      clientAddress: "203.0.113.25",
    },
    challenge: challenge.token,
    nonce: await solveProof(challenge),
    existingVisitorCookie: undefined,
  };
};
```

```ts
it("registers a valid first visit and returns a signed cookie plus total", async () => {
  const result = await useCase.execute(await validRegistrationInput());
  expect(result).toEqual({ kind: "counted", total: 1, visitorCookie: expect.any(String) });
});

it("does not mint a cookie or increase the total for a replayed challenge", async () => {
  repository.consumeChallenge = async () => false;
  await expect(useCase.execute(await validRegistrationInput())).resolves.toEqual({ kind: "rejected" });
});

it("returns a generic 403 for an unqualified request", async () => {
  const response = await POST(visitorRequest({ webdriver: true }));
  expect(response.status).toBe(403);
  await expect(response.json()).resolves.toEqual({ error: "visit not qualified" });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npx vitest run --project unit tests/unit/application/register-qualified-visitor.test.ts tests/unit/presentation/visitor-routes.test.ts`

Expected: FAIL because the use-cases, composition root, and handlers are absent.

- [ ] **Step 3: Implement use-case sequence and route contracts**

`IssueVisitorChallenge` evaluates server-derived request evidence before issuing
a five-minute challenge. `RegisterQualifiedVisitor` verifies browser evidence,
token signature and expiry, proof, rate limit, then consumes the challenge
before registering a visitor. On success it issues a new cookie only when the
request lacks a valid existing cookie. A valid existing cookie yields `known`
with the current total and never increments the total.

The `POST /api/visitor` request body is exactly:

```ts
type RegisterVisitBody = { challenge: string; nonce: number; webdriver: boolean };
```

Both visitor endpoints set `Cache-Control: no-store`, force the Node.js runtime,
and use `NextResponse` cookies configured as `httpOnly: true`, `secure` in
production, `sameSite: "lax"`, and `path: "/"`. Route failures return the
generic rejection body without implementation details. The live health route
always returns `{ status: "ok" }`; readiness calls a repository database ping
and returns 503 `{ status: "unavailable" }` on failure.

- [ ] **Step 4: Run focused application and route tests**

Run: `npx vitest run --project unit tests/unit/application/register-qualified-visitor.test.ts tests/unit/presentation/visitor-routes.test.ts`

Expected: PASS; one count is registered, replay is rejected, and health routes
have distinct liveness/readiness behavior.

- [ ] **Step 5: Commit application and route handlers**

```bash
git add com.madsrs.docs/src/application/use-cases com.madsrs.docs/src/presentation/server com.madsrs.docs/src/app/api com.madsrs.docs/tests/unit/application com.madsrs.docs/tests/unit/presentation
git commit -m "feat(docs): expose qualified visitor endpoints"
```

### Task 6: Build the responsive documentation shell, MDX renderer, search, and tracker

**Files:**
- Create: `com.madsrs.docs/src/app/docs/[...slug]/page.tsx`, `com.madsrs.docs/src/app/{robots,sitemap}.ts`
- Create: `com.madsrs.docs/src/presentation/components/{docs-layout,documentation-page,header,home-hero,mdx-components,mobile-nav,on-page-toc,search-dialog,sidebar,theme-toggle,visitor-counter,visitor-tracker}.tsx`
- Modify: `com.madsrs.docs/src/app/{layout,page,globals}.tsx`
- Create: `com.madsrs.docs/tests/components/{docs-layout,search-dialog,visitor-counter}.test.tsx`

**Interfaces:**
- Consumes: `ReadDocumentation.execute(slug)` from Task 2 and visitor endpoints from Task 5.
- Produces: static documentation routes, metadata, search behavior, theme preference, and progressive visitor registration.

- [ ] **Step 1: Write failing component tests**

Mock navigation in `tests/components/search-dialog.test.tsx` with:

```ts
const mockPush = vi.fn();
vi.mock("next/navigation", () => ({ useRouter: () => ({ push: mockPush }) }));
```

```tsx
it("opens the search dialog and navigates to the selected result", async () => {
  render(<SearchDialog entries={[{ title: "Quick start", slug: ["introduction", "quick-start"], description: "Create an app" }]} />);
  await userEvent.keyboard("{Control>}k{/Control}");
  await userEvent.type(screen.getByRole("searchbox"), "quick");
  await userEvent.click(screen.getByRole("option", { name: /quick start/i }));
  expect(mockPush).toHaveBeenCalledWith("/docs/introduction/quick-start");
});

it("shows an unavailable visitor count without hiding the documentation CTA", () => {
  render(<VisitorCounter state={{ kind: "unavailable" }} />);
  expect(screen.getByText("Visitor count unavailable")).toBeVisible();
  expect(screen.getByRole("link", { name: /get started/i })).toBeVisible();
});
```

- [ ] **Step 2: Run component tests to verify they fail**

Run: `npx vitest run --project components tests/components/docs-layout.test.tsx tests/components/search-dialog.test.tsx tests/components/visitor-counter.test.tsx`

Expected: FAIL because the documentation components do not exist.

- [ ] **Step 3: Implement server-rendered pages and accessible components**

`app/docs/[...slug]/page.tsx` must use `generateStaticParams`,
`generateMetadata`, and `notFound()` with `ReadDocumentation`. Render MDX only
on the server with explicit components for `<pre>`, `<code>`, headings, links,
callouts, and a copyable code block. Generate a local sitemap and robots file
that allows documentation crawling but excludes `/api/`.

Build a keyboard-accessible header and search dialog (`Ctrl/Cmd+K`, Escape,
arrow-key option selection), desktop/sidebar navigation, mobile dialog, page
table of contents, and previous/next links. Theme selection uses the system
value on first render and persists a `data-theme` choice to local storage.
Use semantic landmarks, one h1 per page, visible focus outlines, and CSS that
collapses navigation below 960px.

The root layout mounts `VisitorTracker` as a client component. It requests a
challenge after the document is interactive, solves proof-of-work in a Web
Worker (`workers/visitor-proof.worker.ts`), posts its answer, and silently stops on error. `VisitorCounter` calls
the same flow on the home page and renders `N developers have started with
MADS` only after success; it renders the tested unavailable label when the
endpoint cannot respond. No navigation or document rendering waits for either
component.

- [ ] **Step 4: Run component, type, and build checks**

Run: `npx vitest run --project components tests/components/docs-layout.test.tsx tests/components/search-dialog.test.tsx tests/components/visitor-counter.test.tsx`

Expected: PASS.

Run: `npm run typecheck && npm run build`

Expected: all MDX routes generate and Next.js exits 0.

- [ ] **Step 5: Commit documentation experience**

```bash
git add com.madsrs.docs/src/app com.madsrs.docs/src/presentation/components com.madsrs.docs/tests/components
git commit -m "feat(docs): add responsive documentation experience"
```

### Task 7: Validate MADS documentation accuracy and end-to-end reader flows

**Files:**
- Modify: all `com.madsrs.docs/content/docs/**/*.mdx` files from Task 2 as needed after rendered review
- Create: `com.madsrs.docs/tests/unit/infrastructure/documentation-catalog.test.ts`
- Create: `com.madsrs.docs/tests/e2e/documentation-site.spec.ts`
- Create: `com.madsrs.docs/playwright.config.ts`

**Interfaces:**
- Consumes: documentation repository, static routes, navigation, and home page from Tasks 2 and 6.
- Produces: a complete checked content catalog and browser-level reader journey coverage.

- [ ] **Step 1: Write the failing catalog and browser tests**

```ts
it("publishes every required section with ordered page metadata", async () => {
  const pages = await repository.list();
  expect(pages.map((page) => page.slug.join("/"))).toEqual(expect.arrayContaining([
    "introduction/quick-start",
    "build-an-api/validation",
    "configuration/typed-configuration-and-secrets",
    "data-and-security/postgres-and-diesel",
    "guides/build-a-user-api",
  ]));
});
```

```ts
test("a new reader can reach the quick start, search validation, and use mobile navigation", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("link", { name: "Get started" }).click();
  await expect(page.getByRole("heading", { name: "Quick start" })).toBeVisible();
  await page.keyboard.press("Control+k");
  await page.getByRole("searchbox").fill("validation");
  await page.getByRole("option", { name: /request validation/i }).click();
  await expect(page).toHaveURL(/build-an-api\/validation/);
});
```

- [ ] **Step 2: Run catalog and browser tests to verify they fail**

Run: `npx vitest run --project unit tests/unit/infrastructure/documentation-catalog.test.ts`

Expected: FAIL until every required page and frontmatter record exists.

Run: `npx playwright test tests/e2e/documentation-site.spec.ts`

Expected: FAIL until the browser flow, search dialog, and routes are complete.

- [ ] **Step 3: Complete the final content audit**

Check every code block against the current files listed in Task 2. Preserve
v0.8.0 facts: Rust 1.85 and edition 2024; `mads new` starter uses only `http`
and `runtime-tokio`; conventional `Mads::run` loading order is `.env`,
`mads.toml`, then scalar `MADS_*` overrides; `ValidatedJson`,
`ValidatedQuery`, and `ValidatedPath` return MADS validation errors; database,
JWT, and cookies remain feature-gated opt-ins. Link code samples to the
repository's complete examples when readers need a longer reference.

Include a short privacy statement on the home page and a deployment page that
states the self-hosted counter uses signed anonymous cookies, challenge hashes,
and daily rate-limit hashes. State plainly that sophisticated browser bots can
still imitate browser evidence and solve proof-of-work.

- [ ] **Step 4: Run content and end-to-end checks**

Run: `npx vitest run --project unit tests/unit/infrastructure/documentation-catalog.test.ts`

Expected: PASS with every expected slug present exactly once.

Run: `npm run build && npx playwright test tests/e2e/documentation-site.spec.ts`

Expected: build exits 0 and the reader journey passes in Chromium.

- [ ] **Step 5: Commit content audit and browser tests**

```bash
git add com.madsrs.docs/content com.madsrs.docs/tests/unit/infrastructure/documentation-catalog.test.ts com.madsrs.docs/tests/e2e com.madsrs.docs/playwright.config.ts
git commit -m "docs: complete MADS framework guides"
```

### Task 8: Package the production Docker and Compose deployment contract

**Files:**
- Create: `com.madsrs.docs/Dockerfile`, `com.madsrs.docs/docker-compose.yml`, `com.madsrs.docs/.dockerignore`
- Modify: `com.madsrs.docs/.env.example`, `com.madsrs.docs/package.json`
- Create: `com.madsrs.docs/tests/e2e/docker-compose.spec.ts`, `com.madsrs.docs/tests/e2e/support/compose-harness.ts`

**Interfaces:**
- Consumes: `npm run build`, `npm run db:migrate`, liveness/readiness endpoints, and environment contract from Tasks 1, 4, and 5.
- Produces: `docs-web` runner image, `migrate` image target, and private `postgres` Compose service for the existing deployment webhook.

- [ ] **Step 1: Write failing Docker contract checks**

Create the test-only Compose harness using `execa` and ensure every test calls
`await compose.down()` in `afterEach`:

```ts
export class ComposeHarness {
  constructor(private readonly workingDirectory: string) {}

  async up(services: string[]) {
    await execa("docker", ["compose", "up", "--build", "-d", ...services], { cwd: this.workingDirectory });
  }

  async publishedPorts(service: string) {
    const { stdout } = await execa("docker", ["compose", "port", service], { cwd: this.workingDirectory });
    return stdout.split("\n").filter(Boolean);
  }

  async down() {
    await execa("docker", ["compose", "down", "--volumes"], { cwd: this.workingDirectory });
  }
}
```

```ts
test("the Compose stack migrates before serving liveness and readiness", async () => {
  await compose.up(["postgres", "migrate", "docs-web"]);
  await expect(fetch("http://127.0.0.1:3000/api/health/live")).resolves.toMatchObject({ ok: true });
  await expect(fetch("http://127.0.0.1:3000/api/health/ready")).resolves.toMatchObject({ ok: true });
});

test("PostgreSQL has no published host port", async () => {
  expect(await compose.publishedPorts("postgres")).toEqual([]);
});
```

- [ ] **Step 2: Run the Docker contract test to verify it fails**

Run: `npx playwright test tests/e2e/docker-compose.spec.ts`

Expected: FAIL because Dockerfile and Compose services do not exist.

- [ ] **Step 3: Implement multi-stage image and Compose services**

Create a Dockerfile with `deps`, `builder`, `migrator`, and `runner` targets.
Set `output: "standalone"` in `next.config.ts`; `runner` copies the standalone
server, `.next/static`, and `public` assets and runs as an unprivileged user.
`migrator` includes source, Drizzle migrations, and the dependencies needed by
`npm run db:migrate`.

Compose must define:

```yaml
postgres:
  image: postgres:16-alpine
  volumes: [postgres_data:/var/lib/postgresql/data]
  healthcheck: pg_isready -U $${POSTGRES_USER} -d $${POSTGRES_DB}
  ports: []
migrate:
  build: { context: ., target: migrator }
  depends_on: { postgres: { condition: service_healthy } }
  command: npm run db:migrate
docs-web:
  build: { context: ., target: runner }
  depends_on: { migrate: { condition: service_completed_successfully } }
  ports: [127.0.0.1:3000:3000]
```

Supply all application secrets through `env_file: .env`; document copying
`.env.example` to `.env` locally and setting distinct high-entropy production
secrets on the deployment server. Exclude `node_modules`, `.next`, `.env`, test
artifacts, and Git files from Docker build context. Do not expose PostgreSQL.

- [ ] **Step 4: Run image, Compose, and full verification**

Run: `docker compose config`

Expected: exit 0 and no resolved `postgres.ports` field.

Run: `docker compose build --no-cache`

Expected: both migrator and runner targets build successfully.

Run: `npx playwright test tests/e2e/docker-compose.spec.ts`

Expected: migrations finish before the app, liveness and readiness return 200,
and PostgreSQL has no host-published port.

Run: `npm run verify && npm run test:integration && npm run test:e2e`

Expected: lint, type check, unit/component tests, build, integration tests, and
browser tests all exit 0.

- [ ] **Step 5: Commit delivery configuration**

```bash
git add com.madsrs.docs/Dockerfile com.madsrs.docs/docker-compose.yml com.madsrs.docs/.dockerignore com.madsrs.docs/.env.example com.madsrs.docs/next.config.ts com.madsrs.docs/package.json com.madsrs.docs/tests/e2e/docker-compose.spec.ts com.madsrs.docs/tests/e2e/support/compose-harness.ts
git commit -m "feat(docs): containerize documentation deployment"
```

## Plan Self-Review

- **Spec coverage:** Tasks 1, 2, 6, and 7 implement the public documentation
  experience, MADS curriculum, visual mark, search, accessibility, and
  responsive navigation. Tasks 3 through 5 implement the cleanly layered,
  dependency-free visitor qualification flow. Task 4 implements PostgreSQL
  persistence and migrations. Task 8 implements the Docker/Compose contract
  for existing webhook deployment.
- **Failure behavior:** Task 5 keeps analytics rejection and outage handling
  out of rendering; Task 6 tests the unavailable counter display; Task 8 keeps
  liveness independent of readiness.
- **Privacy:** Tasks 3 and 4 define and test opaque signed cookies, one-way
  hashes, daily HMAC rate keys, and the absence of raw identifying request data.
- **Type consistency:** `DocumentationRepository`, `VisitorRepository`,
  `ChallengeCodec`, `ReadDocumentation`, `IssueVisitorChallenge`, and
  `RegisterQualifiedVisitor` use the same names and return values wherever
  consumed.
