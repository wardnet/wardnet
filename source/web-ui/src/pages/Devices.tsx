import { useState } from "react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/core/ui/tabs";
import { PageHeader } from "@/components/compound/PageHeader";
import { DeviceTable } from "@/components/compound/DeviceTable";
import { DiscoveryPlaceholder } from "@/components/compound/DiscoveryPlaceholder";
import { EmptyStatePlaceholder } from "@/components/compound/EmptyStatePlaceholder";
import { EditDeviceSheet } from "@/components/features/EditDeviceSheet";
import { useDevices } from "@/hooks/useDevices";

function sortDevices<T extends { name: string | null; hostname: string | null; mac: string }>(
  devices: T[],
): T[] {
  return [...devices].sort((a, b) => {
    const nameA = (a.name ?? a.hostname ?? a.mac).toLowerCase();
    const nameB = (b.name ?? b.hostname ?? b.mac).toLowerCase();
    return nameA.localeCompare(nameB);
  });
}

/** Devices page with managed and discovered tabs. */
export default function Devices() {
  const { data, isLoading, isError } = useDevices();
  const allDevices = data?.devices ?? [];
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null);

  const managed = sortDevices(allDevices.filter((d) => d.name != null));
  const discovered = sortDevices(allDevices.filter((d) => d.name == null));

  return (
    <>
      <PageHeader title="Devices" />

      {(isLoading || (!isError && allDevices.length === 0)) && (
        <DiscoveryPlaceholder
          cols={5}
          message="Searching for network devices"
          hint="Devices will appear as they are detected on the network."
        />
      )}

      {allDevices.length > 0 && (
        <Tabs defaultValue="managed" className="flex min-h-0 flex-1 flex-col">
          <TabsList>
            <TabsTrigger value="managed">Managed</TabsTrigger>
            <TabsTrigger value="discovered">Discovered</TabsTrigger>
          </TabsList>
          <TabsContent value="managed" className="mt-4 flex min-h-0 flex-1 flex-col">
            {managed.length > 0 ? (
              <DeviceTable devices={managed} onDeviceClick={setSelectedDeviceId} />
            ) : (
              <EmptyStatePlaceholder
                message="No managed devices yet"
                hint="Click a discovered device to give it a name and it will appear here."
              />
            )}
          </TabsContent>
          <TabsContent value="discovered" className="mt-4 flex min-h-0 flex-1 flex-col">
            {discovered.length > 0 ? (
              <DeviceTable devices={discovered} onDeviceClick={setSelectedDeviceId} />
            ) : (
              <p className="py-10 text-center text-sm text-muted-foreground">
                All devices have been named. New devices will appear here when detected.
              </p>
            )}
          </TabsContent>
        </Tabs>
      )}

      {selectedDeviceId && (
        <EditDeviceSheet
          deviceId={selectedDeviceId}
          open={!!selectedDeviceId}
          onOpenChange={(open) => {
            if (!open) setSelectedDeviceId(null);
          }}
        />
      )}
    </>
  );
}
