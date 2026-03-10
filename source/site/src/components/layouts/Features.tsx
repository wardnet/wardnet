import { Route, Shield, Ban, Globe, Users, Network } from "lucide-react";
import { FeatureCard } from "@/components/compound/FeatureCard";

const FEATURES = [
  {
    icon: <Route size={28} />,
    title: "Per-device routing",
    description:
      "Route each device through a specific VPN tunnel, direct internet, or the network default.",
  },
  {
    icon: <Shield size={28} />,
    title: "WireGuard tunnels",
    description: "Lazy on-demand tunnels that start when needed and tear down after idle timeout.",
  },
  {
    icon: <Ban size={28} />,
    title: "DNS ad blocking",
    description: "Block ads and trackers at the DNS level for all managed devices.",
  },
  {
    icon: <Network size={28} />,
    title: "Built-in DHCP server",
    description: "Auto-detect and manage devices on your network with the integrated DHCP server.",
  },
  {
    icon: <Globe size={28} />,
    title: "VPN provider integration",
    description: "Set up tunnels from NordVPN and other providers with just your credentials.",
  },
  {
    icon: <Users size={28} />,
    title: "Self-service model",
    description: "Admins control shared devices. Users manage their own — auto-detected by IP.",
  },
] as const;

/**
 * Grid of feature cards describing Wardnet's core capabilities.
 */
export function Features() {
  return (
    <section className="px-6 py-20">
      <div className="mx-auto max-w-6xl">
        <h2 className="mb-12 text-center text-3xl font-bold text-gray-900 dark:text-gray-100">
          Everything you need to protect your network
        </h2>
        <div className="grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-3">
          {FEATURES.map((feature) => (
            <FeatureCard
              key={feature.title}
              icon={feature.icon}
              title={feature.title}
              description={feature.description}
              className="animate-fade-in-up"
            />
          ))}
        </div>
      </div>
    </section>
  );
}
