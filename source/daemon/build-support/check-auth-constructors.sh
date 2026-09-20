#!/usr/bin/env bash
#
# Guard the functions that can mint proof of authentication.
#
# There are two, and they are the same class of primitive: given nothing but a
# user id, each hands back something the rest of the system treats as "this
# person authenticated".
#
#   AuthenticatedUser::from_validated_session  -> an in-process principal
#   AuthService::issue_verified_session        -> a session token, i.e. a
#                                                 credential that persists and
#                                                 can be replayed later
#
# Their fields and internals are private, so these are the only ways in — but
# both have to be `pub`, because the code that verifies credentials lives in a
# different crate from the type (and, for the session, in a different crate from
# the service that proved the identity). Rust has no way to say "only these
# functions may call you", so this script says it instead.
#
# The danger it exists for is specific: `devices.owner_user_id` says which
# household user a device belongs to. It is attribution and grants nothing
# (ADR-0031 §4). A plausible-looking line that promotes a `Device` caller to
# its owner would be a silent privilege escalation that no type checks. The
# session variant is worse if it slips: an in-process principal dies with the
# request, a minted session does not.
#
# Adding a file here is a deliberate act. If you are about to, the question to
# answer in the PR is: *what credential did this code just verify?* If there
# isn't one, the answer is no.

set -euo pipefail

DAEMON_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DAEMON_ROOT"

status=0

# Check one guarded symbol against its own allow-list.
#
# Each symbol gets a distinct list rather than one shared one: the set of code
# entitled to build an in-process principal is not the set entitled to mint a
# durable session, and collapsing them would quietly widen both.
check_symbol() {
    local symbol="$1" allowed_re="$2" what="$3"

    local matches violations="" file
    matches="$(grep -rln --include='*.rs' "$symbol" crates || true)"

    while IFS= read -r file; do
        [ -z "$file" ] && continue
        # Tests may build any principal they like — that is the point of a
        # truth-table test.
        case "$file" in
            */tests/*|*/tests.rs|*_test.rs) continue ;;
        esac
        if ! printf '%s\n' "$allowed_re" | grep -qE -f - <(printf '%s\n' "$file"); then
            violations="${violations}${file}\n"
        fi
    done <<< "$matches"

    if [ -n "$violations" ]; then
        echo "error: $symbol called outside the sanctioned files." >&2
        echo >&2
        printf "%b" "$violations" | sed 's/^/  /' >&2
        echo >&2
        echo "$what" >&2
        echo "Only code that has just verified a credential may call it — see" >&2
        echo "docs/adr/0031-household-identity.md and .agents/auth.md." >&2
        echo >&2
        echo "If the new call site really does verify a credential, add it to" >&2
        echo "that symbol's allow-list in" >&2
        echo "build-support/check-auth-constructors.sh and say so in the PR" >&2
        echo "description." >&2
        echo >&2
        status=1
    fi
}

# Files permitted to build an in-process principal, relative to source/daemon.
#
#   - wardnet-common/src/auth.rs   — defines it; `AuthContext::system()` is here.
#   - wardnetd-services/src/auth/  — session and API-key validation: the code
#                                    that has actually just checked a credential.
#   - wardnet-test-support/src/    — test support may fabricate any principal.
check_symbol 'from_validated_session' \
'^crates/wardnet-common/src/auth\.rs$
^crates/wardnetd-services/src/auth/service\.rs$
^crates/wardnet-test-support/src/' \
'This function mints an authenticated principal from a bare UUID.'

# Files permitted to mint a session, relative to source/daemon.
#
#   - wardnetd-services/src/auth/service.rs — declares and implements it.
#   - wardnetd-api/src/api/user_auth.rs     — the OAuth callback, which has just
#                                             verified a provider assertion via
#                                             `complete_oauth_callback`
#                                             (ADR-0031 §11).
check_symbol 'issue_verified_session' \
'^crates/wardnetd-services/src/auth/service\.rs$
^crates/wardnetd-api/src/api/user_auth\.rs$' \
'This function mints a session — a replayable credential — from a bare UUID.'

if [ "$status" -ne 0 ]; then
    exit 1
fi

echo "auth constructors: ok"
