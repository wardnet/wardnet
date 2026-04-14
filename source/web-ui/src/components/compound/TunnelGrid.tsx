import { Card, CardContent } from "@/components/core/ui/card";
import { TunnelCard } from "./TunnelCard";
import { EmptyStatePlaceholder } from "./EmptyStatePlaceholder";
import type { Tunnel, ProviderInfo } from "@wardnet/js";

interface TunnelGridProps {
  tunnels: Tunnel[];
  providers: ProviderInfo[];
  isLoading: boolean;
  isError: boolean;
  onDelete: (id: string) => void;
  /** Called when the user clicks the "Add Tunnel" button in the empty state. */
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
  if (isLoading) {
    return (
      <Card>
        <CardContent className="py-10 text-center text-muted-foreground">
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
        actionLabel={onAdd ? "Add Tunnel" : undefined}
        onAction={onAdd}
      />
    );
  }

  if (tunnels.length === 0) return null;

  return (
    <div className="grid gap-4 md:grid-cols-2">
      {tunnels.map((tunnel) => (
        <TunnelCard key={tunnel.id} tunnel={tunnel} providers={providers} onDelete={onDelete} />
      ))}
    </div>
  );
}
