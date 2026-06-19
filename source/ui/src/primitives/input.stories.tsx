import type { Meta, StoryObj } from "@storybook/react-vite";
import { Input } from "./input";

const meta = {
  title: "Primitives/Input",
  component: Input,
  tags: ["autodocs"],
  args: { placeholder: "e.g. wardnet.local" },
} satisfies Meta<typeof Input>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Text: Story = {};
export const Disabled: Story = { args: { disabled: true, value: "locked" } };

export const Password: Story = {
  args: { type: "password", placeholder: "Admin password" },
};
