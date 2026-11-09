#!/bin/sh
# roll-flow gate: the version in Cargo.toml must have a CHANGELOG.md heading
# (e.g. "## 0.1.3"), so every version bump ships with its changelog entry.
set -eu

version=$(awk '
  /^\[/ { in_pkg = ($0 == "[package]") }
  in_pkg && /^version[[:space:]]*=/ { gsub(/.*=[[:space:]]*"|".*/, ""); print; exit }
' Cargo.toml)

if [ -z "$version" ]; then
  echo "check-changelog: no version found in Cargo.toml [package]" >&2
  exit 1
fi

escaped=$(printf '%s' "$version" | sed 's/\./\\./g')
if grep -Eq "^[[:space:]]*#+.*(^|[^0-9.])v?${escaped}([^0-9A-Za-z.-]|\$)" CHANGELOG.md; then
  echo "check-changelog: CHANGELOG.md has an entry for $version"
else
  echo "check-changelog: CHANGELOG.md has no entry for $version; add a \"## $version\" section" >&2
  exit 1
fi
