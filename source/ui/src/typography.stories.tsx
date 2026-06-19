import type { Meta, StoryObj } from "@storybook/react-vite";
import { Text, Heading, type Role, type Size } from "./primitives/text";

/**
 * Typography foundation — the numeric scale, the semantic roles, and the
 * <Text> / <Heading> primitive that surface them. Roles load transitively
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
          <Text role="micro" as="span" className="w-24 shrink-0 tabular-nums">
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

const ROLE_SPECIMENS: { role: Role; sample: string }[] = [
  { role: "label", sample: "Section label" },
  { role: "body", sample: "Body copy — the default UI and prose voice." },
  { role: "body-strong", sample: "Body strong — inline emphasis." },
  { role: "caption", sample: "Caption — field help and secondary text." },
  { role: "micro", sample: "Micro — tiny meta" },
  { role: "metric", sample: "1,284" },
  { role: "metric-unit", sample: "ms" },
  { role: "mono", sample: "AA:BB:CC:DD:EE:FF" },
];

/** Each named voice rendered through its role. */
export const Roles: Story = {
  render: () => (
    <div className="flex flex-col gap-4">
      {ROLE_SPECIMENS.map(({ role, sample }) => (
        <div key={role} className="flex items-baseline gap-4">
          <Text role="micro" as="span" className="w-28 shrink-0">
            {role}
          </Text>
          <Text role={role}>{sample}</Text>
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

/** Override props in action: role defaults, refined per call. */
export const PrimitiveUsage: Story = {
  render: () => (
    <div className="flex flex-col gap-3">
      <Text role="body">role="body" — plain body line</Text>
      <Text role="body" weight="semibold">
        role="body" weight="semibold" — 600 without a body-strong call-site
      </Text>
      <Text role="h2" as="div">
        role="h2" as="div" — section voice, non-heading element
      </Text>
      <Text size="lg" weight="medium">
        size="lg" weight="medium" — off-role one-off, no Tailwind utilities
      </Text>
      <Text role="metric">
        42<Text role="metric-unit" as="span"> ms</Text>
      </Text>
    </div>
  ),
};

/** Recolour: a colour utility / the `color` prop beats the role's baked ink. */
export const Recolour: Story = {
  render: () => (
    <div className="flex flex-col gap-3">
      <Text role="label">label — baked ink-3</Text>
      <Text role="label" color="danger">
        label color="danger" — utility wins (decision 4)
      </Text>
      <Text role="label" className="text-accent">
        label className="text-accent" — raw utility wins too
      </Text>
      <Text role="h2" color="accent">
        Heading recoloured via color prop
      </Text>
    </div>
  ),
};
