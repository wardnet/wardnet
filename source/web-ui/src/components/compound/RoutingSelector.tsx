import { RadioGroup, RadioGroupItem } from "@/components/core/ui/radio-group";
import { Label } from "@/components/core/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/core/ui/select";
import type { RoutingTarget, Tunnel } from "@wardnet/js";
import { Link } from "react-router";

type RoutingMode = "default" | "direct" | "tunnel";

function modeFromTarget(target: RoutingTarget | null): RoutingMode {
  if (!target) return "default";
  return target.type === "tunnel" ? "tunnel" : target.type;
}

function tunnelIdFromTarget(target: RoutingTarget | null): string | undefined {
  if (target?.type === "tunnel") return target.tunnel_id;
  return undefined;
}

interface RoutingSelectorProps {
  value: RoutingTarget | null;
  onChange: (target: RoutingTarget) => void;
  tunnels: Tunnel[];
  disabled?: boolean;
  isAdmin?: boolean;
}

/** Compound component for selecting a device's routing target. */
export function RoutingSelector({
  value,
  onChange,
  tunnels,
  disabled,
  isAdmin,
}: RoutingSelectorProps) {
  const mode = modeFromTarget(value);
  const selectedTunnelId = tunnelIdFromTarget(value);

  function handleModeChange(newMode: string) {
    switch (newMode) {
      case "direct":
        onChange({ type: "direct" });
        break;
      case "default":
        onChange({ type: "default" });
        break;
      case "tunnel":
        if (tunnels.length > 0) {
          onChange({ type: "tunnel", tunnel_id: tunnels[0].id });
        }
        break;
    }
  }

  function handleTunnelChange(tunnelId: string) {
    onChange({ type: "tunnel", tunnel_id: tunnelId });
  }

  return (
    <div className="flex flex-col gap-3">
      <RadioGroup value={mode} onValueChange={handleModeChange} disabled={disabled}>
        <div className="flex items-center gap-2">
          <RadioGroupItem value="default" id="route-default" />
          <Label htmlFor="route-default" className="cursor-pointer font-normal">
            Default
          </Label>
        </div>
        <div className="flex items-center gap-2">
          <RadioGroupItem value="direct" id="route-direct" />
          <Label htmlFor="route-direct" className="cursor-pointer font-normal">
            Direct (no VPN)
          </Label>
        </div>
        <div className="flex items-center gap-2">
          <RadioGroupItem value="tunnel" id="route-tunnel" />
          <Label htmlFor="route-tunnel" className="cursor-pointer font-normal">
            Via Tunnel
          </Label>
        </div>
      </RadioGroup>

      {mode === "tunnel" && (
        <div className="pl-6">
          {tunnels.length > 0 ? (
            <Select value={selectedTunnelId} onValueChange={handleTunnelChange} disabled={disabled}>
              <SelectTrigger className="w-full">
                <SelectValue placeholder="Select a tunnel" />
              </SelectTrigger>
              <SelectContent>
                {tunnels.map((t) => (
                  <SelectItem key={t.id} value={t.id}>
                    {t.label} ({t.country_code.toUpperCase()})
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : (
            <p className="text-sm text-muted-foreground">
              No tunnels configured.{" "}
              {isAdmin ? (
                <Link to="/tunnels" className="text-primary underline">
                  Add one
                </Link>
              ) : (
                "Contact your network admin."
              )}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
