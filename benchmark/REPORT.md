# Benchmark findings — 2026-09-24

Hotfix follow-up (2026-09-25): the `mads-common` response for oversized JSON
now includes `Connection: close`. With the protected-route example built against
the local hotfix source, `oversized-reuse` returned eight 413 responses across
four clients with zero transport errors. The 0.9.0 outcome below remains a
historical result for the published 0.9.0 build.

## 0.9.1 edge-case follow-up — 2026-09-25

Three new cases ran against debug example binaries built from local MADS 0.9.1
source on loopback HTTP/1.1. A combined HTTP smoke pass with the existing
validation, JWT, and oversized-body cases also passed. Stress passed for each
new case. The extended profile recorded:

| Case | Clients | Attempts and checked responses | p95 | p99 | Unexpected errors |
| --- | ---: | --- | ---: | ---: | ---: |
| Connection churn | 64 | 5,000 fresh connections; 5,000 HTTP 200 | 20.633 ms | 26.780 ms | 0 |
| 2 MiB body boundary | 16 | 192 uploads; 128 HTTP 422, 64 HTTP 413; 192 recovery HTTP 401 | 31.674 ms | 40.835 ms | 0 |
| Aborted upload | 64 | 512 partial uploads closed by clients; 512 login and 512 protected GET responses, all HTTP 200 | 11.636 ms | 15.675 ms | 0 |

The boundary case checks exact byte lengths of 2 MiB − 1, 2 MiB, and
2 MiB + 1. Each 413 advertised `Connection: close`. The aborted-upload case
checks successful login and JWT-protected reads after clients close incomplete
request bodies; it does not expect a response on the abandoned sockets. These
results cover the listed workloads on one host. They do not establish behavior
under longer runs, TLS, or database faults during active requests.

The new `database-connect-timeout` case ran against the same local debug 0.9.1
source build of `posts-crud`. A loopback TCP server accepted the PostgreSQL
connection and deliberately withheld its handshake response. With a configured
2-second connect timeout, the application failed startup in 2.012 seconds with
the MADS persistence connection diagnostic. The HTTP listener did not bind,
the test credential did not appear in the startup log, and the benchmark
reported zero unexpected errors. This is distinct from `database-failure`,
which uses a closed port and can fail immediately. Neither startup-only case
tests faults during active requests; the separate recovery cases below do.

## 0.9.1 database recovery follow-up — 2026-09-25

Two additional cases ran separately against the local debug 0.9.1 `posts-crud`
binary and a disposable PostgreSQL 16 database. Both kept the same HTTP process
alive, served a non-database 404 during the fault, and returned HTTP 200 from
`GET /posts` after recovery, with zero unexpected benchmark errors:

| Fault | Database responses during fault | Timeout | Recovery | Unexpected errors |
| --- | --- | ---: | --- | ---: |
| PostgreSQL statement timeout under a table lock | One HTTP 500 (`internal`) | 2,001.482 ms | HTTP 200 after lock release | 0 |
| Established TCP reply stalled by a loopback proxy; pool acquire timeout | Two HTTP 500 (`internal`) | 2,002.033 and 2,002.725 ms | HTTP 200 after proxy resumes | 0 |

The query case sets PostgreSQL `statement_timeout=2000` through the example's
database URL, only for this run. In the TCP case, SQLx checks an idle connection
with a ping before handing it to a request. The proxy withheld that ping reply;
the configured 2-second `acquire_timeout` therefore affected the first request
as well as the second request waiting for the one-slot pool. This explains the
two 500 responses; they were expected fault responses, not crashes. The proxy
accepted one application TCP connection at startup and still only one after
recovery, so this run did not observe a new database TCP connection. This does
not establish behavior for a socket-read stall after a query has already
acquired its connection, or for prolonged/distributed outages.

## 0.9.0 outcome

The final `extended` run completed 95,064 HTTP responses across routing,
validation, 3 MiB body-limit rejection, Passport JWT, and PostgreSQL CRUD. All
had the expected status and body, with zero transport errors. Database-down
startup failed in 2.02 seconds with a MADS persistence connection diagnostic,
without opening the HTTP listener or printing the test password. Repeated
`stress` and earlier extended runs also passed their normal workloads.

