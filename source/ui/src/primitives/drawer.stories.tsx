import type { Meta, StoryObj } from "@storybook/react-vite";
import {
  Drawer,
  DrawerTrigger,
  DrawerContent,
  DrawerTitle,
  DrawerDescription,
  DrawerClose,
} from "./drawer";
import { Button } from "./button";

const meta = {
  title: "Primitives/Drawer",
  component: Drawer,
  tags: ["autodocs"],
} satisfies Meta<typeof Drawer>;

export default meta;
type Story = StoryObj<typeof meta>;

function Demo({ side }: { side: "left" | "right" | "bottom" }) {
  return (
    <Drawer>
      <DrawerTrigger asChild>
        <Button>Open {side}</Button>
      </DrawerTrigger>
      <DrawerContent side={side} style={{ padding: 20 }}>
        <DrawerTitle>Filters</DrawerTitle>
        <DrawerDescription>Refine the device list.</DrawerDescription>
        <div style={{ marginTop: "auto", paddingTop: 16 }}>
          <DrawerClose asChild>
            <Button variant="ghost">Close</Button>
          </DrawerClose>
        </div>
      </DrawerContent>
    </Drawer>
  );
}

export const Left: Story = { render: () => <Demo side="left" /> };
export const Right: Story = { render: () => <Demo side="right" /> };
export const Bottom: Story = { render: () => <Demo side="bottom" /> };
