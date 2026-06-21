import type { Meta, StoryObj } from "@storybook/react-vite";
import { Popover, PopoverTrigger, PopoverContent } from "./popover";
import { Button } from "./button";

const meta = {
  title: "Primitives/Popover",
  component: Popover,
  tags: ["autodocs"],
} satisfies Meta<typeof Popover>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="ghost">Details</Button>
      </PopoverTrigger>
      <PopoverContent>
        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <strong>Living-room TV</strong>
          <span style={{ color: "var(--ink-3)", fontSize: 12 }}>
            192.168.1.42 · online 4h
          </span>
        </div>
      </PopoverContent>
    </Popover>
  ),
};
