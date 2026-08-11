#!/usr/bin/env bash
#
# Guard the one function that can mint an authenticated principal.
#
# `AuthenticatedUser::from_validated_session` turns a bare `Uuid` plus a role
# into proof that somebody authenticated. Its fields are private, so this is
# the only way in — but it has to be `pub`, because the code that verifies
# credentials lives in a different crate from the type. Rust has no way to say
# "only these functions may call you", so this script says it instead.
#
# The danger it exists for is specific: `devices.owner_user_id` says which
# household user a device belongs to. It is attribution and grants nothing
# (ADR-0031 §4). A plausible-looking line that promotes a `Device` caller to
# its owner would be a silent privilege escalation that no type checks.
#
# Adding a file here is a deliberate act. If you are about to, the question to
# answer in the PR is: *what credential did this code just verify?* If there
# isn't one, the answer is no.

set -euo pipefail

DAEMON_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DAEMON_ROOT"

# Files permitted to call the constructor, relative to source/daemon.
#
#   - wardnet-common/src/auth.rs   — defines it; `AuthContext::system()` is here.
#   - wardnetd-services/src/auth/  — session and API-key validation: the code
#                                    that has actually just checked a credential.
#   - */tests/*, */test*           — test support may fabricate any principal.
ALLOWED_RE='^crates/wardnet-common/src/auth\.rs$
^crates/wardnetd-services/src/auth/service\.rs$
^crates/wardnet-test-support/src/'

matches="$(grep -rln --include='*.rs' 'from_validated_session' crates || true)"

violations=""
while IFS= read -r file; do
    [ -z "$file" ] && continue
    # Tests may build any principal they like — that is the point of a
    # truth-table test.
    case "$file" in
        */tests/*|*/tests.rs|*_test.rs) continue ;;
    esac
    if ! printf '%s\n' "$ALLOWED_RE" | grep -qE -f - <(printf '%s\n' "$file"); then
        violations="${violations}${file}\n"
    fi
done <<< "$matches"

if [ -n "$violations" ]; then
    echo "error: AuthenticatedUser::from_validated_session called outside the sanctioned files." >&2
    echo >&2
    printf "%b" "$violations" | sed 's/^/  /' >&2
    echo >&2
    echo "This function mints an authenticated principal from a bare UUID." >&2
    echo "Only code that has just verified a credential may call it — see" >&2
    echo "docs/adr/0031-household-identity.md and .agents/auth.md." >&2
    echo >&2
    echo "If the new call site really does verify a credential, add it to" >&2
    echo "ALLOWED_RE in build-support/check-auth-constructors.sh and say so in" >&2
    echo "the PR description." >&2
    exit 1
fi

echo "auth constructors: ok"
