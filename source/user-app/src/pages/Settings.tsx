import { SettingsIcon } from "lucide-react";
import { Placeholder } from "@/pages/Placeholder";

/** Settings — notification/push subscription lands in #594. */
export default function Settings() {
  return (
    <Placeholder
      title="Settings"
      description="Manage notifications and how Wardnet keeps you in the loop about your device."
      Icon={SettingsIcon}
    />
  );
}
