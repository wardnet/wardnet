const CREDENTIAL_KEY = "wardnet_biometric_credential_id";

/**
 * WebAuthn platform-authenticator wrapper for the local biometric gate.
 *
 * This is a CLIENT-ONLY gate — no server-side assertion verification is
 * performed. The WebAuthn ceremony proves device presence (FaceID / Touch ID /
 * Windows Hello); session validity is still guarded by the server-side cookie.
 *
 * `rpId` is `window.location.hostname` so it works for both `localhost` dev
 * and `<id>.wardnet.network` production.
 */
export function useBiometric() {
  function isAvailable(): boolean {
    return !!(
      window.PublicKeyCredential &&
      typeof window.PublicKeyCredential
        .isUserVerifyingPlatformAuthenticatorAvailable === "function"
    );
  }

  function isRegistered(): boolean {
    return !!localStorage.getItem(CREDENTIAL_KEY);
  }

  async function register(username: string): Promise<void> {
    const challenge = crypto.getRandomValues(new Uint8Array(32));
    const userId = new TextEncoder().encode(username);

    const credential = await navigator.credentials.create({
      publicKey: {
        challenge,
        rp: { id: window.location.hostname, name: "Wardnet" },
        user: { id: userId, name: username, displayName: username },
        pubKeyCredParams: [
          { type: "public-key", alg: -7 },   // ES256
          { type: "public-key", alg: -257 },  // RS256
        ],
        authenticatorSelection: {
          authenticatorAttachment: "platform",
          userVerification: "required",
        },
        timeout: 60000,
      },
    });

    if (!credential) throw new Error("Credential creation returned null");
    const rawId = (credential as PublicKeyCredential).rawId;
    const base64Id = btoa(String.fromCharCode(...new Uint8Array(rawId)));
    localStorage.setItem(CREDENTIAL_KEY, base64Id);
  }

  async function authenticate(): Promise<void> {
    const storedId = localStorage.getItem(CREDENTIAL_KEY);
    if (!storedId) throw new Error("No biometric credential registered");

    const rawId = Uint8Array.from(atob(storedId), (c) => c.charCodeAt(0));
    const challenge = crypto.getRandomValues(new Uint8Array(32));

    await navigator.credentials.get({
      publicKey: {
        challenge,
        rpId: window.location.hostname,
        allowCredentials: [{ type: "public-key", id: rawId }],
        userVerification: "required",
        timeout: 60000,
      },
    });
  }

  function unregister(): void {
    localStorage.removeItem(CREDENTIAL_KEY);
  }

  return { isAvailable, isRegistered, register, authenticate, unregister };
}
