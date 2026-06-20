import type { Meta, StoryObj } from "@storybook/react-vite";
import { Text } from "./text";

const meta = {
  title: "Primitives/Text",
  component: Text,
  tags: ["autodocs"],
  argTypes: {
    variant: {
      control: "select",
      options: [
        "label",
        "body",
        "body-strong",
        "caption",
        "micro",
        "metric",
        "metric-unit",
        "mono",
        "h1",
        "h2",
        "h3",
      ],
    },
    size: {
      control: "select",
      options: ["2xs", "xs", "sm", "base", "lg", "xl", "2xl", "3xl", "4xl"],
    },
    weight: {
      control: "select",
      options: ["normal", "medium", "semibold", "bold"],
    },
  },
  args: { variant: "body", children: "The quick brown fox jumps" },
} satisfies Meta<typeof Text>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Body: Story = { args: { variant: "body" } };
export const Label: Story = {
  args: { variant: "label", children: "Section label" },
};
export const Caption: Story = {
  args: { variant: "caption", children: "Field help and secondary text." },
};
export const Mono: Story = {
  args: { variant: "mono", children: "AA:BB:CC:DD:EE:FF" },
};

export const AllVariants: Story = {
  render: () => (
    <div className="flex flex-col gap-3">
      <Text variant="label">Label — section label</Text>
      <Text variant="body">Body — default UI and prose voice</Text>
      <Text variant="body-strong">Body strong — inline emphasis</Text>
      <Text variant="caption">Caption — field help and secondary</Text>
      <Text variant="micro">Micro — tiny meta</Text>
      <Text variant="mono">Mono — AA:BB:CC:DD:EE:FF</Text>
      <div className="flex items-baseline gap-1">
        <Text variant="metric">1,284</Text>
        <Text variant="metric-unit">ms</Text>
      </div>
    </div>
  ),
};

export const Sizes: Story = {
  render: () => (
    <div className="flex flex-col gap-2">
      {(["4xl", "3xl", "2xl", "xl", "lg", "base", "sm", "xs", "2xs"] as const).map(
        (size) => (
          <Text key={size} size={size}>
            {size} — The quick brown fox
          </Text>
        ),
      )}
    </div>
  ),
};

export const Weights: Story = {
  render: () => (
    <div className="flex flex-col gap-2">
      {(["normal", "medium", "semibold", "bold"] as const).map((weight) => (
        <Text key={weight} weight={weight}>
          {weight} — The quick brown fox
        </Text>
      ))}
    </div>
  ),
};

export const Recolour: Story = {
  render: () => (
    <div className="flex flex-col gap-2">
      <Text variant="label">label — baked ink-3</Text>
      <Text variant="label" color="danger">
        label color="danger"
      </Text>
      <Text variant="label" color="accent">
        label color="accent"
      </Text>
    </div>
  ),
};
