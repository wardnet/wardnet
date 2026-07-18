#!/usr/bin/env sh
# Fail on inline test code in daemon source files.
#
# `.agents/testing.md` ("Test file layout — STRICT RULE") mandates that Rust
# tests live in separate files under a `tests/` directory, wired via
# `#[cfg(test)] mod tests;`. This gate keeps new inline test code from
# landing after the issue #847 migration. It flags, in any `crates/*/src`
# file outside a `tests/` directory:
#
#   * bare `#[test]` / `#[tokio::test]` functions, whatever wrapper (or no
#     wrapper) they sit in — this is what makes a module a test module, so
#     cfg spellings like `#[cfg(all(test, unix))]` can't dodge the check;
#   * `#[cfg(...test...)] mod ... { ... }` blocks (helper/mock-only modules
#     with no #[test] fns yet), tolerating attributes and `//` comments
#     between the cfg attribute and `mod`. `#[cfg(not(test))]` and quoted
#     feature names like "test-utils" are not matched, and the sanctioned
#     `#[cfg(test)] mod tests;` declaration (no body) never is.
#
# Files named `tests.rs` are exempt: a handful of `<mod>/tests.rs` siblings
# (e.g. push/tests.rs) predate the tests/-directory layout and are already
# separate test files; the exemption grandfathers those, it is not a layout
# testing.md endorses for new code.
set -eu

cd "$(dirname "$0")/.."

# GNU grep with PCRE is required (Linux dev hosts, CI runners, and the
# rust image used by check-daemon-container all have it). Fail closed
# rather than letting an unsupported grep report a vacuous pass.
if ! printf 'x' | grep -Pzq 'x' 2>/dev/null; then
    echo "error: check-inline-tests.sh needs GNU grep with -P and -z support" >&2
    exit 2
fi

# Alternation of the two shapes (grep -P accepts only a single pattern):
# a bare #[test]/#[tokio::test] attribute at the start of a line, or a
# cfg-test attribute followed by an inline `mod ... {` body.
inline_test_fn='(?m)^[ \t]*#\[(?:tokio::)?test[\](]'
inline_test_mod='#\[cfg\([^\]"]*(?<!not\()\btest\b[^\]"]*\)\][ \t]*(?:(?://[^\n]*)?\n[ \t]*)*(?:#\[[^\]]*\][ \t]*(?:(?://[^\n]*)?\n[ \t]*)*)*(?:pub(?:\([^)]*\))?[ \t]+)?mod[ \t]+\w+[ \t]*\{'

rc=0
offenders=$(grep -rlPz --include='*.rs' --exclude-dir=tests --exclude=tests.rs \
    -e "${inline_test_fn}|${inline_test_mod}" crates/*/src) || rc=$?

case "$rc" in
0)
    printf '%s\n' "$offenders" | sed 's/^/error: inline test code in /' >&2
    echo >&2
    echo "Tests must live in separate files under a tests/ directory —" >&2
    echo "see .agents/testing.md, 'Test file layout — STRICT RULE'." >&2
    exit 1
    ;;
1)
    exit 0
    ;;
*)
    echo "error: check-inline-tests.sh: grep failed (exit $rc)" >&2
    exit 2
    ;;
esac
