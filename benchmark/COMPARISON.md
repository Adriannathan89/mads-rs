# MADS vs native Axum vs Go Fiber v2 — local benchmark

Measured 2026-09-25. The [raw result](results/2026-09-25-mads-axum-fiber-stress.json)
contains all five measured runs for every combination, including status counts,
client-observed p50/p95/p99 latency, and errors. All 420,000 measured HTTP
responses had the expected status, and content checks on valid-response paths
passed; there were zero measured transport or contract errors.

| Workload | Framework | Median responses/s | Range across 5 runs | Median p95 | Median p99 |
| --- | --- | ---: | ---: | ---: | ---: |
| Hello World | MADS 0.9.1 | 8,884 | 8,174–8,983 | 17.38 ms | 26.93 ms |
| Hello World | Axum 0.8.9 | 8,436 | 8,241–8,611 | 18.74 ms | 27.96 ms |
| Hello World | Fiber v2.52.15 | 8,432 | 8,269–8,574 | 18.29 ms | 27.26 ms |
| Posts CRUD | MADS 0.9.1 | 5,538 | 4,418–5,589 | 11.78 ms | 16.19 ms |
| Posts CRUD | Axum 0.8.9 | 5,851 | 4,111–5,928 | 11.10 ms | 15.04 ms |
| Posts CRUD | Fiber v2.52.15 | 5,668 | 5,410–5,782 | 11.20 ms | 15.28 ms |
| Login + protected JWT read | MADS 0.9.1 | 6,711 | 6,591–6,883 | 21.22 ms | 30.11 ms |
| Login + protected JWT read | Axum 0.8.9 | 6,861 | 6,771–6,921 | 21.07 ms | 29.42 ms |
| Login + protected JWT read | Fiber v2.52.15 | 6,482 | 5,856–6,709 | 22.16 ms | 30.64 ms |

The medians describe this host and client, not a universal ranking. Hello World
is nearly tied. MADS and Axum are also close for valid JWT traffic; Fiber's
median was lower on that workload in this run. The CRUD ranges are especially
wide for MADS and Axum, so their small median differences are not reliable
evidence of framework overhead. No framework error was found in the valid
request workloads.

## Method

- One local Linux host: AMD Ryzen 5 6600H (6 cores/12 threads), 14 GiB RAM;
  Rust 1.96.0, Go 1.22.2, Python 3.12.3. PostgreSQL 16.15 ran in a disposable
  Docker container. The source checkout was at `3efd8db` before benchmark
  files were added. MADS was built from this checkout's 0.9.1 source.
- Each framework ran alone on loopback HTTP/1.1, with release/optimized
  binaries. The same Python standard-library client drove all runs with
  persistent connections. Logger output went to a separate file for each
  process. Servers ran in MADS, Axum, Fiber order for each workload.
- Five measured runs followed contract checks and a warmup at the same
  concurrency. Hello World: 12,000 GETs/run at 64 clients. CRUD: 800
  create/read/update/delete/confirm-missing transactions/run at 32 clients,
  or 4,000 HTTP responses/run. Authentication: 6,000 valid login + protected
  profile pairs/run at 64 clients, or 12,000 responses/run. The table reports
  the median of each run's throughput and latency percentile, not a pooled
  percentile across runs.
- The three CRUD targets used the same `posts` schema in three separate
  databases within the same PostgreSQL instance. All used a maximum of ten
  database connections. Auth used HS256 JWTs, the same demo credentials, and
  per-success login/profile logging. JSON structure and important statuses
  were checked. Missing/malformed JWT checks require the MADS Passport 401
  status, `WWW-Authenticate: Bearer`, and unauthorized JSON envelope; Bearer
  scheme case is tested as well. Invalid fields, malformed JSON, unsupported
  content types, and missing posts are checked for status and shared error
  code outside the measured hot path; detailed validation/not-found payloads
  are not normalized across frameworks.

## Separate body-limit observation

A 3 MiB JSON upload to `/auth/login` was probed outside measured runs. MADS
and the native Axum target returned HTTP 413. With Fiber v2's 2 MiB body
limit, this Python client instead observed `ConnectionResetError` before
receiving an HTTP response. That is a real interoperability difference for
this client/target combination; the valid-request throughput result does not
test or erase it. The Axum benchmark target initially mapped this rejection
to 422; that adapter error was corrected before the final measurements.

## Interpretation and reproducibility

This is an **end-to-end application comparison**, not an isolated router
microbenchmark. The PostgreSQL paths use SeaORM/SQLx (MADS), SQLx (native
Axum), and Go `database/sql` with `lib/pq` (Fiber). The JWT libraries and
response encodings also differ. Python client throughput, local scheduling,
database cache/checkpoints, and logger I/O contribute to the numbers. No TLS,
HTTP/2, remote network, multi-host setup, or sustained soak was measured.
Five runs reduce but do not eliminate environmental noise; in particular,
several CRUD runs were slower than the rest without correctness failures.
The MADS example updates a post with a lookup followed by an update, while the
Axum and Fiber targets issue a single `UPDATE ... RETURNING`; the CRUD results
therefore do not use identical database round-trip counts.

MADS's existing published-package example lockfiles currently fail a direct
build with a registry checksum mismatch for `mads-common-macros 0.9.1`.
`benchmark/tool/build_mads.py` therefore stages the unchanged examples in a
temporary directory, patches MADS dependencies to this local checkout, and
uses the pinned local-source locks in `benchmark/targets/mads/locks/`. This
does not claim to measure the published crate archive. The Axum `Cargo.lock`
and Fiber `go.sum` are included separately. See [benchmark setup](README.md#mads-vs-native-axum-vs-go-fiber-v2)
for build and rerun commands.
