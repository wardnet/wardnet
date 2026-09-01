import { Button } from "@wardnet/ui";
import { Text } from "@wardnet/ui";
import type { OauthProvider, OauthReturnTo } from "@wardnet/js";

import { useAuthMethods, useStartOauth } from "../hooks/useUsers";

const PROVIDER_LABEL: Record<string, string> = {
  google: "Google",
  github: "GitHub",
};

interface OauthSignInButtonsProps {
  /** Which admin surface to return to after the ceremony. */
  returnTo: OauthReturnTo;
  /**
   * Whether the resulting session should be long-lived.
   *
   * Must be decided here, at the start: it is parked on the ceremony because
   * the provider's callback carries no body, and there is deliberately no
   * endpoint that raises it afterwards.
   */
  rememberMe?: boolean;
}

/**
 * Federated sign-in buttons, rendered from what the box can actually offer.
 *
 * Driven by `GET /api/auth/methods`, which is unauthenticated precisely so a
 * sign-in screen can call it. Renders **nothing** when no provider is enabled
 * — the common case, since federated sign-in is opt-in and needs a public
 * hostname. A button for an unconfigured provider would only fail at the
 * provider, with an error the person cannot act on.
 */
export function OauthSignInButtons({
  returnTo,
  rememberMe = false,
}: OauthSignInButtonsProps) {
  const { data: methods } = useAuthMethods();
  const startOauth = useStartOauth();

  const enabled = (methods?.providers ?? []).filter((p) => p.enabled);
  if (enabled.length === 0) return null;

  function signIn(provider: OauthProvider) {
    startOauth.mutate(
      { provider, returnTo, rememberMe },
      {
        // A full navigation, not a router push: the destination is the
        // provider's own origin. The daemon returns the URL rather than
        // redirecting so a failure surfaces as an error here instead of an
        // opaque cross-origin fetch.
        onSuccess: (url) => window.location.assign(url),
      },
    );
  }

  return (
    <div className="oauth-signin">
      <Text as="p" size="sm" className="oauth-signin__divider">
        or continue with
      </Text>
      {enabled.map((provider) => (
        <Button
          key={provider.provider}
          type="button"
          variant="secondary"
          disabled={startOauth.isPending}
          data-testid={`oauth-signin-${provider.provider}`}
          className="oauth-signin__button"
          onClick={() => signIn(provider.provider as OauthProvider)}
        >
          {PROVIDER_LABEL[provider.provider] ?? provider.provider}
        </Button>
      ))}
    </div>
  );
}
