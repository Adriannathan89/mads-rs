# MADS Documentation Site Design

**Status:** Approved for implementation

**Target:** A production Next.js documentation website served at
`docs.madsrs.com`.

## Goal

Create an approachable, framework-style documentation site for MADS.rs 0.8.0.
It must guide a new Rust developer from installation through a usable HTTP
application while remaining technically faithful to the repository's current
public behavior. The site also displays a privacy-preserving count of unique,
best-effort-qualified human visitors.

## Scope and non-goals

The repository gains a standalone `docs-web/` Next.js application, a Docker
delivery path, and curated documentation content. Existing Rust crates and
their public APIs are not modified.

The documentation site is not a CMS, user account system, general analytics
platform, nor an API documentation generator. Documentation editing remains a
repository workflow through local MDX files. The existing server-side webhook
detects `docs.madsrs.com`, builds the image from the `main` branch, and deploys
it; this repository supplies the image and Compose contract but does not own
the webhook implementation.

No third-party bot-verification provider is required. The human counter is a
best-effort abuse-resistant metric, not an impossible guarantee against a
determined adversary with a full browser and computational resources.

## Reader experience and information architecture

The home page presents the existing `docs/mads.png` mark, a concise MADS
value proposition, a copyable `mads new my-app` command, the public qualified
visitor total, and clear links to begin learning and visit the source project.

Every documentation page has a responsive documentation shell:

- A top bar with the MADS mark, documentation navigation, GitHub link, theme
  switcher, and keyboard-accessible search.
- A collapsible sidebar on small displays and persistent sidebar on desktop.
- Breadcrumb, readable content column, generated on-page table of contents,
  syntax-highlighted code samples with copy controls, and previous/next page
  links.
- System-preference-aware light and dark themes with user preference retained
  locally, visible focus states, semantic headings, and reduced-motion-safe
  interaction.

The initial documentation tree is intentionally learner-first:

```text
Introduction
  What is MADS?
  Installation
  Quick start
  Project anatomy
Fundamentals
  Modules and application scope
  Providers and services
  Lifecycle and startup
Build an API
  Routes and controllers
  Request validation
  REST errors
  CORS
Configuration
  mads.toml and environment overrides
  Typed configuration and secrets
Data and security
  PostgreSQL and Diesel
  Migrations
  Passport, JWT, and cookies
Tooling and reference
  MADS CLI
  Features and compatibility
  Diagnostics and deployment
Guides
  Build a user API
```

Content is written as curated MDX rather than mechanically rendering existing
repository Markdown. The repository README, `docs/CLI.md`,
`docs/ARCHITECTURE.md`, crate READMEs, and complete examples remain the
technical sources. The site makes their v0.8.0 contracts understandable in a
progressive sequence and links to source material when deeper detail helps.

## Clean architecture

The application separates its stable domain and use-cases from the Next.js and
database details:

```text
Presentation (Next.js App Router pages, React components, route handlers)
                    |
Application (load documentation, qualify visit, register unique visitor)
                    |
Domain (documentation records, visit challenge, visitor repository ports)
                    |
Infrastructure (filesystem MDX reader, PostgreSQL repository, crypto helpers)
```

The presentation layer depends on application use-cases only. The application
layer depends on domain types and interfaces, never on SQL, React, cookies, or
environment variables. Infrastructure implements the interfaces and is wired
at a small composition root. This makes visitor policy testable without
Next.js, PostgreSQL, or browser mocks and keeps content reading independent of
the analytics feature.

The primary interfaces are:

- `DocumentationRepository`: lists navigation metadata, resolves a page by
  slug, and supplies its compiled source.
- `VisitorRepository`: records a unique qualified visitor atomically, returns
  the aggregate total, and manages expired rate-limit entries.
- `ChallengeSigner`: creates and validates short-lived signed proof-of-work
  challenges and signed visitor-cookie values.
- `BrowserVisitPolicy`: accepts or rejects the supplied browser evidence
  before persistence.

## Self-hosted qualified visitor counter

The visitor counter is intentionally invisible to ordinary readers and stores
no raw IP address, user agent, page history, or personal profile.

1. A client tracker is mounted once in the root documentation layout after the
   page is usable. It does nothing when JavaScript is unavailable; reading the
   documentation never depends on analytics.
2. The tracker requests a short-lived, HMAC-signed challenge. The server
   rejects cross-origin requests and obvious non-browser user agents.
3. The browser solves a deliberately modest SHA-256 proof-of-work challenge
   in a worker and submits its nonce, solution, and normal same-origin fetch
   metadata. The difficulty is configurable and selected so normal devices
   have no perceptible interaction delay.
4. The route handler validates signature, expiry, one-time challenge use,
   proof, `navigator.webdriver` evidence, expected fetch headers, known bot
   denylist, and a short-lived rate limit keyed by a daily HMAC of the IP.
   The raw address is never written to PostgreSQL.
5. On the first accepted visit, the server issues a signed random
   `HttpOnly`, `Secure`, `SameSite=Lax`, path-scoped visitor cookie. Its
   one-way HMAC hash is the stable visitor identity stored in PostgreSQL.
