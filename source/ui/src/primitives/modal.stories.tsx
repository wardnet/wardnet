import type { Meta, StoryObj } from "@storybook/react-vite";
import {
  Modal,
  ModalTrigger,
  ModalContent,
  ModalHeader,
  ModalTitle,
  ModalDescription,
  ModalBody,
  ModalFooter,
  ModalClose,
} from "./modal";
import { Button } from "./button";
import { Field } from "./field";
import { Input } from "./input";

const meta = {
  title: "Primitives/Modal",
  component: Modal,
  tags: ["autodocs"],
} satisfies Meta<typeof Modal>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <Modal>
      <ModalTrigger asChild>
        <Button>Edit device</Button>
      </ModalTrigger>
      <ModalContent>
        <ModalHeader>
          <ModalTitle>Edit device</ModalTitle>
        </ModalHeader>
        <ModalBody>
          <ModalDescription>
            Update the device's friendly name.
          </ModalDescription>
          <Field label="Name" htmlFor="dev-name">
            <Input id="dev-name" defaultValue="Living-room TV" />
          </Field>
        </ModalBody>
        <ModalFooter>
          <ModalClose asChild>
            <Button variant="ghost">Cancel</Button>
          </ModalClose>
          <ModalClose asChild>
            <Button>Save</Button>
          </ModalClose>
        </ModalFooter>
      </ModalContent>
    </Modal>
  ),
};
