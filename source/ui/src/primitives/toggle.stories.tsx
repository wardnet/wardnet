import * as React from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { Toggle } from "./toggle";

const meta = {
  title: "Primitives/Toggle",
  component: Toggle,
  tags: ["autodocs"],
} satisfies Meta<typeof Toggle>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Off: Story = { args: { defaultChecked: false } };
export const On: Story = { args: { defaultChecked: true } };
export const Disabled: Story = {
  args: { defaultChecked: true, disabled: true },
};

export const Controlled: Story = {
  render: () => {
    const [on, setOn] = React.useState(true);
    return (
      <label className="flex items-center gap-2">
        <Toggle checked={on} onCheckedChange={setOn} />
        <span>{on ? "Enabled" : "Disabled"}</span>
      </label>
    );
  },
};
