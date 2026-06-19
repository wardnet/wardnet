import * as React from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { SegmentedTabs } from "./SegmentedTabs";

const meta = {
  title: "Components/SegmentedTabs",
  component: SegmentedTabs,
  tags: ["autodocs"],
  args: { tabs: [], activeId: "", onChange: () => {} },
} satisfies Meta<typeof SegmentedTabs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => {
    const [active, setActive] = React.useState("all");
    return (
      <SegmentedTabs
        activeId={active}
        onChange={setActive}
        tabs={[
          { id: "all", label: "All" },
          { id: "wired", label: "Wired" },
          { id: "wifi", label: "Wi-Fi" },
        ]}
      />
    );
  },
};

export const WithCounts: Story = {
  render: () => {
    const [active, setActive] = React.useState("all");
    return (
      <SegmentedTabs
        activeId={active}
        onChange={setActive}
        tabs={[
          { id: "all", label: "All", count: 24 },
          { id: "online", label: "Online", count: 18 },
          { id: "offline", label: "Offline", count: 6 },
        ]}
      />
    );
  },
};
