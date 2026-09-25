# Update vulnerable `time` dependency in MADS 0.9.1 lockfile

Status: resolved in the current workspace lockfile. `time` is now 0.3.47, and a follow-up audit no longer reports RUSTSEC-2026-0009. The separate `rsa` advisory remains. The details below record the original finding.

## Finding

The initial `cargo audit` reported `RUSTSEC-2026-0009` for `time 0.3.36` in the MADS workspace `Cargo.lock`. The advisory describes stack exhaustion when attacker-controlled RFC 2822 date input reaches affected `time` parsing functions; `time >=0.3.47` is patched. This was a confirmed dependency-management issue in the framework workspace, **not** a demonstrated exploit through a MADS endpoint.

Dependency paths from `cargo tree --workspace --all-features -i time`:

```text
time 0.3.36
├── cookie 0.18.1 -> mads-common
└── simple_asn1 0.6.3 -> jsonwebtoken 10.3.0 -> mads-common
```

## Origin and reachability check

This version is selected transitively by `cookie` and `simple_asn1`; MADS does not call the affected `time` parser directly. `cookie 0.18.1` parses its `Expires` attribute using four fixed HTTP-date format descriptions. `simple_asn1 0.6.3` parses ASN.1 UTCTime/GeneralizedTime with fixed numeric formats. Neither observed call site uses the RFC 2822 format with deprecated recursive features required by the advisory. MADS's request `CookieJar` splits Cookie header pairs before calling `Cookie::parse_encoded`, so an HTTP `Cookie` header is not a demonstrated route to the `Expires` parser either.

Thus the **workspace lockfile advisory is confirmed**, but an attacker-triggerable stack exhaustion through the current MADS APIs is **not confirmed**. Both upstream crates accept `time` 0.3 generally; MADS does not pin `time` to 0.3.36 in its manifests. Library consumers resolve their own lockfiles, so this exact version is not forced on every downstream application. Keep this as a dependency-hygiene issue, not a proven MADS request-handling vulnerability.

## Reproduce

```sh
cargo audit
cargo tree --workspace --all-features -i time
```

Expected: no known-vulnerable `time` version in the supported dependency graph. Actual: `cargo audit` exits 1 and reports RUSTSEC-2026-0009. The local audit also reports the separate, currently unpatched `rsa` RUSTSEC-2023-0071 advisory; do not conflate the two.

## Proposed follow-up

Update the lockfile/dependency constraints to select `time >=0.3.47` and run all-feature workspace tests and `cargo audit`. Retain the lockfile remediation as defense in depth; the inspected call sites did not establish a MADS-facing RFC 2822 parsing path. Assess `rsa` separately because RustSec lists no patched release.

Sources: [RUSTSEC-2026-0009](https://rustsec.org/advisories/RUSTSEC-2026-0009.html), [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html).
