import * as React from "react";
import { Tabs as RadixTabs } from "radix-ui";
import { clsx } from "clsx";
import s from "./tabs.module.css";

type TabsProps = React.ComponentProps<typeof RadixTabs.Root>;

function Tabs(props: TabsProps) {
  return <RadixTabs.Root {...props} />;
}

type TabsListProps = React.ComponentProps<typeof RadixTabs.List>;

function TabsList({ className, ...props }: TabsListProps) {
  return <RadixTabs.List className={clsx(s.tabs, className)} {...props} />;
}

type TabsTriggerProps = React.ComponentProps<typeof RadixTabs.Trigger>;

function TabsTrigger(props: TabsTriggerProps) {
  return <RadixTabs.Trigger {...props} />;
}

type TabsContentProps = React.ComponentProps<typeof RadixTabs.Content>;

function TabsContent(props: TabsContentProps) {
  return <RadixTabs.Content {...props} />;
}

export { Tabs, TabsList, TabsTrigger, TabsContent };
export type { TabsProps, TabsListProps, TabsTriggerProps, TabsContentProps };
