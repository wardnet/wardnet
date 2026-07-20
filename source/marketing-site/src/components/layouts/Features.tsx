import {
  Route,
  Shield,
  ShieldCheck,
  Ban,
  Layers,
  Server,
  Network,
  Globe,
  Waypoints,
  Save,
  Radio,
  Smartphone,
} from "lucide-react";
import type { ReactNode } from "react";
import { FeatureCard } from "@/components/compound/FeatureCard";

interface Feature {
  icon: ReactNode;
  title: string;
  description: string;
  href: string;
  premium?: boolean;
}

const FEATURES: Feature[] = [
  {
    icon: <Route size={28} />,
    title: "Per-device VPN routing",
    description:
      "Route each device through a specific VPN tunnel, direct internet, or the network default.",
    href: "/docs/device-routing",
  },
  {
    icon: <Waypoints size={28} />,
    title: "Domain-based routing",
    description:
      "Send traffic to a domain, like netflix.com, through a chosen tunnel and exit country, on every device.",
    href: "/docs/domain-routing",
  },
  {
    icon: <Shield size={28} />,
    title: "WireGuard tunnels",
    description: "Lazy on-demand tunnels that start when needed and tear down after idle timeout.",
    href: "/docs/wireguard-tunnels",
  },
  {
    icon: <Ban size={28} />,
    title: "DNS ad blocking",
    description: "Block ads and trackers at the DNS level for every managed device on your LAN.",
    href: "/docs/dns-ad-blocking",
  },
  {
    icon: <Layers size={28} />,
    title: "Network Zones",
    description:
      "Group devices into zones with their own egress rules and admin-access boundaries.",
    href: "/docs/network-zones",
  },
  {
    icon: <Server size={28} />,
    title: "Local DNS",
    description:
      "Authoritative zones, custom records, and conditional forwarding for your own domains.",
    href: "/docs/local-dns",
  },
  {
    icon: <Network size={28} />,
    title: "Built-in DHCP + device discovery",
    description: "Hand out leases and auto-detect devices on your network from a single service.",
    href: "/docs/dhcp-server",
  },
  {
    icon: <Globe size={28} />,
    title: "VPN provider integration",
    description: "Set up tunnels from NordVPN and other providers with just your credentials.",
    href: "/docs/vpn-providers",
  },
  {
    icon: <Save size={28} />,
    title: "Backup & restore",
    description: "Export an encrypted archive of the database, config, and keys, restore anywhere.",
    href: "/docs/backup-restore",
  },
  {
    icon: <Radio size={28} />,
    title: "Remote access & auto-HTTPS",
    description:
      "Dynamic DNS and secure remote tunneling reach your gateway from anywhere, no domain needed.",
    href: "/docs/remote-access",
    premium: true,
  },
  {
    icon: <ShieldCheck size={28} />,
    title: "Personal VPN",
    description:
      "Your own inbound WireGuard server, so your phone and laptop reach your home network from anywhere.",
    href: "/docs/personal-vpn",
    premium: true,
  },
  {
    icon: <Smartphone size={28} />,
    title: "Mobile apps",
    description:
      "A User PWA for household members and an Admin mobile PWA with push notifications.",
    href: "/docs/mobile-apps",
    premium: true,
  },
];

/**
 * Grid of feature cards describing Wardnet's core capabilities. Each card
 * links to its docs detail page; the Premium cards (remote access, personal
 * VPN, mobile apps) carry a Premium tag.
 */
export function Features() {
  return (
    <section className="px-6 py-20">
      <div className="mx-auto max-w-6xl">
        <h2 className="mb-12 text-center t-size-3xl t-weight-bold tracking-tight text-ink">
          Everything you need to protect your network
        </h2>
        <div className="grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-3">
          {FEATURES.map((feature) => (
            <FeatureCard
              key={feature.title}
              icon={feature.icon}
              title={feature.title}
              description={feature.description}
              href={feature.href}
              premium={feature.premium}
              className="animate-fade-in-up"
            />
          ))}
        </div>
      </div>
    </section>
  );
}
