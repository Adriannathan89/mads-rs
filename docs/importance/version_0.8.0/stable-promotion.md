# MADS v0.8.0 Stable Promotion

Stable `0.8.0` promotes the complete `0.8.0-beta.1` feature set. There are
**no new features** in stable promotion. Permitted changes are bug fixes,
tests, documentation corrections, and release verification only.

## Promotion checklist

- [ ] Confirm the workspace version is the intended stable `0.8.0` only after
  the separate release request.
- [ ] Confirm every v0.8 release-scope row still has focused and integration
  evidence.
- [ ] Run formatting and deny-warning Clippy for the complete workspace.
- [ ] Run locked all-feature workspace and rustdoc tests.
- [ ] Run the complete gate on Rust 1.85 and the current stable toolchain.
- [ ] Confirm line coverage remains at or above 85%.
- [ ] Compile, inspect, run, and request `GET /` from a freshly generated
  seven-file application.
- [ ] Run parser, JSON, path-normalization, scaffold, and portable process
  tests on Linux, macOS, and Windows.
- [ ] Run all real PostgreSQL tests against PostgreSQL 16 on Linux.
- [ ] Package all seven crates and inspect their file lists and exact internal
  dependency pins.
- [ ] Scan errors, reports, JSON, responses, formatting, configuration,
  database context, and CLI channels for secret sentinels.
- [ ] Confirm native `Json` remains native, typed parsing remains explicit via
  `Config::parse`, and database delivery remains opt-in through `.into_http()`.
- [ ] Confirm `run` and `dev` still stream raw output and reject `--format`.
- [ ] Confirm schema-version-1 changes, if any, are additive only.
- [ ] Review release notes and documentation for accidental future or stable-
  only feature claims.

This checklist intentionally remains open during beta preparation. Publishing,
tagging, pushing, and creating a GitHub release require a separate explicit
release operation.
