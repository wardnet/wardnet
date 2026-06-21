import * as React from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { Combobox, ComboboxItem } from "./combobox";

const meta = {
  title: "Primitives/Combobox",
  component: Combobox,
  tags: ["autodocs"],
  args: { value: "", onChange: () => {}, trigger: null, children: null },
} satisfies Meta<typeof Combobox>;

export default meta;
type Story = StoryObj<typeof meta>;

const REGIONS = [
  { value: "eu-west", label: "EU West (Amsterdam)" },
  { value: "us-east", label: "US East (New York)" },
  { value: "ap-south", label: "AP South (Singapore)" },
  { value: "sa-east", label: "SA East (São Paulo)" },
];

export const Default: Story = {
  render: () => {
    const [value, setValue] = React.useState("");
    const selected = REGIONS.find((r) => r.value === value);
    return (
      <div style={{ width: 260 }}>
        <Combobox
          value={value}
          onChange={setValue}
          trigger={<span>{selected ? selected.label : "Select region…"}</span>}
          searchPlaceholder="Search regions…"
        >
          {REGIONS.map((r) => (
            <ComboboxItem key={r.value} value={r.value} keywords={[r.label]}>
              {r.label}
            </ComboboxItem>
          ))}
        </Combobox>
      </div>
    );
  },
};
