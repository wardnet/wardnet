import { useState } from "react";
import { Button } from "@wardnet/web";
import {
  Drawer,
  DrawerContent,
  DrawerTitle,
  DrawerTrigger,
} from "@wardnet/web";
import { Sidebar, type SidebarProps } from "./Sidebar";

/** The sidebar's data props, forwarded verbatim — the drawer only adds the
 *  open/close state and the on-navigate auto-close. */
type MobileMenuProps = Omit<SidebarProps, "onNavigate">;

/** Hamburger menu with slide-in sidebar for mobile viewports. */
export function MobileMenu(props: MobileMenuProps) {
  const [open, setOpen] = useState(false);

  return (
    <Drawer open={open} onOpenChange={setOpen}>
      <DrawerTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="topbar__menu"
          data-testid="mobile-menu-trigger"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <line x1="4" x2="20" y1="12" y2="12" />
            <line x1="4" x2="20" y1="6" y2="6" />
            <line x1="4" x2="20" y1="18" y2="18" />
          </svg>
          <span className="sr-only">Toggle menu</span>
        </Button>
      </DrawerTrigger>
      <DrawerContent side="left">
        <DrawerTitle className="sr-only">Navigation</DrawerTitle>
        <Sidebar {...props} onNavigate={() => setOpen(false)} />
      </DrawerContent>
    </Drawer>
  );
}