One separate diagnostic exposed an HTTP/1.1 connection-reuse issue after a
413 response. This means the result does **not** support an unconditional claim
that the backend is seamless for every client behavior. The issue is described
below; the normal workload results remain valid within their stated scope.

## Measured load

The final `extended` run used optimized example binaries:

| Workload | Clients | Responses | Responses/s | p95 | p99 | Unexpected errors |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Hello World | 64 | 50,000 | 5,421 | 31.0 ms | 46.3 ms | 0 |
| Input validation | 32 | 10,000 | 4,222 | 15.4 ms | 20.2 ms | 0 |
| Oversized JSON, fresh connection | 8 | 64 | 1,174 | 8.1 ms | 9.2 ms | 0 |
| Passport JWT | 64 | 25,000 | 6,246 | 25.5 ms | 37.6 ms | 0 |
| PostgreSQL posts CRUD | 32 | 10,000 | 4,548 | 14.3 ms | 21.2 ms | 0 |

For CRUD, 2,000 independent five-request transactions created, read, updated,
deleted, and confirmed removal of their own post. The posts table had zero rows
after the run. The expected 401, 404, 413, and 422 responses are correct
behavior, not unexpected errors. The final `stress` run also passed 25,032
responses with zero errors. The full [extended result](results/2026-09-24-extended.json)
records status counts and p50/p95/p99.

## Observed issue: HTTP connection reuse after 413

Reproduction: run `python3 benchmark/run.py --case oversized-reuse` with the
release protected-route example built. Four clients each send two 3 MiB POSTs
on one persistent HTTP/1.1 connection. Each first request receives the
expected JSON 413, but the response omits `Connection: close`. Each second
request then raises `BrokenPipeError` before receiving a response: four 413s
and four transport failures. The [diagnostic result](results/2026-09-24-oversized-reuse.json)
records this failing check. By contrast, the same oversized requests on fresh
connections returned 413 for all 64 attempts in the extended run.

The immediate cause is reuse of a connection that the server closed after the
body-limit rejection. MADS's
[`fixed_error_response`](../crates/mads-common/src/validation/rejection.rs)
constructs the 413 JSON response without a close header; this local source file
matches the published 0.9.0 source. The exact ownership
of the connection closure between MADS and its underlying Hyper/Axum transport
has not been isolated. [RFC 9112](https://www.rfc-editor.org/rfc/rfc9112.html#section-9.3)
allows a server to close when it does not consume the entire request body, while
[RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html#section-10.1.1)
recommends signaling whether that connection will close. This is an
interoperability concern rather than evidence of data corruption or a crash.

A client can avoid the failure by opening a new connection after an oversized
request is rejected. The framework-side follow-up is to confirm the transport
closure policy and, if it closes after 413, advertise that with
`Connection: close`. No framework fix was made as part of this benchmark task.

The first smoke run also used a wrong benchmark expectation: it expected 400
for malformed JSON. Existing MADS tests and the live response establish that
`ValidatedJson` returns 422. That false alarm was corrected in the harness.

## Environment and method

- Commit: `81af438b67e150568e0312a39e117745c4aa1be9`; the example
  applications consume published MADS and `mads-persistence` 0.9.0 packages.
- Host: AMD Ryzen 5 6600H, 12 logical CPUs, 14 GiB RAM, Linux x86_64.
- Toolchain: Rust 1.96.0; Python 3.12.3 standard-library HTTP client.
- Servers: Cargo `--release`, loopback HTTP/1.1. Ordinary cases reuse one
  connection per worker; oversized rejection uses a fresh connection; the
  separate diagnostic deliberately reuses the rejected connection.
- Database: PostgreSQL 16 in a disposable local cluster with the example
  posts schema. The test cluster was stopped and removed afterward.
- Timing: client-observed wall-clock latency includes Python, HTTP, application
  code, and PostgreSQL where applicable. Percentiles use nearest-rank order
  statistics. Throughput includes worker startup; there was no warm-up phase.

## Limits of the conclusion

These local runs show reliable behavior under the specified concurrent and
large-input workloads, with one reproducible persistent-connection rough edge.
They do not establish universal robustness: each run lasted seconds, not
hours; there was one host and one database; and TLS, distributed clients,
mid-request database outages, memory trends, and production traffic mixes were
not tested. Python may limit the observed request rate, so the figures do not
isolate MADS framework overhead. Run [the benchmark suite](README.md) again
under the same conditions to compare future releases.
