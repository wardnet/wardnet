import * as React from "react";
import { Command as CommandPrimitive } from "cmdk";
import { ChevronsUpDownIcon, SearchIcon } from "lucide-react";

import { Button } from "./button";
import { Popover, PopoverContent, PopoverTrigger } from "./popover";

type ComboboxContextValue = {
  value: string;
  onChange: (value: string) => void;
};

const ComboboxContext = React.createContext<ComboboxContextValue | null>(null);

function useComboboxContext() {
  const ctx = React.useContext(ComboboxContext);
  if (!ctx) throw new Error("ComboboxItem must be rendered inside a Combobox.");
  return ctx;
}

type ComboboxProps = {
  value: string;
  onChange: (value: string) => void;
  trigger: React.ReactNode;
  searchPlaceholder?: string;
  empty?: React.ReactNode;
  disabled?: boolean;
  children: React.ReactNode;
};

function Combobox({
  value,
  onChange,
  trigger,
  searchPlaceholder = "Search…",
  empty = "No results found.",
  disabled,
  children,
}: ComboboxProps) {
  const [open, setOpen] = React.useState(false);

  const ctx = React.useMemo<ComboboxContextValue>(
    () => ({
      value,
      onChange: (next) => {
        onChange(next);
        setOpen(false);
      },
    }),
    [value, onChange],
  );

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          role="combobox"
          aria-expanded={open}
          disabled={disabled}
          className="combobox-trigger"
        >
          {trigger}
          <ChevronsUpDownIcon className="combobox-chevron" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="combobox-content" align="start">
        <ComboboxContext.Provider value={ctx}>
          <CommandPrimitive>
            <div className="combobox-input">
              <SearchIcon />
              <CommandPrimitive.Input placeholder={searchPlaceholder} />
            </div>
            <CommandPrimitive.List className="combobox-list">
              <CommandPrimitive.Empty className="combobox-empty">
                {empty}
              </CommandPrimitive.Empty>
              {children}
            </CommandPrimitive.List>
          </CommandPrimitive>
        </ComboboxContext.Provider>
      </PopoverContent>
    </Popover>
  );
}

type ComboboxItemProps = {
  value: string;
  keywords?: string[];
  disabled?: boolean;
  children: React.ReactNode;
};

function ComboboxItem({
  value,
  keywords,
  disabled,
  children,
}: ComboboxItemProps) {
  const { value: selected, onChange } = useComboboxContext();
  const isSelected = selected === value;
  return (
    <CommandPrimitive.Item
      value={value}
      keywords={keywords}
      disabled={disabled}
      data-state={isSelected ? "checked" : "unchecked"}
      onSelect={() => onChange(value)}
      className="menu-item"
    >
      {children}
    </CommandPrimitive.Item>
  );
}

export { Combobox, ComboboxItem };
export type { ComboboxProps, ComboboxItemProps };
