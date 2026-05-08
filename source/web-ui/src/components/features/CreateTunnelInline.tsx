import { Card, CardContent, CardFooter, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Button } from "@/components/core/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/core/ui/tabs";
import { ManualTunnelTab } from "./ManualTunnelTab";
import { ProviderTunnelTab } from "./ProviderTunnelTab";

interface CreateTunnelInlineProps {
  /** Called when the operator clicks Cancel or after a successful create. */
  onClose: () => void;
  /**
   * Drop the outer Card chrome so this can nest inside another Card
   * (e.g. the setup wizard's auth card) without doubling the borders
   * and padding. Default `false` — the Tunnels page wants the Card.
   */
  embedded?: boolean;
}

/**
 * Inline panel for creating a WireGuard tunnel.
 *
 * Replaces the previous sheet-based flow to match the
 * `DeviceSettingsCard` inline-edit pattern: the Tunnels page and the
 * setup wizard both toggle this content into existence rather than
 * sliding in a side panel. The user-visible form bits (manual config
 * paste vs. provider sheet) are unchanged — only the chrome moved.
 */
export function CreateTunnelInline({ onClose, embedded = false }: CreateTunnelInlineProps) {
  // Tab styling matches the DHCP page (`Dhcp.tsx`) — natural-width
  // buttons rather than the stretched-flex variant the sheet used.
  const body = (
    <Tabs defaultValue="manual">
      <TabsList>
        <TabsTrigger value="manual">Manual</TabsTrigger>
        <TabsTrigger value="provider">Provider</TabsTrigger>
      </TabsList>
      <TabsContent value="manual" className="mt-4">
        <ManualTunnelTab onSuccess={onClose} />
      </TabsContent>
      <TabsContent value="provider" className="mt-4">
        <ProviderTunnelTab onSuccess={onClose} />
      </TabsContent>
    </Tabs>
  );

  if (embedded) {
    return (
      <div className="flex flex-col gap-4">
        <h3 className="text-base font-semibold text-foreground">Add WireGuard tunnel</h3>
        {body}
        <div className="flex justify-end">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
        </div>
      </div>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Add WireGuard tunnel</CardTitle>
      </CardHeader>
      <CardContent>{body}</CardContent>
      <CardFooter className="justify-end">
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
      </CardFooter>
    </Card>
  );
}
