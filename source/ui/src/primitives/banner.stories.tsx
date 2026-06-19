import type { Meta, StoryObj } from "@storybook/react-vite";
import { AlertTriangle, Info, WifiOff } from "lucide-react";
import { Banner } from "./banner";
import { Button } from "./button";

const meta = {
  title: "Primitives/Banner",
  component: Banner,
  tags: ["autodocs"],
  argTypes: {
    tone: { control: "select", options: ["down", "warn", "info"] },
    role: { control: "select", options: ["status", "alert"] },
  },
  args: { tone: "info", children: "Daemon reconnected — all systems nominal." },
} satisfies Meta<typeof Banner>;

export default meta;
type Story = StoryObj<typeof meta>;

export const InfoTone: Story = {
  args: { tone: "info", icon: <Info aria-hidden /> },
};

export const Warn: Story = {
  args: {
    tone: "warn",
    icon: <AlertTriangle aria-hidden />,
    children: "Backup is overdue — last snapshot was 9 days ago.",
  },
};

export const Down: Story = {
  args: {
    tone: "down",
    role: "alert",
    icon: <WifiOff aria-hidden />,
    children: "Connection to the daemon was lost.",
    actions: (
      <Button size="sm" variant="ghost">
        Retry
      </Button>
    ),
  },
};

export const AllTones: Story = {
  render: () => (
    <div className="flex flex-col gap-3">
      <Banner tone="info" icon={<Info aria-hidden />}>
        Advisory notice.
      </Banner>
      <Banner tone="warn" icon={<AlertTriangle aria-hidden />}>
        Something needs attention.
      </Banner>
      <Banner tone="down" role="alert" icon={<WifiOff aria-hidden />}>
        Critical interruption.
      </Banner>
    </div>
  ),
};
