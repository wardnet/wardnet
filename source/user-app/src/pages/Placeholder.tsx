import type { LucideIcon } from "lucide-react";
import { Text } from "@wardnet/web";

interface PlaceholderProps {
  title: string;
  description: string;
  Icon: LucideIcon;
}

/** Stand-in for a feature tab that lands in a later User-PWA stage (#590–#594). */
export function Placeholder({ title, description, Icon }: PlaceholderProps) {
  return (
    <div className="flex flex-col items-center gap-4 px-5 py-16 text-center">
      <Icon className="size-12 text-ink-3/50" strokeWidth={1.5} />
      <Text as="h1" size="lg" weight="semibold" className="text-ink">
        {title}
      </Text>
      <Text as="p" size="sm" className="max-w-md text-ink-3">
        {description}
      </Text>
      <Text
        as="span"
        size="xs"
        weight="medium"
        className="rounded-full border border-line-strong px-3 py-1 text-ink-3"
      >
        Coming soon
      </Text>
    </div>
  );
}
