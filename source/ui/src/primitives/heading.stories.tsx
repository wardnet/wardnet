import type { Meta, StoryObj } from "@storybook/react-vite";
import { Heading } from "./text";

const meta = {
  title: "Primitives/Heading",
  component: Heading,
  tags: ["autodocs"],
  argTypes: {
    level: { control: "select", options: [1, 2, 3] },
  },
  args: { level: 1, children: "The quick brown fox" },
} satisfies Meta<typeof Heading>;

export default meta;
type Story = StoryObj<typeof meta>;

export const H1: Story = { args: { level: 1, children: "Heading 1 — page title" } };
export const H2: Story = {
  args: { level: 2, children: "Heading 2 — section header" },
};
export const H3: Story = {
  args: { level: 3, children: "Heading 3 — modal title" },
};

export const AllLevels: Story = {
  render: () => (
    <div className="flex flex-col gap-3">
      <Heading level={1}>Heading 1 — page title</Heading>
      <Heading level={2}>Heading 2 — section header</Heading>
      <Heading level={3}>Heading 3 — modal title</Heading>
    </div>
  ),
};