6. PostgreSQL inserts that hash with `ON CONFLICT DO NOTHING`; the qualified
   count rises only on a new identity. It returns the aggregate total for the
   public home-page display. Reloads and later visits from the same valid
   cookie do not increment it.

This combination blocks ordinary crawlers, raw HTTP scripts, basic headless
automation, replayed challenges, and rapid repeated submissions without adding
a visible prompt, vendor script, account, or user-facing configuration.
Sophisticated browser automation can imitate these signals and solve the
proof; the site documents the limit rather than overstating the counter's
accuracy.

The route returns a generic rejection for an unqualified request, never sets a
visitor cookie on failure, and exposes no diagnostic signal that makes bypass
easier. PostgreSQL or analytics failures are caught by the presentation layer:
the site remains readable and the count becomes unavailable rather than
preventing a page from rendering.

## Persistence

PostgreSQL is the sole operational dependency. The schema has three focused
tables:

```text
qualified_visitors
  visitor_hash       unique, non-null opaque hash
  first_seen_at      timestamp with time zone
  last_seen_at       timestamp with time zone

visitor_rate_limits
  rate_key           unique, non-null daily HMAC value
  window_ends_at     timestamp with time zone
  attempts           non-negative integer

consumed_visit_challenges
  challenge_hash     unique, non-null one-way hash
  expires_at         timestamp with time zone
```

The visitor table has no IP, user agent, referring URL, or cookie value.
Expired rate-limit rows and consumed challenge records are pruned by the
application's bounded cleanup operation. A consumed challenge is inserted in
the same transaction that qualifies a visit, preventing a valid proof from
being replayed to mint multiple visitor identities. Unique visitor insert and
total retrieval also run transactionally so concurrent first visits cannot
double-count.

`DATABASE_URL`, `VISITOR_COOKIE_SECRET`, `VISITOR_CHALLENGE_SECRET`,
`VISITOR_RATE_LIMIT_SECRET`, `VISITOR_ALLOWED_HOSTS`, and
`VISITOR_PROOF_DIFFICULTY` are required configuration in production. The
secrets must differ, be high-entropy values, and never be committed. Development
has explicit safe defaults only when `NODE_ENV=development`.

## Delivery and operations

`docs-web/` contains a multi-stage Dockerfile that builds the Next.js app and
runs the minimal production output as an unprivileged user. A production
`docker-compose.yml` declares:

- `docs-web`: receives the image built by the existing webhook, consumes
  environment secrets supplied by the server, and exposes an internal HTTP
  port for the domain's reverse proxy.
- `postgres`: PostgreSQL with a persistent named volume, health check, no
  public port, and credentials supplied through deployment environment.
- `migrate`: a short-lived service that waits for PostgreSQL health and applies
  idempotent tracked migrations before the web service is started.

The web application exposes a lightweight liveness endpoint that does not
depend on analytics availability. A separate readiness endpoint verifies the
database connection for deployment diagnostics. The Compose configuration
starts the database and migration service before marking the web service ready.
Reverse-proxy TLS, DNS, webhook verification, and image registry credentials
remain server responsibilities because they are already handled by the user's
deployment mechanism.

## Verification design

Tests protect the architectural boundaries and user-visible contracts:

1. Domain and application tests prove challenge expiry/signature checks,
   proof verification, bot/header rejection, no cookie on rejection, one
   qualified visitor increment, and duplicate-cookie idempotence.
2. PostgreSQL integration tests apply migrations, exercise concurrent unique
   inserts, verify aggregate reads, and confirm rate-limit cleanup.
3. Documentation repository tests check every navigation item resolves to an
   MDX page with title, description, and ordered headings; search indexes only
   published pages.
4. Component and browser tests cover accessible navigation, mobile sidebar,
   theme persistence, search, page table of contents, copyable snippets, and
   the analytics-unavailable display.
5. Production checks run type-checking, linting, unit/integration/e2e suites,
   the Next.js production build, Docker image build, and Compose configuration
   validation. A local Compose smoke test verifies migration, liveness,
   readiness, rendered home page, and a qualified visitor registration.

## Acceptance criteria

1. `docs.madsrs.com` can serve the responsive MADS documentation site through
   the Docker image and Compose contract expected by the existing main-branch
   deployment webhook.
2. A new reader can install MADS, scaffold a project, run it, understand the
   generated layout, and find modules, HTTP, configuration, validation,
   database, authentication, and CLI guidance without reading source code.
3. Site content accurately reflects the checked-in MADS v0.8.0 public
   contracts and uses `docs/mads.png` as the site mark.
4. Qualified human-like browser visits are counted once per signed anonymous
   cookie; obvious bots, cross-origin calls, replayed/invalid challenges, and
   rate-limited requests do not change the total.
5. The counter requires no third-party user-verification service and does not
   collect raw IP addresses, user-agent strings, page history, or account data.
6. Documentation remains fully readable if JavaScript, analytics, or
   PostgreSQL analytics operations fail.
7. The production build, tests, Docker image, migrations, and Compose
   configuration all pass their respective verification checks.
