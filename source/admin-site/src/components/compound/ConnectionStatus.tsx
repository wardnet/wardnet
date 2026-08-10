import { Text } from "@wardnet/web";

interface ConnectionStatusProps {
  /** True while the daemon status is still being fetched. */
  isLoading: boolean;
  /** Whether the daemon answered the last status probe. */
  reachable: boolean;
}

/** Traffic-light status indicator shown in the sidebar footer. The
 *  daemon version is rendered separately under the brand mark (see
 *  Sidebar), so this row stays a single line. Pure presentation — the
 *  shell layout wires the status hook and passes the state in. */
export function ConnectionStatus({
  isLoading,
  reachable,
}: ConnectionStatusProps) {
  const color = isLoading ? "bg-warn" : reachable ? "bg-accent" : "bg-danger";
  const label = isLoading
    ? "Connecting…"
    : reachable
      ? "Connected"
      : "Disconnected";

  return (
    <div className="flex items-center gap-2">
      <span className={`inline-block size-2 rounded-full ${color}`} />
      <Text as="span" size="xs" className="text-side-ink/70">
        {label}
      </Text>
    </div>
  );
}
