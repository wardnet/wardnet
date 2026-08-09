import { Link } from "react-router";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Button } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { Pill } from "@wardnet/web";

interface DhcpLanInfoCardProps {
  /** Count of DHCP-sourced records ({hostname}.lan, auto-registered into the
   *  seeded `lan` zone), derived by the owning page from the shared records
   *  query. */
  dhcpRecordCount: number;
}

/** Read-only explainer for the DHCP `.lan` integration. The auto-registered
 *  records are managed automatically and don't appear in the editable Records
 *  table. Pure presentation — the owning page passes the derived count in. */
export function DhcpLanInfoCard({ dhcpRecordCount }: DhcpLanInfoCardProps) {
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
        <Text as="p" size="sm" className="text-ink-2">
          Devices that take a DHCP lease with a hostname are automatically
          resolvable as{" "}
          <Text size="xs" className="font-mono">
            &#123;hostname&#125;.lan
          </Text>{" "}
          in the{" "}
          <Text size="xs" className="font-mono">
            lan
          </Text>{" "}
          zone. These records are managed for you and aren&apos;t listed under
          Records - revoke a lease or remove the device on the DHCP page to
          clear one.
        </Text>
      </CardContent>
    </Card>
  );
}
