// @wardnet/ui — the Wardnet design system.
//
// Domain-agnostic React primitives + brand components. This package has NO
// dependency on @wardnet/js or any app domain code — that boundary is what
// makes it a design system. Feature components (LoginForm, AppHeader, …) live
// in @wardnet/web, one layer up, and compose these.

// UI Primitives
export {
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
} from "./primitives/alert-modal";
export type {
  AlertModalProps,
  AlertModalTriggerProps,
  AlertModalContentProps,
  AlertModalTitleProps,
  AlertModalDescriptionProps,
  AlertModalActionProps,
  AlertModalCancelProps,
} from "./primitives/alert-modal";
export { Banner } from "./primitives/banner";
export type { BannerProps, BannerTone } from "./primitives/banner";
export { Button } from "./primitives/button";
export type { ButtonProps } from "./primitives/button";
export {
  Card,
  CardHeader,
  CardTitle,
  CardSubtitle,
  CardAction,
  CardContent,
  CardFooter,
} from "./primitives/card";
export { Combobox, ComboboxItem } from "./primitives/combobox";
export type { ComboboxProps, ComboboxItemProps } from "./primitives/combobox";
export {
  Drawer,
  DrawerTrigger,
  DrawerContent,
  DrawerTitle,
  DrawerDescription,
  DrawerClose,
} from "./primitives/drawer";
export type {
  DrawerProps,
  DrawerTriggerProps,
  DrawerContentProps,
  DrawerTitleProps,
  DrawerDescriptionProps,
  DrawerCloseProps,
} from "./primitives/drawer";
export {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from "./primitives/dropdown-menu";
export type {
  DropdownMenuProps,
  DropdownMenuTriggerProps,
  DropdownMenuContentProps,
  DropdownMenuItemProps,
  DropdownMenuSeparatorProps,
} from "./primitives/dropdown-menu";
export { Field } from "./primitives/field";
export type { FieldProps } from "./primitives/field";
export {
  Form,
  Validator,
  FormContext,
  useFormContext,
} from "./primitives/form";
export type { Validate } from "./primitives/form";
export { Input } from "./primitives/input";
export type { InputProps } from "./primitives/input";
export { Label } from "./primitives/label";
export type { LabelProps } from "./primitives/label";
export {
  Modal,
  ModalTrigger,
  ModalContent,
  ModalHeader,
  ModalTitle,
  ModalDescription,
  ModalBody,
  ModalFooter,
  ModalClose,
} from "./primitives/modal";
export type {
  ModalProps,
  ModalTriggerProps,
  ModalContentProps,
} from "./primitives/modal";
export { Pill } from "./primitives/pill";
export type { PillProps } from "./primitives/pill";
export { Popover, PopoverTrigger, PopoverContent } from "./primitives/popover";
export type {
  PopoverProps,
  PopoverTriggerProps,
  PopoverContentProps,
} from "./primitives/popover";
export {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "./primitives/select";
export { Sparkline } from "./primitives/sparkline";
export type { SparklineProps } from "./primitives/sparkline";
export { StatTile } from "./primitives/stat-tile";
export type { StatTileProps } from "./primitives/stat-tile";
export { Tabs, TabsList, TabsTrigger, TabsContent } from "./primitives/tabs";
export type {
  TabsProps,
  TabsListProps,
  TabsTriggerProps,
  TabsContentProps,
} from "./primitives/tabs";
export { Text, Heading } from "./primitives/text";
export type {
  TextProps,
  HeadingProps,
  Variant,
  Size,
  Weight,
  Color,
} from "./primitives/text";
export { Textarea } from "./primitives/textarea";
export type { TextareaProps } from "./primitives/textarea";
export { Toggle } from "./primitives/toggle";
export type { ToggleProps } from "./primitives/toggle";

// Brand + generic compositions
export { Logo } from "./components/Logo";
export { FormActions } from "./components/FormActions";
export { SegmentedTabs } from "./components/SegmentedTabs";
export type { SegmentedTab } from "./components/SegmentedTabs";
