import type { Meta, StoryObj } from "@storybook/react-vite";
import { Sparkline } from "./sparkline";

const meta = {
  title: "Primitives/Sparkline",
  component: Sparkline,
  tags: ["autodocs"],
  args: {
    values: [3, 5, 4, 8, 6, 9, 7, 11, 8, 12, 10, 14],
  },
  decorators: [
    (Story) => (
      <div style={{ width: 200, height: 48 }}>
        <Story />
      </div>
    ),
  ],
} satisfies Meta<typeof Sparkline>;

export default meta;
type Story = StoryObj<typeof meta>;

export const AreaLine: Story = {};

export const LineOnly: Story = { args: { area: false } };

export const Info: Story = { args: { color: "var(--info)" } };

export const Flat: Story = { args: { values: [5, 5, 5, 5, 5] } };
