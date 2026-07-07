import { Check, Home, Sparkles } from "lucide-react";

/** URL of the cloud account SPA where real prices and plans live. */
const ACCOUNT_URL = "https://account.wardnet.network";

const FREE_POINTS = [
  "Ad & tracker blocking for every device",
  "Per-device VPN routing and WireGuard tunnels",
  "Network zones and local DNS",
  "Built-in DHCP and device discovery",
  "The desktop admin site, no account, nothing leaves your LAN",
];

const PREMIUM_POINTS = [
  "Dynamic DNS reaches your gateway with no domain of your own",
  "Secure remote tunneling for encrypted inbound access from anywhere",
  "Roaming private DNS keeps your filtering with you off the LAN",
  "Both mobile apps: the User PWA and the Admin mobile PWA",
];

/**
 * The qualitative premium story: Free (the whole self-hosted LAN gateway)
 * versus Premium (the remote + mobile layer). No prices, plan names, or
 * entitlement numbers, those live behind the CTA on the cloud account SPA
 * at {@link ACCOUNT_URL}.
 */
export function Premium() {
  return (
    <section id="premium" className="px-6 py-20">
      <div className="mx-auto max-w-5xl">
        <div className="mb-3 flex justify-center">
          <span className="inline-flex items-center gap-2 rounded-full bg-accent-soft px-3 py-1 t-size-xs t-weight-semibold uppercase tracking-wider text-accent">
            <Sparkles size={14} />
            Premium
          </span>
        </div>
        <h2 className="mb-3 text-center t-size-3xl t-weight-bold tracking-tight text-ink">
          Cloud-optional, whenever you want it
        </h2>
        <p className="mx-auto mb-12 max-w-2xl text-center t-size-lg text-ink-3">
          Wardnet is a complete privacy gateway on its own. Premium gateway features for self-hosted
          Wardnet add the remote and mobile layer, strictly opt-in, never required.
        </p>

        <div className="grid grid-cols-1 gap-6 md:grid-cols-2">
          {/* Free column */}
          <div className="card flex flex-col">
            <div className="mb-4 flex items-center gap-3">
              <span className="text-ink-3">
                <Home size={22} />
              </span>
              <h3 className="t-size-xl t-weight-semibold text-ink">Free</h3>
            </div>
            <p className="mb-4 t-size-sm leading-relaxed text-ink-3">
              The whole self-hosted LAN gateway. Runs on your own hardware, no account required.
            </p>
            <ul className="flex flex-col gap-3">
              {FREE_POINTS.map((point) => (
                <li key={point} className="flex items-start gap-2 t-size-sm text-ink-2">
                  <Check size={18} className="mt-0.5 shrink-0 text-ink-3" aria-hidden="true" />
                  <span>{point}</span>
                </li>
              ))}
            </ul>
          </div>

          {/* Premium column */}
          <div className="card flex flex-col border-accent/30 bg-accent-soft">
            <div className="mb-4 flex items-center gap-3">
              <span className="text-accent">
                <Sparkles size={22} />
              </span>
              <h3 className="t-size-xl t-weight-semibold text-ink">Premium</h3>
            </div>
            <p className="mb-4 t-size-sm leading-relaxed text-ink-2">
              Unlock dynamic DNS and secure remote tunneling, everything in Free, plus the remote
              and mobile layer.
            </p>
            <ul className="flex flex-col gap-3">
              {PREMIUM_POINTS.map((point) => (
                <li key={point} className="flex items-start gap-2 t-size-sm text-ink-2">
                  <Check size={18} className="mt-0.5 shrink-0 text-accent" aria-hidden="true" />
                  <span>{point}</span>
                </li>
              ))}
            </ul>
          </div>
        </div>

        <div className="mt-10 flex flex-col items-center gap-3">
          <a href={ACCOUNT_URL} className="btn btn--primary justify-center px-8">
            Start free trial
          </a>
          <a href={ACCOUNT_URL} className="t-size-sm t-weight-medium text-accent hover:underline">
            See plans
          </a>
          <p className="t-size-sm text-ink-3">Starts with a free trial · cancel anytime.</p>
        </div>
      </div>
    </section>
  );
}
