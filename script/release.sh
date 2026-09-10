#!/usr/bin/env bash
set -euo pipefail

mode="stable"
if [[ "${1:-}" == "--beta" ]]; then
  mode="beta"
  shift
fi

if [[ "$#" -ne 1 || ! "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  if [[ "$mode" == "beta" ]]; then
    echo "Usage: script/release-beta.sh X.Y.Z" >&2
  else
    echo "Usage: script/release.sh X.Y.Z" >&2
  fi
  exit 2
fi

for command in git python3 cargo; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Required command is unavailable: $command" >&2
    exit 1
  fi
done

repository_root="$(git rev-parse --show-toplevel 2>/dev/null)" || {
  echo "Run this script inside the MADS.rs Git repository." >&2
  exit 1
}
if [[ ! -f "$repository_root/Cargo.toml" || ! -d "$repository_root/crates" ]]; then
  echo "The Git repository is not a MADS.rs workspace." >&2
  exit 1
fi

target_version="$1"
new_version="$({
  python3 - "$repository_root" "$mode" "$target_version" <<'PY'
import os
import re
import sys
import tomllib
from pathlib import Path

root = Path(sys.argv[1])
mode = sys.argv[2]
base = sys.argv[3]
packages = (
    "mads-core-macros",
    "mads-common-macros",
    "mads-core",
    "mads-extra",
    "mads-common",
    "mads",
    "mads-cli",
)

root_manifest = root / "Cargo.toml"
original_root = root_manifest.read_text(encoding="utf-8")
workspace_match = re.search(
    r"(?ms)^\[workspace\.package\]\s*\n(?P<body>.*?)(?=^\[|\Z)", original_root
)
if workspace_match is None:
    raise SystemExit("Cargo.toml does not contain [workspace.package].")
body = workspace_match.group("body")
version_match = re.search(r'(?m)^version\s*=\s*"([^"]+)"\s*$', body)
if version_match is None:
    raise SystemExit("[workspace.package] does not contain one version.")
current = version_match.group(1)

if mode == "beta":
    matching_beta = re.fullmatch(re.escape(base) + r"-beta\.(\d+)", current)
    next_number = int(matching_beta.group(1)) + 1 if matching_beta else 1
    target = f"{base}-beta.{next_number}"
else:
    target = base

changes: dict[Path, str] = {}
body_start = workspace_match.start("body")
version_start = body_start + version_match.start(1)
version_end = body_start + version_match.end(1)
changes[root_manifest] = original_root[:version_start] + target + original_root[version_end:]

pin_pattern = re.compile(r'(\bversion\s*=\s*")=[^"]+("\s*[,}])')
pin_count = 0
for manifest in sorted((root / "crates").glob("*/Cargo.toml")):
    original = manifest.read_text(encoding="utf-8")
    output_lines = []
    for line in original.splitlines(keepends=True):
        if re.match(r"\s*mads(?:-[a-z0-9-]+)?\s*=", line) and "path" in line:
            updated, count = pin_pattern.subn(rf'\g<1>={target}\g<2>', line)
            if count != 1:
                raise SystemExit(f"Expected one exact internal version pin in {manifest}: {line.strip()}")
            line = updated
            pin_count += 1
        output_lines.append(line)
    changes[manifest] = "".join(output_lines)

if pin_count == 0:
    raise SystemExit("No internal MADS dependency pins were found.")

root_lockfile = root / "Cargo.lock"
for lockfile in sorted(root.rglob("Cargo.lock")):
    original_lock = lockfile.read_text(encoding="utf-8")
    updated_lock = original_lock
    for package in packages:
        pattern = re.compile(
            rf'(?m)(^\[\[package\]\]\nname = "{re.escape(package)}"\nversion = ")[^"]+("$)'
        )
        updated_lock, count = pattern.subn(rf'\g<1>{target}\g<2>', updated_lock)
        if count > 1 or (lockfile == root_lockfile and count != 1):
            raise SystemExit(
                f"{lockfile} must contain exactly one package record for {package}."
            )
    changes[lockfile] = updated_lock

for path, contents in changes.items():
    tomllib.loads(contents)

for path, contents in changes.items():
    if path.read_text(encoding="utf-8") == contents:
        continue
    temporary = path.with_name(f".{path.name}.mads-release.tmp")
    temporary.write_text(contents, encoding="utf-8")
    os.replace(temporary, path)

print(target)
PY
})"

cargo metadata --locked --format-version 1 --no-deps --manifest-path "$repository_root/Cargo.toml" >/dev/null
cargo check --locked --workspace --all-targets --manifest-path "$repository_root/Cargo.toml"

echo "Prepared MADS.rs workspace version $new_version"
echo "Review Cargo.toml, crates/*/Cargo.toml, and Cargo.lock before committing."
