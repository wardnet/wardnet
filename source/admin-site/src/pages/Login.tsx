import { useState } from "react";
import { useLocation, useNavigate, useSearchParams } from "react-router";
import {
  LoginForm,
  OauthSignInButtons,
  Text,
  useAuthStore,
} from "@wardnet/web";

/**
 * What the OAuth callback's `oauth_error` codes mean, in words.
 *
 * The callback cannot render — it hands the browser back to this SPA — so its
 * only channel is a query parameter drawn from a closed set. Without this map
 * a declined consent and a broken box look identical: a silent bounce to the
 * sign-in screen.
 */
/**
 * A `Map`, not an object literal: the key comes straight off the query string,
 * and indexing a plain object with attacker-supplied text reaches the
 * prototype chain — `?oauth_error=constructor` would resolve to a function and
 * blow up the render. A `Map` has no such chain to walk.
 */
const OAUTH_ERROR_MESSAGE = new Map<string, string>([
  [
    "access_denied",
    "That account cannot sign in here. Either you declined, or nobody has linked it to a Wardnet user yet — ask an admin to link it.",
  ],
  [
    "invalid_request",
    "That sign-in attempt expired or was already used. Please try again.",
  ],
  [
    "provider_unavailable",
    "Could not reach the sign-in provider. Check this Wardnet's internet connection and try again.",
  ],
  ["server_error", "Something went wrong on this Wardnet. Please try again."],
]);

export default function Login() {
  const navigate = useNavigate();
  const location = useLocation();
  const [params] = useSearchParams();
  const { login } = useAuthStore();
  // Applied to a federated sign-in too — see `onRememberMeChange`.
  const [rememberMe, setRememberMe] = useState(false);

  const oauthError = params.get("oauth_error");
  // AdminRoute stashes the attempted path in location.state.from when it
  // bounces an unauthenticated deep-link here; return there after login,
  // falling back to the dashboard.
  const from = (location.state as { from?: string } | null)?.from ?? "/";
  return (
    <>
      {oauthError && (
        <Text as="p" role="alert" data-testid="oauth-error">
          {OAUTH_ERROR_MESSAGE.get(oauthError) ??
            // An unrecognised code is still worth saying out loud; silence
            // would leave the person with no idea anything was attempted.
            "That sign-in attempt did not work. Please try again."}
        </Text>
      )}
      <LoginForm
        login={login}
        rememberMe="checkbox"
        onRememberMeChange={setRememberMe}
        onSuccess={() => navigate(from)}
      />
      {/*
        Renders nothing unless a provider is configured *and* enabled, which is
        the usual case — federated sign-in is opt-in and needs a public
        hostname. `returnTo` is "admin" because this is the desktop site; the
        callback uses it to send the browser back here rather than to the PWA.
      */}
      <OauthSignInButtons returnTo="admin" rememberMe={rememberMe} />
    </>
  );
}
