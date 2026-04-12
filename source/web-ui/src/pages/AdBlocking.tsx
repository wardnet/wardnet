import { Card, CardContent } from "@/components/core/ui/card";
import { PageHeader } from "@/components/compound/PageHeader";

/** Ad blocking configuration page (admin only). Placeholder until Milestone 1g. */
export default function AdBlocking() {
  return (
    <>
      <PageHeader title="Ad Blocking" />
      <Card>
        <CardContent className="py-10 text-center text-muted-foreground">
          Network-wide ad blocking is coming soon. Wardnet will filter ads at the DNS level for all
          devices, with per-device control — like Pi-hole, built in.
        </CardContent>
      </Card>
    </>
  );
}
