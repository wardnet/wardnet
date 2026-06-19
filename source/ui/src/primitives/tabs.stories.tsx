import type { Meta, StoryObj } from "@storybook/react-vite";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "./tabs";

const meta = {
  title: "Primitives/Tabs",
  component: Tabs,
  tags: ["autodocs"],
} satisfies Meta<typeof Tabs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <Tabs defaultValue="overview">
      <TabsList>
        <TabsTrigger value="overview">Overview</TabsTrigger>
        <TabsTrigger value="devices">Devices</TabsTrigger>
        <TabsTrigger value="logs">Logs</TabsTrigger>
      </TabsList>
      <TabsContent value="overview" style={{ paddingTop: 12 }}>
        Overview panel.
      </TabsContent>
      <TabsContent value="devices" style={{ paddingTop: 12 }}>
        Devices panel.
      </TabsContent>
      <TabsContent value="logs" style={{ paddingTop: 12 }}>
        Logs panel.
      </TabsContent>
    </Tabs>
  ),
};
