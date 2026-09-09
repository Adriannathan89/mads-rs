#!/usr/bin/env bash

# Verifies the exact tracked source payload of every distributable MADS crate.
# Cargo adds the root manifest, lockfile, and VCS record to each archive; every
# other member must come from the corresponding tracked crate directory.

set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temporary_directory="$(mktemp -d)"
trap 'rm -rf "$temporary_directory"' EXIT

packages=(
  mads-core-macros
  mads-common-macros
  mads-core
  mads-extra
  mads-common
  mads
  mads-cli
)

fail() {
  printf 'package-content policy failed: %s\n' "$*" >&2
  exit 1
}

require_file() {
  local package="$1"
  local actual="$2"
  local path="$3"

  if ! grep -Fqx "$path" "$actual"; then
    fail "$package archive is missing required file: $path"
  fi
}

require_prefix() {
  local package="$1"
  local actual="$2"
  local prefix="$3"

  if ! grep -q "^$prefix" "$actual"; then
    fail "$package archive is missing required fixture group: $prefix"
  fi
}

reject_generated_artifacts() {
  local package="$1"
  local actual="$2"
  local path

  while IFS= read -r path; do
    case "$path" in
      Cargo.lock)
        # Cargo generates this root archive member for every packaged crate.
        ;;
      Cargo.lock/* | */Cargo.lock | */Cargo.lock/*)
        fail "$package archive contains a non-generated Cargo.lock: $path"
        ;;
      .git | .git/* | */.git | */.git/*)
        fail "$package archive contains Git metadata: $path"
        ;;
      target | target/* | */target | */target/*)
        fail "$package archive contains a build target artifact: $path"
        ;;
      .mads-new-* | .mads-staging-* | */.mads-new-* | */.mads-new-*/* | */.mads-staging-* | */.mads-staging-*/*)
        fail "$package archive contains a scaffold staging artifact: $path"
        ;;
      tests/consumers/*)
        if [[ "$path" != "tests/consumers/.gitignore" ]]; then
          fail "$package archive contains a generated consumer artifact: $path"
        fi
        ;;
    esac
  done < "$actual"
}

tracked_package_payload() {
  local package_root="$1"
  local source
  local relative
  local nested_project
  local -a nested_projects=()

  while IFS= read -r source; do
    relative="${source#"$package_root"/}"
    if [[ "$relative" != "Cargo.toml" ]]; then
      nested_projects+=("${relative%/Cargo.toml}")
    fi
  done < <(git ls-files -- "$package_root" | grep '/Cargo.toml$' || true)

  while IFS= read -r source; do
    relative="${source#"$package_root"/}"
    for nested_project in "${nested_projects[@]}"; do
      if [[ "$relative" == "$nested_project/"* ]]; then
        continue 2
      fi
    done

    if [[ "$relative" == "Cargo.toml" ]]; then
      printf '%s\n' Cargo.toml.orig
    else
      printf '%s\n' "$relative"
    fi
  done < <(git ls-files -- "$package_root")
}

require_package_specific_content() {
  local package="$1"
  local actual="$2"

  require_file "$package" "$actual" "Cargo.toml"
  require_file "$package" "$actual" "Cargo.toml.orig"
  require_file "$package" "$actual" "Cargo.lock"
  require_file "$package" "$actual" ".cargo_vcs_info.json"
  require_file "$package" "$actual" "README.md"
  require_file "$package" "$actual" "src/lib.rs"

  case "$package" in
    mads)
      for prefix in tests/ui/ tests/ui-configuration/ tests/ui-database-http/ tests/ui-input/ tests/ui-passport/; do
        require_prefix "$package" "$actual" "$prefix"
      done
      ;;
    mads-cli)
      require_file "$package" "$actual" "src/main.rs"
      for template in \
        Cargo.toml.txt \
        app_mod.rs.txt \
        controller.rs.txt \
        mads.toml.txt \
        main.rs.txt \
        routes.rs.txt \
        service.rs.txt; do
        require_file "$package" "$actual" "src/scaffold/templates/$template"
      done
      for fixture in \
        tests/fixtures/dev/Cargo.toml.template \
        tests/fixtures/dev/build.rs \
        tests/fixtures/dev/src/main.rs \
        tests/fixtures/inspection/.gitignore \
        tests/fixtures/migrations/202608220201_create_cli_items/down.sql \
        tests/fixtures/migrations/202608220201_create_cli_items/up.sql \
        tests/fixtures/run/.gitignore \
        tests/fixtures/schema/expected/create_users_down.sql \
        tests/fixtures/schema/expected/create_users_up.sql \
        tests/fixtures/schema/single/src/schema.rs \
        tests/fixtures/schema/split/src/schema/comment.rs \
        tests/fixtures/schema/split/src/schema/user.rs; do
        require_file "$package" "$actual" "$fixture"
      done
      ;;
  esac
}

cd "$repository_root"

for package in "${packages[@]}"; do
  package_root="crates/$package"
  actual="$temporary_directory/$package.actual"
  expected="$temporary_directory/$package.expected"

  cargo package --locked --allow-dirty --list -p "$package" | LC_ALL=C sort -u > "$actual"
  {
    printf '%s\n' .cargo_vcs_info.json Cargo.lock Cargo.toml Cargo.toml.orig README.md
    tracked_package_payload "$package_root"
  } | LC_ALL=C sort -u > "$expected"

  reject_generated_artifacts "$package" "$actual"
  require_package_specific_content "$package" "$actual"

  if ! diff -u "$expected" "$actual"; then
    fail "$package archive contents differ from the approved tracked payload"
  fi
done

printf 'verified package contents for %s crates\n' "${#packages[@]}"
