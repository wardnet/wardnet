import * as React from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "./select";

const meta = {
  title: "Primitives/Select",
  component: Select,
  tags: ["autodocs"],
} satisfies Meta<typeof Select>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => {
    const [value, setValue] = React.useState("");
    return (
      <Select value={value} onValueChange={setValue}>
        <SelectTrigger style={{ minWidth: 200 }}>
          <SelectValue placeholder="Pick a region…" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="eu-west">EU West (Amsterdam)</SelectItem>
          <SelectItem value="us-east">US East (New York)</SelectItem>
          <SelectItem value="ap-south">AP South (Singapore)</SelectItem>
        </SelectContent>
      </Select>
    );
  },
};
