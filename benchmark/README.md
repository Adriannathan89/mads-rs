# MADS HTTP stress benchmark

This suite drives the three [MADS 0.9.1 example applications](../example/) over
real loopback HTTP. It checks response content and status under load, then
reports throughput and client-observed p50/p95/p99 latency. The applications
target MADS 0.9.1 packages, as pinned in their `Cargo.lock` files.
Python 3's standard library generates the traffic; no load-test package is
required.

## Workloads

| Case | Stress profile | Contract checked |
| --- | --- | --- |
| `hello` | 12,000 GETs, 64 clients | Every response is 200 with `Hello, world!`. |
| `connection-churn` | 2,000 GETs, 64 clients | A fresh connection for every request still returns the expected 200 body. |
| `validation` | 3,000 POSTs, 32 clients | Empty fields, malformed JSON, and 128 KiB invalid bodies all return a validation error. |
| `oversized` | 32 POSTs, 8 clients | 3 MiB JSON bodies exceed the default limit and return a structured 413 response. |
| `oversized-reuse` | Eight POSTs, four clients | Each 413 signals `Connection: close`; clients can send the next request without a transport error. |
| `body-limit-boundary` | 96 POSTs and 96 follow-up GETs, 8 clients | Valid JSON at 2 MiB − 1 and 2 MiB gets 422; at 2 MiB + 1 it gets 413 with `Connection: close`; each client then receives a 401 on its next request. |
| `aborted-upload` | 256 partial uploads, 256 logins, and 256 protected GETs, 32 clients | Clients close a 1 KiB upload with a declared 1 MiB body; login and JWT-protected reads still succeed. |
| `jwt` | 6,000 GETs, 64 clients | Valid JWT returns the demo profile; missing and malformed JWTs return 401 with a Bearer challenge. |
| `posts` | 800 CRUD transactions, 32 clients | Each transaction creates, reads, updates, deletes, then confirms 404 for its own post (4,000 HTTP requests total). |
| `database-failure` | One forced startup failure | An unavailable PostgreSQL endpoint fails startup within 10 seconds, never binds HTTP, and does not print the URL password. |
| `database-connect-timeout` | One stalled PostgreSQL handshake | A local TCP server accepts the database connection but never replies; startup must fail near the configured 2-second timeout, before binding HTTP, without leaking credentials. |
| `database-query-timeout-recovery` | One query timeout and recovery | An exclusive posts-table lock makes a query exceed PostgreSQL's 2-second `statement_timeout`; HTTP stays up, returns a safe 500 for that query, and resumes successful CRUD after lock release. |
| `database-tcp-stall-recovery` | One TCP reply stall and recovery | A loopback proxy withholds a database reply on an established connection; requests hit the 2-second pool-acquire timeout, HTTP stays up, and CRUD succeeds after replies resume. |

`smoke` uses 200/40/90/4/12/16/90/20 operations for
hello/connection-churn/validation/oversized/body-limit-boundary/aborted-upload/JWT/posts.
`extended` uses 50,000/5,000/10,000/64/192/512/25,000/2,000 operations
for the same cases. Every profile also runs eight `oversized-reuse` requests.
The runner starts and stops each application, reuses one HTTP connection per
worker, and exits nonzero for unexpected status, incorrect response content,
transport errors, or a failed startup. `database-failure` expects startup to
fail and checks that its test credential is redacted. `database-connect-timeout`
uses a local simulated PostgreSQL socket, not a real database, and requires the
connection to be accepted before the timeout. Neither case needs
`BENCH_DATABASE_URL`. The two recovery cases require a real, isolated
`BENCH_DATABASE_URL` and the posts schema. The query-timeout case also requires
`psql` on `PATH` and holds an exclusive table lock for about 2 seconds. The TCP
case tests pool acquisition while an existing connection's reply is stalled;
it does not claim a separate socket-read timeout. The suite does not impose
arbitrary throughput thresholds. A run against `posts` writes and deletes rows,
so use an isolated PostgreSQL database.
The `oversized` case opens a new connection per request because the body-limit
rejection closes its connection. The `oversized-reuse` case checks that the
response signals this closure and that clients can continue on a new connection.
It is included in the default run.

## Prepare

From the repository root, build optimized binaries:

```sh
cargo build --release --manifest-path example/hello-world/Cargo.toml
cargo build --release --manifest-path example/protected-route/Cargo.toml
cargo build --release --manifest-path example/posts-crud/Cargo.toml
```

Create a disposable PostgreSQL database and apply the example schema. Replace
the URL with your local credentials and database name:

```sh
export BENCH_DATABASE_URL='postgres://postgres:postgres@127.0.0.1:5432/mads_benchmark'
psql "$BENCH_DATABASE_URL" -f example/posts-crud/migrations/001_create_posts.sql
```

The database itself must already exist. The runner never creates or drops a
database or schema. The three example servers use ports 3000, 3002, and 3001,
which must be free. The runner supplies benchmark-only JWT/demo credentials to
the protected-route example. It does not print the database URL or token.

## Run

