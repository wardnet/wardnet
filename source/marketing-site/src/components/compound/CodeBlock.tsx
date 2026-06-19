import { cn } from "@/lib/utils";

interface CodeBlockProps {
  /** The code snippet to display. */
  code: string;
  className?: string;
}

/**
 * Styled code snippet block that consumes Forge's sunken surface and
 * monospace token. Used for non-interactive copy-paste snippets
 * (e.g. docker run commands on the Get Started page).
 */
export function CodeBlock({ code, className }: CodeBlockProps) {
  return (
    <pre
      className={cn(
        "overflow-x-auto rounded-lg border border-line bg-sunken px-6 py-4 t-size-sm leading-relaxed text-ink-2",
        className,
      )}
    >
      <code className="font-mono">{code}</code>
    </pre>
  );
}
