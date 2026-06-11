import { ShieldCheckIcon } from "lucide-react";
import { Placeholder } from "@/pages/Placeholder";

/** Route verification ("check my route") — lands in #593. */
export default function Verify() {
  return (
    <Placeholder
      title="Verify your route"
      description="Confirm your VPN is working by checking the public IP and location your device is using right now."
      Icon={ShieldCheckIcon}
    />
  );
}