```sh
python3 -m unittest discover -s benchmark/tool -p 'test_*.py'
python3 benchmark/tool/run.py --profile smoke --output /tmp/mads-smoke.json
python3 benchmark/tool/run.py --profile stress --output /tmp/mads-stress.json
python3 benchmark/tool/run.py --profile extended --output /tmp/mads-extended.json
python3 benchmark/tool/run.py --case oversized-reuse --output /tmp/mads-reuse.json
python3 benchmark/tool/run.py --case database-connect-timeout \
  --output /tmp/mads-database-timeout.json
python3 benchmark/tool/run.py --case database-query-timeout-recovery \
  --output /tmp/mads-query-recovery.json
python3 benchmark/tool/run.py --case database-tcp-stall-recovery \
  --output /tmp/mads-tcp-recovery.json
python3 benchmark/tool/run.py --profile stress \
  --case connection-churn --case body-limit-boundary --case aborted-upload \
  --output /tmp/mads-edges.json
```

To skip PostgreSQL, select only HTTP cases:

```sh
python3 benchmark/tool/run.py --profile stress \
  --case hello --case connection-churn --case validation --case oversized \
  --case oversized-reuse --case body-limit-boundary --case aborted-upload \
  --case jwt --case database-failure --case database-connect-timeout
```

For a quick local check with existing debug binaries, add
`--binary-profile debug`. Use release binaries for comparable performance
numbers; the 0.9.1 edge-case follow-up explicitly reports debug builds.
The JSON output includes the commit, OS, CPU count, profile, statuses, errors,
request rate, and latency percentiles. A `posts` operation is a five-request
transaction, whereas other operations are one HTTP request.

These results measure the whole loopback path, including Python's client cost,
MADS routing, application code, and optional PostgreSQL. They are useful for
reproducible regression checks and failure discovery, but are not an isolated
measurement of framework overhead or a guarantee for every deployment.
See [the measured run and findings](REPORT.md).

## MADS vs native Axum vs Go Fiber v2

The separate [comparison report](COMPARISON.md) measures the three workloads
from `example/`: Hello World (`GET /`), PostgreSQL posts CRUD (five HTTP
requests per transaction), and login plus JWT-protected profile reads with
input validation and logger output. The native Axum and Fiber counterparts
live in `benchmark/targets/`. The old MADS-only stress and fault runner remains
in `benchmark/tool/run.py`; all Python benchmark tools and their tests now live
in `benchmark/tool/`.

Build release binaries from the repository root:

```sh
python3 benchmark/tool/build_mads.py
cargo build --release --locked --manifest-path benchmark/targets/axum/Cargo.toml
cd benchmark/targets/fiber && mkdir -p bin && go build -o bin/mads-bench-fiber . && cd ../../..
```

`build_mads.py` copies each MADS example to a temporary directory and patches
its `mads`/`mads-persistence` dependencies to this checkout for the build. It
does not edit the examples or their lockfiles. The reproducible local-source
lockfiles are stored in `benchmark/targets/mads/locks/`; on a warm cache,
`build_mads.py --offline` avoids network access. This is necessary because the
current published-package lockfile of the examples has a registry checksum
mismatch for `mads-common-macros 0.9.1`. Results from this command measure
**local MADS 0.9.1 source**, not the published crate archive.

For CRUD, provision three isolated PostgreSQL databases, apply the same
`example/posts-crud/migrations/001_create_posts.sql` to each, and set:

```sh
export BENCH_MADS_DATABASE_URL='postgres://USER:PASSWORD@127.0.0.1:5432/mads_bench_mads?sslmode=disable'
export BENCH_AXUM_DATABASE_URL='postgres://USER:PASSWORD@127.0.0.1:5432/mads_bench_axum?sslmode=disable'
export BENCH_FIBER_DATABASE_URL='postgres://USER:PASSWORD@127.0.0.1:5432/mads_bench_fiber?sslmode=disable'
```

The runner does not query PostgreSQL version metadata separately. Set
`BENCH_POSTGRES_VERSION` (for example, `16.15`) to include the known server
version in the raw result; otherwise it is recorded as `not captured`.

The runner does not create or drop databases. Only use disposable databases:
the CRUD workload writes and deletes rows. The same schema is required in
each database. The example's default ports 3000, 3001, and 3002 must be free.

```sh
python3 -m unittest discover -s benchmark/tool -p 'test_*.py'
python3 benchmark/tool/compare.py --profile smoke --runs 1 --output /tmp/mads-compare-smoke.json
python3 benchmark/tool/compare.py --profile stress --runs 5 --output /tmp/mads-compare-stress.json
```

Use `--framework mads|axum|fiber` and `--scenario hello|posts|auth` to run a
subset. Each target is started one at a time. Contract checks and a warmup
must pass before measured runs begin; an error makes the report fail rather
than silently excluding the run. The JSON contains every run plus median
responses/second and median p95 latency. Authentication performance covers
one valid login and one valid protected read per operation. Invalid input,
missing posts, and unsupported content types are checked by status and shared
error code; detailed validation/not-found error bodies are not normalized
across frameworks. Missing/malformed JWT checks also require the MADS Passport
Bearer challenge and unauthorized JSON envelope, including case-insensitive
Bearer schemes. A 3 MiB body-limit probe
is recorded separately because Fiber v2 can close the connection before this
Python client receives an HTTP response; its outcome does not invalidate
the valid-request throughput run.

These results include Python client overhead, different database access
libraries (SeaORM, SQLx, and Go `database/sql`), JWT libraries, and logger
costs. They are end-to-end application comparisons, not isolated router
overhead; do not extrapolate them to TLS, HTTP/2, distributed deployments, or
other hardware.
