import type { Meta, StoryObj } from "@storybook/react-vite";
import {
  AlertModal,
  AlertModalTrigger,
  AlertModalContent,
  AlertModalHeader,
  AlertModalTitle,
  AlertModalDescription,
  AlertModalBody,
  AlertModalFooter,
  AlertModalAction,
  AlertModalCancel,
} from "./alert-modal";
import { Button } from "./button";

const meta = {
  title: "Primitives/AlertModal",
  component: AlertModal,
  tags: ["autodocs"],
} satisfies Meta<typeof AlertModal>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Destructive: Story = {
  render: () => (
    <AlertModal>
      <AlertModalTrigger asChild>
        <Button variant="destructive">Delete device</Button>
      </AlertModalTrigger>
      <AlertModalContent>
        <AlertModalHeader>
          <AlertModalTitle>Delete this device?</AlertModalTitle>
        </AlertModalHeader>
        <AlertModalBody>
          <AlertModalDescription>
            This removes the device and its history. This action cannot be
            undone.
          </AlertModalDescription>
        </AlertModalBody>
        <AlertModalFooter>
          <AlertModalCancel asChild>
            <Button variant="ghost">Cancel</Button>
          </AlertModalCancel>
          <AlertModalAction asChild>
            <Button variant="destructive">Delete</Button>
          </AlertModalAction>
        </AlertModalFooter>
      </AlertModalContent>
    </AlertModal>
  ),
};
