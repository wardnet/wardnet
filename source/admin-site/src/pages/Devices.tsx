import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { PageHeader } from "@/components/compound/PageHeader";
import { DeviceTable } from "@/components/compound/DeviceTable";
import { DiscoveryPlaceholder } from "@/components/compound/DiscoveryPlaceholder";
import {
  deviceDisplayName,
  findNeighbourMacs,
  matchesDevice,
  sortByLabel,
  Button,
  Text,
  useDevices,
} from "@wardnet/web";
import type { Device } from "@wardnet/js";

type GroupId = "all" | "managed" | "unmanaged" | "recent";

const RECENT_WINDOW_MS = 60 * 60 * 1000; // 1 hour

function isManaged(d: Device): boolean {
  return d.name != null;
}

function isRecent(d: Device, now: number): boolean {
  if (!d.last_seen) return false;
  const t = new Date(d.last_seen).getTime();
  if (Number.isNaN(t)) return false;
  return now - t < RECENT_WINDOW_MS;
}

/** Devices page with grouped, searchable device table. */
export default function Devices() {
  const { data, isLoading, isError } = useDevices();
  const navigate = useNavigate();
  const allDevices = useMemo(() => data?.devices ?? [], [data]);

  const [group, setGroup] = useState<GroupId>("all");
  const [query, setQuery] = useState("");
  // Reference clock for the "Recently seen" window. It must advance with
  // wall-clock time, not freeze at mount — otherwise the 1-hour cutoff drifts
  // on a long-lived tab and devices never age out of the bucket. Refreshing
  // rows via polling doesn't help: the cutoff they're compared against is what
  // goes stale. A modest interval keeps it live without a per-render impurity.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(id);
  }, []);

  const counts = useMemo(() => {
    return {
      all: allDevices.length,
      managed: allDevices.filter(isManaged).length,
      unmanaged: allDevices.filter((d) => !isManaged(d)).length,
      recent: allDevices.filter((d) => isRecent(d, now)).length,
    };
  }, [allDevices, now]);

  const visibleDevices = useMemo(() => {
    const byGroup =
      group === "managed"
        ? allDevices.filter(isManaged)
        : group === "unmanaged"
          ? allDevices.filter((d) => !isManaged(d))
          : group === "recent"
            ? allDevices.filter((d) => isRecent(d, now))
            : allDevices;
    return sortByLabel(
      byGroup.filter((d) => matchesDevice(d, query)),
      deviceDisplayName,
    );
  }, [allDevices, group, query, now]);

  // Whether the query matches anything at all, ignoring the active group. The
  // dead-end help must key off this rather than off `visibleDevices`: a device
  // that matches but sits in a tab the admin forgot was selected is *present*,
  // and telling them "this may be the Bluetooth MAC" would send them chasing a
  // problem they do not have.
  const matchesAnywhere = useMemo(
    () => allDevices.some((d) => matchesDevice(d, query)),
    [allDevices, query],
  );

  // Only offered when the search found nothing anywhere.
  const neighbours = useMemo(
    () => (matchesAnywhere ? [] : findNeighbourMacs(allDevices, query)),
    [matchesAnywhere, allDevices, query],
  );

  function openDevice(id: string) {
    navigate(`/devices/${id}`);
  }

  // Initial / loading state — placeholder fills the full content area
  // (the parent `.scroll` is a flex column so `flex-1` propagates).
  if (isLoading || (!isError && allDevices.length === 0)) {
    return (
      <>
        <PageHeader
          title="Devices"
          description="Every device seen on the network. Name a device to manage its routing and DNS filtering."
        />
        <DiscoveryPlaceholder
          cols={5}
          message="Searching for network devices"
          hint="Devices will appear as they are detected on the network."
        />
      </>
    );
  }

  return (
    <>
      <PageHeader
        title="Devices"
        description="Every device seen on the network. Name a device to manage its routing and DNS filtering."
      />
      <DeviceTable
        devices={visibleDevices}
        onDeviceClick={openDevice}
        groups={[
          { id: "all", label: "All", count: counts.all },
          { id: "managed", label: "Managed", count: counts.managed },
          { id: "unmanaged", label: "Unmanaged", count: counts.unmanaged },
          { id: "recent", label: "Recently seen", count: counts.recent },
        ]}
        activeGroup={group}
        onGroupChange={(id) => setGroup(id as GroupId)}
        searchValue={query}
        onSearchChange={setQuery}
        searchPlaceholder="Search by name, MAC, hostname, manufacturer or IP"
        emptyMessage={
          <NoDevicesFound
            neighbours={neighbours}
            onDeviceClick={openDevice}
            query={query}
            matchesAnywhere={matchesAnywhere}
          />
        }
      />
    </>
  );
}

interface NoDevicesFoundProps {
  neighbours: ReturnType<typeof findNeighbourMacs>;
  onDeviceClick: (id: string) => void;
  query: string;
  /** Whether the query matches any device at all, ignoring the active group. */
  matchesAnywhere: boolean;
}

/**
 * The device table's empty state.
 *
 * Rendered *inside* the grid via `DataTable`'s `emptyMessage`, so it reads as
 * the table having nothing to show rather than as a separate panel bolted
 * underneath it. Follows the message/hint shape `DiscoveryPlaceholder` uses.
 *
 * Three cases, in increasing order of how much help the admin needs:
 *
 *  1. No search at all — the plain filter message the page has always shown.
 *  2. A missed MAC search with near-miss candidates. Vendor apps frequently
 *     print a device's *Bluetooth* MAC while it associates over Wi-Fi under a
 *     different address, so the closest addresses are offered as guesses.
 *  3. A missed search with nothing close — explain the same trap and suggest a
 *     narrower search, so the admin can tell "absent" from "I searched for the
 *     wrong identifier" (the dead end in issue #1099).
 */
function NoDevicesFound({
  neighbours,
  onDeviceClick,
  query,
  matchesAnywhere,
}: NoDevicesFoundProps) {
  if (query.trim() === "") {
    return <>No devices match the current filter.</>;
  }

  // The device exists — this group just excludes it. Saying "no device
  // matches" here would send the admin hunting for a device that is right
  // there under another tab.
  if (matchesAnywhere) {
    return <>No devices in this group match “{query}”.</>;
  }

  return (
    <div className="mx-auto flex max-w-md flex-col items-center gap-2 py-2">
      <Text as="p" size="sm" weight="medium" className="text-ink">
        No device matches “{query}”.
      </Text>
      <Text as="p" size="sm" className="text-ink-3">
        {neighbours.length > 0
          ? "Vendor apps often show a device’s Bluetooth MAC, which usually differs from its Wi-Fi MAC by one or two. These are possible matches, not confirmed ones."
          : "If you copied this from a vendor’s app it may be the device’s Bluetooth MAC rather than the Wi-Fi MAC it uses here. Try just the first three pairs to list that manufacturer, or check the DHCP lease log."}
      </Text>
      {neighbours.length > 0 && (
        <div className="flex flex-wrap justify-center gap-2 pt-1">
          {neighbours.map(({ device, offset }) => (
            <Button
              key={device.id}
              variant="outline"
              size="sm"
              onClick={() => onDeviceClick(device.id)}
              data-testid={`neighbour-match-${device.mac}`}
            >
              <span className="font-mono">{device.mac}</span>
              <span className="text-ink-3">
                {deviceDisplayName(device)} ({offset > 0 ? "+" : ""}
                {offset})
              </span>
            </Button>
          ))}
        </div>
      )}
    </div>
  );
}
