#!/usr/bin/env python3
"""Build unchanged MADS examples against this checkout without editing their locks."""

from __future__ import annotations

import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

from run import ROOT


EXAMPLES = ("hello-world", "posts-crud", "protected-route")


def stage_example(source: Path, destination: Path, lockfile: Path | None = None) -> Path:
    shutil.copytree(source, destination, ignore=shutil.ignore_patterns("Cargo.lock", "target", ".env", ".env.*"))
    if lockfile is not None:
        shutil.copy2(lockfile, destination / "Cargo.lock")
    return destination


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--example", action="append", choices=EXAMPLES)
    parser.add_argument("--offline", action="store_true", help="use cached Cargo dependencies only")
    args = parser.parse_args()
    target = ROOT / "benchmark" / "targets" / "mads" / "target"
    locks = ROOT / "benchmark" / "targets" / "mads" / "locks"
    locks.mkdir(parents=True, exist_ok=True)
    examples = args.example or EXAMPLES
    with tempfile.TemporaryDirectory(prefix="mads-bench-build-") as directory:
        for name in examples:
            lockfile = locks / f"{name}.lock"
            staged = stage_example(
                ROOT / "example" / name, Path(directory) / name,
                lockfile if lockfile.is_file() else None,
            )
            command = [
                "cargo",
                "--config", f'patch.crates-io.mads.path="{ROOT / "crates" / "mads"}"',
            ]
            if name == "posts-crud":
                command.extend([
                    "--config", f'patch.crates-io.mads-persistence.path="{ROOT / "crates" / "mads-persistence"}"',
                ])
            command.extend([
                "build", "--release", "--manifest-path", str(staged / "Cargo.toml"),
                "--target-dir", str(target),
            ])
            if args.offline:
                command.append("--offline")
            if lockfile.is_file():
                command.append("--locked")
            subprocess.run(command, check=True)
            if not lockfile.is_file():
                shutil.copy2(staged / "Cargo.lock", lockfile)


if __name__ == "__main__":
    main()
