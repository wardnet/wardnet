import { ChartColumnIcon } from "lucide-react";
import { Placeholder } from "@/pages/Placeholder";

/** Personal DNS stats — lands in #592. */
export default function Stats() {
  return (
    <Placeholder
      title="DNS stats"
      description="See what your device has been doing — total DNS queries and more, scoped just to you."
      Icon={ChartColumnIcon}
    />
  );
}
