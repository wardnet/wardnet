import { Sheet, SheetContent, SheetTitle } from "@/components/core/ui/sheet";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/core/ui/tabs";
import { ManualTunnelTab } from "./ManualTunnelTab";
import { ProviderTunnelTab } from "./ProviderTunnelTab";

interface CreateTunnelSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** Sheet for creating a new WireGuard tunnel via manual config or provider setup. */
export function CreateTunnelSheet({ open, onOpenChange }: CreateTunnelSheetProps) {
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-full overflow-y-auto p-6">
        <SheetTitle>Add WireGuard Tunnel</SheetTitle>
        <Tabs defaultValue="manual" className="mt-6">
          <TabsList className="w-full">
            <TabsTrigger value="manual" className="flex-1">
              Manual
            </TabsTrigger>
            <TabsTrigger value="provider" className="flex-1">
              Provider
            </TabsTrigger>
          </TabsList>
          <TabsContent value="manual" className="mt-4">
            <ManualTunnelTab onSuccess={() => onOpenChange(false)} />
          </TabsContent>
          <TabsContent value="provider" className="mt-4">
            <ProviderTunnelTab onSuccess={() => onOpenChange(false)} />
          </TabsContent>
        </Tabs>
      </SheetContent>
    </Sheet>
  );
}
