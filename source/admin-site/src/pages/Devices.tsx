import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { PageHeader } from "@/components/compound/PageHeader";
import { DeviceTable } from "@/components/compound/DeviceTable";
import { DiscoveryPlaceholder } from "@/components/compound/DiscoveryPlaceholder";
import { deviceDisplayName, sortByLabel, useDevices } from "@wardnet/web";
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

function matchesSearch(d: Device, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  return (
    d.mac.toLowerCase().includes(needle) ||
    (d.hostname ?? "").toLowerCase().includes(needle) ||
    (d.last_ip ?? "").toLowerCase().includes(needle) ||
    (d.name ?? "").toLowerCase().includes(needle)
  );
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
      byGroup.filter((d) => matchesSearch(d, query)),
      deviceDisplayName,
    );
  }, [allDevices, group, query, now]);

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
        searchPlaceholder="Search by MAC, hostname or IP"
        emptyMessage="No devices match the current filter."
      />
    </>
  );
}
