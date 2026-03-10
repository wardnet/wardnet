import { cn } from "@/lib/utils";

interface CodeBlockProps {
  /** The code snippet to display. */
  code: string;
  className?: string;
}

/**
 * Styled code snippet block with a dark background and monospace font.
 */
export function CodeBlock({ code, className }: CodeBlockProps) {
  return (
    <pre
      className={cn(
        "overflow-x-auto rounded-lg bg-[oklch(0.16_0.02_270)] px-6 py-4 text-sm leading-relaxed text-gray-200",
        className,
      )}
    >
      <code className="font-mono">{code}</code>
    </pre>
  );
}
