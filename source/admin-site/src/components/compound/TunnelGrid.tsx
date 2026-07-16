import { useMemo } from "react";
import { Card, CardContent } from "@wardnet/web";
import { sortByLabel } from "@wardnet/web";
import { TunnelCard } from "./TunnelCard";
import { EmptyStatePlaceholder } from "./EmptyStatePlaceholder";
import type { Tunnel, ProviderInfo } from "@wardnet/js";

interface TunnelGridProps {
  /** Rendered alphabetically by label regardless of the order passed in. */
  tunnels: Tunnel[];
  providers: ProviderInfo[];
  isLoading: boolean;
  isError: boolean;
  onDelete: (id: string) => void;
  /** Called when the user clicks the "Add tunnel" button in the empty state. */
  onAdd?: () => void;
}

/** Responsive grid of tunnel cards with loading/empty states. */
export function TunnelGrid({
  tunnels,
  providers,
  isLoading,
  isError,
  onDelete,
  onAdd,
}: TunnelGridProps) {
  // Before the loading/empty early-returns: hook order must not depend on state.
  const sorted = useMemo(() => sortByLabel(tunnels, (t) => t.label), [tunnels]);

  if (isLoading) {
    return (
      <Card>
        <CardContent className="py-10 text-center text-ink-3">
          Loading tunnels...
        </CardContent>
      </Card>
    );
  }

  if (!isError && tunnels.length === 0) {
    return (
      <EmptyStatePlaceholder
        message="No tunnels configured"
        hint="Add a WireGuard tunnel to route device traffic through a VPN provider."
        actionLabel={onAdd ? "Add tunnel" : undefined}
        onAction={onAdd}
        actionTestId="tunnel-add"
      />
    );
  }

  if (tunnels.length === 0) return null;

  return (
    <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
      {sorted.map((tunnel) => (
        <TunnelCard
          key={tunnel.id}
          tunnel={tunnel}
          providers={providers}
          onDelete={onDelete}
        />
      ))}
    </div>
  );
}
