import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/forge-web/select";
import { Link } from "react-router";
import { WifiOffIcon } from "lucide-react";
import { countryFlag } from "@/lib/country";
import type { RoutingTarget, TunnelSummary } from "@wardnet/js";

const DIRECT_VALUE = "direct";

function valueFromTarget(target: RoutingTarget | null, tunnels: TunnelSummary[]): string {
  if (target?.type === "tunnel") {
    if (tunnels.some((t) => t.id === target.tunnel_id)) return target.tunnel_id;
  }
  return DIRECT_VALUE;
}

interface RoutingSelectorProps {
  value: RoutingTarget | null;
  onChange: (target: RoutingTarget) => void;
  tunnels: TunnelSummary[];
  disabled?: boolean;
  isAdmin?: boolean;
}

/** Compound component for selecting a device's routing target.
 *
 * Single dropdown: "Direct (no VPN)" plus one entry per tunnel (with country
 * flag). The legacy `Default` target (resolved server-side via
 * `network.default_policy`) is intentionally not exposed — saving rewrites
 * any incoming `default` to an explicit choice. */
export function RoutingSelector({
  value,
  onChange,
  tunnels,
  disabled,
  isAdmin,
}: RoutingSelectorProps) {
  const selected = valueFromTarget(value, tunnels);

  function handleChange(next: string) {
    if (next === DIRECT_VALUE) {
      onChange({ type: "direct" });
    } else {
      onChange({ type: "tunnel", tunnel_id: next });
    }
  }

  if (tunnels.length === 0) {
    return (
      <div className="flex flex-col gap-2">
        <Select value={DIRECT_VALUE} onValueChange={handleChange} disabled>
          <SelectTrigger className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={DIRECT_VALUE}>
              <span className="inline-flex items-center gap-2">
                <WifiOffIcon className="size-4 text-ink-3" />
                Direct (no VPN)
              </span>
            </SelectItem>
          </SelectContent>
        </Select>
        <p className="text-sm text-ink-3">
          No tunnels configured.{" "}
          {isAdmin ? (
            <Link to="/tunnels" className="text-accent underline">
              Add one
            </Link>
          ) : (
            "Contact your network admin."
          )}
        </p>
      </div>
    );
  }

  return (
    <Select value={selected} onValueChange={handleChange} disabled={disabled}>
      <SelectTrigger className="w-full">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value={DIRECT_VALUE}>
          <span className="inline-flex items-center gap-2">
            <WifiOffIcon className="size-4 text-ink-3" />
            Direct (no VPN)
          </span>
        </SelectItem>
        {tunnels.map((t) => {
          const flag = t.country_code ? countryFlag(t.country_code) : "";
          return (
            <SelectItem key={t.id} value={t.id}>
              <span className="inline-flex items-center gap-2">
                {flag ? <span aria-hidden>{flag}</span> : null}
                {t.label}
              </span>
            </SelectItem>
          );
        })}
      </SelectContent>
    </Select>
  );
}
