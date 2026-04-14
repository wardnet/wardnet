import { Card, CardContent } from "@/components/core/ui/card";
import { PageHeader } from "@/components/compound/PageHeader";

/** DNS server configuration page (admin only). Placeholder until Milestone 1g. */
export default function Dns() {
  return (
    <>
      <PageHeader title="DNS" />
      <Card>
        <CardContent className="py-10 text-center text-muted-foreground">
          DNS server configuration is coming soon. Once enabled, Wardnet will handle all DNS
          resolution for devices on the network.
        </CardContent>
      </Card>
    </>
  );
}
