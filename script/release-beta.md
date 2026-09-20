# Preparing a beta release

Use this script to prepare the next MADS.rs beta version without changing
documentation or publishing anything.

## Prerequisites

- Run it from anywhere inside the MADS.rs Git repository.
- Install Bash, Python 3.11 or newer, Cargo, and Git.
- Update README and `CHANGELOG.md` separately when release notes change.

## Usage

```bash
script/release-beta.sh 0.7.0
```

To keep the current `mads-cli` package version while advancing the framework,
pass `--keep-cli-version`:

```bash
script/release-beta.sh --keep-cli-version 0.8.1
```

If the current workspace version is `0.7.0-beta.1`, the result is
`0.7.0-beta.2`. If the current version has another base, the result starts at
`0.7.0-beta.1`.

The script updates `[workspace.package].version`, every exact internal MADS
dependency pin, and the seven workspace package records in `Cargo.lock`. It
then runs locked Cargo metadata and workspace checks.

With `--keep-cli-version`, the current CLI version is retained (and made
explicit if it previously inherited the workspace version), while its internal
framework dependency pins advance to the beta version.

It does not edit README or changelog content, commit, tag, push, or publish.
Review and commit the Cargo changes, update the `## [X.Y.Z-beta.N]` changelog
section, then push the commit to `beta` to use the existing beta publication
workflow. Configure `CRATES_IO_TOKEN` in the GitHub `beta` environment.
