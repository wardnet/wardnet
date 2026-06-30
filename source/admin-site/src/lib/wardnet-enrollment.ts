import { useEffect, useState } from "react";
import {
  useCheckDdnsSlug,
  useConfigureCloudflare,
  useEnrollDdns,
  useRegisterDdns,
  useRequestEnrollmentCode,
} from "@wardnet/web";
import { isValidName, suggestName } from "@/lib/suggestName";

export type Provider = "wardnet" | "cloudflare";

/**
 * The wardnet path is a three-step enrollment before the slug is chosen:
 * email the account a one-time code → enter the code (enroll) → pick a slug.
 */
export type WardnetStep = "email" | "code" | "slug";

export type Availability =
  | "unknown"
  | "checking"
  | "available"
  | "taken"
  | "invalid"
  | "error";

/**
 * Shared enrollment state machine for the wardnet/Cloudflare provider form.
 *
 * Owns every field the form edits (provider, step, email, code, slug, token,
 * domain), the live debounced slug-availability check, the derived availability/
 * busy/disabled state, and the four action handlers. The two surfaces that use
 * it — the setup wizard's `Step7RemoteAccess` and the `RemoteAccess` settings
 * page — differ only in what happens on success (`onProvisioned`) and how
 * errors surface (`onError`); the wizard maps 502/503 to a dedicated "service
 * unavailable" view while settings just shows the error inline. `clearError`
 * runs at the start of every action so stale errors don't linger.
 *
 * Per-action pending flags are exposed individually (`pending`) so each step's
 * button gates only on its own mutation, never on an unrelated one in flight.
 */
export interface WardnetEnrollment {
  provider: Provider;
  setProvider: (p: Provider) => void;
  wardnetStep: WardnetStep;
  setWardnetStep: (s: WardnetStep) => void;
  email: string;
  setEmail: (v: string) => void;
  code: string;
  setCode: (v: string) => void;
  slug: string;
  setSlug: (v: string) => void;
  token: string;
  setToken: (v: string) => void;
  domain: string;
  setDomain: (v: string) => void;
  /** Combined client+server availability for the chosen slug. */
  availability: Availability;
  /** Any enrollment mutation is in flight — for gating page-level controls (e.g. Cancel). */
  busy: boolean;
  /** The slug "Register" button should be disabled (busy, checking, or invalid). */
  slugDisabled: boolean;
  /** Per-action pending flags, so each button gates only on its own mutation. */
  pending: {
    sendCode: boolean;
    verify: boolean;
    register: boolean;
    configureCf: boolean;
  };
  /** Reset all fields to a fresh enrollment (does not touch `provider`). */
  reset: () => void;
  sendCode: () => Promise<void>;
  verifyCode: () => Promise<void>;
  registerWardnet: () => Promise<void>;
  enableCloudflare: () => Promise<void>;
}

export function useWardnetEnrollment(opts: {
  onProvisioned: () => void;
  onError: (err: unknown) => void;
  clearError: () => void;
}): WardnetEnrollment {
  const { onProvisioned, onError, clearError } = opts;

  const requestCode = useRequestEnrollmentCode();
  const enroll = useEnrollDdns();
  const register = useRegisterDdns();
  const configureCf = useConfigureCloudflare();
  // `mutateAsync` is referentially stable, so it's safe in the effect deps.
  const { mutateAsync: checkSlugAsync } = useCheckDdnsSlug();

  const [provider, setProvider] = useState<Provider>("wardnet");
  const [wardnetStep, setWardnetStep] = useState<WardnetStep>("email");
  const [email, setEmail] = useState("");
  const [code, setCode] = useState("");
  const [slug, setSlug] = useState(() => suggestName());
  const [serverAvailability, setServerAvailability] = useState<
    "unknown" | "checking" | "available" | "taken" | "error"
  >("unknown");
  const [token, setToken] = useState("");
  const [domain, setDomain] = useState("");

  // Client-side validity is derived during render (no effect/setState), so the
  // "invalid" hint and the disabled button update instantly as the user types.
  const clientValid =
    provider !== "wardnet" || wardnetStep !== "slug" || isValidName(slug);
  const availability: Availability = !clientValid
    ? "invalid"
    : serverAvailability;

  // Debounced live availability check for the wardnet slug — only once enrolled
  // and on the slug step. All state writes are async (inside the timer /
  // promise), never synchronous in the effect body.
  useEffect(() => {
    if (provider !== "wardnet" || wardnetStep !== "slug" || !isValidName(slug))
      return;
    let cancelled = false;
    const handle = setTimeout(() => {
      setServerAvailability("checking");
      checkSlugAsync(slug)
        .then((res) => {
          if (!cancelled)
            setServerAvailability(res.available ? "available" : "taken");
        })
        .catch(() => {
          // The daemon couldn't reach the cloud (offline / service down).
          // Surface it rather than leaving the field with no feedback.
          if (!cancelled) setServerAvailability("error");
        });
    }, 400);
    return () => {
      cancelled = true;
      clearTimeout(handle);
    };
  }, [slug, provider, wardnetStep, checkSlugAsync]);

  const busy =
    requestCode.isPending ||
    enroll.isPending ||
    register.isPending ||
    configureCf.isPending;
  const slugDisabled =
    register.isPending ||
    availability === "checking" ||
    availability === "invalid";

  function reset() {
    setWardnetStep("email");
    setEmail("");
    setCode("");
    setSlug(suggestName());
    setServerAvailability("unknown");
    setToken("");
    setDomain("");
  }

  async function sendCode() {
    clearError();
    try {
      await requestCode.mutateAsync({ email });
      setWardnetStep("code");
    } catch (err) {
      onError(err);
    }
  }

  async function verifyCode() {
    clearError();
    try {
      await enroll.mutateAsync({ code });
      setWardnetStep("slug");
    } catch (err) {
      onError(err);
    }
  }

  async function registerWardnet() {
    clearError();
    try {
      await register.mutateAsync({ slug });
      onProvisioned();
    } catch (err) {
      onError(err);
    }
  }

  async function enableCloudflare() {
    clearError();
    try {
      await configureCf.mutateAsync({ token, domain });
      onProvisioned();
    } catch (err) {
      onError(err);
    }
  }

  return {
    provider,
    setProvider,
    wardnetStep,
    setWardnetStep,
    email,
    setEmail,
    code,
    setCode,
    slug,
    setSlug,
    token,
    setToken,
    domain,
    setDomain,
    availability,
    busy,
    slugDisabled,
    pending: {
      sendCode: requestCode.isPending,
      verify: enroll.isPending,
      register: register.isPending,
      configureCf: configureCf.isPending,
    },
    reset,
    sendCode,
    verifyCode,
    registerWardnet,
    enableCloudflare,
  };
}
