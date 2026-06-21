import type { Meta, StoryObj } from "@storybook/react-vite";
import { Label } from "./label";
import { Input } from "./input";

const meta = {
  title: "Primitives/Label",
  component: Label,
  tags: ["autodocs"],
  args: { children: "Hostname" },
} satisfies Meta<typeof Label>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const WithControl: Story = {
  render: () => (
    <div className="flex flex-col gap-1.5" style={{ maxWidth: 280 }}>
      <Label htmlFor="hostname">Hostname</Label>
      <Input id="hostname" placeholder="wardnet.local" />
    </div>
  ),
};
