#!/usr/bin/env sh
# Fail on inline `#[cfg(test)] mod ... { ... }` blocks in daemon source files.
#
# `.agents/testing.md` ("Test file layout — STRICT RULE") mandates that Rust
# tests live in separate files under a `tests/` directory (or a `tests.rs`
# sibling), wired via `#[cfg(test)] mod tests;`. This gate keeps new inline
# test modules from landing after the issue #847 migration.
#
# A `#[cfg(test)] mod tests;` *declaration* (no `{` body) is the sanctioned
# wiring and does not match. Attributes between `#[cfg(test)]` and `mod`
# (e.g. `#[allow(...)]`) are tolerated by the pattern so they can't be used
# to dodge the check.
set -eu

cd "$(dirname "$0")/.."

pattern='#\[cfg\(test\)\]\s*(#\[[^]]*\]\s*)*(pub(\([^)]*\))?\s+)?mod\s+\w+\s*\{'

status=0
for f in $(find crates -path '*/src/*' -name '*.rs' ! -path '*/tests/*' ! -name 'tests.rs' | sort); do
    if grep -Pzoq "$pattern" "$f"; then
        echo "error: inline #[cfg(test)] mod block in $f" >&2
        status=1
    fi
done

if [ "$status" -ne 0 ]; then
    echo >&2
    echo "Tests must live in separate files under a tests/ directory —" >&2
    echo "see .agents/testing.md, 'Test file layout — STRICT RULE'." >&2
fi
exit "$status"
