import type { LucideIcon } from "lucide-react";

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
      <h1 className="text-lg font-semibold text-ink">{title}</h1>
      <p className="max-w-md text-sm text-ink-3">{description}</p>
      <span className="rounded-full border border-line-strong px-3 py-1 text-xs font-medium text-ink-3">
        Coming soon
      </span>
    </div>
  );
}
