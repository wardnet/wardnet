import type { ReactNode } from "react";
import { Text } from "@wardnet/web";

export function SectionLabel({ children }: { children: ReactNode }) {
  return (
    <Text as="p" size="2xs" weight="semibold" className="mb-2 px-1 uppercase tracking-wider text-ink-3">
      {children}
    </Text>
  );
}
