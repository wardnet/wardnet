import type { Meta, StoryObj } from "@storybook/react-vite";
import { Pill } from "./pill";

const meta = {
  title: "Primitives/Pill",
  component: Pill,
  tags: ["autodocs"],
  argTypes: {
    variant: {
      control: "select",
      options: ["ok", "warn", "down", "info", "ghost"],
    },
  },
  args: { children: "Online", variant: "ok" },
} satisfies Meta<typeof Pill>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Ok: Story = { args: { variant: "ok", children: "Online" } };
export const Warn: Story = { args: { variant: "warn", children: "Degraded" } };
export const Down: Story = { args: { variant: "down", children: "Offline" } };
export const Info: Story = { args: { variant: "info", children: "Syncing" } };
export const Ghost: Story = { args: { variant: "ghost", children: "Idle" } };

export const AllVariants: Story = {
  render: () => (
    <div className="flex flex-wrap items-center gap-2">
      <Pill variant="ok">Online</Pill>
      <Pill variant="warn">Degraded</Pill>
      <Pill variant="down">Offline</Pill>
      <Pill variant="info">Syncing</Pill>
      <Pill variant="ghost">Idle</Pill>
    </div>
  ),
};
