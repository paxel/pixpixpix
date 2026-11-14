#!/usr/bin/env bash
# The coverage ratchet: line coverage may only go up.
#
# Runs cargo-tarpaulin and compares the result against the floor in .coverage-floor at
# the repo root. Below the floor the gate fails: some change shipped without tests.
# Above it, the floor is raised to the new value (commit the file and the badge), which
# is what makes the ratchet a ratchet; with --check the floor is only verified, never
# rewritten, which is what CI runs.
#
# The tests take another branch when a native library pixpixpix loads at runtime is
# missing, so the number depends on the machine. CI measures with libturbojpeg,
# libheif (with its decoder plugins) and libmpv installed; raise the floor only from a
# machine that has all of them too.
#
# Usage:
#   scripts/coverage.sh          run, fail below the floor, raise it when above
#   scripts/coverage.sh --check  run, fail below the floor, never write (CI)
set -euo pipefail

# awk's printf writes the decimal separator of the session's locale; a floor written as
# "87,30" is then misread as 87 by every C-locale awk, CI's included.
export LC_ALL=C

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
FLOOR_FILE="$ROOT/.coverage-floor"

check_only=false
[ "${1:-}" = "--check" ] && check_only=true

floor=$(tr -d "[:space:]" < "$FLOOR_FILE")

# The README's coverage badge reads this file from the default branch, via shields.io's
# endpoint API; no coverage service involved. Kept in step with the floor, which the
# ratchet keeps in step with reality.
write_badge() {
  color=$(awk -v c="$1" 'BEGIN { print (c >= 90) ? "brightgreen" : (c >= 75) ? "green" : (c >= 60) ? "yellow" : "red" }')
  mkdir -p "$ROOT/.github/badges"
  printf '{"schemaVersion": 1, "label": "coverage", "message": "%s%%", "color": "%s"}\n' \
    "$1" "$color" > "$ROOT/.github/badges/coverage.json"
}

# The llvm engine, not the default ptrace one: only llvm collects coverage from the
# binaries the tests spawn, via the inherited profile environment. The LCOV file is for
# the SonarQube job in quality.yml.
#
# The helper processes the tests start end once their stdin closes, and write
# their profile on the way out; without the delay tarpaulin merges a profile
# still being written and fails on it ("parser failure: Nom(Eof)").
#
# Tarpaulin's last line reads like "74.32% coverage, 1234/1660 lines covered".
coverage=$(cd "$ROOT" && cargo tarpaulin --engine llvm --locked --skip-clean \
  --post-test-delay 2 --out Stdout --out Lcov 2>&1 | tee /dev/stderr | grep -oE '^[0-9]+\.[0-9]+% coverage' | cut -d% -f1)

if [ -z "$coverage" ]; then
  echo "coverage: tarpaulin produced no percentage" >&2
  exit 1
fi

below=$(awk -v c="$coverage" -v f="$floor" 'BEGIN { print (c < f) ? 1 : 0 }')
if [ "$below" = 1 ]; then
  echo "coverage: $coverage% is below the floor of $floor%; the ratchet only goes up" >&2
  exit 1
fi

# The floor trails the measurement by a tenth of a point, as a guard against
# run-to-run noise.
raised=$(awk -v c="$coverage" 'BEGIN { printf "%.2f", c - 0.10 }')
above=$(awk -v r="$raised" -v f="$floor" 'BEGIN { print (r > f) ? 1 : 0 }')
if [ "$above" = 1 ] && [ "$check_only" = false ]; then
  echo "$raised" > "$FLOOR_FILE"
  write_badge "$raised"
  echo "coverage: $coverage%; floor raised from $floor% to $raised%, commit .coverage-floor and the badge"
else
  echo "coverage: $coverage% (floor $floor%)"
fi
