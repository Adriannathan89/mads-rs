# MADS HTTP stress benchmark

This suite drives the three [MADS 0.9 example applications](../example/) over
real loopback HTTP. It checks response content and status under load, then
reports throughput and client-observed p50/p95/p99 latency. The applications
use published MADS 0.9.0 packages, as pinned in their `Cargo.lock` files.
Python 3's standard library generates the traffic; no load-test package is
required.

## Workloads

| Case | Stress profile | Contract checked |
| --- | --- | --- |
| `hello` | 12,000 GETs, 64 clients | Every response is 200 with `Hello, world!`. |
| `validation` | 3,000 POSTs, 32 clients | Empty fields, malformed JSON, and 128 KiB invalid bodies all return a validation error. |
| `oversized` | 32 POSTs, 8 clients | 3 MiB JSON bodies exceed the default limit and return a structured 413 response. |
| `oversized-reuse` | Eight POSTs, four clients | Diagnostic for reusing one HTTP/1.1 connection after a 413. Currently reproduces the issue in [REPORT.md](REPORT.md) and exits nonzero. |
| `jwt` | 6,000 GETs, 64 clients | Valid JWT returns the demo profile; missing and malformed JWTs return 401 with a Bearer challenge. |
| `posts` | 800 CRUD transactions, 32 clients | Each transaction creates, reads, updates, deletes, then confirms 404 for its own post (4,000 HTTP requests total). |
| `database-failure` | One forced startup failure | An unavailable PostgreSQL endpoint fails startup within 10 seconds, never binds HTTP, and does not print the URL password. |

`smoke` uses 200/90/4/90/20 operations for hello/validation/oversized/JWT/posts.
`extended` uses 50,000/10,000/64/25,000/2,000 operations for those cases.
The runner starts and stops each application, reuses one HTTP connection per
worker, and exits nonzero for unexpected status, incorrect response content,
transport errors, or a failed startup. `database-failure` expects startup to
fail and checks that its test credential is redacted. The suite does not impose
arbitrary throughput thresholds. A run against `posts` writes and deletes rows,
so use an isolated PostgreSQL database.
The `oversized` case opens a new connection per request because the body-limit
rejection may close its connection. The separate `oversized-reuse` case tests
the persistent-connection behavior explicitly; it is excluded from the default
set while that issue is under triage.

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
python3 -m unittest discover -s benchmark -p 'test_*.py'
python3 benchmark/run.py --profile smoke --output /tmp/mads-smoke.json
python3 benchmark/run.py --profile stress --output /tmp/mads-stress.json
python3 benchmark/run.py --profile extended --output /tmp/mads-extended.json
python3 benchmark/run.py --case oversized-reuse --output /tmp/mads-reuse.json
```

To skip PostgreSQL, select only HTTP cases:

```sh
python3 benchmark/run.py --profile stress \
  --case hello --case validation --case oversized --case jwt --case database-failure
```

For a quick local check with existing debug binaries, add
`--binary-profile debug`. Use release binaries for reported performance numbers.
The JSON output includes the commit, OS, CPU count, profile, statuses, errors,
request rate, and latency percentiles. A `posts` operation is a five-request
transaction, whereas other operations are one HTTP request.

These results measure the whole loopback path, including Python's client cost,
MADS routing, application code, and optional PostgreSQL. They are useful for
reproducible regression checks and failure discovery, but are not an isolated
measurement of framework overhead or a guarantee for every deployment.
See [the measured run and findings](REPORT.md).
