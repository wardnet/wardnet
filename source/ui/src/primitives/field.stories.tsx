import type { Meta, StoryObj } from "@storybook/react-vite";
import { Field } from "./field";
import { Input } from "./input";
import { Toggle } from "./toggle";

const meta = {
  title: "Primitives/Field",
  component: Field,
  tags: ["autodocs"],
} satisfies Meta<typeof Field>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Column: Story = {
  render: () => (
    <div style={{ maxWidth: 320 }}>
      <Field label="Hostname" htmlFor="h" help="Used to reach the gateway.">
        <Input id="h" placeholder="wardnet.local" />
      </Field>
    </div>
  ),
};

export const WithError: Story = {
  render: () => (
    <div style={{ maxWidth: 320 }}>
      <Field label="Hostname" htmlFor="he" error="Hostname is required.">
        <Input id="he" />
      </Field>
    </div>
  ),
};

export const Row: Story = {
  render: () => (
    <div style={{ maxWidth: 360 }}>
      <Field label="Block ads" direction="row" help="Filter known ad domains.">
        <Toggle defaultChecked />
      </Field>
    </div>
  ),
};

export const ReadOnly: Story = {
  render: () => (
    <div style={{ maxWidth: 320 }}>
      <Field label="MAC address" editing={false} value="a4:c3:f0:12:9b:7e" />
    </div>
  ),
};
