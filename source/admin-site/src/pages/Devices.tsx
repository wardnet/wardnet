import { useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { PageHeader } from "@/components/compound/PageHeader";
import { DeviceTable } from "@/components/compound/DeviceTable";
import { DiscoveryPlaceholder } from "@/components/compound/DiscoveryPlaceholder";
import { useDevices } from "@wardnet/web";
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

function sortDevices(devices: Device[]): Device[] {
  return [...devices].sort((a, b) => {
    const nameA = (a.name ?? a.hostname ?? a.mac).toLowerCase();
    const nameB = (b.name ?? b.hostname ?? b.mac).toLowerCase();
    return nameA.localeCompare(nameB);
  });
}

/** Devices page with grouped, searchable device table. */
export default function Devices() {
  const { data, isLoading, isError } = useDevices();
  const navigate = useNavigate();
  const allDevices = useMemo(() => data?.devices ?? [], [data]);

  const [group, setGroup] = useState<GroupId>("all");
  const [query, setQuery] = useState("");
  // `now` is captured once at mount so the React Compiler doesn't flag
  // an impure call during render. The "Recently seen" window remains
  // accurate for the session — `useDevices` already polls so the rows
  // it filters from are themselves fresh.
  const [now] = useState(() => Date.now());

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
    return sortDevices(byGroup.filter((d) => matchesSearch(d, query)));
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
