import { Monitor, Smartphone, ShieldCheck } from "lucide-react";
import type { ReactNode } from "react";

interface Surface {
  icon: ReactNode;
  name: string;
  audience: string;
  description: string;
  premium?: boolean;
  /** Screenshot aspect ratio, desktop is wide, phones are tall. */
  shape: "desktop" | "phone";
  image: string;
}

const SURFACES: Surface[] = [
  {
    icon: <Monitor size={24} />,
    name: "Desktop admin site",
    audience: "For the operator",
    description:
      "The full control surface for tunnels, routing, zones, DNS, devices, and stats. Served straight from the gateway on your LAN, no account, no cloud.",
    shape: "desktop",
    image: "/docs/app-surfaces/desktop-admin.png",
  },
  {
    icon: <Smartphone size={24} />,
    name: "User PWA",
    audience: "For household members",
    description:
      "A self-service app where each person manages their own devices and tunnels, installable to the home screen, reachable from anywhere.",
    premium: true,
    shape: "phone",
    image: "/docs/mobile-apps/user-home.png",
  },
  {
    icon: <ShieldCheck size={24} />,
    name: "Admin mobile PWA",
    audience: "For admins on the go",
    description:
      "Administer the whole gateway from your phone, with push notifications for rule requests and network alerts.",
    premium: true,
    shape: "phone",
    image: "/docs/mobile-apps/admin-dashboard.png",
  },
];

/**
 * Showcases the three app surfaces Wardnet ships: the desktop admin site
 * plus the two mobile PWAs (marketed as Premium). Screenshots land in a
 * later session, for now each surface shows a framed placeholder under
 * `public/docs/mobile-apps/`.
 */
export function AppSurfaces() {
  return (
    <section className="bg-sunken px-6 py-20">
      <div className="mx-auto max-w-6xl">
        <h2 className="mb-3 text-center t-size-3xl t-weight-bold tracking-tight text-ink">
          Three ways to see your network
        </h2>
        <p className="mx-auto mb-12 max-w-2xl text-center t-size-lg text-ink-3">
          One gateway, three tailored surfaces, a desktop admin site for the operator, and two
          mobile apps for everyone else.
        </p>
        <div className="grid grid-cols-1 gap-8 md:grid-cols-3">
          {SURFACES.map((surface) => (
            <SurfaceCard key={surface.name} surface={surface} />
          ))}
        </div>
      </div>
    </section>
  );
}

function SurfaceCard({ surface }: { surface: Surface }) {
  return (
    <div className="card flex flex-col">
      <div className="mb-4 flex items-center justify-between">
        <span className="text-accent">{surface.icon}</span>
        {surface.premium && (
          <span className="rounded-full bg-accent-soft px-2 py-0.5 t-size-xs t-weight-semibold uppercase tracking-wider text-accent">
            Premium
          </span>
        )}
      </div>
      <div
        className={
          "mb-4 overflow-hidden rounded-lg border border-line-strong bg-elev " +
          (surface.shape === "desktop" ? "aspect-video" : "mx-auto aspect-[9/16] w-40")
        }
      >
        <img
          src={surface.image}
          alt={`${surface.name} screenshot`}
          loading="lazy"
          className="h-full w-full object-cover object-top"
        />
      </div>
      <p className="t-size-xs t-weight-semibold uppercase tracking-wider text-accent">
        {surface.audience}
      </p>
      <h3 className="mb-2 t-size-lg t-weight-semibold text-ink">{surface.name}</h3>
      <p className="t-size-sm leading-relaxed text-ink-3">{surface.description}</p>
    </div>
  );
}
