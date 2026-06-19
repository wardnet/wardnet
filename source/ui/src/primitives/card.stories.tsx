import type { Meta, StoryObj } from "@storybook/react-vite";
import {
  Card,
  CardHeader,
  CardTitle,
  CardSubtitle,
  CardAction,
  CardContent,
  CardFooter,
} from "./card";
import { Button } from "./button";
import { Pill } from "./pill";

const meta = {
  title: "Primitives/Card",
  component: Card,
  tags: ["autodocs"],
} satisfies Meta<typeof Card>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Plain: Story = {
  render: () => (
    <Card style={{ maxWidth: 420 }}>
      <CardContent>A bare card with padded content.</CardContent>
    </Card>
  ),
};

export const WithHeaderAndFooter: Story = {
  render: () => (
    <Card style={{ maxWidth: 420 }}>
      <CardHeader>
        <CardTitle>Devices</CardTitle>
        <CardAction>
          <Pill variant="ok">8 online</Pill>
        </CardAction>
        <CardSubtitle>Connected clients on the LAN.</CardSubtitle>
      </CardHeader>
      <CardContent>Table or body content goes here.</CardContent>
      <CardFooter>
        <Button variant="ghost">Cancel</Button>
        <Button>Save</Button>
      </CardFooter>
    </Card>
  ),
};

export const Error: Story = {
  render: () => (
    <Card style={{ maxWidth: 420 }} error="Failed to load device list." />
  ),
};
