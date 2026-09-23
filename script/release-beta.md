# Preparing a beta release

Use this script to prepare the next MADS.rs beta version without changing
documentation or publishing anything.

## Prerequisites

- Run it from anywhere inside the MADS.rs Git repository.
- Install Bash, Python 3.11 or newer, Cargo, and Git.
- Update README and `CHANGELOG.md` separately when release notes change.

## Usage

```bash
script/release-beta.sh 0.9.0
```

If the current workspace version is `0.9.0-beta.1`, the result is
`0.9.0-beta.2`. If the current version has another base, the result starts at
`0.9.0-beta.1`.

The script updates `[workspace.package].version`, every exact internal MADS
dependency pin, and all eight workspace package records in `Cargo.lock`. It
then runs locked Cargo metadata and workspace checks.

All packages, including `mads-cli`, inherit the workspace version. The script
rejects a CLI manifest that pins its own version.

It does not edit README or changelog content, commit, tag, push, or publish.
Review and commit the Cargo changes, update the `## [X.Y.Z-beta.N]` changelog
section, then push the commit to `beta` to use the existing beta publication
workflow. Configure `CRATES_IO_TOKEN` in the GitHub `beta` environment.
