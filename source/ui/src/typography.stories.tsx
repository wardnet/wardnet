import type { Meta, StoryObj } from "@storybook/react-vite";
import { Text, Heading, type Variant, type Size } from "./primitives/text";

/**
 * Typography foundation — the numeric scale, the semantic variants, and the
 * <Text> / <Heading> primitive that surface them. Variants load transitively
 * through `@wardnet/styles` (preview.css → theme.css → styles.css →
 * typography.css), the same chain every app uses.
 */
const meta = {
  title: "Foundations/Typography",
  component: Text,
  parameters: { layout: "padded" },
} satisfies Meta<typeof Text>;

export default meta;
type Story = StoryObj<typeof meta>;

const SIZES: { size: Size; px: string }[] = [
  { size: "4xl", px: "32px" },
  { size: "3xl", px: "26px" },
  { size: "2xl", px: "22px" },
  { size: "xl", px: "18px" },
  { size: "lg", px: "16px" },
  { size: "base", px: "14px" },
  { size: "sm", px: "13px" },
  { size: "xs", px: "12px" },
  { size: "2xs", px: "11px" },
];

/** The numeric base scale, largest → smallest, with paired line-heights. */
export const ScaleRamp: Story = {
  render: () => (
    <div className="flex flex-col gap-3">
      {SIZES.map(({ size, px }) => (
        <div key={size} className="flex items-baseline gap-4">
          <Text variant="micro" as="span" className="w-24 shrink-0 tabular-nums">
            {size} · {px}
          </Text>
          <Text size={size} as="span">
            The quick brown fox jumps
          </Text>
        </div>
      ))}
    </div>
  ),
};

const VARIANT_SPECIMENS: { variant: Variant; sample: string }[] = [
  { variant: "label", sample: "Section label" },
  { variant: "body", sample: "Body copy — the default UI and prose voice." },
  { variant: "body-strong", sample: "Body strong — inline emphasis." },
  { variant: "caption", sample: "Caption — field help and secondary text." },
  { variant: "micro", sample: "Micro — tiny meta" },
  { variant: "metric", sample: "1,284" },
  { variant: "metric-unit", sample: "ms" },
  { variant: "mono", sample: "AA:BB:CC:DD:EE:FF" },
];

/** Each named voice rendered through its variant. */
export const Variants: Story = {
  render: () => (
    <div className="flex flex-col gap-4">
      {VARIANT_SPECIMENS.map(({ variant, sample }) => (
        <div key={variant} className="flex items-baseline gap-4">
          <Text variant="micro" as="span" className="w-28 shrink-0">
            {variant}
          </Text>
          <Text variant={variant}>{sample}</Text>
        </div>
      ))}
    </div>
  ),
};

/** Heading levels via the <Heading> alias. */
export const Headings: Story = {
  render: () => (
    <div className="flex flex-col gap-3">
      <Heading level={1}>Heading 1 — page title</Heading>
      <Heading level={2}>Heading 2 — section header</Heading>
      <Heading level={3}>Heading 3 — modal title</Heading>
    </div>
  ),
};

/** Override props in action: variant defaults, refined per call. */
export const PrimitiveUsage: Story = {
  render: () => (
    <div className="flex flex-col gap-3">
      <Text variant="body">variant="body" — plain body line</Text>
      <Text variant="body" weight="semibold">
        variant="body" weight="semibold" — 600 without a body-strong call-site
      </Text>
      <Text variant="h2" as="div">
        variant="h2" as="div" — section voice, non-heading element
      </Text>
      <Text size="lg" weight="medium">
        size="lg" weight="medium" — off-variant one-off, no Tailwind utilities
      </Text>
      <Text variant="metric">
        42<Text variant="metric-unit" as="span"> ms</Text>
      </Text>
    </div>
  ),
};

/** Recolour: a colour utility / the `color` prop beats the variant's baked ink. */
export const Recolour: Story = {
  render: () => (
    <div className="flex flex-col gap-3">
      <Text variant="label">label — baked ink-3</Text>
      <Text variant="label" color="danger">
        label color="danger" — utility wins (decision 4)
      </Text>
      <Text variant="label" className="text-accent">
        label className="text-accent" — raw utility wins too
      </Text>
      <Text variant="h2" color="accent">
        Heading recoloured via color prop
      </Text>
    </div>
  ),
};
