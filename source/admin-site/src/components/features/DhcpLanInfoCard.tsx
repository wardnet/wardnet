import { useMemo } from "react";
import { Link } from "react-router";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Button } from "@wardnet/web";
import { Pill } from "@wardnet/web";
import { useDnsRecords } from "@wardnet/web";

/** Read-only explainer for the DHCP `.lan` integration. Owns its own data —
 *  counts the DHCP-sourced records ({hostname}.lan, auto-registered into the
 *  seeded `lan` zone). These are managed automatically and don't appear in the
 *  editable Records table. */
export function DhcpLanInfoCard() {
  const { data } = useDnsRecords();
  const dhcpRecordCount = useMemo(
    () => (data?.records ?? []).filter((r) => r.source === "dhcp").length,
    [data],
  );

  return (
    <Card>
      <CardHeader>
        <CardTitle>DHCP .lan names</CardTitle>
        <Pill variant="ghost">{dhcpRecordCount} auto-registered</Pill>
        <CardAction>
          <Button asChild variant="outline" size="sm">
            <Link to="/dhcp">Manage DHCP</Link>
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        <p className="text-sm text-ink-2">
          Devices that take a DHCP lease with a hostname are automatically
          resolvable as{" "}
          <span className="font-mono text-xs">&#123;hostname&#125;.lan</span> in
          the <span className="font-mono text-xs">lan</span> zone. These records
          are managed for you and aren&apos;t listed under Records — revoke a
          lease or remove the device on the DHCP page to clear one.
        </p>
      </CardContent>
    </Card>
  );
}
