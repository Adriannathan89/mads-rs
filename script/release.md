# Preparing a stable release

Use this script to change only Cargo workspace/package version metadata for a
stable MADS.rs release.

## Prerequisites

- Run it from anywhere inside the MADS.rs Git repository.
- Install Bash, Python 3.11 or newer, Cargo, and Git.
- Ensure the target version has a completed `CHANGELOG.md` section named
  `## [X.Y.Z]` and that README/release documentation is current.

## Usage

```bash
script/release.sh 0.8.0
```

To advance framework crates while retaining the current `mads-cli` package
version, use:

```bash
script/release.sh --keep-cli-version 0.8.1
```

The script sets the workspace version to exactly `0.8.0`, updates every exact
internal MADS crate dependency to `=0.8.0`, updates matching MADS package
records in every `Cargo.lock`, then runs locked Cargo metadata and workspace
checks.

`--keep-cli-version` preserves the current CLI package version. When the CLI
inherits the workspace version, the script first records that current version
as an explicit `mads-cli` package version; its dependencies still pin the new
framework version.

It does not edit README or changelog content, commit, tag, push, or publish.
Review the Cargo changes and complete the release documentation manually.

After committing the prepared stable version, push it to `main`. The stable
publication workflow runs all release gates, publishes missing crate versions
to crates.io in dependency order, and creates the `v0.8.0` Git tag and stable
GitHub Release. Configure `CRATES_IO_TOKEN` in the GitHub `stable` environment.
