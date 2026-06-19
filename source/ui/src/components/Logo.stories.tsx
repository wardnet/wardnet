import type { Meta, StoryObj } from "@storybook/react-vite";
import { Logo } from "./Logo";

const meta = {
  title: "Components/Logo",
  component: Logo,
  tags: ["autodocs"],
  argTypes: {
    variant: { control: "select", options: ["light", "dark"] },
    height: { control: { type: "range", min: 16, max: 96, step: 4 } },
  },
  args: { height: 32, variant: "dark", tagline: false },
} satisfies Meta<typeof Logo>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Dark: Story = { args: { variant: "dark" } };
export const Light: Story = { args: { variant: "light" } };
export const WithTagline: Story = { args: { tagline: true, height: 48 } };
