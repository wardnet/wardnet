import { useState } from "react";
import { Button } from "@/components/core/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/core/ui/popover";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/core/ui/command";
import { CheckIcon, ChevronsUpDownIcon } from "lucide-react";
import { countryFlag } from "@/lib/country";
import type { CountryInfo } from "@wardnet/js";
import { cn } from "@/lib/utils";

interface CountryComboboxProps {
  countries: CountryInfo[];
  value: string;
  onChange: (code: string) => void;
  placeholder?: string;
  disabled?: boolean;
}

/** Searchable country picker using a combobox (Popover + Command). */
export function CountryCombobox({
  countries,
  value,
  onChange,
  placeholder = "All countries",
  disabled,
}: CountryComboboxProps) {
  const [open, setOpen] = useState(false);
  const selected = countries.find((c) => c.code === value);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          role="combobox"
          aria-expanded={open}
          disabled={disabled}
          className="w-full justify-between font-normal"
        >
          {selected ? (
            <span className="flex items-center gap-2">
              <span>{countryFlag(selected.code)}</span>
              {selected.name}
            </span>
          ) : (
            <span className="text-muted-foreground">{placeholder}</span>
          )}
          <ChevronsUpDownIcon className="ml-2 size-4 shrink-0 opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-(--radix-popover-trigger-width) p-0" align="start">
        <Command>
          <CommandInput placeholder="Search country..." />
          <CommandList>
            <CommandEmpty>No country found.</CommandEmpty>
            <CommandGroup>
              {countries.map((c) => (
                <CommandItem
                  key={c.code}
                  value={`${c.name} ${c.code}`}
                  onSelect={() => {
                    onChange(c.code === value ? "" : c.code);
                    setOpen(false);
                  }}
                  data-checked={value === c.code}
                >
                  <span className="flex items-center gap-2">
                    <span>{countryFlag(c.code)}</span>
                    {c.name}
                  </span>
                  <CheckIcon
                    className={cn("ml-auto size-4", value === c.code ? "opacity-100" : "opacity-0")}
                  />
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
