import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Text,
} from "@wardnet/ui";
import { Link } from "react-router";
import { useMemo } from "react";
import { WifiOffIcon } from "lucide-react";
import { countryFlag } from "../lib/country";
import { sortByLabel } from "../lib/utils";
import type { RoutingTarget, TunnelSummary } from "@wardnet/js";

const DIRECT_VALUE = "direct";

function valueFromTarget(
  target: RoutingTarget | null,
  tunnels: TunnelSummary[],
): string {
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
  /** e2e locator, forwarded to the rendered `<SelectTrigger>`. */
  "data-testid"?: string;
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
  "data-testid": dataTestId,
}: RoutingSelectorProps) {
  const selected = valueFromTarget(value, tunnels);
  // Before the empty-state early-return: hook order must not depend on props.
  const sorted = useMemo(() => sortByLabel(tunnels, (t) => t.label), [tunnels]);

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
          <SelectTrigger className="w-full" data-testid={dataTestId}>
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
        <Text as="p" size="sm" className="text-ink-3">
          No tunnels configured.{" "}
          {isAdmin ? (
            <Link to="/tunnels" className="text-accent underline">
              Add one
            </Link>
          ) : (
            "Contact your network admin."
          )}
        </Text>
      </div>
    );
  }

  return (
    <Select value={selected} onValueChange={handleChange} disabled={disabled}>
      <SelectTrigger className="w-full" data-testid={dataTestId}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value={DIRECT_VALUE}>
          <span className="inline-flex items-center gap-2">
            <WifiOffIcon className="size-4 text-ink-3" />
            Direct (no VPN)
          </span>
        </SelectItem>
        {sorted.map((t) => {
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
