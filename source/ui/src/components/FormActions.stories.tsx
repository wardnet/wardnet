import type { Meta, StoryObj } from "@storybook/react-vite";
import { FormActions } from "./FormActions";
import { Card, CardContent } from "../primitives/card";

const meta = {
  title: "Components/FormActions",
  component: FormActions,
  tags: ["autodocs"],
  decorators: [
    (Story) => (
      <Card style={{ maxWidth: 420 }}>
        <CardContent>Form body…</CardContent>
        <Story />
      </Card>
    ),
  ],
  args: {
    primaryLabel: "Save",
    onPrimary: () => {},
  },
} satisfies Meta<typeof FormActions>;

export default meta;
type Story = StoryObj<typeof meta>;

export const PrimaryOnly: Story = {};

export const WithSecondary: Story = {
  args: { secondaryLabel: "Cancel", onSecondary: () => {} },
};

export const Destructive: Story = {
  args: {
    primaryLabel: "Delete",
    primaryVariant: "destructive",
    secondaryLabel: "Cancel",
    onSecondary: () => {},
  },
};
