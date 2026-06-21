import type { Meta, StoryObj } from "@storybook/react-vite";
import { StatTile } from "./stat-tile";
import { Sparkline } from "./sparkline";
import { Pill } from "./pill";

const meta = {
  title: "Primitives/StatTile",
  component: StatTile,
  tags: ["autodocs"],
  args: { label: "Blocked queries", value: "12,481" },
} satisfies Meta<typeof StatTile>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Basic: Story = {};

export const WithUnit: Story = {
  args: { label: "CPU", value: "37", unit: "%" },
};

export const WithSub: Story = {
  args: {
    label: "Uptime",
    value: "12d 4h",
    sub: "since last reboot",
  },
};

export const WithBar: Story = {
  args: { label: "Disk", value: "68", unit: "%", bar: 68 },
};

export const WithPillAndSpark: Story = {
  args: {
    label: "Throughput",
    value: "4.2",
    unit: "MB/s",
    pill: <Pill variant="ok">live</Pill>,
    spark: (
      <Sparkline
        values={[3, 5, 4, 8, 6, 9, 7, 11, 8, 12]}
        color="var(--accent)"
      />
    ),
  },
};

export const Grid: Story = {
  render: () => (
    <div className="grid grid-cols-2 gap-3" style={{ maxWidth: 480 }}>
      <StatTile label="Blocked" value="12,481" />
      <StatTile label="CPU" value="37" unit="%" bar={37} />
      <StatTile label="Memory" value="61" unit="%" bar={61} />
      <StatTile
        label="Throughput"
        value="4.2"
        unit="MB/s"
        spark={<Sparkline values={[2, 4, 3, 6, 5, 8, 7, 10]} />}
      />
    </div>
  ),
};
